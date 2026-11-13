// Offline map: MapLibre draws the base layers (sea, bundled Natural Earth land, graticule) and
// deck.gl draws everything that moves. No network tiles are used.

import * as maplibregl from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
// MapLibre locates its worker relative to its own module URL, which Vite's dependency
// pre-bundling and Tauri's custom protocol both break. The worker file is self-contained, so
// ship it as a static asset and point MapLibre at it explicitly.
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import { MapboxOverlay } from "@deck.gl/mapbox";
import {
  IconLayer,
  LineLayer,
  PathLayer,
  PolygonLayer,
  ScatterplotLayer,
  TextLayer,
} from "@deck.gl/layers";
import type { Layer, PickingInfo } from "@deck.gl/core";
import type {
  BearingView,
  ContactView,
  Domain,
  GeoPoint,
  Lane_Deserialize as Lane,
  Minefield,
  Outcome,
  UnitView,
  Viewer,
  WeaponTrackView,
  WeaponView,
} from "./bindings";
import { n, project } from "./format";
import { AFFILIATION_COLOUR, functionCode, symbolUrl, type Affiliation } from "./symbols";
import {
  MUNITION_LABEL,
  figureCanvas,
  figureKind,
  figureShape,
  badgeUrl,
  figureUrl,
  figureSizeM,
  FIGURE_MIN_PX,
  FIGURE_MAX_PX,
  munitionKind,
  munitionUrl,
  type FigureKind,
} from "./silhouettes";

type LonLat = [number, number];

/** Above this many units on the map, unit name labels are decluttered (selected unit only). */
export const LABEL_LIMIT = 1500;

// Fixed glyph set for SDF labels. "auto" rescans every label on every update, which is costly at
// 10,000 units. Printable ASCII plus the punctuation used in names and contact labels.
const LABEL_CHARSET = [
  ...Array.from({ length: 95 }, (_, i) => String.fromCharCode(32 + i)),
  "—",
  "–",
  "·",
  "°",
  "′",
];
type RGBA = [number, number, number, number];
const rgba = (c: [number, number, number], a: number): RGBA => [c[0], c[1], c[2], a];

function graticule(stepDeg: number): GeoJSON.FeatureCollection {
  const features: GeoJSON.Feature[] = [];
  for (let lon = -180; lon <= 180; lon += stepDeg) {
    const coords: LonLat[] = [];
    for (let lat = -85; lat <= 85; lat += 1) coords.push([lon, lat]);
    features.push({
      type: "Feature",
      properties: {},
      geometry: { type: "LineString", coordinates: coords },
    });
  }
  for (let lat = -80; lat <= 80; lat += stepDeg) {
    const coords: LonLat[] = [];
    for (let lon = -180; lon <= 180; lon += 1) coords.push([lon, lat]);
    features.push({
      type: "Feature",
      properties: {},
      geometry: { type: "LineString", coordinates: coords },
    });
  }
  return { type: "FeatureCollection", features };
}

export function createMap(container: HTMLElement, center: LonLat) {
  maplibregl.setWorkerUrl(new URL(workerUrl, window.location.href).href);
  const landUrl = new URL("/basemap/ne_50m_land.geojson", window.location.href).href;
  const map = new maplibregl.Map({
    container,
    center,
    zoom: 6,
    attributionControl: {
      compact: true,
      customAttribution: "Coastline: Natural Earth (public domain)",
    },
    style: {
      version: 8,
      sources: {
        land: { type: "geojson", data: landUrl },
        grid: { type: "geojson", data: graticule(1) },
      },
      layers: [
        { id: "sea", type: "background", paint: { "background-color": "#0d2030" } },
        {
          id: "grid",
          type: "line",
          source: "grid",
          paint: { "line-color": "#1d3a50", "line-width": 0.6 },
        },
        { id: "land", type: "fill", source: "land", paint: { "fill-color": "#283139" } },
        {
          id: "coast",
          type: "line",
          source: "land",
          paint: { "line-color": "#7890a0", "line-width": 0.8 },
        },
      ],
    },
  });
  map.addControl(new maplibregl.NavigationControl({ visualizePitch: false }), "top-right");
  map.addControl(new maplibregl.ScaleControl({ unit: "nautical" }), "bottom-right");
  const overlay = new MapboxOverlay({ interleaved: false, layers: [] });
  map.addControl(overlay);
  return { map, overlay };
}

export type MapTheme = "dark" | "light";

const BASE_COLOURS: Record<MapTheme, { sea: string; grid: string; land: string; coast: string }> = {
  dark: { sea: "#0d2030", grid: "#1d3a50", land: "#283139", coast: "#7890a0" },
  light: { sea: "#cfe3ee", grid: "#b3cddb", land: "#f2efe6", coast: "#6d8394" },
};

/** Recolours the base layers for a theme (once the style has loaded). */
export function setMapTheme(map: maplibregl.Map, theme: MapTheme) {
  const apply = () => {
    const c = BASE_COLOURS[theme];
    map.setPaintProperty("sea", "background-color", c.sea);
    map.setPaintProperty("grid", "line-color", c.grid);
    map.setPaintProperty("land", "fill-color", c.land);
    map.setPaintProperty("coast", "line-color", c.coast);
  };
  if (map.isStyleLoaded()) apply();
  else map.once("load", apply);
}

export interface SceneInput {
  units: UnitView[];
  /** The viewing side's contacts (empty for the white cell, which sees ground truth). */
  contacts: ContactView[];
  /** The viewing side's ESM bearing lines. */
  bearings?: BearingView[];
  /** Missiles and torpedoes in flight that the viewer may see. */
  weapons?: WeaponView[];
  /** The side's tracks on other sides' weapons in flight (side viewers). */
  weaponTracks?: WeaponTrackView[];
  /** Minefields the viewer may see. */
  minefields?: Minefield[];
  gameTimeS: number;
  viewer: Viewer;
  selected: number | null;
  draft: GeoPoint[] | null;
  onPickUnit: (id: number) => void;
  /** Label keys hidden by `declutter` (none if absent). */
  hiddenLabels?: Set<string>;
  /** Unit figures (default) or APP-6 frames with a function code. */
  symbolStyle?: SymbolStyle;
}

