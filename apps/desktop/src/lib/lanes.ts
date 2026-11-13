// Shipping lanes in the white cell's planning tab (Phase 4 slice 5, docs/models/choke-points.md):
// importing them from GeoJSON, and the words and pictures for a choke point's water.

import type { GeoPoint, Lane_Deserialize as Lane } from "./bindings";

export type { Lane };

/** Lane width when the file gives none, metres (sim-core `DEFAULT_LANE_WIDTH_M`). */
export const DEFAULT_LANE_WIDTH_M = 2000;

type Feature = {
  type?: string;
  geometry?: { type?: string; coordinates?: unknown } | null;
  properties?: Record<string, unknown> | null;
};

const num = (v: unknown): number | null => {
  if (typeof v === "number") return Number.isFinite(v) ? v : null;
  if (typeof v === "string" && v.trim() !== "" && Number.isFinite(Number(v))) return Number(v);
  return null;
};
const str = (v: unknown): string | null => (typeof v === "string" && v.trim() ? v.trim() : null);

function lines(g: Feature["geometry"]): GeoPoint[][] {
  const pt = (c: unknown): GeoPoint | null =>
    Array.isArray(c) && typeof c[0] === "number" && typeof c[1] === "number"
      ? { lat_deg: c[1], lon_deg: c[0] }
      : null;
  const line = (cs: unknown): GeoPoint[] =>
    Array.isArray(cs) ? cs.map(pt).filter((p): p is GeoPoint => p !== null) : [];
  if (g?.type === "LineString") return [line(g.coordinates)];
  if (g?.type === "MultiLineString" && Array.isArray(g.coordinates)) return g.coordinates.map(line);
  return [];
}

/**
 * Lanes from a GeoJSON file of LineString (or MultiLineString) features. Properties read:
 * `name`, `width_m`, `ships_per_day`, `source`, `choke_point`. A traffic figure without a
 * `source` is dropped (and noted): traffic data comes in only from a named, licensed source.
 * Ids follow on from `firstId`.
 */
export function lanesFromGeoJson(
  text: string,
  firstId: number,
  fileName: string,
): { lanes: Lane[]; notes: string[] } {
  const notes: string[] = [];
  let doc: unknown;
  try {
    doc = JSON.parse(text);
  } catch {
    return { lanes: [], notes: [`${fileName} is not JSON`] };
  }
  const d = doc as { type?: string; features?: Feature[] } & Feature;
  const features: Feature[] =
    d.type === "FeatureCollection" && Array.isArray(d.features) ? d.features : [d];
  const lanes: Lane[] = [];
  let id = firstId;
  features.forEach((f, i) => {
    const p = f.properties ?? {};
    const name = str(p.name) ?? `${fileName} lane ${i + 1}`;
    const source = str(p.source);
    let traffic = num(p.ships_per_day);
    if (traffic !== null && source === null) {
      notes.push(`${name}: traffic dropped, the file names no source for it`);
      traffic = null;
    }
    for (const points of lines(f.geometry)) {
      if (points.length < 2) continue;
      lanes.push({
        id: id++,
        name,
        points,
        width_m: Math.max(1, num(p.width_m) ?? DEFAULT_LANE_WIDTH_M),
        ships_per_day: traffic,
        source: source ?? fileName,
        choke_point: str(p.choke_point),
      });
    }
  });
  if (lanes.length === 0) notes.push(`${fileName} has no LineString features`);
  return { lanes, notes };
}

/** The width of water at least `depth_m` deep along a choke point's section, metres: the
 * longest unbroken run of samples that deep. */
export function navigableWidthM(
  section: { distance_m: number | null; depth_m: number | null }[],
  depth_m: number,
): number {
  let best = 0;
  let start: number | null = null;
  for (const s of section) {
    const d = s.distance_m ?? 0;
    if ((s.depth_m ?? -1) >= depth_m) {
      start ??= d;
      best = Math.max(best, d - start);
    } else {
      start = null;
    }
  }
  return best;
}
