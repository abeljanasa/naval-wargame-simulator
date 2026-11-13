// Terrain and bathymetry on the map (Phase 4 slice 2): depth shading and contours over the sea,
// relief over land, and the height under the cursor. The heights come from the packs installed
// on this machine (`terrain_grid`, raw bytes); this module only draws them.
//
// The grid is sampled evenly in Web Mercator, so an image placed on the map's corners lines up
// with the coastline at any latitude. The images sit in the MapLibre style below the coastline
// and below the deck.gl picture, so units and labels always draw on top.

import { invoke } from "@tauri-apps/api/core";
import type * as maplibregl from "maplibre-gl";
import { commands, type TerrainStatus_Serialize as TerrainStatus } from "./bindings";

export type { TerrainStatus };

/** No pack covers this cell. */
export const NO_DATA = -32768;

/** Largest grid asked for, each way (the host refuses more than 2048). */
const MAX_CELLS = 1024;
const MAX_MERC_LAT = 85.05;
const RAD = Math.PI / 180;

export function mercatorY(latDeg: number): number {
  return Math.log(Math.tan(Math.PI / 4 + (latDeg * RAD) / 2));
}

export function mercatorLat(y: number): number {
  return Math.atan(Math.sinh(y)) / RAD;
}

/** Latitudes of `rows` row centres from `north` to `south`, evenly spaced in Web Mercator. */
export function mercatorLats(north: number, south: number, rows: number): number[] {
  const yn = mercatorY(Math.min(north, MAX_MERC_LAT));
  const ys = mercatorY(Math.max(south, -MAX_MERC_LAT));
  const step = (yn - ys) / rows;
  return Array.from({ length: rows }, (_, r) => mercatorLat(yn - (r + 0.5) * step));
}

export type TerrainTheme = "dark" | "light";
type Rgb = [number, number, number];

/** Depth colour stops, metres: pale and close to the coast, dark in deep water. */
const DEPTH_STOPS: Record<TerrainTheme, [number, Rgb][]> = {
  dark: [
    [0, [44, 98, 116]],
    [50, [31, 78, 98]],
    [200, [22, 59, 81]],
    [1000, [15, 42, 62]],
    [4000, [10, 28, 44]],
    [8000, [6, 17, 29]],
  ],
  light: [
    [0, [222, 240, 247]],
    [50, [192, 224, 237]],
    [200, [164, 205, 224]],
    [1000, [132, 181, 207]],
    [4000, [100, 152, 186]],
    [8000, [74, 124, 162]],
  ],
};

/** The shading colour for a water depth (metres, positive down). Interpolated on log depth. */
export function depthColour(depthM: number, theme: TerrainTheme): Rgb {
  const stops = DEPTH_STOPS[theme];
  const d = Math.max(0, depthM);
  const last = stops[stops.length - 1];
  if (!last) return [0, 0, 0];
  for (let i = 1; i < stops.length; i++) {
    const hi = stops[i];
    const lo = stops[i - 1];
    if (!hi || !lo || d > hi[0]) continue;
    const t = (Math.log1p(d) - Math.log1p(lo[0])) / (Math.log1p(hi[0]) - Math.log1p(lo[0]));
    const mix = (k: 0 | 1 | 2) => Math.round(lo[1][k] + (hi[1][k] - lo[1][k]) * t);
    return [mix(0), mix(1), mix(2)];
  }
  return last[1];
}

/** Contour depths for a zoom level, metres: fewer and deeper when zoomed out. */
export function contourLevels(zoom: number): number[] {
  if (zoom < 4) return [200, 2000];
  if (zoom < 6) return [100, 200, 1000, 2000, 4000];
  if (zoom < 9) return [20, 50, 100, 200, 500, 1000, 2000];
  return [10, 20, 30, 50, 100, 200, 500];
}

/** A grid of heights as the map asked for it: rows north to south at `lats`, columns from `west`
 * to `east`. */
export interface TerrainGrid {
  west: number;
  east: number;
  north: number;
  south: number;
  lats: number[];
  cols: number;
  heights: Int16Array;
}