export type SymbolStyle = "figures" | "frames";

/** How long an outcome marker stays on the map, and how long it is labelled, game seconds. */
export const OUTCOME_SHOW_S = 60;
export const OUTCOME_LABEL_S = 20;

/**
 * A one-line notice of a weapon outcome, worded for the viewer. `side` is the viewer's side
 * (`null` for the white cell). Side viewers never learn more than the outcome feed gives them.
 */
export function outcomeText(o: Outcome, side: number | null): string {
  const what = MUNITION_LABEL[munitionKind(o.weapon)];
  const cap = what.charAt(0).toUpperCase() + what.slice(1);
  const why = o.reason && o.result === "miss" ? ` (${o.reason})` : "";
  // Point defence names the defender in the reason ("shot down by …").
  const downBy = o.reason?.startsWith("shot down by ") ? ` was ${o.reason}` : " was shot down";
  if (side === null) {
    const from = o.shooter_name ? ` from ${o.shooter_name}` : "";
    const target = o.target_name ?? "its target";
    return o.result === "hit"
      ? `${cap}${from} hit ${target}`
      : o.result === "miss"
        ? `${cap}${from} missed ${target}${why}`
        : `${cap}${from} at ${target}${downBy}`;
  }
  if (o.shooter_side === side) {
    const from = o.shooter_name ? ` from ${o.shooter_name}` : "";
    const result =
      o.result === "hit"
        ? "hit its target"
        : o.result === "miss"
          ? `missed${why}`
          : "was shot down";
    return `Our ${what}${from} ${result}`;
  }
  const target = o.target_name ?? "One of our units";
  return o.result === "hit"
    ? `${target} was hit by an enemy ${what}`
    : o.result === "miss"
      ? `An enemy ${what} missed ${target}`
      : `An enemy ${what} aimed at ${target}${downBy}`;
}

/** Markers where outcomes happened, fading over OUTCOME_SHOW_S: a filled burst for a hit, a
 * pale ring for a miss, an amber ring for a weapon shot down; labelled for the first seconds. */
export function outcomeLayers(outcomes: Outcome[], gameTimeS: number): Layer[] {
  const live = outcomes.filter((o) => gameTimeS - n(o.tick) <= OUTCOME_SHOW_S);
  const fade = (o: Outcome) =>
    Math.round(255 * Math.max(0.15, 1 - (gameTimeS - n(o.tick)) / OUTCOME_SHOW_S));
  const colour = (o: Outcome): [number, number, number] =>
    o.result === "hit" ? [255, 90, 40] : o.result === "miss" ? [220, 230, 238] : [255, 200, 70];
  return [
    new ScatterplotLayer<Outcome>({
      id: "outcomes",
      data: live,
      getPosition: (o) => pt(o.position),
      getRadius: (o) => 9 + Math.min(10, (gameTimeS - n(o.tick)) / 3),
      radiusUnits: "pixels",
      stroked: true,
      filled: true,
      getFillColor: (o): RGBA =>
        o.result === "hit" ? [...colour(o), Math.round(fade(o) * 0.6)] : [0, 0, 0, 0],
      getLineColor: (o): RGBA => [...colour(o), fade(o)],
      lineWidthUnits: "pixels",
      getLineWidth: (o) => (o.result === "miss" ? 1.5 : 2.5),
      updateTriggers: {
        getRadius: [gameTimeS],
        getFillColor: [gameTimeS],
        getLineColor: [gameTimeS],
      },
    }),
    new TextLayer<Outcome>({
      id: "outcome-labels",
      data: live.filter((o) => gameTimeS - n(o.tick) <= OUTCOME_LABEL_S),
      getPosition: (o) => pt(o.position),
      getText: (o) => (o.result === "hit" ? "HIT" : o.result === "miss" ? "MISS" : "SHOT DOWN"),
      getSize: 13,
      getColor: (o): RGBA => [...colour(o), 255],
      getPixelOffset: [0, -24],
      fontFamily: "system-ui, sans-serif",
      fontWeight: 700,
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: LABEL_CHARSET,
    }),
  ];
}

/** A figure's size on the ground, metres (selected: a quarter larger); the layer clamps it on
 * screen to FIGURE_MIN_PX–FIGURE_MAX_PX, so it shrinks as the map zooms out. */
function figureSize(kind: FigureKind, selected: boolean): number {
  return figureSizeM(kind) * (selected ? 1.25 : 1);
}

/** Size of the affiliation badge drawn beside a figure's label, px. */
export const BADGE_PX = 13;

/** The affiliation badge: a small APP-6 frame, so the side reads by shape as well as colour. */
function badgeIcon(aff: Affiliation, domain: Domain) {
  return { id: `badge-${aff}-${domain}`, url: badgeUrl(aff, domain), width: 64, height: 64 };
}

/** Figure icon for a unit or contact, drawn nose up; `getAngle` turns it to the heading. */
function figureIcon(kind: FigureKind, aff: Affiliation, selected: boolean) {
  const [width, height] = figureCanvas(kind);
  return {
    id: `fig-${kind}-${aff}-${selected}`,
    url: figureUrl(kind, aff, selected),
    width,
    height,
  };
}

/** Deck rotates icons counter-clockwise in degrees; headings are clockwise from north. */
function iconAngle(kind: FigureKind | null, heading: number): number {
  return kind && figureShape(kind) === "topdown" ? -heading : 0;
}

/** Phase 1 has no postures (Phase 2, sim-c2): own side is shown as friend and every other side as unknown. */
function affiliation(u: UnitView, viewer: Viewer): Affiliation {
  const own = viewer.kind === "side" ? viewer.side : 1;
  return u.side === own ? "friend" : "unknown";
}

const pt = (g: GeoPoint): LonLat => [n(g.lon_deg), n(g.lat_deg)];

/** Scale from a one-sigma 2-D error ellipse to the 95 % containment ellipse: √χ²₂(0.95). */
export const CONTAINMENT_95 = 2.4477;

