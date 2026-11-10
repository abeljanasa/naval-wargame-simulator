//! After-action review tables, built from the command log and the event log alone.

use serde::{Deserialize, Serialize};
use sim_core::{
    Adjudication, Audience, CommandPayload, DoctrineScope, Facilitation, Issuer, LoggedCommand,
    SeatRole, SideId, SimEvent, SimEventKind, Tick, UnitId,
};
use std::collections::{BTreeMap, BTreeSet};

/// One side's totals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SideSummary {
    pub side: SideId,
    pub name: String,
    /// Units on the board at tick 0.
    pub units_start: u32,
    /// Units that came in later (reserves, white-cell spawns).
    pub reinforcements: u32,
    pub destroyed: u32,
    /// Taken out of the exercise by the white cell (not counted as destroyed).
    pub removed: u32,
    pub remaining: u32,
    /// Strike weapons fired (missiles, torpedoes, guns, mines struck).
    pub weapons_fired: u32,
    pub hits: u32,
    /// Enemy units destroyed with this side's weapons.
    pub kills: u32,
}

/// One unit lost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LossRow {
    pub tick: Tick,
    pub unit: UnitId,
    pub name: String,
    pub side: SideId,
    pub side_name: String,
    pub platform: Option<String>,
    /// Removed by the white cell rather than destroyed.
    pub removed: bool,
    /// What did it: the weapon and who fired it, or the white cell's reason.
    pub cause: String,
    /// The side credited with the kill, if any.
    pub by_side: Option<SideId>,
}

/// Weapons expended against effective, for one weapon type of one side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WeaponRow {
    pub side: SideId,
    pub side_name: String,
    pub weapon: String,
    /// Fired in defence (an interceptor at an incoming weapon) rather than at a unit.
    pub defensive: bool,
    pub fired: u32,
    /// Hit rolls won (for interceptors: threats killed).
    pub hits: u32,
    /// Hit rolls lost, or no hit for another reason (lost track, seeker, fuel, adjudicated).
    pub missed: u32,
    /// Shot down by the target side's point defence.
    pub intercepted: u32,
    /// Units destroyed with this as the last weapon to hit them.
    pub kills: u32,
}

/// One contact held by one side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DetectionRow {
    pub side: SideId,
    pub side_name: String,
    pub contact: u32,
    pub target: UnitId,
    pub target_name: String,
    pub target_side: Option<SideId>,
    pub gained: Tick,
    /// `None`: still held at the end.
    pub lost: Option<Tick>,
    /// Seconds held (to the end of the record if never lost).
    pub held_s: f64,
}

/// What a timeline entry is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum TimelineKind {
    Order,
    Refused,
    Contact,
    Launch,
    Hit,
    Miss,
    Intercept,
    Damage,
    Loss,
    Mine,
    Override,
    Facilitation,
    Event,
    ScriptError,
}

/// One line of the exercise timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TimelineEntry {
    pub tick: Tick,
    pub kind: TimelineKind,
    /// The side it concerns (`None`: exercise control).
    pub side: Option<SideId>,
    pub text: String,
}

/// The after-action review of one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AarReport {
    /// The tick the logs run to.
    pub end_tick: Tick,
    pub sides: Vec<SideSummary>,
    pub losses: Vec<LossRow>,
    pub weapons: Vec<WeaponRow>,
    pub detections: Vec<DetectionRow>,
    pub timeline: Vec<TimelineEntry>,
}

#[derive(Debug, Clone)]
struct UnitInfo {
    name: String,
    side: SideId,
    platform: Option<String>,
}

/// Who a hit is credited to.
#[derive(Debug, Clone)]
struct Credit {
    side: SideId,
    weapon: String,
    shooter: Option<UnitId>,
}

struct Builder<'a> {
    side_names: BTreeMap<SideId, String>,
    units: BTreeMap<UnitId, UnitInfo>,
    seats: BTreeMap<u32, String>,
    commands: BTreeMap<u64, &'a LoggedCommand>,
    sides: BTreeMap<SideId, SideSummary>,
    weapons: BTreeMap<(SideId, String, bool), WeaponRow>,
    /// Missile and torpedo launches by weapon id.
    launches: BTreeMap<u64, Credit>,
    /// The credit for the hit being resolved now (consumed by the next `DamageTaken`).
    pending_hit: Option<Credit>,
    last_hit: BTreeMap<UnitId, Credit>,
    /// White-cell overrides that can destroy a unit, with their tick and reason.
    adjudicated: BTreeMap<UnitId, (Tick, String)>,
    intercepted: BTreeSet<u64>,
    losses: Vec<LossRow>,
    open_contacts: BTreeMap<(SideId, u32), usize>,
    detections: Vec<DetectionRow>,
    timeline: Vec<TimelineEntry>,
}

