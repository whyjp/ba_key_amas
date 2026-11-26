// The pure part of the heatmap: layout, face placement and sizing. No DOM, so node can test it (test/heatmap.test.ts).

/** Squarified treemap (Bruls, Huizing, van Wijk): items sorted by value descending, laid in rows along the shorter side. */
export function squarify(items, x, y, w, h) {
  const total = items.reduce((n, i) => n + i.value, 0);
  const scale = total > 0 ? (w * h) / total : 0;
  const rest = items.map((i) => ({ item: i, area: i.value * scale }));
  const out = [];
  const worst = (row, side) => {
    const sum = row.reduce((n, r) => n + r.area, 0);
    const max = Math.max(...row.map((r) => r.area));
    const min = Math.min(...row.map((r) => r.area));
    return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
  };
  while (rest.length) {
    const side = Math.min(w, h);
    const row = [rest.shift()];
    while (rest.length && worst([...row, rest[0]], side) <= worst(row, side)) row.push(rest.shift());
    const sum = row.reduce((n, r) => n + r.area, 0);
    if (w >= h) {
      const rw = sum / h;
      let cy = y;
      for (const r of row) { const rh = r.area / rw; out.push({ item: r.item, x, y: cy, w: rw, h: rh }); cy += rh; }
      x += rw;
      w -= rw;
    } else {
      const rh = sum / w;
      let cx = x;
      for (const r of row) { const rw = r.area / rh; out.push({ item: r.item, x: cx, y, w: rw, h: rh }); cx += rw; }
      y += rh;
      h -= rh;
    }
  }
  return out;
}

// The portraits are 200 x 226 pixels. The face is at (fx, fy) as fractions of them, from meta.faces.
export const PORTRAIT_W = 200;
const PORTRAIT_H = 226;
const FACE_DEFAULT = [0.51, 0.48];
/** Where the face sits in a cell: horizontally in the middle, vertically in the upper third (a face in the exact
 *  middle looks low once the body is cropped off). */
const FACE_AT = [0.5, 0.36];

/**
 * Size and offset of the portrait inside a w x h cell so the face lands on FACE_AT as far as the picture allows.
 * The picture is scaled to cover the cell and then enlarged a little: a tall cell crops the sides, so only
 * enlarging gives it room to move the face up; a small cell gains by showing the face larger.
 */
export function placeFace(w, h, face) {
  const [fx, fy] = face ?? FACE_DEFAULT;
  const cover = Math.max(w / PORTRAIT_W, h / PORTRAIT_H);
  const tall = h / w > 1.1;
  const zoom = tall ? 1.24 : Math.min(w, h) < 70 ? 1.2 : 1.08;
  const s = cover * zoom;
  const iw = PORTRAIT_W * s;
  const ih = PORTRAIT_H * s;
  const left = Math.min(0, Math.max(w - iw, w * FACE_AT[0] - fx * iw));
  const top = Math.min(0, Math.max(h - ih, h * FACE_AT[1] - fy * ih));
  return { size: `${iw.toFixed(1)}px ${ih.toFixed(1)}px`, pos: `${left.toFixed(1)}px ${top.toFixed(1)}px` };
}


/** The narrowest a piece of a split cell may be, so a face still fits in it. */
export const PIECE_MIN = 50;
/** Layout bonus per extra outfit: a character with outfits takes more room than its score alone gives, so its pieces stay
 *  big enough to pick. It changes only the size of the cell, never the score shown. */
const OUTFIT_ROOM = 0.45;
export const outfitRoom = (ch) => 1 + OUTFIT_ROOM * (Math.min(ch.outfits.filter((o) => o.id).length, 3) - 1);

/** Area (px squared) the map aims to give each character. Below about 60 x 60 a face is hard to read, so a narrow window
 *  gets a taller map (and scrolls) instead of a crowd of slivers. */
export const CELL_AREA = 3800;
export const MAX_HEIGHT = 1600;

/** The height a map of `n` tiles needs at this width. */
export function wantedHeight(n, width) {
  return Math.min(MAX_HEIGHT, Math.ceil((n * CELL_AREA) / Math.max(width, 1)));
}


/** A cell below this area (px squared, about 45 px a side) is too small to pick or to name, so the tail is folded into one cell. One value for every device. */
export const FOLD_AREA = 2000;

/**
 * Items sorted by value descending, laid out in `area`: those whose cell would be smaller than `minArea` are the tail.
 * A tail of one is kept as it is (folding one cell hides nothing and gains nothing).
 */
export function foldTail(items, area, minArea = FOLD_AREA) {
  const total = items.reduce((n, i) => n + i.value, 0);
  if (total <= 0) return { kept: items, tail: [] };
  const scale = area / total;
  const at = items.findIndex((i) => i.value * scale < minArea);
  if (at < 0 || items.length - at < 2) return { kept: items, tail: [] };
  return { kept: items.slice(0, at), tail: items.slice(at) };
}

/** 0 (lowest) to 4 (highest): the quantile of a rank, so the colour tells how high a cell stands even where it is small. */
export function heatLevel(rank, n) {
  const p = n > 0 ? rank / n : 1;
  return p < 0.05 ? 4 : p < 0.15 ? 3 : p < 0.35 ? 2 : p < 0.65 ? 1 : 0;
}

/** Whether a redraw is worth it: the width changed, or the height by more than a phone's address bar (about 100 px). */
export function resizeMatters(prev, w, h) {
  return !prev || Math.abs(prev.w - w) >= 1 || Math.abs(prev.h - h) >= 150;
}
