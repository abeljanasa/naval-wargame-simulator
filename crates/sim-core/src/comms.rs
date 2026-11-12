//! Comms links from command posts to units, in the world (Phase 3 slice 10). The hop rule is in
//! `sim_c2::comms`; model doc `docs/models/comms.md`.
//!
//! * An order from a player reaches a unit when a command post that commands it is up and has a
//!   link to it: directly, or through the post's relay units (each hop under the same rules).
//! * A post is *disrupted* while jammed at the post (it cannot hear a unit at the edge of its
//!   range) or while its carrier's comms are knocked out. It is then down, its alternate takes
//!   over after the handover delay, and it comes back when the disruption ends. A post carried by
//!   a unit that is lost stays down.
//!
//! No draws: every rule is geometry and figures, so the state hash only changes through the
//! planning layer (posts going down and coming back).

use crate::c2::C2State;
use crate::combat::Health;
use crate::components::{Domain, Position, Unit};
use crate::detection::Jammer;
use crate::ids::{SideId, UnitId};
use crate::planning::{Facility, PlanningLayer};
use crate::sim::UnitIndex;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_c2::comms::{
    hop, total_js_db, LinkSpec, LinkStatus, Receives, DEFAULT_ANTI_JAM_DB, DEFAULT_POST_ANTENNA_M,
    LINK_JS_THRESHOLD_DB, SUBMERGED_COMMS_DEPTH_M,
};
use sim_c2::Emcon;
use sim_geo::{inverse, radar_horizon_m, GeoPoint};
use sim_weapons::System;

/// Antenna height of a surface ship, m (mast).
pub const SHIP_ANTENNA_M: f64 = 25.0;
/// Antenna height of a land unit, m.
pub const LAND_ANTENNA_M: f64 = 10.0;
/// Antenna height of a submarine at or above [`SUBMERGED_COMMS_DEPTH_M`] (mast raised), m.
pub const SUBMARINE_ANTENNA_M: f64 = 2.0;

/// One end of a link, where it is now.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Node {
    pub(crate) side: SideId,
    pub(crate) point: GeoPoint,
    pub(crate) antenna_m: f64,
    pub(crate) receives: Receives,
}

/// A radiating jammer: (side, position, antenna height, figure at 1 km), in unit-id order.
pub(crate) type Jammers = Vec<(SideId, GeoPoint, f64, f64)>;

/// A unit's comms as its side sees them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CommsView {
    pub unit: UnitId,
    pub status: LinkStatus,
    /// The post whose link reaches the unit (or that was tried last).
    pub post: Option<u32>,
    /// The relay the link goes through, if not direct.
    pub via: Option<UnitId>,
}

/// A command post's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PostView {
    pub facility: u32,
    pub up: bool,
    /// Why it is down: jammed, comms knocked out, or (lost with its unit) `no_post`.
    pub status: LinkStatus,
    /// Game time it went down.
    pub since_s: Option<f64>,
}

fn unit_node(world: &World, e: Entity) -> Option<Node> {
    let u = world.get::<Unit>(e)?;
    let p = world.get::<Position>(e)?;
    let knocked_out = world
        .get::<Health>(e)
        .is_some_and(|h| h.criticals.contains(&System::Comms));
    let (antenna_m, receives) = match u.domain {
        Domain::Surface => (SHIP_ANTENNA_M, Ok(())),
        Domain::Land => (LAND_ANTENNA_M, Ok(())),
        Domain::Air => (p.altitude_m.max(0.0), Ok(())),
        Domain::Subsurface if p.altitude_m >= SUBMERGED_COMMS_DEPTH_M => {
            (SUBMARINE_ANTENNA_M, Ok(()))
        }
        Domain::Subsurface => (0.0, Err(LinkStatus::Submerged)),
    };
    Some(Node {
        side: u.side,
        point: p.point,
        antenna_m,
        receives: if knocked_out {
            Err(LinkStatus::KnockedOut)
        } else {
            receives
        },
    })
}

pub(crate) fn node_of(world: &World, unit: UnitId) -> Option<Node> {
    let e = *world.resource::<UnitIndex>().0.get(&unit)?;
    unit_node(world, e)
}

/// Where a post is, and its antenna (`None`: carried by a unit that has been lost).
pub(crate) fn post_node(world: &World, f: &Facility) -> Option<Node> {
    match f.unit {
        Some(u) => node_of(world, u),
        None => Some(Node {
            side: f.side,
            point: f.position,
            antenna_m: f.antenna_m.unwrap_or(DEFAULT_POST_ANTENNA_M),
            receives: Ok(()),
        }),
    }
}