/** Outline of a contact's 95 % position-error ellipse, as a closed lon/lat ring. */
export function ellipseRing(c: ContactView, points = 48): LonLat[] {
  const lat = n(c.position.lat_deg);
  const lon = n(c.position.lon_deg);
  const a = n(c.error.semi_major_m) * CONTAINMENT_95;
  const b = n(c.error.semi_minor_m) * CONTAINMENT_95;
  const o = (n(c.error.orientation_deg) * Math.PI) / 180;
  // Major axis along bearing o: east/north unit vectors of the major and minor axes.
  const [ue, un] = [Math.sin(o), Math.cos(o)];
  const [ve, vn] = [Math.cos(o), -Math.sin(o)];
  const ring: LonLat[] = [];
  for (let i = 0; i <= points; i++) {
    const t = (2 * Math.PI * i) / points;
    const e = a * Math.cos(t) * ue + b * Math.sin(t) * ve;
    const nn = a * Math.cos(t) * un + b * Math.sin(t) * vn;
    const d = Math.hypot(e, nn);
    ring.push(d === 0 ? [lon, lat] : project(lat, lon, (Math.atan2(e, nn) * 180) / Math.PI, d));
  }
  return ring;
}

/** Symbol affiliation of a contact: unknown until the side identifies it to class level, then
 * the side's posture toward the contact's side (unfriendly is drawn as hostile). */
export function contactAffiliation(c: ContactView): Affiliation {
  switch (c.affiliation) {
    case "friendly":
      return "friend";
    case "neutral":
      return "neutral";
    case "unfriendly":
    case "hostile":
      return "hostile";
    default:
      return "unknown";
  }
}

/** A closed lon/lat ring of `radius_m` round `centre`. */
export function circleRing(centre: GeoPoint, radius_m: number, points = 64): LonLat[] {
  const ring: LonLat[] = [];
  for (let i = 0; i <= points; i++) {
    ring.push(project(n(centre.lat_deg), n(centre.lon_deg), (360 * i) / points, radius_m));
  }
  return ring;
}

/** What the side knows about a contact: its class, else category, else domain, else unknown. */
export function contactKind(c: ContactView): string {
  return c.platform ?? c.category?.replaceAll("_", " ") ?? c.domain ?? "unknown";
}

/** Length a bearing line is drawn on the map, metres: long enough to show the direction and to
 * cross another ship's line, short enough not to cut across the whole picture. */
export const BEARING_DRAW_M = 40_000;

/** How far another unit's route is drawn (its next leg only); the selected unit shows all of it. */
export const ROUTE_STUB_M = 15_000;

/**
 * Label decluttering: where two labels would overlap on screen, the lower priority is hidden
 * until the user zooms in. Selected unit, then units, contacts, facilities, minefields, zones,
 * reference points.
 */
export const LABEL_PRIORITY = {
  selected: 1000,
  unit: 500,
  contact: 400,
  facility: 300,
  minefield: 250,
  zone: 200,
  point: 100,
} as const;

/** A label to place on screen: its key, anchor, pixel offset, font size, text and priority. */
export interface LabelCandidate {
  key: string;
  at: LonLat;
  dy: number;
  size: number;
  text: string;
  priority: number;
}

/**
 * Keeps labels in priority order and hides any whose box would overlap one already kept, in
 * screen pixels. `project` maps a lon/lat to pixels (the map's projection). Box: about 0.58 em
 * per character, plus a little padding. Returns the keys to hide.
 */
export function declutter(
  candidates: LabelCandidate[],
  project: (p: LonLat) => [number, number],
): Set<string> {
  const kept: [number, number, number, number][] = [];
  const hidden = new Set<string>();
  const order = [...candidates].sort((a, b) => b.priority - a.priority);
  for (const c of order) {
    const [x, y0] = project(c.at);
    const y = y0 + c.dy;
    const w = c.text.length * c.size * 0.58 + 6;
    const h = c.size + 4;
    const box: [number, number, number, number] = [x - w / 2, y - h / 2, x + w / 2, y + h / 2];
    const clash = kept.some(
      (k) => box[0] < k[2] && box[2] > k[0] && box[1] < k[3] && box[3] > k[1],
    );
    if (clash) hidden.add(c.key);
    else kept.push(box);
  }
  return hidden;
}

export const unitLabel = (u: UnitView) => u.name;
export const contactLabel = (c: ContactView, gameTimeS: number) =>
  `C${c.number} · ${contactKind(c)} · ${contactAge(gameTimeS, c)}`;
export const minefieldLabel = (f: Minefield) => `M${f.id} · ${f.mines} mines`;
export const zoneLabel = (z: MapZone) => (z.layer ? `${z.name} (${z.layer})` : z.name);
export const facilityLabel = (f: MapFacility) => `${FACILITY_CODE[f.kind] ?? "?"} ${f.name}`;

/** Every label the picture would draw, as declutter candidates. */
export function labelCandidates(s: {
  units: UnitView[];
  contacts: ContactView[];
  minefields: Minefield[];
  facilities: MapFacility[];
  zones: MapZone[];
  points: MapPoint[];
  selected: number | null;
  gameTimeS: number;
}): LabelCandidate[] {
  const P = LABEL_PRIORITY;
  const unitsShown =
    s.units.length <= LABEL_LIMIT ? s.units : s.units.filter((u) => u.id === s.selected);
  return [
    ...unitsShown.map((u) => ({
      key: `u:${u.id}`,
      at: pt(u.position),
      dy: 24,
      size: 12,
      text: unitLabel(u),
      priority: u.id === s.selected ? P.selected : P.unit,
    })),
    ...s.contacts.map((c) => ({
      key: `c:${c.number}`,
      at: pt(c.position),
      dy: 20,
      size: 12,
      text: contactLabel(c, s.gameTimeS),
      priority: P.contact,
    })),
    ...s.facilities.map((f) => ({
      key: `f:${f.name}`,
      at: pt(f.position),
      dy: 0,
      size: 12,
      text: facilityLabel(f),
      priority: P.facility,
    })),
    ...s.minefields.map((f) => ({
      key: `m:${f.id}`,
      at: pt(f.centre),
      dy: 0,
      size: 12,
      text: minefieldLabel(f),
      priority: P.minefield,
    })),
    ...s.zones.map((z) => ({
      key: `z:${z.name}`,
      at: zoneAnchor(z),
      dy: -10,
      size: 11,
      text: zoneLabel(z),
      priority: P.zone,
    })),
    ...s.points.map((q) => ({
      key: `p:${q.name}`,
      at: pt(q.position),
      dy: -14,
      size: 12,
      text: q.name,
      priority: P.point,
    })),
  ];
}

