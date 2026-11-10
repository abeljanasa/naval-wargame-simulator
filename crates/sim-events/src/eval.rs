//! Deciding when events fire: triggers against a tick's facts, conditions against the world.

use crate::model::{Condition, Trigger};
use serde::{Deserialize, Serialize};
use sim_geo::GeoPoint;
use std::collections::BTreeSet;

/// A unit in play this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitFact {
    pub id: u64,
    pub side: u16,
    pub position: GeoPoint,
}

/// What happened this tick, gathered by the engine.
#[derive(Debug, Clone, Copy)]
pub struct TickFacts<'a> {
    /// Game time now and one tick earlier, seconds.
    pub now_s: f64,
    pub prev_s: f64,
    /// Units in play, in id order.
    pub units: &'a [UnitFact],
    /// Units destroyed this tick: (unit, side).
    pub destroyed: &'a [(u64, u16)],
    /// Tracks gained this tick: (side, target's side).
    pub contacts_gained: &'a [(u16, u16)],
}

/// An event's state during a run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRuntime {
    pub enabled: bool,
    /// Times it has fired.
    pub fired: u32,
    /// Area triggers: the matching units inside the area at the last check.
    #[serde(default)]
    pub inside: BTreeSet<u64>,
}

fn matches(side: Option<u16>, unit: Option<u64>, u_id: u64, u_side: u16) -> bool {
    side.is_none_or(|s| s == u_side) && unit.is_none_or(|id| id == u_id)
}

/// Periods of an `Every` trigger begun by time `t` (`None` before the start).
fn periods(t: f64, period_s: f64, start_s: f64) -> Option<f64> {
    (t >= start_s).then(|| ((t - start_s) / period_s).floor())
}

impl Trigger {
    /// Did the trigger occur this tick? Area triggers also update `inside` (the matching units
    /// now in the area), so this runs every tick whether or not the event can fire.
    pub fn occurred(&self, f: &TickFacts<'_>, inside: &mut BTreeSet<u64>) -> bool {
        match self {
            Trigger::Time { at_s } => f.prev_s < *at_s && *at_s <= f.now_s,
            Trigger::Every { period_s, start_s } => {
                let now = periods(f.now_s, *period_s, *start_s);
                now.is_some() && now != periods(f.prev_s, *period_s, *start_s)
            }
            Trigger::AreaEntered { area, side, unit } => {
                let now: BTreeSet<u64> = f
                    .units
                    .iter()
                    .filter(|u| matches(*side, *unit, u.id, u.side) && area.contains(u.position))
                    .map(|u| u.id)
                    .collect();
                let entered = now.iter().any(|id| !inside.contains(id));
                *inside = now;
                entered
            }
            Trigger::UnitDestroyed { side, unit } => f
                .destroyed
                .iter()
                .any(|&(id, s)| matches(*side, *unit, id, s)),
            Trigger::ContactGained { side, target_side } => f
                .contacts_gained
                .iter()
                .any(|&(s, t)| s == *side && target_side.is_none_or(|ts| ts == t)),
        }
    }
}

/// What conditions may ask of the simulation.
pub trait EventWorld {
    /// Game time, seconds.
    fn now_s(&self) -> f64;
    /// Times the event has fired (0 for an unknown event).
    fn fired(&self, event: &str) -> u32;
    fn unit_alive(&self, unit: u64) -> bool;
    /// Units of `side` in play.
    fn units_alive(&self, side: u16) -> u32;
    /// The next uniform draw in [0, 1) from the `Events` stream.
    fn draw(&mut self) -> f64;
    /// A unit in play, if it is.
    fn unit(&self, unit: u64) -> Option<UnitFact>;
}

impl Condition {
    /// Does the condition hold now? Only a Lua condition can fail to say.
    pub fn holds(&self, w: &mut dyn EventWorld) -> Result<bool, String> {
        Ok(match self {
            Condition::EventFired { event } => w.fired(event) > 0,
            Condition::EventNotFired { event } => w.fired(event) == 0,
            Condition::UnitAlive { unit } => w.unit_alive(*unit),
            Condition::UnitGone { unit } => !w.unit_alive(*unit),
            Condition::UnitsAlive { side, cmp, count } => cmp.test(w.units_alive(*side), *count),
            Condition::TimeBetween { from_s, to_s } => {
                let t = w.now_s();
                from_s.is_none_or(|f| t >= f) && to_s.is_none_or(|e| t <= e)
            }
            Condition::Probability { p } => w.draw() < *p,
            Condition::Lua { script } => return crate::lua::condition(script, w),
        })
    }

    /// Index of the first condition that does not hold, or `None` when all hold. Conditions
    /// after a failing one are not checked (so they take no draws). A condition that cannot be
    /// evaluated (a Lua error) is returned as `Err((index, reason))`.
    pub fn first_failing(
        conditions: &[Condition],
        w: &mut dyn EventWorld,
    ) -> Result<Option<usize>, (usize, String)> {
        for (i, c) in conditions.iter().enumerate() {
            if !c.holds(w).map_err(|e| (i, e))? {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Area, Cmp};
    use std::collections::BTreeMap;

    fn facts<'a>(now: f64, units: &'a [UnitFact]) -> TickFacts<'a> {
        TickFacts {
            now_s: now,
            prev_s: now - 1.0,
            units,
            destroyed: &[],
            contacts_gained: &[],
        }
    }

