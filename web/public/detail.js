// The detail sheet of one owned student: opens over the tiles, closes with its button, Esc or a click outside.
import { ARMOR_HUE, armorColor, ATTACK_HUE, attackColor, bondBadge, EQUIP_KO, halo, icon, meter, name, pinBadge, POSITION_KO, ROLE, schoolLogo, stars, TACTIC_KO } from "./domain.js";
import { glyph, positionGlyph } from "./icons.js";
import { el, num, portrait } from "./ui.js";

function tag(text, color) {
  const t = el("span", "tag", text);
  if (color) t.style.setProperty("--tag", color);
  return t;
}

function skillRow(label, value, max, locked) {
  const row = el("div", "skill" + (locked ? " locked" : ""));
  row.append(el("span", "skill-name", label), meter(value ?? 0, max), el("b", "", locked ? "잠금" : value === null ? "-" : `Lv.${value}`));
  return row;
}


// Card tiers by the character's place in the teacher's overall ranking. The top three are special
// illustration cards (the memorial lobby art over the whole card; SAR has no frame, UR a gold one, SR a silver one),
// four and five are framed foil cards, the rest plain.
const TIERS = [
  { key: "sar", label: "SAR", art: "lobby", full: true, frame: "none" },
  { key: "ur", label: "UR", art: "lobby", full: true, frame: "gold" },
  { key: "sr", label: "SR", art: "lobby", full: true, frame: "silver" },
  { key: "ar", label: "AR", art: "standing", full: false },
  { key: "rr", label: "RR", art: "standing", full: false },
];

/** Share of ALL characters of the game each grade takes, from the top: SAR 1%, UR 2%, SR 3%, AR 6%, RR 10%. The
 *  grade of a card is its character's place among the teacher's characters, measured against this share of the
 *  whole roster, so it means the same for a teacher with 40 characters and one with 140. */
export const GRADE_SHARE = [0.01, 0.02, 0.03, 0.06, 0.10];
const FALLBACK_TOTAL = 145;

/** `rank` is 0-based. The grade, or the plain card past the last one. */
export function tierFor(rank, totalCharacters) {
  if (rank < 0) return PLAIN;
  const total = totalCharacters || FALLBACK_TOTAL;
  let upTo = 0;
  for (let i = 0; i < TIERS.length; i++) {
    upTo += Math.max(i === 0 ? 1 : 0, Math.round(total * GRADE_SHARE[i]));
    if (rank < upTo) return TIERS[i];
  }
  return PLAIN;
}
const PLAIN = { key: "n", label: "", art: "standing", full: false };
const CARD_W = 380;
const CARD_H = 640;

/** The background of a student's card, as chosen at upload time; a student uploaded before backgrounds existed gets
 *  one picked from the name, so it never changes. Used by the card and by the student tile, so they match. */
export function cardBackground(s, meta) {
  return s.bg ? { bg: s.bg, third: s.bg_third ?? 1 } : fallbackBackground(s.name, meta.backgrounds);
}

/** Puts a background (and the third of it that shows) on an element: the picture is enlarged, one third is visible. */
export function paintBackground(node, pick) {
  if (!pick) return;
  node.style.backgroundImage = `url("/backgrounds/${encodeURIComponent(pick.bg)}.jpg")`;
  node.style.backgroundSize = "170% auto";
  node.style.backgroundPosition = ["0% 50%", "50% 50%", "100% 50%"][pick.third] ?? "50% 50%";
}

function fallbackBackground(name, list) {
  if (!list?.length) return null;
  let h = 0;
  for (const c of name) h = (h * 31 + c.codePointAt(0)) >>> 0;
  return { bg: list[h % list.length], third: (h >>> 8) % 3 };
}

/** The cut artwork first, then the uncut one, then the small portrait; a card never ends up without a picture. */
function artFor(s, tier, art) {
  const id = encodeURIComponent(s.student_id);
  const chain = (tier.art === "lobby" ? [`/card-lobby/${id}.webp`, `/lobby/${id}.webp`] : [`/card/${id}.webp`, `/standing/${id}.webp`]).concat(`/portraits/${id}.webp`);
  const img = el("img", "pc-art-img " + tier.art);
  // full-art cards continue the picture, blurred, behind the text area
  img.addEventListener("load", () => art.style.setProperty("--art", `url("${img.src}")`));
  let i = 0;
  img.addEventListener("error", () => { if (++i < chain.length) img.src = chain[i]; else img.replaceWith(el("div", "pc-art-none", "일러스트 없음")); });
  img.src = chain[0];
  img.alt = "";
  return img;
}

