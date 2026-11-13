// @vitest-environment jsdom
// Unit tests for the desktop UI helpers: display formatting, symbology and map decluttering.

import { describe, expect, it } from "vitest";
import { bearing, distance, gameTime, knots, latLon, MPS_PER_KNOT, project } from "./format";
import { frameSvg, functionCode, symbolUrl } from "./symbols";
import { figureKind, figureShape, figureUrl, munitionKind } from "./silhouettes";
import {
  buildLayers,
  circleRing,
  contactAffiliation,
  contactAge,
  contactKind,
  CONTAINMENT_95,
  ellipseRing,
  LABEL_LIMIT,
  ROUTE_STUB_M,
  outcomeText,
  declutter,
  distanceM,
  labelCandidates,
  zoneAnchor,
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

  it("puts a function code in the frame: by class, else category, else none", () => {
    expect(functionCode("GEN-DDG")).toBe("DDG");
    expect(functionCode("GEN-SSK-AIP")).toBe("SSK");
    expect(functionCode("GEN-FTR-MR")).toBe("F");
    // An unknown class falls back to its category; a contact classified to category shows it.
    expect(functionCode("XYZ-1", "submarine")).toBe("SS");
    expect(functionCode(null, "helicopter")).toBe("H");
    // A surface combatant of unknown type, and an unclassified contact, show the frame alone.
    expect(functionCode(null, "surface_combatant")).toBe("");
    expect(functionCode(null, null)).toBe("");
    const url = decodeURIComponent(symbolUrl("hostile", false, "subsurface", "SSK"));
    expect(url).toContain(">SSK</text>");
    expect(symbolUrl("hostile", false, "subsurface", "SSK")).not.toBe(
      symbolUrl("hostile", false, "subsurface"),
    );
  });

  it("draws a figure by class, else category, else domain; munitions by weapon id", () => {
    expect(figureKind("GEN-FFG")).toBe("frigate");
    expect(figureKind("GEN-SSK")).toBe("sub_diesel");
    expect(figureKind("XYZ", "submarine")).toBe("sub");
    expect(figureKind(null, null, "air")).toBe("aircraft");
    expect(figureKind(null, null, null)).toBeNull();
    // Ships, submarines and aircraft are plan views turned to the heading; land sites upright.
    expect(figureShape("frigate")).toBe("topdown");
    expect(figureShape("sub_diesel")).toBe("topdown");
    expect(figureShape("fighter")).toBe("topdown");
    expect(figureShape("radar_site")).toBe("icon");
    expect(decodeURIComponent(figureUrl("frigate", "friend", false))).toContain(
      'viewBox="0 0 64 64"',
    );
    expect(munitionKind("WPN-ASM-SUB")).toBe("anti_ship_missile");
    expect(munitionKind("WPN-TORP-HW")).toBe("torpedo");
    expect(munitionKind("WPN-GUN-76")).toBe("shell");
  });

  it("has a figure for every generic platform class and every weapon", async () => {
    const { readFileSync, readdirSync } = await import("node:fs");
    const { join } = await import("node:path");
    const dir = join(process.cwd(), "../../data/db/open/platforms");
    const ids = readdirSync(dir)
      .filter((f) => f.endsWith(".json"))
      .flatMap((f) =>
        (
          JSON.parse(readFileSync(join(dir, f), "utf8")) as { platforms: { id: string }[] }
        ).platforms.map((p) => p.id),
      );
    // A class's own figure, not just its domain's.
    expect(ids.filter((id) => figureKind(id) === null)).toEqual([]);
    const weapons = (
      JSON.parse(readFileSync(join(process.cwd(), "../../data/db/open/weapons.json"), "utf8")) as {
        weapons: { id: string; kind: string }[];
      }
    ).weapons;
    for (const w of weapons) {
      const k = munitionKind(w.id);
      if (w.kind === "gun" || w.kind === "ciws") expect(k).toBe("shell");
      else expect(k).not.toBe("shell");
    }
  });

  it("words a weapon outcome for each viewer", () => {
    const o = {
      tick: 100,
      weapon: "WPN-ASM-SUB",
      result: "hit" as const,
      position: { lat_deg: 0, lon_deg: 0 },
      shooter: 2,
      shooter_name: "Bastion",
      shooter_side: 1,
      target: 11,
      target_name: "Cinder",
      target_side: 2,
      reason: null,
    };
    expect(outcomeText(o, null)).toBe("Anti-ship missile from Bastion hit Cinder");
    expect(outcomeText({ ...o, target: null, target_name: null }, 1)).toBe(
      "Our anti-ship missile from Bastion hit its target",
    );
    expect(outcomeText({ ...o, shooter: null, shooter_name: null }, 2)).toBe(
      "Cinder was hit by an enemy anti-ship missile",
    );
    expect(outcomeText({ ...o, result: "miss", reason: "seeker did not acquire" }, null)).toBe(
      "Anti-ship missile from Bastion missed Cinder (seeker did not acquire)",
    );
    const down = { ...o, result: "shot_down" as const, reason: "shot down by Cinder" };
    expect(outcomeText(down, null)).toBe(
      "Anti-ship missile from Bastion at Cinder was shot down by Cinder",
    );
    expect(outcomeText({ ...down, target: null, target_name: null }, 1)).toBe(
      "Our anti-ship missile from Bastion was shot down",
    );
  });

  it("has a function code for every generic platform class", async () => {
    const { readFileSync, readdirSync } = await import("node:fs");
    const { join } = await import("node:path");
    // Tests run from apps/desktop.
    const dir = join(process.cwd(), "../../data/db/open/platforms");
    const ids = readdirSync(dir)
      .filter((f) => f.endsWith(".json"))
      .flatMap((f) =>
        (
          JSON.parse(readFileSync(join(dir, f), "utf8")) as { platforms: { id: string }[] }
        ).platforms.map((p) => p.id),
      );
    expect(ids.length).toBeGreaterThan(40);
    expect(ids.filter((id) => functionCode(id) === "")).toEqual([]);
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

  it("draws the selected unit's whole route and only a stub of the others", () => {
    const routed = (id: number) =>
      unit(id, {
        route: {
          waypoints: [
            { lat_deg: -39.6, lon_deg: 146.0 },
            { lat_deg: -39.6, lon_deg: 145.0 },
          ],
          next: 0,
        },
      } as Partial<UnitView>);
    const layers = scene([routed(1), routed(2)], 1);
    const routes = layer(layers, "routes").props.data as { id: number; path: number[][] }[];
    expect(routes.find((r) => r.id === 1)?.path).toHaveLength(3);
    const stub = routes.find((r) => r.id === 2)?.path ?? [];
    expect(stub).toHaveLength(2);
    const [a = [0, 0], b = [0, 0]] = stub;
    const m = distanceM(
      { lat_deg: a[1] ?? 0, lon_deg: a[0] ?? 0 },
      { lat_deg: b[1] ?? 0, lon_deg: b[0] ?? 0 },
    );
    expect(m).toBeLessThanOrEqual(ROUTE_STUB_M * 1.01);
    // Waypoint dots only for the selected unit.
    expect(layer(layers, "waypoints").props.data).toHaveLength(2);
  });

  it("puts a zone's name at its northern edge; labels share one collision group", () => {
    const circle = zoneAnchor({
      name: "S",
      kind: "sense",
      area: { shape: "circle", centre: { lat_deg: 0, lon_deg: 0 }, radius_m: 111_195 },
    } as never);
    expect(circle[1]).toBeCloseTo(1, 2);
    const poly = zoneAnchor({
      name: "E",
      kind: "engage",
      area: {
        shape: "polygon",
        points: [
          { lat_deg: 1, lon_deg: 1 },
          { lat_deg: 3, lon_deg: 2 },
          { lat_deg: 2, lon_deg: 3 },
        ],
      },
    } as never);
    expect(poly).toEqual([2, 3]);
  });

  it("declutters labels by priority in screen space", () => {
    // Identity projection: lon/lat are pixels.
    const at = (x: number, y: number): [number, number] => [x, y];
    const c = (key: string, x: number, priority: number) => ({
      key,
      at: at(x, 100),
      dy: 0,
      size: 12,
      text: "ten chars.",
      priority,
    });
    // Two overlapping labels: the lower priority hides; a distant one stays.
    const hidden = declutter(
      [c("zone", 100, 200), c("unit", 110, 500), c("far", 400, 100)],
      (p) => [p[0], p[1]],
    );
    expect([...hidden]).toEqual(["zone"]);
    // The candidates cover every label kind the picture draws.
    const keys = labelCandidates({
      units: [unit(1)],
      contacts: [],
      minefields: [],
      facilities: [
        {
          name: "Port",
          kind: "sheltering_port",
          position: { lat_deg: 0, lon_deg: 0 },
          radius_m: 1,
        },
      ],
      zones: [],
      points: [{ name: "RV", position: { lat_deg: 0, lon_deg: 0 } }],
      selected: 1,
      gameTimeS: 0,
    }).map((x) => `${x.key}:${x.priority}`);
    expect(keys).toEqual(["u:1:1000", "f:Port:300", "p:RV:100"]);
  });

  it("lays figures flat so they keep their heading in a rotated view; the badge is separate", () => {
    const flat = (layers: ReturnType<typeof scene>) =>
      (layers.find((l) => l.id === "units") as unknown as { props: { billboard: boolean } }).props
        .billboard;
    const layers = scene([unit(1), unit(2)]);
    // Plan views lie on the map (turned with it and tilted with it), not facing the screen.
    expect(flat(layers)).toBe(false);
    // One affiliation badge per unit, drawn beside the name rather than inside the figure.
    expect(layer(layers, "unit-badges").props.data).toHaveLength(2);
    expect(decodeURIComponent(figureUrl("frigate", "friend", false))).not.toContain("scale(");
    // APP-6 frames still face the screen, and carry their own frame.
    const frames = buildLayers({
      units: [unit(1)],
      contacts: [],
      gameTimeS: 0,
      viewer: { kind: "white_cell" },
      selected: null,
      draft: null,
      onPickUnit: () => {},
      symbolStyle: "frames",
    });
    expect(flat(frames)).toBe(true);
    expect(layer(frames, "unit-badges").props.data).toHaveLength(0);
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
