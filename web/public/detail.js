// The detail sheet of one owned student: opens over the tiles, closes with its button, Esc or a click outside.
import { armorColor, attackColor, bondBadge, meter, name, ROLE, schoolLogo, stars } from "./domain.js";
import { el, portrait } from "./ui.js";

function tag(text, color) {
  const t = el("span", "tag", text);
  if (color) t.style.setProperty("--tag", color);
  return t;
}

function skillRow(label, value, max) {
  const row = el("div", "skill");
  row.append(el("span", "skill-name", label), meter(value ?? 0, max), el("b", "", value === null ? "-" : `Lv.${value}`));
  return row;
}

export function openDetail(s, meta, ctx) {
  const prev = document.activeElement;
  const scrim = el("div", "scrim");
  const sheet = el("div", "sheet");
  sheet.setAttribute("role", "dialog");
  sheet.setAttribute("aria-modal", "true");
  sheet.setAttribute("aria-label", s.name);

  const art = el("div", "sheet-art");
  art.append(portrait(s.student_id, "sheet-img"));
  const lv = el("div", "sheet-lv");
  lv.append(el("small", "", "Lv."), el("b", "", s.level === null ? "-" : String(s.level)));
  art.append(lv);

  const info = el("div", "sheet-info");
  const head = el("div", "sheet-head");
  const title = el("div");
  title.append(el("h2", "", s.name));
  const school = el("div", "school-line");
  school.append(schoolLogo(s.school), el("span", "", name.school(meta, s.school)));
  title.append(school);
  const close = el("button", "ghost x", "닫기");
  head.append(title, close);

  const tags = el("div", "tags");
  if (s.role) tags.append(tag(ROLE[s.role] ?? s.role));
  if (s.attack) tags.append(tag("공격 " + name.attack(meta, s.attack), attackColor(s.attack)));
  if (s.armor) tags.append(tag("방어 " + name.armor(meta, s.armor), armorColor(s.armor)));

  const badges = el("div", "tags");
  if (s.pinned) badges.append(tag("고정", "#c8a020"));
  if (s.memorial) badges.append(tag("메모리얼 로비 해금", "#d65aaa"));
  if (ctx.duty.has(s.name)) badges.append(tag("당번", "#2c6fd0"));
  const unread = ctx.unread.get(s.name);
  if (unread) badges.append(tag(`안 읽은 모모톡 ${unread}`, "#2f9a6a"));

  const grid = el("div", "sheet-grid");
  const box = (label, ...kids) => {
    const b = el("div", "stat-box");
    b.append(el("span", "stat-label", label), ...kids);
    return b;
  };
  grid.append(
    box("성급", s.stars ? stars(s.stars, "student") : el("b", "", "-")),
    box("인연 랭크", bondBadge(s.bond, "bond lg")),
    box("전용 무기", s.uw_stars ? stars(s.uw_stars, "weapon", 4) : el("b", "", "없음"), el("small", "", s.uw_level ? `Lv.${s.uw_level}` : "")),
  );

  const skills = el("div", "skills");
  skills.append(skillRow("EX 스킬", s.ex_skill, 5), skillRow("기본 스킬", s.normal_skill, 10), skillRow("강화 스킬", s.passive_skill, 10), skillRow("서브 스킬", s.sub_skill, 10));

  info.append(head, tags, badges, grid, skills);
  sheet.append(art, info);
  scrim.append(sheet);
  document.body.append(scrim);

  const shut = () => {
    document.removeEventListener("keydown", onKey);
    scrim.remove();
    prev?.focus?.();
  };
  const onKey = (e) => { if (e.key === "Escape") shut(); };
  document.addEventListener("keydown", onKey);
  scrim.addEventListener("click", (e) => { if (e.target === scrim) shut(); });
  close.addEventListener("click", shut);
  close.focus();
}
