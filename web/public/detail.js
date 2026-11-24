// The detail sheet of one owned student: opens over the tiles, closes with its button, Esc or a click outside.
import { armorColor, attackColor, bondBadge, halo, icon, meter, name, ROLE, schoolLogo, stars } from "./domain.js";
import { el, num, portrait } from "./ui.js";

function tag(text, color) {
  const t = el("span", "tag", text);
  if (color) t.style.setProperty("--tag", color);
  return t;
}

function skillRow(label, value, max, locked) {
  const row = el("div", "skill" + (locked ? " locked" : ""));
  row.append(el("span", "skill-name", label), meter(value ?? 0, max), el("b", "", value === null ? "-" : `Lv.${value}`));
  return row;
}


const POSITION = { FRONT: "전열", MIDDLE: "중열", BACK: "후열" };
const GRADE = ["D", "C", "B", "A", "S"];
const EXTRA_STATS = [
  ["accuracy", "명중 수치"], ["evasion", "회피 수치"], ["critical", "치명 수치"], ["critical_damage_pct", "치명 피해율", "%"],
  ["defense_pierce", "방어력 무시"], ["stability", "안정 수치"], ["range", "사거리"], ["cost_recovery", "코스트 회복력"],
  ["cc_power", "CC 강화력"], ["cc_resist", "CC 저항력"], ["type_bonus_pct", "타입 보너스", "%"], ["max_cost_bonus", "최대 코스트 보너스"],
];

function section(title, ...kids) {
  const s = el("div", "dsec");
  s.append(el("h3", "", title), ...kids);
  return s;
}

function figure(label, value, unit = "") {
  const f = el("div", "fig");
  f.append(el("span", "stat-label", label), el("b", "", value === undefined || value === null ? "-" : num(value) + unit));
  return f;
}

