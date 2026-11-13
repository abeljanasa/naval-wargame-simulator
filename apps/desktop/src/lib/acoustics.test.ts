// Acoustics in the panels (lib/acoustics.ts): reading a profile from CSV, checking it as the
// engine will, and the wording.

import { describe, expect, it } from "vitest";
import {
  layerDepthM,
  profileText,
  svpFromCsv,
  svpProblem,
  towedText,
  type SoundProfile,
} from "./acoustics";

const winter = [
  { depth_m: 0, speed_mps: 1500 },
  { depth_m: 60, speed_mps: 1501 },
  { depth_m: 1000, speed_mps: 1480 },
  { depth_m: 4000, speed_mps: 1515 },
];

describe("svpFromCsv", () => {
  it("reads pairs under a header, skipping comments, and sorts by depth", () => {
    const { points, notes } = svpFromCsv(
      "depth_m,speed_mps\n# cast 14\n60,1501\n0;1500\n\n1000\t1480\n4000,1515\n",
    );
    expect(points).toEqual(winter);
    expect(notes).toEqual([]);
  });

  it("notes lines it cannot read after the first point", () => {
    const { points, notes } = svpFromCsv("0,1500\n60,fast\n100,1490");
    expect(points).toHaveLength(2);
    expect(notes).toEqual(["line 2 skipped: 60,fast"]);
  });
});

const surface = { depth_m: 0, speed_mps: 1500 };

describe("svpProblem", () => {
  it("accepts a good profile", () => expect(svpProblem(winter)).toBeNull());
  it("wants the surface first", () =>
    expect(svpProblem(winter.slice(1))).toMatch(/starts at the surface/));
  it("wants depths increasing", () => expect(svpProblem([surface, surface])).toMatch(/increase/));
  it("wants plausible speeds", () =>
    expect(svpProblem([surface, { depth_m: 10, speed_mps: 340 }])).toMatch(/1,400/));
  it("wants two points", () => expect(svpProblem([surface])).toMatch(/two points/));
});

describe("layerDepthM", () => {
  it("finds the speed peak near the surface", () => expect(layerDepthM(winter)).toBe(60));
  it("is zero when speed falls from the surface", () =>
    expect(layerDepthM([{ depth_m: 0, speed_mps: 1530 }, ...winter.slice(2)])).toBe(0));
});

describe("wording", () => {
  it("describes a profile", () => {
    const p: SoundProfile = {
      id: 1,
      name: "Winter",
      area: null,
      months: [12, 1, 2],
      points: winter,
    };
    expect(profileText(p)).toBe("layer 60 m · 4 points to 4000 m · Dec Jan Feb · everywhere");
  });
  it("describes the towed array", () => {
    expect(towedText({ streamed: true, ready_in_s: 0 })).toBe("streamed");
    expect(towedText({ streamed: true, ready_in_s: 290 })).toBe("streaming, ready in 5 min");
    expect(towedText({ streamed: false, ready_in_s: 0 })).toBe("recovered");
    expect(towedText({ streamed: false, ready_in_s: 60 })).toBe("recovering, 1 min");
  });
});
