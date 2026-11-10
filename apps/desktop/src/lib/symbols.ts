// Our own unit symbology, following the public APP-6 / MIL-STD-2525 conventions: frame shape
// encodes affiliation, so it reads without colour (colour-blind-safe), and colour reinforces it.
// Drawn here from scratch; not copied from any product.

export type Affiliation = "friend" | "hostile" | "neutral" | "unknown";

export const AFFILIATION_COLOUR: Record<Affiliation, [number, number, number]> = {
  friend: [128, 224, 255],
  hostile: [255, 128, 128],
  neutral: [170, 255, 170],
  unknown: [255, 255, 128],
};

const FRAMES: Record<Affiliation, string> = {
  // Sea-surface frames: friend = circle, hostile = diamond, neutral = square, unknown = quatrefoil.
  friend: `<circle cx="32" cy="32" r="22"/>`,
  hostile: `<path d="M32 6 L58 32 L32 58 L6 32 Z"/>`,
  neutral: `<rect x="11" y="11" width="42" height="42"/>`,
  unknown: `<path d="M32 10 a11 11 0 0 1 11 11 a11 11 0 0 1 0 22 a11 11 0 0 1 -22 0 a11 11 0 0 1 0 -22 a11 11 0 0 1 11 -11 Z"/>`,
};

function rgb([r, g, b]: [number, number, number]) {
  return `rgb(${r},${g},${b})`;
}

const urlCache = new Map<string, string>();

/** An SVG data URL for a frame, selected or not. Cached: the map asks for it once per unit per frame. */
export function symbolUrl(aff: Affiliation, selected: boolean): string {
  const key = `${aff}-${selected}`;
  let url = urlCache.get(key);
  if (url === undefined) {
    url = buildSymbolUrl(aff, selected);
    urlCache.set(key, url);
  }
  return url;
}

function buildSymbolUrl(aff: Affiliation, selected: boolean): string {
  const fill = rgb(AFFILIATION_COLOUR[aff]);
  const stroke = selected ? "#ffffff" : "#0b1b2b";
  const sw = selected ? 5 : 3;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><g fill="${fill}" fill-opacity="0.9" stroke="${stroke}" stroke-width="${sw}">${FRAMES[aff]}</g></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}
