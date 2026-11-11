// Pure helpers for the scenario editor (docs/models/scenario-editor.md). The draft is the
// scenario JSON as plain data; the engine checks it (`editor_check`) and is the only judge of
// what is valid. Nothing here talks to the backend.

import type { Emcon, FacilityKind, GeoPoint, Mission, Posture, Wcs, ZoneKind } from "./bindings";
import templates from "./event-templates.json";
import { n } from "./format";

export const SCENARIO_FORMAT = "nws-scenario";
export const SCENARIO_FORMAT_VERSION = 5;

export interface DraftPosture {
  toward: number;
  posture: Posture;
}

export interface DraftSide {
  id: number;
  name: string;
  postures?: DraftPosture[];
  doctrine?: unknown;
}

export interface DraftUnit {
  id: number;
  name: string;
  side: number;
  platform?: string;
  domain?: string;
  position: GeoPoint;
  altitude_m?: number;
  heading_deg?: number;
  speed_mps?: number | null;
  max_speed_mps?: number | null;
  accel_mps2?: number | null;
  route?: GeoPoint[];
  group?: string;
  doctrine?: unknown;
  jammer?: unknown;
  mission?: Mission;
  reserve?: boolean;
}

export interface DraftMinefield {
  side: number;
  centre: GeoPoint;
  radius_m: number;
  mines: number;
  weapon: string;
}

export interface DraftReferencePoint {
  name: string;
  position: GeoPoint;
  side?: number;
}

export interface DraftServices {
  repair?: boolean;
  rearm?: boolean;
  refuel?: boolean;
}

export interface DraftFacility {
  id: number;
  name: string;
  side: number;
  kind: FacilityKind;
  position: GeoPoint;
  radius_m: number;
  range_m?: number | null;
  berths?: number;
  services?: DraftServices;
}

export interface DraftDoctrine {
  wcs?: Wcs | null;
  emcon?: Emcon | null;
  engage_unfriendly?: boolean | null;
}

export type DraftArea =
  | { shape: "circle"; centre: GeoPoint; radius_m: number }
  | { shape: "polygon"; points: GeoPoint[] };

export interface DraftZone {
  id: number;
  name: string;
  side: number;
  kind: ZoneKind;
  area: DraftArea;
  doctrine?: DraftDoctrine;
  choke_point?: string | null;
}

export interface DraftStation {
  unit: number;
  bearing_deg: number;
  range_m: number;
}

export interface DraftActionGroup {
  name: string;
  side: number;
  leader: number;
  members: DraftStation[];
}

export interface DraftEnvironment {
  sea_state?: number;
  weather_loss_db?: number;
  start_unix_s?: number;
  visibility_km?: number;
}

/** A scenario as the editor holds it. Fields the editor has no form for (groups, doctrine,
 * jammers, events, injects, seats) are carried through unchanged or edited as JSON. */
export interface DraftScenario {
  format: string;
  format_version: number;
  name: string;
  description?: string;
  seed: number;
  duration_s: number;
  sides: DraftSide[];
  groups?: unknown[];
  minefields?: DraftMinefield[];
  environment?: DraftEnvironment;
  units: DraftUnit[];
  events?: unknown[];
  injects?: unknown[];
  seats?: unknown[];
  reference_points?: DraftReferencePoint[];
  facilities?: DraftFacility[];
  zones?: DraftZone[];
  action_groups?: DraftActionGroup[];
  [other: string]: unknown;
}

/** A new scenario: two sides, no units, four hours. */
export function blankScenario(): DraftScenario {
  return {
    format: SCENARIO_FORMAT,
    format_version: SCENARIO_FORMAT_VERSION,
    name: "New scenario",
    description: "",
    seed: 1,
    duration_s: 4 * 3600,
    sides: [
      { id: 1, name: "Blue", postures: [{ toward: 2, posture: "hostile" }] },
      { id: 2, name: "Red", postures: [{ toward: 1, posture: "hostile" }] },
    ],
    units: [],
  };
}

/** A deep copy that the editor may change freely (drafts never share objects). */
export function cloneDraft(s: DraftScenario): DraftScenario {
  return JSON.parse(JSON.stringify(s)) as DraftScenario;
}

/** The draft as a scenario file, saved at the current format version. */
export function toJson(s: DraftScenario): string {
  return `${JSON.stringify({ ...s, format: SCENARIO_FORMAT, format_version: SCENARIO_FORMAT_VERSION }, null, 2)}\n`;
}

export function nextUnitId(s: DraftScenario): number {
  return s.units.reduce((m, u) => Math.max(m, u.id), 0) + 1;
}

export function nextSideId(s: DraftScenario): number {
  return s.sides.reduce((m, x) => Math.max(m, x.id), 0) + 1;
}

/** Adds a unit of `side` at `position` and returns its id. */
export function addUnit(
  s: DraftScenario,
  side: number,
  position: GeoPoint,
  platform: string | undefined,
): number {
  const id = nextUnitId(s);
  const sideName = s.sides.find((x) => x.id === side)?.name ?? `Side ${side}`;
  s.units.push({
    id,
    name: `${sideName} ${id}`,
    side,
    ...(platform ? { platform } : {}),
    position,
    heading_deg: 0,
    route: [],
  });
  return id;
}

/** A copy of unit `id`, offset slightly so both can be seen, with a new id. */
export function duplicateUnit(s: DraftScenario, id: number): number | null {
  const u = s.units.find((x) => x.id === id);
  if (!u) return null;
  const copy = JSON.parse(JSON.stringify(u)) as DraftUnit;
  copy.id = nextUnitId(s);
  copy.name = `${u.name} (copy)`;
  copy.position = {
    lat_deg: n(u.position.lat_deg) - 0.02,
    lon_deg: n(u.position.lon_deg) + 0.02,
  };
  s.units.push(copy);
  return copy.id;
}

