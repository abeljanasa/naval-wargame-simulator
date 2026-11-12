//! Tauri shell: runs the simulation on a background thread and streams frames to the UI.
//! The UI never touches simulation state. It sends commands and renders snapshots (design principle 3).

#![forbid(unsafe_code)]

mod host;

use host::{AarView, ClockStatus, Host, SnapshotUpdate, TimeCompression, MAIN_WINDOW};
use serde::{Deserialize, Serialize};
use sim_core::{
    Adjudication, CommandPayload, Doctrine, Facilitation, GeoPoint, Mission, Seat, SideId, UnitId,
    Viewer,
};
use sim_scenario::{ReferencePoint, Scenario, ScenarioIssue, Severity};
use specta_typescript::Typescript;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tauri::{EventTarget, Manager, State, WebviewWindow};
use tauri_specta::{collect_commands, collect_events, Event};

/// Phase 1 ships one built-in scenario, embedded so the app works with no files and no network.
const DEFAULT_SCENARIO: &str =
    include_str!("../../../../data/scenarios/phase1-strait-transit.json");

/// Frame interval: 30 Hz, the snapshot-rate ceiling in the spec.
const FRAME: Duration = Duration::from_millis(33);
/// Wall time per frame spent simulating in `Max` compression.
const MAX_MODE_BUDGET: Duration = Duration::from_millis(25);

type SharedHost = Arc<Mutex<Host>>;

fn lock(h: &SharedHost) -> MutexGuard<'_, Host> {
    // A panic while holding the lock would leave the sim mid-tick. The simulation step itself never
    // panics on valid input, so we recover the guard rather than crash the UI.
    h.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Streamed to the UI at ≤ 30 Hz.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct FrameEvent {
    pub clock: ClockStatus,
    pub update: SnapshotUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SideInfo {
    pub id: SideId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SessionInfo {
    pub scenario_name: String,
    pub description: String,
    pub sides: Vec<SideInfo>,
    pub viewer: Viewer,
    /// This window's seat.
    pub seat: Seat,
    /// Every seat (for the seat picker and seat management).
    pub seats: Vec<Seat>,
    /// This window is locked to its seat (a seat window).
    pub locked: bool,
    /// This window may run the clock and manage seats.
    pub controls_time: bool,
    /// The scenario's reference points this window's seat may see.
    pub reference_points: Vec<ReferencePoint>,
    /// Planning-layer facilities this window's seat may see (its side's; all for the white cell).
    pub facilities: Vec<sim_core::Facility>,
    /// Planning-layer zones this window's seat may see.
    pub zones: Vec<sim_core::Zone>,
    /// Surface action groups this window's seat may see.
    pub action_groups: Vec<sim_scenario::ActionGroup>,
}

#[tauri::command]
#[specta::specta]
fn get_session(window: WebviewWindow, host: State<'_, SharedHost>) -> Result<SessionInfo, String> {
    session_info(&lock(&host), window.label())
}

/// Closes seat windows whose seats are gone.
fn close_windows(window: &WebviewWindow, labels: &[String]) {
    for l in labels {
        if let Some(w) = window.app_handle().get_webview_window(l) {
            // A window that fails to close is unbound already; it just shows nothing new.
            let _ = w.close();
        }
    }
}

/// Largest scenario file accepted from the UI (a scenario is text; 5 MB is far above any real one).
const MAX_SCENARIO_BYTES: usize = 5 * 1024 * 1024;

/// Loads a scenario from the JSON text of a file the user picked in the UI. The webview reads
/// the file, so the app needs no filesystem permission. The text gets the same checks as any
/// scenario: format and version, structure, platform resolution and command validation.
#[tauri::command]
#[specta::specta]
fn load_scenario(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    json: String,
) -> Result<SessionInfo, String> {
    if json.len() > MAX_SCENARIO_BYTES {
        return Err(format!(
            "scenario file is {} bytes; the limit is {MAX_SCENARIO_BYTES}",
            json.len()
        ));
    }
    let scenario = Scenario::from_json(&json).map_err(|e| e.to_string())?;
    let mut h = lock(&host);
    if !h.controls_time(window.label()) {
        return Err("only the white cell loads scenarios".into());
    }
    let closing = h.load(scenario).map_err(|e| e.to_string())?;
    close_windows(&window, &closing);
    session_info(&h, MAIN_WINDOW)
}

/// One row of the platform database browser.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DbPlatformRow {
    pub id: String,
    pub name: String,
    pub category: String,
    pub domain: String,
    pub max_speed_mps: f64,
}

/// A fitted sensor as shown in the browser.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DbSensorRow {
    pub sensor_id: String,
    pub kind: String,
    pub mount_height_m: f64,
    pub max_range_m: f64,
}

