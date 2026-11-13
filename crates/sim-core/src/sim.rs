//! The `Simulation` facade: the only public way to drive the world.

use crate::adjudication::{adjudicate, AdjudicationLog, ADJUDICATION_VIEW_LIMIT};
use crate::ai::{ai_system, MissionState};
use crate::c2::C2State;
use crate::combat::{
    damage_control_system, default_damage_profile, engage, weapons_system, Health, Magazine,
    WeaponCatalog, WeaponsInFlight,
};
use crate::command::DoctrineScope;
use crate::command::{Command, CommandPayload, Issuer, LoggedCommand, UnitSpawn, PROTOCOL_VERSION};
use crate::components::{Domain, Kinematics, Position, Route, Unit};
use crate::detection::{
    default_rcs_m2, detection_system, ContactTable, EnvState, Jammer, SensorSuite, Signature,
};
use crate::error::SimError;
use crate::esm::{BearingTable, EmitterLibrary};
use crate::events::{EventLog, SimEvent, SimEventKind};
use crate::hash::StateHash;
use crate::ids::{SideId, UnitId};
use crate::mines::{lay_mines, mine_system, Minefields};
use crate::movement::movement_system;
use crate::planning::{
    command_authority, planning_system, Authority, Loadout, PlanningLayer, QueuedOrder,
};
use crate::point_defence::{point_defence_system, PointDefence};
use crate::rng::RngStreams;
use crate::save::{SaveGame, UnitRecord, SAVE_FORMAT_VERSION, SAVE_MAGIC};
use crate::scenario_events::{facilitate, scenario_events_system, EventEngine, MessageLog};
use crate::seats::Seats;
use crate::snapshot::{ContactView, PostureView, RouteView, Snapshot, UnitView, Viewer};
use crate::snapshot::{StoreView, WeaponView};
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::SingleThreadedExecutor;
use sim_c2::track_affiliation;
use sim_geo::land::LandMask;
use sim_geo::terrain::Terrain;
use sim_geo::GeoPoint;
use sim_weapons::Status;
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

/// Heights and depths from terrain and bathymetry packs (Phase 4 slice 2). `None` = no packs:
/// models that need a depth fall back to their scenario values.
#[derive(Debug, Default, Clone, Resource)]
pub(crate) struct TerrainRes(pub(crate) Option<Arc<Terrain>>);

/// A route as planned: the waypoints to store, legs replanned, and legs with no route deep
/// enough for the unit's draft (planned round land only).
pub(crate) struct SeaRoute {
    pub(crate) waypoints: Vec<GeoPoint>,
    pub(crate) replanned: usize,
    pub(crate) shallow_legs: usize,
    /// Every leg was planned deep enough for the unit's draft.
    pub(crate) charted: bool,
}

/// Plans `waypoints` from `from` round land for ships and submarines, and, with terrain packs
/// and a known draft, out of water shallower than the draft plus under-keel clearance
/// (`docs/models/shallow-water.md`).
pub(crate) fn plan_sea_route(
    world: &World,
    domain: Domain,
    draft_m: Option<f64>,
    from: GeoPoint,
    waypoints: &[GeoPoint],
) -> Result<SeaRoute, String> {
    let mask = world.resource::<LandMaskRes>().0.clone();
    let terrain = world.resource::<TerrainRes>().0.clone();
    match (mask, domain) {
        (Some(mask), Domain::Surface | Domain::Subsurface) if !waypoints.is_empty() => {
            let limit = match (&terrain, draft_m) {
                (Some(t), Some(d)) if d > 0.0 => Some(sim_geo::land::DepthLimit {
                    terrain: t,
                    min_depth_m: d + crate::planning::UNDER_KEEL_CLEARANCE_M,
                }),
                _ => None,
            };
            let charted = limit.is_some();
            mask.plan_route_for(from, waypoints, limit)
                .map(|p| SeaRoute {
                    charted: charted && p.shallow_legs == 0,
                    waypoints: p.waypoints,
                    replanned: p.replanned,
                    shallow_legs: p.shallow_legs,
                })
                .map_err(|e| format!("route refused: {e}"))
        }
        _ => Ok(SeaRoute {
            waypoints: waypoints.to_vec(),
            replanned: 0,
            shallow_legs: 0,
            charted: false,
        }),
    }
}

