//! Tauri shell: runs the simulation on a background thread and streams frames to the UI.
//! The UI never touches simulation state. It sends commands and renders snapshots (design principle 3).

#![forbid(unsafe_code)]

mod host;

use host::{ClockStatus, Host, SnapshotUpdate, TimeCompression};
use serde::{Deserialize, Serialize};
use sim_core::{Doctrine, GeoPoint, Mission, SideId, UnitId, Viewer};
use sim_scenario::Scenario;
use specta_typescript::Typescript;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tauri::{Manager, State};
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
}

#[tauri::command]
#[specta::specta]
fn get_session(host: State<'_, SharedHost>) -> SessionInfo {
    session_info(&lock(&host))
}

/// Largest scenario file accepted from the UI (a scenario is text; 5 MB is far above any real one).
const MAX_SCENARIO_BYTES: usize = 5 * 1024 * 1024;

/// Loads a scenario from the JSON text of a file the user picked in the UI. The webview reads
/// the file, so the app needs no filesystem permission. The text gets the same checks as any
/// scenario: format and version, structure, platform resolution and command validation.
#[tauri::command]
#[specta::specta]
fn load_scenario(host: State<'_, SharedHost>, json: String) -> Result<SessionInfo, String> {
    if json.len() > MAX_SCENARIO_BYTES {
        return Err(format!(
            "scenario file is {} bytes; the limit is {MAX_SCENARIO_BYTES}",
            json.len()
        ));
    }
    let scenario = Scenario::from_json(&json).map_err(|e| e.to_string())?;
    let mut h = lock(&host);
    h.load(scenario).map_err(|e| e.to_string())?;
    Ok(session_info(&h))
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

fn session_info(h: &Host) -> SessionInfo {
    SessionInfo {
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
        viewer: h.viewer(),
    }
}

#[tauri::command]
#[specta::specta]
fn set_paused(host: State<'_, SharedHost>, paused: bool) {
    lock(&host).set_paused(paused);
}

#[tauri::command]
#[specta::specta]
fn set_time_compression(host: State<'_, SharedHost>, compression: TimeCompression) {
    lock(&host).set_compression(compression);
}

#[tauri::command]
#[specta::specta]
fn set_viewer(host: State<'_, SharedHost>, viewer: Viewer) {
    lock(&host).set_viewer(viewer);
}

/// Advances a paused simulation by `ticks` (single-step for the umpire).
#[tauri::command]
#[specta::specta]
fn step_ticks(host: State<'_, SharedHost>, ticks: u32) {
    lock(&host).step(u64::from(ticks));
}

#[tauri::command]
#[specta::specta]
fn order_route(
    host: State<'_, SharedHost>,
    unit: UnitId,
    waypoints: Vec<GeoPoint>,
) -> Result<(), String> {
    lock(&host)
        .order_route(unit, waypoints)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
fn order_speed(host: State<'_, SharedHost>, unit: UnitId, speed_mps: f64) -> Result<(), String> {
    lock(&host)
        .order_speed(unit, speed_mps)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Sets doctrine (WCS, EMCON, ROE) on one unit; unset fields inherit from its group and side.
#[tauri::command]
#[specta::specta]
fn order_doctrine(
    host: State<'_, SharedHost>,
    unit: UnitId,
    doctrine: Doctrine,
) -> Result<(), String> {
    lock(&host)
        .order_doctrine(unit, doctrine)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Fires `rounds` of `weapon` from `shooter` at the seat's contact `contact`.
#[tauri::command]
#[specta::specta]
fn order_engage(
    host: State<'_, SharedHost>,
    shooter: UnitId,
    contact: u32,
    weapon: String,
    rounds: u32,
) -> Result<(), String> {
    lock(&host)
        .order_engage(shooter, contact, weapon, rounds)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Gives a unit a mission (patrol, ferry, escort, strike), or clears it with `null`.
#[tauri::command]
#[specta::specta]
fn order_mission(
    host: State<'_, SharedHost>,
    unit: UnitId,
    mission: Option<Mission>,
) -> Result<(), String> {
    lock(&host)
        .order_mission(unit, mission)
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
            set_viewer,
            step_ticks,
            order_route,
            order_speed,
            order_doctrine,
            order_engage,
            order_mission,
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
                let frame = {
                    let mut h = lock(&host);
                    h.advance(elapsed, MAX_MODE_BUDGET);
                    FrameEvent {
                        clock: h.clock(),
                        update: h.update(),
                    }
                };
                if frame.emit(&app).is_err() {
                    // The window has gone; stop the runner.
                    break;
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
}