/// Every radiating jammer (EMCON active), in unit-id order.
pub(crate) fn jammers(world: &World) -> Jammers {
    let c2 = world.resource::<C2State>();
    let layer = world.resource::<PlanningLayer>();
    world
        .resource::<UnitIndex>()
        .0
        .values()
        .filter_map(|&e| {
            let spec = world.get::<Jammer>(e)?.0?;
            let u = world.get::<Unit>(e)?;
            let p = world.get::<Position>(e)?;
            if layer.doctrine_for(c2, u, p.point).emcon != Emcon::Active {
                return None;
            }
            let n = unit_node(world, e)?;
            Some((u.side, p.point, n.antenna_m, spec.jn_db_at_1km))
        })
        .collect()
}

/// Other sides' jammers within radio line of sight of `at`: (figure at 1 km, distance m).
fn jam_at(jammers: &Jammers, at: &Node) -> Vec<(f64, f64)> {
    jammers
        .iter()
        .filter(|(side, ..)| *side != at.side)
        .filter_map(|&(_, p, h, j1)| {
            let d = inverse(at.point, p).distance_m;
            (d <= radar_horizon_m(at.antenna_m, h)).then_some((j1, d))
        })
        .collect()
}

fn link_spec(f: &Facility) -> LinkSpec {
    LinkSpec {
        range_m: f.range_m.unwrap_or(f.radius_m),
        beyond_line_of_sight: !f.line_of_sight,
        anti_jam_db: f.anti_jam_db.unwrap_or(DEFAULT_ANTI_JAM_DB),
    }
}

fn hop_between(spec: LinkSpec, jammers: &Jammers, tx: &Node, rx: &Node) -> LinkStatus {
    let d = inverse(tx.point, rx.point).distance_m;
    hop(
        spec,
        (rx.receives, tx.receives),
        d,
        radar_horizon_m(tx.antenna_m, rx.antenna_m),
        &jam_at(jammers, rx),
        &jam_at(jammers, tx),
    )
}

/// The link from post `f` to `unit`: direct, else through the post's relays (each relay reached
/// from the post or an earlier relay, in list order). Returns the status and the relay used; on
/// failure, the direct hop's status.
pub(crate) fn link(
    world: &World,
    jammers: &Jammers,
    f: &Facility,
    unit: UnitId,
) -> (LinkStatus, Option<UnitId>) {
    let (Some(post), Some(target)) = (post_node(world, f), node_of(world, unit)) else {
        return (LinkStatus::NoPost, None);
    };
    let spec = link_spec(f);
    let direct = hop_between(spec, jammers, &post, &target);
    if direct == LinkStatus::Up || f.relays.is_empty() {
        return (direct, None);
    }
    // Relays the post reaches, growing in list order until nothing changes.
    let mut reached: Vec<(UnitId, Node)> = Vec::new();
    loop {
        let before = reached.len();
        for &r in &f.relays {
            if r == unit || reached.iter().any(|(id, _)| *id == r) {
                continue;
            }
            let Some(rn) = node_of(world, r).filter(|n| n.side == f.side) else {
                continue;
            };
            let from_post = hop_between(spec, jammers, &post, &rn) == LinkStatus::Up;
            let from_relay = || {
                reached
                    .iter()
                    .any(|(_, n)| hop_between(spec, jammers, n, &rn) == LinkStatus::Up)
            };
            if from_post || from_relay() {
                reached.push((r, rn));
            }
        }
        if reached.len() == before {
            break;
        }
    }
    for (r, rn) in &reached {
        if hop_between(spec, jammers, rn, &target) == LinkStatus::Up {
            return (LinkStatus::Up, Some(*r));
        }
    }
    (direct, None)
}

/// Why a post that still exists cannot command: jammed at the post (it cannot hear a unit at the
/// edge of its range), or its carrier's comms are knocked out. `None`: it can.
pub(crate) fn disruption(world: &World, jammers: &Jammers, f: &Facility) -> Option<LinkStatus> {
    let node = post_node(world, f)?;
    if let Err(s) = node.receives {
        return Some(s);
    }
    let spec = link_spec(f);
    let js = total_js_db(&jam_at(jammers, &node), spec.range_m, spec.anti_jam_db);
    js.is_some_and(|v| v >= LINK_JS_THRESHOLD_DB)
        .then_some(LinkStatus::Jammed)
}