/// Stable unit id → ECS entity.
#[derive(Debug, Default, Resource)]
pub(crate) struct UnitIndex(pub(crate) BTreeMap<UnitId, Entity>);

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
        // Display only: saved but not hashed (outcomes.rs).
        world.insert_resource(crate::outcomes::OutcomeFeed::default());
        world.insert_resource(CommandLog::default());
        world.insert_resource(UnitIndex::default());
        world.insert_resource(ContactTable::default());
        world.insert_resource(C2State::default());
        world.insert_resource(EnvState::default());
        world.insert_resource(BearingTable::default());
        world.insert_resource(WeaponCatalog::default());
        world.insert_resource(WeaponsInFlight::default());
        world.insert_resource(PointDefence::default());
        world.insert_resource(Minefields::default());
        world.insert_resource(AdjudicationLog::default());
        world.insert_resource(EventEngine::default());
        world.insert_resource(MessageLog::default());
        world.insert_resource(Seats::default());
        world.insert_resource(PlanningLayer::default());
        world.insert_resource(EmitterLibrary::default());
        world.insert_resource(LandMaskRes::default());
        world.insert_resource(TerrainRes::default());
        world.insert_resource(crate::weapon_tracks::WeaponTracks::default());
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

    /// The id of the coastline in use, if any (record it to replay the run).
    #[must_use]
    pub fn land_mask_id(&self) -> Option<String> {
        self.world
            .resource::<LandMaskRes>()
            .0
            .as_ref()
            .map(|m| m.id().to_owned())
    }

    /// Use these terrain and bathymetry packs from now on. Like the coastline, part of the state
    /// hash (by id) and not saved: set it again after loading a save.
    pub fn set_terrain(&mut self, terrain: Option<Arc<Terrain>>) {
        self.world.insert_resource(TerrainRes(terrain));
    }

    /// The id of the terrain in use, if any (record it to replay the run).
    #[must_use]
    pub fn terrain_id(&self) -> Option<String> {
        self.terrain().map(|t| t.id().to_owned())
    }

    /// The water of every choke point the planning layer names, by name (display only).
    #[must_use]
    pub fn choke_depths(&self) -> Vec<crate::lanes::ChokeDepths> {
        let layer = self.world.resource::<PlanningLayer>();
        let names: std::collections::BTreeSet<&str> = layer
            .zones
            .iter()
            .filter_map(|z| z.choke_point.as_deref())
            .collect();
        names
            .into_iter()
            .filter_map(|n| crate::lanes::choke_depths(layer, self.terrain().map(|t| &**t), n))
            .collect()
    }

    /// The shipping lanes in play.
    #[must_use]
    pub fn lanes(&self) -> Vec<crate::lanes::Lane> {
        self.world.resource::<PlanningLayer>().lanes.clone()
    }

    /// The terrain in use, if any.
    #[must_use]
    pub fn terrain(&self) -> Option<&Arc<Terrain>> {
        self.world.resource::<TerrainRes>().0.as_ref()
    }

    /// The water under `unit` and along the rest of its route, from the terrain packs, when
    /// `viewer` sees the unit as its own (any unit for the white cell). `None` without terrain.
    #[must_use]
    pub fn route_depths(&self, unit: UnitId, viewer: Viewer) -> Option<crate::depths::RouteDepths> {
        let terrain = self.terrain()?;
        let e = *self.world.resource::<UnitIndex>().0.get(&unit)?;
        let u = self.world.get::<Unit>(e)?;
        if let Viewer::Side { side } = viewer {
            if u.side != side {
                return None;
            }
        }
        Some(crate::depths::route_depths(
            terrain,
            u,
            self.world.get::<Position>(e)?,
            self.world.get::<Route>(e)?,
        ))
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

    /// Missiles and torpedoes in flight, in launch order (ground truth: for tests, AAR and the
    /// white cell, never for a side's picture).
    #[must_use]
    pub fn weapons_in_flight(&self) -> &[crate::combat::WeaponInFlight] {
        &self.world.resource::<WeaponsInFlight>().list
    }

    /// Coverage and gaps of sense zone `zone` by its side's sensors now
    /// (`docs/models/planning-layer.md` §Coverage). A query: it changes nothing. `None`: no such
    /// sense zone.
    pub fn zone_coverage(&mut self, zone: u32) -> Option<crate::coverage::ZoneCoverage> {
        crate::coverage::zone_coverage(&mut self.world, zone)
    }

    /// The side a zone belongs to, if it exists.
    #[must_use]
    pub fn zone_side(&self, zone: u32) -> Option<SideId> {
        self.world
            .resource::<PlanningLayer>()
            .zones
            .iter()
            .find(|z| z.id == zone)
            .map(|z| z.side)
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
            &Jammer,
            &Magazine,
            &Health,
            &MissionState,
            Option<&Loadout>,
            Option<&crate::fuel::Fuel>,
        )>();
        let mut v: Vec<UnitRecord> = q
            .iter(&self.world)
            .map(|(u, p, k, r, s, sig, j, m, hl, ms, lo, fu)| UnitRecord {
                unit: u.clone(),
                position: *p,
                kinematics: *k,
                route: r.clone(),
                sensors: s.clone(),
                signature: Some(*sig),
                jammer: j.0,
                magazine: m.clone(),
                health: Some(hl.clone()),
                mission: ms.clone(),
                loadout: lo.cloned(),
                fuel: fu.copied(),
            })
            .collect();
        v.sort_by_key(|r| r.unit.id);
        v
    }

    /// State as seen by `viewer`.
    pub fn snapshot(&mut self, viewer: Viewer) -> Snapshot {
        let tick = self.tick();
        let c2 = self.world.resource::<C2State>().clone();
        let planning = self.world.resource::<PlanningLayer>().clone();
        let records = self.unit_records();
        let side_of: BTreeMap<UnitId, SideId> =
            records.iter().map(|r| (r.unit.id, r.unit.side)).collect();
        let units = records
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
                let doctrine = planning.doctrine_for(&c2, &r.unit, r.position.point);
                UnitView {
                    doctrine,
                    doctrine_set: r.unit.doctrine,
                    status: r.health.as_ref().map_or(Status::Operational, |h| h.status),
                    damage_fraction: r
                        .health
                        .as_ref()
                        .map_or(0.0, |h| h.damage / h.profile.damage_points),
                    criticals: r
                        .health
                        .as_ref()
                        .map_or_else(Vec::new, |h| h.criticals.clone()),
                    fires: r.health.as_ref().map_or(0, |h| h.fires),
                    flooding: r.health.as_ref().is_some_and(|h| h.flooding),
                    mission: r.mission.mission.clone(),
                    mission_complete: r.mission.complete,
                    fuel_fraction: r.fuel.map(|f| f.fraction()),
                    stores: r
                        .magazine
                        .0
                        .iter()
                        .map(|(w, n)| StoreView {
                            weapon: w.clone(),
                            count: *n,
                        })
                        .collect(),
                    group: r.unit.group.clone(),
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
                error: c.error_at(tick),
                classification: c.classification,
                domain: c.domain,
                category: c.category.clone(),
                platform: c.platform.clone(),
                affiliation: side_of
                    .get(&c.target)
                    .and_then(|t| track_affiliation(c.classification, c2.posture(side, *t))),
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
            postures: c2
                .postures
                .iter()
                .filter(|((side, _), _)| match viewer {
                    Viewer::WhiteCell => true,
                    Viewer::Side { side: s } => *side == s,
                })
                .map(|(&(side, toward), &posture)| PostureView {
                    side,
                    toward,
                    posture,
                })
                .collect(),
            environment: self.world.resource::<EnvState>().0,
            outcomes: crate::outcomes::visible(
                self.world.resource::<crate::outcomes::OutcomeFeed>(),
                tick,
                match viewer {
                    Viewer::WhiteCell => None,
                    Viewer::Side { side } => Some(side),
                },
            ),
            adjudications: match viewer {
                Viewer::WhiteCell => {
                    let log = &self.world.resource::<AdjudicationLog>().0;
                    log[log.len().saturating_sub(ADJUDICATION_VIEW_LIMIT)..].to_vec()
                }
                Viewer::Side { .. } => Vec::new(),
            },
            seats: self.world.resource::<Seats>().visible(match viewer {
                Viewer::WhiteCell => None,
                Viewer::Side { side } => Some(side),
            }),
            injects: match viewer {
                Viewer::WhiteCell => self.world.resource::<EventEngine>().inject_status(),
                Viewer::Side { .. } => Vec::new(),
            },
            scenario_events: match viewer {
                Viewer::WhiteCell => self.world.resource::<EventEngine>().event_status(),
                Viewer::Side { .. } => Vec::new(),
            },
            messages: self.world.resource::<MessageLog>().visible(match viewer {
                Viewer::WhiteCell => None,
                Viewer::Side { side } => Some(side),
            }),
            minefields: self.world.resource::<Minefields>().visible(match viewer {
                Viewer::WhiteCell => None,
                Viewer::Side { side } => Some(side),
            }),
            weapons: self
                .world
                .resource::<WeaponsInFlight>()
                .list
                .iter()
                .filter(|w| match viewer {
                    Viewer::WhiteCell => true,
                    Viewer::Side { side } => w.side == side,
                })
                .map(|w| WeaponView {
                    #[allow(clippy::cast_possible_truncation)]
                    id: w.id as u32,
                    side: w.side,
                    weapon: w.weapon.clone(),
                    position: w.position,
                    altitude_m: w.altitude_m,
                    heading_deg: w.heading_deg,
                    homing: w.homing,
                })
                .collect(),
            comms: match viewer {
                Viewer::WhiteCell => {
                    let mut sides: Vec<SideId> = side_of.values().copied().collect();
                    sides.sort_unstable();
                    sides.dedup();
                    sides
                        .into_iter()
                        .flat_map(|s| crate::planning::comms_views(&self.world, s))
                        .collect()
                }
                Viewer::Side { side } => crate::planning::comms_views(&self.world, side),
            },
            action_groups: self
                .world
                .resource::<PlanningLayer>()
                .action_groups
                .iter()
                .filter(|g| match viewer {
                    Viewer::WhiteCell => true,
                    Viewer::Side { side } => g.side == side,
                })
                .cloned()
                .collect(),
            weapon_tracks: crate::weapon_tracks::views(
                &self.world,
                match viewer {
                    Viewer::WhiteCell => None,
                    Viewer::Side { side } => Some(side),
                },
            ),
            posts: crate::planning::post_views(
                &self.world,
                match viewer {
                    Viewer::WhiteCell => None,
                    Viewer::Side { side } => Some(side),
                },
            ),
            bearings: self.world.resource::<BearingTable>().views(
                match viewer {
                    Viewer::WhiteCell => None,
                    Viewer::Side { side } => Some(side),
                },
                tick,
            ),
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
        // Only when set, so runs without packs keep their hashes (and the golden files).
        if let Some(t) = self.terrain() {
            h.str("terrain");
            h.str(t.id());
        }
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
            h.opt_str(r.unit.group.as_deref());
            hash_doctrine(&mut h, &r.unit.doctrine);
            h.f64(r.signature.map_or(0.0, |s| s.rcs_m2));
            h.opt_f64(r.signature.and_then(|s| s.radiated_noise_db));
            h.opt_f64(r.signature.and_then(|s| s.length_m));
            h.opt_str(r.unit.category.as_deref());
            hash_mission(&mut h, &r.mission);
            h.u64(r.magazine.0.len() as u64);
            for (w, n) in &r.magazine.0 {
                h.str(w);
                h.u64(u64::from(*n));
            }
            if let Some(hl) = &r.health {
                h.f64(hl.profile.damage_points);
                h.f64(hl.damage);
                h.u64(u64::from(hl.fires));
                h.u8(u8::from(hl.flooding));
                for c in &hl.criticals {
                    h.u8(c.code());
                }
            }
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
                h.opt_f64(s.clutter_db_per_sea_state);
                h.opt_f64(s.beamwidth_deg);
                h.u8(u8::from(s.active));
            }
            h.opt_f64(r.jammer.map(|j| j.jn_db_at_1km));
            // Hashed only when present, so units without fuel keep their hashes.
            if let Some(f) = &r.fuel {
                h.f64(f.capacity_t);
                h.f64(f.endurance_m);
                h.f64(f.cruise_mps);
                h.f64(f.fuel_t);
                h.f64(f.bingo);
            }
        }
        let catalog = self.world.resource::<WeaponCatalog>();
        h.u64(catalog.0.len() as u64);
        for (id, w) in &catalog.0 {
            h.str(id);
            h.u8(w.kind.code());
            h.f64(w.min_range_m);
            h.f64(w.max_range_m);
            h.f64(w.speed_mps);
            h.f64(w.warhead_kg);
            h.f64(w.base_ph);
        }
        let wif = self.world.resource::<WeaponsInFlight>();
        h.u64(wif.next_id);
        for w in &wif.list {
            h.u64(w.id);
            h.str(&w.weapon);
            h.u16(w.side.0);
            h.u64(w.target.0);
            h.f64(w.position.lat_deg);
            h.f64(w.position.lon_deg);
            h.f64(w.aim.lat_deg);
            h.f64(w.aim.lon_deg);
            h.f64(w.range_left_m);
            h.u8(u8::from(w.seeker_tried) | (u8::from(w.homing) << 1));
            h.opt_f64(w.decoy_effect);
        }
        let adj = self.world.resource::<AdjudicationLog>();
        h.u64(adj.0.len() as u64);
        for a in &adj.0 {
            h.u64(a.tick.0);
            h.str(&a.umpire);
            h.str(&a.reason);
        }
        let ev = self.world.resource::<EventEngine>();
        h.u64(ev.events.len() as u64);
        for (id, rt) in &ev.runtime {
            h.str(id);
            h.u8(u8::from(rt.enabled));
            h.u64(u64::from(rt.fired));
            h.u64(rt.inside.len() as u64);
            for u in &rt.inside {
                h.u64(*u);
            }
        }
        for id in ev.reserves.keys() {
            h.u64(id.0);
        }
        h.u64(ev.injects.len() as u64);
        for (id, n) in &ev.sent {
            h.str(id);
            h.u64(u64::from(*n));
        }
        for (u, side) in &ev.sides {
            h.u64(u.0);
            h.u16(side.0);
        }
        let seats = self.world.resource::<Seats>();
        h.u64(seats.0.len() as u64);
        for s in seats.0.values() {
            h.u64(u64::from(s.id));
            h.str(&s.name);
            match s.role {
                crate::seats::SeatRole::WhiteCell => h.u16(u16::MAX),
                crate::seats::SeatRole::Side { side } => h.u16(side.0),
            }
        }
        self.world.resource::<PlanningLayer>().hash(&mut h);
        self.world
            .resource::<crate::weapon_tracks::WeaponTracks>()
            .hash(&mut h);
        let msgs = self.world.resource::<MessageLog>();
        h.u64(msgs.0.len() as u64);
        for m in &msgs.0 {
            h.u64(m.tick.0);
            h.str(&m.from);
            h.str(&m.text);
        }
        let mf = self.world.resource::<Minefields>();
        h.u64(u64::from(mf.next_id));
        for f in &mf.fields {
            h.u64(u64::from(f.id));
            h.u16(f.side.0);
            h.f64(f.centre.lat_deg);
            h.f64(f.centre.lon_deg);
            h.f64(f.radius_m);
            h.u64(u64::from(f.mines));
            h.str(&f.weapon);
        }
        for (u, p) in &mf.last_position {
            h.u64(u.0);
            h.f64(p.lat_deg);
            h.f64(p.lon_deg);
        }
        let pd = self.world.resource::<PointDefence>();
        for ((d, w), busy) in &pd.channels {
            h.u64(d.0);
            h.str(w);
            for b in busy {
                h.u64(b.0);
            }
        }
        for ((d, t), seen) in &pd.first_seen {
            h.u64(d.0);
            h.u64(*t);
            h.u64(seen.0);
        }
        for s in &pd.shots {
            h.u64(s.defender.0);
            h.str(&s.interceptor);
            h.u64(s.threat);
            h.u64(s.resolve_at.0);
        }
        let lib = self.world.resource::<EmitterLibrary>();
        h.u64(lib.0.len() as u64);
        for (emitter, carriers) in &lib.0 {
            h.str(emitter);
            h.u64(carriers.len() as u64);
            for c in carriers {
                h.str(&c.platform);
                h.str(&c.category);
                h.str(&c.domain);
            }
        }
        let bearings = self.world.resource::<BearingTable>();
        for (side, targets) in &bearings.by_side {
            for (target, observers) in targets {
                for (observer, r) in observers {
                    h.u16(side.0);
                    h.u64(target.0);
                    h.u64(observer.0);
                    h.u64(r.tick.0);
                    h.f64(r.line.from.lat_deg);
                    h.f64(r.line.from.lon_deg);
                    h.f64(r.line.bearing_deg);
                    h.f64(r.line.sigma_deg);
                    h.str(&r.emitter);
                }
            }
        }
        let env = self.world.resource::<EnvState>().0;
        h.u8(env.sea_state);
        h.f64(env.weather_loss_db);
        #[allow(clippy::cast_sign_loss)]
        h.u64(env.start_unix_s as u64);
        h.f64(env.visibility_km);
        let c2 = self.world.resource::<C2State>();
        for (&(side, toward), posture) in &c2.postures {
            h.u16(side.0);
            h.u16(toward.0);
            h.u8(posture.code());
        }
        h.u64(c2.side_doctrine.len() as u64);
        for (side, d) in &c2.side_doctrine {
            h.u16(side.0);
            hash_doctrine(&mut h, d);
        }
        h.u64(c2.group_doctrine.len() as u64);
        for ((side, group), d) in &c2.group_doctrine {
            h.u16(side.0);
            h.str(group);
            hash_doctrine(&mut h, d);
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
            h.f64(c.cov.ee);
            h.f64(c.cov.en);
            h.f64(c.cov.nn);
            h.u8(c.classification.code());
            h.u8(c.domain.map_or(255, Domain::code));
            h.opt_str(c.category.as_deref());
            h.opt_str(c.platform.as_deref());
            h.u64(c.detected_by.0);
            h.str(&c.sensor_id);
        }
        h.finish()
    }

    /// Captures everything needed to resume this run exactly.
    pub fn save(&mut self) -> SaveGame {
        self.save_with(true)
    }

    /// A save without the event log: a replay keyframe. The run continues exactly from it (the
    /// model reads only the current tick's events), but its state hash differs from the original
    /// run's, which also counts past events.
    pub fn keyframe(&mut self) -> SaveGame {
        self.save_with(false)
    }

    fn save_with(&mut self, with_events: bool) -> SaveGame {
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
            events: if with_events {
                self.events().to_vec()
            } else {
                Vec::new()
            },
            contacts: self.world.resource::<ContactTable>().records(),
            next_contact_numbers: self.world.resource::<ContactTable>().next_numbers(),
            c2: self.world.resource::<C2State>().clone(),
            environment: self.world.resource::<EnvState>().0,
            bearings: self.world.resource::<BearingTable>().clone(),
            emitter_library: self.world.resource::<EmitterLibrary>().clone(),
            weapon_catalog: self.world.resource::<WeaponCatalog>().clone(),
            weapons_in_flight: self.world.resource::<WeaponsInFlight>().clone(),
            point_defence: self.world.resource::<PointDefence>().clone(),
            minefields: self.world.resource::<Minefields>().clone(),
            adjudications: self.world.resource::<AdjudicationLog>().clone(),
            events_engine: self.world.resource::<EventEngine>().clone(),
            messages: self.world.resource::<MessageLog>().clone(),
            seats: self.world.resource::<Seats>().clone(),
            planning: self.world.resource::<PlanningLayer>().clone(),
            weapon_tracks: self
                .world
                .resource::<crate::weapon_tracks::WeaponTracks>()
                .clone(),
            outcomes: self
                .world
                .resource::<crate::outcomes::OutcomeFeed>()
                .clone(),
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
        sim.world.insert_resource(save.c2);
        sim.world.insert_resource(EnvState(save.environment));
        sim.world.insert_resource(save.bearings);
        sim.world.insert_resource(save.emitter_library);
        sim.world.insert_resource(save.weapon_catalog);
        sim.world.insert_resource(save.weapons_in_flight);
        sim.world.insert_resource(save.point_defence);
        sim.world.insert_resource(save.minefields);
        sim.world.insert_resource(save.adjudications);
        sim.world.insert_resource(save.events_engine);
        sim.world.insert_resource(save.messages);
        sim.world.insert_resource(save.seats);
        sim.world.insert_resource(save.planning);
        sim.world.insert_resource(save.weapon_tracks);
        sim.world.insert_resource(save.outcomes);
        sim.world.insert_resource(ContactTable::restore(
            save.contacts,
            save.next_contact_numbers,
        ));

        let mut index = BTreeMap::new();
        for r in save.units {
            let id = r.unit.id;
            let signature = r.signature.unwrap_or(Signature {
                rcs_m2: default_rcs_m2(r.unit.domain),
                radiated_noise_db: None,
                length_m: None,
            });
            let health = r
                .health
                .unwrap_or_else(|| Health::new(default_damage_profile(r.unit.domain)));
            // Saves before format 17 have no loadout: take what the unit has now.
            let loadout = r.loadout.unwrap_or_else(|| Loadout {
                sensors: r.sensors.0.clone(),
                max_speed_mps: r.kinematics.max_speed_mps,
                stores: r.magazine.0.clone(),
            });
            let fuel = r.fuel;
            let e = sim
                .world
                .spawn((
                    r.unit,
                    r.position,
                    r.kinematics,
                    r.route,
                    r.sensors,
                    signature,
                    Jammer(r.jammer),
                    health,
                    r.magazine,
                    r.mission,
                    loadout,
                ))
                .id();
            if let Some(f) = fuel {
                sim.world.entity_mut(e).insert(f);
            }
            if index.insert(id, e).is_some() {
                return Err(SimError::CorruptSave(format!("duplicate unit {id}")));
            }
        }
        sim.world.insert_resource(UnitIndex(index));
        Ok(sim)
    }
}