/** RGBA pixels: depth shading where the height is below sea level, clear elsewhere. */
export function depthPixels(g: TerrainGrid, theme: TerrainTheme): Uint8ClampedArray {
  const px = new Uint8ClampedArray(g.heights.length * 4);
  for (let i = 0; i < g.heights.length; i++) {
    const h = g.heights[i] ?? NO_DATA;
    if (h === NO_DATA || h >= 0) continue;
    const [r, gr, b] = depthColour(-h, theme);
    px[i * 4] = r;
    px[i * 4 + 1] = gr;
    px[i * 4 + 2] = b;
    px[i * 4 + 3] = 255;
  }
  return px;
}

/** RGBA pixels: hill shading over land, lit from the north-west. Dark on slopes facing away,
 * light on slopes facing the light, clear on flat ground and at sea. */
export function reliefPixels(g: TerrainGrid, exaggeration = 2): Uint8ClampedArray {
  const rows = g.lats.length;
  const cols = g.cols;
  const px = new Uint8ClampedArray(rows * cols * 4);
  const dxDeg = (g.east - g.west) / cols;
  // Light from azimuth 315°, altitude 45°.
  const lx = -0.5;
  const ly = 0.5;
  const lz = Math.SQRT1_2;
  const at = (r: number, c: number) => g.heights[r * cols + c] ?? NO_DATA;
  const latOf = (r: number) => g.lats[r] ?? 0;
  for (let r = 1; r < rows - 1; r++) {
    const dx = 2 * dxDeg * 111_320 * Math.cos(latOf(r) * RAD);
    const dy = (latOf(r - 1) - latOf(r + 1)) * 111_320;
    for (let c = 1; c < cols - 1; c++) {
      const h = at(r, c);
      if (h === NO_DATA || h <= 0) continue;
      const e = at(r, c + 1);
      const w = at(r, c - 1);
      const nn = at(r - 1, c);
      const s = at(r + 1, c);
      if (e === NO_DATA || w === NO_DATA || nn === NO_DATA || s === NO_DATA) continue;
      const gx = (exaggeration * (e - w)) / dx;
      const gy = (exaggeration * (nn - s)) / dy;
      const shade = (-gx * lx - gy * ly + lz) / Math.sqrt(gx * gx + gy * gy + 1);
      const i = (r * cols + c) * 4;
      const d = shade - lz;
      const v = d < 0 ? 0 : 255;
      px[i] = v;
      px[i + 1] = v;
      px[i + 2] = v;
      px[i + 3] = Math.round(Math.min(0.5, Math.abs(d) * 1.2) * 255);
    }
  }
  return px;
}

type LineFeature = {
  type: "Feature";
  properties: { depth_m: number; major: boolean };
  geometry: { type: "MultiLineString"; coordinates: [number, number][][] };
};

/** Depth contours by marching squares over the grid's cell centres, one feature per depth. */
export function contourLines(
  g: TerrainGrid,
  levels: number[],
): { type: "FeatureCollection"; features: LineFeature[] } {
  const rows = g.lats.length;
  const cols = g.cols;
  const dx = (g.east - g.west) / cols;
  const lon = (c: number) => g.west + (c + 0.5) * dx;
  const features: LineFeature[] = [];
  for (const depth of levels) {
    const z = -depth;
    const segs: [number, number][][] = [];
    for (let r = 0; r < rows - 1; r++) {
      const lat0 = g.lats[r] ?? 0;
      const lat1 = g.lats[r + 1] ?? 0;
      for (let c = 0; c < cols - 1; c++) {
        const a = g.heights[r * cols + c] ?? NO_DATA; // top left
        const b = g.heights[r * cols + c + 1] ?? NO_DATA; // top right
        const d = g.heights[(r + 1) * cols + c] ?? NO_DATA; // bottom left
        const e = g.heights[(r + 1) * cols + c + 1] ?? NO_DATA; // bottom right
        if (a === NO_DATA || b === NO_DATA || d === NO_DATA || e === NO_DATA) continue;
        const pts: [number, number][] = [];
        // Where the level crosses each edge: top, right, bottom, left.
        const cross = (p: number, q: number) => p < z !== q < z;
        const f = (p: number, q: number) => (z - p) / (q - p);
        if (cross(a, b)) pts.push([lon(c) + f(a, b) * dx, lat0]);
        if (cross(b, e)) pts.push([lon(c + 1), lat0 + f(b, e) * (lat1 - lat0)]);
        if (cross(d, e)) pts.push([lon(c) + f(d, e) * dx, lat1]);
        if (cross(a, d)) pts.push([lon(c), lat0 + f(a, d) * (lat1 - lat0)]);
        if (pts.length === 2) segs.push(pts);
        else if (pts.length === 4) {
          // A saddle: pair the crossings by the centre value.
          const centre = (a + b + d + e) / 4 < z;
          const [t, rt, bt, lt] = pts as [
            [number, number],
            [number, number],
            [number, number],
            [number, number],
          ];
          if (centre === a < z) {
            segs.push([t, rt], [bt, lt]);
          } else {
            segs.push([t, lt], [rt, bt]);
          }
        }
      }
    }
    if (segs.length > 0) {
      features.push({
        type: "Feature",
        properties: { depth_m: depth, major: depth === 200 || depth % 1000 === 0 },
        geometry: { type: "MultiLineString", coordinates: segs },
      });
    }
  }
  return { type: "FeatureCollection", features };
}

