//! Owns the running simulation, paces it against wall-clock time, and serves one picture per
//! window (seat).
//!
//! Real-time pacing is the *only* place wall-clock time is used, and it decides only how many
//! fixed ticks to run. It never changes what a tick does (ADR-0003 §1).
//!
//! Seats (`docs/models/multi-seat.md`):
//! - The main window is the **control station**. It may sit at any seat and always controls
//!   time.
//! - Seat windows, opened by the control station or a white-cell seat, are locked to one seat.
//!   They control time only if that seat is a white-cell seat.
//! - Every window has its own picture, diff state and refusals.

use serde::{Deserialize, Serialize};
use sim_core::{
    Adjudication, AdjudicationRecord, BearingView, Command, CommandPayload, ContactView, Doctrine,
    DoctrineScope, Environment, EventStatus, Facilitation, GeoPoint, InjectStatus, Issuer, Message,
    Minefield, Mission, Seat, SeatRole, SideId, SimEventKind, Simulation, Snapshot, Tick, UnitId,
    UnitView, Viewer, WeaponView,
};
use sim_scenario::Scenario;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// The label of the control-station window.
pub const MAIN_WINDOW: &str = "main";

/// Time compression settings offered in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TimeCompression {
    X1,
    X2,
    X5,
    X15,
    X30,
    X60,
    /// As fast as the machine allows.
    Max,
}

impl TimeCompression {
    /// Game seconds per wall second, or `None` for unbounded.
    #[must_use]
    pub fn factor(self) -> Option<f64> {
        match self {
            Self::X1 => Some(1.0),
            Self::X2 => Some(2.0),
            Self::X5 => Some(5.0),
            Self::X15 => Some(15.0),
            Self::X30 => Some(30.0),
            Self::X60 => Some(60.0),
            Self::Max => None,
        }
    }
}

/// One frame's change to the viewer's picture. `full` = replace everything; otherwise `units`
/// are added or replaced by id and `removed` are dropped. Contacts are always sent whole.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SnapshotUpdate {
    pub full: bool,
    pub tick: Tick,
    pub game_time_s: f64,
    pub viewer: Viewer,
    pub units: Vec<UnitView>,
    pub removed: Vec<UnitId>,
    pub contacts: Vec<ContactView>,
    /// Sea state and weather (small; sent every frame).
    pub environment: Environment,
    /// The viewer's recent ESM bearing lines (sent whole, like contacts).
    pub bearings: Vec<BearingView>,
    /// The viewer's weapons in flight (sent whole).
    pub weapons: Vec<WeaponView>,
    /// Why orders from this seat were refused since the last frame.
    pub refusals: Vec<String>,
    /// The viewer's minefields (own side; every field for the white cell).
    pub minefields: Vec<Minefield>,
    /// White cell only: the most recent overrides (sent whole).
    pub adjudications: Vec<AdjudicationRecord>,
    /// The seat's message log, latest 100 (sent whole).
    pub messages: Vec<Message>,
    /// White cell only: the inject library with send counts.
    pub injects: Vec<InjectStatus>,
    /// White cell only: every scenario event's state.
    pub scenario_events: Vec<EventStatus>,
}

/// Clock status sent with every frame.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ClockStatus {
    pub tick: Tick,
    pub game_time_s: f64,
    pub paused: bool,
    pub compression: TimeCompression,
    /// Game seconds per wall second actually achieved over the last frame.
    pub achieved_ratio: f64,
    pub scenario_duration_s: f64,
}

/// Longest wall-clock gap credited in one frame. This stops a stall (debugger, sleep) from
/// turning into a burst of thousands of ticks.
const MAX_FRAME_CREDIT: Duration = Duration::from_millis(250);

/// One window's seat and picture state.
#[derive(Debug)]
struct Station {
    seat: u32,
    /// Locked to its seat (a seat window); the control station is not.
    locked: bool,
    /// The picture last streamed, for diffing; `None` forces a full update.
    last_sent: Option<Snapshot>,
    /// Orders from this window not yet applied or refused (sequence numbers).
    pending: BTreeSet<u64>,
    /// Events already scanned for refusals.
    events_seen: usize,
}

impl Station {
    fn new(seat: u32, locked: bool, events_seen: usize) -> Self {
        Self {
            seat,
            locked,
            last_sent: None,
            pending: BTreeSet::new(),
            events_seen,
        }
    }
}