impl<'a> Builder<'a> {
    fn new(commands: &'a [LoggedCommand], sides: &[(SideId, String)]) -> Self {
        let mut b = Self {
            side_names: sides.iter().cloned().collect(),
            units: BTreeMap::new(),
            seats: BTreeMap::new(),
            commands: commands.iter().map(|c| (c.seq, c)).collect(),
            sides: BTreeMap::new(),
            weapons: BTreeMap::new(),
            launches: BTreeMap::new(),
            pending_hit: None,
            last_hit: BTreeMap::new(),
            adjudicated: BTreeMap::new(),
            intercepted: BTreeSet::new(),
            losses: Vec::new(),
            open_contacts: BTreeMap::new(),
            detections: Vec::new(),
            timeline: Vec::new(),
        };
        // Every unit that could appear, and the seat names, from the log (by sequence number).
        for c in b.commands.values() {
            match &c.command.payload {
                CommandPayload::SpawnUnit(s) => {
                    b.units.insert(s.id, info(s));
                }
                CommandPayload::SetEvents { reserves, .. } => {
                    for s in reserves {
                        b.units.insert(s.id, info(s));
                    }
                }
                CommandPayload::SetSeats { seats } => {
                    for s in seats {
                        b.seats.insert(s.id, s.name.clone());
                    }
                }
                _ => {}
            }
        }
        for (id, name) in &b.side_names {
            b.sides.insert(*id, summary(*id, name.clone()));
        }
        b
    }

    fn side_name(&self, side: SideId) -> String {
        self.side_names
            .get(&side)
            .cloned()
            .unwrap_or_else(|| side.to_string())
    }

    fn unit_name(&self, unit: UnitId) -> String {
        self.units
            .get(&unit)
            .map_or_else(|| unit.to_string(), |u| u.name.clone())
    }

    fn unit_side(&self, unit: UnitId) -> Option<SideId> {
        self.units.get(&unit).map(|u| u.side)
    }

    fn side_mut(&mut self, side: SideId) -> &mut SideSummary {
        let name = self.side_name(side);
        self.sides
            .entry(side)
            .or_insert_with(|| summary(side, name))
    }

    fn weapon_mut(&mut self, side: SideId, weapon: &str, defensive: bool) -> &mut WeaponRow {
        let side_name = self.side_name(side);
        self.weapons
            .entry((side, weapon.to_owned(), defensive))
            .or_insert_with(|| WeaponRow {
                side,
                side_name,
                weapon: weapon.to_owned(),
                defensive,
                fired: 0,
                hits: 0,
                missed: 0,
                intercepted: 0,
                kills: 0,
            })
    }

    fn note(&mut self, tick: Tick, kind: TimelineKind, side: Option<SideId>, text: String) {
        self.timeline.push(TimelineEntry {
            tick,
            kind,
            side,
            text,
        });
    }

    fn issuer(&self, issuer: &Issuer) -> String {
        match issuer {
            Issuer::Scenario => "Scenario".into(),
            Issuer::Player { seat } => self
                .seats
                .get(seat)
                .cloned()
                .unwrap_or_else(|| format!("Seat {seat}")),
            Issuer::WhiteCell { umpire } if umpire.is_empty() => "White cell".into(),
            Issuer::WhiteCell { umpire } => format!("White cell ({umpire})"),
            Issuer::Ai => "AI".into(),
            Issuer::Script => "Script".into(),
        }
    }

