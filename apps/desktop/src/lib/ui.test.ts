// @vitest-environment jsdom
// Unit tests for the desktop UI helpers: display formatting, symbology and map decluttering.

import { describe, expect, it } from "vitest";
import { bearing, gameTime, knots, latLon, MPS_PER_KNOT, project } from "./format";
import { frameSvg, symbolUrl } from "./symbols";
import { buildLayers, contactAge, LABEL_LIMIT } from "./map";
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
  ...overrides,
});

describe("format", () => {
  it("formats game time, speed, bearing and position", () => {
    expect(gameTime(0)).toBe("D+0 00:00:00");
    expect(gameTime(90061)).toBe("D+1 01:01:01");
    expect(knots(10 * MPS_PER_KNOT)).toBe("10.0 kn");
    expect(knots(null)).toBe("—");
    expect(bearing(5)).toBe("005°");
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
