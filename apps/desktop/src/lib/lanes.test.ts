// Shipping lanes (lib/lanes.ts): GeoJSON import and the width of navigable water.

import { describe, expect, it } from "vitest";
import { lanesFromGeoJson, navigableWidthM } from "./lanes";

describe("shipping lanes", () => {
  it("imports LineString features, keeping traffic only with a source", () => {
    const geo = JSON.stringify({
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          properties: { name: "Inbound", width_m: 3000, ships_per_day: 80, source: "AIS 2025" },
          geometry: {
            type: "LineString",
            coordinates: [
              [120.1, 12.0],
              [120.6, 12.3],
            ],
          },
        },
        {
          type: "Feature",
          properties: { name: "Outbound", ships_per_day: "40" },
          geometry: {
            type: "MultiLineString",
            coordinates: [
              [
                [120.1, 11.9],
                [120.6, 12.2],
              ],
              [[121, 12]],
            ],
          },
        },
        { type: "Feature", properties: {}, geometry: { type: "Point", coordinates: [0, 0] } },
      ],
    });
    const { lanes, notes } = lanesFromGeoJson(geo, 5, "lanes.geojson");
    expect(lanes.map((l) => [l.id, l.name, l.width_m, l.ships_per_day, l.source])).toEqual([
      [5, "Inbound", 3000, 80, "AIS 2025"],
      [6, "Outbound", 2000, null, "lanes.geojson"],
    ]);
    expect(lanes[0]?.points[0]).toEqual({ lat_deg: 12.0, lon_deg: 120.1 });
    expect(notes).toEqual(["Outbound: traffic dropped, the file names no source for it"]);
    expect(lanesFromGeoJson("nope", 1, "x.json").notes).toEqual(["x.json is not JSON"]);
  });

  it("measures the widest run of water deep enough", () => {
    const section = [0, 10, 25, 30, 8, 30, 30, 30, 2].map((d, i) => ({
      distance_m: i * 100,
      depth_m: d,
    }));
    // 10–30 m from 100 to 300 m along (200 m wide); 30 m from 500 to 700 m (200 m).
    expect(navigableWidthM(section, 10)).toBe(200);
    expect(navigableWidthM(section, 26)).toBe(200);
    expect(navigableWidthM(section, 50)).toBe(0);
    expect(navigableWidthM([{ distance_m: 0, depth_m: null }], 1)).toBe(0);
  });
});
