//! What an event is: trigger, conditions, actions. Ids are plain integers (unit `u64`, side
//! `u16`), the same JSON as `sim-core`'s `UnitId` and `SideId`.

use serde::{Deserialize, Serialize};
use sim_ai::Mission;
use sim_c2::{Doctrine, Posture};
use sim_geo::GeoPoint;
use sim_sensors::Environment;
use std::collections::BTreeSet;

/// A region of the sea.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Area {
    /// Within `radius_m` (geodesic) of `centre`.
    Circle { centre: GeoPoint, radius_m: f64 },
    /// Inside the polygon, whose edges are straight lines in latitude and longitude. It must not
    /// cross the antimeridian.
    Polygon { points: Vec<GeoPoint> },
}

impl Area {
    /// True when `p` is inside the area (on a circle's edge counts as inside).
    #[must_use]
    pub fn contains(&self, p: GeoPoint) -> bool {
        match self {
            Area::Circle { centre, radius_m } => {
                sim_geo::inverse(*centre, p).distance_m <= *radius_m
            }
            Area::Polygon { points } => {
                // Even-odd ray cast towards +longitude.
                let mut inside = false;
                let n = points.len();
                for i in 0..n {
                    let a = points[i];
                    let b = points[(i + n - 1) % n];
                    if (a.lat_deg > p.lat_deg) != (b.lat_deg > p.lat_deg) {
                        let x = a.lon_deg
                            + (p.lat_deg - a.lat_deg) / (b.lat_deg - a.lat_deg)
                                * (b.lon_deg - a.lon_deg);
                        if p.lon_deg < x {
                            inside = !inside;
                        }
                    }
                }
                inside
            }
        }
    }

    /// Checks the shape: a finite radius > 0, or at least three valid polygon points.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Area::Circle { centre, radius_m } => {
                if centre.is_valid() && radius_m.is_finite() && *radius_m > 0.0 {
                    Ok(())
                } else {
                    Err("circle needs a valid centre and a radius > 0".into())
                }
            }
            Area::Polygon { points } => {
                if points.len() >= 3 && points.iter().all(GeoPoint::is_valid) {
                    Ok(())
                } else {
                    Err("polygon needs at least 3 valid points".into())
                }
            }
        }
    }
}

/// What makes an event check its conditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Once, on the tick when game time reaches `at_s`.
    Time { at_s: f64 },
    /// At `start_s`, then every `period_s` after it.
    Every {
        period_s: f64,
        #[serde(default)]
        start_s: f64,
    },
    /// A unit (of `side`, or the given `unit`, or any) comes into the area. A unit already
    /// inside when the events are loaded counts as entering on the first check.
    AreaEntered {
        area: Area,
        #[serde(default)]
        side: Option<u16>,
        #[serde(default)]
        #[cfg_attr(feature = "specta", specta(type = Option<u32>))]
        unit: Option<u64>,
    },
    /// A unit (of `side`, or the given `unit`, or any) is destroyed.
    UnitDestroyed {
        #[serde(default)]
        side: Option<u16>,
        #[serde(default)]
        #[cfg_attr(feature = "specta", specta(type = Option<u32>))]
        unit: Option<u64>,
    },
    /// `side` gains a new track (on a unit of `target_side`, or any).
    ContactGained {
        side: u16,
        #[serde(default)]
        target_side: Option<u16>,
    },
}

/// Comparison for counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

impl Cmp {
    #[must_use]
    pub fn test(self, a: u32, b: u32) -> bool {
        match self {
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
            Cmp::Eq => a == b,
            Cmp::Ge => a >= b,
            Cmp::Gt => a > b,
        }
    }
}

/// Something that must be true for a triggered event to fire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    /// The event has fired at least once.
    EventFired { event: String },
    /// The event has not fired.
    EventNotFired { event: String },
    UnitAlive {
        #[cfg_attr(feature = "specta", specta(type = u32))]
        unit: u64,
    },
    /// The unit has been destroyed or removed (or never existed).
    UnitGone {
        #[cfg_attr(feature = "specta", specta(type = u32))]
        unit: u64,
    },
    /// The number of `side`'s units still in play, compared with `count`.
    UnitsAlive { side: u16, cmp: Cmp, count: u32 },
    /// Game time is within the window (either end may be left open).
    TimeBetween {
        #[serde(default)]
        from_s: Option<f64>,
        #[serde(default)]
        to_s: Option<f64>,
    },
    /// Holds with probability `p` (one draw from the `Events` stream, taken only when this
    /// condition is reached).
    Probability { p: f64 },
    /// A Lua script that returns `true` or `false` (`docs/models/lua-scripting.md`).
    Lua { script: String },
}