fn hash_mission(h: &mut StateHash, m: &MissionState) {
    h.u8(u8::from(m.complete));
    match &m.mission {
        None => h.u8(255),
        Some(mission) => {
            h.u8(mission.code());
            match mission {
                sim_ai::Mission::Patrol { centre, radius_m } => {
                    h.f64(centre.lat_deg);
                    h.f64(centre.lon_deg);
                    h.f64(*radius_m);
                }
                sim_ai::Mission::Ferry { destination } => {
                    h.f64(destination.lat_deg);
                    h.f64(destination.lon_deg);
                }
                sim_ai::Mission::Escort {
                    escorted,
                    bearing_deg,
                    range_m,
                } => {
                    h.u64(*escorted);
                    h.f64(*bearing_deg);
                    h.f64(*range_m);
                }
                sim_ai::Mission::Strike { contact } => h.u64(u64::from(*contact)),
                sim_ai::Mission::Support { station, radius_m }
                | sim_ai::Mission::MineClearing {
                    centre: station,
                    radius_m,
                } => {
                    h.f64(station.lat_deg);
                    h.f64(station.lon_deg);
                    h.f64(*radius_m);
                }
                sim_ai::Mission::Mining {
                    centre,
                    radius_m,
                    mines,
                } => {
                    h.f64(centre.lat_deg);
                    h.f64(centre.lon_deg);
                    h.f64(*radius_m);
                    h.u64(u64::from(*mines));
                }
                sim_ai::Mission::Cargo {
                    origin,
                    destination,
                    tonnes,
                } => {
                    h.opt_f64(origin.map(|o| o.lat_deg));
                    h.opt_f64(origin.map(|o| o.lon_deg));
                    h.f64(destination.lat_deg);
                    h.f64(destination.lon_deg);
                    h.f64(*tonnes);
                }
                sim_ai::Mission::BarrierPatrol { start, end } => {
                    for p in [start, end] {
                        h.f64(p.lat_deg);
                        h.f64(p.lon_deg);
                    }
                }
                sim_ai::Mission::Denial { choke_point } => h.str(choke_point),
            }
        }
    }
    // Hashed only when used, so earlier runs keep their hashes.
    if m.phase != 0 || m.since.is_some() {
        h.u8(m.phase);
        h.u64(m.since.map_or(u64::MAX, |t| t.0));
    }
}