/** Great-circle distance, metres (spherical; for drawing only). */
export function distanceM(a: GeoPoint, b: GeoPoint): number {
  const r = 6_371_000;
  const f1 = (n(a.lat_deg) * Math.PI) / 180;
  const f2 = (n(b.lat_deg) * Math.PI) / 180;
  const df = f2 - f1;
  const dl = ((n(b.lon_deg) - n(a.lon_deg)) * Math.PI) / 180;
  const h = Math.sin(df / 2) ** 2 + Math.cos(f1) * Math.cos(f2) * Math.sin(dl / 2) ** 2;
  return 2 * r * Math.asin(Math.min(1, Math.sqrt(h)));
}

/** At most `max_m` of the way from `a` toward `b` (straight in lon/lat; for drawing only). */
function clip(a: GeoPoint, b: GeoPoint, max_m: number): LonLat {
  const d = distanceM(a, b);
  if (d <= max_m) return pt(b);
  const t = max_m / d;
  return [
    n(a.lon_deg) + (n(b.lon_deg) - n(a.lon_deg)) * t,
    n(a.lat_deg) + (n(b.lat_deg) - n(a.lat_deg)) * t,
  ];
}

export function buildLayers(s: SceneInput): Layer[] {
  const figures = (s.symbolStyle ?? "figures") === "figures";
  const unitKind = (u: UnitView) => (figures ? figureKind(u.platform, null, u.domain) : null);
  /** A classified contact with a plan-view figure and an estimated course. */
  const flatContact = (c: ContactView) => {
    const kind = figureKind(c.platform, c.category, c.domain);
    return kind !== null && figureShape(kind) === "topdown" && c.course_deg != null;
  };
  /** A plan-view figure, drawn flat on the map and turned to the heading. */
  const flatFigure = (u: UnitView) => {
    const kind = unitKind(u);
    return kind !== null && figureShape(kind) === "topdown";
  };
  /** Own units: figures (or APP-6 frames). `billboard`: face the screen rather than lie flat. */
  const unitLayer = (id: string, data: UnitView[], billboard: boolean) =>
    new IconLayer<UnitView>({
      id,
      data,
      pickable: true,
      billboard,
      getPosition: (u) => pt(u.position),
      getIcon: (u) => {
        const aff = affiliation(u, s.viewer);
        const sel = u.id === s.selected;
        const kind = unitKind(u);
        if (kind) return figureIcon(kind, aff, sel);
        const code = functionCode(u.platform);
        return {
          id: `${aff}-${u.domain}-${sel}-${code}`,
          url: symbolUrl(aff, sel, u.domain, code),
          width: 64,
          height: 64,
        };
      },
      getSize: (u) => {
        const kind = unitKind(u);
        // Frames: large enough to read the function code.
        return kind ? figureSize(kind, u.id === s.selected) : 36;
      },
      getAngle: (u) => iconAngle(unitKind(u), n(u.heading_deg)),
      sizeUnits: figures ? "meters" : "pixels",
      sizeMinPixels: figures ? FIGURE_MIN_PX : 0,
      sizeMaxPixels: figures ? FIGURE_MAX_PX : Number.MAX_SAFE_INTEGER,
      updateTriggers: {
        getIcon: [s.selected, s.viewer, figures],
        getSize: [s.selected, figures],
        getAngle: [figures],
      },
      onClick: (info: PickingInfo<UnitView>) => {
        if (info.object) s.onPickUnit(info.object.id);
        return true;
      },
    });
  // The selected unit's whole route; for the others only a short stub of the next leg, so long
  // transits (a ferry 100 km away) do not draw lines across the whole picture.
  const routes = s.units
    .filter((u) => u.route && u.route.next < u.route.waypoints.length)
    .map((u) => {
      const ahead = u.route?.waypoints.slice(u.route.next) ?? [];
      const selected = u.id === s.selected;
      const next = ahead[0];
      return {
        id: u.id,
        selected,
        path: selected
          ? [pt(u.position), ...ahead.map(pt)]
          : next
            ? [pt(u.position), clip(u.position, next, ROUTE_STUB_M)]
            : [],
        colour: AFFILIATION_COLOUR[affiliation(u, s.viewer)],
      };
    });
  const waypoints = routes
    .filter((r) => r.selected)
    .flatMap((r) => r.path.slice(1).map((p) => ({ p, colour: r.colour })));
  // Leader: where the unit will be in 6 minutes at its current speed and heading.
  // Decluttered like labels above LABEL_LIMIT units (selected unit only).
  const leaders = s.units
    .filter((u) => s.units.length <= LABEL_LIMIT || u.id === s.selected)
    .filter((u) => n(u.speed_mps) > 0)
    .map((u) => ({
      from: pt(u.position),
      to: project(
        n(u.position.lat_deg),
        n(u.position.lon_deg),
        n(u.heading_deg),
        n(u.speed_mps) * 360,
      ),
      colour: AFFILIATION_COLOUR[affiliation(u, s.viewer)],
    }));
  const draft = s.draft ? s.draft.map(pt) : [];
  const draftStart = s.units.find((u) => u.id === s.selected);
  const draftPath = draftStart && draft.length ? [pt(draftStart.position), ...draft] : [];

  return [
    new PathLayer({
      id: "routes",
      data: routes,
      getPath: (d) => d.path,
      getColor: (d) => rgba(d.colour, d.selected ? 170 : 90),
      getWidth: (d) => (d.selected ? 2 : 1.5),
      widthUnits: "pixels",
      getDashArray: [6, 4],
    }),
    new ScatterplotLayer({
      id: "waypoints",
      data: waypoints,
      getPosition: (d) => d.p,
      getRadius: 4,
      radiusUnits: "pixels",
      stroked: true,
      filled: false,
      getLineColor: (d) => rgba(d.colour, 200),
      lineWidthMinPixels: 1.5,
    }),
    new PathLayer({
      id: "draft-route",
      data: draftPath.length ? [{ path: draftPath }] : [],
      getPath: (d) => d.path,
      getColor: [255, 210, 80, 230],
      getWidth: 2.5,
      widthUnits: "pixels",
    }),
    new ScatterplotLayer({
      id: "draft-waypoints",
      data: draft,
      getPosition: (d) => d,
      getRadius: 5,
      radiusUnits: "pixels",
      getFillColor: [255, 210, 80, 230],
    }),
    new LineLayer({
      id: "leaders",
      data: leaders,
      getSourcePosition: (d) => d.from,
      getTargetPosition: (d) => d.to,
      getColor: (d) => rgba(d.colour, 220),
      getWidth: 1.5,
    }),
    // Plan-view figures (ships, submarines, aircraft) lie flat on the map, turned to the heading,
    // so they point along it however the view is rotated or tilted. Upright icons (land sites)
    // and APP-6 frames face the screen.
    unitLayer("units", figures ? s.units.filter(flatFigure) : s.units, !figures),
    unitLayer("unit-icons", figures ? s.units.filter((u) => !flatFigure(u)) : [], true),
    // Affiliation by shape as well as colour, beside the name: it never turns with the figure.
    new IconLayer<UnitView>({
      id: "unit-badges",
      data: figures ? s.units : [],
      getPosition: (u) => pt(u.position),
      getIcon: (u) => badgeIcon(affiliation(u, s.viewer), u.domain),
      getSize: BADGE_PX,
      sizeUnits: "pixels",
      getPixelOffset: [0, 24],
      updateTriggers: { getIcon: [s.viewer] },
    }),
    new TextLayer<UnitView>({
      id: "labels",
      // Declutter: past LABEL_LIMIT units, labels overlap into noise and re-laying them out on every
      // snapshot costs the frame budget, so only the selected unit keeps its label.
      data: (s.units.length <= LABEL_LIMIT
        ? s.units
        : s.units.filter((u) => u.id === s.selected)
      ).filter((u) => !s.hiddenLabels?.has(`u:${u.id}`)),
      getPosition: (u) => pt(u.position),
      getText: unitLabel,
      getSize: 12,
      getColor: [220, 230, 238, 255],
      // With figures the name starts just after the affiliation badge under the unit.
      getTextAnchor: figures ? "start" : "middle",
      getPixelOffset: figures ? [BADGE_PX / 2 + 4, 24] : [0, 24],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: LABEL_CHARSET,
    }),
    // Contacts: what the viewing side believes is out there, at the estimated position, with the
    // 95 % error ellipse. The frame shows the domain once classified; affiliation stays unknown
    // until postures exist (sim-c2). No ground-truth identity is available to draw.
    // ESM bearing lines: from the intercepting ship along the measured bearing. They fade as
    // they age (60 s at most).
    // Minefields: a dashed-looking ring (many short segments) with the mines left.
    new PathLayer<Minefield>({
      id: "minefields",
      data: s.minefields ?? [],
      getPath: (f) => circleRing(f.centre, n(f.radius_m)),
      getColor: [230, 120, 220, 200],
      getWidth: 2,
      widthUnits: "pixels",
    }),
    new TextLayer<Minefield>({
      id: "minefield-labels",
      data: (s.minefields ?? []).filter((f) => !s.hiddenLabels?.has(`m:${f.id}`)),
      getPosition: (f) => pt(f.centre),
      getText: minefieldLabel,
      getSize: 12,
      getColor: [230, 120, 220, 255],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: LABEL_CHARSET,
    }),
    // Weapons in flight: a short line along the heading, brighter once the seeker has locked on.
    new IconLayer<WeaponView>({
      id: "weapons",
      data: s.weapons ?? [],
      getPosition: (w) => pt(w.position),
      getIcon: (w) => {
        const kind = munitionKind(w.weapon);
        const aff = affiliation({ side: w.side } as UnitView, s.viewer);
        return {
          id: `mun-${kind}-${aff}-${w.homing}`,
          url: munitionUrl(kind, aff, w.homing),
          width: 64,
          height: 64,
        };
      },
      getSize: (w) => (munitionKind(w.weapon) === "shell" ? 1200 : 2800),
      getAngle: (w) => -n(w.heading_deg),
      // Flat on the map, like unit figures, so it points along its heading in a rotated view.
      billboard: false,
      sizeUnits: "meters",
      sizeMinPixels: 9,
      sizeMaxPixels: 30,
      updateTriggers: { getIcon: [s.viewer] },
    }),
    new IconLayer<WeaponTrackView>({
      id: "weapon-tracks",
      data: s.weaponTracks ?? [],
      getPosition: (t) => pt(t.position),
      getIcon: (t) => {
        const kind = t.domain === "subsurface" ? "torpedo" : "anti_ship_missile";
        return {
          id: `track-${kind}`,
          url: munitionUrl(kind, "hostile", false),
          width: 64,
          height: 64,
        };
      },
      getSize: 22,
      sizeUnits: "pixels",
    }),
    new PathLayer<BearingView>({
      id: "bearings",
      data: s.bearings ?? [],
      getPath: (b) => [
        pt(b.line.from),
        project(
          n(b.line.from.lat_deg),
          n(b.line.from.lon_deg),
          n(b.line.bearing_deg),
          BEARING_DRAW_M,
        ),
      ],
      getColor: (b) => [255, 170, 80, Math.round(200 * (1 - Math.min(n(b.age_s), 60) / 75))],
      getWidth: 1.5,
      widthUnits: "pixels",
      updateTriggers: { getColor: [s.gameTimeS] },
    }),
    new PathLayer<ContactView>({
      id: "contact-error",
      data: s.contacts,
      getPath: (c) => ellipseRing(c),
      getColor: [255, 240, 150, 170],
      getWidth: 1.5,
      widthUnits: "pixels",
      updateTriggers: { getPath: [s.gameTimeS] },
    }),
    // A classified contact whose course the side has estimated lies flat on the map, turned to
    // that course, like own units; it keeps it however the view is rotated or tilted.
    new IconLayer<ContactView>({
      id: "contact-figures",
      data: figures ? s.contacts.filter(flatContact) : [],
      billboard: false,
      getPosition: (c) => pt(c.position),
      getIcon: (c) => {
        const kind = figureKind(c.platform, c.category, c.domain);
        return figureIcon(kind ?? "ship", contactAffiliation(c), false);
      },
      getSize: (c) => {
        const kind = figureKind(c.platform, c.category, c.domain);
        return kind ? figureSize(kind, false) * 0.9 : 30;
      },
      getAngle: (c) => -n(c.course_deg ?? 0),
      sizeUnits: "meters",
      sizeMinPixels: FIGURE_MIN_PX - 2,
      sizeMaxPixels: FIGURE_MAX_PX - 8,
    }),
    // Contacts with no course yet (newly acquired, stopped), unclassified ones and APP-6 frames
    // face the screen: the side does not know which way they point.
    new IconLayer<ContactView>({
      id: "contacts",
      data: figures ? s.contacts.filter((c) => !flatContact(c)) : s.contacts,
      getPosition: (c) => pt(c.position),
      getIcon: (c) => {
        const domain = c.domain ?? "surface";
        const aff = contactAffiliation(c);
        // The figure (or code) shows only as far as the side has classified the contact; an
        // unclassified contact keeps the APP-6 frame.
        const kind = figures ? figureKind(c.platform, c.category, c.domain) : null;
        if (kind) return figureIcon(kind, aff, false);
        const code = functionCode(c.platform, c.category);
        return {
          id: `contact-${aff}-${domain}-${code}`,
          url: symbolUrl(aff, false, domain, code),
          width: 64,
          height: 64,
        };
      },
      getSize: (c) => {
        const kind = figures ? figureKind(c.platform, c.category, c.domain) : null;
        // North-up: a side does not know a contact's course.
        return kind ? figureSize(kind, false) * 0.9 : 30;
      },
      sizeUnits: figures ? "meters" : "pixels",
      sizeMinPixels: figures ? FIGURE_MIN_PX - 2 : 0,
      sizeMaxPixels: figures ? FIGURE_MAX_PX - 8 : Number.MAX_SAFE_INTEGER,
      updateTriggers: { getIcon: [figures], getSize: [figures] },
    }),
    // A classified contact's figure carries no frame, so its affiliation badge goes beside its
    // label, as for own units. An unclassified contact keeps the APP-6 frame and needs none.
    new IconLayer<ContactView>({
      id: "contact-badges",
      data: figures ? s.contacts.filter((c) => figureKind(c.platform, c.category, c.domain)) : [],
      getPosition: (c) => pt(c.position),
      getIcon: (c) => badgeIcon(contactAffiliation(c), c.domain ?? "surface"),
      getSize: BADGE_PX,
      sizeUnits: "pixels",
      getPixelOffset: [0, 20],
    }),
    new TextLayer<ContactView>({
      id: "contact-labels",
      data: s.contacts.filter((c) => !s.hiddenLabels?.has(`c:${c.number}`)),
      getPosition: (c) => pt(c.position),
      getText: (c) => contactLabel(c, s.gameTimeS),
      getSize: 12,
      getColor: [255, 240, 150, 255],
      getTextAnchor: figures ? "start" : "middle",
      getPixelOffset: figures ? [BADGE_PX / 2 + 4, 20] : [0, 20],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: LABEL_CHARSET,
      updateTriggers: { getText: [s.gameTimeS] },
    }),
  ];
}

