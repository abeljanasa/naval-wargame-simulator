// Weather zones in the panels (lib/weather.ts): which zone a unit is in, and the wording.

import { describe, expect, it } from "vitest";
import { inZone, weatherAt, weatherText, type WeatherZone } from "./weather";

const squall: WeatherZone = {
  id: 2,
  name: "Squall",
  area: { shape: "circle", centre: { lat_deg: 12, lon_deg: 120 }, radius_m: 10_000 },
  sea_state: 7,
  wind_kn: 45,
  wind_from_deg: 230,
  visibility_km: 1.5,
  rain_mm_h: 20,
  cloud_base_m: 300,
};
const front: WeatherZone = {
  ...squall,
  id: 1,
  name: "Front",
  area: {
    shape: "polygon",
    points: [
      { lat_deg: 11, lon_deg: 119 },
      { lat_deg: 11, lon_deg: 121 },
      { lat_deg: 13, lon_deg: 121 },
      { lat_deg: 13, lon_deg: 119 },
    ],
  },
  sea_state: 5,
  rain_mm_h: 0,
  cloud_base_m: null,
};
const env = { sea_state: 2, weather_loss_db: 0, start_unix_s: 0, visibility_km: 30 };

describe("weather zones", () => {
  it("finds the zone a point is in, the higher id winning", () => {
    expect(inZone(squall, { lat_deg: 12.05, lon_deg: 120 })).toBe(true);
    expect(inZone(squall, { lat_deg: 12.2, lon_deg: 120 })).toBe(false);
    expect(inZone(front, { lat_deg: 12.5, lon_deg: 120.5 })).toBe(true);
    expect(weatherAt(env, [front, squall], { lat_deg: 12, lon_deg: 120 }).zone?.name).toBe(
      "Squall",
    );
    expect(weatherAt(env, [front, squall], { lat_deg: 12.5, lon_deg: 120.5 }).sea_state).toBe(5);
    const out = weatherAt(env, [front, squall], { lat_deg: 20, lon_deg: 120 });
    expect([out.zone, out.sea_state, out.visibility_km]).toEqual([null, 2, 30]);
  });

  it("words a zone's weather", () => {
    expect(weatherText(squall)).toBe(
      "sea state 7 · wind 45 kn from 230° · visibility 1.5 km · rain 20 mm/h · cloud base 300 m",
    );
    expect(weatherText({ ...front, wind_kn: 0 })).toBe("sea state 5 · visibility 1.5 km");
  });
});
