//! The `Simulation` facade: the only public way to drive the world.

use crate::command::{Command, CommandPayload, LoggedCommand, UnitSpawn, PROTOCOL_VERSION};
use crate::components::{Domain, Kinematics, Position, Route, Unit};
use crate::detection::{default_rcs_m2, detection_system, ContactTable, SensorSuite, Signature};
use crate::error::SimError;
use crate::events::{EventLog, SimEvent, SimEventKind};
use crate::hash::StateHash;
use crate::ids::UnitId;
use crate::movement::movement_system;
use crate::rng::RngStreams;
use crate::save::{SaveGame, UnitRecord, SAVE_FORMAT_VERSION, SAVE_MAGIC};
use crate::snapshot::{ContactView, RouteView, Snapshot, UnitView, Viewer};
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::SingleThreadedExecutor;
use sim_geo::land::LandMask;
use sim_geo::GeoPoint;
use std::collections::BTreeMap;
use std::sync::Arc;

/// All accepted commands, keyed by (apply tick, sequence number). This is also the replay log.
#[derive(Debug, Default, Resource)]
struct CommandLog {
    next_seq: u64,
    entries: BTreeMap<(Tick, u64), Command>,
}

impl CommandLog {
    fn due(&self, tick: Tick) -> Vec<(u64, Command)> {
        self.entries
            .range((tick, 0)..=(tick, u64::MAX))
            .map(|(&(_, seq), cmd)| (seq, cmd.clone()))
            .collect()
    }

    fn in_seq_order(&self) -> Vec<LoggedCommand> {
        let mut v: Vec<LoggedCommand> = self
            .entries
            .iter()
            .map(|(&(_, seq), command)| LoggedCommand {
                seq,
                command: command.clone(),
            })
            .collect();
        v.sort_by_key(|c| c.seq);
        v
    }
}

/// The coastline the movement model navigates around. `None` = open ocean everywhere
/// (unit tests, or scenarios that opt out).
#[derive(Debug, Default, Clone, Resource)]
pub(crate) struct LandMaskRes(pub(crate) Option<Arc<LandMask>>);

/// Plans `waypoints` from `from` around land for ships and submarines. Returns the route to store
/// and, when legs were replanned, how many.
fn plan_sea_route(
    world: &World,
    domain: Domain,
    from: GeoPoint,
    waypoints: &[GeoPoint],
) -> Result<(Vec<GeoPoint>, usize), String> {
    let mask = world.resource::<LandMaskRes>().0.clone();
    match (mask, domain) {
        (Some(mask), Domain::Surface | Domain::Subsurface) if !waypoints.is_empty() => mask
            .plan_route(from, waypoints)
            .map_err(|e| format!("route refused: {e}")),
        _ => Ok((waypoints.to_vec(), 0)),
    }
}

/// Stable unit id → ECS entity.
#[derive(Debug, Default, Resource)]
struct UnitIndex(BTreeMap<UnitId, Entity>);

/// A deterministic simulation run.
pub struct Simulation {
    world: World,
    schedule: Schedule,
}

impl std::fmt::Debug for Simulation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Simulation")
            .field("tick", &self.tick())
            .finish_non_exhaustive()
    }
}