/** Time since the contact was last detected, e.g. "12 s" or "4 min". */
export function contactAge(nowS: number, c: ContactView): string {
  const age = Math.max(0, Math.round(nowS - n(c.last_seen_s)));
  return age < 120 ? `${age} s` : `${Math.round(age / 60)} min`;
}

/** A named mark on the map: a scenario reference point. */
export interface MapPoint {
  name: string;
  position: GeoPoint;
}

const POINT_COLOUR: RGBA = [120, 220, 160, 255];

/** Shipping lanes (slice 5): a translucent band of the lane's width, and its name and traffic
 * at the middle of its centreline. */
export function laneLayers(lanes: Lane[], hidden?: Set<string>): Layer[] {
  const mid = (l: Lane): LonLat => {
    const k = Math.floor((l.points.length - 1) / 2);
    const p = l.points[k] ?? { lat_deg: 0, lon_deg: 0 };
    const q = l.points[k + 1] ?? p;
    return [(n(p.lon_deg) + n(q.lon_deg)) / 2, (n(p.lat_deg) + n(q.lat_deg)) / 2];
  };
  return [
    new PathLayer<Lane>({
      id: "lanes",
      data: lanes,
      getPath: (l) => l.points.map(pt),
      getWidth: (l) => n(l.width_m),
      widthUnits: "meters",
      widthMinPixels: 2,
      getColor: [214, 178, 92, 70],
      capRounded: true,
      jointRounded: true,
    }),
    new TextLayer<Lane>({
      id: "lane-labels",
      data: lanes.filter((l) => !hidden?.has(`lane:${l.id}`)),
      getPosition: mid,
      getText: (l) =>
        l.ships_per_day == null ? l.name : `${l.name} · ${Math.round(l.ships_per_day)}/day`,
      getSize: 11,
      getColor: [226, 196, 120, 230],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: LABEL_CHARSET,
    }),
  ];
}