pub struct Host {
    pub scenario: Scenario,
    platforms: sim_db::PlatformDb,
    sim: Simulation,
    paused: bool,
    compression: TimeCompression,
    /// Fractional ticks owed from previous frames.
    carry: f64,
    achieved_ratio: f64,
    seats: BTreeMap<u32, Seat>,
    /// By window label.
    stations: BTreeMap<String, Station>,
    /// Numbers seat windows uniquely.
    windows_opened: u32,
}

fn viewer_of(seat: &Seat) -> Viewer {
    match seat.role {
        SeatRole::WhiteCell => Viewer::WhiteCell,
        SeatRole::Side { side } => Viewer::Side { side },
    }
}

/// Builds a scenario's session: the simulation (with the seat table set, defaulting to one
/// white-cell seat and one per side) and the main window's first seat (the first side's).
fn start(
    scenario: &Scenario,
    platforms: &sim_db::PlatformDb,
) -> Result<(Simulation, BTreeMap<u32, Seat>, u32), sim_scenario::ScenarioError> {
    let mut sim = scenario.build(None, platforms)?;
    let seats = scenario.seats_or_default();
    if scenario.seats.is_empty() {
        sim.submit(Command {
            tick: Tick(0),
            issuer: Issuer::Scenario,
            side: None,
            payload: CommandPayload::SetSeats {
                seats: seats.clone(),
            },
        })?;
    }
    let first = seats
        .iter()
        .find(|s| matches!(s.role, SeatRole::Side { .. }))
        .or_else(|| seats.first())
        .map_or(0, |s| s.id);
    Ok((sim, seats.into_iter().map(|s| (s.id, s)).collect(), first))
}

impl Host {
    pub fn new(
        scenario: Scenario,
        platforms: sim_db::PlatformDb,
    ) -> Result<Self, sim_scenario::ScenarioError> {
        let (sim, seats, first) = start(&scenario, &platforms)?;
        Ok(Self {
            scenario,
            platforms,
            sim,
            paused: true,
            compression: TimeCompression::X1,
            carry: 0.0,
            achieved_ratio: 0.0,
            seats,
            stations: BTreeMap::from([(MAIN_WINDOW.to_owned(), Station::new(first, false, 0))]),
            windows_opened: 0,
        })
    }

    /// Replaces the running scenario. The new one is fully built first, so a scenario that fails
    /// to load leaves the current session untouched. The new session starts paused, at 1×, with
    /// the main window at the first side's seat. Returns the seat windows to close (their seats
    /// belonged to the old scenario).
    pub fn load(&mut self, scenario: Scenario) -> Result<Vec<String>, sim_scenario::ScenarioError> {
        let (sim, seats, first) = start(&scenario, &self.platforms)?;
        self.scenario = scenario;
        self.sim = sim;
        self.seats = seats;
        let closing: Vec<String> = self
            .stations
            .keys()
            .filter(|l| *l != MAIN_WINDOW)
            .cloned()
            .collect();
        self.stations = BTreeMap::from([(MAIN_WINDOW.to_owned(), Station::new(first, false, 0))]);
        self.paused = true;
        self.compression = TimeCompression::X1;
        self.carry = 0.0;
        self.achieved_ratio = 0.0;
        Ok(closing)
    }

    /// The platform database (validated when it was opened).
    #[must_use]
    pub fn platforms(&self) -> &sim_db::PlatformDb {
        &self.platforms
    }

    fn station(&self, label: &str) -> Result<&Station, String> {
        self.stations
            .get(label)
            .ok_or_else(|| format!("window {label:?} has no seat"))
    }

    /// The seat a window sits at.
    pub fn seat_of(&self, label: &str) -> Result<&Seat, String> {
        let id = self.station(label)?.seat;
        self.seats
            .get(&id)
            .ok_or_else(|| format!("seat {id} no longer exists"))
    }

    /// Whose picture a window shows.
    #[must_use]
    pub fn viewer(&self, label: &str) -> Viewer {
        self.seat_of(label).map_or(Viewer::WhiteCell, viewer_of)
    }

    /// Is the window locked to its seat?
    #[must_use]
    pub fn locked(&self, label: &str) -> bool {
        self.station(label).is_ok_and(|s| s.locked)
    }

