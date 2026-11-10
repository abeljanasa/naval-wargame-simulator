// Display helpers. The engine works in SI units (m, m/s, s). The UI shows nautical units.

export const MPS_PER_KNOT = 1852 / 3600;

/** specta types f64 as `number | null` (serde writes NaN as null). Map null back to NaN for maths. */
export function n(x: number | null | undefined): number {
  return x ?? Number.NaN;
}

export function knots(mps: number | null): string {
  const v = n(mps) / MPS_PER_KNOT;
  return Number.isFinite(v) ? `${v.toFixed(1)} kn` : "—";
}

/** A distance: metres below 1 km, else km to one decimal. */
export function distance(m: number | null): string {
  const v = n(m);
  if (!Number.isFinite(v)) return "—";
  return v < 1000 ? `${Math.round(v)} m` : `${(v / 1000).toFixed(1)} km`;
}

export function bearing(deg: number | null): string {
  const v = n(deg);
  return Number.isFinite(v) ? `${Math.round(v).toString().padStart(3, "0")}°` : "—";
}

/** Game time as D+d HH:MM:SS (day offset from scenario start). */
export function gameTime(seconds: number | null): string {
  const s = Math.max(0, Math.floor(n(seconds) || 0));
  const d = Math.floor(s / 86400);
  const hh = Math.floor((s % 86400) / 3600);
  const mm = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  const p = (x: number) => x.toString().padStart(2, "0");
  return `D+${d} ${p(hh)}:${p(mm)}:${p(ss)}`;
}

/** Degrees → D°MM.m′H, the format used on charts. */
export function latLon(lat: number | null, lon: number | null): string {
  const fmt = (v: number, pos: string, neg: string, degWidth: number) => {
    const h = v >= 0 ? pos : neg;
    const a = Math.abs(v);
    const d = Math.floor(a);
    const m = (a - d) * 60;
    return `${d.toString().padStart(degWidth, "0")}°${m.toFixed(1).padStart(4, "0")}′${h}`;
  };
  const la = n(lat);
  const lo = n(lon);
  if (!Number.isFinite(la) || !Number.isFinite(lo)) return "—";
  return `${fmt(la, "N", "S", 2)} ${fmt(lo, "E", "W", 3)}`;
}

/**
 * Display-only spherical forward projection, used to draw heading leaders. Simulation geodesy
 * is done in Rust on the WGS-84 ellipsoid; nothing here feeds back into the engine.
 */
export function project(
  lat: number,
  lon: number,
  bearingDeg: number,
  distM: number,
): [number, number] {
  const R = 6_371_000;
  const toRad = Math.PI / 180;
  const phi1 = lat * toRad;
  const lambda1 = lon * toRad;
  const theta = bearingDeg * toRad;
  const delta = distM / R;
  const phi2 = Math.asin(
    Math.sin(phi1) * Math.cos(delta) + Math.cos(phi1) * Math.sin(delta) * Math.cos(theta),
  );
  const lambda2 =
    lambda1 +
    Math.atan2(
      Math.sin(theta) * Math.sin(delta) * Math.cos(phi1),
      Math.cos(delta) - Math.sin(phi1) * Math.sin(phi2),
    );
  return [((lambda2 / toRad + 540) % 360) - 180, phi2 / toRad];
}