function detailSections(d) {
  if (!d) return [el("p", "empty", "이 학생은 능력치를 읽지 않았습니다. 학생 스캔을 다시 하면 채워집니다.")];
  const out = [];
  const head = el("div", "tags");
  if (d.position) head.append(Object.assign(el("span", "tag", POSITION[d.position] ?? d.position), {}));
  if (d.weapon_class) head.append(el("span", "tag", "무기 " + d.weapon_class));
  if (head.childNodes.length) out.push(head);
  if (d.stats) {
    const g = el("div", "figs");
    g.append(figure("최대 체력", d.stats.max_hp), figure("공격력", d.stats.attack), figure("방어력", d.stats.defense), figure("치유력", d.stats.healing));
    out.push(section("전투 능력치", g));
  }
  if (d.exp && d.exp.required) {
    const bar = el("div", "xp");
    const fill = el("i");
    fill.style.width = Math.min(100, Math.round(((d.exp.current ?? 0) / d.exp.required) * 100)) + "%";
    bar.append(fill);
    out.push(section("레벨 경험치", bar, el("small", "hint", `${num(d.exp.current)} / ${num(d.exp.required)}`)));
  }
  if (d.stat_detail) {
    const g = el("div", "figs small");
    for (const [k, label, unit] of EXTRA_STATS) if (d.stat_detail[k] !== undefined) g.append(figure(label, d.stat_detail[k], unit));
    out.push(section("상세 능력치", g));
  }
  if (d.terrain) {
    const row = el("div", "terrain");
    for (const [k, label] of [["urban", "시가지"], ["outdoor", "야외"], ["indoor", "실내"]]) {
      if (d.terrain[k] === undefined) continue;
      const grade = GRADE[d.terrain[k]];
      const c = el("div", "terr g" + grade);
      c.append(el("span", "", label), el("b", "", grade));
      row.append(c);
    }
    out.push(section("지형 적응", row));
  }
  if (d.equipment) {
    const row = el("div", "gear");
    for (const e of d.equipment) {
      const g = el("div", "gear-slot" + (e.equipped === false ? " empty" : ""));
      g.append(el("b", "", e.unique ? "고유 장비" : `장비 ${e.slot}`), el("span", "", e.item_name ?? e.type ?? (e.equipped === false ? "미장착" : "-")), el("small", "", [e.tier ? `T${e.tier}` : "", e.level ? `Lv.${e.level}` : ""].filter(Boolean).join(" ")));
      row.append(g);
    }
    out.push(section("장비", row));
  }
  if (d.mystic) {
    const g = el("div", "figs small");
    for (const [k, label] of [["stars", "현재 성급"], ["next_stars", "다음 성급"], ["max_bond_rank", "인연 랭크 상한"], ["next_max_bond_rank", "다음 인연 상한"], ["next_max_hp", "다음 최대 체력"], ["next_attack", "다음 공격력"], ["next_healing", "다음 치유력"]]) {
      if (d.mystic[k] !== undefined) g.append(figure(label, d.mystic[k]));
    }
    out.push(section("신비 해방", g));
  }
  return out;
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
const PLAIN = { key: "n", label: "", art: "standing", full: false };
const CARD_W = 380;
const CARD_H = 640;

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

function stat(label, value) {
  const d = el("div", "pc-stat");
  d.append(el("span", "", label), el("b", "", value === undefined || value === null ? "-" : num(value)));
  return d;
}

/** One card. `animate` plays the landing motion; switching outfits skips the long intro. */
function buildCard(s, meta, ctx, rank, animate) {
  const tier = TIERS[rank] ?? PLAIN;
  const det = s.detail ?? {};
  const card = el("div", "pcard tier-" + tier.key + (tier.full ? " full frame-" + tier.frame : "") + (animate ? "" : " quick"));
  card.setAttribute("role", "dialog");
  card.setAttribute("aria-modal", "true");
  card.setAttribute("aria-label", s.name);
  const inner = el("div", "pc-inner");

  const front = el("div", "pc-face pc-front");
  const head = el("div", "pc-head");
  const title = el("div", "pc-title");
  title.append(el("small", "", tier.label ? `${tier.label} · 전체 ${rank + 1}위` : (s.role ? ROLE[s.role] ?? "" : "")), el("h2", "", s.name));
  const hp = el("div", "pc-hp");
  hp.append(el("small", "", "최대 체력"), el("b", "", det.stats?.max_hp ? num(det.stats.max_hp) : "-"));
  head.append(schoolLogo(s.school), title, hp);

  const band = el("div", "pc-band");
  const art = el("div", "pc-art");
  if (s.student_id) art.append(artFor(s, tier, art));
  else art.append(el("div", "pc-art-none", "일러스트 없음"));
  const ring = halo(meta, s.student_id, "pc-halo");
  if (ring) art.append(ring);
  if (tier.key !== "n") {
    // the foil lives inside the picture only, so the text beside it stays readable
    art.append(el("div", "pc-holo"), el("div", "pc-glare"));
    if (tier.full) art.append(el("div", "pc-sparkle"));
  }
  const lv = el("div", "pc-lv");
  lv.append(el("small", "", "Lv."), el("b", "", s.level === null ? "-" : String(s.level)));
  art.append(lv);
  if (s.stars) {
    const st = stars(s.stars, "student");
    st.classList.add("pc-stars");
    art.append(st);
  }
  band.append(art);

  const types = el("div", "pc-types");
  if (s.role) types.append(tag(ROLE[s.role] ?? s.role));
  if (s.attack) types.append(tag("공격 " + name.attack(meta, s.attack), attackColor(s.attack)));
  if (s.armor) types.append(tag("방어 " + name.armor(meta, s.armor), armorColor(s.armor)));
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
  if (s.pinned) marks.append(tag("고정", "#c8a020"));
  if (s.memorial) marks.append(tag("메모리얼 로비", "#d65aaa"));
  if (ctx.duty.has(s.name)) marks.append(tag("당번", "#2c6fd0"));
  const unread = ctx.unread.get(s.name);
  if (unread) marks.append(tag(`안 읽은 모모톡 ${unread}`, "#2f9a6a"));

  const flip = el("button", "pc-flip", "상세 능력치 보기");
  flip.type = "button";
  const body = el("div", "pc-body");
  body.append(types, moves, foot, marks, flip);
  front.append(band, head, body);

  const back = el("div", "pc-face pc-back");
  const bh = el("div", "pc-back-head");
  bh.append(el("h2", "", s.name), el("small", "", "상세 능력치"));
  const specs = el("div", "pc-specs");
  specs.append(...detailSections(s.detail));
  const unflip = el("button", "pc-flip", "카드 앞면으로");
  unflip.type = "button";
  back.append(bh, specs, unflip);
  inner.append(front, back);
  card.append(inner);

  let flipped = false;
  const turn = () => { flipped = !flipped; card.classList.toggle("flipped", flipped); };
  flip.addEventListener("click", turn);
  unflip.addEventListener("click", turn);

  // tilt and light follow the pointer
  card.addEventListener("pointermove", (e) => {
    const r = card.getBoundingClientRect();
    const x = clamp01((e.clientX - r.left) / r.width);
    const y = clamp01((e.clientY - r.top) / r.height);
    card.style.setProperty("--mx", (x * 100).toFixed(1) + "%");
    card.style.setProperty("--my", (y * 100).toFixed(1) + "%");
    card.style.setProperty("--ry", ((x - 0.5) * 16).toFixed(1) + "deg");
    card.style.setProperty("--rx", ((0.5 - y) * 12).toFixed(1) + "deg");
    card.classList.add("live");
  });
  card.addEventListener("pointerleave", () => {
    card.classList.remove("live");
    for (const k of ["--mx", "--my", "--rx", "--ry"]) card.style.removeProperty(k);
  });
  return card;
}

const clamp01 = (v) => Math.min(Math.max(v, 0), 1);

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
function intro(s, meta, tier) {
  const box = el("div", "pc-intro pi-" + tier.key);
  const ring = halo(meta, s.student_id, "pi-halo") ?? el("div", "pi-halo pi-ring");
  box.append(ring);
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
  const tier = TIERS[rank] ?? PLAIN;
  const outfits = ctx.siblings ? ctx.siblings(s.name) : [s];

  const scrim = el("div", "scrim");
  const stage = el("div", "pc-stage");
  const close = el("button", "ghost pc-close", "닫기");
  close.type = "button";

  // the card is laid out at one size and scaled to whatever window it is shown in
  const fit = () => stage.style.setProperty("--fit", Math.min(1, (innerWidth - 24) / CARD_W, (innerHeight - (outfits.length > 1 ? 150 : 100)) / CARD_H).toFixed(3));
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
  scrim.append(intro(s, meta, tier), stage);
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