/** Reference points: a small ring and the name. */
export function referenceLayers(
  points: MapPoint[],
  id = "reference-points",
  hidden?: Set<string>,
): Layer[] {
  return [
    new ScatterplotLayer<MapPoint>({
      id,
      data: points,
      getPosition: (p) => pt(p.position),
      getRadius: 5,
      radiusUnits: "pixels",
      stroked: true,
      filled: false,
      getLineColor: POINT_COLOUR,
      getLineWidth: 2,
      lineWidthUnits: "pixels",
    }),
    new TextLayer<MapPoint>({
      id: `${id}-labels`,
      data: points.filter((q) => !hidden?.has(`p:${q.name}`)),
      getPosition: (p) => pt(p.position),
      getText: (p) => p.name,
      getSize: 12,
      getColor: POINT_COLOUR,
      getPixelOffset: [0, -14],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: LABEL_CHARSET,
    }),
  ];
}

/** What the scenario editor draws: the draft's units, routes, patrol areas and minefields. */
export interface EditorScene {
  units: {
    id: number;
    name: string;
    side: number;
    position: GeoPoint;
    route?: GeoPoint[];
    mission?: { kind: string; centre?: GeoPoint; radius_m?: number | null; destination?: GeoPoint };
    reserve?: boolean;
  }[];
  minefields: { centre: GeoPoint; radius_m: number; mines: number }[];
  points: MapPoint[];
  /** Side ids in order: the first side is drawn blue, the second red, others in turn. */
  sides: number[];
  selected: number | null;
  onPickUnit: (id: number) => void;
}