/// Who receives a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Audience {
    /// Every side (and the white cell).
    All,
    /// One side (and the white cell).
    Side { side: u16 },
    /// The white cell only.
    WhiteCell,
}

impl Audience {
    /// Does a viewer of `side` (`None` = the white cell) receive it? The white cell sees all.
    #[must_use]
    pub fn reaches(self, side: Option<u16>) -> bool {
        match (self, side) {
            (_, None) | (Audience::All, _) => true,
            (Audience::Side { side: to }, Some(s)) => to == s,
            (Audience::WhiteCell, Some(_)) => false,
        }
    }
}

/// Where a doctrine action applies (the same JSON as `sim-core`'s `DoctrineScope`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum DoctrineTarget {
    Side {
        side: u16,
    },
    Group {
        side: u16,
        group: String,
    },
    Unit {
        #[cfg_attr(feature = "specta", specta(type = u32))]
        unit: u64,
    },
}

/// What a fired event does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventAction {
    /// Sends a message to the audience's message log.
    Message {
        #[serde(flatten)]
        to: Audience,
        text: String,
    },
    SetPosture {
        side: u16,
        toward: u16,
        posture: Posture,
    },
    SetDoctrine {
        scope: DoctrineTarget,
        doctrine: Doctrine,
    },
    SetEnvironment {
        environment: Environment,
    },
    SetSpeed {
        #[cfg_attr(feature = "specta", specta(type = u32))]
        unit: u64,
        speed_mps: f64,
    },
    SetRoute {
        #[cfg_attr(feature = "specta", specta(type = u32))]
        unit: u64,
        waypoints: Vec<GeoPoint>,
    },
    AssignMission {
        #[cfg_attr(feature = "specta", specta(type = u32))]
        unit: u64,
        mission: Option<Mission>,
    },
    /// Brings a reserve unit (declared in the scenario, held off the board) into play.
    SpawnReserve {
        #[cfg_attr(feature = "specta", specta(type = u32))]
        unit: u64,
    },
    /// Turns an event on or off.
    EnableEvent {
        event: String,
        enabled: bool,
    },
    /// Delivers an inject from the scenario's library.
    SendInject {
        inject: String,
    },
    /// A Lua script whose `sim.*` requests become actions (`docs/models/lua-scripting.md`).
    Lua {
        script: String,
    },
}

/// A prepared message in the scenario's inject library, sent by an event or by the white cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Inject {
    /// Unique id, referred to by `send_inject` actions.
    pub id: String,
    /// Short heading shown with the message.
    pub title: String,
    /// Who the message appears to come from (role-play), e.g. `MV Ember`. Empty: the event's
    /// name, or `Exercise control` when the white cell sends it.
    #[serde(default)]
    pub from: String,
    #[serde(flatten)]
    pub to: Audience,
    pub text: String,
    /// Guidance for the white cell (when to use it, expected response). Never sent to sides.
    #[serde(default)]
    pub note: String,
}

impl EventAction {
    fn validate(&self, ids: &BTreeSet<&str>, injects: &BTreeSet<&str>) -> Result<(), String> {
        match self {
            EventAction::Message { text, .. } if text.trim().is_empty() => {
                Err("message text is empty".into())
            }
            EventAction::SetPosture { side, toward, .. } if side == toward => {
                Err("a side is always friendly to itself".into())
            }
            EventAction::SetDoctrine {
                scope: DoctrineTarget::Group { group, .. },
                ..
            } if group.trim().is_empty() => Err("group name is empty".into()),
            EventAction::SetEnvironment { environment } => environment.validate(),
            EventAction::SetSpeed { speed_mps, .. }
                if !(speed_mps.is_finite() && *speed_mps >= 0.0) =>
            {
                Err("speed must be finite and >= 0".into())
            }
            EventAction::SetRoute { waypoints, .. }
                if !waypoints.iter().all(GeoPoint::is_valid) =>
            {
                Err("waypoint has invalid coordinates".into())
            }
            EventAction::AssignMission {
                mission: Some(m), ..
            } => m.validate(),
            EventAction::EnableEvent { event, .. } if !ids.contains(event.as_str()) => {
                Err(format!("unknown event {event:?}"))
            }
            EventAction::SendInject { inject } if !injects.contains(inject.as_str()) => {
                Err(format!("unknown inject {inject:?}"))
            }
            EventAction::Lua { script } => {
                crate::lua::check(script).map_err(|e| format!("Lua action: {e}"))
            }
            _ => Ok(()),
        }
    }
}