/** The height in the grid nearest a point (metres), or `null` outside it or where no pack
 * covers. */
export function heightAt(g: TerrainGrid, lon: number, lat: number): number | null {
  let x = lon;
  while (x < g.west) x += 360;
  while (x >= g.west + 360) x -= 360;
  const c = Math.floor(((x - g.west) / (g.east - g.west)) * g.cols);
  if (c < 0 || c >= g.cols || lat > g.north || lat < g.south) return null;
  const yn = mercatorY(g.north);
  const ys = mercatorY(g.south);
  const r = Math.floor(((yn - mercatorY(lat)) / (yn - ys)) * g.lats.length);
  if (r < 0 || r >= g.lats.length) return null;
  const h = g.heights[r * g.cols + c] ?? NO_DATA;
  return h === NO_DATA ? null : h;
}

/** A pack's cell size in words: arc-minutes or arc-seconds. */
export function cellText(arcsec: number | null): string {
  const v = arcsec ?? 0;
  return v >= 60 ? `${v / 60}′` : `${v}″`;
}

/** The area a pack covers (south, west, north, east in degrees), in words. */
export function coversText(b: (number | null)[]): string {
  const [s = 0, w = 0, n = 0, e = 0] = b.map((v) => v ?? 0);
  if (e - w >= 359.9) return "the world";
  const lat = (v: number) => `${Math.abs(v).toFixed(0)}°${v < 0 ? "S" : "N"}`;
  const lon = (v: number) => `${Math.abs(v).toFixed(0)}°${v < 0 ? "W" : "E"}`;
  return `${lat(s)}–${lat(n)}, ${lon(w)}–${lon(e)}`;
}

/** Words for the height under the cursor. */
export function heightText(h: number): string {
  return h < 0 ? `Depth ${Math.round(-h).toLocaleString("en-GB")} m` : `Height ${Math.round(h)} m`;
}

/** Under-keel clearance the engine plans with, metres (sim-core `UNDER_KEEL_CLEARANCE_M`). */
export const UNDER_KEEL_CLEARANCE_M = 1;

/** The water under a unit and ahead of it, as the engine reports it (`route_depths`). */
export interface WaterView {
  draft_m: number | null;
  here_m: number | null;
  least_m: number | null;
  least_at_m: number | null;
  charted: boolean;
  profile: { distance_m: number | null; depth_m: number | null }[];
}

const metres = (v: number) => `${Math.round(v).toLocaleString("en-GB")} m`;
const nm = (m: number) => `${(m / 1852).toFixed(1)} nm`;

/** True when the least water ahead (or here) is less than the draft plus clearance. */
export function waterTight(w: WaterView): boolean {
  if (w.draft_m == null) return false;
  const need = w.draft_m + UNDER_KEEL_CLEARANCE_M;
  return (w.least_m != null && w.least_m < need) || (w.here_m != null && w.here_m < need);
}

