// @vitest-environment jsdom
// Unit tests for the desktop UI helpers: display formatting, symbology and map decluttering.

import { describe, expect, it } from "vitest";
import { bearing, distance, gameTime, knots, latLon, MPS_PER_KNOT, project } from "./format";
import { frameSvg, symbolUrl } from "./symbols";
import {
  buildLayers,
  circleRing,
  contactAffiliation,
  contactAge,
  contactKind,
  CONTAINMENT_95,
  ellipseRing,
  LABEL_LIMIT,
} from "./map";
import type { ContactView, UnitView } from "./bindings";
// @ts-expect-error: plain ES module without type declarations
import { satisfiable } from "../../scripts/check-licenses.mjs";

const unit = (id: number, overrides: Partial<UnitView> = {}): UnitView => ({
  id,
  name: `Unit ${id}`,
  side: 1,
  domain: "surface",
  position: { lat_deg: -39, lon_deg: 146 },
  altitude_m: 0,
  heading_deg: 90,
  speed_mps: 8,
  ordered_speed_mps: 8,
  max_speed_mps: 15,
  platform: "GEN-FFG",
  route: null,
  group: null,
  doctrine: { wcs: "tight", emcon: "active", engage_unfriendly: false },
  doctrine_set: { wcs: null, emcon: null, engage_unfriendly: null },
  status: "operational",
  damage_fraction: 0,
  criticals: [],
  fires: 0,
  flooding: false,
  mission: null,
  mission_complete: false,
  stores: [],
  ...overrides,
});

describe("format", () => {
  it("formats game time, speed, bearing and position", () => {
    expect(gameTime(0)).toBe("D+0 00:00:00");
    expect(gameTime(90061)).toBe("D+1 01:01:01");
    expect(knots(10 * MPS_PER_KNOT)).toBe("10.0 kn");
    expect(knots(null)).toBe("—");
    expect(bearing(5)).toBe("005°");
    expect(distance(430.4)).toBe("430 m");
    expect(distance(1223.85)).toBe("1.2 km");
    expect(latLon(-39.5, 146.25)).toBe("39°30.0′S 146°15.0′E");
  });

  it("projects a point due east along the equator by the expected arc", () => {
    const [lon, lat] = project(0, 0, 90, 111_194.93);
    expect(lat).toBeCloseTo(0, 9);
    expect(lon).toBeCloseTo(1, 4);
  });
});

describe("symbols", () => {
  it("uses a different frame per domain and per affiliation", () => {
    const frames = new Set<string>();
    for (const d of ["surface", "air", "subsurface", "land"] as const) {
      for (const a of ["friend", "hostile", "neutral", "unknown"] as const) {
        frames.add(`${d}:${frameSvg(a, d)}`);
      }
    }
    expect(frames.size).toBe(16);
    expect(frameSvg("friend", "subsurface")).toContain("matrix(1 0 0 -1 0 64)");
  });

  it("returns cached, valid SVG data URLs", () => {
    const a = symbolUrl("friend", false, "air");
    expect(a).toBe(symbolUrl("friend", false, "air"));
    expect(a.startsWith("data:image/svg+xml")).toBe(true);
    expect(a).not.toBe(symbolUrl("friend", true, "air"));
  });
});