    #[test]
    fn time_fires_once_on_the_tick_it_is_reached() {
        let t = Trigger::Time { at_s: 10.5 };
        let mut s = BTreeSet::new();
        let fired: Vec<f64> = (0..20)
            .map(f64::from)
            .filter(|&n| t.occurred(&facts(n, &[]), &mut s))
            .collect();
        assert_eq!(fired, vec![11.0]);
        // At exactly 0, on the first tick.
        assert!(Trigger::Time { at_s: 0.0 }.occurred(&facts(0.0, &[]), &mut s));
    }

    #[test]
    fn every_fires_at_start_then_each_period() {
        let t = Trigger::Every {
            period_s: 30.0,
            start_s: 60.0,
        };
        let mut s = BTreeSet::new();
        let fired: Vec<f64> = (0..=150)
            .map(f64::from)
            .filter(|&n| t.occurred(&facts(n, &[]), &mut s))
            .collect();
        assert_eq!(fired, vec![60.0, 90.0, 120.0, 150.0]);
    }

    #[test]
    fn area_fires_on_entry_not_while_inside_and_again_on_re_entry() {
        let t = Trigger::AreaEntered {
            area: Area::Circle {
                centre: GeoPoint::new(0.0, 0.0),
                radius_m: 1000.0,
            },
            side: Some(2),
            unit: None,
        };
        let at = |lon: f64, side: u16| UnitFact {
            id: 7,
            side,
            position: GeoPoint::new(0.0, lon),
        };
        let mut inside = BTreeSet::new();
        let path = [0.1, 0.005, 0.004, 0.1, 0.0];
        let fired: Vec<bool> = path
            .iter()
            .map(|&lon| t.occurred(&facts(0.0, &[at(lon, 2)]), &mut inside))
            .collect();
        assert_eq!(fired, vec![false, true, false, false, true]);
        // A unit of another side does not count.
        let mut inside = BTreeSet::new();
        assert!(!t.occurred(&facts(0.0, &[at(0.0, 1)]), &mut inside));
    }

    #[test]
    fn destroyed_and_contact_triggers_filter_by_side_and_unit() {
        let mut s = BTreeSet::new();
        let f = TickFacts {
            now_s: 5.0,
            prev_s: 4.0,
            units: &[],
            destroyed: &[(12, 2)],
            contacts_gained: &[(1, 2)],
        };
        assert!(Trigger::UnitDestroyed {
            side: Some(2),
            unit: None
        }
        .occurred(&f, &mut s));
        assert!(!Trigger::UnitDestroyed {
            side: None,
            unit: Some(13)
        }
        .occurred(&f, &mut s));
        assert!(Trigger::ContactGained {
            side: 1,
            target_side: None
        }
        .occurred(&f, &mut s));
        assert!(!Trigger::ContactGained {
            side: 2,
            target_side: None
        }
        .occurred(&f, &mut s));
        assert!(!Trigger::ContactGained {
            side: 1,
            target_side: Some(3)
        }
        .occurred(&f, &mut s));
    }

    struct World {
        now: f64,
        fired: BTreeMap<String, u32>,
        alive: BTreeMap<u64, u16>,
        draws: Vec<f64>,
    }

    impl EventWorld for World {
        fn now_s(&self) -> f64 {
            self.now
        }
        fn fired(&self, event: &str) -> u32 {
            self.fired.get(event).copied().unwrap_or(0)
        }
        fn unit_alive(&self, unit: u64) -> bool {
            self.alive.contains_key(&unit)
        }
        fn units_alive(&self, side: u16) -> u32 {
            u32::try_from(self.alive.values().filter(|&&s| s == side).count()).unwrap()
        }
        fn draw(&mut self) -> f64 {
            self.draws.remove(0)
        }
        fn unit(&self, unit: u64) -> Option<UnitFact> {
            self.alive.get(&unit).map(|&side| UnitFact {
                id: unit,
                side,
                position: GeoPoint::new(0.0, 0.0),
            })
        }
    }

    #[test]
    fn conditions_hold_in_order_and_draw_only_when_reached() {
        let mut w = World {
            now: 100.0,
            fired: BTreeMap::from([("a".to_owned(), 1)]),
            alive: BTreeMap::from([(1, 1), (2, 2), (3, 2)]),
            draws: vec![0.3, 0.9],
        };
        let all = [
            Condition::EventFired { event: "a".into() },
            Condition::EventNotFired { event: "b".into() },
            Condition::UnitAlive { unit: 1 },
            Condition::UnitGone { unit: 9 },
            Condition::UnitsAlive {
                side: 2,
                cmp: Cmp::Eq,
                count: 2,
            },
            Condition::TimeBetween {
                from_s: Some(50.0),
                to_s: None,
            },
            Condition::Probability { p: 0.5 },
        ];
        assert_eq!(Condition::first_failing(&all, &mut w), Ok(None));
        assert_eq!(w.draws, vec![0.9]);
        // The second draw (0.9) fails a 0.5 probability.
        assert_eq!(
            Condition::first_failing(&[Condition::Probability { p: 0.5 }], &mut w),
            Ok(Some(0))
        );
        // A failing condition stops the checks: the probability is not drawn.
        w.draws = vec![0.0];
        let c = [
            Condition::UnitsAlive {
                side: 2,
                cmp: Cmp::Lt,
                count: 2,
            },
            Condition::Probability { p: 1.0 },
        ];
        assert_eq!(Condition::first_failing(&c, &mut w), Ok(Some(0)));
        assert_eq!(w.draws, vec![0.0]);
    }
}