    fn describe(&self, p: &CommandPayload) -> String {
        let n = |u: &UnitId| self.unit_name(*u);
        match p {
            CommandPayload::SpawnUnit(s) => format!("brought in {}", s.name),
            CommandPayload::SetRoute { unit, waypoints } if waypoints.is_empty() => {
                format!("cleared the route of {}", n(unit))
            }
            CommandPayload::SetRoute { unit, waypoints } => {
                format!("routed {} ({} waypoints)", n(unit), waypoints.len())
            }
            CommandPayload::SetSpeed { unit, speed_mps } => {
                format!("set {} to {:.0} kn", n(unit), speed_mps / KNOT_MPS)
            }
            CommandPayload::SetPosture {
                side,
                toward,
                posture,
            } => format!(
                "set {} {:?} toward {}",
                self.side_name(*side),
                posture,
                self.side_name(*toward)
            ),
            CommandPayload::SetDoctrine { scope, .. } => match scope {
                DoctrineScope::Side { side } => {
                    format!("changed {} doctrine", self.side_name(*side))
                }
                DoctrineScope::Group { group, .. } => format!("changed doctrine of {group}"),
                DoctrineScope::Unit { unit } => format!("changed doctrine of {}", n(unit)),
            },
            CommandPayload::SetEnvironment { environment } => format!(
                "set sea state {} and weather loss {:.1} dB",
                environment.sea_state, environment.weather_loss_db
            ),
            CommandPayload::SetEmitterLibrary { .. } => "set the emitter library".into(),
            CommandPayload::SetWeaponCatalog { .. } => "set the weapon catalogue".into(),
            CommandPayload::Engage {
                shooter,
                contact,
                weapon,
                rounds,
            } => format!(
                "ordered {} to fire {rounds} × {weapon} at C{contact}",
                n(shooter)
            ),
            CommandPayload::Adjudicate { reason, action } => {
                format!("{}: {reason}", self.adjudication(action))
            }
            CommandPayload::SetEvents { events, .. } => {
                format!("set {} scenario events", events.len())
            }
            CommandPayload::Facilitate { action } => self.facilitation(action),
            CommandPayload::SetSeats { seats } => format!("set {} seats", seats.len()),
            CommandPayload::CreateMinefield {
                side,
                mines,
                weapon,
                ..
            } => format!(
                "created a {} minefield ({mines} × {weapon})",
                self.side_name(*side)
            ),
            CommandPayload::LayMines { unit, mines, .. } => {
                format!("ordered {} to lay {mines} mines", n(unit))
            }
            CommandPayload::AssignMission {
                unit,
                mission: None,
            } => {
                format!("cleared the mission of {}", n(unit))
            }
            CommandPayload::AssignMission {
                unit,
                mission: Some(m),
            } => format!("gave {} a {} mission", n(unit), mission_name(m)),
        }
    }

    fn adjudication(&self, a: &Adjudication) -> String {
        let n = |u: &UnitId| self.unit_name(*u);
        match a {
            Adjudication::ForceWeaponOutcome { weapon_id, hit } => format!(
                "forced weapon W{weapon_id} to {}",
                if *hit { "hit" } else { "miss" }
            ),
            Adjudication::SetDamage {
                unit,
                damage_dp,
                clear_effects,
            } => {
                if *clear_effects {
                    format!("repaired {} to {damage_dp:.0} DP", n(unit))
                } else {
                    format!("set {} damage to {damage_dp:.0} DP", n(unit))
                }
            }
            Adjudication::DestroyUnit { unit } => format!("destroyed {}", n(unit)),
            Adjudication::RemoveUnit { unit } => format!("removed {}", n(unit)),
            Adjudication::MoveUnit { unit, .. } => format!("moved {}", n(unit)),
            Adjudication::SetStores {
                unit,
                weapon,
                count,
            } => format!("set {} {weapon} to {count}", n(unit)),
        }
    }

    fn facilitation(&self, f: &Facilitation) -> String {
        match f {
            Facilitation::SendInject { inject } => format!("sent inject {inject}"),
            Facilitation::SendMessage { to, text } => {
                let to = match to {
                    Audience::All => "all sides".to_owned(),
                    Audience::Side { side } => self.side_name(SideId(*side)),
                    Audience::WhiteCell => "the white cell".to_owned(),
                };
                format!("messaged {to}: {text}")
            }
            Facilitation::FireEvent { event } => format!("fired event {event}"),
            Facilitation::SetEventEnabled { event, enabled } => format!(
                "turned event {event} {}",
                if *enabled { "on" } else { "off" }
            ),
        }
    }

