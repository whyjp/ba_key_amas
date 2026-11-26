// Heatmap of the teacher's characters: the area of a tile is the character's score, so a standout student
// takes more room. Outfits of one character share a tile. Big tiles show portraits and the name, small ones
// only the name; the tail too small to pick is folded into one "others" tile. The same data can be read as a
// list, which is the easier view on a phone and with a screen reader.
//
// A tile of an owned student opens its card at once, on a phone too (the card plays its halo and entrance, and has its own
// outfit bar for switching). The sheet with the numbers is only for a tile that has no card to open (the community map,
// the folded "others" tile). Keyboard: one tab stop for the whole map, arrows move between tiles.
import { openDetail } from "./detail.js";
import { FOLD_AREA, FOLD_AREA_TOUCH, foldTail, heatLevel, outfitRoom, PIECE_MIN, placeFace, resizeMatters, squarify, wantedHeight } from "./heatmap-math.js";
import { el, num, portrait } from "./ui.js";

const coarse = () => matchMedia("(hover: none), (pointer: coarse)").matches;
const reducedMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

const VIEW_KEY = "hm-view";
const readView = () => { try { return localStorage.getItem(VIEW_KEY) === "list" ? "list" : "map"; } catch { return "map"; } };
const saveView = (v) => { try { localStorage.setItem(VIEW_KEY, v); } catch { /* private window: the choice just is not kept */ } };

const FILTERS = [
  ["all", "전체", () => true],
  ["pinned", "고정", (c) => c.pinned],
  ["memorial", "기억 로비", (c) => c.memorial],
  ["bond30", "인연 30+", (c) => c.bond >= 30],
];

const describe = (ch, rank) => `${rank + 1}위 ${ch.base}, ${num(ch.total)}점${ch.owners ? `, ${ch.owners}명 보유` : ""}`;

/** The tap sheet: one per board, at the bottom of the window. Escape or its button closes it and focus goes back. */
function makeSheet() {
  let node = null;
  let back = null;
  const close = () => {
    node?.remove();
    node = null;
    back?.focus?.({ preventScroll: true });
    back = null;
  };
  addEventListener("hashchange", () => { back = null; close(); });
  return {
    close,
    /** `entries`: [{ label, note?, open? }]; a row with `open` is a button. */
    show(title, facts, entries, from) {
      node?.remove();
      back = from ?? null;
      node = el("div", "hm-sheet");
      node.setAttribute("role", "dialog");
      node.setAttribute("aria-label", title);
      const head = el("div", "hm-sheet-head");
      const h = el("h3", "", title);
      h.tabIndex = -1;
      const x = el("button", "ghost hm-sheet-close", "닫기");
      x.type = "button";
      x.addEventListener("click", close);
      head.append(h, x);
      node.append(head);
      if (facts) node.append(el("p", "hm-sheet-facts", facts));
      const list = el("ul", "hm-sheet-list");
      for (const e of entries) {
        const li = el("li");
        if (e.open) {
          const b = el("button", "hm-sheet-row", e.label);
          b.type = "button";
          b.addEventListener("click", () => { close(); e.open(); });
          li.append(b);
        } else li.append(el("span", "hm-sheet-row plain", e.label));
        if (e.note) li.append(el("small", "", e.note));
        list.append(li);
      }
      node.append(list);
      node.addEventListener("keydown", (ev) => { if (ev.key === "Escape") { ev.stopPropagation(); close(); } });
      document.body.append(node);
      h.focus({ preventScroll: true });
    },
  };
}

/** What a tap or Enter on a tile does. `opens(o)` returns a function that opens the card of an outfit, when the teacher owns it. */
function activation(ch, rank, sheet, opens, meta, from) {
  const facts = `${num(ch.total)}점${ch.owners ? ` · ${ch.owners}명 보유` : ""} · ${rank + 1}위`;
  const entries = ch.outfits.map((o) => ({ label: o.name, note: opens(o) ? "카드 열기" : undefined, open: opens(o) }));
  return { facts, entries, show: () => sheet.show(ch.base, facts, entries, from) };
}

function foldedCharacters(list, sheet, opens, from, firstRank) {
  const entries = list.map((ch, i) => {
    const first = ch.outfits.find((o) => opens(o));
    return { label: `${firstRank + i + 1}위 ${ch.base}`, note: `${num(ch.total)}점${ch.owners ? ` · ${ch.owners}명` : ""}`, open: first ? opens(first) : undefined };
  });
  sheet.show(`그 외 ${list.length}명`, "칸이 너무 작아 한 칸으로 모았습니다.", entries, from);
}