/** An icon in a round chip. The words show on hover or keyboard focus. */
function chip(svg, text, cls, label = false) {
  const c = el("span", "pc-chip " + (cls ?? "") + (label ? " lab" : ""));
  c.tabIndex = 0;
  c.dataset.tip = text;
  c.setAttribute("aria-label", text);
  c.append(svg);
  if (label) c.append(el("em", "", text));
  return c;
}

/** Attack and armor type together as one pill, (Mystic / Heavy armor) style: two halves, each with its own icon and colour. */
function typePill(s, meta) {
  const pill = el("span", "type-pill");
  const half = (key, kind, label, hue) => {
    const h = el("span", "half " + kind);
    h.tabIndex = 0;
    h.dataset.tip = label;
    h.setAttribute("aria-label", label);
    h.style.setProperty("--hue", String(hue));
    h.append(glyph(key, 15));
    return h;
  };
  pill.append(
    half(s.attack ?? "Normal", "atk", "공격 타입 " + name.attack(meta, s.attack), ATTACK_HUE[s.attack] ?? 220),
    half(s.armor ?? "Normal", "arm", "방어 타입 " + name.armor(meta, s.armor), ARMOR_HUE[s.armor] ?? 220),
  );
  return pill;
}

function stat(label, value) {
  const d = el("div", "pc-stat");
  d.append(el("span", "", label), el("b", "", value === undefined || value === null ? "-" : num(value)));
  return d;
}


const GRADE_LETTER = ["D", "C", "B", "A", "S"];
const TIER_HUE = [210, 200, 170, 130, 90, 50, 30, 10, 330, 290];

function bkSection(title, ...kids) {
  const s = el("section", "bk-sec");
  s.append(el("h3", "", title), ...kids);
  return s;
}

function bkRow(label, value) {
  const r = el("div", "bk-row");
  r.append(el("span", "", label), el("b", "", value === undefined || value === null ? "-" : typeof value === "string" ? value : num(value)));
  return r;
}

