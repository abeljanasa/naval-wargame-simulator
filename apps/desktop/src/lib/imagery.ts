// The imagery base layer (Phase 4 slice 6, docs/models/imagery.md): the client's licensed tile
// pack, served by the app itself (the `nwstiles` scheme), never fetched from a network.

import { invoke } from "@tauri-apps/api/core";
import type * as maplibregl from "maplibre-gl";
import { commands, type ImageryInfo_Serialize as ImageryInfo } from "./bindings";
import type { ImageryStatus_Serialize as ImageryStatus } from "./bindings";

export type { ImageryInfo, ImageryStatus };

/** Where the app serves its tiles: a custom scheme, spelled per platform by the webview. */
export function tileBase(userAgent: string): string {
  return /Windows/i.test(userAgent) ? "http://nwstiles.localhost" : "nwstiles://localhost";
}

/** The tile URL template for a pack. */
export function tileTemplate(base: string, file: string): string {
  return `${base}/${encodeURIComponent(file)}/{z}/{x}/{y}`;
}

/** What the map credits for a pack: its own attribution, or a note that it gave none. */
export function imageryCredit(p: ImageryInfo): string {
  return p.attribution?.trim() || `${p.name} (the pack gives no attribution: ask its supplier)`;
}

export async function imageryStatus(): Promise<ImageryStatus> {
  return commands.imageryStatus();
}

export async function rescanImagery(): Promise<ImageryStatus> {
  return commands.rescanImagery();
}

export async function installImagery(file: File): Promise<ImageryStatus> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  return invoke<ImageryStatus>("install_imagery", bytes, {
    headers: { "x-file-name": file.name },
  });
}

const SOURCE = "imagery";

/** Shows one pack as the map's base layer (or none). The coastline stays on top; the land fill
 * is hidden and the depth shading dimmed while imagery is shown, so the imagery shows through. */
export function setImagery(map: maplibregl.Map, pack: ImageryInfo | null) {
  const apply = () => {
    if (map.getLayer(SOURCE)) map.removeLayer(SOURCE);
    if (map.getSource(SOURCE)) map.removeSource(SOURCE);
    const on = pack !== null;
    if (map.getLayer("land")) map.setPaintProperty("land", "fill-opacity", on ? 0 : 1);
    if (map.getLayer("terrain-depth"))
      map.setPaintProperty("terrain-depth", "raster-opacity", on ? 0.45 : 1);
    if (!pack) return;
    const [w, s, e, n] = pack.bounds.map((v) => v ?? 0) as [number, number, number, number];
    map.addSource(SOURCE, {
      type: "raster",
      tiles: [tileTemplate(tileBase(navigator.userAgent), pack.file)],
      tileSize: pack.tile_size,
      minzoom: pack.min_zoom,
      maxzoom: pack.max_zoom,
      bounds: [w, s, e, n],
      attribution: imageryCredit(pack),
    });
    const before = ["terrain-depth", "grid"].find((id) => map.getLayer(id));
    map.addLayer(
      { id: SOURCE, type: "raster", source: SOURCE, paint: { "raster-fade-duration": 0 } },
      before,
    );
  };
  if (map.isStyleLoaded()) apply();
  else map.once("idle", apply);
}
