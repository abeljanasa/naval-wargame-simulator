// The imagery base layer (lib/imagery.ts): tile URLs and the credit shown with the map.

import { describe, expect, it } from "vitest";
import { imageryCredit, tileBase, tileTemplate } from "./imagery";

describe("imagery", () => {
  it("addresses the app's own tile scheme, as each platform's webview spells it", () => {
    expect(tileBase("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Edg/158")).toBe(
      "http://nwstiles.localhost",
    );
    expect(tileBase("Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0) AppleWebKit")).toBe(
      "nwstiles://localhost",
    );
    expect(tileTemplate("nwstiles://localhost", "Strait Mosaic.pmtiles")).toBe(
      "nwstiles://localhost/Strait%20Mosaic.pmtiles/{z}/{x}/{y}",
    );
  });

  it("credits the pack, or says it gave no credit", () => {
    const p = {
      file: "a.mbtiles",
      name: "Coast",
      format: "jpg",
      min_zoom: 1,
      max_zoom: 12,
      bounds: [119, 11, 122, 14] as [number, number, number, number],
      attribution: "© Example Imagery Ltd",
      tile_size: 256,
    };
    expect(imageryCredit(p)).toBe("© Example Imagery Ltd");
    expect(imageryCredit({ ...p, attribution: null })).toContain("gives no attribution");
  });
});
