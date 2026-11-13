//! Acoustic state of a run (Phase 4 slice 8, `docs/models/acoustics.md`): sound-velocity
//! profiles, towed-array streaming and laid sonobuoys.
//!
//! Without profiles, towed-array orders or sonobuoys, the resource is empty, is not hashed, and
//! sonar works exactly as before (the Phase 2 transmission-loss law).

use crate::components::{Domain, Kinematics, Position, Route, Unit};
use crate::detection::{SensorKind, SensorSuite};
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::sim::UnitIndex;
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_events::SoundProfile;
use sim_geo::GeoPoint;
use std::collections::BTreeMap;

/// Streaming or recovering a towed array takes this long, s.
pub const TOW_TRANSITION_S: u64 = 600;
/// A streamed towed array is usable at or below this speed, m/s (15 kn).
pub const TOW_MAX_SPEED_MPS: f64 = 15.0 * 1852.0 / 3600.0;
/// Depth of a streamed towed array below a surface ship, m (submarines: their depth + 50 m).
pub const TOWED_DEPTH_M: f64 = 150.0;
/// Hull sonar depth of a surface ship, m (about the keel).
pub const HULL_DEPTH_M: f64 = 6.0;
/// Depth of a surface ship as a noise source, m.
pub const SHIP_SOURCE_DEPTH_M: f64 = 5.0;
/// Dipping sonar body depth, m.
pub const DIPPING_DEPTH_M: f64 = 100.0;
/// Sonobuoy hydrophone depths, m: shallow and deep settings.
pub const BUOY_SHALLOW_M: f64 = 60.0;
pub const BUOY_DEEP_M: f64 = 300.0;
/// A sonobuoy's life, s.
pub const BUOY_LIFE_S: u64 = 4 * 3600;
/// Sonobuoys carried per sonobuoy sensor when none are recorded.
pub const BUOY_LOAD: u32 = 16;
/// A drop point is reached within this distance, m.
pub const DROP_REACH_M: f64 = 1_000.0;
/// Synthetic observer ids for buoys start here (bearings are grouped per observer).
pub const BUOY_OBSERVER_BASE: u64 = 1 << 40;

/// A towed array's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TowedState {
    pub streamed: bool,
    /// When the streaming or recovery under way is complete.
    pub ready_at: Tick,
}

/// A laid sonobuoy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Sonobuoy {
    pub id: u32,
    pub side: SideId,
    /// The unit that laid it (its side's contacts are fed through it).
    pub owner: UnitId,
    pub sensor_id: String,
    pub position: GeoPoint,
    pub depth_m: f64,
    pub max_range_m: f64,
    pub laid: Tick,
    pub expires: Tick,
}

/// Drops still to make along an ordered pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingDrops {
    pub points: Vec<GeoPoint>,
    pub deep: bool,
}

/// Profiles, towed arrays and sonobuoys.
#[derive(Debug, Clone, Default, PartialEq, Resource, Serialize, Deserialize)]
pub struct Acoustics {
    #[serde(default)]
    pub profiles: Vec<SoundProfile>,
    /// Towed arrays ordered in or out; a unit not here has its array streamed and ready.
    #[serde(default)]
    pub towed: BTreeMap<UnitId, TowedState>,
    #[serde(default)]
    pub buoys: Vec<Sonobuoy>,
    #[serde(default)]
    pub pending: BTreeMap<UnitId, PendingDrops>,
    /// Sonobuoys left aboard, once a unit has started laying.
    #[serde(default)]
    pub loads: BTreeMap<UnitId, u32>,
    #[serde(default)]
    pub next_buoy: u32,
}

impl Acoustics {
    /// Whether a unit's towed array can listen now.
    #[must_use]
    pub fn towed_usable(&self, unit: UnitId, speed_mps: f64, tick: Tick) -> bool {
        let ready = self
            .towed
            .get(&unit)
            .is_none_or(|t| t.streamed && tick >= t.ready_at);
        ready && speed_mps <= TOW_MAX_SPEED_MPS
    }
}