    /// May the window run the clock and manage seats? The control station and white-cell seats
    /// may; a side's seat window may not.
    #[must_use]
    pub fn controls_time(&self, label: &str) -> bool {
        label == MAIN_WINDOW
            || self
                .seat_of(label)
                .is_ok_and(|s| s.role == SeatRole::WhiteCell)
    }

    fn require_control(&self, label: &str) -> Result<(), String> {
        if self.controls_time(label) {
            Ok(())
        } else {
            Err("only the white cell controls time and seats".into())
        }
    }

    /// Every seat, by id.
    #[must_use]
    pub fn seats(&self) -> Vec<Seat> {
        self.seats.values().cloned().collect()
    }

    pub fn set_paused(&mut self, label: &str, paused: bool) -> Result<(), String> {
        self.require_control(label)?;
        self.paused = paused;
        self.carry = 0.0;
        Ok(())
    }

    pub fn set_compression(&mut self, label: &str, c: TimeCompression) -> Result<(), String> {
        self.require_control(label)?;
        self.compression = c;
        self.carry = 0.0;
        Ok(())
    }

    /// Advances by `ticks` (single-step for the umpire).
    pub fn step(&mut self, label: &str, ticks: u64) -> Result<(), String> {
        self.require_control(label)?;
        self.sim.run_ticks(ticks);
        Ok(())
    }

    /// Moves an unlocked window (the control station) to another seat.
    pub fn set_seat(&mut self, label: &str, seat: u32) -> Result<(), String> {
        if !self.seats.contains_key(&seat) {
            return Err(format!("no seat {seat}"));
        }
        let events_seen = self.sim.events().len();
        let st = self
            .stations
            .get_mut(label)
            .ok_or_else(|| format!("window {label:?} has no seat"))?;
        if st.locked {
            return Err("this window is locked to its seat".into());
        }
        *st = Station::new(seat, false, events_seen);
        Ok(())
    }

    /// A label for a new seat window.
    pub fn next_window_label(&mut self, seat: u32) -> String {
        self.windows_opened += 1;
        format!("seat-{seat}-{}", self.windows_opened)
    }

    /// Binds a new seat window, opened from window `from`, to `seat`.
    pub fn bind(&mut self, from: &str, label: &str, seat: u32) -> Result<(), String> {
        self.require_control(from)?;
        if !self.seats.contains_key(&seat) {
            return Err(format!("no seat {seat}"));
        }
        let events_seen = self.sim.events().len();
        self.stations
            .insert(label.to_owned(), Station::new(seat, true, events_seen));
        Ok(())
    }

    /// Forgets a closed window.
    pub fn unbind(&mut self, label: &str) {
        if label != MAIN_WINDOW {
            self.stations.remove(label);
        }
    }

    /// The umpire id the host uses for seat changes: the caller's seat if it is a white-cell
    /// seat, else the first white-cell seat.
    fn umpire_for(&self, label: &str) -> String {
        self.seat_of(label)
            .ok()
            .filter(|s| s.role == SeatRole::WhiteCell)
            .or_else(|| self.seats.values().find(|s| s.role == SeatRole::WhiteCell))
            .map_or_else(String::new, |s| s.name.clone())
    }

