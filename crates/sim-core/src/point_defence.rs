//! Point defence and soft kill (Phase 2 slice 9). The rules are in `sim-weapons` (ADR-0005);
//! model doc `docs/models/weapons-damage.md` §Point defence.
//!
//! Every tick, before weapons move:
//! 1. Interceptor shots due this tick resolve with a `Weapons` Ph draw; a kill removes the
//!    threat.
//! 2. Each operational, radiating defender (unit-id order) detects enemy missiles inside its
//!    radar horizon and R₅₀ against a 0.1 m² missile, and after the weapon's reaction time
//!    engages them (threat-id order) with SAMs (area defence), guns and CIWS (self-defence only),
//!    one shot per free fire channel. Saturation emerges: channels stay busy until intercept.
//! 3. A ship under attack fires a matched decoy once per threat when it closes inside 10 km
//!    (3 km for torpedoes); the decoy's effect lowers the threat's Ph at impact.
//!
//! Point defence is self-defence: it works under any weapons control status.

use crate::c2::C2State;
use crate::combat::{Health, Magazine, WeaponCatalog, WeaponInFlight, WeaponsInFlight};
use crate::components::{Position, Unit};
use crate::detection::{sensor_height_m, SensorKind, SensorSuite};
use crate::events::{EventLog, SimEventKind};
use crate::ids::{SideId, UnitId};
use crate::rng::{RngStreams, StreamId};
use crate::sim::UnitIndex;
use crate::time::{SimClock, Tick};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_c2::Emcon;
use sim_geo::{inverse, radar_horizon_m};
use sim_sensors::detection_range_m;
use sim_weapons::{
    decoy_matches, fire_channels, intercept_ticks, ph, ph_speed_modifier, point_defence_capable,
    reaction_time_s, Status, WeaponKind, WeaponSpec, DECOY_RANGE_MISSILE_M, DECOY_RANGE_TORPEDO_M,
    MISSILE_RCS_M2,
};
use std::collections::{BTreeMap, BTreeSet};

/// An interceptor on its way to a threat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    pub defender: UnitId,
    pub interceptor: String,
    pub threat: u64,
    pub resolve_at: Tick,
}

/// Point-defence state.
#[derive(Debug, Clone, Default, Resource, Serialize, Deserialize)]
pub struct PointDefence {
    /// (defender, interceptor weapon) → the tick each fire channel is busy until.
    pub channels: BTreeMap<(UnitId, String), Vec<Tick>>,
    /// (defender, threat weapon id) → when the defender first detected it.
    pub first_seen: BTreeMap<(UnitId, u64), Tick>,
    /// Shots in the air, in firing order.
    pub shots: Vec<Shot>,
}

struct Defender {
    id: UnitId,
    entity: Entity,
    side: SideId,
    position: Position,
    /// (antenna height, R₅₀ against a 0.1 m² missile) per radar.
    radars: Vec<(f64, f64)>,
}