/// One platform record with its provenance.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DbPlatformDetail {
    pub row: DbPlatformRow,
    pub cruise_speed_mps: f64,
    pub accel_mps2: f64,
    pub length_m: Option<f64>,
    pub rcs_m2: Option<f64>,
    pub sensors: Vec<DbSensorRow>,
    pub source_citation: String,
    pub confidence: String,
    pub classification: String,
}

fn row(p: &sim_db::PlatformSummary) -> DbPlatformRow {
    DbPlatformRow {
        id: p.id.clone(),
        name: p.name.clone(),
        category: p.category.clone(),
        domain: p.domain.clone(),
        max_speed_mps: p.max_speed_mps,
    }
}

/// Platforms whose id, name or category contains `query` (case-insensitive), sorted by id.
#[tauri::command]
#[specta::specta]
fn db_search(host: State<'_, SharedHost>, query: String) -> Result<Vec<DbPlatformRow>, String> {
    let q = query.trim().to_lowercase();
    let h = lock(&host);
    let all = h.platforms().platforms().map_err(|e| e.to_string())?;
    Ok(all
        .iter()
        .filter(|p| {
            q.is_empty()
                || p.id.to_lowercase().contains(&q)
                || p.name.to_lowercase().contains(&q)
                || p.category.to_lowercase().contains(&q)
        })
        .map(row)
        .collect())
}

/// One platform record, with its sensors, RCS and provenance.
#[tauri::command]
#[specta::specta]
fn db_platform(host: State<'_, SharedHost>, id: String) -> Result<DbPlatformDetail, String> {
    let h = lock(&host);
    let db = h.platforms();
    let p = db
        .platform(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no platform {id}"))?;
    let (source_citation, confidence, classification) = db
        .platform_provenance(&id)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    Ok(DbPlatformDetail {
        row: row(&p),
        cruise_speed_mps: p.cruise_speed_mps,
        accel_mps2: p.accel_mps2,
        length_m: p.length_m,
        rcs_m2: db.rcs_m2(&id).map_err(|e| e.to_string())?,
        sensors: db
            .sensors_of(&id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|s| DbSensorRow {
                sensor_id: s.sensor_id,
                kind: s.kind,
                mount_height_m: s.mount_height_m,
                max_range_m: s.max_range_m,
            })
            .collect(),
        source_citation,
        confidence,
        classification,
    })
}

/// A validation finding on the platform database.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DbIssue {
    /// `error` or `warning`.
    pub severity: String,
    pub rule: String,
    /// `table/record`, e.g. `platforms/GEN-FFG`.
    pub location: String,
    pub message: String,
}

/// Every validation finding on the loaded database (warnings only: errors stop it loading).
#[tauri::command]
#[specta::specta]
fn db_issues(host: State<'_, SharedHost>) -> Result<Vec<DbIssue>, String> {
    let h = lock(&host);
    Ok(h.platforms()
        .issues()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|i| DbIssue {
            severity: match i.severity {
                sim_db::validate::Severity::Error => "error",
                sim_db::validate::Severity::Warning => "warning",
            }
            .to_owned(),
            rule: i.rule.to_owned(),
            location: i.location,
            message: i.message,
        })
        .collect())
}

fn session_info(h: &Host, label: &str) -> Result<SessionInfo, String> {
    Ok(SessionInfo {
        scenario_name: h.scenario.name.clone(),
        description: h.scenario.description.clone(),
        sides: h
            .scenario
            .sides
            .iter()
            .map(|s| SideInfo {
                id: s.id,
                name: s.name.clone(),
            })
            .collect(),
        viewer: h.viewer(label),
        seat: h.seat_of(label)?.clone(),
        seats: h.seats(),
        locked: h.locked(label),
        controls_time: h.controls_time(label),
        reference_points: h
            .scenario
            .reference_points
            .iter()
            .filter(|r| match (h.viewer(label), r.side) {
                (Viewer::WhiteCell, _) | (_, None) => true,
                (Viewer::Side { side }, Some(s)) => s == side,
            })
            .cloned()
            .collect(),
        facilities: h
            .scenario
            .facilities
            .iter()
            .filter(|f| sees(h.viewer(label), f.side))
            .cloned()
            .collect(),
        zones: h
            .zones
            .iter()
            .filter(|z| sees(h.viewer(label), z.side))
            .cloned()
            .collect(),
        action_groups: h
            .scenario
            .action_groups
            .iter()
            .filter(|g| sees(h.viewer(label), g.side))
            .cloned()
            .collect(),
    })
}

