//! Scenario files: JSON, versioned, validated on load.
//!
//! Phase 1 holds the seed, sides and initial units. A unit names a platform-database class
//! (`"platform": "GEN-FFG"`), which supplies its domain, performance, sensors and radar
//! cross-section. Any of the performance values can be overridden per unit. Missions, events
//! (triggers → conditions → actions), Lua scripts and planning objects arrive in Phase 3.
//!
//! Format history:
//! * v1: units carried domain, max speed and acceleration inline.
//! * v2: optional `platform` reference; inline values become optional overrides. v1 files still load.

#![forbid(unsafe_code)]

pub mod synthetic;

use serde::{Deserialize, Serialize};
use sim_core::{
    Carrier, Command, CommandPayload, DamageProfile, Doctrine, DoctrineScope, Domain, EmitterEntry,
    Environment, GeoPoint, Guidance, Issuer, JammerSpec, Mission, Posture, SensorKind, SensorSpec,
    SideId, Simulation, Tick, UnitId, UnitSpawn, WeaponKind, WeaponSpec,
};
use sim_db::PlatformDb;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const SCENARIO_FORMAT: &str = "nws-scenario";
pub const SCENARIO_FORMAT_VERSION: u32 = 2;

#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("cannot read scenario file: {0}")]
    Io(#[from] std::io::Error),
    #[error("scenario JSON is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a scenario file (format = {0:?})")]
    WrongFormat(String),
    #[error("scenario format version {found} is newer than this build supports ({supported})")]
    FutureVersion { found: u32, supported: u32 },
    #[error("scenario is invalid: {0}")]
    Invalid(String),
    #[error("platform database: {0}")]
    Db(#[from] sim_db::DbError),
    #[error(transparent)]
    Sim(#[from] sim_core::SimError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideDef {
    pub id: SideId,
    pub name: String,
    /// How this side regards the others. Sides not listed are neutral.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postures: Vec<SidePosture>,
    /// Side-wide doctrine; groups and units inherit unset fields from it.
    #[serde(default, skip_serializing_if = "is_default")]
    pub doctrine: Doctrine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidePosture {
    pub toward: SideId,
    pub posture: Posture,
}

/// A named group (task group, formation) of one side, with its doctrine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupDef {
    pub side: SideId,
    pub name: String,
    #[serde(default)]
    pub doctrine: Doctrine,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// A unit as written in a scenario file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioUnit {
    pub id: UnitId,
    pub name: String,
    pub side: SideId,
    /// Platform database class. When given, it supplies everything below that is left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<Domain>,
    pub position: GeoPoint,
    #[serde(default)]
    pub altitude_m: f64,
    #[serde(default)]
    pub heading_deg: f64,
    /// Initial (and ordered) speed. Defaults to the platform's cruise speed, or 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_mps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_speed_mps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accel_mps2: Option<f64>,
    #[serde(default)]
    pub route: Vec<GeoPoint>,
    /// Group within the side (declared in `groups`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Doctrine set on this unit; unset fields inherit from its group and side.
    #[serde(default, skip_serializing_if = "is_default")]
    pub doctrine: Doctrine,
    /// A noise jammer (radiates while the unit's EMCON is active). Until the database carries
    /// ECM equipment (Phase 2 slice 9), jammers are given per unit in the scenario.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jammer: Option<JammerSpec>,
    /// Mission at start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission: Option<Mission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub format: String,
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Default RNG seed. Batch runs override it per replication.
    pub seed: u64,
    /// Default run length in game seconds.
    pub duration_s: u64,
    pub sides: Vec<SideDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupDef>,
    /// Sea state and weather loss for the whole scenario (default: calm, clear).
    #[serde(default, skip_serializing_if = "is_default")]
    pub environment: Environment,
    pub units: Vec<ScenarioUnit>,
}

fn domain_from_db(s: &str) -> Option<Domain> {
    Some(match s {
        "surface" => Domain::Surface,
        "subsurface" => Domain::Subsurface,
        "air" => Domain::Air,
        "land" => Domain::Land,
        _ => return None,
    })
}

impl Scenario {
    pub fn from_json(text: &str) -> Result<Self, ScenarioError> {
        #[derive(Deserialize)]
        struct Header {
            format: String,
            format_version: u32,
        }
        let header: Header = serde_json::from_str(text)?;
        if header.format != SCENARIO_FORMAT {
            return Err(ScenarioError::WrongFormat(header.format));
        }
        if header.format_version > SCENARIO_FORMAT_VERSION {
            return Err(ScenarioError::FutureVersion {
                found: header.format_version,
                supported: SCENARIO_FORMAT_VERSION,
            });
        }
        let scenario: Scenario = serde_json::from_str(text)?;
        scenario.validate()?;
        Ok(scenario)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ScenarioError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// Structural checks: unique side and unit ids, every unit on a declared side, and every
    /// unit without a platform carrying its own domain, max speed and acceleration.
    /// Per-unit value checks run when the spawn command is submitted.
    pub fn validate(&self) -> Result<(), ScenarioError> {
        let mut sides = BTreeSet::new();
        for s in &self.sides {
            if !sides.insert(s.id) {
                return Err(ScenarioError::Invalid(format!(
                    "duplicate side id {}",
                    s.id
                )));
            }
        }
        for s in &self.sides {
            for p in &s.postures {
                if p.toward == s.id || !sides.contains(&p.toward) {
                    return Err(ScenarioError::Invalid(format!(
                        "side {} has a posture toward {}, which is itself or undeclared",
                        s.id, p.toward
                    )));
                }
            }
        }
        let mut groups = BTreeSet::new();
        for g in &self.groups {
            if !sides.contains(&g.side) || g.name.trim().is_empty() {
                return Err(ScenarioError::Invalid(format!(
                    "group {:?} needs a declared side and a name",
                    g.name
                )));
            }
            if !groups.insert((g.side, g.name.clone())) {
                return Err(ScenarioError::Invalid(format!(
                    "duplicate group {:?} on side {}",
                    g.name, g.side
                )));
            }
        }
        let mut units = BTreeSet::new();
        for u in &self.units {
            if let Some(g) = &u.group {
                if !groups.contains(&(u.side, g.clone())) {
                    return Err(ScenarioError::Invalid(format!(
                        "unit {} is in group {g:?}, which its side does not declare",
                        u.id
                    )));
                }
            }
            if !units.insert(u.id) {
                return Err(ScenarioError::Invalid(format!(
                    "duplicate unit id {}",
                    u.id
                )));
            }
            if !sides.contains(&u.side) {
                return Err(ScenarioError::Invalid(format!(
                    "unit {} is on undeclared side {}",
                    u.id, u.side
                )));
            }
            if u.platform.is_none()
                && (u.domain.is_none() || u.max_speed_mps.is_none() || u.accel_mps2.is_none())
            {
                return Err(ScenarioError::Invalid(format!(
                    "unit {} has no platform, so it needs domain, max_speed_mps and accel_mps2",
                    u.id
                )));
            }
        }
        self.environment
            .validate()
            .map_err(|e| ScenarioError::Invalid(format!("environment: {e}")))?;
        if self.duration_s == 0 {
            return Err(ScenarioError::Invalid("duration_s must be > 0".into()));
        }
        Ok(())
    }

    /// Run length in ticks. `TICK_SECONDS` is 1.0, so seconds and ticks are the same number (ADR-0003).
    #[must_use]
    pub fn duration_ticks(&self) -> Tick {
        Tick(self.duration_s)
    }

    /// Resolves every unit against the platform database into a spawn command payload.
    pub fn resolve(&self, db: &PlatformDb) -> Result<Vec<UnitSpawn>, ScenarioError> {
        self.units.iter().map(|u| resolve_unit(u, db)).collect()
    }

    /// Builds a simulation at tick 0, with the initial units queued as scenario commands.
    pub fn build(&self, seed: Option<u64>, db: &PlatformDb) -> Result<Simulation, ScenarioError> {
        let spawns = self.resolve(db)?;
        let mut sim = Simulation::new(seed.unwrap_or(self.seed));
        // Ships and submarines navigate round the bundled coastline (docs/models/land-avoidance.md).
        sim.set_land_mask(sim_geo::land::embedded());
        // Environment, the ESM emitter library, postures and doctrine first, so they are in
        // force when the units spawn.
        let mut c2 = Vec::new();
        let catalog = weapon_catalog(&spawns, db)?;
        if !catalog.is_empty() {
            c2.push(CommandPayload::SetWeaponCatalog { weapons: catalog });
        }
        let library = emitter_library(&spawns, db)?;
        if !library.is_empty() {
            c2.push(CommandPayload::SetEmitterLibrary { entries: library });
        }
        if self.environment != Environment::default() {
            c2.push(CommandPayload::SetEnvironment {
                environment: self.environment,
            });
        }
        for s in &self.sides {
            for p in &s.postures {
                c2.push(CommandPayload::SetPosture {
                    side: s.id,
                    toward: p.toward,
                    posture: p.posture,
                });
            }
            if s.doctrine != Doctrine::default() {
                c2.push(CommandPayload::SetDoctrine {
                    scope: DoctrineScope::Side { side: s.id },
                    doctrine: s.doctrine,
                });
            }
        }
        for g in &self.groups {
            c2.push(CommandPayload::SetDoctrine {
                scope: DoctrineScope::Group {
                    side: g.side,
                    group: g.name.clone(),
                },
                doctrine: g.doctrine,
            });
        }
        for payload in c2 {
            sim.submit(Command {
                tick: Tick(0),
                issuer: Issuer::Scenario,
                side: None,
                payload,
            })?;
        }
        for s in spawns {
            sim.submit(Command {
                tick: Tick(0),
                issuer: Issuer::Scenario,
                side: None,
                payload: CommandPayload::SpawnUnit(s),
            })?;
        }
        Ok(sim)
    }
}

fn resolve_unit(u: &ScenarioUnit, db: &PlatformDb) -> Result<UnitSpawn, ScenarioError> {
    let invalid = |msg: String| ScenarioError::Invalid(format!("unit {}: {msg}", u.id));
    let (
        domain,
        max_speed,
        cruise,
        accel,
        sensors,
        rcs,
        turn,
        noise,
        category,
        length,
        stores,
        damage,
    ) = match &u.platform {
        Some(pid) => {
            let p = db
                .platform(pid)?
                .ok_or_else(|| invalid(format!("unknown platform {pid}")))?;
            let db_domain = domain_from_db(&p.domain).ok_or_else(|| {
                invalid(format!("platform {pid} has unknown domain {}", p.domain))
            })?;
            if let Some(d) = u.domain {
                if d != db_domain {
                    return Err(invalid(format!(
                        "domain {d:?} contradicts platform {pid} ({db_domain:?})"
                    )));
                }
            }
            let sensors = db
                .sensors_of(pid)?
                .into_iter()
                .map(|s| {
                    let kind = SensorKind::from_db(&s.kind).ok_or_else(|| {
                        invalid(format!(
                            "sensor {} has unknown kind {}",
                            s.sensor_id, s.kind
                        ))
                    })?;
                    Ok(SensorSpec {
                        sensor_id: s.sensor_id,
                        kind,
                        mount_height_m: s.mount_height_m,
                        min_range_m: s.min_range_m,
                        max_range_m: s.max_range_m,
                        ref_range_m: s.ref_detect_range_m,
                        ref_rcs_m2: s.ref_target_rcs_m2,
                        max_target_altitude_m: s.max_target_altitude_m,
                        clutter_db_per_sea_state: s.clutter_db_per_sea_state,
                        beamwidth_deg: s.beamwidth_deg,
                        active: s.is_active,
                    })
                })
                .collect::<Result<Vec<_>, ScenarioError>>()?;
            (
                db_domain,
                u.max_speed_mps.unwrap_or(p.max_speed_mps),
                p.cruise_speed_mps,
                u.accel_mps2.unwrap_or(p.accel_mps2),
                sensors,
                db.rcs_m2(pid)?,
                turn_performance(&p.category, p.length_m),
                radiated_noise_db(&p.category),
                Some(p.category.clone()),
                p.length_m,
                db.stores_of(pid)?.into_iter().collect(),
                db.damage_profile_of(pid)?
                    .map(|(dp, mk, fire, flood)| DamageProfile {
                        damage_points: dp,
                        mission_kill_fraction: mk,
                        fire_susceptibility: fire,
                        flooding_susceptibility: flood,
                    }),
            )
        }
        // `validate` guarantees these are present when there is no platform.
        None => (
            u.domain.ok_or_else(|| invalid("missing domain".into()))?,
            u.max_speed_mps
                .ok_or_else(|| invalid("missing max_speed_mps".into()))?,
            0.0,
            u.accel_mps2
                .ok_or_else(|| invalid("missing accel_mps2".into()))?,
            vec![],
            None,
            (None, None),
            None,
            None,
            None,
            BTreeMap::new(),
            None,
        ),
    };
    Ok(UnitSpawn {
        id: u.id,
        name: u.name.clone(),
        side: u.side,
        domain,
        position: u.position,
        altitude_m: u.altitude_m,
        heading_deg: u.heading_deg,
        speed_mps: u.speed_mps.unwrap_or(cruise).min(max_speed),
        max_speed_mps: max_speed,
        accel_mps2: accel,
        route: u.route.clone(),
        platform: u.platform.clone(),
        sensors,
        rcs_m2: rcs,
        max_turn_rate_dps: turn.0,
        min_turn_radius_m: turn.1,
        group: u.group.clone(),
        doctrine: u.doctrine,
        jammer: u.jammer,
        radiated_noise_db: noise,
        category,
        length_m: length,
        stores,
        damage_profile: damage,
        mission: u.mission.clone(),
    })
}

/// The weapon catalogue: every weapon carried by a unit in this scenario, from the database.
fn weapon_catalog(spawns: &[UnitSpawn], db: &PlatformDb) -> Result<Vec<WeaponSpec>, ScenarioError> {
    let ids: BTreeSet<&str> = spawns
        .iter()
        .flat_map(|s| s.stores.keys())
        .map(String::as_str)
        .collect();
    let mut out = Vec::new();
    for want in ids {
        let Some((id, name, kind, guidance, min, max, speed, warhead, ph, domains)) =
            db.weapon(want)?
        else {
            return Err(ScenarioError::Invalid(format!("unknown weapon {want}")));
        };
        let kind = WeaponKind::from_db(&kind).ok_or_else(|| {
            ScenarioError::Invalid(format!("weapon {id} has unknown kind {kind}"))
        })?;
        out.push(WeaponSpec {
            id,
            name,
            kind,
            guidance: Guidance::from_db(&guidance),
            min_range_m: min,
            max_range_m: max,
            speed_mps: speed,
            warhead_kg: warhead,
            base_ph: ph,
            target_domains: domains.split(',').map(|d| d.trim().to_owned()).collect(),
        });
    }
    Ok(out)
}

/// The ESM emitter library for the radars fitted in this scenario: each radar type with every
/// database platform class that carries it (not only the classes present in the scenario).
fn emitter_library(
    spawns: &[UnitSpawn],
    db: &PlatformDb,
) -> Result<Vec<EmitterEntry>, ScenarioError> {
    let radars: BTreeSet<&str> = spawns
        .iter()
        .flat_map(|s| s.sensors.iter())
        .filter(|s| s.kind == SensorKind::Radar)
        .map(|s| s.sensor_id.as_str())
        .collect();
    radars
        .into_iter()
        .map(|r| {
            let carriers = db
                .carriers_of(r)?
                .into_iter()
                .map(|(platform, category, domain)| Carrier {
                    platform,
                    category,
                    domain,
                })
                .collect();
            Ok(EmitterEntry {
                emitter: r.to_owned(),
                carriers,
            })
        })
        .collect()
}

/// Generic radiated noise at 10 kn by platform category, dB (the database has no acoustic data
/// yet; see docs/models/sonar.md). `None` for categories that are not waterborne, and for units
/// without a platform (the engine's domain default then applies).
fn radiated_noise_db(category: &str) -> Option<f64> {
    match category {
        "submarine" => Some(115.0),
        "surface_combatant" => Some(140.0),
        "auxiliary" => Some(145.0),
        "merchant" => Some(150.0),
        _ => None,
    }
}

/// Generic turn performance by platform class (the database has no turn data yet; see
/// docs/models/geodesy-and-movement.md §Turning). Returns (max rate deg/s, min radius m).
fn turn_performance(category: &str, length_m: Option<f64>) -> (Option<f64>, Option<f64>) {
    match category {
        // Ships: tactical diameter ≈ 5 hull lengths → radius 2.5 lengths, capped at 3°/s.
        "surface_combatant" | "auxiliary" | "merchant" | "submarine" => {
            (Some(3.0), Some(2.5 * length_m.unwrap_or(100.0)))
        }
        "fighter" => (Some(15.0), None),
        "helicopter" | "uav" => (Some(8.0), None),
        "mpa" | "aew" | "tanker" => (Some(3.0), None),
        // Land units and fixed sites: no turn limit.
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INLINE: &str = r#"{
        "format": "nws-scenario", "format_version": 1, "name": "t", "seed": 1, "duration_s": 10,
        "sides": [{"id": 1, "name": "Blue"}],
        "units": [{"id": 1, "name": "A", "side": 1, "domain": "surface",
                   "position": {"lat_deg": 0.0, "lon_deg": 0.0},
                   "max_speed_mps": 10.0, "accel_mps2": 0.1}]
    }"#;

    const WITH_PLATFORM: &str = r#"{
        "format": "nws-scenario", "format_version": 2, "name": "t", "seed": 1, "duration_s": 10,
        "sides": [{"id": 1, "name": "Blue"}],
        "units": [{"id": 1, "name": "F", "side": 1, "platform": "GEN-FFG",
                   "position": {"lat_deg": 0.0, "lon_deg": 0.0}}]
    }"#;

    fn db() -> PlatformDb {
        sim_db::embedded::open_platform_db().unwrap()
    }

    #[test]
    fn v1_inline_units_still_load() {
        let s = Scenario::from_json(INLINE).unwrap();
        let mut sim = s.build(None, &db()).unwrap();
        sim.step();
        let snap = sim.snapshot(sim_core::Viewer::WhiteCell);
        assert_eq!(snap.units.len(), 1);
        assert_eq!(snap.units[0].platform, None);
    }

    #[test]
    fn platform_supplies_performance_sensors_and_rcs() {
        let s = Scenario::from_json(WITH_PLATFORM).unwrap();
        let spawn = &s.resolve(&db()).unwrap()[0];
        assert_eq!(spawn.domain, Domain::Surface);
        assert!((spawn.max_speed_mps - 28.0 * 1852.0 / 3600.0).abs() < 1e-9);
        // Starts at the platform's cruise speed (16 kn).
        assert!((spawn.speed_mps - 16.0 * 1852.0 / 3600.0).abs() < 1e-9);
        assert_eq!(spawn.rcs_m2, Some(5000.0));
        assert!(spawn
            .sensors
            .iter()
            .any(|s| s.kind == SensorKind::Radar && s.sensor_id == "SEN-RDR-3D-MR"));
    }

    #[test]
    fn overrides_win_and_contradictions_are_rejected() {
        let over = WITH_PLATFORM.replace(
            r#""platform": "GEN-FFG","#,
            r#""platform": "GEN-FFG", "max_speed_mps": 5.0, "speed_mps": 9.0,"#,
        );
        let spawn = &Scenario::from_json(&over).unwrap().resolve(&db()).unwrap()[0];
        assert!((spawn.max_speed_mps - 5.0).abs() < 1e-12);
        assert!(
            (spawn.speed_mps - 5.0).abs() < 1e-12,
            "initial speed capped at max"
        );

        let wrong = WITH_PLATFORM.replace(
            r#""platform": "GEN-FFG","#,
            r#""platform": "GEN-FFG", "domain": "air","#,
        );
        assert!(matches!(
            Scenario::from_json(&wrong).unwrap().resolve(&db()),
            Err(ScenarioError::Invalid(_))
        ));
        let unknown = WITH_PLATFORM.replace("GEN-FFG", "GEN-NOPE");
        assert!(matches!(
            Scenario::from_json(&unknown).unwrap().resolve(&db()),
            Err(ScenarioError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_wrong_format_future_version_and_incomplete_units() {
        let wrong = INLINE.replace("nws-scenario", "other");
        assert!(matches!(
            Scenario::from_json(&wrong),
            Err(ScenarioError::WrongFormat(_))
        ));
        let future = INLINE.replace("\"format_version\": 1", "\"format_version\": 99");
        assert!(matches!(
            Scenario::from_json(&future),
            Err(ScenarioError::FutureVersion { .. })
        ));
        let no_speed = INLINE.replace("\"max_speed_mps\": 10.0, ", "");
        assert!(matches!(
            Scenario::from_json(&no_speed),
            Err(ScenarioError::Invalid(_))
        ));
        let bad_side = INLINE.replace("\"side\": 1", "\"side\": 2");
        assert!(matches!(
            Scenario::from_json(&bad_side),
            Err(ScenarioError::Invalid(_))
        ));
    }

    const WITH_C2: &str = r#"{
        "format": "nws-scenario", "format_version": 2, "name": "c2", "seed": 1, "duration_s": 10,
        "sides": [
            {"id": 1, "name": "Blue", "postures": [{"toward": 2, "posture": "hostile"}],
             "doctrine": {"wcs": "free", "emcon": "silent"}},
            {"id": 2, "name": "Red"}
        ],
        "groups": [{"side": 1, "name": "TG1", "doctrine": {"wcs": "safe"}}],
        "units": [{"id": 1, "name": "A", "side": 1, "domain": "surface", "group": "TG1",
                   "doctrine": {"emcon": "active"},
                   "position": {"lat_deg": 0.0, "lon_deg": 0.0},
                   "max_speed_mps": 10.0, "accel_mps2": 0.1}]
    }"#;

    #[test]
    fn postures_groups_and_doctrine_load_and_apply() {
        let s = Scenario::from_json(WITH_C2).unwrap();
        let mut sim = s.build(None, &db()).unwrap();
        sim.step();
        let snap = sim.snapshot(sim_core::Viewer::Side { side: SideId(1) });
        let d = snap.units[0].doctrine;
        assert_eq!(d.wcs, sim_core::Wcs::Safe, "group over side");
        assert_eq!(d.emcon, sim_core::Emcon::Active, "unit over side");
        assert_eq!(snap.postures.len(), 1);
        assert_eq!(snap.postures[0].posture, Posture::Hostile);
    }

    #[test]
    fn bad_postures_and_groups_are_rejected() {
        for (from, to) in [
            (r#""toward": 2"#, r#""toward": 1"#),
            (r#""toward": 2"#, r#""toward": 9"#),
            (r#""group": "TG1""#, r#""group": "TG9""#),
            (r#""side": 1, "name": "TG1""#, r#""side": 7, "name": "TG1""#),
        ] {
            let bad = WITH_C2.replacen(from, to, 1);
            assert!(
                matches!(Scenario::from_json(&bad), Err(ScenarioError::Invalid(_))),
                "{to}"
            );
        }
    }

    #[test]
    fn environment_and_jammers_load_and_are_validated() {
        let with = INLINE
            .replacen(
                r#""duration_s": 10,"#,
                r#""duration_s": 10, "environment": {"sea_state": 4, "weather_loss_db": 2.0},"#,
                1,
            )
            .replacen(
                r#""accel_mps2": 0.1}"#,
                r#""accel_mps2": 0.1, "jammer": {"jn_db_at_1km": 50.0}}"#,
                1,
            );
        let s = Scenario::from_json(&with).unwrap();
        assert_eq!(s.environment.sea_state, 4);
        let spawn = &s.resolve(&db()).unwrap()[0];
        assert_eq!(spawn.jammer.map(|j| j.jn_db_at_1km), Some(50.0));
        let mut sim = s.build(None, &db()).unwrap();
        sim.step();
        let env = sim.snapshot(sim_core::Viewer::WhiteCell).environment;
        assert_eq!((env.sea_state, env.weather_loss_db), (4, 2.0));
        let rough = with.replacen(r#""sea_state": 4"#, r#""sea_state": 11"#, 1);
        assert!(matches!(
            Scenario::from_json(&rough),
            Err(ScenarioError::Invalid(_))
        ));
    }
}