export function removeUnit(s: DraftScenario, id: number): void {
  s.units = s.units.filter((u) => u.id !== id);
  // A group loses the unit; a group whose leader goes is dissolved.
  s.action_groups = (s.action_groups ?? [])
    .filter((g) => g.leader !== id)
    .map((g) => ({ ...g, members: g.members.filter((m) => m.unit !== id) }));
}

/** The default services of a new facility of `kind`. */
export function defaultServices(kind: FacilityKind): DraftServices {
  switch (kind) {
    case "sheltering_port":
      return { repair: true, rearm: true, refuel: true };
    case "refuelling_point":
      return { refuel: true };
    default:
      return {};
  }
}

/** Adds a facility of `side` at `position` and returns its id. */
export function addFacility(
  s: DraftScenario,
  side: number,
  kind: FacilityKind,
  position: GeoPoint,
): number {
  const id = (s.facilities ?? []).reduce((m, f) => Math.max(m, f.id), 0) + 1;
  const names: Record<FacilityKind, string> = {
    command_post: "Command post",
    refuelling_point: "Refuelling point",
    sheltering_port: "Port",
    anchorage: "Anchorage",
  };
  s.facilities = [
    ...(s.facilities ?? []),
    {
      id,
      name: `${names[kind]} ${id}`,
      side,
      kind,
      position,
      radius_m: kind === "command_post" ? 1_000 : 3_000,
      ...(kind === "command_post" ? { range_m: 200_000 } : {}),
      services: defaultServices(kind),
    },
  ];
  return id;
}

/** A choke point: a sense zone (radius `senseM`, an alert ring) round an engage zone (radius
 * `engageM`, weapons free against unfriendly contacts), both of `side`. Returns their ids. */
export function addChokePoint(
  s: DraftScenario,
  side: number,
  centre: GeoPoint,
  name: string,
  senseM = 40_000,
  engageM = 15_000,
): [number, number] {
  const next = (s.zones ?? []).reduce((m, z) => Math.max(m, z.id), 0) + 1;
  s.zones = [
    ...(s.zones ?? []),
    {
      id: next,
      name: `${name} sense zone`,
      side,
      kind: "sense",
      area: { shape: "circle", centre, radius_m: senseM },
      choke_point: name,
    },
    {
      id: next + 1,
      name: `${name} engage zone`,
      side,
      kind: "engage",
      area: { shape: "circle", centre, radius_m: engageM },
      doctrine: { wcs: "free", engage_unfriendly: true },
      choke_point: name,
    },
  ];
  return [next, next + 1];
}

export function addSide(s: DraftScenario, name: string): number {
  const id = nextSideId(s);
  s.sides.push({ id, name: name.trim() || `Side ${id}` });
  return id;
}

/** Removes a side and everything that belongs to it (units, minefields, postures toward it,
 * its reference points). Returns how many units went with it. */
export function removeSide(s: DraftScenario, id: number): number {
  const before = s.units.length;
  s.sides = s.sides.filter((x) => x.id !== id);
  for (const x of s.sides) x.postures = (x.postures ?? []).filter((p) => p.toward !== id);
  s.units = s.units.filter((u) => u.side !== id);
  s.minefields = (s.minefields ?? []).filter((m) => m.side !== id);
  s.reference_points = (s.reference_points ?? []).filter((r) => r.side !== id);
  s.facilities = (s.facilities ?? []).filter((f) => f.side !== id);
  s.zones = (s.zones ?? []).filter((z) => z.side !== id);
  s.action_groups = (s.action_groups ?? []).filter((g) => g.side !== id);
  return before - s.units.length;
}

/** How `side` regards `toward` (neutral when unset). */
export function postureOf(s: DraftScenario, side: number, toward: number): Posture {
  return (
    s.sides.find((x) => x.id === side)?.postures?.find((p) => p.toward === toward)?.posture ??
    "neutral"
  );
}

/** Sets how `side` regards `toward`; neutral is the default and is not written. */
export function setPosture(s: DraftScenario, side: number, toward: number, posture: Posture) {
  const x = s.sides.find((y) => y.id === side);
  if (!x || side === toward) return;
  const rest = (x.postures ?? []).filter((p) => p.toward !== toward);
  x.postures = posture === "neutral" ? rest : [...rest, { toward, posture }];
  x.postures.sort((a, b) => a.toward - b.toward);
}

/** Parses a JSON array typed into the editor (events, injects, seats). */
export function parseJsonArray(text: string): { value: unknown[] } | { error: string } {
  if (!text.trim()) return { value: [] };
  try {
    const v: unknown = JSON.parse(text);
    if (!Array.isArray(v)) return { error: "must be a JSON array: [ … ]" };
    return { value: v };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

/** Scenario events to start from. Each is valid as written, once its id is made unique
 * (checked against the engine by the desktop crate's `event_templates_are_valid` test). */
export const EVENT_TEMPLATES = templates as { label: string; event: { id: string } }[];

/** A unique id for a new event: `base`, then `base-2`, `base-3`… */
export function uniqueEventId(events: unknown[], base: string): string {
  const ids = new Set(
    events.map((e) => (e && typeof e === "object" && "id" in e ? String(e.id) : "")),
  );
  if (!ids.has(base)) return base;
  let i = 2;
  while (ids.has(`${base}-${i}`)) i += 1;
  return `${base}-${i}`;
}