    fn credit_kill(&mut self, tick: Tick, unit: UnitId) -> (String, Option<SideId>) {
        if let Some((at, reason)) = self.adjudicated.remove(&unit) {
            if at == tick {
                return (format!("white cell: {reason}"), None);
            }
        }
        match self.last_hit.get(&unit).cloned() {
            Some(c) => {
                let by = match c.shooter {
                    Some(s) => format!("{} ({})", self.unit_name(s), self.side_name(c.side)),
                    None => self.side_name(c.side),
                };
                let victim_side = self.unit_side(unit);
                if victim_side != Some(c.side) {
                    self.weapon_mut(c.side, &c.weapon, false).kills += 1;
                    self.side_mut(c.side).kills += 1;
                }
                (format!("{} from {by}", c.weapon), Some(c.side))
            }
            None => ("unknown".into(), None),
        }
    }

    fn loss(&mut self, tick: Tick, unit: UnitId, removed: bool, cause: String, by: Option<SideId>) {
        let info = self.units.get(&unit).cloned();
        let side = info.as_ref().map_or(SideId(0), |u| u.side);
        let s = self.side_mut(side);
        if removed {
            s.removed += 1;
        } else {
            s.destroyed += 1;
        }
        let side_name = self.side_name(side);
        let name = info
            .as_ref()
            .map_or_else(|| unit.to_string(), |u| u.name.clone());
        self.note(
            tick,
            TimelineKind::Loss,
            Some(side),
            if removed {
                format!("{name} removed ({cause})")
            } else {
                format!("{name} destroyed ({cause})")
            },
        );
        self.losses.push(LossRow {
            tick,
            unit,
            name,
            side,
            side_name,
            platform: info.and_then(|u| u.platform),
            removed,
            cause,
            by_side: by,
        });
    }

