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

function cell(c, rank, byName, meta, ctx) {
  const ch = c.item.character;
  const a = c.w * c.h;
  const t = el("div", "hm");
  t.style.left = c.x + "px";
  t.style.top = c.y + "px";
  t.style.width = Math.max(c.w - 2, 0) + "px";
  t.style.height = Math.max(c.h - 2, 0) + "px";
  t.title = `${ch.outfits.map((o) => o.name).join(" · ")}\n${num(ch.total)}점`;
  if (rank < 3) t.classList.add("top" + (rank + 1));
  const showArt = a >= 5000 && c.w >= 60 && c.h >= 60;
  if (showArt) {
    const art = el("div", "hm-art");
    const shown = ch.outfits.filter((o) => o.id).slice(0, c.w >= 200 ? 3 : c.w >= 120 ? 2 : 1);
    for (const o of shown) {
      const p = el("i");
      p.style.backgroundImage = `url(/portraits/${encodeURIComponent(o.id)}.webp)`;
      art.append(p);
    }
    t.append(art);
  }
  const label = el("div", "hm-label");
  if (c.w >= 30 && c.h >= 16) {
    const big = a >= 9000;
    const small = a < 1800;
    label.append(el("b", small ? "tiny" : big ? "big" : "", ch.base));
    if (big && c.h >= 70) label.append(el("span", "", num(ch.total)));
  }
  t.append(label);
  const first = byName.get(ch.outfits[0]?.name);
  if (first) {
    t.tabIndex = 0;
    t.setAttribute("role", "button");
    const open = () => openDetail(first, meta, ctx);
    t.addEventListener("click", open);
    t.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); open(); } });
  }
  return t;
}

export function heatmapView(d, ctx, meta) {
  const root = el("div", "main-view");
  const note = el("p", "hint", "넓을수록 점수가 높은 학생입니다. 의상이 다른 같은 학생은 한 칸에 모았습니다. 칸을 누르면 상세가 열립니다.");
  const frame = el("div", "hm-frame");
  root.append(note, frame);
  const chars = [...d.analysis.characters].filter((c) => c.total > 0).sort((a, b) => b.total - a.total);
  const draw = () => {
    const { width, height } = frame.getBoundingClientRect();
    if (width < 10 || height < 10) return;
    const tiles = squarify(chars.map((character) => ({ value: character.total, character })), 0, 0, width, height);
    frame.replaceChildren(...tiles.map((c, i) => cell(c, i, ctx.byName, meta, ctx)));
  };
  const ro = new ResizeObserver(() => draw());
  ro.observe(frame);
  return root;
}
