//! Database validation suite (master prompt §9). Rule ids are stable, so CI logs and data
//! reviewers can refer to them. See `docs/db/data-dictionary.md` §Validation rules.

use crate::error::DbError;
use crate::{OPEN_CLASSIFICATION, PROVENANCE_TABLES};
use rusqlite::Connection;
use sim_geo::radar_horizon_m;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

/// Which build the database belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Open, unclassified build: every record must be marked UNCLASSIFIED.
    Open,
    /// Accredited build: classification markings other than UNCLASSIFIED are allowed.
    Accredited,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Issue {
    pub severity: Severity,
    pub rule: &'static str,
    /// `table/record`, e.g. `platforms/GEN-FFG`.
    pub location: String,
    pub message: String,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Error => "ERROR",
            Severity::Warning => "WARN ",
        };
        write!(
            f,
            "{sev} {:<28} {:<40} {}",
            self.rule, self.location, self.message
        )
    }
}

struct Collector(Vec<Issue>);

impl Collector {
    fn push(&mut self, severity: Severity, rule: &'static str, location: String, message: String) {
        self.0.push(Issue {
            severity,
            rule,
            location,
            message,
        });
    }
    fn err(&mut self, rule: &'static str, location: String, message: String) {
        self.push(Severity::Error, rule, location, message);
    }
}

/// Runs a query whose rows are (location, message) pairs and records an issue for each row.
fn rows(
    conn: &Connection,
    out: &mut Collector,
    severity: Severity,
    rule: &'static str,
    sql: &str,
) -> Result<(), DbError> {
    let mut stmt = conn.prepare(sql)?;
    let found = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    for (loc, msg) in found {
        out.push(severity, rule, loc, msg);
    }
    Ok(())
}

/// Validates the whole database. Issues come back sorted: errors first, then by rule and location.
pub fn validate(conn: &Connection, policy: Policy) -> Result<Vec<Issue>, DbError> {
    let mut out = Collector(Vec::new());
    provenance(conn, policy, &mut out)?;
    foreign_keys(conn, &mut out)?;
    sensors(conn, &mut out)?;
    weapons(conn, &mut out)?;
    capacity(conn, &mut out)?;
    platforms(conn, &mut out)?;
    let mut issues = out.0;
    issues.sort();
    Ok(issues)
}

fn key_column(table: &str) -> &'static str {
    match table {
        "platform_signatures" => "platform_id || ':' || kind || ':' || aspect || ':' || speed_band",
        "mounts" | "magazines" => "platform_id || '#' || name",
        _ => "id",
    }
}

fn provenance(conn: &Connection, policy: Policy, out: &mut Collector) -> Result<(), DbError> {
    for table in PROVENANCE_TABLES {
        let key = key_column(table);
        rows(conn, out, Severity::Error, "provenance.citation_missing",
            &format!("SELECT '{table}/' || {key}, 'source_citation is empty' FROM {table} WHERE trim(source_citation) = '' ORDER BY 1"))?;
        rows(conn, out, Severity::Error, "provenance.created_by_missing",
            &format!("SELECT '{table}/' || {key}, 'created_by is empty' FROM {table} WHERE trim(created_by) = '' ORDER BY 1"))?;
        rows(conn, out, Severity::Error, "provenance.updated_at_format",
            &format!("SELECT '{table}/' || {key}, 'updated_at must be an ISO date (YYYY-MM-DD), got ' || quote(updated_at) FROM {table} WHERE date(updated_at) IS NULL OR date(updated_at) != updated_at ORDER BY 1"))?;
        if policy == Policy::Open {
            rows(conn, out, Severity::Error, "classification.open_build",
                &format!("SELECT '{table}/' || {key}, 'classification ' || quote(classification) || ' is not allowed in the open build' FROM {table} WHERE classification != '{OPEN_CLASSIFICATION}' ORDER BY 1"))?;
            rows(conn, out, Severity::Warning, "provenance.not_generic",
                &format!("SELECT '{table}/' || {key}, 'record is not marked generic; open-build data should be generic classes' FROM {table} WHERE is_generic = 0 ORDER BY 1"))?;
        }
    }
    Ok(())
}