impl Simulation {
    /// An empty world at tick 0 with RNG streams derived from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut world = World::new();
        world.insert_resource(SimClock::default());
        world.insert_resource(RngStreams::new(seed));
        world.insert_resource(EventLog::default());
        world.insert_resource(CommandLog::default());
        world.insert_resource(UnitIndex::default());
        world.insert_resource(ContactTable::default());
        world.insert_resource(LandMaskRes::default());
        Self {
            world,
            schedule: build_schedule(),
        }
    }

    /// The tick that the next [`Simulation::step`] will execute.
    #[must_use]
    pub fn tick(&self) -> Tick {
        self.world.resource::<SimClock>().tick
    }

    /// Navigate ships and submarines around this coastline from now on. Part of the state hash
    /// (by id), so it must be set again after loading a save, or the run will not match.
    pub fn set_land_mask(&mut self, mask: Arc<LandMask>) {
        self.world.insert_resource(LandMaskRes(Some(mask)));
    }

    #[must_use]
    pub fn seed(&self) -> u64 {
        self.world.resource::<RngStreams>().seed()
    }

    /// Accepts a command into the log. It is applied at the start of `command.tick`.
    /// Returns its sequence number.
    pub fn submit(&mut self, command: Command) -> Result<u64, SimError> {
        let now = self.tick();
        if command.tick < now {
            return Err(SimError::CommandInPast {
                scheduled: command.tick,
                now,
            });
        }
        command
            .payload
            .validate()
            .map_err(SimError::InvalidCommand)?;
        let mut log = self.world.resource_mut::<CommandLog>();
        let seq = log.next_seq;
        log.next_seq += 1;
        log.entries.insert((command.tick, seq), command);
        Ok(seq)
    }

    /// Runs exactly one tick.
    pub fn step(&mut self) {
        self.schedule.run(&mut self.world);
    }

    /// Runs `n` ticks.
    pub fn run_ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
        }
    }

    /// Runs until `self.tick() == until` (does nothing if already there or past).
    pub fn run_until(&mut self, until: Tick) {
        while self.tick() < until {
            self.step();
        }
    }

    /// Every accepted command in sequence order: the replay log.
    #[must_use]
    pub fn command_log(&self) -> Vec<LoggedCommand> {
        self.world.resource::<CommandLog>().in_seq_order()
    }

    #[must_use]
    pub fn events(&self) -> &[SimEvent] {
        &self.world.resource::<EventLog>().events
    }

    /// Rebuilds a run from a seed and a command log, then advances to `until`.
    pub fn replay(seed: u64, log: &[LoggedCommand], until: Tick) -> Result<Self, SimError> {
        let mut sim = Self::new(seed);
        let mut ordered: Vec<&LoggedCommand> = log.iter().collect();
        ordered.sort_by_key(|c| c.seq);
        for c in ordered {
            sim.submit(c.command.clone())?;
        }
        sim.run_until(until);
        Ok(sim)
    }

    fn unit_records(&mut self) -> Vec<UnitRecord> {
        let mut q = self.world.query::<(
            &Unit,
            &Position,
            &Kinematics,
            &Route,
            &SensorSuite,
            &Signature,
        )>();
        let mut v: Vec<UnitRecord> = q
            .iter(&self.world)
            .map(|(u, p, k, r, s, sig)| UnitRecord {
                unit: u.clone(),
                position: *p,
                kinematics: *k,
                route: r.clone(),
                sensors: s.clone(),
                signature: Some(*sig),
            })
            .collect();
        v.sort_by_key(|r| r.unit.id);
        v
    }

    /// State as seen by `viewer`.
    pub fn snapshot(&mut self, viewer: Viewer) -> Snapshot {
        let tick = self.tick();
        let units = self
            .unit_records()
            .into_iter()
            .filter(|r| match viewer {
                Viewer::WhiteCell => true,
                Viewer::Side { side } => r.unit.side == side,
            })
            .map(|r| {
                #[allow(clippy::cast_possible_truncation)] // routes are far shorter than u32::MAX
                let route = Some(RouteView {
                    next: r.route.next as u32,
                    waypoints: r.route.waypoints,
                });
                UnitView {
                    id: r.unit.id,
                    name: r.unit.name,
                    side: r.unit.side,
                    domain: r.unit.domain,
                    position: r.position.point,
                    altitude_m: r.position.altitude_m,
                    heading_deg: r.kinematics.heading_deg,
                    speed_mps: r.kinematics.speed_mps,
                    ordered_speed_mps: r.kinematics.ordered_speed_mps,
                    max_speed_mps: r.kinematics.max_speed_mps,
                    platform: r.unit.platform,
                    route,
                }
            })
            .collect();
        let contacts = self
            .world
            .resource::<ContactTable>()
            .iter()
            .filter(|(side, _)| match viewer {
                Viewer::WhiteCell => true,
                Viewer::Side { side: s } => *side == s,
            })
            .map(|(side, c)| ContactView {
                side,
                number: c.number,
                position: c.position,
                first_seen_s: c.first_seen.seconds(),
                last_seen_s: c.last_seen.seconds(),
                detected_by: c.detected_by,
                sensor_id: c.sensor_id.clone(),
            })
            .collect();
        Snapshot {
            protocol_version: PROTOCOL_VERSION,
            tick,
            game_time_s: tick.seconds(),
            viewer,
            units,
            contacts,
        }
    }

    /// Canonical hash of the full simulation state (ADR-0003 §5).
    pub fn state_hash(&mut self) -> u64 {
        let mut h = StateHash::default();
        h.u64(self.tick().0);
        let rng = self.world.resource::<RngStreams>();
        h.u64(rng.seed());
        for (id, pos) in rng.positions() {
            h.u64(id as u64);
            h.u128(pos);
        }
        let log = self.world.resource::<CommandLog>();
        h.u64(log.next_seq);
        h.u64(log.entries.len() as u64);
        h.opt_str(
            self.world
                .resource::<LandMaskRes>()
                .0
                .as_ref()
                .map(|m| m.id()),
        );
        for r in self.unit_records() {
            h.u64(r.unit.id.0);
            h.u16(r.unit.side.0);
            h.str(&r.unit.name);
            h.u8(r.unit.domain.code());
            h.f64(r.position.point.lat_deg);
            h.f64(r.position.point.lon_deg);
            h.f64(r.position.altitude_m);
            h.f64(r.kinematics.heading_deg);
            h.f64(r.kinematics.speed_mps);
            h.f64(r.kinematics.ordered_speed_mps);
            h.f64(r.kinematics.max_speed_mps);
            h.f64(r.kinematics.accel_mps2);
            h.opt_f64(r.kinematics.max_turn_rate_dps);
            h.f64(r.kinematics.min_turn_radius_m);
            h.u64(r.route.next as u64);
            h.u64(r.route.waypoints.len() as u64);
            for w in &r.route.waypoints {
                h.f64(w.lat_deg);
                h.f64(w.lon_deg);
            }
            h.opt_str(r.unit.platform.as_deref());
            h.f64(r.signature.map_or(0.0, |s| s.rcs_m2));
            h.u64(r.sensors.0.len() as u64);
            for s in &r.sensors.0 {
                h.str(&s.sensor_id);
                h.u8(s.kind.code());
                h.f64(s.mount_height_m);
                h.f64(s.min_range_m);
                h.f64(s.max_range_m);
                h.opt_f64(s.ref_range_m);
                h.opt_f64(s.ref_rcs_m2);
                h.f64(s.max_target_altitude_m);
            }
        }
        let contacts = self.world.resource::<ContactTable>();
        for (side, n) in contacts.next_numbers() {
            h.u16(side.0);
            h.u64(u64::from(n));
        }
        for (side, c) in contacts.iter() {
            h.u16(side.0);
            h.u64(u64::from(c.number));
            h.u64(c.target.0);
            h.u64(c.first_seen.0);
            h.u64(c.last_seen.0);
            h.f64(c.position.lat_deg);
            h.f64(c.position.lon_deg);
            h.u64(c.detected_by.0);
            h.str(&c.sensor_id);
        }
        h.finish()
    }

    /// Captures everything needed to resume this run exactly.
    pub fn save(&mut self) -> SaveGame {
        let units = self.unit_records();
        let log = self.world.resource::<CommandLog>();
        SaveGame {
            magic: SAVE_MAGIC.to_owned(),
            format_version: SAVE_FORMAT_VERSION,
            protocol_version: PROTOCOL_VERSION,
            tick: self.tick(),
            next_seq: log.next_seq,
            commands: log.in_seq_order(),
            rng: self.world.resource::<RngStreams>().clone(),
            units,
            events: self.events().to_vec(),
            contacts: self.world.resource::<ContactTable>().records(),
            next_contact_numbers: self.world.resource::<ContactTable>().next_numbers(),
        }
    }

    /// Restores a run from a save.
    pub fn from_save(save: SaveGame) -> Result<Self, SimError> {
        let mut sim = Self::new(save.rng.seed());
        sim.world.insert_resource(SimClock { tick: save.tick });
        sim.world.insert_resource(save.rng);
        sim.world.insert_resource(EventLog {
            events: save.events,
        });
        let mut log = CommandLog {
            next_seq: save.next_seq,
            entries: BTreeMap::new(),
        };
        for c in save.commands {
            if c.seq >= save.next_seq {
                return Err(SimError::CorruptSave(format!(
                    "command seq {} >= next_seq {}",
                    c.seq, save.next_seq
                )));
            }
            log.entries.insert((c.command.tick, c.seq), c.command);
        }
        sim.world.insert_resource(log);
        sim.world.insert_resource(ContactTable::restore(
            save.contacts,
            save.next_contact_numbers,
        ));

        let mut index = BTreeMap::new();
        for r in save.units {
            let id = r.unit.id;
            let signature = r.signature.unwrap_or(Signature {
                rcs_m2: default_rcs_m2(r.unit.domain),
            });
            let e = sim
                .world
                .spawn((
                    r.unit,
                    r.position,
                    r.kinematics,
                    r.route,
                    r.sensors,
                    signature,
                ))
                .id();
            if index.insert(id, e).is_some() {
                return Err(SimError::CorruptSave(format!("duplicate unit {id}")));
            }
        }
        sim.world.insert_resource(UnitIndex(index));
        Ok(sim)
    }
}