fn hash_doctrine(h: &mut StateHash, d: &sim_c2::Doctrine) {
    h.u8(d.wcs.map_or(255, sim_c2::Wcs::code));
    h.u8(d.emcon.map_or(255, sim_c2::Emcon::code));
    h.u8(d.engage_unfriendly.map_or(255, u8::from));
}

fn build_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    // ADR-0003 §3: single-threaded, fixed order.
    schedule.set_executor(SingleThreadedExecutor::new());
    schedule.add_systems(
        (
            apply_commands_system,
            movement_system,
            crate::weapon_tracks::weapon_tracks_system,
            point_defence_system,
            weapons_system,
            mine_system,
            damage_control_system,
            detection_system,
            ai_system,
            planning_system,
            scenario_events_system,
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
        let checked = world.resource::<Seats>().check(&cmd).and_then(|()| {
            match command_authority(world, &cmd) {
                Authority::Ok => Ok(()),
                Authority::Refused(reason) => Err(reason),
                // Held for the alternate command post; logged as refused for now, with why.
                Authority::Queued { release, reason } => {
                    world
                        .resource_mut::<PlanningLayer>()
                        .queued
                        .push(QueuedOrder {
                            release,
                            seq,
                            command: cmd.clone(),
                        });
                    Err(format!("queued: {reason}"))
                }
            }
        });
        let kind = match checked.and_then(|()| apply_command(world, &cmd)) {
            Ok(()) => SimEventKind::CommandApplied { seq },
            Err(reason) => SimEventKind::CommandRejected { seq, reason },
        };
        world.resource_mut::<EventLog>().push(tick, kind);
    }
}