/** One line on the water: here, the least ahead and where, and the draft. */
export function waterText(w: WaterView): string {
  const parts: string[] = [];
  parts.push(w.here_m == null ? "no chart here" : `${metres(w.here_m)} here`);
  // The least water is worth a mention only when it is ahead, not where the unit is now.
  if (w.least_m != null && w.least_at_m != null && w.least_at_m >= 200 && w.profile.length > 0) {
    parts.push(`least ${metres(w.least_m)}, ${nm(w.least_at_m)} ahead`);
  }
  if (w.draft_m != null) parts.push(`draft ${w.draft_m.toFixed(1)} m`);
  if (w.profile.length > 0 && !w.charted) parts.push("route not charted for its draft");
  return parts.join(" · ");
}

/**
 * The route's depth profile as SVG paths in a `width × height` box: the sea bed (filled below)
 * and the draft line. Depths are scaled to the larger of 4 × draft and 50 m; deeper water sits
 * on the bottom edge, so the shallow parts that matter have the room.
 */
export function profilePaths(
  w: WaterView,
  width: number,
  height: number,
): { bed: string; draftY: number | null } {
  const cap = Math.max(50, 4 * (w.draft_m ?? 0));
  const last = w.profile[w.profile.length - 1]?.distance_m ?? 0;
  const span = last > 0 ? last : 1;
  const y = (d: number) => (Math.min(Math.max(d, 0), cap) / cap) * height;
  const pts = w.profile.map((s) => {
    const x = ((s.distance_m ?? 0) / span) * width;
    return `${x.toFixed(1)},${y(s.depth_m ?? cap).toFixed(1)}`;
  });
  const bed = pts.length ? `M0,${height} L${pts.join(" L")} L${width},${height} Z` : "";
  return { bed, draftY: w.draft_m == null ? null : y(w.draft_m) };
}

// ---- Talking to the host ----

export async function terrainStatus(): Promise<TerrainStatus> {
  return commands.terrainStatus();
}

/** Installs a pack after the host checks it against its `.sha256` file. */
export async function installTerrainPack(pack: File, sha256: File | null): Promise<TerrainStatus> {
  const line = sha256 ? await sha256.text() : "";
  const bytes = new Uint8Array(await pack.arrayBuffer());
  return invoke<TerrainStatus>("install_terrain_pack", bytes, { headers: { "x-sha256": line } });
}

async function fetchGrid(
  west: number,
  east: number,
  lats: number[],
  cols: number,
): Promise<Int16Array | null> {
  const buf = await invoke<ArrayBuffer>("terrain_grid", { west, east, lats, cols });
  if (!buf || buf.byteLength === 0) return null;
  return new Int16Array(buf.slice(0));
}

function toDataUrl(px: Uint8ClampedArray, cols: number, rows: number): string {
  const canvas = document.createElement("canvas");
  canvas.width = cols;
  canvas.height = rows;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  ctx.putImageData(new ImageData(px as Uint8ClampedArray<ArrayBuffer>, cols, rows), 0, 0);
  return canvas.toDataURL("image/png");
}

const CONTOUR_COLOUR: Record<TerrainTheme, string> = {
  dark: "rgba(150, 196, 214, 0.32)",
  light: "rgba(40, 92, 128, 0.35)",
};

/** What the map shows of the terrain. */
export interface TerrainView {
  depth: boolean;
  contours: boolean;
  relief: boolean;
}

/**
 * Keeps the map's terrain layers in step with the view: fetches a grid for the visible area
 * when the map stops moving, and redraws when the theme or the choice of layers changes.
 */
export class TerrainLayers {
  private grid: TerrainGrid | null = null;
  private seq = 0;
  private view: TerrainView = { depth: true, contours: true, relief: true };
  private theme: TerrainTheme = "dark";
  private available = false;

  constructor(
    private map: maplibregl.Map,
    private credit: () => string,
  ) {
    map.on("moveend", () => void this.refresh());
    // A window resize changes the visible area without a move.
    map.on("resize", () => void this.refresh());
  }

  /** The height under a point of the current grid, if any. */
  heightAt(lon: number, lat: number): number | null {
    return this.grid ? heightAt(this.grid, lon, lat) : null;
  }