    /// Sends the seat table to the simulation as a white-cell command (logged and replayed).
    fn publish_seats(&mut self, umpire: String) -> Result<(), String> {
        self.sim
            .submit(Command {
                tick: self.sim.tick(),
                issuer: Issuer::WhiteCell { umpire },
                side: None,
                payload: CommandPayload::SetSeats {
                    seats: self.seats(),
                },
            })
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Adds a seat (`side` `None` = a white-cell seat).
    pub fn add_seat(
        &mut self,
        label: &str,
        name: &str,
        side: Option<SideId>,
    ) -> Result<Seat, String> {
        self.require_control(label)?;
        let name = name.trim();
        if name.is_empty() {
            return Err("give the seat a name".into());
        }
        if self.seats.values().any(|s| s.name == name) {
            return Err(format!("there is already a seat called {name:?}"));
        }
        if let Some(side) = side {
            if !self.scenario.sides.iter().any(|s| s.id == side) {
                return Err(format!("no side {}", side.0));
            }
        }
        let umpire = self.umpire_for(label);
        let id = self.seats.keys().next_back().map_or(0, |k| k + 1);
        let seat = Seat {
            id,
            name: name.to_owned(),
            role: side.map_or(SeatRole::WhiteCell, |side| SeatRole::Side { side }),
        };
        self.seats.insert(id, seat.clone());
        if let Err(e) = self.publish_seats(umpire) {
            self.seats.remove(&id);
            return Err(e);
        }
        Ok(seat)
    }

    /// Removes a seat. The main window's seat and the last white-cell seat cannot be removed.
    /// Returns the seat windows to close.
    pub fn remove_seat(&mut self, label: &str, id: u32) -> Result<Vec<String>, String> {
        self.require_control(label)?;
        let seat = self
            .seats
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("no seat {id}"))?;
        if self.stations.get(MAIN_WINDOW).is_some_and(|s| s.seat == id) {
            return Err("the main window sits at this seat; move it first".into());
        }
        if seat.role == SeatRole::WhiteCell
            && self
                .seats
                .values()
                .filter(|s| s.role == SeatRole::WhiteCell)
                .count()
                == 1
        {
            return Err("the last white-cell seat cannot be removed".into());
        }
        let umpire = self.umpire_for(label);
        self.seats.remove(&id);
        if let Err(e) = self.publish_seats(umpire) {
            self.seats.insert(id, seat);
            return Err(e);
        }
        let closing: Vec<String> = self
            .stations
            .iter()
            .filter(|(_, s)| s.seat == id)
            .map(|(l, _)| l.clone())
            .collect();
        for l in &closing {
            self.stations.remove(l);
        }
        Ok(closing)
    }

    /// Runs the ticks owed for `wall_elapsed` of real time. In `Max` mode it runs until
    /// `max_budget` of wall time has been spent. Returns the number of ticks run.
    pub fn advance(&mut self, wall_elapsed: Duration, max_budget: Duration) -> u64 {
        if self.paused {
            self.achieved_ratio = 0.0;
            return 0;
        }
        let credited = wall_elapsed.min(MAX_FRAME_CREDIT);
        let ran = if let Some(factor) = self.compression.factor() {
            let owed = credited.as_secs_f64() * factor + self.carry;
            let whole = owed.floor();
            self.carry = owed - whole;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // `whole` is non-negative and bounded by 60 × 0.25 s
            let n = whole as u64;
            self.sim.run_ticks(n);
            n
        } else {
            // Wall-clock pacing lives outside the deterministic core (ADR-0003 §1).
            #[allow(clippy::disallowed_methods)]
            let start = Instant::now();
            let mut n = 0;
            while start.elapsed() < max_budget {
                self.sim.run_ticks(32);
                n += 32;
            }
            n
        };
        let secs = wall_elapsed.as_secs_f64();
        #[allow(clippy::cast_precision_loss)]
        if secs > 0.0 {
            self.achieved_ratio = ran as f64 / secs;
        }
        ran
    }

    pub fn clock(&self) -> ClockStatus {
        let tick = self.sim.tick();
        #[allow(clippy::cast_precision_loss)]
        let duration = self.scenario.duration_s as f64;
        ClockStatus {
            tick,
            game_time_s: tick.seconds(),
            paused: self.paused,
            compression: self.compression,
            achieved_ratio: self.achieved_ratio,
            scenario_duration_s: duration,
        }
    }

    /// The next update for one window: a full picture the first time (and after any seat or
    /// scenario change), then only the units that changed and the ids of those that left the
    /// picture (§6.2).
    #[cfg(test)]
    pub fn update(&mut self, label: &str) -> Option<SnapshotUpdate> {
        let viewer = self.viewer(label);
        let st = self.stations.get_mut(label)?;
        Some(update_station(&mut self.sim, viewer, st))
    }

    /// The next update for every window, by label.
    pub fn updates(&mut self) -> Vec<(String, SnapshotUpdate)> {
        let viewers: Vec<(String, Viewer)> = self
            .stations
            .keys()
            .map(|l| (l.clone(), self.viewer(l)))
            .collect();
        viewers
            .into_iter()
            .filter_map(|(label, viewer)| {
                let st = self.stations.get_mut(&label)?;
                let u = update_station(&mut self.sim, viewer, st);
                Some((label, u))
            })
            .collect()
    }

