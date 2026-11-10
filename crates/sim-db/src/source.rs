//! JSON source format: the diffable, reviewable source of truth for the database (ADR-0004).
//!
//! Layout of a source directory:
//!
//! ```text
//! reference.json      nations, services, propulsion, damage_profiles, comms
//! sensors.json        sensors
//! weapons.json        weapons
//! platforms/*.json    platforms (with signatures, sensors, launchers, magazines, loadouts, comms)
//! ```
//!
//! Every file has a `defaults` provenance block, which any record may override with its own
//! `provenance`. Authoring uses naval units (kn, nm, km); the database stores SI.
//!
//! The builder does **not** enforce foreign keys. Dangling references are reported, all at once and
//! with context, by [`crate::validate`].

use crate::error::DbError;
use crate::migrate::migrate;
use rusqlite::{params, Connection, Transaction};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MPS_PER_KNOT: f64 = 1852.0 / 3600.0;
const M_PER_NM: f64 = 1852.0;
const M_PER_KM: f64 = 1000.0;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceSpec {
    pub source_citation: Option<String>,
    pub confidence: Option<String>,
    pub classification: Option<String>,
    pub is_generic: Option<bool>,
    pub created_by: Option<String>,
    pub updated_at: Option<String>,
}

/// Fully resolved provenance written to every value row.
#[derive(Debug, Clone)]
struct Provenance {
    source_citation: String,
    confidence: String,
    classification: String,
    is_generic: bool,
    created_by: String,
    updated_at: String,
}

impl Provenance {
    fn resolve(
        defaults: &ProvenanceSpec,
        over: Option<&ProvenanceSpec>,
        ctx: &str,
    ) -> Result<Self, DbError> {
        let empty = ProvenanceSpec::default();
        let o = over.unwrap_or(&empty);
        let pick = |a: &Option<String>, b: &Option<String>| a.clone().or_else(|| b.clone());
        let missing = |field: &str| DbError::Source {
            context: ctx.to_owned(),
            message: format!("provenance field `{field}` has no value and no file default"),
        };
        Ok(Self {
            // A missing citation is allowed here so that validation can report every one of them.
            source_citation: pick(&o.source_citation, &defaults.source_citation)
                .unwrap_or_default(),
            confidence: pick(&o.confidence, &defaults.confidence)
                .ok_or_else(|| missing("confidence"))?,
            classification: pick(&o.classification, &defaults.classification)
                .ok_or_else(|| missing("classification"))?,
            is_generic: o
                .is_generic
                .or(defaults.is_generic)
                .ok_or_else(|| missing("is_generic"))?,
            created_by: pick(&o.created_by, &defaults.created_by)
                .ok_or_else(|| missing("created_by"))?,
            updated_at: pick(&o.updated_at, &defaults.updated_at)
                .ok_or_else(|| missing("updated_at"))?,
        })
    }
}

