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
import type { ContactView, GeoPoint, UnitView, Viewer } from "./bindings";
import { n, project } from "./format";
import { AFFILIATION_COLOUR, symbolUrl, type Affiliation } from "./symbols";

type LonLat = [number, number];
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
  const landUrl = new URL("basemap/ne_50m_land.geojson", window.location.href).href;
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

export interface SceneInput {
  units: UnitView[];
  /** The viewing side's contacts (empty for the white cell, which sees ground truth). */
  contacts: ContactView[];
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
  const leaders = s.units
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
        return { id: `${aff}-${sel}`, url: symbolUrl(aff, sel), width: 64, height: 64 };
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
      data: s.units,
      getPosition: (u) => pt(u.position),
      getText: (u) => u.name,
      getSize: 12,
      getColor: [220, 230, 238, 255],
      getPixelOffset: [0, 22],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: "auto",
    }),
    // Contacts: what the viewing side believes is out there. Unknown frame until Phase 2
    // classification; no ground-truth identity is available to draw.
    new IconLayer<ContactView>({
      id: "contacts",
      data: s.contacts,
      getPosition: (c) => pt(c.position),
      getIcon: () => ({ id: "contact", url: symbolUrl("unknown", false), width: 64, height: 64 }),
      getSize: 24,
      sizeUnits: "pixels",
    }),
    new TextLayer<ContactView>({
      id: "contact-labels",
      data: s.contacts,
      getPosition: (c) => pt(c.position),
      getText: (c) => `C${c.number} · ${contactAge(s.gameTimeS, c)}`,
      getSize: 12,
      getColor: [255, 240, 150, 255],
      getPixelOffset: [0, 20],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
      characterSet: "auto",
      updateTriggers: { getText: [s.gameTimeS] },
    }),
  ];
}

/** Time since the contact was last detected, e.g. "12 s" or "4 min". */
export function contactAge(nowS: number, c: ContactView): string {
  const age = Math.max(0, Math.round(nowS - n(c.last_seen_s)));
  return age < 120 ? `${age} s` : `${Math.round(age / 60)} min`;
}
