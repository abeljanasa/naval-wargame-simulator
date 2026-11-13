// Air operations in the panels (lib/air.ts): the wording of an aircraft's state, and the
// missions the Air tab builds.

import { describe, expect, it } from "vitest";
import { airborneText, bingoText, needsPoint, parkedText, sortieMission } from "./air";
import type { AirUnitView, ParkedView } from "./bindings";

const parked = (over: Partial<ParkedView>): ParkedView => ({
  unit: 1,
  name: "Viper 1",
  platform: "GEN-FTR-MR",
  readiness: "ready",
  launching: false,
  ready_in_s: 0,
  loadout: "GEN-FTR-MR/CAP",
  loadouts: [],
  fuel_fraction: 1,
  damaged: false,
  ...over,
});

describe("parkedText", () => {
  it("says what a parked aircraft is doing", () => {
    expect(parkedText(parked({}))).toBe("ready");
    expect(parkedText(parked({ readiness: "alert15" }))).toBe("alert 15");
    expect(parkedText(parked({ ready_in_s: 610 }))).toBe("turning round, 11 min");
    expect(parkedText(parked({ launching: true, ready_in_s: 900 }))).toBe("launching in 15 min");
    expect(parkedText(parked({ launching: true }))).toBe("waiting for the runway");
    expect(parkedText(parked({ readiness: "maintenance", ready_in_s: 3600 }))).toBe(
      "maintenance, 60 min",
    );
    expect(parkedText(parked({ damaged: true }))).toBe("damaged: cannot fly");
  });
});

describe("airborne", () => {
  const a: AirUnitView = {
    base: 1,
    state: "airborne",
    loadout: null,
    bingo_fraction: 0.234,
    tanker: false,
  };
  it("says what an airborne aircraft is doing", () => {
    expect(airborneText(a)).toBe("on a sortie");
    expect(airborneText({ ...a, state: "returning" })).toBe("returning to base");
    expect(airborneText({ ...a, state: "tanking" })).toBe("refuelling from a tanker");
    expect(airborneText({ ...a, tanker: true })).toBe("on station (tanker)");
  });
  it("gives bingo as a share of the tank", () => {
    expect(bingoText(a)).toBe("bingo 23 %");
    expect(bingoText({ ...a, bingo_fraction: null })).toBeNull();
  });
});

describe("sortieMission", () => {
  const at = { lat_deg: 12, lon_deg: 120 };
  it("builds each mission", () => {
    expect(sortieMission("patrol", at, null, 40_000)).toEqual({
      kind: "patrol",
      centre: at,
      radius_m: 40_000,
    });
    expect(sortieMission("support", at, null, 20_000)).toEqual({
      kind: "support",
      station: at,
      radius_m: 20_000,
    });
    expect(sortieMission("ferry", at, null, 0)).toEqual({ kind: "ferry", destination: at });
    expect(sortieMission("strike", null, 4, 0)).toEqual({ kind: "strike", contact: 4 });
  });
  it("needs a point or a contact", () => {
    expect(sortieMission("patrol", null, null, 1)).toBeNull();
    expect(sortieMission("strike", at, null, 1)).toBeNull();
    expect(needsPoint("strike")).toBe(false);
    expect(needsPoint("ferry")).toBe(true);
  });
});
