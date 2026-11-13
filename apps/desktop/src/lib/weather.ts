// Weather zones on the map and in the panels (Phase 4 slice 7, docs/models/weather.md). The
// engine decides what the weather does; this module only shows it.

import { PolygonLayer, TextLayer } from "@deck.gl/layers";
import type { Layer } from "@deck.gl/core";
import type { Environment, GeoPoint, WeatherZone_Serialize as WeatherZone } from "./bindings";
import { distance, n } from "./format";
import { circleRing } from "./map";

export type { WeatherZone };

/** Whether `p` is in the zone's area (as the engine decides it: geodesic circle, even–odd
 * polygon). */
export function inZone(z: WeatherZone, p: GeoPoint): boolean {
  const a = z.area;
  if (a.shape === "circle") {
    return geodesicM(a.centre, p) <= n(a.radius_m);
  }
  let inside = false;
  const pts = a.points;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const pi = pts[i];
    const pj = pts[j];
    if (!pi || !pj) continue;
    const [ai, aj] = [n(pi.lat_deg), n(pj.lat_deg)];
    if (ai > n(p.lat_deg) !== aj > n(p.lat_deg)) {
      const x = n(pi.lon_deg) + ((n(p.lat_deg) - ai) / (aj - ai)) * (n(pj.lon_deg) - n(pi.lon_deg));
      if (n(p.lon_deg) < x) inside = !inside;
    }
  }
  return inside;
}

/** Great-circle distance, metres (spherical; for display). */
function geodesicM(a: GeoPoint, b: GeoPoint): number {
  const r = Math.PI / 180;
  const [la, lb] = [n(a.lat_deg) * r, n(b.lat_deg) * r];
  const dl = (n(b.lon_deg) - n(a.lon_deg)) * r;
  const h = Math.sin((lb - la) / 2) ** 2 + Math.cos(la) * Math.cos(lb) * Math.sin(dl / 2) ** 2;
  return 2 * 6_371_000 * Math.asin(Math.min(1, Math.sqrt(h)));
}

/** The weather at `p`: the zone with the highest id there, else the scenario-wide weather. */
export function weatherAt(
  env: Environment | null,
  zones: WeatherZone[],
  p: GeoPoint,
): { zone: WeatherZone | null; sea_state: number; visibility_km: number } {
  const zone = zones.filter((z) => inZone(z, p)).sort((a, b) => b.id - a.id)[0] ?? null;
  return {
    zone,
    sea_state: zone ? zone.sea_state : (env?.sea_state ?? 0),
    visibility_km: n(zone ? zone.visibility_km : (env?.visibility_km ?? 40)),
  };
}

/** The weather of a zone in a few words. */
export function weatherText(z: WeatherZone): string {
  const parts = [`sea state ${z.sea_state}`];
  if (n(z.wind_kn) > 0)
    parts.push(`wind ${Math.round(n(z.wind_kn))} kn from ${Math.round(n(z.wind_from_deg)) % 360}°`);
  parts.push(`visibility ${distance(n(z.visibility_km) * 1000)}`);
  if (n(z.rain_mm_h) > 0) parts.push(`rain ${n(z.rain_mm_h)} mm/h`);
  if (z.cloud_base_m != null) parts.push(`cloud base ${Math.round(n(z.cloud_base_m))} m`);
  return parts.join(" · ");
}

function ring(z: WeatherZone): [number, number][] {
  const a = z.area;
  if (a.shape === "circle") return circleRing(a.centre, n(a.radius_m));
  const r = a.points.map((p): [number, number] => [n(p.lon_deg), n(p.lat_deg)]);
  return r.length ? [...r, r[0] as [number, number]] : r;
}

/** Weather zones: a pale, dashed-looking outline with a light wash where it rains, and the name
 * and sea state at the zone's middle. */
export function weatherLayers(zones: WeatherZone[]): Layer[] {
  const centre = (z: WeatherZone): [number, number] => {
    const r = ring(z);
    const lon = r.reduce((s, p) => s + p[0], 0) / Math.max(1, r.length);
    const lat = r.reduce((s, p) => s + p[1], 0) / Math.max(1, r.length);
    return [lon, lat];
  };
  return [
    new PolygonLayer<WeatherZone>({
      id: "weather-zones",
      data: zones,
      getPolygon: ring,
      stroked: true,
      filled: true,
      getFillColor: (z) => (n(z.rain_mm_h) > 0 ? [150, 170, 200, 28] : [0, 0, 0, 0]),
      getLineColor: [176, 196, 222, 150],
      lineWidthUnits: "pixels",
      getLineWidth: 1.5,
    }),
    new TextLayer<WeatherZone>({
      id: "weather-labels",
      data: zones,
      getPosition: centre,
      getText: (z) => `${z.name} · SS${z.sea_state}`,
      getSize: 11,
      getColor: [196, 212, 232, 220],
      fontFamily: "system-ui, sans-serif",
      outlineWidth: 2,
      outlineColor: [11, 27, 43, 255],
      fontSettings: { sdf: true },
    }),
  ];
}
