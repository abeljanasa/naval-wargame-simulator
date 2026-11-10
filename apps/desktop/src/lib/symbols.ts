// Our own unit symbology, following the public APP-6 / MIL-STD-2525 conventions: frame shape
// encodes affiliation (so it reads without colour; colour-blind-safe) and domain (sea surface =
// full frame, air = upper half, subsurface = lower half, land = full frame with a rectangular
// friend). Colour reinforces affiliation. Drawn here from scratch; not copied from any product.

import type { Domain } from "./bindings";

export type Affiliation = "friend" | "hostile" | "neutral" | "unknown";

export const AFFILIATION_COLOUR: Record<Affiliation, [number, number, number]> = {
  friend: [128, 224, 255],
  hostile: [255, 128, 128],
  neutral: [170, 255, 170],
  unknown: [255, 255, 128],
};

// Full frames on a 64×64 canvas (sea surface; land except the friend rectangle).
const FULL: Record<Affiliation, string> = {
  friend: `<circle cx="32" cy="32" r="22"/>`,
  hostile: `<path d="M32 6 L58 32 L32 58 L6 32 Z"/>`,
  neutral: `<rect x="11" y="11" width="42" height="42"/>`,
  unknown: `<path d="M32 10 a11 11 0 0 1 11 11 a11 11 0 0 1 0 22 a11 11 0 0 1 -22 0 a11 11 0 0 1 0 -22 a11 11 0 0 1 11 -11 Z"/>`,
};

// Upper-half frames (air) closed by a flat base at y = 44; subsurface uses the mirror image.
const UPPER: Record<Affiliation, string> = {
  friend: `M8 44 A24 24 0 0 1 56 44 Z`,
  hostile: `M8 44 L32 14 L56 44 Z`,
  neutral: `M10 44 V14 H54 V44 Z`,
  unknown: `M10 44 A11 11 0 0 1 21 26 A11 11 0 0 1 43 26 A11 11 0 0 1 54 44 Z`,
};

/** The frame's SVG elements for an affiliation and domain. */
export function frameSvg(aff: Affiliation, domain: Domain): string {
  switch (domain) {
    case "air":
      return `<path d="${UPPER[aff]}"/>`;
    case "subsurface":
      // Mirror the upper half about the horizontal centre line (y → 64 − y).
      return `<path transform="matrix(1 0 0 -1 0 64)" d="${UPPER[aff]}"/>`;
    case "land":
      return aff === "friend" ? `<rect x="5" y="16" width="54" height="32"/>` : FULL[aff];
    default:
      return FULL[aff];
  }
}

function rgb([r, g, b]: [number, number, number]) {
  return `rgb(${r},${g},${b})`;
}

const urlCache = new Map<string, string>();

/** An SVG data URL for a frame. Cached: the map asks for it once per unit per frame. */
export function symbolUrl(aff: Affiliation, selected: boolean, domain: Domain = "surface"): string {
  const key = `${aff}-${domain}-${selected}`;
  let url = urlCache.get(key);
  if (url === undefined) {
    url = buildSymbolUrl(aff, selected, domain);
    urlCache.set(key, url);
  }
  return url;
}

function buildSymbolUrl(aff: Affiliation, selected: boolean, domain: Domain): string {
  const fill = rgb(AFFILIATION_COLOUR[aff]);
  const stroke = selected ? "#ffffff" : "#0b1b2b";
  const sw = selected ? 5 : 3;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><g fill="${fill}" fill-opacity="0.9" stroke="${stroke}" stroke-width="${sw}">${frameSvg(aff, domain)}</g></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}