const SIDE_COLOURS: [number, number, number][] = [
  [90, 170, 255],
  [255, 110, 100],
  [250, 210, 90],
  [180, 140, 255],
  [120, 220, 160],
];

/** The scenario editor's layers (drawn instead of the live picture while it is open). */
export function editorLayers(e: EditorScene): Layer[] {
  const colour = (side: number, alpha = 255): RGBA =>
    rgba(
      SIDE_COLOURS[Math.max(0, e.sides.indexOf(side)) % SIDE_COLOURS.length] ?? [200, 200, 200],
      alpha,
    );
  const routes = e.units
    .filter((u) => (u.route ?? []).length > 0)
    .map((u) => ({ side: u.side, path: [pt(u.position), ...(u.route ?? []).map(pt)] }));
  const areas = e.units.flatMap((u) =>
    u.mission?.kind === "patrol" && u.mission.centre
      ? [{ side: u.side, path: circleRing(u.mission.centre, n(u.mission.radius_m ?? 0)) }]
      : u.mission?.kind === "ferry" && u.mission.destination
        ? [{ side: u.side, path: [pt(u.position), pt(u.mission.destination)] }]
        : [],
  );
  return [
    new PathLayer<{ side: number; path: LonLat[] }>({
      id: "editor-routes",
      data: [...routes, ...areas],
      getPath: (r) => r.path,
      getColor: (r) => colour(r.side, 170),
      getWidth: 2,
      widthUnits: "pixels",
    }),
    new PathLayer<EditorScene["minefields"][number]>({
      id: "editor-minefields",
      data: e.minefields,
      getPath: (f) => circleRing(f.centre, n(f.radius_m)),
      getColor: [230, 120, 220, 200],
      getWidth: 2,
      widthUnits: "pixels",
    }),
    new ScatterplotLayer<EditorScene["units"][number]>({
      id: "editor-units",
      data: e.units,
      getPosition: (u) => pt(u.position),
      getRadius: (u) => (u.id === e.selected ? 9 : 6),
      radiusUnits: "pixels",
      stroked: true,
      getFillColor: (u) => colour(u.side, u.reserve ? 60 : 220),
      getLineColor: (u) => (u.id === e.selected ? [255, 255, 255, 255] : [11, 27, 43, 255]),
      getLineWidth: 2,
      lineWidthUnits: "pixels",
      pickable: true,
      onClick: (info: PickingInfo<EditorScene["units"][number]>) => {
        if (info.object) e.onPickUnit(info.object.id);
        return true;
      },
      updateTriggers: { getRadius: [e.selected], getLineColor: [e.selected] },
    }),
    new TextLayer<EditorScene["units"][number]>({
      id: "editor-unit-labels",
      data: e.units,
      getPosition: (u) => pt(u.position),
      getText: (u) => (u.reserve ? `${u.name} (reserve)` : u.name),
      getSize: 12,
      getColor: (u) => colour(u.side),
      getPixelOffset: [0, 16],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: LABEL_CHARSET,
    }),
    ...referenceLayers(e.points, "editor-points"),
  ];
}

/** A planning-layer facility as drawn: kind, name and place. */
export interface MapFacility {
  name: string;
  kind: string;
  position: GeoPoint;
  radius_m: number | null;
  /** The facility's water, drawn instead of its radius. */
  area?: MapZone["area"] | null;
}

/** A sense or engage zone as drawn. */
export interface MapZone {
  name: string;
  kind: string;
  layer?: string | null;
  area:
    | { shape: "circle"; centre: GeoPoint; radius_m: number | null }
    | { shape: "polygon"; points: GeoPoint[] };
}

const FACILITY_CODE: Record<string, string> = {
  command_post: "CP",
  refuelling_point: "RAS",
  sheltering_port: "PORT",
  anchorage: "ANCH",
  airbase: "AIR",
};

/** One sense zone's coverage, as the engine samples it. */
export interface MapCoverage {
  cells: { centre: GeoPoint; covered: boolean }[];
  half_lat_deg: number | null;
  half_lon_deg: number | null;
}

const COVERED_FILL: RGBA = [90, 200, 120, 70];
const GAP_FILL: RGBA = [235, 80, 70, 90];

/** Sense-zone coverage: covered cells green, gaps red. */
export function coverageLayers(coverage: MapCoverage[]): Layer[] {
  type Cell = { ring: LonLat[]; covered: boolean };
  const cells: Cell[] = coverage.flatMap((c) => {
    const h = n(c.half_lat_deg);
    const w = n(c.half_lon_deg);
    return c.cells.map((cell) => {
      const lat = n(cell.centre.lat_deg);
      const lon = n(cell.centre.lon_deg);
      return {
        covered: cell.covered,
        ring: [
          [lon - w, lat - h],
          [lon + w, lat - h],
          [lon + w, lat + h],
          [lon - w, lat + h],
        ] as LonLat[],
      };
    });
  });
  return [
    new PolygonLayer<Cell>({
      id: "coverage",
      data: cells,
      getPolygon: (c) => c.ring,
      getFillColor: (c) => (c.covered ? COVERED_FILL : GAP_FILL),
      stroked: false,
      filled: true,
    }),
  ];
}