/// May a viewer see planning data belonging to `side`?
fn sees(viewer: Viewer, side: SideId) -> bool {
    match viewer {
        Viewer::WhiteCell => true,
        Viewer::Side { side: own } => own == side,
    }
}

/// A weapon type in the platform database (for the scenario editor's pickers).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DbWeaponRow {
    pub id: String,
    pub name: String,
    pub kind: String,
}

/// Every weapon type, sorted by id.
#[tauri::command]
#[specta::specta]
fn db_weapons(host: State<'_, SharedHost>) -> Result<Vec<DbWeaponRow>, String> {
    Ok(lock(&host)
        .platforms()
        .weapons()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(id, name, kind)| DbWeaponRow { id, name, kind })
        .collect())
}

/// The scenario editor's check of a draft.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct EditorCheck {
    /// Errors first, then warnings. Errors block saving and playing.
    pub issues: Vec<ScenarioIssue>,
    /// The draft as the engine reads it (absent when it is not a scenario at all).
    pub scenario: Option<Scenario>,
}

/// Checks a draft scenario for the editor: every error and warning at once
/// (`docs/models/scenario-editor.md`).
#[tauri::command]
#[specta::specta]
fn editor_check(host: State<'_, SharedHost>, json: String) -> Result<EditorCheck, String> {
    check_draft(host.inner(), &json)
}

fn check_draft(host: &SharedHost, json: &str) -> Result<EditorCheck, String> {
    if json.len() > MAX_SCENARIO_BYTES {
        return Err(format!(
            "scenario is {} bytes; the limit is {MAX_SCENARIO_BYTES}",
            json.len()
        ));
    }
    let error = |message: String| EditorCheck {
        issues: vec![ScenarioIssue {
            severity: Severity::Error,
            message,
        }],
        scenario: None,
    };
    let scenario: Scenario = match serde_json::from_str(json) {
        Ok(s) => s,
        Err(e) => return Ok(error(format!("not a valid scenario: {e}"))),
    };
    if scenario.format != sim_scenario::SCENARIO_FORMAT
        || scenario.format_version > sim_scenario::SCENARIO_FORMAT_VERSION
    {
        return Ok(error(format!(
            "format must be {:?} version {} or older",
            sim_scenario::SCENARIO_FORMAT,
            sim_scenario::SCENARIO_FORMAT_VERSION
        )));
    }
    let issues = scenario.check(lock(host).platforms());
    Ok(EditorCheck {
        issues,
        scenario: Some(scenario),
    })
}

/// The scenario now loaded, for the editor to start from.
#[tauri::command]
#[specta::specta]
fn editor_current(host: State<'_, SharedHost>) -> Scenario {
    lock(&host).scenario.clone()
}

/// Runs or pauses the clock (white-cell windows only).
#[tauri::command]
#[specta::specta]
fn set_paused(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    paused: bool,
) -> Result<(), String> {
    lock(&host).set_paused(window.label(), paused)
}

/// Sets time compression (white-cell windows only).
#[tauri::command]
#[specta::specta]
fn set_time_compression(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    compression: TimeCompression,
) -> Result<(), String> {
    lock(&host).set_compression(window.label(), compression)
}

/// Moves the main window to another seat (seat windows are locked).
#[tauri::command]
#[specta::specta]
fn set_seat(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    seat: u32,
) -> Result<SessionInfo, String> {
    let mut h = lock(&host);
    h.set_seat(window.label(), seat)?;
    session_info(&h, window.label())
}

/// Adds a seat for `side`, or a white-cell seat when `side` is null (white-cell windows only).
#[tauri::command]
#[specta::specta]
fn add_seat(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    name: String,
    side: Option<SideId>,
) -> Result<SessionInfo, String> {
    let mut h = lock(&host);
    h.add_seat(window.label(), &name, side)?;
    session_info(&h, window.label())
}

/// Removes a seat and closes its windows (white-cell windows only).
#[tauri::command]
#[specta::specta]
fn remove_seat(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    seat: u32,
) -> Result<SessionInfo, String> {
    let mut h = lock(&host);
    let closing = h.remove_seat(window.label(), seat)?;
    close_windows(&window, &closing);
    session_info(&h, window.label())
}