/// Depth of a sensor below the sea surface, m.
#[must_use]
pub fn sensor_depth_m(kind: SensorKind, domain: Domain, altitude_m: f64) -> f64 {
    let own = if domain == Domain::Subsurface {
        (-altitude_m).max(5.0)
    } else {
        HULL_DEPTH_M
    };
    match kind {
        SensorKind::SonarTowed if domain == Domain::Subsurface => own + 50.0,
        SensorKind::SonarTowed => TOWED_DEPTH_M,
        SensorKind::SonarDipping => DIPPING_DEPTH_M,
        _ => own,
    }
}

/// Depth of a unit as a noise source or echo, m.
#[must_use]
pub fn source_depth_m(domain: Domain, altitude_m: f64) -> f64 {
    if domain == Domain::Subsurface {
        (-altitude_m).max(5.0)
    } else {
        SHIP_SOURCE_DEPTH_M
    }
}

/// Validated, sorted profiles for `SetSoundProfiles`.
pub(crate) fn set_profiles(world: &mut World, profiles: &[SoundProfile]) -> Result<(), String> {
    sim_events::validate_profiles(profiles)?;
    let mut p = profiles.to_vec();
    p.sort_by_key(|s| s.id);
    world.resource_mut::<Acoustics>().profiles = p;
    Ok(())
}

/// A towed-array order: streaming or recovery starts now and takes [`TOW_TRANSITION_S`].
pub(crate) fn order_towed(
    world: &mut World,
    e: Entity,
    unit: UnitId,
    streamed: bool,
) -> Result<(), String> {
    let has = world
        .get::<SensorSuite>(e)
        .is_some_and(|s| s.0.iter().any(|x| x.kind == SensorKind::SonarTowed));
    if !has {
        return Err(format!("{unit} has no towed array"));
    }
    let tick = world.resource::<SimClock>().tick;
    let mut ac = world.resource_mut::<Acoustics>();
    let now = ac.towed.get(&unit).copied().unwrap_or(TowedState {
        streamed: true,
        ready_at: Tick(0),
    });
    if now.streamed == streamed {
        return Ok(());
    }
    ac.towed.insert(
        unit,
        TowedState {
            streamed,
            ready_at: Tick(tick.0 + TOW_TRANSITION_S),
        },
    );
    world.resource_mut::<EventLog>().push(
        tick,
        SimEventKind::TowedArray {
            unit,
            streamed,
            ready_s: TOW_TRANSITION_S,
        },
    );
    Ok(())
}

/// A sonobuoy-laying order: the unit flies (or steams) through the points and drops one buoy at
/// each.
pub(crate) fn order_buoys(
    world: &mut World,
    e: Entity,
    unit: UnitId,
    points: &[GeoPoint],
    deep: bool,
) -> Result<(), String> {
    if points.is_empty() || points.len() > 64 {
        return Err("give 1 to 64 drop points".into());
    }
    if !points.iter().all(GeoPoint::is_valid) {
        return Err("a drop point has invalid coordinates".into());
    }
    let has = world
        .get::<SensorSuite>(e)
        .is_some_and(|s| s.0.iter().any(|x| x.kind == SensorKind::Sonobuoy));
    if !has {
        return Err(format!("{unit} carries no sonobuoys"));
    }
    let left = world
        .resource::<Acoustics>()
        .loads
        .get(&unit)
        .copied()
        .unwrap_or(BUOY_LOAD);
    if left == 0 {
        return Err(format!("{unit} has no sonobuoys left"));
    }
    if let Some(mut r) = world.get_mut::<Route>(e) {
        r.waypoints = points.to_vec();
        r.next = 0;
        r.charted = false;
    }
    if let Some(mut k) = world.get_mut::<Kinematics>(e) {
        if k.ordered_speed_mps == 0.0 {
            k.ordered_speed_mps = 0.7 * k.max_speed_mps;
        }
    }
    world.resource_mut::<Acoustics>().pending.insert(
        unit,
        PendingDrops {
            points: points.to_vec(),
            deep,
        },
    );
    Ok(())
}