    #[allow(clippy::too_many_lines)]
    fn event(&mut self, e: &SimEvent) {
        let t = e.tick;
        match &e.kind {
            SimEventKind::UnitSpawned { unit } => {
                if let Some(side) = self.unit_side(*unit) {
                    let s = self.side_mut(side);
                    if t.0 == 0 {
                        s.units_start += 1;
                    } else {
                        s.reinforcements += 1;
                    }
                    if t.0 > 0 {
                        let name = self.unit_name(*unit);
                        self.note(
                            t,
                            TimelineKind::Event,
                            Some(side),
                            format!("{name} arrived"),
                        );
                    }
                }
            }
            SimEventKind::CommandApplied { seq } | SimEventKind::CommandRejected { seq, .. } => {
                let Some(c) = self.commands.get(seq).copied() else {
                    return;
                };
                let white = matches!(c.command.issuer, Issuer::WhiteCell { .. });
                if !white && !matches!(c.command.issuer, Issuer::Player { .. }) {
                    return; // scenario set-up
                }
                let who = self.issuer(&c.command.issuer);
                let what = self.describe(&c.command.payload);
                let side = if white { None } else { c.command.side };
                match &e.kind {
                    SimEventKind::CommandRejected { reason, .. } => self.note(
                        t,
                        TimelineKind::Refused,
                        side,
                        format!("{who}: {what} (refused: {reason})"),
                    ),
                    _ => {
                        let kind = match c.command.payload {
                            CommandPayload::Adjudicate { .. } => TimelineKind::Override,
                            CommandPayload::Facilitate { .. } => TimelineKind::Facilitation,
                            _ => TimelineKind::Order,
                        };
                        self.note(t, kind, side, format!("{who}: {what}"));
                    }
                }
            }
            SimEventKind::ContactGained {
                side,
                number,
                target,
            } => {
                let target_name = self.unit_name(*target);
                let target_side = self.unit_side(*target);
                self.open_contacts
                    .insert((*side, *number), self.detections.len());
                self.detections.push(DetectionRow {
                    side: *side,
                    side_name: self.side_name(*side),
                    contact: *number,
                    target: *target,
                    target_name: target_name.clone(),
                    target_side,
                    gained: t,
                    lost: None,
                    held_s: 0.0,
                });
                self.note(
                    t,
                    TimelineKind::Contact,
                    Some(*side),
                    format!("{} gained C{number} ({target_name})", self.side_name(*side)),
                );
            }
            SimEventKind::ContactLost {
                side,
                number,
                target,
            } => {
                if let Some(i) = self.open_contacts.remove(&(*side, *number)) {
                    if let Some(row) = self.detections.get_mut(i) {
                        row.lost = Some(t);
                    }
                }
                let target_name = self.unit_name(*target);
                self.note(
                    t,
                    TimelineKind::Contact,
                    Some(*side),
                    format!("{} lost C{number} ({target_name})", self.side_name(*side)),
                );
            }
            SimEventKind::WeaponLaunched {
                weapon_id,
                weapon,
                shooter,
                side,
                contact,
            } => {
                self.launches.insert(
                    *weapon_id,
                    Credit {
                        side: *side,
                        weapon: weapon.clone(),
                        shooter: Some(*shooter),
                    },
                );
                self.weapon_mut(*side, weapon, false).fired += 1;
                self.side_mut(*side).weapons_fired += 1;
                let shooter = self.unit_name(*shooter);
                self.note(
                    t,
                    TimelineKind::Launch,
                    Some(*side),
                    format!("{shooter} launched {weapon} (W{weapon_id}) at C{contact}"),
                );
            }
            SimEventKind::HitRoll {
                weapon_id,
                weapon,
                shooter,
                side,
                target,
                ph,
                draw,
                hit,
                ..
            } => {
                let credit = match weapon_id {
                    Some(id) => self.launches.get(id).cloned(),
                    None => {
                        let by = side.or_else(|| shooter.and_then(|s| self.unit_side(s)));
                        by.map(|side| Credit {
                            side,
                            weapon: weapon.clone(),
                            shooter: *shooter,
                        })
                    }
                };
                if let Some(c) = &credit {
                    if weapon_id.is_none() {
                        // Guns and mines are fired and resolved in the same moment.
                        self.weapon_mut(c.side, &c.weapon, false).fired += 1;
                        self.side_mut(c.side).weapons_fired += 1;
                    }
                    let row = self.weapon_mut(c.side, &c.weapon, false);
                    if *hit {
                        row.hits += 1;
                    } else {
                        row.missed += 1;
                    }
                    if *hit {
                        self.side_mut(c.side).hits += 1;
                    }
                }
                let target_name = self.unit_name(*target);
                let (kind, verb) = if *hit {
                    (TimelineKind::Hit, "hit")
                } else {
                    (TimelineKind::Miss, "missed")
                };
                self.note(
                    t,
                    kind,
                    credit.as_ref().map(|c| c.side),
                    format!("{weapon} {verb} {target_name} (Ph {ph:.2}, draw {draw:.2})"),
                );
                self.pending_hit = if *hit { credit } else { None };
            }
            SimEventKind::DamageTaken {
                unit,
                damage_dp,
                status,
                ..
            } => {
                if let Some(c) = self.pending_hit.take() {
                    self.last_hit.insert(*unit, c);
                }
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Damage,
                    self.unit_side(*unit),
                    format!("{name} took {damage_dp:.0} DP, now {status:?}"),
                );
            }
            SimEventKind::PointDefenceShot {
                defender,
                interceptor,
                ..
            } => {
                if let Some(side) = self.unit_side(*defender) {
                    self.weapon_mut(side, interceptor, true).fired += 1;
                }
            }
            SimEventKind::InterceptRoll {
                defender,
                interceptor,
                threat,
                killed,
                ..
            } => {
                let side = self.unit_side(*defender);
                if let Some(side) = side {
                    let row = self.weapon_mut(side, interceptor, true);
                    if *killed {
                        row.hits += 1;
                    } else {
                        row.missed += 1;
                    }
                }
                if *killed {
                    self.intercepted.insert(*threat);
                    if let Some(c) = self.launches.get(threat).cloned() {
                        self.weapon_mut(c.side, &c.weapon, false).intercepted += 1;
                    }
                    let name = self.unit_name(*defender);
                    self.note(
                        t,
                        TimelineKind::Intercept,
                        side,
                        format!("{name} shot down W{threat} with {interceptor}"),
                    );
                }
            }
            SimEventKind::WeaponMissed { weapon_id, reason } => {
                if self.intercepted.contains(weapon_id) {
                    return;
                }
                if let Some(c) = self.launches.get(weapon_id).cloned() {
                    self.weapon_mut(c.side, &c.weapon, false).missed += 1;
                    self.note(
                        t,
                        TimelineKind::Miss,
                        Some(c.side),
                        format!("{} W{weapon_id} missed: {reason}", c.weapon),
                    );
                }
            }
            SimEventKind::Adjudicated { reason, action, .. } => {
                match action {
                    Adjudication::DestroyUnit { unit } | Adjudication::SetDamage { unit, .. } => {
                        self.adjudicated.insert(*unit, (t, reason.clone()));
                    }
                    Adjudication::RemoveUnit { unit } => {
                        self.loss(t, *unit, true, format!("white cell: {reason}"), None);
                    }
                    Adjudication::ForceWeaponOutcome {
                        weapon_id,
                        hit: true,
                    } => {
                        // The forced hit's damage follows at once.
                        self.pending_hit = self.launches.get(weapon_id).cloned();
                        if let Some(c) = &self.pending_hit {
                            let (side, weapon) = (c.side, c.weapon.clone());
                            self.weapon_mut(side, &weapon, false).hits += 1;
                            self.side_mut(side).hits += 1;
                        }
                    }
                    _ => {}
                }
            }
            SimEventKind::UnitDestroyed { unit } => {
                let (cause, by) = self.credit_kill(t, *unit);
                self.loss(t, *unit, false, cause, by);
            }
            SimEventKind::MineEncounter {
                unit,
                encountered: true,
                ..
            } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Mine,
                    self.unit_side(*unit),
                    format!("{name} struck a mine"),
                );
            }
            SimEventKind::ScenarioEventFired { event } => {
                self.note(t, TimelineKind::Event, None, format!("event {event} fired"));
            }
            SimEventKind::ScenarioScriptFailed { event, reason, .. } => self.note(
                t,
                TimelineKind::ScriptError,
                None,
                format!("event {event}: script failed: {reason}"),
            ),
            SimEventKind::ScenarioActionFailed { event, reason, .. } => self.note(
                t,
                TimelineKind::ScriptError,
                None,
                format!("event {event}: action failed: {reason}"),
            ),
            _ => {}
        }
    }

    fn finish(mut self, end: Tick) -> AarReport {
        for row in &mut self.detections {
            let until = row.lost.unwrap_or(end);
            row.held_s = until.seconds() - row.gained.seconds();
        }
        let destroyed_or_removed: BTreeMap<SideId, u32> =
            self.losses.iter().fold(BTreeMap::new(), |mut m, l| {
                *m.entry(l.side).or_insert(0) += 1;
                m
            });
        let mut sides: Vec<SideSummary> = self.sides.into_values().collect();
        for s in &mut sides {
            let gone = destroyed_or_removed.get(&s.side).copied().unwrap_or(0);
            s.remaining = (s.units_start + s.reinforcements).saturating_sub(gone);
        }
        AarReport {
            end_tick: end,
            sides,
            losses: self.losses,
            weapons: self.weapons.into_values().collect(),
            detections: self.detections,
            timeline: self.timeline,
        }
    }
}