fn build_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    // ADR-0003 §3: single-threaded, fixed order.
    schedule.set_executor(SingleThreadedExecutor::new());
    schedule.add_systems(
        (
            apply_commands_system,
            movement_system,
            detection_system,
            advance_clock_system,
        )
            .chain(),
    );
    schedule
}

fn advance_clock_system(mut clock: ResMut<SimClock>) {
    clock.tick = clock.tick.next();
}

/// Applies every command due this tick, in sequence order.
fn apply_commands_system(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    let due = world.resource::<CommandLog>().due(tick);
    for (seq, cmd) in due {
        let kind = match apply_command(world, &cmd) {
            Ok(()) => SimEventKind::CommandApplied { seq },
            Err(reason) => SimEventKind::CommandRejected { seq, reason },
        };
        world.resource_mut::<EventLog>().push(tick, kind);
    }
}

fn apply_command(world: &mut World, cmd: &Command) -> Result<(), String> {
    match &cmd.payload {
        CommandPayload::SpawnUnit(spawn) => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may spawn units".into());
            }
            spawn_unit(world, spawn)
        }
        CommandPayload::SetRoute { unit, waypoints } => {
            let entity = authorised_entity(world, cmd, *unit)?;
            let (domain, from) = match (world.get::<Unit>(entity), world.get::<Position>(entity)) {
                (Some(u), Some(p)) => (u.domain, p.point),
                _ => return Err(format!("{unit} has no position")),
            };
            let (planned, replanned) = plan_sea_route(world, domain, from, waypoints)?;
            let count = planned.len();
            let mut route = world
                .get_mut::<Route>(entity)
                .ok_or_else(|| format!("{unit} has no route component"))?;
            route.waypoints = planned;
            route.next = 0;
            if replanned > 0 {
                let tick = world.resource::<SimClock>().tick;
                #[allow(clippy::cast_possible_truncation)]
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::RouteReplanned {
                        unit: *unit,
                        legs: replanned as u32,
                        waypoints: count as u32,
                    },
                );
            }
            Ok(())
        }
        CommandPayload::SetSpeed { unit, speed_mps } => {
            let entity = authorised_entity(world, cmd, *unit)?;
            let mut kin = world
                .get_mut::<Kinematics>(entity)
                .ok_or_else(|| format!("{unit} has no kinematics component"))?;
            kin.ordered_speed_mps = *speed_mps;
            Ok(())
        }
    }
}

