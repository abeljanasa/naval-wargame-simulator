// Terrain drawing (lib/terrain.ts): the maths the map relies on, without a map.

import { describe, expect, it } from "vitest";
import {
  NO_DATA,
  cellText,
  contourLevels,
  contourLines,
  coversText,
  depthColour,
  depthPixels,
  heightAt,
  heightText,
  mercatorLat,
  mercatorLats,
  mercatorY,
  reliefPixels,
  type TerrainGrid,
} from "./terrain";

function grid(rows: number[][], west = 120, east = 121, north = 11, south = 10): TerrainGrid {
  const lats = mercatorLats(north, south, rows.length);
  return {
    west,
    east,
    north,
    south,
    lats,
    cols: rows[0]?.length ?? 0,
    heights: Int16Array.from(rows.flat()),
  };
}

describe("terrain on the map", () => {
  it("spaces rows evenly in Web Mercator, north first", () => {
    const lats = mercatorLats(60, 0, 4);
    expect(lats[0]).toBeGreaterThan(lats[3] ?? 99);
    const ys = lats.map(mercatorY);
    const steps = ys.slice(1).map((y, i) => (ys[i] ?? 0) - y);
    for (const s of steps) expect(s).toBeCloseTo(steps[0] ?? 0, 9);
    expect(mercatorLat(mercatorY(37.5))).toBeCloseTo(37.5, 9);
  });

  it("shades shallow water paler than deep water, and leaves land and gaps clear", () => {
    const [sr, sg, sb] = depthColour(10, "dark");
    const [dr, dg, db] = depthColour(5000, "dark");
    expect(sr + sg + sb).toBeGreaterThan(dr + dg + db);
    expect(depthColour(0, "light")).toEqual([222, 240, 247]);
    expect(depthColour(99_999, "dark")).toEqual([6, 17, 29]);
    const px = depthPixels(grid([[-100, 50, NO_DATA]]), "dark");
    expect(px[3]).toBe(255);
    expect(px[7]).toBe(0);
    expect(px[11]).toBe(0);
  });

  it("lights slopes facing north-west and darkens slopes facing south-east", () => {
    // A ridge running north–south at the middle column: its west face looks west, its east face
    // looks east (away from the north-west light).
    const g = grid([
      [10, 500, 1000, 500, 10],
      [10, 500, 1000, 500, 10],
      [10, 500, 1000, 500, 10],
    ]);
    const px = reliefPixels(g);
    const at = (c: number) => (1 * 5 + c) * 4;
    expect(px[at(1)]).toBe(255); // west face: light
    expect(px[at(3)]).toBe(0); // east face: dark
    expect(px[at(1) + 3]).toBeGreaterThan(0);
    expect(px[at(0) + 3]).toBe(0); // edge cells are left clear
  });

  it("draws a contour between cells either side of its depth", () => {
    const g = grid([
      [-10, -10, -300],
      [-10, -10, -300],
    ]);
    const fc = contourLines(g, [100, 200, 5000]);
    expect(fc.features.map((f) => f.properties.depth_m)).toEqual([100, 200]);
    const seg = fc.features[0]?.geometry.coordinates[0] ?? [];
    expect(seg).toHaveLength(2);
    // −100 m lies 90/290 of the way from the middle column's centre to the east column's.
    const dx = 1 / 3;
    const expected = 120 + 1.5 * dx + (90 / 290) * dx;
    for (const [lon] of seg) expect(lon).toBeCloseTo(expected, 9);
    expect(fc.features[1]?.properties.major).toBe(true);
  });

  it("skips cells next to a gap in the data", () => {
    const fc = contourLines(
      grid([
        [-10, NO_DATA],
        [-10, -300],
      ]),
      [100],
    );
    expect(fc.features).toEqual([]);
  });

  it("reads the height under the cursor", () => {
    const g = grid([
      [-1, -2],
      [-3, NO_DATA],
    ]);
    expect(heightAt(g, 120.1, 10.9)).toBe(-1);
    expect(heightAt(g, 120.9, 10.9)).toBe(-2);
    expect(heightAt(g, 120.1, 10.1)).toBe(-3);
    expect(heightAt(g, 120.9, 10.1)).toBeNull();
    expect(heightAt(g, 122, 10.5)).toBeNull();
    expect(heightAt(g, 120.1 - 360, 10.9)).toBe(-1);
    expect(heightText(-1234)).toBe("Depth 1,234 m");
    expect(heightText(87)).toBe("Height 87 m");
  });

  it("chooses more, shallower contours as the map zooms in", () => {
    expect(contourLevels(3)).toContain(200);
    expect(contourLevels(10)).toContain(10);
    expect(contourLevels(10).length).toBeGreaterThan(contourLevels(3).length);
  });

  it("describes a pack in words", () => {
    expect(cellText(15)).toBe("15″");
    expect(cellText(240)).toBe("4′");
    expect(coversText([-90, -180, 90, 180])).toBe("the world");
    expect(coversText([3, 111, 27, 137])).toBe("3°N–27°N, 111°E–137°E");
  });
});