/// Opens a window locked to `seat` (white-cell windows only). Async: on Windows, creating a
/// window from a synchronous command deadlocks.
#[tauri::command]
#[specta::specta]
async fn open_seat_window(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    seat: u32,
) -> Result<(), String> {
    let (label, title) = {
        let mut h = lock(&host);
        if !h.controls_time(window.label()) {
            return Err("only the white cell opens seat windows".into());
        }
        let name = h
            .seats()
            .into_iter()
            .find(|s| s.id == seat)
            .map(|s| s.name)
            .ok_or_else(|| format!("no seat {seat}"))?;
        let label = h.next_window_label(seat);
        h.bind(window.label(), &label, seat)?;
        (label, format!("Naval Wargame Simulator — {name}"))
    };
    tauri::WebviewWindowBuilder::new(
        window.app_handle(),
        &label,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title(title)
    .inner_size(1280.0, 800.0)
    .build()
    .map(|_| ())
    .map_err(|e| {
        lock(&host).unbind(&label);
        e.to_string()
    })
}

/// Largest exercise record accepted from the UI.
const MAX_RECORD_BYTES: usize = 64 * 1024 * 1024;

/// Reviews the live exercise so far as a replay (control station and white-cell seats).
#[tauri::command]
#[specta::specta]
fn review_exercise(window: WebviewWindow, host: State<'_, SharedHost>) -> Result<(), String> {
    lock(&host).review(window.label())
}

/// Leaves the replay and shows the live exercise again.
#[tauri::command]
#[specta::specta]
fn close_review(window: WebviewWindow, host: State<'_, SharedHost>) -> Result<(), String> {
    lock(&host).close_review(window.label())
}

/// Moves the replay scrubber to `tick`.
#[tauri::command]
#[specta::specta]
fn seek_replay(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    tick: sim_core::Tick,
) -> Result<(), String> {
    lock(&host).seek(window.label(), tick)
}

/// The exercise record (replay under review, or the live run so far) for the UI to save.
#[tauri::command]
#[specta::specta]
fn save_record(window: WebviewWindow, host: State<'_, SharedHost>) -> Result<Vec<u8>, String> {
    lock(&host).record_bytes(window.label())
}

/// Opens an exercise record the user picked (the webview reads the file): its scenario replaces
/// the session and the record is replayed for review.
#[tauri::command]
#[specta::specta]
fn open_record(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    bytes: Vec<u8>,
) -> Result<SessionInfo, String> {
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(format!(
            "record is {} bytes; the limit is {MAX_RECORD_BYTES}",
            bytes.len()
        ));
    }
    let mut h = lock(&host);
    let closing = h.open_record(window.label(), &bytes)?;
    close_windows(&window, &closing);
    session_info(&h, window.label())
}

/// The after-action review tables.
#[tauri::command]
#[specta::specta]
fn get_aar(window: WebviewWindow, host: State<'_, SharedHost>) -> Result<AarView, String> {
    lock(&host).aar(window.label())
}

/// One AAR table as CSV text, for the UI to save.
#[tauri::command]
#[specta::specta]
fn aar_csv(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    table: sim_aar::AarTable,
) -> Result<String, String> {
    lock(&host).aar_csv(window.label(), table)
}

/// Changes one zone's doctrine (white cell only): a per-zone ROE/WCS override.
#[tauri::command]
#[specta::specta]
fn order_zone_doctrine(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    zone: u32,
    doctrine: Doctrine,
) -> Result<SessionInfo, String> {
    let mut h = lock(&host);
    h.order_zone_doctrine(window.label(), zone, doctrine)?;
    session_info(&h, window.label())
}

/// Coverage and gaps of a sense zone by its side's sensors now (the window's own side's zones;
/// any zone for the white cell).
#[tauri::command]
#[specta::specta]
fn zone_coverage(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    zone: u32,
) -> Result<Option<sim_core::ZoneCoverage>, String> {
    lock(&host).zone_coverage(window.label(), zone)
}

/// Advances a paused simulation by `ticks` (single-step for the umpire; white-cell windows only).
#[tauri::command]
#[specta::specta]
fn step_ticks(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    ticks: u32,
) -> Result<(), String> {
    lock(&host).step(window.label(), u64::from(ticks))
}

