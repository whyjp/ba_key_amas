// Heatmap of the teacher's characters: the area of a tile is the character's score, so a standout student
// takes more room. Outfits of one character share a tile. Big tiles show portraits and the name, small ones
// only the name, the smallest nothing but a tooltip. Layout is a squarified treemap, drawn with absolutely
// positioned tiles and redone when the frame is resized.
import { openDetail } from "./detail.js";
import { el, num } from "./ui.js";

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
const PORTRAIT_W = 200;
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

function cell(c, rank, byName, meta, ctx) {
  const ch = c.item.character;
  const a = c.w * c.h;
  const t = el("div", "hm");
  t.style.left = c.x + "px";
  t.style.top = c.y + "px";
  t.style.width = Math.max(c.w - 2, 0) + "px";
  t.style.height = Math.max(c.h - 2, 0) + "px";
  t.title = `${ch.outfits.map((o) => o.name).join(" · ")}\n${num(ch.total)}점${ch.owners ? ` · ${ch.owners}명 보유` : ""}`;
  if (rank < 3) t.classList.add("top" + (rank + 1));
  // a portrait wherever there is room for a face; only a sliver gets none
  const showArt = Math.min(c.w, c.h) >= 16 && a >= 400;
  const owned = (o) => byName.get(o.name);
  const shown = showArt ? ch.outfits.filter((o) => o.id).slice(0, Math.max(1, Math.min(3, Math.floor((c.w - 2) / PIECE_MIN)))) : [];
  // a character whose outfits share the cell is split into left / middle / right pieces; each piece is its own button
  const split = shown.length > 1;
  if (showArt) {
    const art = el("div", "hm-art");
    const slotW = (c.w - 2) / Math.max(shown.length, 1);
    for (const o of shown) {
      const seg = el("i");
      const p = el("b");
      const { size, pos } = placeFace(slotW, c.h - 2, meta.faces?.[String(o.id)]);
      p.style.backgroundImage = `url(/portraits/${encodeURIComponent(o.id)}.webp)`;
      p.style.backgroundSize = size;
      p.style.backgroundPosition = pos;
      seg.append(p);
      if (split && owned(o)) {
        seg.classList.add("pick");
        seg.tabIndex = 0;
        seg.setAttribute("role", "button");
        seg.setAttribute("aria-label", o.name);
        seg.title = `${o.name}
카드 열기`;
        const open = (e) => { e.stopPropagation(); openDetail(owned(o), meta, ctx); };
        seg.addEventListener("click", open);
        seg.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); open(e); } });
      }
      art.append(seg);
    }
    t.append(art);
  }
  const label = el("div", "hm-label");
  if (c.w >= 34 && c.h >= 18) {
    const big = a >= 9000;
    const small = a < 1800;
    label.append(el("b", small ? "tiny" : big ? "big" : "", ch.base));
    if (big && c.h >= 70) label.append(el("span", "", num(ch.total)));
  }
  t.append(label);
  if (split) t.classList.add("split");
  const first = byName.get(ch.outfits[0]?.name);
  if (first && !split) {
    t.tabIndex = 0;
    t.setAttribute("role", "button");
    const open = () => openDetail(first, meta, ctx);
    t.addEventListener("click", open);
    t.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); open(); } });
  } else if (first) {
    // a click on the name strip or a gap opens the first outfit
    t.addEventListener("click", () => openDetail(first, meta, ctx));
  }
  return t;
}

/** The narrowest a piece of a split cell may be, so a face still fits in it. */
const PIECE_MIN = 50;
/** Layout bonus per extra outfit: a character with outfits takes more room than its score alone gives, so its pieces stay
 *  big enough to pick. It changes only the size of the cell, never the score shown. */
const OUTFIT_ROOM = 0.45;
const outfitRoom = (ch) => 1 + OUTFIT_ROOM * (Math.min(ch.outfits.filter((o) => o.id).length, 3) - 1);

/** Area (px squared) the map aims to give each character. Below about 60 x 60 a face is hard to read, so a narrow window
 *  gets a taller map (and scrolls) instead of a crowd of slivers. */
const CELL_AREA = 3800;
const MAX_HEIGHT = 1600;

/** The height a map of `n` tiles needs at this width. */
export function wantedHeight(n, width) {
  return Math.min(MAX_HEIGHT, Math.ceil((n * CELL_AREA) / Math.max(width, 1)));
}

/**
 * Draws the treemap into `frame` and redraws when the frame or the window changes size. `fit` frames fill the room
 * their page gives them (the profile), and only get a minimum height; the others (the leaderboard page) are sized
 * here: the window's height, or what the tiles need, whichever is larger.
 */
function mountTreemap(frame, chars, byName, meta, ctx, fit) {
  const draw = () => {
    if (!frame.isConnected) return;
    const width = frame.getBoundingClientRect().width;
    if (width < 10) return;
    const need = wantedHeight(chars.length, width);
    if (fit) frame.style.minHeight = need + "px";
    else frame.style.height = Math.max(380, innerHeight - 330, need) + "px";
    const { height } = frame.getBoundingClientRect();
    if (height < 10) return;
    const tiles = squarify(chars.map((character) => ({ value: character.total * outfitRoom(character), character })), 0, 0, width, height);
    frame.replaceChildren(...tiles.map((c, i) => cell(c, i, byName, meta, ctx)));
  };
  new ResizeObserver(draw).observe(frame);
  addEventListener("resize", draw);
}

/** One teacher's characters. `top` (summary and favourites) sits above the map. */
export function heatmapView(d, ctx, meta, top) {
  const root = el("div", "main-view");
  const note = el("p", "hint", "넓을수록 점수가 높은 학생입니다. 의상이 다른 같은 학생은 한 칸에 모았습니다. 한 칸에 의상이 여럿이면 칸이 조각으로 나뉘고 의상이 많을수록 칸이 조금 더 넓어지며, 마음에 드는 조각을 눌러 그 의상의 카드를 엽니다.");
  const frame = el("div", "hm-frame");
  if (top) root.append(top);
  root.append(note, frame);
  root.classList.add("scrollable");
  mountTreemap(frame, [...d.analysis.characters].filter((c) => c.total > 0).sort((a, b) => b.total - a.total), ctx.byName, meta, ctx, true);
  return root;
}

/** Every public teacher's characters added up. A tile is a character, its area the sum of what it is worth to the
 *  teachers who own it. Nothing here belongs to one student, so there is nothing to open. */
export function communityHeatmap(characters, meta, teachers) {
  const root = el("div", "community");
  root.append(el("p", "hint", `공개한 선생님 ${teachers}명이 가진 학생을 캐릭터별로 더한 지도입니다. 넓을수록 많은 선생님이 아끼는 학생입니다. 칸에 마우스를 올리면 보유 인원이 보입니다.`));
  const frame = el("div", "hm-frame community-frame");
  root.append(frame);
  mountTreemap(frame, characters.filter((c) => c.total > 0), new Map(), meta, { duty: new Set(), unread: new Map(), byName: new Map() }, false);
  return root;
}
