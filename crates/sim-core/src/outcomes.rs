//! A display-only feed of recent weapon outcomes: hit, miss, or shot down, with where each
//! happened. It drives the hit/miss markers on the map and the status-bar notices.
//!
//! The feed is filled where the outcome is decided, because a destroyed target leaves the
//! world in the same step. It is **not** part of the state hash: it only shows what the event
//! log already records (`HitRoll`, `InterceptRoll`, `WeaponMissed`), which stays authoritative.
//! It **is** saved (format 25), so a replay that seeks to a keyframe shows the same markers as a
//! straight run.

use crate::components::{Position, Unit};
use crate::ids::{SideId, UnitId};
use crate::sim::UnitIndex;
use crate::time::Tick;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use sim_geo::GeoPoint;

/// How long an outcome stays in the feed, game seconds.
pub const KEEP_S: u64 = 120;

/// How long the picture shows an outcome, game seconds.
pub const SHOW_S: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    Hit,
    Miss,
    /// Killed in flight by point defence.
    ShotDown,
}

/// One weapon outcome. Names are as they were when it happened (a destroyed unit leaves the
/// world). The picture blanks what a side's viewer may not know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Outcome {
    pub tick: Tick,
    /// Weapon type, e.g. `WPN-ASM-SUB`.
    pub weapon: String,
    pub result: OutcomeKind,
    /// Where it happened: the target for a hit, the weapon for a miss or a kill in flight.
    pub position: GeoPoint,
    pub shooter: Option<UnitId>,
    pub shooter_name: Option<String>,
    /// The firing side (a minefield's side for a mine).
    pub shooter_side: Option<SideId>,
    pub target: Option<UnitId>,
    pub target_name: Option<String>,
    pub target_side: Option<SideId>,
    /// Why it missed, or who shot it down.
    pub reason: Option<String>,
}

/// The feed (saved, not hashed).
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OutcomeFeed(pub Vec<Outcome>);

/// A unit's name, side and position, if it is in the world.
pub(crate) fn unit_info(
    world: &World,
    id: UnitId,
) -> (Option<String>, Option<SideId>, Option<GeoPoint>) {
    let Some(e) = world.resource::<UnitIndex>().0.get(&id).copied() else {
        return (None, None, None);
    };
    let unit = world.get::<Unit>(e);
    (
        unit.map(|u| u.name.clone()),
        unit.map(|u| u.side),
        world.get::<Position>(e).map(|p| p.point),
    )
}

/// What the caller knows of an outcome; names and sides are filled in from the world.
pub(crate) struct Note {
    pub tick: Tick,
    pub weapon: String,
    pub result: OutcomeKind,
    /// Position if the caller has it (the weapon's); otherwise the target's.
    pub position: Option<GeoPoint>,
    pub shooter: Option<UnitId>,
    pub shooter_side: Option<SideId>,
    pub target: Option<UnitId>,
    pub reason: Option<String>,
}

/// Adds an outcome to the feed and drops those older than [`KEEP_S`]. Call it before the
/// target is destroyed, while it is still in the world.
pub(crate) fn note(world: &mut World, n: Note) {
    let (shooter_name, side_of_shooter, _) = n
        .shooter
        .map_or((None, None, None), |s| unit_info(world, s));
    let (target_name, target_side, target_at) =
        n.target.map_or((None, None, None), |t| unit_info(world, t));
    let Some(position) = n.position.or(target_at) else {
        return;
    };
    let outcome = Outcome {
        tick: n.tick,
        weapon: n.weapon,
        result: n.result,
        position,
        shooter: n.shooter,
        shooter_name,
        shooter_side: n.shooter_side.or(side_of_shooter),
        target: n.target,
        target_name,
        target_side,
        reason: n.reason,
    };
    let mut feed = world.resource_mut::<OutcomeFeed>();
    feed.0
        .retain(|o| n.tick.0.saturating_sub(o.tick.0) <= KEEP_S);
    feed.0.push(outcome);
}

/// The outcomes a viewer may see from the last [`SHOW_S`] seconds.
/// - The white cell (`side` = `None`) sees all of them.
/// - A side sees its own shots, but not what it hit: it fired at a contact, so the target's
///   identity is blanked.
/// - A side sees attacks on its own units, but not which unit fired.
#[must_use]
pub fn visible(feed: &OutcomeFeed, now: Tick, side: Option<SideId>) -> Vec<Outcome> {
    feed.0
        .iter()
        .filter(|o| now.0.saturating_sub(o.tick.0) <= SHOW_S)
        .filter_map(|o| match side {
            None => Some(o.clone()),
            Some(s) if o.shooter_side == Some(s) => Some(Outcome {
                target: None,
                target_name: None,
                ..o.clone()
            }),
            Some(s) if o.target_side == Some(s) => Some(Outcome {
                shooter: None,
                shooter_name: None,
                ..o.clone()
            }),
            Some(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(tick: u64, shooter_side: u16, target_side: u16) -> Outcome {
        Outcome {
            tick: Tick(tick),
            weapon: "WPN-ASM-SUB".into(),
            result: OutcomeKind::Hit,
            position: GeoPoint::new(-39.6, 147.0),
            shooter: Some(UnitId(3)),
            shooter_name: Some("Coral".into()),
            shooter_side: Some(SideId(shooter_side)),
            target: Some(UnitId(11)),
            target_name: Some("Cinder".into()),
            target_side: Some(SideId(target_side)),
            reason: None,
        }
    }

    #[test]
    fn each_viewer_sees_what_fog_of_war_allows() {
        let feed = OutcomeFeed(vec![outcome(100, 1, 2), outcome(10, 1, 2)]);
        let now = Tick(120);
        // The old one (110 s ago) is past SHOW_S for everyone.
        let wc = visible(&feed, now, None);
        assert_eq!(wc.len(), 1);
        assert_eq!(wc[0].target_name.as_deref(), Some("Cinder"));
        let blue = visible(&feed, now, Some(SideId(1)));
        assert_eq!(blue.len(), 1);
        assert!(blue[0].target.is_none() && blue[0].target_name.is_none());
        assert_eq!(blue[0].shooter_name.as_deref(), Some("Coral"));
        let red = visible(&feed, now, Some(SideId(2)));
        assert_eq!(red.len(), 1);
        assert!(red[0].shooter.is_none() && red[0].shooter_name.is_none());
        assert_eq!(red[0].target_name.as_deref(), Some("Cinder"));
        assert!(visible(&feed, now, Some(SideId(3))).is_empty());
    }
}