/// A scenario event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ScenarioEvent {
    /// Unique id, referred to by conditions and actions. Events are checked in id order.
    pub id: String,
    /// For the white cell and the logs.
    #[serde(default)]
    pub name: String,
    /// A disabled event is not checked until an action or the white cell enables it.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Fire every time the trigger occurs, rather than once.
    #[serde(default)]
    pub repeatable: bool,
    pub trigger: Trigger,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    pub actions: Vec<EventAction>,
}

fn yes() -> bool {
    true
}

fn finite_time(t: f64) -> bool {
    t.is_finite() && t >= 0.0
}

/// Checks a set of events and injects on their own: unique ids, valid areas and values, and
/// references to events and injects that exist. References to units and sides are checked when
/// they are used.
pub fn validate_events(events: &[ScenarioEvent], injects: &[Inject]) -> Result<(), String> {
    let mut inject_ids = BTreeSet::new();
    for i in injects {
        if i.id.trim().is_empty() || i.text.trim().is_empty() {
            return Err(format!("inject {:?} needs an id and text", i.id));
        }
        if !inject_ids.insert(i.id.as_str()) {
            return Err(format!("duplicate inject id {:?}", i.id));
        }
    }
    let mut ids = BTreeSet::new();
    for e in events {
        if e.id.trim().is_empty() {
            return Err("an event has an empty id".into());
        }
        if !ids.insert(e.id.as_str()) {
            return Err(format!("duplicate event id {:?}", e.id));
        }
    }
    for e in events {
        let fail = |m: String| format!("event {:?}: {m}", e.id);
        match &e.trigger {
            Trigger::Time { at_s } if !finite_time(*at_s) => {
                return Err(fail("time must be finite and >= 0".into()))
            }
            Trigger::Every { period_s, start_s }
                if !(period_s.is_finite() && *period_s > 0.0 && finite_time(*start_s)) =>
            {
                return Err(fail("period must be > 0 and start >= 0".into()))
            }
            Trigger::AreaEntered { area, .. } => area.validate().map_err(fail)?,
            _ => {}
        }
        for c in &e.conditions {
            match c {
                Condition::EventFired { event } | Condition::EventNotFired { event }
                    if !ids.contains(event.as_str()) =>
                {
                    return Err(fail(format!("condition refers to unknown event {event:?}")))
                }
                Condition::Probability { p } if !(0.0..=1.0).contains(p) => {
                    return Err(fail("probability must be in 0..=1".into()))
                }
                Condition::Lua { script } => {
                    crate::lua::check(script).map_err(|e| fail(format!("Lua condition: {e}")))?;
                }
                Condition::TimeBetween { from_s, to_s }
                    if from_s.is_some_and(|t| !finite_time(t))
                        || to_s.is_some_and(|t| !finite_time(t)) =>
                {
                    return Err(fail("times must be finite and >= 0".into()))
                }
                _ => {}
            }
        }
        if e.actions.is_empty() {
            return Err(fail("no actions".into()));
        }
        for a in &e.actions {
            a.validate(&ids, &inject_ids).map_err(fail)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str, trigger: Trigger) -> ScenarioEvent {
        ScenarioEvent {
            id: id.into(),
            name: String::new(),
            enabled: true,
            repeatable: false,
            trigger,
            conditions: vec![],
            actions: vec![EventAction::Message {
                to: Audience::All,
                text: "hello".into(),
            }],
        }
    }

    #[test]
    fn circle_and_polygon_containment() {
        let c = Area::Circle {
            centre: GeoPoint::new(0.0, 0.0),
            radius_m: 10_000.0,
        };
        // 0.08° of longitude at the equator is 8 905.6 m; 0.1° is 11 132 m.
        assert!(c.contains(GeoPoint::new(0.0, 0.08)));
        assert!(!c.contains(GeoPoint::new(0.0, 0.1)));
        let square = Area::Polygon {
            points: vec![
                GeoPoint::new(0.0, 0.0),
                GeoPoint::new(0.0, 1.0),
                GeoPoint::new(1.0, 1.0),
                GeoPoint::new(1.0, 0.0),
            ],
        };
        assert!(square.contains(GeoPoint::new(0.5, 0.5)));
        assert!(!square.contains(GeoPoint::new(1.5, 0.5)));
        assert!(!square.contains(GeoPoint::new(0.5, -0.1)));
        // A concave L shape: the notch is outside.
        let l = Area::Polygon {
            points: vec![
                GeoPoint::new(0.0, 0.0),
                GeoPoint::new(0.0, 2.0),
                GeoPoint::new(1.0, 2.0),
                GeoPoint::new(1.0, 1.0),
                GeoPoint::new(2.0, 1.0),
                GeoPoint::new(2.0, 0.0),
            ],
        };
        assert!(l.contains(GeoPoint::new(1.5, 0.5)));
        assert!(!l.contains(GeoPoint::new(1.5, 1.5)));
    }

    #[test]
    fn audience_reaches_its_side_and_always_the_white_cell() {
        assert!(Audience::Side { side: 1 }.reaches(Some(1)));
        assert!(!Audience::Side { side: 1 }.reaches(Some(2)));
        assert!(Audience::Side { side: 1 }.reaches(None));
        assert!(Audience::All.reaches(Some(2)));
        assert!(!Audience::WhiteCell.reaches(Some(1)));
        assert!(Audience::WhiteCell.reaches(None));
    }

    #[test]
    fn validation_catches_bad_events() {
        let ok = event("a", Trigger::Time { at_s: 60.0 });
        assert!(validate_events(std::slice::from_ref(&ok), &[]).is_ok());
        assert!(validate_events(&[ok.clone(), ok.clone()], &[])
            .unwrap_err()
            .contains("duplicate"));
        let mut bad = ok.clone();
        bad.conditions
            .push(Condition::EventFired { event: "x".into() });
        assert!(validate_events(&[bad], &[])
            .unwrap_err()
            .contains("unknown event"));
        let mut bad = ok.clone();
        bad.conditions.push(Condition::Probability { p: 1.5 });
        assert!(validate_events(&[bad], &[]).is_err());
        let mut bad = ok.clone();
        bad.actions.clear();
        assert!(validate_events(&[bad], &[])
            .unwrap_err()
            .contains("no actions"));
        let bad = event(
            "a",
            Trigger::Every {
                period_s: 0.0,
                start_s: 0.0,
            },
        );
        assert!(validate_events(&[bad], &[]).is_err());
        let bad = event(
            "a",
            Trigger::AreaEntered {
                area: Area::Polygon {
                    points: vec![GeoPoint::new(0.0, 0.0)],
                },
                side: None,
                unit: None,
            },
        );
        assert!(validate_events(&[bad], &[])
            .unwrap_err()
            .contains("polygon"));
    }

    #[test]
    fn injects_are_checked_and_referenced() {
        let inject = Inject {
            id: "i1".into(),
            title: "Distress call".into(),
            from: String::new(),
            to: Audience::Side { side: 1 },
            text: "MV Ember reports engine failure".into(),
            note: String::new(),
        };
        let mut e = event("a", Trigger::Time { at_s: 1.0 });
        e.actions = vec![EventAction::SendInject {
            inject: "i1".into(),
        }];
        assert!(validate_events(std::slice::from_ref(&e), std::slice::from_ref(&inject)).is_ok());
        assert!(validate_events(std::slice::from_ref(&e), &[])
            .unwrap_err()
            .contains("unknown inject"));
        assert!(validate_events(&[], &[inject.clone(), inject.clone()])
            .unwrap_err()
            .contains("duplicate inject"));
        let json = r#"{"id":"i1","title":"Distress call","to":"side","side":1,
                       "text":"MV Ember reports engine failure"}"#;
        assert_eq!(serde_json::from_str::<Inject>(json).unwrap(), inject);
    }

    #[test]
    fn json_form() {
        let text = r#"{"id":"warn","trigger":{"type":"time","at_s":600},
            "conditions":[{"type":"units_alive","side":2,"cmp":"ge","count":1}],
            "actions":[{"type":"message","to":"side","side":1,"text":"Hostile group sighted"},
                       {"type":"set_posture","side":1,"toward":2,"posture":"hostile"}]}"#;
        let e: ScenarioEvent = serde_json::from_str(text).unwrap();
        assert!(e.enabled && !e.repeatable);
        assert_eq!(
            e.actions[0],
            EventAction::Message {
                to: Audience::Side { side: 1 },
                text: "Hostile group sighted".into()
            }
        );
        let back: ScenarioEvent =
            serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }
}