fn take_round(world: &mut World, e: Entity, weapon: &str, left: u32) {
    if let Some(mut m) = world.get_mut::<Magazine>(e) {
        if left <= 1 {
            m.0.remove(weapon);
        } else {
            m.0.insert(weapon.to_owned(), left - 1);
        }
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn point_defence_system(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    let catalog = world.resource::<WeaponCatalog>().0.clone();
    resolve_shots(world, tick, &catalog);

    let threats = world.resource::<WeaponsInFlight>().list.clone();
    if threats.is_empty() {
        world.resource_mut::<PointDefence>().first_seen.clear();
        return;
    }

    // Defenders: operational units with a radiating radar, in unit-id order.
    let c2 = world.resource::<C2State>().clone();
    let planning = world.resource::<crate::planning::PlanningLayer>().clone();
    let mut defenders: Vec<Defender> = world
        .query::<(Entity, &Unit, &Position, &SensorSuite, &Health)>()
        .iter(world)
        .filter(|(_, u, p, _, h)| {
            h.status == Status::Operational
                && planning.doctrine_for(&c2, u, p.point).emcon == Emcon::Active
        })
        .filter_map(|(e, u, p, s, _)| {
            let radars: Vec<(f64, f64)> =
                s.0.iter()
                    .filter(|r| r.kind == SensorKind::Radar)
                    .filter_map(|r| {
                        let h = sensor_height_m(r.mount_height_m, u.domain, p.altitude_m)?;
                        let r50 = detection_range_m(
                            r.ref_range_m?,
                            r.ref_rcs_m2?,
                            MISSILE_RCS_M2,
                            r.max_range_m,
                        );
                        Some((h, r50))
                    })
                    .collect();
            (!radars.is_empty()).then_some(Defender {
                id: u.id,
                entity: e,
                side: u.side,
                position: *p,
                radars,
            })
        })
        .collect();
    defenders.sort_by_key(|d| d.id);

    for d in &defenders {
        let stores: Vec<(String, WeaponSpec)> = world
            .get::<Magazine>(d.entity)
            .map(|m| {
                m.0.iter()
                    .filter(|(_, n)| **n > 0)
                    .filter_map(|(w, _)| catalog.get(w).map(|s| (w.clone(), s.clone())))
                    .filter(|(_, s)| point_defence_capable(s))
                    .collect()
            })
            .unwrap_or_default();
        if stores.is_empty() {
            continue;
        }
        for t in &threats {
            if t.side == d.side {
                continue;
            }
            let Some(tspec) = catalog.get(&t.weapon) else {
                continue;
            };
            if tspec.kind == WeaponKind::Torpedo {
                continue;
            }
            let range = inverse(d.position.point, t.position).distance_m;
            let seen = d
                .radars
                .iter()
                .any(|&(h, r50)| range <= r50.min(radar_horizon_m(h, t.altitude_m.max(0.0))));
            if !seen {
                continue;
            }
            let first = *world
                .resource_mut::<PointDefence>()
                .first_seen
                .entry((d.id, t.id))
                .or_insert(tick);
            let busy_on_threat = world
                .resource::<PointDefence>()
                .shots
                .iter()
                .any(|s| s.defender == d.id && s.threat == t.id);
            if busy_on_threat {
                continue;
            }
            for (wid, w) in &stores {
                // Guns and CIWS defend only their own ship; SAMs defend the area.
                if w.kind != WeaponKind::Sam && t.target != d.id {
                    continue;
                }
                if tick.0 < first.0 + reaction_time_s(w.kind) {
                    continue;
                }
                if range < w.min_range_m || range > w.max_range_m {
                    continue;
                }
                let left = world
                    .get::<Magazine>(d.entity)
                    .and_then(|m| m.0.get(wid).copied())
                    .unwrap_or(0);
                if left == 0 {
                    continue;
                }
                let resolve_at =
                    Tick(tick.0 + intercept_ticks(range, tspec.speed_mps, w.speed_mps));
                let fired = {
                    let mut pd = world.resource_mut::<PointDefence>();
                    let channels = pd
                        .channels
                        .entry((d.id, wid.clone()))
                        .or_insert_with(|| vec![Tick(0); fire_channels(w.kind)]);
                    if let Some(free) = channels.iter_mut().find(|b| **b <= tick) {
                        *free = resolve_at;
                        pd.shots.push(Shot {
                            defender: d.id,
                            interceptor: wid.clone(),
                            threat: t.id,
                            resolve_at,
                        });
                        true
                    } else {
                        false
                    }
                };
                if fired {
                    take_round(world, d.entity, wid, left);
                    world.resource_mut::<EventLog>().push(
                        tick,
                        SimEventKind::PointDefenceShot {
                            defender: d.id,
                            interceptor: wid.clone(),
                            threat: t.id,
                            range_m: range,
                            resolve_at,
                        },
                    );
                    break;
                }
            }
        }
    }
    soft_kill(world, tick, &catalog, &threats);
    // Forget threats that are gone.
    let live: BTreeSet<u64> = world
        .resource::<WeaponsInFlight>()
        .list
        .iter()
        .map(|w| w.id)
        .collect();
    world
        .resource_mut::<PointDefence>()
        .first_seen
        .retain(|(_, id), _| live.contains(id));
}

fn resolve_shots(world: &mut World, tick: Tick, catalog: &BTreeMap<String, WeaponSpec>) {
    let due: Vec<Shot> = {
        let mut pd = world.resource_mut::<PointDefence>();
        let (due, later): (Vec<Shot>, Vec<Shot>) = std::mem::take(&mut pd.shots)
            .into_iter()
            .partition(|s| s.resolve_at <= tick);
        pd.shots = later;
        due
    };
    for s in due {
        let found = world
            .resource::<WeaponsInFlight>()
            .list
            .iter()
            .position(|w| w.id == s.threat);
        // Already gone: hit, missed or shot down by someone else.
        let Some(idx) = found else {
            continue;
        };
        let threat_weapon = world.resource::<WeaponsInFlight>().list[idx].weapon.clone();
        let (Some(int), Some(tspec)) = (catalog.get(&s.interceptor), catalog.get(&threat_weapon))
        else {
            continue;
        };
        let p = ph(
            int.base_ph,
            1.0,
            ph_speed_modifier(true, tspec.speed_mps),
            1.0,
        );
        let draw = world
            .resource_mut::<RngStreams>()
            .uniform(StreamId::Weapons);
        let killed = draw < p;
        world.resource_mut::<EventLog>().push(
            tick,
            SimEventKind::InterceptRoll {
                defender: s.defender,
                interceptor: s.interceptor.clone(),
                threat: s.threat,
                ph: p,
                draw,
                killed,
            },
        );
        if killed {
            world.resource_mut::<WeaponsInFlight>().list.remove(idx);
            world.resource_mut::<EventLog>().push(
                tick,
                SimEventKind::WeaponMissed {
                    weapon_id: s.threat,
                    reason: format!("shot down by {}", s.defender),
                },
            );
        }
    }
}

/// A ship under attack fires one matched decoy per incoming weapon.
fn soft_kill(
    world: &mut World,
    tick: Tick,
    catalog: &BTreeMap<String, WeaponSpec>,
    threats: &[WeaponInFlight],
) {
    for t in threats {
        if t.decoy_effect.is_some() {
            continue;
        }
        let Some(tspec) = catalog.get(&t.weapon) else {
            continue;
        };
        let Some(e) = world.resource::<UnitIndex>().0.get(&t.target).copied() else {
            continue;
        };
        let Some(pos) = world.get::<Position>(e).copied() else {
            continue;
        };
        let trigger = if tspec.kind == WeaponKind::Torpedo {
            DECOY_RANGE_TORPEDO_M
        } else {
            DECOY_RANGE_MISSILE_M
        };
        if inverse(pos.point, t.position).distance_m > trigger {
            continue;
        }
        let decoy = world.get::<Magazine>(e).and_then(|m| {
            m.0.iter()
                .filter(|(_, n)| **n > 0)
                .filter_map(|(w, n)| catalog.get(w).map(|s| (w.clone(), *n, s.clone())))
                .find(|(_, _, s)| s.kind == WeaponKind::Decoy && decoy_matches(s, tspec.guidance))
        });
        let Some((did, left, dspec)) = decoy else {
            continue;
        };
        take_round(world, e, &did, left);
        if let Some(w) = world
            .resource_mut::<WeaponsInFlight>()
            .list
            .iter_mut()
            .find(|w| w.id == t.id)
        {
            w.decoy_effect = Some(dspec.base_ph);
        }
        world.resource_mut::<EventLog>().push(
            tick,
            SimEventKind::DecoyLaunched {
                unit: t.target,
                decoy: did,
                threat: t.id,
                effect: dspec.base_ph,
            },
        );
    }
}