/** The back of the card: every scanned spec, in panels. Missing parts are left out, not shown as dashes. */
function backFace(s, meta, tier, rank, turn) {
  const d = s.detail ?? {};
  const back = el("div", "pc-face pc-back");
  const head = el("div", "bk-head");
  const who = el("div", "bk-who");
  who.append(el("h2", "", s.name), el("small", "", [name.school(meta, s.school), tier.label ? `${tier.label} · 전체 ${rank + 1}위` : ""].filter(Boolean).join(" · ")));
  head.append(icon(s.student_id, "ico bk-ico"), who);
  const body = el("div", "bk-body");

  if (d.stats) {
    const g = el("div", "bk-stats");
    for (const [k, label] of [["max_hp", "최대 체력"], ["attack", "공격력"], ["defense", "방어력"], ["healing", "치유력"]]) {
      const c = el("div", "bk-stat " + k);
      c.append(el("span", "", label), el("b", "", d.stats[k] === undefined ? "-" : num(d.stats[k])));
      g.append(c);
    }
    body.append(bkSection("전투 능력치", g));
  }
  if (d.exp && d.exp.required) {
    const bar = el("div", "bk-xp");
    const fill = el("i");
    fill.style.width = Math.min(100, Math.round(((d.exp.current ?? 0) / d.exp.required) * 100)) + "%";
    bar.append(fill);
    body.append(bkSection("레벨 경험치", bar, el("small", "bk-note", `${num(d.exp.current)} / ${num(d.exp.required)}`)));
  }
  const pop = el("div", "bk-pop");
  if (d.stat_detail) {
    const rows = el("div", "bk-rows");
    for (const [k, label, unit] of EXTRA_STATS) if (d.stat_detail[k] !== undefined) rows.append(bkRow(label, unit ? `${num(d.stat_detail[k])}${unit}` : d.stat_detail[k]));
    if (rows.childNodes.length) pop.append(bkSection("상세 능력치", rows));
  }
  if (d.terrain) {
    const row = el("div", "bk-terrain");
    for (const [k, label] of [["urban", "시가지"], ["outdoor", "야외"], ["indoor", "실내"]]) {
      if (d.terrain[k] === undefined) continue;
      const grade = GRADE_LETTER[d.terrain[k]];
      const c = el("div", "bk-terr g" + grade);
      c.append(el("b", "", grade), el("span", "", label));
      row.append(c);
    }
    body.append(bkSection("지형 적응", row));
  }
  if (d.equipment) {
    const slots = el("div", "bk-gear");
    for (const e of d.equipment) {
      // the type comes from the student's slot (static data); the scan only says the tier and the level
      const kind = e.unique ? null : e.type ?? s.equipment_slots?.[e.slot - 1] ?? null;
      const item = kind && e.tier ? meta.equipment?.[kind]?.[String(e.tier)] : null;
      const g = el("div", "bk-slot" + (e.equipped === false ? " empty" : ""));
      if (e.tier) g.style.setProperty("--th", String(TIER_HUE[e.tier - 1] ?? 210));
      const top = el("div", "bk-slot-top");
      top.append(el("span", "bk-slot-kind", e.unique ? "고유 장비" : EQUIP_KO[kind] ?? `장비 ${e.slot}`));
      if (e.tier) top.append(el("b", "bk-tier", `T${e.tier}`));
      g.append(top);
      g.append(el("span", "bk-slot-name", item?.name ?? (e.equipped === false ? "미장착" : e.tier ? "이름 없음" : "티어를 읽지 못함")));
      if (e.level) {
        const cap = item?.max_level ?? TIER_LEVEL_CAP[(e.tier ?? 0) - 1];
        const bar = el("div", "bk-lv");
        const fill = el("i");
        fill.style.width = cap ? Math.min(100, Math.round((e.level / cap) * 100)) + "%" : "100%";
        bar.append(fill);
        g.append(bar, el("small", "", `Lv.${e.level}${cap ? ` / ${cap}` : ""}`));
      }
      slots.append(g);
    }
    body.append(bkSection("장비", slots));
  }
  if (d.mystic) {
    const rows = el("div", "bk-rows");
    for (const [k, label] of [["stars", "현재 성급"], ["next_stars", "다음 성급"], ["max_bond_rank", "인연 랭크 상한"], ["next_max_bond_rank", "다음 인연 상한"], ["next_max_hp", "다음 최대 체력"], ["next_attack", "다음 공격력"], ["next_healing", "다음 치유력"]]) {
      if (d.mystic[k] !== undefined) rows.append(bkRow(label, d.mystic[k]));
    }
    if (rows.childNodes.length) pop.append(bkSection("신비 해방", rows));
  }
  // the first section (battle stats) opens the popup with the rest of the specs: the card never grows or scrolls
  const hot = body.firstElementChild;
  if (pop.childNodes.length && hot) {
    hot.classList.add("bk-hot");
    hot.tabIndex = 0;
    hot.querySelector("h3")?.append(el("em", "bk-hint", "상세 보기"));
  }
  if (!body.childNodes.length) body.append(el("p", "bk-empty", "이 학생은 능력치를 읽지 않았습니다. 학생 스캔을 다시 하면 채워집니다."));

  back.append(head, body);
  if (pop.childNodes.length) back.append(pop);
  return back;
}

const TIER_LEVEL_CAP = [10, 20, 30, 40, 45, 50, 55, 60, 65, 70];
const EXTRA_STATS = [
  ["accuracy", "명중 수치"], ["evasion", "회피 수치"], ["critical", "치명 수치"], ["critical_damage_pct", "치명 피해율", "%"],
  ["defense_pierce", "방어력 무시"], ["stability", "안정 수치"], ["range", "사거리"], ["cost_recovery", "코스트 회복력"],
  ["cc_power", "CC 강화력"], ["cc_resist", "CC 저항력"], ["type_bonus_pct", "타입 보너스", "%"], ["max_cost_bonus", "최대 코스트 보너스"],
];

