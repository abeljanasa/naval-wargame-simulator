// Unit tests for the scenario editor's pure helpers. The engine's own checks are tested in Rust
// (sim-scenario `check`, desktop `editor_check` and `event_templates_are_valid`).

import { describe, expect, it } from "vitest";
import {
  addChokePoint,
  addFacility,
  addSide,
  addUnit,
  blankScenario,
  cloneDraft,
  duplicateUnit,
  EVENT_TEMPLATES,
  parseJsonArray,
  postureOf,
  removeSide,
  removeUnit,
  SCENARIO_FORMAT_VERSION,
  setPosture,
  toJson,
  uniqueEventId,
} from "./scenario-edit";

const here = { lat_deg: -39, lon_deg: 146 };

describe("scenario drafts", () => {
  it("starts blank with two hostile sides at the current format", () => {
    const s = blankScenario();
    expect(s.format_version).toBe(SCENARIO_FORMAT_VERSION);
    expect(s.sides.map((x) => x.name)).toEqual(["Blue", "Red"]);
    expect(postureOf(s, 1, 2)).toBe("hostile");
    expect(s.units).toEqual([]);
  });

  it("adds, duplicates and removes units with fresh ids", () => {
    const s = blankScenario();
    const a = addUnit(s, 1, here, "GEN-FFG");
    const b = addUnit(s, 2, here, undefined);
    expect([a, b]).toEqual([1, 2]);
    expect(s.units[0]?.name).toBe("Blue 1");
    expect(s.units[1]?.platform).toBeUndefined();
    const c = duplicateUnit(s, a);
    expect(c).toBe(3);
    expect(s.units[2]?.position.lat_deg).toBeCloseTo(-39.02);
    expect(s.units[2]?.platform).toBe("GEN-FFG");
    removeUnit(s, a);
    expect(s.units.map((u) => u.id)).toEqual([2, 3]);
    expect(duplicateUnit(s, 99)).toBeNull();
  });

  it("sets postures, writing only what is not neutral", () => {
    const s = blankScenario();
    setPosture(s, 1, 2, "neutral");
    expect(s.sides[0]?.postures).toEqual([]);
    expect(postureOf(s, 1, 2)).toBe("neutral");
    setPosture(s, 1, 2, "unfriendly");
    expect(postureOf(s, 1, 2)).toBe("unfriendly");
    setPosture(s, 1, 1, "hostile");
    expect(postureOf(s, 1, 1)).toBe("neutral");
  });

  it("removing a side takes its units, minefields, points and postures toward it", () => {
    const s = blankScenario();
    const green = addSide(s, " Green ");
    expect(green).toBe(3);
    expect(s.sides[2]?.name).toBe("Green");
    setPosture(s, 1, green, "friendly");
    addUnit(s, green, here, undefined);
    addUnit(s, 1, here, undefined);
    s.minefields = [{ side: green, centre: here, radius_m: 1000, mines: 5, weapon: "M" }];
    s.reference_points = [
      { name: "G", position: here, side: green },
      { name: "All", position: here },
    ];
    expect(removeSide(s, green)).toBe(1);
    expect(s.units.map((u) => u.side)).toEqual([1]);
    expect(s.minefields).toEqual([]);
    expect(s.reference_points?.map((r) => r.name)).toEqual(["All"]);
    expect(postureOf(s, 1, green)).toBe("neutral");
  });

  it("saves as formatted JSON at the current version, and clones deeply", () => {
    const s = blankScenario();
    s.format_version = 2;
    const text = toJson(s);
    expect(text.endsWith("\n")).toBe(true);
    expect(JSON.parse(text).format_version).toBe(SCENARIO_FORMAT_VERSION);
    const c = cloneDraft(s);
    const first = c.sides[0];
    if (first) first.name = "Changed";
    expect(s.sides[0]?.name).toBe("Blue");
  });

  it("parses JSON arrays typed by the user", () => {
    expect(parseJsonArray("")).toEqual({ value: [] });
    expect(parseJsonArray("[1, 2]")).toEqual({ value: [1, 2] });
    expect(parseJsonArray("{}")).toHaveProperty("error");
    expect(parseJsonArray("[1,")).toHaveProperty("error");
  });

  it("gives template events unique ids", () => {
    expect(EVENT_TEMPLATES.length).toBeGreaterThan(0);
    const events = [{ id: "a" }, { id: "a-2" }];
    expect(uniqueEventId(events, "b")).toBe("b");
    expect(uniqueEventId(events, "a")).toBe("a-3");
  });

  it("adds facilities with their kind's defaults and choke points as zone pairs", () => {
    const s = blankScenario();
    expect(addFacility(s, 1, "sheltering_port", here)).toBe(1);
    expect(addFacility(s, 1, "command_post", here)).toBe(2);
    expect(s.facilities?.[0]?.services).toEqual({ repair: true, rearm: true, refuel: true });
    expect(s.facilities?.[1]?.range_m).toBe(200_000);
    expect(addChokePoint(s, 1, here, "Narrows")).toEqual([1, 2]);
    expect(addChokePoint(s, 2, here, "Gap")).toEqual([3, 4]);
    const [sense, engage] = s.zones ?? [];
    expect(sense?.kind).toBe("sense");
    expect(engage?.doctrine).toEqual({ wcs: "free", engage_unfriendly: true });
    expect(engage?.choke_point).toBe("Narrows");
    removeSide(s, 2);
    expect(s.zones?.map((z) => z.id)).toEqual([1, 2]);
  });

  it("removing a unit takes it out of its action group; removing a leader dissolves it", () => {
    const s = blankScenario();
    addUnit(s, 1, here, undefined);
    addUnit(s, 1, here, undefined);
    addUnit(s, 1, here, undefined);
    s.action_groups = [
      {
        name: "SAG",
        side: 1,
        leader: 1,
        members: [
          { unit: 2, bearing_deg: 90, range_m: 3000 },
          { unit: 3, bearing_deg: 270, range_m: 3000 },
        ],
      },
    ];
    removeUnit(s, 2);
    expect(s.action_groups?.[0]?.members.map((m) => m.unit)).toEqual([3]);
    removeUnit(s, 1);
    expect(s.action_groups).toEqual([]);
  });
});
