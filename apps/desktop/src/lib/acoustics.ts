// Sound-velocity profiles, towed arrays and sonobuoys in the panels and on the map (Phase 4
// slice 8, docs/models/acoustics.md). The engine decides what sound does; this module reads
// profiles in, checks them as the engine will, and shows the state.

import { ScatterplotLayer, TextLayer } from "@deck.gl/layers";
import type { Layer } from "@deck.gl/core";
import type {
  SonobuoyView,
  SoundProfile_Serialize as SoundProfile,
  SvpPoint,
  TowedView,
} from "./bindings";
import { n } from "./format";

export type { SoundProfile, SvpPoint };

/** Why a profile's points would be refused, or null (the engine's `validate_svp`). */
export function svpProblem(points: SvpPoint[]): string | null {
  if (points.length < 2) return "a sound-velocity profile needs at least two points";
  if (n(points[0]?.depth_m) !== 0)
    return "a sound-velocity profile starts at the surface (depth 0)";
  for (let i = 1; i < points.length; i++) {
    if (!(n(points[i]?.depth_m) > n(points[i - 1]?.depth_m))) return "profile depths must increase";
  }
  if (points.some((p) => !(n(p.speed_mps) >= 1400 && n(p.speed_mps) <= 1600)))
    return "sound speeds must be 1,400–1,600 m/s";
  if (points.some((p) => !(n(p.depth_m) >= 0 && n(p.depth_m) <= 12000)))
    return "profile depths must be 0–12,000 m";
  return null;
}

/** Profile points from CSV text: one `depth_m,speed_mps` pair per line; a header line, blank
 * lines and `#` comments are skipped; `;` or tabs also separate. Notes say what was skipped. */
export function svpFromCsv(text: string): { points: SvpPoint[]; notes: string[] } {
  const points: SvpPoint[] = [];
  const notes: string[] = [];
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const cells = line.split(/[,;\t]/).map((c) => c.trim());
    const [d, c] = [Number(cells[0]), Number(cells[1])];
    if (
      cells.length < 2 ||
      cells[0] === "" ||
      cells[1] === "" ||
      !Number.isFinite(d) ||
      !Number.isFinite(c)
    ) {
      // A header before the first point is expected; anything else is worth saying.
      if (points.length > 0 || !/[a-z]/i.test(line)) notes.push(`line ${i + 1} skipped: ${line}`);
      return;
    }
    points.push({ depth_m: d, speed_mps: c });
  });
  points.sort((a, b) => n(a.depth_m) - n(b.depth_m));
  return { points, notes };
}

/** Depth of the surface layer, m: where speed peaks within the top 500 m (0: no layer). The
 * engine's `mixed_layer_depth`. */
export function layerDepthM(points: SvpPoint[]): number {
  let best = { d: 0, c: n(points[0]?.speed_mps) };
  for (const p of points) {
    if (n(p.depth_m) > 500) break;
    if (n(p.speed_mps) > best.c) best = { d: n(p.depth_m), c: n(p.speed_mps) };
    else if (n(p.speed_mps) < best.c) break;
  }
  return best.d;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** A profile in a few words. */
export function profileText(p: SoundProfile): string {
  const layer = layerDepthM(p.points);
  const deepest = n(p.points[p.points.length - 1]?.depth_m);
  const parts = [layer > 0 ? `layer ${Math.round(layer)} m` : "no surface layer"];
  parts.push(`${p.points.length} points to ${Math.round(deepest)} m`);
  parts.push(
    p.months.length ? p.months.map((m) => MONTHS[m - 1] ?? String(m)).join(" ") : "all year",
  );
  parts.push(p.area ? "in its area" : "everywhere");
  return parts.join(" · ");
}

/** The towed array's state in a few words. */
export function towedText(t: TowedView): string {
  const wait = n(t.ready_in_s);
  if (wait <= 0) return t.streamed ? "streamed" : "recovered";
  const min = Math.ceil(wait / 60);
  return t.streamed ? `streaming, ready in ${min} min` : `recovering, ${min} min`;
}

/** Laid sonobuoys: a small ring at each, and its life left in hours. */
export function buoyLayers(buoys: SonobuoyView[]): Layer[] {
  const pos = (b: SonobuoyView): [number, number] => [n(b.position.lon_deg), n(b.position.lat_deg)];
  return [
    new ScatterplotLayer<SonobuoyView>({
      id: "sonobuoys",
      data: buoys,
      getPosition: pos,
      getRadius: 4,
      radiusUnits: "pixels",
      stroked: true,
      filled: true,
      getFillColor: [11, 27, 43, 200],
      getLineColor: [120, 210, 200, 230],
      lineWidthUnits: "pixels",
      getLineWidth: 1.5,
    }),
    new TextLayer<SonobuoyView>({
      id: "sonobuoy-labels",
      data: buoys,
      getPosition: pos,
      getText: (b) => `${(n(b.life_s) / 3600).toFixed(1)} h`,
      getSize: 10,
      getPixelOffset: [0, 12],
      getColor: [160, 220, 210, 200],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
    }),
  ];
}