/** One card. `animate` plays the landing motion; switching outfits skips the long intro. */
function buildCard(s, meta, ctx, rank, animate) {
  const tier = tierFor(rank, ctx.totalCharacters);
  const det = s.detail ?? {};
  const card = el("div", "pcard tier-" + tier.key + (tier.full ? " full frame-" + tier.frame : "") + (animate ? "" : " quick"));
  card.setAttribute("role", "dialog");
  card.setAttribute("aria-modal", "true");
  card.setAttribute("aria-label", s.name);
  card.dataset.atk = s.attack ?? "Normal";
  card.dataset.arm = s.armor ?? "Normal";
  card.style.setProperty("--h", String(ATTACK_HUE[s.attack] ?? 220));
  card.style.setProperty("--ha", String(ARMOR_HUE[s.armor] ?? 220));
  const inner = el("div", "pc-inner");

  const front = el("div", "pc-face pc-front");
  const head = el("div", "pc-head");
  const title = el("div", "pc-title");
  title.append(el("small", "", name.school(meta, s.school)), el("h2", "", s.name));
  const hp = el("div", "pc-hp");
  hp.append(el("small", "", "최대 체력"), el("b", "", det.stats?.max_hp ? num(det.stats.max_hp) : "-"));
  head.append(schoolLogo(s.school), title, hp);

  const band = el("div", "pc-band");
  const art = el("div", "pc-art");
  // a high resolution portrait cut fills the whole card; the small thumbnail falls back to picture plus blurred extension
  if (tier.full && meta.hiresLobby?.has(s.student_id)) art.classList.add("hires");
  if (s.student_id) art.append(artFor(s, tier, art));
  else art.append(el("div", "pc-art-none", "일러스트 없음"));
  if (!tier.full) {
    // mid-grade cards: a background picked at upload time under the cut-out portrait
    const bg = el("div", "pc-bg");
    paintBackground(bg, cardBackground(s, meta));
    art.prepend(bg);
  }
  if (tier.key !== "n") {
    // the foil lives inside the picture only, so the text beside it stays readable
    // layers, bottom to top: base foil, texture, glitter that only shows near the light, glare
    art.append(el("div", "pc-foil"), el("div", "pc-holo"), el("div", "pc-sparkle"), el("div", "pc-glare"));
  }
  const lv = el("div", "pc-lv");
  lv.append(el("small", "", "Lv."), el("b", "", s.level === null ? "-" : String(s.level)));
  const starRow = s.stars ? stars(s.stars, "student") : null;
  starRow?.classList.add("pc-stars");
  if (!tier.full) {
    art.append(lv);
    if (starRow) art.append(starRow);
  }
  band.append(art);

  const types = el("div", "pc-types");
  // tank / dealer / supporter / healer, and front / middle / back: both written out, they matter most in a fight
  if (s.tactic) types.append(chip(glyph(s.tactic, 16), TACTIC_KO[s.tactic] ?? s.tactic, "tactic " + s.tactic, true));
  const position = det.position ?? s.position;
  if (position) types.append(chip(positionGlyph(position, 16), POSITION_KO[position] ?? position, "pos", true));
  if (s.role) types.append(chip(glyph(s.role, 16), ROLE[s.role] ?? s.role, "role"));
  if (s.attack || s.armor) types.append(typePill(s, meta));
  if (s.uw_stars) {
    const w = el("span", "pc-weapon");
    w.append(el("small", "", "전용 무기"), stars(s.uw_stars, "weapon", 4), el("small", "", s.uw_level ? `Lv.${s.uw_level}` : ""));
    types.append(w);
  }

  const lock = det.skill_locked ?? {};
  const moves = el("div", "pc-moves");
  moves.append(skillRow("EX 스킬", s.ex_skill, 5, lock.ex), skillRow("기본 스킬", s.normal_skill, 10, lock.basic), skillRow("강화 스킬", s.passive_skill, 10, lock.passive), skillRow("서브 스킬", s.sub_skill, 10, lock.sub));

  const foot = el("div", "pc-foot");
  const bond = el("div", "pc-bond");
  bond.append(el("small", "", "인연 랭크"), bondBadge(s.bond, "bond lg"));
  const nums = el("div", "pc-stats");
  nums.append(stat("공격력", det.stats?.attack), stat("방어력", det.stats?.defense), stat("치유력", det.stats?.healing));
  foot.append(bond, nums);

  const marks = el("div", "tags pc-marks");
  if (s.pinned) {
    const pin = tag("", "#c8a020");
    pin.classList.add("pin-tag");
    pin.append(pinBadge("pin-in"));
    marks.append(pin);
  }
  if (s.memorial) marks.append(tag("메모리얼 로비", "#d65aaa"));
  if (ctx.duty.has(s.name)) marks.append(tag("당번", "#2c6fd0"));
  const unread = ctx.unread.get(s.name);
  if (unread) marks.append(tag(`안 읽은 모모톡 ${unread}`, "#2f9a6a"));

  const body = el("div", "pc-body");
  body.append(types, moves, foot, marks);
  if (tier.full) {
    // full art: level and stars float as the first line of the overlay, not as badges on the picture
    const line = el("div", "pc-lvline");
    line.append(lv);
    if (starRow) line.append(starRow);
    body.prepend(line);
  }
  if (tier.label) {
    // the grade and the overall place, at the very bottom of the card
    const grade = el("div", "pc-grade");
    grade.append(el("b", "", tier.label), el("span", "", `전체 ${rank + 1}위 · 상위 ${((rank + 1) / (ctx.totalCharacters || FALLBACK_TOTAL) * 100).toFixed(1)}%`));
    body.append(grade);
  }
  front.append(band, head, body);

  const back = backFace(s, meta, tier, rank, () => turn());
  inner.append(front, back);
  card.append(inner);

  let flipped = false;
  let turning = 0;
  const turn = () => {
    flipped = !flipped;
    card.classList.toggle("flipped", flipped);
    // the frame and the shine are flat layers over the card: they step aside while it turns
    card.classList.add("turning");
    clearTimeout(turning);
    turning = setTimeout(() => card.classList.remove("turning"), 750);
  };

  // the rim belongs to each face, so it turns with the card instead of staying put
  front.append(el("div", "pc-edge"));
  back.append(el("div", "pc-edge"));
  lightEngine(card);
  dragFlip(card, inner, () => flipped, turn);
  return card;
}