pub(crate) fn apply_command(world: &mut World, cmd: &Command) -> Result<(), String> {
    match &cmd.payload {
        CommandPayload::SpawnUnit(spawn) => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may spawn units".into());
            }
            spawn_unit(world, spawn)
        }
        CommandPayload::SetRoute { unit, waypoints } => {
            let entity = authorised_entity(world, cmd, *unit)?;
            let (domain, draft, from) =
                match (world.get::<Unit>(entity), world.get::<Position>(entity)) {
                    (Some(u), Some(p)) => (u.domain, u.draft_m, p.point),
                    _ => return Err(format!("{unit} has no position")),
                };
            let SeaRoute {
                waypoints: planned,
                replanned,
                shallow_legs,
                charted,
            } = plan_sea_route(world, domain, draft, from, waypoints)?;
            let count = planned.len();
            let mut route = world
                .get_mut::<Route>(entity)
                .ok_or_else(|| format!("{unit} has no route component"))?;
            route.waypoints = planned;
            route.next = 0;
            route.charted = charted;
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
            if shallow_legs > 0 {
                let tick = world.resource::<SimClock>().tick;
                #[allow(clippy::cast_possible_truncation)]
                world.resource_mut::<EventLog>().push(
                    tick,
                    SimEventKind::RouteShallow {
                        unit: *unit,
                        legs: shallow_legs as u32,
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
        CommandPayload::SetPosture {
            side,
            toward,
            posture,
        } => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may set postures".into());
            }
            world
                .resource_mut::<C2State>()
                .postures
                .insert((*side, *toward), *posture);
            Ok(())
        }
        CommandPayload::SetEmitterLibrary { entries } => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may set the emitter library".into());
            }
            let mut lib = EmitterLibrary::default();
            for e in entries {
                let mut carriers = e.carriers.clone();
                carriers.sort();
                carriers.dedup();
                lib.0.insert(e.emitter.clone(), carriers);
            }
            world.insert_resource(lib);
            Ok(())
        }
        CommandPayload::SetWeaponCatalog { weapons } => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may set the weapon catalogue".into());
            }
            let mut cat = WeaponCatalog::default();
            for w in weapons {
                cat.0.insert(w.id.clone(), w.clone());
            }
            world.insert_resource(cat);
            Ok(())
        }
        CommandPayload::Engage {
            shooter,
            contact,
            weapon,
            rounds,
        } => {
            let entity = authorised_entity(world, cmd, *shooter)?;
            engage(world, cmd, *shooter, *contact, weapon, *rounds, entity)
        }
        CommandPayload::Adjudicate { reason, action } => match &cmd.issuer {
            Issuer::WhiteCell { umpire } => adjudicate(world, umpire, reason, action),
            _ => Err("only the white cell may adjudicate".into()),
        },
        CommandPayload::SetPlanning {
            facilities,
            zones,
            action_groups,
            lanes,
        } => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may set the planning layer".into());
            }
            if let Some(lanes) = lanes {
                crate::lanes::validate_lanes(lanes)?;
            }
            let mut layer = world.resource_mut::<PlanningLayer>();
            layer.set(facilities.clone(), zones.clone());
            if let Some(groups) = action_groups {
                layer.action_groups.clone_from(groups);
            }
            if let Some(lanes) = lanes {
                let mut lanes = lanes.clone();
                lanes.sort_by_key(|l| l.id);
                layer.lanes = lanes;
            }
            Ok(())
        }
        CommandPayload::AssignGroupMission { .. }
        | CommandPayload::DetachFromGroup { .. }
        | CommandPayload::JoinGroup { .. } => crate::planning::group_order(world, cmd),
        CommandPayload::SetSeats { seats } => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may set seats".into());
            }
            world.insert_resource(Seats(seats.iter().map(|s| (s.id, s.clone())).collect()));
            Ok(())
        }
        CommandPayload::Facilitate { action } => match &cmd.issuer {
            Issuer::WhiteCell { umpire } => facilitate(world, umpire, action),
            _ => Err("only the white cell may send injects or run events".into()),
        },
        CommandPayload::SetEvents {
            events,
            reserves,
            injects,
        } => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may set events".into());
            }
            let index = &world.resource::<UnitIndex>().0;
            if let Some(r) = reserves.iter().find(|r| index.contains_key(&r.id)) {
                return Err(format!("reserve {} is already in play", r.id));
            }
            world.resource_mut::<EventEngine>().load(
                events.clone(),
                reserves.clone(),
                injects.clone(),
            );
            Ok(())
        }
        CommandPayload::CreateMinefield {
            side,
            centre,
            radius_m,
            mines,
            weapon,
        } => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may create minefields".into());
            }
            if !world.resource::<WeaponCatalog>().0.contains_key(weapon) {
                return Err(format!("unknown mine type {weapon}"));
            }
            world.resource_mut::<Minefields>().add(
                *side,
                *centre,
                *radius_m,
                *mines,
                weapon.clone(),
            );
            Ok(())
        }
        CommandPayload::LayMines {
            unit,
            mines,
            radius_m,
        } => {
            let entity = authorised_entity(world, cmd, *unit)?;
            lay_mines(world, *unit, entity, *mines, *radius_m).map(|_| ())
        }
        CommandPayload::AssignMission { unit, mission } => {
            let entity = authorised_entity(world, cmd, *unit)?;
            // A new mission ends any diversion (repair, fuel, weather, staging).
            world.resource_mut::<PlanningLayer>().diverted.remove(unit);
            let mut m = world
                .get_mut::<MissionState>(entity)
                .ok_or_else(|| format!("{unit} cannot take missions"))?;
            m.mission = mission.clone();
            m.complete = false;
            m.phase = 0;
            m.since = None;
            Ok(())
        }
        CommandPayload::SetEnvironment { environment } => {
            if !cmd.issuer.is_privileged() {
                return Err("only the scenario or white cell may set the environment".into());
            }
            world.insert_resource(EnvState(*environment));
            Ok(())
        }
        CommandPayload::SetDoctrine { scope, doctrine } => match scope {
            DoctrineScope::Side { side } => {
                authorised_side(cmd, *side)?;
                world
                    .resource_mut::<C2State>()
                    .side_doctrine
                    .insert(*side, *doctrine);
                Ok(())
            }
            DoctrineScope::Group { side, group } => {
                authorised_side(cmd, *side)?;
                world
                    .resource_mut::<C2State>()
                    .group_doctrine
                    .insert((*side, group.clone()), *doctrine);
                Ok(())
            }
            DoctrineScope::Unit { unit } => {
                let entity = authorised_entity(world, cmd, *unit)?;
                world
                    .get_mut::<Unit>(entity)
                    .ok_or_else(|| format!("{unit} has no identity"))?
                    .doctrine = *doctrine;
                Ok(())
            }
        },
    }
}