describe("map layers", () => {
  const scene = (units: UnitView[], selected: number | null = null) =>
    buildLayers({
      units,
      contacts: [],
      gameTimeS: 0,
      viewer: { kind: "white_cell" },
      selected,
      draft: null,
      onPickUnit: () => {},
    });
  const layer = (layers: ReturnType<typeof scene>, id: string) =>
    layers.find((l) => l.id === id) as unknown as { props: { data: unknown[] } };

  it("labels every unit at or below the declutter limit", () => {
    const units = Array.from({ length: LABEL_LIMIT }, (_, i) => unit(i + 1));
    expect(layer(scene(units), "labels").props.data).toHaveLength(LABEL_LIMIT);
    expect(layer(scene(units), "leaders").props.data).toHaveLength(LABEL_LIMIT);
  });

  it("labels only the selected unit above the limit, but draws every symbol", () => {
    const units = Array.from({ length: LABEL_LIMIT + 1 }, (_, i) => unit(i + 1));
    const layers = scene(units, 7);
    expect(layer(layers, "labels").props.data).toHaveLength(1);
    expect(layer(layers, "leaders").props.data).toHaveLength(1);
    expect(layer(layers, "units").props.data).toHaveLength(LABEL_LIMIT + 1);
  });

  it("draws the 95 % error ellipse with its major axis on the reported bearing", () => {
    const c = {
      position: { lat_deg: 0, lon_deg: 0 },
      error: { semi_major_m: 1000, semi_minor_m: 100, orientation_deg: 0 },
      domain: "surface",
    } as ContactView;
    const ring = ellipseRing(c, 4);
    expect(ring).toHaveLength(5);
    // t = 0: the major axis tip, due north at 2.4477 km: 2447.7 m / 111 195 m per degree.
    expect(ring[0]?.[0]).toBeCloseTo(0, 9);
    expect(ring[0]?.[1]).toBeCloseTo((1000 * CONTAINMENT_95) / 111_194.93, 6);
    // t = 90°: the minor axis tip, due east at 244.77 m.
    expect(ring[1]?.[0]).toBeCloseTo((100 * CONTAINMENT_95) / 111_194.93, 6);
    expect(ring[4]).toEqual(ring[0]);
    expect(contactKind(c)).toBe("surface");
    expect(contactKind({ ...c, category: "surface_combatant" })).toBe("surface combatant");
    expect(contactKind({ ...c, category: "surface_combatant", platform: "GEN-FFG" })).toBe(
      "GEN-FFG",
    );
    expect(contactKind({ domain: null } as ContactView)).toBe("unknown");
    expect(contactAffiliation(c)).toBe("unknown");
    expect(contactAffiliation({ ...c, affiliation: "unfriendly" })).toBe("hostile");
    expect(contactAffiliation({ ...c, affiliation: "neutral" })).toBe("neutral");
  });

  it("draws a minefield ring at its radius", () => {
    const ring = circleRing({ lat_deg: 0, lon_deg: 0 }, 2_000, 4);
    expect(ring).toHaveLength(5);
    expect(ring[0]?.[1]).toBeCloseTo(2_000 / 111_194.93, 6);
    expect(ring[4]).toEqual(ring[0]);
  });

  it("formats contact age from the last detection", () => {
    const c = { last_seen_s: 100 } as ContactView;
    expect(contactAge(130, c)).toBe("30 s");
    expect(contactAge(100 + 600, c)).toBe("10 min");
  });
});

describe("npm licence policy", () => {
  it("accepts permissive licences and rejects copyleft ones", () => {
    expect(satisfiable("MIT")).toBe(true);
    expect(satisfiable("(MIT OR Apache-2.0)")).toBe(true);
    expect(satisfiable("GPL-3.0 OR MIT")).toBe(true);
    expect(satisfiable("MIT AND GPL-3.0")).toBe(false);
    expect(satisfiable("AGPL-3.0")).toBe(false);
    expect(satisfiable("LGPL-2.1-only")).toBe(false);
  });
});

describe("day/night shading", () => {
  it("puts the sun where the engine's solar model does (hand values)", async () => {
    const { subsolarPoint, nightPolygon } = await import("./map");
    // 1970-01-01 00:00 UTC: midnight at Greenwich, so the sun is over 180°; day 0, so the
    // declination is −23.44 × cos(360/365 × 10°) = −23.09°.
    const s = subsolarPoint(0);
    expect(s.lon).toBeCloseTo(180, 9);
    expect(s.lat).toBeCloseTo(-23.09, 2);
    // Six hours later the sun is over 90° E.
    expect(subsolarPoint(6 * 3600).lon).toBeCloseTo(90, 9);
    const ring = nightPolygon(0);
    const at = (lon: number) => ring.find(([x]) => x === lon)?.[1];
    // A quarter of the way round from the sun the terminator crosses the equator.
    expect(at(90)).toBeCloseTo(0, 6);
    // Under the sun it is at atan(−1 / tan(−23.09°)) = 66.9° N: the polar night beyond.
    expect(at(180)).toBeCloseTo(66.9, 1);
    // Southern summer: the night side holds the north pole.
    expect(ring.at(-1)?.[1]).toBe(85);
  });
});