// ---------- reference.json ----------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceFile {
    defaults: ProvenanceSpec,
    nations: Vec<Nation>,
    services: Vec<Service>,
    propulsion: Vec<Propulsion>,
    damage_profiles: Vec<DamageProfile>,
    comms: Vec<Comms>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Nation {
    id: String,
    name: String,
    provenance: Option<ProvenanceSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Service {
    id: String,
    nation: String,
    name: String,
    branch: String,
    provenance: Option<ProvenanceSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Propulsion {
    id: String,
    name: String,
    kind: String,
    fuel_type: String,
    provenance: Option<ProvenanceSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DamageProfile {
    id: String,
    name: String,
    damage_points: f64,
    mission_kill_fraction: f64,
    fire_susceptibility: f64,
    flooding_susceptibility: f64,
    provenance: Option<ProvenanceSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Comms {
    id: String,
    name: String,
    kind: String,
    range_km: f64,
    is_datalink: bool,
    provenance: Option<ProvenanceSpec>,
}

// ---------- sensors.json / weapons.json ----------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SensorsFile {
    defaults: ProvenanceSpec,
    sensors: Vec<Sensor>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sensor {
    id: String,
    name: String,
    kind: String,
    role: String,
    band: Option<String>,
    active: bool,
    #[serde(default)]
    min_range_km: f64,
    max_range_km: f64,
    ref_detect_range_km: Option<f64>,
    ref_target_rcs_m2: Option<f64>,
    max_target_altitude_m: f64,
    scan_period_s: Option<f64>,
    provenance: Option<ProvenanceSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WeaponsFile {
    defaults: ProvenanceSpec,
    weapons: Vec<Weapon>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Weapon {
    id: String,
    name: String,
    kind: String,
    guidance: String,
    #[serde(default)]
    min_range_km: f64,
    max_range_km: f64,
    speed_mps: f64,
    warhead_kg: f64,
    base_ph: f64,
    targets: Vec<String>,
    provenance: Option<ProvenanceSpec>,
}

// ---------- platforms/*.json ----------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlatformsFile {
    defaults: ProvenanceSpec,
    platforms: Vec<Platform>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Platform {
    id: String,
    name: String,
    category: String,
    domain: String,
    service: String,
    propulsion: String,
    damage_profile: String,
    length_m: Option<f64>,
    displacement_t: Option<f64>,
    draft_m: Option<f64>,
    max_speed_kn: f64,
    cruise_speed_kn: f64,
    accel_mps2: f64,
    endurance_nm: Option<f64>,
    max_altitude_m: Option<f64>,
    max_depth_m: Option<f64>,
    hardpoints: Option<i64>,
    #[serde(default)]
    signatures: Vec<Signature>,
    #[serde(default)]
    sensors: Vec<SensorMount>,
    #[serde(default)]
    launchers: Vec<Launcher>,
    #[serde(default)]
    magazines: Vec<Magazine>,
    #[serde(default)]
    loadouts: Vec<Loadout>,
    #[serde(default)]
    comms: Vec<String>,
    provenance: Option<ProvenanceSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Signature {
    kind: String,
    #[serde(default = "all")]
    aspect: String,
    #[serde(default = "any")]
    speed_band: String,
    value: f64,
    unit: String,
}

fn all() -> String {
    "all".into()
}
fn any() -> String {
    "any".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SensorMount {
    sensor: String,
    name: Option<String>,
    height_m: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Launcher {
    name: String,
    height_m: f64,
    weapons: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Magazine {
    name: String,
    capacity: i64,
    contents: BTreeMap<String, i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Loadout {
    id: String,
    name: String,
    role: String,
    items: BTreeMap<String, i64>,
}

// ---------- building ----------

/// Row counts after a build.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildStats {
    pub platforms: usize,
    pub sensors: usize,
    pub weapons: usize,
    pub files: usize,
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, DbError> {
    let text = std::fs::read_to_string(path).map_err(|source| DbError::Io {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| DbError::Json {
        path: path.to_owned(),
        source,
    })
}

/// Platform files in a stable (sorted) order, so row ids are reproducible.
fn platform_files(dir: &Path) -> Result<Vec<PathBuf>, DbError> {
    let pdir = dir.join("platforms");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&pdir)
        .map_err(|source| DbError::Io {
            path: pdir.clone(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    Ok(files)
}

/// Creates the schema on `conn` (which must be empty) and loads every source file into it.
pub fn build(conn: &mut Connection, dir: &Path) -> Result<BuildStats, DbError> {
    migrate(conn)?;
    // Referential integrity is checked by validation, which reports every problem with context.
    conn.pragma_update(None, "foreign_keys", false)?;
    let tx = conn.transaction()?;
    let mut stats = BuildStats::default();

    let reference: ReferenceFile = read_json(&dir.join("reference.json"))?;
    insert_reference(&tx, &reference)?;
    let sensors: SensorsFile = read_json(&dir.join("sensors.json"))?;
    stats.sensors = insert_sensors(&tx, &sensors)?;
    let weapons: WeaponsFile = read_json(&dir.join("weapons.json"))?;
    stats.weapons = insert_weapons(&tx, &weapons)?;
    stats.files = 3;
    for path in platform_files(dir)? {
        let file: PlatformsFile = read_json(&path)?;
        for p in &file.platforms {
            insert_platform(&tx, &file.defaults, p).map_err(|e| match e {
                DbError::Sqlite(err) => DbError::Source {
                    context: format!("{} / platform {}", path.display(), p.id),
                    message: err.to_string(),
                },
                other => other,
            })?;
            stats.platforms += 1;
        }
        stats.files += 1;
    }
    tx.commit()?;
    Ok(stats)
}

macro_rules! prov_params {
    ($p:expr) => {
        (
            &$p.source_citation,
            &$p.confidence,
            &$p.classification,
            $p.is_generic,
            &$p.created_by,
            &$p.updated_at,
        )
    };
}

const PROV_COLS: &str =
    "source_citation, confidence, classification, is_generic, created_by, updated_at";

fn insert_reference(tx: &Transaction<'_>, f: &ReferenceFile) -> Result<(), DbError> {
    for n in &f.nations {
        let p = Provenance::resolve(
            &f.defaults,
            n.provenance.as_ref(),
            &format!("nation {}", n.id),
        )?;
        let (c, conf, cl, g, by, at) = prov_params!(p);
        tx.execute(
            &format!(
                "INSERT INTO nations (id, name, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)"
            ),
            params![n.id, n.name, c, conf, cl, g, by, at],
        )?;
    }
    for s in &f.services {
        let p = Provenance::resolve(
            &f.defaults,
            s.provenance.as_ref(),
            &format!("service {}", s.id),
        )?;
        let (c, conf, cl, g, by, at) = prov_params!(p);
        tx.execute(
            &format!("INSERT INTO services (id, nation_id, name, branch, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"),
            params![s.id, s.nation, s.name, s.branch, c, conf, cl, g, by, at],
        )?;
    }
    for x in &f.propulsion {
        let p = Provenance::resolve(
            &f.defaults,
            x.provenance.as_ref(),
            &format!("propulsion {}", x.id),
        )?;
        let (c, conf, cl, g, by, at) = prov_params!(p);
        tx.execute(
            &format!("INSERT INTO propulsion (id, name, kind, fuel_type, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"),
            params![x.id, x.name, x.kind, x.fuel_type, c, conf, cl, g, by, at],
        )?;
    }
    for d in &f.damage_profiles {
        let p = Provenance::resolve(
            &f.defaults,
            d.provenance.as_ref(),
            &format!("damage profile {}", d.id),
        )?;
        let (c, conf, cl, g, by, at) = prov_params!(p);
        tx.execute(
            &format!("INSERT INTO damage_profiles (id, name, damage_points, mission_kill_fraction, fire_susceptibility, flooding_susceptibility, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"),
            params![d.id, d.name, d.damage_points, d.mission_kill_fraction, d.fire_susceptibility, d.flooding_susceptibility, c, conf, cl, g, by, at],
        )?;
    }
    for m in &f.comms {
        let p = Provenance::resolve(
            &f.defaults,
            m.provenance.as_ref(),
            &format!("comms {}", m.id),
        )?;
        let (c, conf, cl, g, by, at) = prov_params!(p);
        tx.execute(
            &format!("INSERT INTO comms_systems (id, name, kind, range_m, is_datalink, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)"),
            params![m.id, m.name, m.kind, m.range_km * M_PER_KM, m.is_datalink, c, conf, cl, g, by, at],
        )?;
    }
    Ok(())
}

fn insert_sensors(tx: &Transaction<'_>, f: &SensorsFile) -> Result<usize, DbError> {
    for s in &f.sensors {
        let p = Provenance::resolve(
            &f.defaults,
            s.provenance.as_ref(),
            &format!("sensor {}", s.id),
        )?;
        let (c, conf, cl, g, by, at) = prov_params!(p);
        tx.execute(
            &format!("INSERT INTO sensors (id, name, kind, role, band, is_active, min_range_m, max_range_m, ref_detect_range_m, ref_target_rcs_m2, max_target_altitude_m, scan_period_s, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)"),
            params![
                s.id, s.name, s.kind, s.role, s.band, s.active,
                s.min_range_km * M_PER_KM, s.max_range_km * M_PER_KM,
                s.ref_detect_range_km.map(|r| r * M_PER_KM), s.ref_target_rcs_m2,
                s.max_target_altitude_m, s.scan_period_s,
                c, conf, cl, g, by, at
            ],
        )?;
    }
    Ok(f.sensors.len())
}

fn insert_weapons(tx: &Transaction<'_>, f: &WeaponsFile) -> Result<usize, DbError> {
    for w in &f.weapons {
        let p = Provenance::resolve(
            &f.defaults,
            w.provenance.as_ref(),
            &format!("weapon {}", w.id),
        )?;
        let (c, conf, cl, g, by, at) = prov_params!(p);
        tx.execute(
            &format!("INSERT INTO weapons (id, name, kind, guidance, min_range_m, max_range_m, speed_mps, warhead_kg, base_ph, target_domains, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)"),
            params![
                w.id, w.name, w.kind, w.guidance,
                w.min_range_km * M_PER_KM, w.max_range_km * M_PER_KM,
                w.speed_mps, w.warhead_kg, w.base_ph, w.targets.join(","),
                c, conf, cl, g, by, at
            ],
        )?;
    }
    Ok(f.weapons.len())
}

fn insert_platform(
    tx: &Transaction<'_>,
    defaults: &ProvenanceSpec,
    pl: &Platform,
) -> Result<(), DbError> {
    let p = Provenance::resolve(
        defaults,
        pl.provenance.as_ref(),
        &format!("platform {}", pl.id),
    )?;
    let (c, conf, cl, g, by, at) = prov_params!(p);
    tx.execute(
        &format!("INSERT INTO platforms (id, name, category, domain, service_id, propulsion_id, damage_profile_id, length_m, displacement_t, draft_m, max_speed_mps, cruise_speed_mps, accel_mps2, endurance_m, max_altitude_m, max_depth_m, hardpoints, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)"),
        params![
            pl.id, pl.name, pl.category, pl.domain, pl.service, pl.propulsion, pl.damage_profile,
            pl.length_m, pl.displacement_t, pl.draft_m,
            pl.max_speed_kn * MPS_PER_KNOT, pl.cruise_speed_kn * MPS_PER_KNOT, pl.accel_mps2,
            pl.endurance_nm.map(|e| e * M_PER_NM), pl.max_altitude_m, pl.max_depth_m, pl.hardpoints,
            c, conf, cl, g, by, at
        ],
    )?;
    for s in &pl.signatures {
        tx.execute(
            &format!("INSERT INTO platform_signatures (platform_id, kind, aspect, speed_band, value, unit, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"),
            params![pl.id, s.kind, s.aspect, s.speed_band, s.value, s.unit, c, conf, cl, g, by, at],
        )?;
    }
    for m in &pl.sensors {
        let name = m.name.clone().unwrap_or_else(|| m.sensor.clone());
        tx.execute(
            &format!("INSERT INTO mounts (platform_id, name, kind, sensor_id, height_m, {PROV_COLS}) VALUES (?1,?2,'sensor',?3,?4,?5,?6,?7,?8,?9,?10)"),
            params![pl.id, name, m.sensor, m.height_m, c, conf, cl, g, by, at],
        )?;
    }
    for l in &pl.launchers {
        tx.execute(
            &format!("INSERT INTO mounts (platform_id, name, kind, sensor_id, height_m, {PROV_COLS}) VALUES (?1,?2,'launcher',NULL,?3,?4,?5,?6,?7,?8,?9)"),
            params![pl.id, l.name, l.height_m, c, conf, cl, g, by, at],
        )?;
        let mount_id = tx.last_insert_rowid();
        for w in &l.weapons {
            tx.execute(
                "INSERT INTO launcher_weapons (mount_id, weapon_id) VALUES (?1, ?2)",
                params![mount_id, w],
            )?;
        }
    }
    for m in &pl.magazines {
        tx.execute(
            &format!("INSERT INTO magazines (platform_id, name, capacity, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"),
            params![pl.id, m.name, m.capacity, c, conf, cl, g, by, at],
        )?;
        let mag_id = tx.last_insert_rowid();
        for (w, q) in &m.contents {
            tx.execute(
                "INSERT INTO magazine_contents (magazine_id, weapon_id, quantity) VALUES (?1, ?2, ?3)",
                params![mag_id, w, q],
            )?;
        }
    }
    for l in &pl.loadouts {
        tx.execute(
            &format!("INSERT INTO loadouts (id, platform_id, name, role, {PROV_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"),
            params![l.id, pl.id, l.name, l.role, c, conf, cl, g, by, at],
        )?;
        for (w, q) in &l.items {
            tx.execute(
                "INSERT INTO loadout_items (loadout_id, weapon_id, quantity) VALUES (?1, ?2, ?3)",
                params![l.id, w, q],
            )?;
        }
    }
    for cm in &pl.comms {
        tx.execute(
            "INSERT INTO platform_comms (platform_id, comms_id) VALUES (?1, ?2)",
            params![pl.id, cm],
        )?;
    }
    Ok(())
}