fn authorised_side(cmd: &Command, side: SideId) -> Result<(), String> {
    if cmd.issuer.is_privileged() || cmd.side == Some(side) {
        Ok(())
    } else {
        Err(format!(
            "side {} doctrine is not the issuing side's",
            side.0
        ))
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
    let mut shallow_note = None;
    let mut charted = false;
    let (route, route_note) = match plan_sea_route(world, s.domain, s.draft_m, s.position, &s.route)
    {
        Ok(planned) => {
            charted = planned.charted;
            #[allow(clippy::cast_possible_truncation)]
            if planned.shallow_legs > 0 {
                shallow_note = Some(SimEventKind::RouteShallow {
                    unit: s.id,
                    legs: planned.shallow_legs as u32,
                });
            }
            #[allow(clippy::cast_possible_truncation)]
            let note = (planned.replanned > 0).then_some(SimEventKind::RouteReplanned {
                unit: s.id,
                legs: planned.replanned as u32,
                waypoints: planned.waypoints.len() as u32,
            });
            (planned.waypoints, note)
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
                category: s.category.clone(),
                group: s.group.clone(),
                doctrine: s.doctrine,
                draft_m: s.draft_m,
            },
            Position {
                point: s.position,
                altitude_m: s.altitude_m,
            },
            SensorSuite(s.sensors.clone()),
            Signature {
                rcs_m2: s.rcs_m2.unwrap_or_else(|| default_rcs_m2(s.domain)),
                radiated_noise_db: s.radiated_noise_db,
                length_m: s.length_m,
            },
            Jammer(s.jammer),
            Magazine(s.stores.clone()),
            Loadout {
                sensors: s.sensors.clone(),
                max_speed_mps: s.max_speed_mps,
                stores: s.stores.clone(),
            },
            MissionState {
                mission: s.mission.clone(),
                complete: false,
                phase: 0,
                since: None,
            },
            Health::new(
                s.damage_profile
                    .unwrap_or_else(|| default_damage_profile(s.domain)),
            ),
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
                charted,
            },
        ))
        .id();
    if let Some(f) = &s.fuel {
        world
            .entity_mut(entity)
            .insert(crate::fuel::Fuel::from_spec(f));
    }
    world.resource_mut::<UnitIndex>().0.insert(s.id, entity);
    world
        .resource_mut::<EventLog>()
        .push(tick, SimEventKind::UnitSpawned { unit: s.id });
    for note in [route_note, shallow_note].into_iter().flatten() {
        world.resource_mut::<EventLog>().push(tick, note);
    }
    Ok(())
}