fn authorised_entity(world: &World, cmd: &Command, unit: UnitId) -> Result<Entity, String> {
    let entity = *world
        .resource::<UnitIndex>()
        .0
        .get(&unit)
        .ok_or_else(|| format!("unknown unit {unit}"))?;
    if !cmd.issuer.is_privileged() {
        let side = world
            .get::<Unit>(entity)
            .map(|u| u.side)
            .ok_or_else(|| format!("{unit} has no identity"))?;
        if cmd.side != Some(side) {
            return Err(format!("{unit} does not belong to the issuing side"));
        }
    }
    Ok(entity)
}

fn spawn_unit(world: &mut World, s: &UnitSpawn) -> Result<(), String> {
    if world.resource::<UnitIndex>().0.contains_key(&s.id) {
        return Err(format!("duplicate unit id {}", s.id));
    }
    // A scenario route that crosses land is planned round it; one that cannot be planned is dropped
    // (the unit still spawns, holding its course) and the reason is logged.
    let tick = world.resource::<SimClock>().tick;
    let (route, route_note) = match plan_sea_route(world, s.domain, s.position, &s.route) {
        Ok((planned, 0)) => (planned, None),
        #[allow(clippy::cast_possible_truncation)]
        Ok((planned, legs)) => {
            let n = planned.len() as u32;
            (
                planned,
                Some(SimEventKind::RouteReplanned {
                    unit: s.id,
                    legs: legs as u32,
                    waypoints: n,
                }),
            )
        }
        Err(reason) => (
            Vec::new(),
            Some(SimEventKind::RouteBlocked { unit: s.id, reason }),
        ),
    };
    let entity = world
        .spawn((
            Unit {
                id: s.id,
                name: s.name.clone(),
                side: s.side,
                domain: s.domain,
                platform: s.platform.clone(),
            },
            Position {
                point: s.position,
                altitude_m: s.altitude_m,
            },
            SensorSuite(s.sensors.clone()),
            Signature {
                rcs_m2: s.rcs_m2.unwrap_or_else(|| default_rcs_m2(s.domain)),
            },
            Kinematics {
                heading_deg: sim_geo::normalize_azimuth_deg(s.heading_deg),
                speed_mps: s.speed_mps.clamp(0.0, s.max_speed_mps),
                ordered_speed_mps: s.speed_mps.clamp(0.0, s.max_speed_mps),
                max_speed_mps: s.max_speed_mps,
                accel_mps2: s.accel_mps2,
                max_turn_rate_dps: s.max_turn_rate_dps,
                min_turn_radius_m: s.min_turn_radius_m.unwrap_or(0.0),
            },
            Route {
                waypoints: route,
                next: 0,
            },
        ))
        .id();
    world.resource_mut::<UnitIndex>().0.insert(s.id, entity);
    world
        .resource_mut::<EventLog>()
        .push(tick, SimEventKind::UnitSpawned { unit: s.id });
    if let Some(note) = route_note {
        world.resource_mut::<EventLog>().push(tick, note);
    }
    Ok(())
}