const FLIP_AT = 55; // degrees of drag after which the card turns over; less than this and it settles back
const PX_TO_DEG = 0.55; // sideways: a drag of 330 px turns the card 180 degrees
const TILT_MAX = 35; // an up or down part of the drag only tilts the card, up to this

/**
 * Turn the card by hand. Dragging sideways follows the pointer one to one (left to right, right to left); an up or
 * down part tilts it, so a diagonal drag turns and tilts together. On release, past FLIP_AT degrees the card finishes
 * the turn in the direction it was dragged and shows the other face, otherwise it settles back. Buttons, links and the
 * outfit picker are left alone, and a drag never ends as a click on what is under the pointer.
 */
function dragFlip(card, inner, isFlipped, turn) {
  const SKIP = "a, button, input, select, textarea, [role=tab], [data-tip]";
  // the browser's own image drag would take over the gesture
  card.addEventListener("dragstart", (e) => e.preventDefault());
  for (const img of card.querySelectorAll("img")) img.draggable = false;
  let drag = null;
  let swallow = false;
  card.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || drag || e.target.closest(SKIP)) return;
    drag = { id: e.pointerId, x: e.clientX, y: e.clientY, base: isFlipped() ? 180 : 0, ry: 0, rx: 0, moved: false };
  });
  card.addEventListener("pointermove", (e) => {
    if (!drag || e.pointerId !== drag.id) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (!drag.moved) {
      if (Math.hypot(dx, dy) < 6) return;
      drag.moved = true;
      try { card.setPointerCapture(e.pointerId); } catch { /* a pointer that is already gone */ }
      card.classList.add("dragging");
      inner.style.transition = "none";
    }
    drag.ry = Math.max(-180, Math.min(180, dx * PX_TO_DEG));
    drag.rawTilt = -dy * 0.4;
    drag.rx = Math.max(-TILT_MAX, Math.min(TILT_MAX, drag.rawTilt));
    inner.style.transform = `rotateX(${drag.rx}deg) rotateY(${drag.base + drag.ry}deg)`;
  });
  const release = (e) => {
    if (!drag || e.pointerId !== drag.id) return;
    const d = drag;
    drag = null;
    if (!d.moved) {
      if (e.type === "pointerup") turn();
      return;
    }
    swallow = true;
    setTimeout(() => { swallow = false; }, 0);
    const over = Math.hypot(d.ry, d.rawTilt ?? 0) > FLIP_AT && e.type !== "pointercancel";
    const sign = Math.abs(d.ry) >= Math.abs(d.rawTilt ?? 0) ? Math.sign(d.ry) || 1 : Math.sign(-(d.rawTilt ?? 0)) || 1;
    inner.style.transition = "transform .5s cubic-bezier(.3, .8, .3, 1)";
    inner.style.transform = `rotateX(0deg) rotateY(${d.base + (over ? sign * 180 : 0)}deg)`;
    let done = false;
    const settle = () => {
      if (done) return;
      done = true;
      // hand the pose back to the stylesheet: same picture, no inline state left behind
      inner.style.transition = "none";
      inner.style.transform = "";
      if (over) turn();
      void inner.offsetWidth;
      inner.style.transition = "";
      card.classList.remove("dragging");
    };
    inner.addEventListener("transitionend", settle, { once: true });
    setTimeout(settle, 600);
  };
  card.addEventListener("pointerup", release);
  card.addEventListener("pointercancel", release);
  card.addEventListener("click", (e) => { if (swallow) { e.stopPropagation(); e.preventDefault(); } }, true);
}