    /// Issues an order from a window's seat: a white-cell seat as the umpire (its name), a side
    /// seat as that seat's player. The core checks the seat and the side.
    pub fn order(&mut self, label: &str, payload: CommandPayload) -> Result<u64, String> {
        let seat = self.seat_of(label)?.clone();
        let (issuer, side) = match seat.role {
            SeatRole::WhiteCell => (Issuer::WhiteCell { umpire: seat.name }, None),
            SeatRole::Side { side } => (Issuer::Player { seat: seat.id }, Some(side)),
        };
        let seq = self
            .sim
            .submit(Command {
                tick: self.sim.tick(),
                issuer,
                side,
                payload,
            })
            .map_err(|e| e.to_string())?;
        if let Some(st) = self.stations.get_mut(label) {
            st.pending.insert(seq);
        }
        Ok(seq)
    }

    /// Fires `rounds` of `weapon` from `shooter` at this seat's contact `contact`. The engine
    /// checks the order when it applies it; a refusal comes back in the next update.
    pub fn order_engage(
        &mut self,
        label: &str,
        shooter: UnitId,
        contact: u32,
        weapon: String,
        rounds: u32,
    ) -> Result<u64, String> {
        self.order(
            label,
            CommandPayload::Engage {
                shooter,
                contact,
                weapon,
                rounds,
            },
        )
    }

    /// Gives `unit` a mission, or clears it with `None`.
    pub fn order_mission(
        &mut self,
        label: &str,
        unit: UnitId,
        mission: Option<Mission>,
    ) -> Result<u64, String> {
        self.order(label, CommandPayload::AssignMission { unit, mission })
    }

    /// A white-cell override. Only a white-cell seat may adjudicate (the core refuses others).
    pub fn order_adjudicate(
        &mut self,
        label: &str,
        reason: String,
        action: Adjudication,
    ) -> Result<u64, String> {
        self.order(label, CommandPayload::Adjudicate { reason, action })
    }

    /// A white-cell facilitation action: send an inject or a message, fire an event, or turn
    /// one on or off. Only a white-cell seat may (the core refuses others).
    pub fn order_facilitate(&mut self, label: &str, action: Facilitation) -> Result<u64, String> {
        self.order(label, CommandPayload::Facilitate { action })
    }

    pub fn order_route(
        &mut self,
        label: &str,
        unit: UnitId,
        waypoints: Vec<GeoPoint>,
    ) -> Result<u64, String> {
        self.order(label, CommandPayload::SetRoute { unit, waypoints })
    }

    pub fn order_speed(
        &mut self,
        label: &str,
        unit: UnitId,
        speed_mps: f64,
    ) -> Result<u64, String> {
        self.order(label, CommandPayload::SetSpeed { unit, speed_mps })
    }

    /// Replaces the doctrine set on `unit` (unset fields inherit from its group and side).
    pub fn order_doctrine(
        &mut self,
        label: &str,
        unit: UnitId,
        doctrine: Doctrine,
    ) -> Result<u64, String> {
        self.order(
            label,
            CommandPayload::SetDoctrine {
                scope: DoctrineScope::Unit { unit },
                doctrine,
            },
        )
    }
}

/// Reasons a station's orders were refused since the last call.
fn refusals(sim: &Simulation, st: &mut Station) -> Vec<String> {
    let events = sim.events();
    let mut out = Vec::new();
    for e in &events[st.events_seen.min(events.len())..] {
        match &e.kind {
            SimEventKind::CommandRejected { seq, reason } if st.pending.remove(seq) => {
                out.push(reason.clone());
            }
            SimEventKind::CommandApplied { seq } => {
                st.pending.remove(seq);
            }
            _ => {}
        }
    }
    st.events_seen = events.len();
    out
}

fn update_station(sim: &mut Simulation, viewer: Viewer, st: &mut Station) -> SnapshotUpdate {
    let snap = sim.snapshot(viewer);
    let refusals = refusals(sim, st);
    let update = match &st.last_sent {
        Some(prev) if prev.viewer == snap.viewer => {
            let old: BTreeMap<UnitId, &UnitView> = prev.units.iter().map(|u| (u.id, u)).collect();
            let new_ids: BTreeSet<UnitId> = snap.units.iter().map(|u| u.id).collect();
            SnapshotUpdate {
                full: false,
                units: snap
                    .units
                    .iter()
                    .filter(|u| old.get(&u.id).is_none_or(|o| *o != *u))
                    .cloned()
                    .collect(),
                removed: old
                    .keys()
                    .filter(|id| !new_ids.contains(id))
                    .copied()
                    .collect(),
                ..whole(&snap, refusals)
            }
        }
        _ => SnapshotUpdate {
            full: true,
            units: snap.units.clone(),
            removed: vec![],
            ..whole(&snap, refusals)
        },
    };
    st.last_sent = Some(snap);
    update
}

