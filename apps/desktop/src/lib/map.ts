// Offline map: MapLibre draws the base layers (sea, bundled Natural Earth land, graticule) and
// deck.gl draws everything that moves. No network tiles are used.

import * as maplibregl from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
// MapLibre locates its worker relative to its own module URL, which Vite's dependency
// pre-bundling and Tauri's custom protocol both break. The worker file is self-contained, so
// ship it as a static asset and point MapLibre at it explicitly.
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import { MapboxOverlay } from "@deck.gl/mapbox";
import { IconLayer, LineLayer, PathLayer, ScatterplotLayer, TextLayer } from "@deck.gl/layers";
import type { Layer, PickingInfo } from "@deck.gl/core";
import type {
  BearingView,
  ContactView,
  GeoPoint,
  Minefield,
  UnitView,
  Viewer,
  WeaponView,
} from "./bindings";
import { n, project } from "./format";
import { AFFILIATION_COLOUR, symbolUrl, type Affiliation } from "./symbols";

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
  /** Minefields the viewer may see. */
  minefields?: Minefield[];
  gameTimeS: number;
  viewer: Viewer;
  selected: number | null;
  draft: GeoPoint[] | null;
  onPickUnit: (id: number) => void;
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

/** Length a bearing line is drawn on the map, metres. */
export const BEARING_DRAW_M = 150_000;

export function buildLayers(s: SceneInput): Layer[] {
  const routes = s.units
    .filter((u) => u.route && u.route.next < u.route.waypoints.length)
    .map((u) => ({
      id: u.id,
      path: [pt(u.position), ...(u.route?.waypoints.slice(u.route.next) ?? []).map(pt)],
      colour: AFFILIATION_COLOUR[affiliation(u, s.viewer)],
    }));
  const waypoints = routes.flatMap((r) => r.path.slice(1).map((p) => ({ p, colour: r.colour })));
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
      getColor: (d) => rgba(d.colour, 140),
      getWidth: 2,
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
    new IconLayer<UnitView>({
      id: "units",
      data: s.units,
      pickable: true,
      getPosition: (u) => pt(u.position),
      getIcon: (u) => {
        const aff = affiliation(u, s.viewer);
        const sel = u.id === s.selected;
        return {
          id: `${aff}-${u.domain}-${sel}`,
          url: symbolUrl(aff, sel, u.domain),
          width: 64,
          height: 64,
        };
      },
      getSize: 28,
      sizeUnits: "pixels",
      updateTriggers: { getIcon: [s.selected, s.viewer] },
      onClick: (info: PickingInfo<UnitView>) => {
        if (info.object) s.onPickUnit(info.object.id);
        return true;
      },
    }),
    new TextLayer<UnitView>({
      id: "labels",
      // Declutter: past LABEL_LIMIT units, labels overlap into noise and re-laying them out on every
      // snapshot costs the frame budget, so only the selected unit keeps its label.
      data: s.units.length <= LABEL_LIMIT ? s.units : s.units.filter((u) => u.id === s.selected),
      getPosition: (u) => pt(u.position),
      getText: (u) => u.name,
      getSize: 12,
      getColor: [220, 230, 238, 255],
      getPixelOffset: [0, 22],
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
      data: s.minefields ?? [],
      getPosition: (f) => pt(f.centre),
      getText: (f) => `M${f.id} · ${f.mines} mines`,
      getSize: 12,
      getColor: [230, 120, 220, 255],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: LABEL_CHARSET,
    }),
    // Weapons in flight: a short line along the heading, brighter once the seeker has locked on.
    new LineLayer<WeaponView>({
      id: "weapons",
      data: s.weapons ?? [],
      getSourcePosition: (w) => pt(w.position),
      getTargetPosition: (w) =>
        project(n(w.position.lat_deg), n(w.position.lon_deg), n(w.heading_deg), 1500),
      getColor: (w) => (w.homing ? [255, 90, 60, 255] : [255, 200, 120, 220]),
      getWidth: 3,
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
    new IconLayer<ContactView>({
      id: "contacts",
      data: s.contacts,
      getPosition: (c) => pt(c.position),
      getIcon: (c) => {
        const domain = c.domain ?? "surface";
        const aff = contactAffiliation(c);
        return {
          id: `contact-${aff}-${domain}`,
          url: symbolUrl(aff, false, domain),
          width: 64,
          height: 64,
        };
      },
      getSize: 24,
      sizeUnits: "pixels",
    }),
    new TextLayer<ContactView>({
      id: "contact-labels",
      data: s.contacts,
      getPosition: (c) => pt(c.position),
      getText: (c) => `C${c.number} · ${contactKind(c)} · ${contactAge(s.gameTimeS, c)}`,
      getSize: 12,
      getColor: [255, 240, 150, 255],
      getPixelOffset: [0, 20],
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