const KNOT_MPS: f64 = 1852.0 / 3600.0;

fn info(s: &sim_core::UnitSpawn) -> UnitInfo {
    UnitInfo {
        name: s.name.clone(),
        side: s.side,
        platform: s.platform.clone(),
    }
}

fn summary(side: SideId, name: String) -> SideSummary {
    SideSummary {
        side,
        name,
        units_start: 0,
        reinforcements: 0,
        destroyed: 0,
        removed: 0,
        remaining: 0,
        weapons_fired: 0,
        hits: 0,
        kills: 0,
    }
}

fn mission_name(m: &sim_core::Mission) -> &'static str {
    match m {
        sim_core::Mission::Patrol { .. } => "patrol",
        sim_core::Mission::Ferry { .. } => "ferry",
        sim_core::Mission::Escort { .. } => "escort",
        sim_core::Mission::Strike { .. } => "strike",
    }
}

impl AarReport {
    /// Builds the review from a run's logs. `sides` names the sides (in table order); sides not
    /// named appear under their id.
    #[must_use]
    pub fn build(
        commands: &[LoggedCommand],
        events: &[SimEvent],
        sides: &[(SideId, String)],
        end: Tick,
    ) -> Self {
        let mut b = Builder::new(commands, sides);
        for e in events {
            b.event(e);
        }
        b.finish(end)
    }

    /// Timeline entries a seat may see: everything for the white cell, else the side's own
    /// entries.
    #[must_use]
    pub fn timeline_for(&self, role: &SeatRole) -> Vec<TimelineEntry> {
        match role {
            SeatRole::WhiteCell => self.timeline.clone(),
            SeatRole::Side { side } => self
                .timeline
                .iter()
                .filter(|e| e.side == Some(*side))
                .cloned()
                .collect(),
        }
    }
}