  set(view: TerrainView, theme: TerrainTheme, available: boolean) {
    const fetchNeeded = available && !this.available;
    this.view = view;
    this.theme = theme;
    this.available = available;
    if (!available) this.grid = null;
    if (fetchNeeded) void this.refresh();
    else this.draw();
  }

  async refresh() {
    const on = this.view.depth || this.view.contours || this.view.relief;
    if (!this.available || !on) {
      this.draw();
      return;
    }
    const seq = ++this.seq;
    const b = this.map.getBounds();
    let west = b.getWest();
    let east = b.getEast();
    if (east - west >= 360) {
      west = -180;
      east = 180;
    }
    const north = Math.min(b.getNorth(), MAX_MERC_LAT);
    const south = Math.max(b.getSouth(), -MAX_MERC_LAT);
    const canvas = this.map.getCanvas();
    const cols = Math.max(16, Math.min(MAX_CELLS, Math.round(canvas.clientWidth / 2)));
    const rows = Math.max(16, Math.min(MAX_CELLS, Math.round(canvas.clientHeight / 2)));
    const lats = mercatorLats(north, south, rows);
    let heights: Int16Array | null;
    try {
      heights = await fetchGrid(west, east, lats, cols);
    } catch {
      heights = null;
    }
    if (seq !== this.seq) return; // a newer view has been asked for
    this.grid = heights ? { west, east, north, south, lats, cols, heights } : null;
    this.draw();
  }

  private draw() {
    const m = this.map;
    if (!m.isStyleLoaded()) {
      m.once("idle", () => this.draw());
      return;
    }
    const g = this.available ? this.grid : null;
    const corners = g
      ? ([
          [g.west, g.north],
          [g.east, g.north],
          [g.east, g.south],
          [g.west, g.south],
        ] as [[number, number], [number, number], [number, number], [number, number]])
      : null;
    if (g && corners && this.view.depth) {
      this.image(
        "terrain-depth",
        "grid",
        toDataUrl(depthPixels(g, this.theme), g.cols, g.lats.length),
        corners,
      );
    } else this.hide("terrain-depth");
    if (g && corners && this.view.relief) {
      this.image(
        "terrain-relief",
        "coast",
        toDataUrl(reliefPixels(g), g.cols, g.lats.length),
        corners,
      );
    } else this.hide("terrain-relief");
    const lines =
      g && this.view.contours
        ? contourLines(g, contourLevels(m.getZoom()))
        : { type: "FeatureCollection" as const, features: [] };
    const src = m.getSource("terrain-contours") as maplibregl.GeoJSONSource | undefined;
    if (src) src.setData(lines);
    else {
      m.addSource("terrain-contours", {
        type: "geojson",
        data: lines,
        attribution: this.credit(),
      });
      m.addLayer(
        {
          id: "terrain-contours",
          type: "line",
          source: "terrain-contours",
          paint: {
            "line-color": CONTOUR_COLOUR[this.theme],
            "line-width": ["case", ["get", "major"], 1.1, 0.6],
          },
        },
        "land",
      );
    }
    m.setPaintProperty("terrain-contours", "line-color", CONTOUR_COLOUR[this.theme]);
  }

  private hide(id: string) {
    if (this.map.getLayer(id)) this.map.setLayoutProperty(id, "visibility", "none");
  }

  private image(
    id: string,
    before: string,
    url: string,
    corners: [[number, number], [number, number], [number, number], [number, number]],
  ) {
    const m = this.map;
    const src = m.getSource(id) as maplibregl.ImageSource | undefined;
    if (src) src.updateImage({ url, coordinates: corners });
    else {
      m.addSource(id, { type: "image", url, coordinates: corners });
      m.addLayer(
        {
          id,
          type: "raster",
          source: id,
          paint: {
            "raster-fade-duration": 0,
            "raster-resampling": "linear",
            // Dimmed over the imagery base layer, so the imagery shows through.
            "raster-opacity": id === "terrain-depth" && m.getLayer("imagery") ? 0.45 : 1,
          },
        },
        before,
      );
    }
    m.setLayoutProperty(id, "visibility", "visible");
  }
}