/// Each tick: drops buoys where layers reach their next point, and lets old buoys die.
pub(crate) fn sonobuoy_system(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    let ac = world.resource::<Acoustics>();
    if ac.pending.is_empty() && ac.buoys.is_empty() {
        return;
    }
    // Expiry.
    let expired: Vec<u32> = ac
        .buoys
        .iter()
        .filter(|b| tick >= b.expires)
        .map(|b| b.id)
        .collect();
    if !expired.is_empty() {
        world
            .resource_mut::<Acoustics>()
            .buoys
            .retain(|b| tick < b.expires);
        let mut log = world.resource_mut::<EventLog>();
        for id in expired {
            log.push(tick, SimEventKind::SonobuoyExpired { buoy: id });
        }
    }
    // Drops.
    let pending: Vec<(UnitId, PendingDrops)> = world
        .resource::<Acoustics>()
        .pending
        .iter()
        .map(|(u, p)| (*u, p.clone()))
        .collect();
    for (unit, mut drops) in pending {
        let Some(&e) = world.resource::<UnitIndex>().0.get(&unit) else {
            world.resource_mut::<Acoustics>().pending.remove(&unit);
            continue;
        };
        let (Some(u), Some(p)) = (world.get::<Unit>(e), world.get::<Position>(e)) else {
            continue;
        };
        let (side, at) = (u.side, p.point);
        let spec = world.get::<SensorSuite>(e).and_then(|s| {
            s.0.iter()
                .find(|x| x.kind == SensorKind::Sonobuoy)
                .map(|x| (x.sensor_id.clone(), x.max_range_m))
        });
        let Some((sensor_id, max_range_m)) = spec else {
            world.resource_mut::<Acoustics>().pending.remove(&unit);
            continue;
        };
        let Some(&next) = drops.points.first() else {
            world.resource_mut::<Acoustics>().pending.remove(&unit);
            continue;
        };
        if sim_geo::inverse(at, next).distance_m > DROP_REACH_M {
            continue;
        }
        let mut ac = world.resource_mut::<Acoustics>();
        let left = ac.loads.get(&unit).copied().unwrap_or(BUOY_LOAD);
        drops.points.remove(0);
        if left == 0 {
            ac.pending.remove(&unit);
            continue;
        }
        let id = ac.next_buoy;
        ac.next_buoy += 1;
        ac.loads.insert(unit, left - 1);
        ac.buoys.push(Sonobuoy {
            id,
            side,
            owner: unit,
            sensor_id,
            position: at,
            depth_m: if drops.deep {
                BUOY_DEEP_M
            } else {
                BUOY_SHALLOW_M
            },
            max_range_m,
            laid: tick,
            expires: Tick(tick.0 + BUOY_LIFE_S),
        });
        if drops.points.is_empty() || left == 1 {
            ac.pending.remove(&unit);
        } else {
            ac.pending.insert(unit, drops);
        }
        world.resource_mut::<EventLog>().push(
            tick,
            SimEventKind::SonobuoyLaid {
                unit,
                buoy: id,
                position: at,
            },
        );
    }
}

/// A towed array, as a seat sees it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TowedView {
    /// Streamed (or streaming); else recovered (or recovering).
    pub streamed: bool,
    /// Seconds until the ordered state is reached (0: there).
    pub ready_in_s: f64,
}

impl Acoustics {
    /// The towed-array view of `unit` at `tick` (streamed and ready unless ordered otherwise).
    #[must_use]
    pub fn towed_view(&self, unit: UnitId, tick: Tick) -> TowedView {
        self.towed.get(&unit).map_or(
            TowedView {
                streamed: true,
                ready_in_s: 0.0,
            },
            |t| TowedView {
                streamed: t.streamed,
                #[allow(clippy::cast_precision_loss)]
                ready_in_s: t.ready_at.0.saturating_sub(tick.0) as f64,
            },
        )
    }
}

/// A laid sonobuoy, as a seat sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SonobuoyView {
    pub id: u32,
    pub side: SideId,
    pub position: GeoPoint,
    pub depth_m: f64,
    /// Seconds of life left.
    pub life_s: f64,
}

/// The sonobuoys `side` laid (all for the white cell, `None`).
#[must_use]
pub fn buoy_views(ac: &Acoustics, side: Option<SideId>, tick: Tick) -> Vec<SonobuoyView> {
    ac.buoys
        .iter()
        .filter(|b| side.is_none_or(|s| s == b.side))
        .map(|b| SonobuoyView {
            id: b.id,
            side: b.side,
            position: b.position,
            depth_m: b.depth_m,
            #[allow(clippy::cast_precision_loss)]
            life_s: b.expires.0.saturating_sub(tick.0) as f64,
        })
        .collect()
}
