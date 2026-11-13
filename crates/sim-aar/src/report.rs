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

/// One choke-point transit, assessed (`docs/models/choke-points.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TransitRow {
    pub tick: Tick,
    pub choke_point: String,
    pub unit: UnitId,
    pub unit_name: String,
    pub side: Option<SideId>,
    /// The lane the unit was in, if any.
    pub lane: Option<String>,
    /// The choke point's lanes' traffic, ships per day.
    pub traffic_per_day: f64,
    /// Water under the unit as it entered (terrain packs), and its draft.
    pub depth_m: Option<f64>,
    pub draft_m: Option<f64>,
    pub risk: sim_core::TransitRisk,
}

/// One sortie (`docs/models/air-operations.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SortieRow {
    pub launched: Tick,
    pub unit: UnitId,
    pub unit_name: String,
    pub side: Option<SideId>,
    pub base: String,
    pub mission: String,
    pub loadout: Option<String>,
    /// When it landed or was lost (`None`: still flying at the end).
    pub ended: Option<Tick>,
    /// Recovered, lost (and why), or airborne at the end.
    pub outcome: String,
    /// Fuel taken from tankers, t.
    pub tanker_fuel_t: f64,
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
    /// Air operations: sorties, recoveries, bingo, tanking.
    Air,
    /// Automatic sense-zone tasking: sensors sent, goals short or met.
    Tasking,
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
    /// Choke-point transits with lanes or terrain packs (older reports: none).
    #[serde(default)]
    pub transits: Vec<TransitRow>,
    /// Sorties flown (older reports: none).
    #[serde(default)]
    pub sorties: Vec<SortieRow>,
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
    /// Hulls in a collision, with its tick and the other hull (a loss that tick is the
    /// collision's, not the last weapon's).
    collided: BTreeMap<UnitId, (Tick, UnitId)>,
    /// Aircraft lost other than to a weapon (out of fuel, with their carrier), and why.
    air_lost: BTreeMap<UnitId, (Tick, String)>,
    /// Airbase names, from the planning layer in the log.
    bases: BTreeMap<u32, String>,
    /// Zone names and sides, from the planning layer in the log.
    zones: BTreeMap<u32, (String, SideId)>,
    sorties: Vec<SortieRow>,
    /// The open sortie of each aircraft in the air, by index in `sorties`.
    flying: BTreeMap<UnitId, usize>,
    intercepted: BTreeSet<u64>,
    losses: Vec<LossRow>,
    open_contacts: BTreeMap<(SideId, u32), usize>,
    detections: Vec<DetectionRow>,
    transits: Vec<TransitRow>,
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
            collided: BTreeMap::new(),
            air_lost: BTreeMap::new(),
            bases: BTreeMap::new(),
            zones: BTreeMap::new(),
            sorties: Vec::new(),
            flying: BTreeMap::new(),
            intercepted: BTreeSet::new(),
            losses: Vec::new(),
            open_contacts: BTreeMap::new(),
            detections: Vec::new(),
            transits: Vec::new(),
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
                CommandPayload::SetPlanning {
                    facilities, zones, ..
                } => {
                    for f in facilities {
                        b.bases.insert(f.id, f.name.clone());
                    }
                    for z in zones {
                        b.zones.insert(z.id, (z.name.clone(), z.side));
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
            CommandPayload::SetWeather { zones } => match zones.len() {
                0 => "cleared the weather zones".into(),
                n => format!(
                    "set {n} weather zone{} ({})",
                    if n == 1 { "" } else { "s" },
                    zones
                        .iter()
                        .map(|z| format!("{}: sea state {}", z.name, z.sea_state))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            },
            CommandPayload::SetSoundProfiles { profiles } => {
                format!("set {} sound-velocity profile(s)", profiles.len())
            }
            CommandPayload::StreamTowedArray { unit, streamed } => format!(
                "{}: {} the towed array",
                self.unit_name(*unit),
                if *streamed { "stream" } else { "recover" }
            ),
            CommandPayload::LaunchSortie {
                unit,
                mission,
                loadout,
            } => format!(
                "launched {} on a {} sortie{}",
                n(unit),
                sim_core::mission_name(mission),
                loadout
                    .as_deref()
                    .map(|l| format!(" with {l}"))
                    .unwrap_or_default()
            ),
            CommandPayload::RecallAircraft { unit, .. } => format!("recalled {}", n(unit)),
            CommandPayload::SetReadiness {
                unit, readiness, ..
            } => format!(
                "set {} to {}",
                n(unit),
                sim_core::readiness_text(*readiness)
            ),
            CommandPayload::LaySonobuoys { unit, points, .. } => format!(
                "{}: lay {} sonobuoy(s)",
                self.unit_name(*unit),
                points.len()
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
            CommandPayload::SetPlanning {
                facilities, zones, ..
            } => format!(
                "set the planning layer ({} facilities, {} zones)",
                facilities.len(),
                zones.len()
            ),
            CommandPayload::AssignGroupMission { group, mission } => match mission {
                Some(m) => format!("{group}: {}", mission_name(m)),
                None => format!("{group}: mission cleared"),
            },
            CommandPayload::DetachFromGroup { unit } => {
                format!("{} detached from its group", self.unit_name(*unit))
            }
            CommandPayload::JoinGroup { unit, group, .. } => {
                format!("{} joined {group}", self.unit_name(*unit))
            }
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
        if let Some((at, why)) = self.air_lost.remove(&unit) {
            if at == tick {
                return (why, None);
            }
        }
        if let Some((at, other)) = self.collided.remove(&unit) {
            if at == tick {
                return (format!("collision with {}", self.unit_name(other)), None);
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
                if let Some(i) = self.flying.remove(unit) {
                    self.sorties[i].ended = Some(t);
                    self.sorties[i].outcome = format!("lost: {cause}");
                }
                self.loss(t, *unit, false, cause, by);
            }
            SimEventKind::Collision {
                a,
                b,
                closing_mps,
                damage_fraction_a,
                damage_fraction_b,
            } => {
                // Collision damage follows; it is nobody's weapon hit.
                self.pending_hit = None;
                self.collided.insert(*a, (t, *b));
                self.collided.insert(*b, (t, *a));
                let (na, nb) = (self.unit_name(*a), self.unit_name(*b));
                let kn = closing_mps * 3600.0 / 1852.0;
                let text = format!(
                    "{na} and {nb} collided at {kn:.0} kn closing ({:.0} % and {:.0} % of their damage points)",
                    damage_fraction_a * 100.0,
                    damage_fraction_b * 100.0
                );
                self.note(t, TimelineKind::Damage, self.unit_side(*a), text);
            }
            SimEventKind::SortieLaunched {
                unit,
                base,
                loadout,
                mission,
            } => {
                let name = self.unit_name(*unit);
                let base_name = self.base_name(*base);
                self.flying.insert(*unit, self.sorties.len());
                self.sorties.push(SortieRow {
                    launched: t,
                    unit: *unit,
                    unit_name: name.clone(),
                    side: self.unit_side(*unit),
                    base: base_name.clone(),
                    mission: mission.clone(),
                    loadout: loadout.clone(),
                    ended: None,
                    outcome: String::new(),
                    tanker_fuel_t: 0.0,
                });
                let with = loadout
                    .as_deref()
                    .map(|l| format!(" with {l}"))
                    .unwrap_or_default();
                self.note(
                    t,
                    TimelineKind::Air,
                    self.unit_side(*unit),
                    format!("{name} took off from {base_name} on a {mission} sortie{with}"),
                );
            }
            SimEventKind::AircraftReturning { unit, base, reason } => {
                let text = format!(
                    "{} returning to {}: {reason}",
                    self.unit_name(*unit),
                    self.base_name(*base)
                );
                self.note(t, TimelineKind::Air, self.unit_side(*unit), text);
            }
            SimEventKind::AircraftRecovered {
                unit, base, fuel_t, ..
            } => {
                let base_name = self.base_name(*base);
                if let Some(i) = self.flying.remove(unit) {
                    self.sorties[i].ended = Some(t);
                    self.sorties[i].outcome = format!("recovered at {base_name}");
                }
                let text = format!(
                    "{} landed at {base_name} and took {fuel_t:.1} t of fuel",
                    self.unit_name(*unit)
                );
                self.note(t, TimelineKind::Air, self.unit_side(*unit), text);
            }
            SimEventKind::BingoFuel { unit, tanker, .. } => {
                let to = tanker.map_or_else(
                    || "for home".to_owned(),
                    |k| format!("for the tanker {}", self.unit_name(k)),
                );
                let text = format!("{} at bingo fuel, heading {to}", self.unit_name(*unit));
                self.note(t, TimelineKind::Air, self.unit_side(*unit), text);
            }
            SimEventKind::TankingComplete {
                receiver,
                tanker,
                taken_t,
                ..
            } => {
                if let Some(i) = self.flying.get(receiver) {
                    self.sorties[*i].tanker_fuel_t += taken_t;
                }
                let text = format!(
                    "{} took {taken_t:.1} t from {}",
                    self.unit_name(*receiver),
                    self.unit_name(*tanker)
                );
                self.note(t, TimelineKind::Air, self.unit_side(*receiver), text);
            }
            SimEventKind::SensorTasked {
                zone,
                unit,
                coverage,
                projected,
                ..
            } => {
                let (name, side) = self.zone_of(*zone);
                let text = format!(
                    "{} tasked to watch {name} (coverage {:.0} % → {:.0} %)",
                    self.unit_name(*unit),
                    coverage * 100.0,
                    projected * 100.0
                );
                self.note(t, TimelineKind::Tasking, side, text);
            }
            SimEventKind::CoverageShortfall {
                zone,
                coverage,
                goal,
            } => {
                let (name, side) = self.zone_of(*zone);
                let text = format!(
                    "{name} short of its coverage goal ({:.0} % of {:.0} %): no sensor left to send",
                    coverage * 100.0,
                    goal * 100.0
                );
                self.note(t, TimelineKind::Tasking, side, text);
            }
            SimEventKind::CoverageGoalMet { zone, coverage } => {
                let (name, side) = self.zone_of(*zone);
                let text = format!(
                    "{name} back at its coverage goal ({:.0} %)",
                    coverage * 100.0
                );
                self.note(t, TimelineKind::Tasking, side, text);
            }
            SimEventKind::AircraftLost { unit, reason } => {
                self.air_lost.insert(*unit, (t, reason.clone()));
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
            SimEventKind::ZoneAlert {
                side,
                zone,
                contact,
            } => {
                let name = self.side_name(*side);
                self.note(
                    t,
                    TimelineKind::Contact,
                    Some(*side),
                    format!("{name} alert: C{contact} in sense zone {zone}"),
                );
            }
            SimEventKind::UnitRepaired { unit, facility } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} repaired at facility {facility}"),
                );
            }
            SimEventKind::UnitRearmed { unit, facility } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} rearmed at facility {facility}"),
                );
            }
            SimEventKind::CommandPostDown { facility } => self.note(
                t,
                TimelineKind::Event,
                None,
                format!("command post {facility} is down"),
            ),
            SimEventKind::CommandPostDisrupted { facility, status } => self.note(
                t,
                TimelineKind::Event,
                None,
                format!("command post {facility} is down: {}", status.describe()),
            ),
            SimEventKind::MissionMinesLaid { unit, field, mines } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Mine,
                    self.unit_side(*unit),
                    format!("{name} laid {mines} mines (field {field})"),
                );
            }
            SimEventKind::MissionFailed { unit, reason } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name}'s mission failed: {reason}"),
                );
            }
            SimEventKind::MineSwept {
                unit,
                field,
                swept: true,
                ..
            } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Mine,
                    self.unit_side(*unit),
                    format!("{name} swept a mine of field {field}"),
                );
            }
            SimEventKind::CargoLoaded { unit, tonnes } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} loaded {tonnes:.0} t of cargo"),
                );
            }
            SimEventKind::CargoDelivered { unit, tonnes } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} delivered {tonnes:.0} t of cargo"),
                );
            }
            SimEventKind::ShelteringFromWeather { unit, facility } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} sheltering from the weather at facility {facility}"),
                );
            }
            SimEventKind::WeatherEased { unit } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} leaves shelter: the weather has eased"),
                );
            }
            SimEventKind::HoldingAtAnchorage { unit, facility } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} holding at anchorage {facility}"),
                );
            }
            SimEventKind::GroupLeaderChanged { group, leader } => {
                let name = self.unit_name(*leader);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*leader),
                    format!("{name} now leads {group}"),
                );
            }
            SimEventKind::GroupDissolved { group } => self.note(
                t,
                TimelineKind::Event,
                None,
                format!("{group} dissolved: no units left"),
            ),
            SimEventKind::UnitDetached { unit, group } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} detached from {group}"),
                );
            }
            SimEventKind::UnitJoined { unit, group } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} joined {group}"),
                );
            }
            SimEventKind::WeaponDetected {
                side,
                weapon_id,
                sensor,
                ..
            } => {
                let name = self.side_name(*side);
                let what = self
                    .launches
                    .get(weapon_id)
                    .map_or_else(|| "a weapon".to_owned(), |c| c.weapon.clone());
                self.note(
                    t,
                    TimelineKind::Contact,
                    Some(*side),
                    format!("{name} detected {what} W{weapon_id} in flight ({sensor:?})"),
                );
            }
            SimEventKind::FuelExhausted { unit } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} ran out of fuel"),
                );
            }
            SimEventKind::DivertedForFuel { unit, facility } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} diverted to facility {facility} to refuel"),
                );
            }
            SimEventKind::UnitRefuelled { unit, facility } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} refuelled at facility {facility}"),
                );
            }
            SimEventKind::WaitingForBerth { unit, facility } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} waiting for a berth at facility {facility}"),
                );
            }
            SimEventKind::CommandPostRestored { facility } => self.note(
                t,
                TimelineKind::Event,
                None,
                format!("command post {facility} is back up"),
            ),
            SimEventKind::ZoneEngage {
                zone,
                unit,
                contact,
                weapon,
            } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Launch,
                    self.unit_side(*unit),
                    format!("{name} engaged C{contact} with {weapon} in engage zone {zone}"),
                );
            }
            SimEventKind::TransitAssessed {
                choke_point,
                unit,
                lane,
                traffic_per_day,
                depth_m,
                draft_m,
                risk,
            } => {
                let name = self.unit_name(*unit);
                let risk_word = match risk {
                    sim_core::TransitRisk::Low => "low",
                    sim_core::TransitRisk::Moderate => "moderate",
                    sim_core::TransitRisk::High => "high",
                };
                let lane_text = lane
                    .as_ref()
                    .map_or_else(String::new, |l| format!(" in lane {l}"));
                let water = match (depth_m, draft_m) {
                    (Some(d), Some(k)) => format!(", {d:.0} m of water for a {k:.1} m draft"),
                    (Some(d), None) => format!(", {d:.0} m of water"),
                    _ => String::new(),
                };
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!(
                        "{name}: transit of {choke_point}{lane_text}, {traffic_per_day:.0} ships a \
                         day{water}: {risk_word} risk"
                    ),
                );
                self.transits.push(TransitRow {
                    tick: t,
                    choke_point: choke_point.clone(),
                    unit: *unit,
                    unit_name: name,
                    side: self.unit_side(*unit),
                    lane: lane.clone(),
                    traffic_per_day: *traffic_per_day,
                    depth_m: *depth_m,
                    draft_m: *draft_m,
                    risk: *risk,
                });
            }
            SimEventKind::ChokePointTransit { choke_point, unit } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} entered choke point {choke_point}"),
                );
            }
            SimEventKind::DivertedToPort { unit, facility } => {
                let name = self.unit_name(*unit);
                self.note(
                    t,
                    TimelineKind::Event,
                    self.unit_side(*unit),
                    format!("{name} diverted to facility {facility} for repair"),
                );
            }
            SimEventKind::QueuedOrderApplied { seq } => {
                if let Some(c) = self.commands.get(seq).copied() {
                    let who = self.issuer(&c.command.issuer);
                    let what = self.describe(&c.command.payload);
                    self.note(
                        t,
                        TimelineKind::Order,
                        c.command.side,
                        format!("{who}: {what} (released by the alternate command post)"),
                    );
                }
            }
            SimEventKind::QueuedOrderDropped { seq, reason } => {
                if let Some(c) = self.commands.get(seq).copied() {
                    let who = self.issuer(&c.command.issuer);
                    let what = self.describe(&c.command.payload);
                    self.note(
                        t,
                        TimelineKind::Refused,
                        c.command.side,
                        format!("{who}: {what} (queued order dropped: {reason})"),
                    );
                }
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

    fn zone_of(&self, id: u32) -> (String, Option<SideId>) {
        self.zones.get(&id).map_or_else(
            || (format!("zone {id}"), None),
            |(n, s)| (n.clone(), Some(*s)),
        )
    }

    fn base_name(&self, id: u32) -> String {
        self.bases
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("airbase {id}"))
    }

    fn finish(mut self, end: Tick) -> AarReport {
        for i in std::mem::take(&mut self.flying).into_values() {
            self.sorties[i].outcome = "airborne at the end".into();
        }
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
            transits: self.transits,
            sorties: self.sorties,
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
        sim_core::Mission::Support { .. } => "support",
        sim_core::Mission::Mining { .. } => "mining",
        sim_core::Mission::MineClearing { .. } => "mine clearing",
        sim_core::Mission::Cargo { .. } => "cargo",
        sim_core::Mission::BarrierPatrol { .. } => "barrier patrol",
        sim_core::Mission::Denial { .. } => "denial",
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