fn foreign_keys(conn: &Connection, out: &mut Collector) -> Result<(), DbError> {
    // pragma_foreign_key_check reports every row whose parent row is missing.
    let mut stmt = conn.prepare(
        "SELECT \"table\", rowid, parent, fkid FROM pragma_foreign_key_check ORDER BY 1, 2, 3",
    )?;
    let found = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (table, rowid, parent) in found {
        // Name the offending row by its own key where we can.
        let key = key_column(&table);
        let label: Option<String> = match rowid {
            Some(id) if PROVENANCE_TABLES.contains(&table.as_str()) => conn
                .query_row(
                    &format!("SELECT {key} FROM {table} WHERE rowid = ?1"),
                    [id],
                    |r| r.get(0),
                )
                .ok(),
            _ => None,
        };
        let loc = format!(
            "{table}/{}",
            label.unwrap_or_else(|| rowid.map_or_else(|| "?".into(), |r| format!("rowid {r}")))
        );
        out.err(
            "fk.dangling",
            loc,
            format!("references a missing row in `{parent}`"),
        );
    }
    Ok(())
}

/// Sensor kinds limited by the geometric line of sight (radar horizon).
const LOS_SENSOR_KINDS: &str = "'radar','esm','ir','eo','visual'";

fn sensors(conn: &Connection, out: &mut Collector) -> Result<(), DbError> {
    rows(conn, out, Severity::Error, "sensor.range_order",
        "SELECT 'sensors/' || id, 'min_range_m ' || min_range_m || ' must be >= 0 and < max_range_m ' || max_range_m FROM sensors WHERE min_range_m < 0 OR min_range_m >= max_range_m ORDER BY 1")?;
    rows(conn, out, Severity::Error, "sensor.reference_range",
        "SELECT 'sensors/' || id, 'ref_detect_range_m and ref_target_rcs_m2 must be given together, positive, with ref range <= max range' FROM sensors WHERE (ref_detect_range_m IS NULL) != (ref_target_rcs_m2 IS NULL) OR ref_detect_range_m <= 0 OR ref_target_rcs_m2 <= 0 OR ref_detect_range_m > max_range_m ORDER BY 1")?;
    rows(conn, out, Severity::Error, "sensor.radar_fields",
        "SELECT 'sensors/' || id, 'clutter_db_per_sea_state (>= 0) and beamwidth_deg (0 < b <= 360) apply to radars only' FROM sensors WHERE ((clutter_db_per_sea_state IS NOT NULL OR beamwidth_deg IS NOT NULL) AND kind != 'radar') OR clutter_db_per_sea_state < 0 OR beamwidth_deg <= 0 OR beamwidth_deg > 360 ORDER BY 1")?;
    rows(conn, out, Severity::Error, "sensor.sonar_fields",
        "SELECT 'sensors/' || id, 'sonar figures apply to sonars only, within frequency 0.01–100 kHz, DI 0–40 dB, DT -10–30 dB, source level 150–250 dB, self-noise 20–120 dB' FROM sensors WHERE ((sonar_frequency_khz IS NOT NULL OR sonar_di_db IS NOT NULL OR sonar_dt_db IS NOT NULL OR sonar_source_level_db IS NOT NULL OR sonar_self_noise_db IS NOT NULL) AND kind NOT IN ('sonar_hull','sonar_towed','sonar_dipping','sonobuoy')) OR sonar_frequency_khz NOT BETWEEN 0.01 AND 100 OR sonar_di_db NOT BETWEEN 0 AND 40 OR sonar_dt_db NOT BETWEEN -10 AND 30 OR sonar_source_level_db NOT BETWEEN 150 AND 250 OR sonar_self_noise_db NOT BETWEEN 20 AND 120 ORDER BY 1")?;
    rows(conn, out, Severity::Warning, "sensor.radar_defaults",
        "SELECT 'sensors/' || id, 'radar has no clutter_db_per_sea_state or beamwidth_deg; the model defaults apply' FROM sensors WHERE kind = 'radar' AND (clutter_db_per_sea_state IS NULL OR beamwidth_deg IS NULL) ORDER BY 1")?;
    rows(conn, out, Severity::Error, "sensor.radar_reference",
        "SELECT 'sensors/' || id, 'radar needs ref_detect_range_m and ref_target_rcs_m2 (Phase 2 detection model)' FROM sensors WHERE kind = 'radar' AND ref_detect_range_m IS NULL ORDER BY 1")?;

    // Horizon check: a line-of-sight sensor cannot see further than the radar horizon between its
    // mount and a target at the highest altitude it is designed to cover.
    let mut stmt = conn.prepare(&format!(
        "SELECT m.platform_id, m.name, s.id, s.max_range_m, s.max_target_altitude_m, m.height_m,
                p.domain, p.max_altitude_m
           FROM mounts m
           JOIN sensors s ON s.id = m.sensor_id
           JOIN platforms p ON p.id = m.platform_id
          WHERE s.kind IN ({LOS_SENSOR_KINDS})
          ORDER BY m.platform_id, m.id"
    ))?;
    let found = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<f64>>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (platform, mount, sensor, max_range, target_alt, mount_h, domain, ceiling) in found {
        // Airborne sensors are assessed at the platform's service ceiling (best case).
        let sensor_h = if domain == "air" {
            mount_h + ceiling.unwrap_or(0.0)
        } else {
            mount_h
        };
        let horizon = radar_horizon_m(sensor_h, target_alt);
        if max_range > horizon {
            out.err(
                "sensor.beyond_horizon",
                format!("mounts/{platform}#{mount}"),
                format!(
                    "{sensor} max range {:.0} km exceeds the {:.0} km horizon for a {:.0} m mount against targets at {:.0} m",
                    max_range / 1000.0,
                    horizon / 1000.0,
                    sensor_h,
                    target_alt
                ),
            );
        }
    }
    Ok(())
}