/**
 * The sun's subsolar point at `unixS`, by the engine's own simplified solar position
 * (`sim_sensors::sun_elevation_deg`): declination −23.44° · cos(360°/365 · (N + 10)) with N the
 * day count, and the subsolar longitude from mean solar time.
 */
export function subsolarPoint(unixS: number): { lat: number; lon: number } {
  const n = (((unixS / 86_400) % 365.2422) + 365.2422) % 365.2422;
  const dec = -23.44 * Math.cos(((360 / 365) * (n + 10) * Math.PI) / 180);
  const utcHours = (((unixS % 86_400) + 86_400) % 86_400) / 3600;
  let lon = 15 * (12 - utcHours);
  if (lon > 180) lon -= 360;
  if (lon <= -180) lon += 360;
  return { lat: dec, lon };
}

/** The night side of the earth at `unixS` (sun below the horizon), as a polygon in [lon, lat]. */
export function nightPolygon(unixS: number): LonLat[] {
  const { lat: dec, lon: lon0 } = subsolarPoint(unixS);
  const rad = Math.PI / 180;
  // Avoid the equinox singularity: the terminator is then a meridian pair.
  const d = Math.abs(dec) < 0.01 ? (dec < 0 ? -0.01 : 0.01) : dec;
  const line: LonLat[] = [];
  for (let lon = -180; lon <= 180; lon += 2) {
    const h = (lon - lon0) * rad;
    const lat = Math.atan(-Math.cos(h) / Math.tan(d * rad)) / rad;
    line.push([lon, Math.max(-85, Math.min(85, lat))]);
  }
  // The pole away from the sun is in the night.
  const pole = d > 0 ? -85 : 85;
  return [...line, [180, pole], [-180, pole]];
}

const NIGHT_FILL: RGBA = [0, 0, 30, 70];

/** Night shading for the map at game time `unixS`. */
export function nightLayers(unixS: number): Layer[] {
  return [
    new PolygonLayer<{ ring: LonLat[] }>({
      id: "night",
      data: [{ ring: nightPolygon(unixS) }],
      getPolygon: (x) => x.ring,
      getFillColor: NIGHT_FILL,
      stroked: false,
      filled: true,
      // Recompute when the minute changes.
      updateTriggers: { getPolygon: Math.floor(unixS / 60) },
    }),
  ];
}

/** Outline of a zone, closed. */
export function zoneRing(z: MapZone): LonLat[] {
  if (z.area.shape === "circle") return circleRing(z.area.centre, n(z.area.radius_m));
  const ring = z.area.points.map(pt);
  return ring.length ? [...ring, ring[0] as LonLat] : ring;
}

/** Where a zone's label goes: the circle's centre, or the polygon's first point. */
/** Where a zone's name goes: its northern edge (a circle's top, a polygon's northernmost
 * corner), clear of the reference points and units that sit at its centre. */
export function zoneAnchor(z: MapZone): LonLat {
  if (z.area.shape === "circle") {
    const c = z.area.centre;
    return project(n(c.lat_deg), n(c.lon_deg), 0, n(z.area.radius_m));
  }
  const top = z.area.points.reduce<GeoPoint | null>(
    (best, q) => (best === null || n(q.lat_deg) > n(best.lat_deg) ? q : best),
    null,
  );
  return top ? pt(top) : [0, 0];
}

const SENSE_COLOUR: RGBA = [110, 190, 255, 200];
const ENGAGE_COLOUR: RGBA = [255, 110, 100, 220];
const FACILITY_COLOUR: RGBA = [230, 230, 160, 255];

/** The planning layer: zones as outlines (sense blue, engage red) and facilities as labelled
 * marks with their radius. */
export function planningLayers(
  facilities: MapFacility[],
  zones: MapZone[],
  id = "planning",
  hidden?: Set<string>,
): Layer[] {
  const label = {
    fontFamily: "system-ui, sans-serif",
    outlineWidth: 2,
    outlineColor: [11, 27, 43, 255] as RGBA,
    fontSettings: { sdf: true },
    characterSet: LABEL_CHARSET,
  };
  return [
    new PathLayer<MapZone>({
      id: `${id}-zones`,
      data: zones,
      getPath: zoneRing,
      getColor: (z) => (z.kind === "engage" ? ENGAGE_COLOUR : SENSE_COLOUR),
      getWidth: (z) => (z.kind === "engage" ? 2 : 1.5),
      widthUnits: "pixels",
    }),
    new TextLayer<MapZone>({
      id: `${id}-zone-labels`,
      data: zones.filter((z) => !hidden?.has(`z:${z.name}`)),
      getPosition: zoneAnchor,
      getText: zoneLabel,
      getSize: 11,
      getColor: (z) => (z.kind === "engage" ? ENGAGE_COLOUR : SENSE_COLOUR),
      getPixelOffset: [0, -10],
      ...label,
    }),
    new PathLayer<MapFacility>({
      id: `${id}-facility-rings`,
      data: facilities,
      getPath: (f) =>
        f.area
          ? zoneRing({ name: f.name, kind: f.kind, area: f.area })
          : circleRing(f.position, n(f.radius_m)),
      getColor: FACILITY_COLOUR,
      getWidth: 1,
      widthUnits: "pixels",
    }),
    new TextLayer<MapFacility>({
      id: `${id}-facilities`,
      data: facilities.filter((f) => !hidden?.has(`f:${f.name}`)),
      getPosition: (f) => pt(f.position),
      getText: facilityLabel,
      getSize: 12,
      getColor: FACILITY_COLOUR,
      ...label,
    }),
  ];
}