const clamp01 = (v) => Math.min(Math.max(v, 0), 1);

/**
 * The light and tilt of a held card. The pointer sets a target; the card follows it with a damped spring, so
 * motion is smooth and has weight. With no pointer the card rests: nothing moves by itself, so the text stays still. The state is published as CSS variables the stylesheet turns into tilt, parallax and foil:
 *   --px --py   light position in percent      --nx --ny   the same, -1 to 1
 *   --rx --ry   tilt in degrees                --o         light intensity, 0 to 1
 *   --hyp       distance of the light from the card centre, 0 to 1
 */
function lightEngine(card) {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
    card.style.setProperty("--o", "0.5");
    return;
  }
  const s = { x: 50, y: 50, vx: 0, vy: 0, o: 0.5, vo: 0 };
  let tx = 50, ty = 50, to = 0.5, pointer = false;
  const t0 = performance.now();
  const onMove = (e) => {
    const r = card.getBoundingClientRect();
    tx = clamp01((e.clientX - r.left) / r.width) * 100;
    ty = clamp01((e.clientY - r.top) / r.height) * 100;
    to = 1;
    pointer = true;
  };
  card.addEventListener("pointermove", onMove);
  card.addEventListener("pointerleave", () => { pointer = false; });
  let last = t0;
  const frame = (now) => {
    if (!card.isConnected) return;
    const dt = Math.min((now - last) / 1000, 0.05);
    last = now;
    if (!pointer) {
      tx = 50;
      ty = 50;
      to = 0.5;
    }
    // critically damped spring: x'' = k (target - x) - c x'
    const k = 90, c = 2 * Math.sqrt(k);
    s.vx += (k * (tx - s.x) - c * s.vx) * dt; s.x += s.vx * dt;
    s.vy += (k * (ty - s.y) - c * s.vy) * dt; s.y += s.vy * dt;
    s.vo += (60 * (to - s.o) - 2 * Math.sqrt(60) * s.vo) * dt; s.o += s.vo * dt;
    const nx = (s.x - 50) / 50, ny = (s.y - 50) / 50;
    const st = card.style;
    st.setProperty("--px", s.x.toFixed(2) + "%");
    st.setProperty("--py", s.y.toFixed(2) + "%");
    st.setProperty("--nx", nx.toFixed(3));
    st.setProperty("--ny", ny.toFixed(3));
    st.setProperty("--rx", (-ny * 9).toFixed(2) + "deg");
    st.setProperty("--ry", (nx * 11).toFixed(2) + "deg");
    st.setProperty("--o", clamp01(s.o).toFixed(3));
    st.setProperty("--hyp", clamp01(Math.hypot(nx, ny) / 1.2).toFixed(3));
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

/** The picked outfit's siblings: one small button per owned outfit of the same character. */
function outfitPicker(list, current, onPick) {
  const bar = el("div", "pc-outfits");
  bar.setAttribute("role", "tablist");
  bar.setAttribute("aria-label", "의상 선택");
  for (const o of list) {
    const b = el("button", "pc-outfit", "");
    b.type = "button";
    b.setAttribute("role", "tab");
    b.setAttribute("aria-selected", String(o.name === current.name));
    b.title = o.name;
    b.append(icon(o.student_id, "ico"), el("span", "", o.name));
    b.addEventListener("click", () => onPick(o));
    bar.append(b);
  }
  return bar;
}

const INTRO_MS = 1100;

/** The halo spins up over the dark scrim and throws sparkles, then shrinks away as the card turns into view. */
function genericHalo() {
  // a plain halo drawn here for students whose own halo is not in the assets: broken rings, so the turn is visible
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 200 200");
  svg.setAttribute("class", "pi-halo pi-generic");
  const add = (tag, attrs) => {
    const n = document.createElementNS(NS, tag);
    for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
    svg.append(n);
  };
  add("circle", { cx: 100, cy: 100, r: 74, fill: "none", stroke: "currentColor", "stroke-width": 6, "stroke-dasharray": "64 18 22 18", "stroke-linecap": "round" });
  add("circle", { cx: 100, cy: 100, r: 48, fill: "none", stroke: "currentColor", "stroke-width": 4, "stroke-dasharray": "40 12" });
  add("path", { d: "M100 14 L108 40 L92 40 Z M186 100 L160 108 L160 92 Z", fill: "currentColor" });
  add("circle", { cx: 100, cy: 100, r: 9, fill: "currentColor" });
  return svg;
}

/** The student's halo, else the halo of another outfit of the same character, else a plain one. */
function introHalo(students, meta) {
  for (const x of students) {
    const h = halo(meta, x.student_id, "pi-halo");
    if (h) return h;
  }
  return genericHalo();
}

function intro(s, meta, tier, students) {
  const box = el("div", "pc-intro pi-" + tier.key);
  box.append(introHalo(students, meta));
  for (let i = 0; i < 18; i++) {
    const p = el("i", "pi-spark");
    p.style.setProperty("--a", Math.round((360 / 18) * i + Math.random() * 14) + "deg");
    p.style.setProperty("--d", 90 + Math.round(Math.random() * 90) + "px");
    p.style.setProperty("--s", (0.6 + Math.random() * 0.9).toFixed(2));
    p.style.setProperty("--t", (0.35 + Math.random() * 0.45).toFixed(2) + "s");
    box.append(p);
  }
  return box;
}

export function openDetail(s, meta, ctx) {
  const prev = document.activeElement;
  const rank = ctx.rankOf ? ctx.rankOf(s.name) : -1;
  const tier = tierFor(rank, ctx.totalCharacters);
  const outfits = ctx.siblings ? ctx.siblings(s.name) : [s];

  const scrim = el("div", "scrim");
  const stage = el("div", "pc-stage");
  const close = el("button", "ghost pc-close", "닫기");
  close.type = "button";

  // the card is laid out at one size and scaled to whatever window it is shown in
  const fit = () => stage.style.setProperty("--fit", Math.min(1, (innerWidth - 24) / CARD_W, (innerHeight - 150) / CARD_H).toFixed(3));
  fit();
  addEventListener("resize", fit);

  let currentCard = null;
  const show = (student, animate) => {
    currentCard = buildCard(student, meta, ctx, rank, animate);
    stage.replaceChildren(currentCard);
    picker?.querySelectorAll("button").forEach((b) => b.setAttribute("aria-selected", String(b.title === student.name)));
  };
  const picker = outfits.length > 1 ? outfitPicker(outfits, s, (o) => show(o, false)) : null;

  show(s, true);
  scrim.append(intro(s, meta, tier, [s, ...outfits.filter((o) => o !== s)]), stage);
  if (picker) scrim.append(picker);
  scrim.append(close);
  document.body.append(scrim);
  setTimeout(() => scrim.querySelector(".pc-intro")?.remove(), INTRO_MS + 700);

  const shut = () => {
    document.removeEventListener("keydown", onKey);
    removeEventListener("resize", fit);
    scrim.classList.add("out");
    setTimeout(() => scrim.remove(), 160);
    prev?.focus?.();
  };
  const onKey = (e) => { if (e.key === "Escape") shut(); };
  document.addEventListener("keydown", onKey);
  scrim.addEventListener("click", (e) => { if (e.target === scrim) shut(); });
  close.addEventListener("click", shut);
  close.focus();
}