/// The parts of an update that are always sent whole.
fn whole(snap: &Snapshot, refusals: Vec<String>) -> SnapshotUpdate {
    SnapshotUpdate {
        full: true,
        tick: snap.tick,
        game_time_s: snap.game_time_s,
        viewer: snap.viewer,
        units: vec![],
        removed: vec![],
        contacts: snap.contacts.clone(),
        environment: snap.environment,
        bearings: snap.bearings.clone(),
        weapons: snap.weapons.clone(),
        refusals,
        minefields: snap.minefields.clone(),
        adjudications: snap.adjudications.clone(),
        messages: snap.messages.clone(),
        injects: snap.injects.clone(),
        scenario_events: snap.scenario_events.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &str = MAIN_WINDOW;

    fn host() -> Host {
        let s = Scenario::from_json(include_str!(
            "../../../../data/scenarios/phase1-strait-transit.json"
        ))
        .unwrap();
        Host::new(s, sim_db::embedded::open_platform_db().unwrap()).unwrap()
    }

    fn seat_named(h: &Host, name: &str) -> u32 {
        h.seats().iter().find(|s| s.name == name).unwrap().id
    }

    #[test]
    fn default_seats_are_the_white_cell_and_one_per_side_and_main_starts_at_the_first_side() {
        let h = host();
        let names: Vec<String> = h.seats().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["White cell", "Blue", "Neutral shipping"]);
        assert_eq!(
            h.viewer(M),
            Viewer::Side {
                side: sim_core::SideId(1)
            }
        );
        assert!(!h.locked(M) && h.controls_time(M));
    }

    #[test]
    fn a_refused_engage_order_reports_its_reason_in_the_next_update() {
        let mut h = host();
        h.step(M, 1).unwrap();
        let _ = h.update(M);
        h.order_engage(M, UnitId(1), 99, "WPN-ASM-SUB".into(), 1)
            .unwrap();
        h.step(M, 1).unwrap();
        let u = h.update(M).unwrap();
        assert_eq!(u.refusals.len(), 1, "{:?}", u.refusals);
        assert!(u.refusals[0].contains("no contact C99"), "{:?}", u.refusals);
        // Reported once only.
        h.step(M, 1).unwrap();
        assert!(h.update(M).unwrap().refusals.is_empty());
        // The frigate's magazines came from the platform database.
        let snap = h.sim.snapshot(Viewer::Side {
            side: sim_core::SideId(1),
        });
        assert!(snap.units[0]
            .stores
            .iter()
            .any(|s| s.weapon == "WPN-ASM-SUB" && s.count == 8));
    }

    #[test]
    fn a_player_sets_doctrine_on_its_own_unit() {
        let mut h = host();
        h.step(M, 1).unwrap();
        let silent = Doctrine {
            emcon: Some(sim_core::Emcon::Silent),
            ..Doctrine::default()
        };
        h.order_doctrine(M, UnitId(1), silent).unwrap();
        h.step(M, 1).unwrap();
        let snap = h.sim.snapshot(Viewer::Side {
            side: sim_core::SideId(1),
        });
        assert_eq!(snap.units[0].doctrine.emcon, sim_core::Emcon::Silent);
        // The demo's Blue side doctrine (WCS safe) is inherited.
        assert_eq!(snap.units[0].doctrine.wcs, sim_core::Wcs::Safe);
    }

    #[test]
    fn platform_database_is_browsable_with_provenance() {
        let h = host();
        let db = h.platforms();
        let all = db.platforms().unwrap();
        assert!(all.len() >= 50);
        let (citation, _confidence, classification) =
            db.platform_provenance("GEN-FFG").unwrap().unwrap();
        assert!(!citation.is_empty());
        assert_eq!(classification, "UNCLASSIFIED");
        assert!(db.platform_provenance("NO-SUCH").unwrap().is_none());
        assert!(db
            .issues()
            .unwrap()
            .iter()
            .all(|i| i.severity == sim_db::Severity::Warning));
    }

    #[test]
    fn starts_paused_and_does_not_advance() {
        let mut h = host();
        assert_eq!(h.advance(Duration::from_secs(1), Duration::ZERO), 0);
        assert_eq!(h.clock().tick, Tick(0));
    }

    #[test]
    fn compression_runs_whole_ticks_and_carries_fractions() {
        let mut h = host();
        h.set_paused(M, false).unwrap();
        h.set_compression(M, TimeCompression::X15).unwrap();
        // 33 ms at 15× = 0.495 ticks → 0, carry 0.495; second frame → 0.99 → 0; third → 1.485 → 1.
        let budget = Duration::ZERO;
        let f = Duration::from_millis(33);
        assert_eq!(h.advance(f, budget), 0);
        assert_eq!(h.advance(f, budget), 0);
        assert_eq!(h.advance(f, budget), 1);
    }

    #[test]
    fn long_stall_is_capped() {
        let mut h = host();
        h.set_paused(M, false).unwrap();
        h.set_compression(M, TimeCompression::X60).unwrap();
        // A 10 s stall is credited as 0.25 s → 15 ticks.
        assert_eq!(h.advance(Duration::from_secs(10), Duration::ZERO), 15);
    }

    #[test]
    fn side_seat_cannot_order_other_side_but_white_cell_can() {
        let mut h = host();
        let seq = h.order_speed(M, UnitId(2), 1.0).unwrap();
        h.step(M, 1).unwrap();
        let snap = h.sim.snapshot(Viewer::WhiteCell);
        let merchant = snap.units.iter().find(|u| u.id == UnitId(2)).unwrap();
        assert!(
            (merchant.ordered_speed_mps - 6.5).abs() < 1e-9,
            "order from wrong side must be rejected (seq {seq})"
        );
        let white = seat_named(&h, "White cell");
        h.set_seat(M, white).unwrap();
        h.order_speed(M, UnitId(2), 1.0).unwrap();
        h.step(M, 1).unwrap();
        let snap = h.sim.snapshot(Viewer::WhiteCell);
        let merchant = snap.units.iter().find(|u| u.id == UnitId(2)).unwrap();
        assert!((merchant.ordered_speed_mps - 1.0).abs() < 1e-9);
    }

    #[test]
    fn loading_a_scenario_replaces_the_session_closes_seat_windows_and_a_bad_one_changes_nothing() {
        let mut h = host();
        h.set_paused(M, false).unwrap();
        h.step(M, 100).unwrap();
        h.bind(M, "seat-1-1", 1).unwrap();
        let inline = r#"{"format":"nws-scenario","format_version":2,"name":"Loaded","seed":3,"duration_s":60,
            "sides":[{"id":7,"name":"Red"}],
            "units":[{"id":1,"name":"A","side":7,"platform":"GEN-OPV","position":{"lat_deg":-39.3,"lon_deg":146.0}}]}"#;
        let closing = h.load(Scenario::from_json(inline).unwrap()).unwrap();
        assert_eq!(closing, vec!["seat-1-1".to_owned()]);
        assert_eq!(h.scenario.name, "Loaded");
        assert_eq!(h.clock().tick, Tick(0));
        assert!(h.clock().paused);
        assert_eq!(
            h.viewer(M),
            Viewer::Side {
                side: sim_core::SideId(7)
            }
        );

        let bad = inline.replace("GEN-OPV", "GEN-NOPE");
        assert!(h.load(Scenario::from_json(&bad).unwrap()).is_err());
        assert_eq!(
            h.scenario.name, "Loaded",
            "failed load must not change the session"
        );
    }

    #[test]
    fn updates_are_full_first_then_only_changes() {
        let mut h = host();
        let white = seat_named(&h, "White cell");
        h.set_seat(M, white).unwrap();
        h.step(M, 1).unwrap(); // units spawn on the first tick
        let first = h.update(M).unwrap();
        assert!(first.full);
        assert_eq!(first.units.len(), 2);
        // Paused and nothing ordered: nothing changed, so nothing is resent.
        let idle = h.update(M).unwrap();
        assert!(!idle.full);
        assert!(idle.units.is_empty() && idle.removed.is_empty());
        // One tick of movement: both ships moved, so both are resent (and only once).
        h.step(M, 1).unwrap();
        let moved = h.update(M).unwrap();
        assert!(!moved.full);
        assert_eq!(moved.units.len(), 2);
        // A seat change forces a full picture for the new viewer.
        h.set_seat(M, seat_named(&h, "Blue")).unwrap();
        let seat = h.update(M).unwrap();
        assert!(seat.full);
        assert_eq!(seat.units.len(), 1);
    }

    #[test]
    fn seat_windows_get_their_own_picture_and_cannot_run_the_clock_or_change_seat() {
        let mut h = host();
        let blue = seat_named(&h, "Blue");
        let white = seat_named(&h, "White cell");
        h.set_seat(M, white).unwrap();
        h.bind(M, "seat-b", blue).unwrap();
        h.step(M, 1).unwrap();
        let ups: BTreeMap<String, SnapshotUpdate> = h.updates().into_iter().collect();
        assert_eq!(ups["main"].units.len(), 2, "white cell sees every unit");
        assert_eq!(ups["seat-b"].units.len(), 1, "Blue sees its own");
        assert_eq!(
            ups["seat-b"].viewer,
            Viewer::Side {
                side: sim_core::SideId(1)
            }
        );
        // Locked: no seat change, no clock, no seat management.
        assert!(h.set_seat("seat-b", white).is_err());
        assert!(h.set_paused("seat-b", false).is_err());
        assert!(h.set_compression("seat-b", TimeCompression::X60).is_err());
        assert!(h.step("seat-b", 1).is_err());
        assert!(h.add_seat("seat-b", "Spy", None).is_err());
        // Its orders go out as its player seat; another side's unit is refused by the core.
        h.order_speed("seat-b", UnitId(2), 1.0).unwrap();
        h.step(M, 1).unwrap();
        let refused = h.update("seat-b").unwrap().refusals;
        assert_eq!(refused.len(), 1, "{refused:?}");
        // The refusal is the Blue window's alone.
        assert!(h.update(M).unwrap().refusals.is_empty());
        // A white-cell seat window does control the clock.
        h.bind(M, "seat-w", white).unwrap();
        h.set_paused("seat-w", false).unwrap();
        assert!(!h.clock().paused);
        h.unbind("seat-b");
        assert!(h.update("seat-b").is_none());
    }

    #[test]
    fn the_white_cell_adds_and_removes_seats_and_the_core_enforces_them() {
        let mut h = host();
        let added = h.add_seat(M, "Blue XO", Some(sim_core::SideId(1))).unwrap();
        assert_eq!(added.id, 3);
        assert!(h.add_seat(M, "Blue XO", None).is_err(), "names are unique");
        assert!(h.add_seat(M, "Ghost", Some(sim_core::SideId(9))).is_err());
        let umpire2 = h.add_seat(M, "Umpire 2", None).unwrap();
        h.bind(M, "seat-xo", added.id).unwrap();
        h.step(M, 1).unwrap();
        // The new seat's orders are accepted by the core for its side.
        h.order_speed("seat-xo", UnitId(1), 2.0).unwrap();
        h.step(M, 1).unwrap();
        assert!(h.update("seat-xo").unwrap().refusals.is_empty());
        // An umpire seat window's overrides carry its name.
        h.bind(M, "seat-u2", umpire2.id).unwrap();
        h.order_adjudicate(
            "seat-u2",
            "test".into(),
            sim_core::Adjudication::SetStores {
                unit: UnitId(1),
                weapon: "WPN-ASM-SUB".into(),
                count: 4,
            },
        )
        .unwrap();
        h.step(M, 1).unwrap();
        let white = h.sim.snapshot(Viewer::WhiteCell);
        assert_eq!(white.adjudications.last().unwrap().umpire, "Umpire 2");
        // Removing a seat closes its windows; the main window's seat and the last white-cell
        // seat stay.
        assert_eq!(
            h.remove_seat(M, added.id).unwrap(),
            vec!["seat-xo".to_owned()]
        );
        assert!(h.remove_seat(M, seat_named(&h, "Blue")).is_err());
        h.remove_seat(M, umpire2.id).unwrap();
        assert!(h.remove_seat(M, seat_named(&h, "White cell")).is_err());
        h.step(M, 1).unwrap();
        assert_eq!(h.sim.snapshot(Viewer::WhiteCell).seats.len(), 3);
    }
}