fn weapons(conn: &Connection, out: &mut Collector) -> Result<(), DbError> {
    rows(conn, out, Severity::Error, "weapon.range_order",
        "SELECT 'weapons/' || id, 'min_range_m ' || min_range_m || ' must be >= 0 and < max_range_m ' || max_range_m FROM weapons WHERE min_range_m < 0 OR min_range_m >= max_range_m ORDER BY 1")?;
    rows(conn, out, Severity::Error, "weapon.base_ph",
        "SELECT 'weapons/' || id, 'base_ph ' || base_ph || ' must be in [0, 1]' FROM weapons WHERE base_ph < 0 OR base_ph > 1 ORDER BY 1")?;
    rows(conn, out, Severity::Error, "weapon.values",
        "SELECT 'weapons/' || id, 'speed_mps and warhead_kg must be >= 0' FROM weapons WHERE speed_mps < 0 OR warhead_kg < 0 ORDER BY 1")?;

    // Target domains must be a non-empty subset of the known domains.
    let mut stmt = conn.prepare("SELECT id, target_domains FROM weapons ORDER BY id")?;
    let found = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, domains) in found {
        let bad: Vec<&str> = domains
            .split(',')
            .filter(|d| !["surface", "subsurface", "air", "land"].contains(d))
            .collect();
        if domains.is_empty() || !bad.is_empty() {
            out.err(
                "weapon.target_domains",
                format!("weapons/{id}"),
                format!("invalid target domains {domains:?}"),
            );
        }
    }

    // A weapon stowed in a ship's magazine needs a launcher on that platform that can fire it.
    rows(conn, out, Severity::Error, "weapon.no_launcher",
        "SELECT 'magazines/' || mg.platform_id || '#' || mg.name, 'holds ' || mc.weapon_id || ' but no launcher on ' || mg.platform_id || ' can fire it'
           FROM magazine_contents mc JOIN magazines mg ON mg.id = mc.magazine_id
          WHERE NOT EXISTS (
                SELECT 1 FROM launcher_weapons lw JOIN mounts m ON m.id = lw.mount_id
                 WHERE m.platform_id = mg.platform_id AND lw.weapon_id = mc.weapon_id)
          ORDER BY 1, 2")?;
    rows(
        conn,
        out,
        Severity::Warning,
        "weapon.unused",
        "SELECT 'weapons/' || w.id, 'not fitted to any launcher or loadout'
           FROM weapons w
          WHERE NOT EXISTS (SELECT 1 FROM launcher_weapons lw WHERE lw.weapon_id = w.id)
            AND NOT EXISTS (SELECT 1 FROM loadout_items li WHERE li.weapon_id = w.id)
          ORDER BY 1",
    )?;
    rows(
        conn,
        out,
        Severity::Warning,
        "launcher.empty",
        "SELECT 'mounts/' || m.platform_id || '#' || m.name, 'launcher lists no weapons'
           FROM mounts m WHERE m.kind = 'launcher'
            AND NOT EXISTS (SELECT 1 FROM launcher_weapons lw WHERE lw.mount_id = m.id)
          ORDER BY 1",
    )?;
    rows(conn, out, Severity::Warning, "launcher.no_ammunition",
        "SELECT 'mounts/' || m.platform_id || '#' || m.name, 'can fire ' || lw.weapon_id || ' but no magazine on the platform holds it'
           FROM mounts m JOIN launcher_weapons lw ON lw.mount_id = m.id
          WHERE NOT EXISTS (SELECT 1 FROM magazines mg JOIN magazine_contents mc ON mc.magazine_id = mg.id
                             WHERE mg.platform_id = m.platform_id AND mc.weapon_id = lw.weapon_id)
          ORDER BY 1, 2")?;
    Ok(())
}

fn capacity(conn: &Connection, out: &mut Collector) -> Result<(), DbError> {
    rows(conn, out, Severity::Error, "magazine.capacity",
        "SELECT 'magazines/' || platform_id || '#' || name, 'capacity must be > 0' FROM magazines WHERE capacity <= 0 ORDER BY 1")?;
    rows(conn, out, Severity::Error, "magazine.quantity",
        "SELECT 'magazines/' || mg.platform_id || '#' || mg.name, mc.weapon_id || ' quantity must be > 0'
           FROM magazine_contents mc JOIN magazines mg ON mg.id = mc.magazine_id WHERE mc.quantity <= 0 ORDER BY 1, 2")?;
    rows(conn, out, Severity::Error, "magazine.over_capacity",
        "SELECT 'magazines/' || mg.platform_id || '#' || mg.name, 'contents ' || sum(mc.quantity) || ' exceed capacity ' || mg.capacity
           FROM magazines mg JOIN magazine_contents mc ON mc.magazine_id = mg.id
          GROUP BY mg.id HAVING sum(mc.quantity) > mg.capacity ORDER BY 1")?;
    rows(conn, out, Severity::Error, "loadout.quantity",
        "SELECT 'loadouts/' || li.loadout_id, li.weapon_id || ' quantity must be > 0' FROM loadout_items li WHERE li.quantity <= 0 ORDER BY 1, 2")?;
    rows(conn, out, Severity::Error, "loadout.no_hardpoints",
        "SELECT 'loadouts/' || l.id, 'platform ' || p.id || ' has no hardpoints defined'
           FROM loadouts l JOIN platforms p ON p.id = l.platform_id WHERE p.hardpoints IS NULL ORDER BY 1")?;
    rows(conn, out, Severity::Error, "loadout.over_hardpoints",
        "SELECT 'loadouts/' || l.id, 'items ' || sum(li.quantity) || ' exceed ' || p.hardpoints || ' hardpoints on ' || p.id
           FROM loadouts l JOIN loadout_items li ON li.loadout_id = l.id JOIN platforms p ON p.id = l.platform_id
          WHERE p.hardpoints IS NOT NULL GROUP BY l.id HAVING sum(li.quantity) > p.hardpoints ORDER BY 1")?;
    Ok(())
}

fn expected_domain(category: &str) -> Option<&'static str> {
    Some(match category {
        "surface_combatant" | "auxiliary" | "merchant" => "surface",
        "submarine" => "subsurface",
        "mpa" | "aew" | "tanker" | "fighter" | "helicopter" | "uav" => "air",
        "shore_battery" | "sensor_site" | "airbase" | "port" => "land",
        _ => return None,
    })
}

fn platforms(conn: &Connection, out: &mut Collector) -> Result<(), DbError> {
    rows(conn, out, Severity::Error, "platform.speed",
        "SELECT 'platforms/' || id, 'speeds must satisfy 0 <= cruise <= max (max ' || max_speed_mps || ', cruise ' || cruise_speed_mps || ' m/s)' FROM platforms WHERE max_speed_mps < 0 OR cruise_speed_mps < 0 OR cruise_speed_mps > max_speed_mps ORDER BY 1")?;
    rows(conn, out, Severity::Error, "platform.accel",
        "SELECT 'platforms/' || id, 'a mobile platform needs accel_mps2 > 0' FROM platforms WHERE max_speed_mps > 0 AND accel_mps2 <= 0 ORDER BY 1")?;
    rows(conn, out, Severity::Error, "platform.dimensions",
        "SELECT 'platforms/' || id, 'length, displacement, draft, endurance and hardpoints must be positive when given' FROM platforms
          WHERE length_m <= 0 OR displacement_t <= 0 OR draft_m <= 0 OR endurance_m <= 0 OR hardpoints < 0 ORDER BY 1")?;
    rows(conn, out, Severity::Error, "platform.depth",
        "SELECT 'platforms/' || id, CASE WHEN domain = 'subsurface' THEN 'submarine needs max_depth_m > 0' ELSE 'only subsurface platforms may have max_depth_m' END
           FROM platforms WHERE (domain = 'subsurface' AND (max_depth_m IS NULL OR max_depth_m <= 0))
                             OR (domain != 'subsurface' AND max_depth_m IS NOT NULL) ORDER BY 1")?;
    rows(conn, out, Severity::Error, "platform.altitude",
        "SELECT 'platforms/' || id, CASE WHEN domain = 'air' THEN 'aircraft need max_altitude_m > 0' ELSE 'only air platforms may have max_altitude_m' END
           FROM platforms WHERE (domain = 'air' AND (max_altitude_m IS NULL OR max_altitude_m <= 0))
                             OR (domain != 'air' AND max_altitude_m IS NOT NULL) ORDER BY 1")?;
    rows(conn, out, Severity::Error, "damage.fractions",
        "SELECT 'damage_profiles/' || id, 'damage_points must be > 0 and fractions in [0, 1]' FROM damage_profiles
          WHERE damage_points <= 0 OR mission_kill_fraction NOT BETWEEN 0 AND 1
             OR fire_susceptibility NOT BETWEEN 0 AND 1 OR flooding_susceptibility NOT BETWEEN 0 AND 1 ORDER BY 1")?;
    rows(conn, out, Severity::Error, "signature.value",
        "SELECT 'platform_signatures/' || platform_id || ':' || kind, 'signature value must be >= 0' FROM platform_signatures WHERE value < 0 ORDER BY 1")?;
    rows(conn, out, Severity::Warning, "platform.no_rcs",
        "SELECT 'platforms/' || p.id, 'no radar cross-section signature (radar detection will fall back to a default)'
           FROM platforms p WHERE p.domain IN ('surface','air','subsurface')
            AND NOT EXISTS (SELECT 1 FROM platform_signatures s WHERE s.platform_id = p.id AND s.kind = 'rcs') ORDER BY 1")?;
    rows(conn, out, Severity::Warning, "platform.no_acoustic",
        "SELECT 'platforms/' || p.id, 'no acoustic signature (sonar detection will fall back to a default)'
           FROM platforms p WHERE p.domain IN ('surface','subsurface')
            AND NOT EXISTS (SELECT 1 FROM platform_signatures s WHERE s.platform_id = p.id AND s.kind = 'acoustic') ORDER BY 1")?;

    let mut stmt = conn.prepare("SELECT id, category, domain FROM platforms ORDER BY id")?;
    let found = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, category, domain) in found {
        if let Some(expected) = expected_domain(&category) {
            if expected != domain {
                out.err(
                    "platform.category_domain",
                    format!("platforms/{id}"),
                    format!("category {category} must be in domain {expected}, not {domain}"),
                );
            }
        }
    }
    Ok(())
}

/// Count of issues by severity.
#[must_use]
pub fn counts(issues: &[Issue]) -> BTreeMap<Severity, usize> {
    let mut m = BTreeMap::new();
    for i in issues {
        *m.entry(i.severity).or_insert(0) += 1;
    }
    m
}