function cell(c, rank, n, ctx, st) {
  const { meta, sheet, opens, dim } = ctx;
  const t = el("div", "hm");
  t.style.left = c.x + "px";
  t.style.top = c.y + "px";
  t.style.width = Math.max(c.w - 2, 0) + "px";
  t.style.height = Math.max(c.h - 2, 0) + "px";
  t.style.setProperty("--i", String(Math.min(rank, 40)));
  t.dataset.rank = String(rank);
  t.setAttribute("role", "button");
  t.tabIndex = -1;
  const a = c.w * c.h;

  if (c.item.folded) {
    const list = c.item.folded;
    t.classList.add("hm-fold");
    t.dataset.heat = "0";
    t.setAttribute("aria-label", `그 외 ${list.length}명 묶음`);
    t.title = `그 외 ${list.length}명`;
    t.append(el("div", "hm-label"));
    t.firstChild.append(el("b", "", `+${list.length}`), el("span", "", "그 외"));
    const go = () => foldedCharacters(list, sheet, opens, t, c.item.firstRank);
    t.addEventListener("click", go);
    t.activate = go;
    return t;
  }

  const ch = c.item.character;
  t.dataset.heat = String(heatLevel(rank, n));
  t.setAttribute("aria-label", describe(ch, rank));
  t.title = `${ch.outfits.map((o) => o.name).join(" · ")}\n${num(ch.total)}점${ch.owners ? ` · ${ch.owners}명 보유` : ""}`;
  if (rank < 3) t.classList.add("top" + (rank + 1));
  if (dim && !dim(ch)) t.classList.add("dim");

  // a portrait wherever there is room for a face; only a sliver gets none
  const showArt = Math.min(c.w, c.h) >= 16 && a >= 400;
  const shown = showArt ? ch.outfits.filter((o) => o.id).slice(0, Math.max(1, Math.min(3, Math.floor((c.w - 2) / PIECE_MIN)))) : [];
  const split = shown.length > 1;
  const act = activation(ch, rank, sheet, opens, meta, t);
  const first = ch.outfits.map(opens).find(Boolean);
  // the card directly whenever the teacher owns an outfit of the character, else the sheet of numbers
  const direct = Boolean(first);
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
      const open = opens(o);
      if (split && open) {
        seg.classList.add("pick");
        seg.title = `${o.name}\n카드 열기`;
        seg.addEventListener("click", (e) => { e.stopPropagation(); open(); });
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
  if (rank < 10 && Math.min(c.w, c.h) >= 38) t.append(el("em", "hm-rank", String(rank + 1)));
  if (split) t.classList.add("split");

  const go = direct ? first : act.show;
  t.addEventListener("click", go);
  t.activate = go;
  return t;
}

/** Arrow keys move between tiles (one tab stop for the whole map instead of one per tile). */
function roving(frame) {
  const tiles = () => [...frame.querySelectorAll(".hm")];
  frame.addEventListener("focusin", (e) => {
    const t = e.target.closest?.(".hm");
    if (!t) return;
    for (const o of tiles()) o.tabIndex = o === t ? 0 : -1;
  });
  frame.addEventListener("keydown", (e) => {
    const t = e.target.closest?.(".hm");
    if (!t || e.target !== t) return;
    const all = tiles();
    const i = all.indexOf(t);
    const go = { ArrowRight: i + 1, ArrowDown: i + 1, ArrowLeft: i - 1, ArrowUp: i - 1, Home: 0, End: all.length - 1 }[e.key];
    if (go !== undefined) {
      e.preventDefault();
      all[Math.max(0, Math.min(all.length - 1, go))]?.focus();
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      t.activate?.();
    }
  });
}

/** One row of the list view. */
function row(ch, rank, n, max, opens, sheet) {
  const first = ch.outfits.map(opens).find(Boolean);
  const body = [
    el("span", "hm-row-rank", String(rank + 1)),
    portrait(ch.outfits.find((o) => o.id)?.id, "pt sm"),
  ];
  const name = el("span", "hm-row-name");
  name.append(el("b", "", ch.base), el("small", "", ch.outfits.length > 1 ? `의상 ${ch.outfits.length}` : ""));
  const score = el("span", "hm-row-score");
  const bar = el("i");
  bar.style.width = Math.max(2, (ch.total / max) * 100) + "%";
  score.append(el("b", "", `${num(ch.total)}점`), el("small", "", ch.owners ? `${ch.owners}명 보유` : ""), bar);
  body.push(name, score);
  const li = el("li", "hm-row");
  li.dataset.heat = String(heatLevel(rank, n));
  if (first || ch.owners !== undefined) {
    const b = el("button", "hm-row-btn");
    b.type = "button";
    b.setAttribute("aria-label", describe(ch, rank));
    b.append(...body);
    b.addEventListener("click", () => (first ? first() : activation(ch, rank, sheet, opens, null, b).show()));
    li.append(b);
  } else li.append(...body);
  return li;
}

/**
 * Draws the board into `host`: a toolbar (map or list, filters), then either the treemap or the list. `fit` frames fill
 * the room their page gives them (the profile), the others are sized to the window or what the tiles need.
 */
function mountBoard(host, chars, byName, meta, ctx, fit, filters) {
  const sheet = makeSheet();
  const opens = (o) => {
    const s = byName.get(o.name);
    return s ? () => openDetail(s, meta, ctx) : undefined;
  };
  const state = { view: readView(), filter: "all", school: "", attack: "" };
  const bar = el("div", "hm-bar");
  const views = el("div", "seg");
  const viewBtns = [["map", "지도"], ["list", "목록"]].map(([id, label]) => {
    const b = el("button", "", label);
    b.type = "button";
    b.dataset.view = id;
    b.setAttribute("aria-pressed", "false");
    views.append(b);
    return b;
  });
  bar.append(views);
  const chips = el("div", "hm-chips");
  const chipBtns = [];
  if (filters) {
    for (const [id, label] of FILTERS) {
      const b = el("button", "chip-btn", label);
      b.type = "button";
      b.dataset.filter = id;
      chipBtns.push(b);
      chips.append(b);
    }
    bar.append(chips);
  }
  // school and attack type: a select each (native, so a phone shows its own picker); shown only when the server sent the traits
  const pickers = [["school", "학교", "모든 학교", meta.schools], ["attack", "공격 유형", "모든 공격 유형", meta.attack]];
  for (const [key, label, all, names] of pickers) {
    const keys = [...new Set(chars.map((c) => c[key]).filter(Boolean))].sort((a, b) => (names?.[a] ?? a).localeCompare(names?.[b] ?? b, "ko"));
    if (!keys.length) continue;
    const sel = el("select", "hm-select");
    sel.setAttribute("aria-label", label);
    sel.append(new Option(all, ""), ...keys.map((k) => new Option(names?.[k] ?? k, k)));
    sel.addEventListener("change", () => { state[key] = sel.value; announce(); state.redraw(); });
    bar.append(sel);
  }
  const legend = el("div", "hm-legend");
  legend.setAttribute("aria-hidden", "true");
  legend.append(el("span", "", "낮음"));
  for (let i = 0; i < 5; i++) { const k = el("i"); k.dataset.heat = String(i); legend.append(k); }
  legend.append(el("span", "", "높음"));
  const live = el("p", "sr-only");
  live.setAttribute("role", "status");
  const body = el("div", "hm-body");
  host.append(bar, legend, live, body);

  let stop = null;
  const flag = () => FILTERS.find((f) => f[0] === state.filter)?.[2] ?? (() => true);
  const active = () => state.filter !== "all" || state.school !== "" || state.attack !== "";
  const predicate = () => { const f = flag(); return (c) => f(c) && (!state.school || c.school === state.school) && (!state.attack || c.attack === state.attack); };
  const mapView = () => {
    const frame = el("div", fit ? "hm-frame" : "hm-frame community-frame");
    frame.setAttribute("role", "group");
    frame.setAttribute("aria-label", "학생 히트맵. 방향키로 칸을 옮기고 Enter로 엽니다.");
    body.replaceChildren(frame);
    roving(frame);
    let prev = null;
    let first = true;
    const draw = () => {
      if (!frame.isConnected) return;
      const width = frame.getBoundingClientRect().width;
      if (width < 10) return;
      const need = wantedHeight(chars.length, width);
      if (fit) frame.style.minHeight = need + "px";
      else frame.style.height = Math.max(380, innerHeight - 330, need) + "px";
      const { height } = frame.getBoundingClientRect();
      if (height < 10) return;
      prev = { w: width, h: height };
      const items = chars.map((character) => ({ value: character.total * outfitRoom(character), character }));
      const { kept, tail } = foldTail(items, width * height, coarse() ? FOLD_AREA_TOUCH : FOLD_AREA);
      const laid = tail.length ? [...kept, { value: tail.reduce((s, i) => s + i.value, 0), folded: tail.map((i) => i.character), firstRank: kept.length }] : kept;
      const tiles = squarify(laid, 0, 0, width, height);
      const keep = document.activeElement?.closest?.(".hm")?.dataset.rank;
      const pass = predicate();
      const cx = { meta, sheet, opens, dim: active() ? pass : null };
      const nodes = tiles.map((c, i) => cell(c, i, chars.length, cx, state));
      frame.replaceChildren(...nodes);
      (nodes.find((n) => n.dataset.rank === keep) ?? nodes[0])?.setAttribute("tabindex", "0");
      if (keep !== undefined) nodes.find((n) => n.dataset.rank === keep)?.focus({ preventScroll: true });
      if (first && !reducedMotion()) {
        frame.classList.add("hm-enter");
        setTimeout(() => frame.classList.remove("hm-enter"), 1600);
      }
      first = false;
    };
    const ro = new ResizeObserver(() => {
      const r = frame.getBoundingClientRect();
      if (resizeMatters(prev, r.width, r.height)) requestAnimationFrame(draw);
    });
    ro.observe(frame);
    stop = () => ro.disconnect();
    state.redraw = () => { prev = null; draw(); };
  };
  const listView = () => {
    const max = Math.max(...chars.map((c) => c.total), 1);
    const pass = predicate();
    const ol = el("ol", "hm-list");
    chars.forEach((ch, i) => {
      const li = row(ch, i, chars.length, max, opens, sheet);
      if (active() && !pass(ch)) li.classList.add("dim");
      ol.append(li);
    });
    body.replaceChildren(ol);
    state.redraw = () => listView();
  };
  const announce = () => {
    live.textContent = active() ? `${chars.filter(predicate()).length}명이 조건에 맞습니다.` : "";
  };
  const show = () => {
    stop?.();
    stop = null;
    sheet.close();
    for (const b of viewBtns) b.setAttribute("aria-pressed", String(b.dataset.view === state.view));
    for (const b of chipBtns) b.setAttribute("aria-pressed", String(b.dataset.filter === state.filter));
    legend.hidden = state.view === "list";
    if (state.view === "list") listView();
    else mapView();
    announce();
  };
  for (const b of viewBtns) b.addEventListener("click", () => { state.view = b.dataset.view; saveView(state.view); show(); });
  for (const b of chipBtns) b.addEventListener("click", () => { state.filter = state.filter === b.dataset.filter ? "all" : b.dataset.filter; for (const k of chipBtns) k.setAttribute("aria-pressed", String(k.dataset.filter === state.filter)); announce(); state.redraw(); });
  show();
}

/** One teacher's characters. `top` (summary and favourites) sits above the map. */
export function heatmapView(d, ctx, meta, top) {
  const root = el("div", "main-view");
  const note = el("p", "hint", "넓을수록 점수가 높은 학생입니다. 의상이 다른 같은 학생은 한 칸에 모았고, 너무 작은 칸은 \"그 외\"로 묶었습니다. 칸을 누르면 바로 카드가 열리고, 카드 아래에서 같은 학생의 다른 의상으로 바꿀 수 있습니다. 목록으로 바꾸면 순위대로 읽을 수 있습니다.");
  if (top) root.append(top);
  root.append(note);
  root.classList.add("scrollable");
  const chars = [...d.analysis.characters].filter((c) => c.total > 0).sort((a, b) => b.total - a.total);
  mountBoard(root, chars, ctx.byName, meta, ctx, true, true);
  return root;
}

/** Every public teacher's characters added up. A tile is a character, its area the sum of what it is worth to the
 *  teachers who own it. Nothing here belongs to one student, so a tile shows its numbers and opens no card. */
export function communityHeatmap(characters, meta, teachers) {
  const root = el("div", "community");
  root.append(el("p", "hint", `공개한 선생님 ${teachers}명이 가진 학생을 캐릭터별로 더한 지도입니다. 넓을수록 많은 선생님이 아끼는 학생입니다. 칸을 누르거나 목록으로 바꾸면 보유 인원이 보입니다.`));
  mountBoard(root, characters.filter((c) => c.total > 0), new Map(), meta, { duty: new Set(), unread: new Map(), byName: new Map() }, false, false);
  return root;
}