#[tauri::command]
#[specta::specta]
fn order_route(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    unit: UnitId,
    waypoints: Vec<GeoPoint>,
) -> Result<(), String> {
    lock(&host)
        .order_route(window.label(), unit, waypoints)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
fn order_speed(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    unit: UnitId,
    speed_mps: f64,
) -> Result<(), String> {
    lock(&host)
        .order_speed(window.label(), unit, speed_mps)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Sets doctrine (WCS, EMCON, ROE) on one unit; unset fields inherit from its group and side.
#[tauri::command]
#[specta::specta]
fn order_doctrine(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    unit: UnitId,
    doctrine: Doctrine,
) -> Result<(), String> {
    lock(&host)
        .order_doctrine(window.label(), unit, doctrine)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Fires `rounds` of `weapon` from `shooter` at the seat's contact `contact`.
#[tauri::command]
#[specta::specta]
fn order_engage(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    shooter: UnitId,
    contact: u32,
    weapon: String,
    rounds: u32,
) -> Result<(), String> {
    lock(&host)
        .order_engage(window.label(), shooter, contact, weapon, rounds)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Gives a unit a mission (patrol, ferry, escort, strike), or clears it with `null`.
#[tauri::command]
#[specta::specta]
fn order_mission(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    unit: UnitId,
    mission: Option<Mission>,
) -> Result<(), String> {
    lock(&host)
        .order_mission(window.label(), unit, mission)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Gives an action group its mission (run by its leader); `None` clears it.
#[tauri::command]
#[specta::specta]
fn order_group_mission(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    group: String,
    mission: Option<Mission>,
) -> Result<(), String> {
    lock(&host)
        .order_group(
            window.label(),
            CommandPayload::AssignGroupMission { group, mission },
        )
        .map(|_| ())
}

/// Takes a unit out of its action group.
#[tauri::command]
#[specta::specta]
fn order_detach(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    unit: UnitId,
) -> Result<(), String> {
    lock(&host)
        .order_group(window.label(), CommandPayload::DetachFromGroup { unit })
        .map(|_| ())
}

/// Puts a unit into an action group at a station on its leader.
#[tauri::command]
#[specta::specta]
fn order_join(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    unit: UnitId,
    group: String,
    bearing_deg: f64,
    range_m: f64,
) -> Result<(), String> {
    lock(&host)
        .order_group(
            window.label(),
            CommandPayload::JoinGroup {
                unit,
                group,
                bearing_deg,
                range_m,
            },
        )
        .map(|_| ())
}

/// A white-cell adjudication override with its reason (white-cell seat only).
#[tauri::command]
#[specta::specta]
fn order_adjudicate(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    reason: String,
    action: Adjudication,
) -> Result<(), String> {
    lock(&host)
        .order_adjudicate(window.label(), reason, action)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// A white-cell facilitation action: inject, message, fire or switch an event (white-cell seat
/// only).
#[tauri::command]
#[specta::specta]
fn order_facilitate(
    window: WebviewWindow,
    host: State<'_, SharedHost>,
    action: Facilitation,
) -> Result<(), String> {
    lock(&host)
        .order_facilitate(window.label(), action)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            get_session,
            load_scenario,
            db_search,
            db_platform,
            db_issues,
            set_paused,
            set_time_compression,
            set_seat,
            add_seat,
            remove_seat,
            open_seat_window,
            step_ticks,
            order_route,
            order_speed,
            order_doctrine,
            order_engage,
            order_mission,
            order_adjudicate,
            order_facilitate,
            order_zone_doctrine,
            zone_coverage,
            order_group_mission,
            order_detach,
            order_join,
            db_weapons,
            editor_check,
            editor_current,
            review_exercise,
            close_review,
            seek_replay,
            save_record,
            open_record,
            get_aar,
            aar_csv,
        ])
        .events(collect_events![FrameEvent])
}

fn typescript() -> Typescript {
    // u64 newtypes (UnitId, Tick) are mapped to `number` at their definitions in sim-core.
    Typescript::default().header("// Generated by tauri-specta from apps/desktop/src-tauri. Do not edit.\n// Regenerate: cargo test -p nws-desktop export_bindings\n")
}

/// Writes the TypeScript bindings for the UI.
pub fn export_bindings(path: &std::path::Path) -> Result<(), String> {
    specta_builder()
        .export(typescript(), path)
        .map_err(|e| e.to_string())
}

fn spawn_runner(app: tauri::AppHandle, host: SharedHost) {
    std::thread::Builder::new()
        .name("sim-runner".into())
        .spawn(move || {
            // Wall-clock pacing for the UI only (ADR-0003 §1).
            #[allow(clippy::disallowed_methods)]
            let mut last = std::time::Instant::now();
            loop {
                std::thread::sleep(FRAME);
                #[allow(clippy::disallowed_methods)]
                let now = std::time::Instant::now();
                let elapsed = now - last;
                last = now;
                let frames: Vec<(String, FrameEvent)> = {
                    let mut h = lock(&host);
                    h.advance(elapsed, MAX_MODE_BUDGET);
                    let clock = h.clock();
                    h.updates()
                        .into_iter()
                        .map(|(label, update)| {
                            (
                                label,
                                FrameEvent {
                                    clock: clock.clone(),
                                    update,
                                },
                            )
                        })
                        .collect()
                };
                if app.get_webview_window(MAIN_WINDOW).is_none() {
                    // The main window has gone; stop the runner.
                    break;
                }
                for (label, frame) in frames {
                    // Each window receives only its own seat's picture.
                    let _ = frame.emit_to(&app, EventTarget::webview_window(label));
                }
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| eprintln!("failed to start simulation thread: {e}"));
}

/// Desktop entry point.
// `tauri::generate_context!` expands to code that uses `HashMap`. That is app wiring, not
// simulation logic, so the determinism ban (ADR-0003) does not apply here.
#[allow(clippy::disallowed_types)]
pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    if let Err(e) = export_bindings(std::path::Path::new("../src/lib/bindings.ts")) {
        eprintln!("warning: could not export TypeScript bindings: {e}");
    }

    let scenario = match Scenario::from_json(DEFAULT_SCENARIO) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("built-in scenario is invalid: {e}");
            std::process::exit(1);
        }
    };
    // The open platform database is compiled into the app: no data files, works air-gapped.
    let platforms = match sim_db::embedded::open_platform_db() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("built-in platform database is invalid: {e}");
            std::process::exit(1);
        }
    };
    let host = match Host::new(scenario, platforms) {
        Ok(h) => Arc::new(Mutex::new(h)),
        Err(e) => {
            eprintln!("cannot start simulation: {e}");
            std::process::exit(1);
        }
    };

    let result = tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .manage(host.clone())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                lock(window.state::<SharedHost>().inner()).unbind(window.label());
            }
        })
        .setup(move |app| {
            builder.mount_events(app);
            spawn_runner(
                app.handle().clone(),
                app.state::<SharedHost>().inner().clone(),
            );
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("error while running the application: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    /// Regenerates `apps/desktop/src/lib/bindings.ts`. CI fails if the result differs from the
    /// committed file (git diff --exit-code).
    #[test]
    fn export_bindings() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts");
        super::export_bindings(&path).unwrap();
    }

    /// The scenario editor's event templates (`src/lib/event-templates.json`) are valid events
    /// for a two-side scenario, so "Add event" never starts from a broken one.
    #[test]
    fn event_templates_are_valid() {
        let templates: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../src/lib/event-templates.json")).unwrap();
        assert!(!templates.is_empty());
        let events: Vec<serde_json::Value> = templates.iter().map(|t| t["event"].clone()).collect();
        let scenario = serde_json::json!({
            "format": "nws-scenario", "format_version": 4, "name": "t", "seed": 1,
            "duration_s": 3600,
            "sides": [{"id": 1, "name": "Blue"}, {"id": 2, "name": "Red"}],
            "units": [],
            "events": events,
        });
        let s = sim_scenario::Scenario::from_json(&scenario.to_string())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.events.len(), templates.len());
        let db = sim_db::embedded::open_platform_db().unwrap();
        assert!(s.build(None, &db).is_ok());
    }

    #[test]
    fn editor_check_reports_parse_errors_and_every_issue() {
        let db = sim_db::embedded::open_platform_db().unwrap();
        let scenario = sim_scenario::Scenario::from_json(super::DEFAULT_SCENARIO).unwrap();
        let host = std::sync::Arc::new(std::sync::Mutex::new(
            super::Host::new(scenario, db).unwrap(),
        ));
        let check = |json: &str| super::check_draft(&host, json).unwrap();
        let bad = check("{ not json");
        assert_eq!(bad.issues.len(), 1);
        assert!(bad.scenario.is_none());
        let wrong = check(r#"{"format": "other", "format_version": 1}"#);
        assert!(
            wrong.issues[0].message.contains("not a valid scenario")
                || wrong.issues[0].message.contains("format")
        );
        let current = serde_json::to_string(&host.lock().unwrap().scenario).unwrap();
        let ok = check(&current);
        assert!(ok.scenario.is_some());
        assert!(ok
            .issues
            .iter()
            .all(|i| i.severity == sim_scenario::Severity::Warning));
    }
}
