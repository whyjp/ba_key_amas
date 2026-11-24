import { openDetail } from "./detail.js";
import { heatmapView } from "./heatmap.js";
import { armorColor, attackColor, bondBadge, halo, icon, loadMeta, name, ROLE, schoolLogo, stars } from "./domain.js";
import { card, el, empty, mount, num, outfitStack, portrait, post, session } from "./ui.js";

const NOT_SCANNED = "아직 스캔한 기록이 없습니다.";

/** What every tab needs: lookups built once per profile. */
function context(d) {
  const duty = new Set((d.progress?.duty?.students ?? []).filter((s) => s.selected === true).map((s) => s.name));
  const unread = new Map((d.progress?.momotalk?.unread ?? []).map((u) => [u.name, u.count]));
  const byName = new Map(d.students.map((s) => [s.name, s]));
  // place of each character in the overall ranking (the analysis lists them best first)
  const place = new Map(d.analysis.characters.map((c, i) => [c.base, i]));
  const rankOf = (studentName) => place.get(studentName.replace(/\s*\([^)]*\)\s*$/, "").trim() || studentName) ?? -1;
  const baseOf = (n) => n.replace(/\s*\([^)]*\)\s*$/, "").trim() || n;
  /** Every owned outfit of the same character, the plain one first. */
  const siblings = (n) => d.students.filter((s) => baseOf(s.name) === baseOf(n)).sort((a, b) => Number(a.name !== baseOf(a.name)) - Number(b.name !== baseOf(b.name)) || a.name.localeCompare(b.name, "ko"));
  return { duty, unread, byName, rankOf, siblings };
}

function tile(s, ctx, meta, showInfo) {
  const t = el("div", "tile");
  t.setAttribute("role", "button");
  t.tabIndex = 0;
  t.title = s.name;
  t.append(portrait(s.student_id, "tile-img"));
  if (showInfo) {
    if (s.stars) t.append(Object.assign(stars(s.stars, "student"), { className: "stars student tile-stars" }));
    if (s.level !== null) t.append(el("span", "lv", "Lv." + s.level));
    const ft = el("div", "ft");
    if (s.bond !== null) ft.append(bondBadge(s.bond, "bond sm"));
    if (s.uw_stars) ft.append(el("span", "chip uw", "전용 " + s.uw_stars + "성"));
    if (s.pinned) ft.append(el("span", "chip pin", "고정"));
    if (s.memorial) ft.append(el("span", "chip mem", "메모리얼"));
    if (ctx.duty.has(s.name)) ft.append(el("span", "chip duty", "당번"));
    t.append(ft);
  }
  t.append(el("div", "name", s.name));
  const open = () => openDetail(s, meta, ctx);
  t.addEventListener("click", open);
  t.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); open(); } });
  return t;
}

function summaryBar(d) {
  const a = d.analysis;
  const acc = d.account;
  const bar = el("div", "sumbar");
  const add = (v, label) => {
    const x = el("div");
    x.append(el("b", "", String(v)), el("span", "", label));
    bar.append(x);
  };
  add(a.stats.characters, "보유 캐릭터");
  add(d.students.length, "보유 학생");
  add(a.stats.bond30, "인연 30+");
  add(a.stats.memorial, "메모리얼 로비");
  add(a.stats.pinned, "고정");
  if (acc?.credits !== undefined) add(num(acc.credits), "크레딧");
  if (acc?.pyroxene !== undefined) add(num(acc.pyroxene), "청휘석");
  add(new Date(d.updated_at * 1000).toLocaleDateString("ko-KR"), "마지막 업로드");
  return bar;
}

/** The teacher's three favourites. Closed: portraits and the total. Hover or focus: the name and what it means. */
function favourites(d) {
  const row = el("div", "favs");
  d.analysis.top.forEach((c, i) => {
    const f = el("div", "fav");
    f.tabIndex = 0;
    f.append(el("span", "rank r" + (i + 1), String(i + 1)), outfitStack(c.outfits, "pt md"), el("b", "fav-score", num(c.total)));
    const tip = el("div", "fav-tip");
    tip.append(el("b", "", c.base), el("small", "", c.outfits.map((o) => o.name).join(" · ")), el("span", "", "선생님이 가장 아끼는 학생"));
    f.append(tip);
    row.append(f);
  });
  return row;
}

function studentsView(d, ctx, meta) {
  const root = el("div", "main-view");
  const top = el("div", "main-top");
  top.append(summaryBar(d));
  if (d.analysis.top.length) top.append(favourites(d));

  const bar = el("div", "bar");
  const filter = el("input");
  filter.type = "search";
  filter.placeholder = "학생 이름 검색";
  filter.maxLength = 20;
  const info = el("input");
  info.type = "checkbox";
  info.checked = true;
  const infoLabel = el("label", "check");
  infoLabel.append(info, el("span", "", "정보 표시"));
  const sort = el("select");
  for (const [v, l] of [["bond", "인연 랭크순"], ["level", "레벨순"], ["stars", "성급순"], ["name", "이름순"]]) sort.append(Object.assign(el("option", "", l), { value: v }));
  const count = el("span", "hint");
  bar.append(filter, sort, infoLabel, count);

  const grid = el("div", "tiles scroll");
  const draw = () => {
    const q = filter.value.trim();
    const key = sort.value;
    const list = d.students.filter((s) => !q || s.name.includes(q));
    list.sort((a, b) => (key === "name" ? a.name.localeCompare(b.name, "ko") : (b[key] ?? -1) - (a[key] ?? -1) || a.name.localeCompare(b.name, "ko")));
    count.textContent = list.length + "명";
    grid.replaceChildren(...list.map((s) => tile(s, ctx, meta, info.checked)));
  };
  filter.addEventListener("input", draw);
  sort.addEventListener("change", draw);
  info.addEventListener("change", draw);
  draw();
  root.append(top, bar, grid);
  return root;
}

const LIST_COLS = [
  ["name", "학생", "l"], ["school", "학교", "l"], ["level", "레벨"], ["stars", "성급"], ["uw", "전용 무기"],
  ["ex_skill", "EX 스킬"], ["normal_skill", "기본 스킬"], ["passive_skill", "강화 스킬"], ["sub_skill", "서브 스킬"],
  ["bond", "인연 랭크"], ["role", "전투 역할", "l"], ["attack", "공격 타입", "l"], ["armor", "방어 타입", "l"],
  ["pinned", "고정"], ["memorial", "메모리얼 로비"], ["duty", "당번"],
];

function sortValue(s, col, ctx) {
  if (col === "uw") return s.uw_stars ? s.uw_stars * 100 + (s.uw_level ?? 0) : null;
  if (col === "duty") return ctx.duty.has(s.name) ? 1 : 0;
  if (col === "pinned" || col === "memorial") return s[col] === null ? null : Number(s[col]);
  return s[col] ?? null;
}

function listView(d, ctx, meta) {
  const root = el("div", "main-view");
  const wrap = el("div", "listwrap scroll");
  let sort = { col: "bond", dir: -1 };
  const draw = () => {
    const rows = [...d.students].sort((a, b) => {
      const x = sortValue(a, sort.col, ctx), y = sortValue(b, sort.col, ctx);
      if (x === null && y === null) return a.name.localeCompare(b.name, "ko");
      if (x === null) return 1;
      if (y === null) return -1;
      const c = typeof x === "number" ? x - y : String(x).localeCompare(String(y), "ko");
      return c * sort.dir || a.name.localeCompare(b.name, "ko");
    });
    const head = el("tr");
    for (const [col, label, cls] of LIST_COLS) {
      const th = el("th", cls || "", label);
      th.tabIndex = 0;
      if (sort.col === col) th.setAttribute("aria-sort", sort.dir > 0 ? "ascending" : "descending");
      const pick = () => { sort = { col, dir: sort.col === col ? -sort.dir : cls === "l" ? 1 : -1 }; draw(); };
      th.addEventListener("click", pick);
      th.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); pick(); } });
      head.append(th);
    }
    const thead = el("thead");
    thead.append(head);
    const tb = el("tbody");
    for (const s of rows) {
      const tr = el("tr", "clickable");
      tr.tabIndex = 0;
      const open = () => openDetail(s, meta, ctx);
      tr.addEventListener("click", open);
      tr.addEventListener("keydown", (e) => { if (e.key === "Enter") open(); });
      const who = el("td", "l");
      const w = el("div", "who-cell");
      w.append(icon(s.student_id), halo(meta, s.student_id, "ico-halo") ?? "", el("span", "strong", s.name));
      who.append(w);
      const sc = el("td", "l");
      const sw = el("div", "who-cell");
      sw.append(schoolLogo(s.school), el("span", "", name.school(meta, s.school)));
      sc.append(sw);
      const uw = el("td");
      if (s.uw_stars) uw.append(stars(s.uw_stars, "weapon", 4), el("small", "", ` Lv.${s.uw_level ?? "-"}`));
      else uw.textContent = "-";
      const plain = (v) => el("td", v === null ? "null" : "", v === null ? "-" : String(v));
      const flag = (v) => el("td", v ? "yes" : "null", v ? "●" : "-");
      const typeCell = (text, color) => {
        const td = el("td", "l");
        td.append(Object.assign(el("span", "tag", text), {}));
        if (color) td.firstChild.style.setProperty("--tag", color);
        return td;
      };
      const starCell = el("td");
      if (s.stars) starCell.append(stars(s.stars, "student")); else starCell.textContent = "-";
      const bondCell = el("td");
      bondCell.append(bondBadge(s.bond, "bond sm"));
      tr.append(
        who, sc, plain(s.level), starCell, uw, plain(s.ex_skill), plain(s.normal_skill), plain(s.passive_skill), plain(s.sub_skill), bondCell,
        el("td", "l", s.role ? ROLE[s.role] ?? s.role : "-"),
        typeCell(name.attack(meta, s.attack), attackColor(s.attack)), typeCell(name.armor(meta, s.armor), armorColor(s.armor)),
        flag(s.pinned), flag(s.memorial), flag(ctx.duty.has(s.name)),
      );
      tb.append(tr);
    }
    const table = el("table", "rich");
    table.append(thead, tb);
    wrap.replaceChildren(table);
  };
  draw();
  root.append(wrap);
  return root;
}

function storyView(d) {
  const s = d.progress?.story;
  const groups = [["메인 스토리", s?.main], ["미니 스토리", s?.mini], ["그룹 스토리", s?.group]];
  const box = el("div", "col scroll");
  for (const [title, vols] of groups) {
    if (!vols || vols.length === 0) { box.append(card(title, empty(NOT_SCANNED))); continue; }
    const list = el("div", "vols");
    for (const v of vols) {
      const row = el("div", "vol");
      const head = el("div", "vol-head");
      head.append(el("b", "", v.volume), el("span", "state " + (v.cleared ? "ok" : "no"), v.cleared === null ? "미확인" : v.cleared ? "클리어" : "진행 중"));
      const chs = el("ul", "story-list");
      for (const c of v.chapters ?? []) {
        const li = el("li", c.cleared ? "ok" : "");
        li.append(el("i", "tick"), el("span", "", [c.chapter, c.title].filter(Boolean).join(" ")));
        chs.append(li);
      }
      row.append(head, chs);
      list.append(row);
    }
    box.append(card(title, list));
  }
  return box;
}

function eventView(d, meta) {
  const ev = d.progress?.events;
  if (!ev) return card("이벤트 스토리", empty(NOT_SCANNED));
  const names = meta.events ?? [];
  const past = ev.past ?? [];
  const list = el("ul", "story-list wide");
  for (const p of past) {
    const named = p.name ?? names[p.index]?.name ?? "이름을 알 수 없는 이벤트";
    const li = el("li", p.cleared ? "ok" : "");
    li.append(el("i", "tick"), el("span", "", named), el("em", "", p.cleared ? "클리어" : "미클리어"));
    list.append(li);
  }
  const cleared = past.filter((p) => p.cleared).length;
  const wrap = el("div", "col scroll");
  wrap.append(card(`이벤트 스토리 ${cleared} / ${past.length} 클리어`, list));
  return wrap;
}

function missionView(d) {
  const areas = d.progress?.missions?.areas ?? [];
  if (areas.length === 0) return card("임무", empty(NOT_SCANNED));
  const wrap = el("div", "col scroll");
  for (const a of areas) {
    const chips = el("div", "chips");
    for (const m of a.missions) {
      const c = el("span", "chip " + (m.cleared ? "uw" : ""), m.label + " ");
      c.append(stars(m.stars ?? 0, "student", 3));
      chips.append(c);
    }
    wrap.append(card(`${a.area ?? "?"}구역 ${a.mode === "Hard" ? "하드" : "노말"}`, chips));
  }
  return wrap;
}

function momoView(d, ctx) {
  const m = d.progress?.momotalk;
  if (!m) return card("모모톡", empty(NOT_SCANNED));
  const wrap = el("div", "col scroll");
  const head = el("div", "momo-head");
  head.append(el("b", "", num(m.unread_total)), el("span", "", "읽지 않은 메시지"));
  const list = el("div", "momo-list");
  for (const u of m.unread ?? []) {
    const s = ctx.byName.get(u.name);
    const row = el("div", "momo");
    const text = el("div", "momo-text");
    text.append(el("b", "", u.name), el("span", "", "새 메시지가 도착했어요"));
    row.append(portrait(s?.student_id, "momo-img"), text, bondBadge(s?.bond ?? null, "bond"), el("span", "unread", String(u.count)));
    list.append(row);
  }
  wrap.append(card("모모톡", head, (m.unread ?? []).length ? list : empty("읽지 않은 메시지가 있는 학생이 없습니다.")));
  return wrap;
}

function dutyView(d, ctx) {
  const du = d.progress?.duty;
  if (!du) return card("당번과 메모리얼 로비", empty(NOT_SCANNED));
  const wrap = el("div", "col scroll");
  const face = (n) => {
    const s = ctx.byName.get(n);
    const t = el("div", "face");
    t.append(portrait(s?.student_id, "face-img"), bondBadge(s?.bond ?? null, "bond sm face-bond"), el("span", "face-name", n));
    return t;
  };
  const picked = du.students.filter((s) => s.selected === true);
  const memorial = du.students.filter((s) => s.memorial_lobby === true);
  const pg = el("div", "faces");
  pg.append(...picked.map((s) => face(s.name)));
  const mg = el("div", "faces");
  mg.append(...memorial.map((s) => face(s.name)));
  wrap.append(
    card(`당번 ${du.selected_count ?? picked.length} / ${du.limit ?? 10}`, picked.length ? pg : empty("지정한 당번이 없습니다.")),
    card(`메모리얼 로비 해금 ${memorial.length}명`, memorial.length ? mg : empty("해금한 학생이 없습니다.")),
  );
  return wrap;
}

function scheduleView(d) {
  const sc = d.progress?.schedule;
  if (!sc) return card("스케줄", empty(NOT_SCANNED));
  const wrap = el("div", "col scroll");
  const hero = el("div", "sched-hero");
  hero.append(el("span", "", "스케줄 종합 랭크"), el("b", "", String(sc.total_rank ?? "-")));
  const grid = el("div", "sched-grid");
  for (const r of sc.regions ?? []) {
    const c = el("div", "sched");
    c.append(el("span", "sched-name", r.name), el("b", "sched-rank", r.rank === null ? "-" : `Rank ${r.rank}`));
    const bar = el("span", "meter sched-bar");
    for (let i = 1; i <= 9; i++) bar.append(el("i", i <= (r.rank ?? 0) ? "on" : ""));
    c.append(bar);
    grid.append(c);
  }
  wrap.append(hero, grid);
  return wrap;
}

const TABS = [
  ["students", "학생", (d, c, m) => studentsView(d, c, m)],
  ["heatmap", "학생 히트맵", (d, c, m) => heatmapView(d, c, m)],
  ["list", "학생 리스트", (d, c, m) => listView(d, c, m)],
  ["story", "스토리", (d) => storyView(d)],
  ["events", "이벤트 스토리", (d, c, m) => eventView(d, m)],
  ["missions", "임무", (d) => missionView(d)],
  ["momotalk", "모모톡", (d, c) => momoView(d, c)],
  ["duty", "당번 · 메모리얼", (d, c) => dutyView(d, c)],
  ["schedule", "스케줄", (d) => scheduleView(d)],
];

function teacherPanel(d, root) {
  const panel = el("div", "teacher");
  const level = d.progress?.account?.level;
  panel.append(el("small", "", "선생님"), el("b", "", `${d.nickname}${level ? ` (Lv.${level})` : ""}`));
  if (d.view === "owner") {
    const vis = el("button", "ghost wide", d.public ? "리더보드 공개 중" : "리더보드 비공개");
    vis.title = "누르면 공개 여부를 바꿉니다";
    vis.addEventListener("click", async () => {
      try {
        const r = await post("/api/visibility", { key: session.key, public: !d.public });
        d.public = r.public;
        vis.textContent = d.public ? "리더보드 공개 중" : "리더보드 비공개";
      } catch (e) { alert(e.message); }
    });
    const rekey = el("button", "ghost wide", "키 바꾸기");
    rekey.addEventListener("click", async () => {
      if (!confirm("키를 새로 만듭니다. 이전 키는 곧바로 쓸 수 없게 됩니다.")) return;
      try {
        const r = await post("/api/rekey", { key: session.key });
        session.key = r.key;
        prompt("새 키입니다. 지금 복사해 두세요.", r.key);
      } catch (e) { alert(e.message); }
    });
    const out = el("button", "ghost wide", "닫기");
    out.addEventListener("click", () => { session.key = null; session.data = null; renderMe(root); });
    panel.append(vis, rekey, out);
  } else {
    const back = el("a", "button wide", "리더보드로");
    back.href = "#/board";
    panel.append(back);
  }
  return panel;
}

async function board(root, d) {
  const meta = await loadMeta();
  const ctx = context(d);
  const side = el("nav", "side");
  side.setAttribute("aria-label", "정보 보기 탭");
  const body = el("div", "tabbody");
  const show = (id) => {
    for (const b of side.querySelectorAll("button.tab")) b.setAttribute("aria-selected", String(b.dataset.tab === id));
    body.replaceChildren(TABS.find((t) => t[0] === id)[2](d, ctx, meta));
  };
  const tabs = el("div", "tabs");
  for (const [id, label] of TABS) {
    const b = el("button", "tab", label);
    b.dataset.tab = id;
    b.setAttribute("role", "tab");
    b.addEventListener("click", () => show(id));
    tabs.append(b);
  }
  side.append(tabs, teacherPanel(d, root));
  const layout = el("div", "layout fit");
  layout.append(side, body);
  mount(root, layout);
  show("students");
}

export async function renderMe(root, publicNickname) {
  if (publicNickname) {
    try {
      const d = await fetch("/api/public?nickname=" + encodeURIComponent(publicNickname)).then(async (r) => {
        const out = await r.json();
        if (!r.ok) throw new Error(out.error || "불러오지 못했습니다");
        return out;
      });
      return board(root, d);
    } catch (e) {
      return mount(root, card("조회할 수 없습니다", el("p", "msg", e.message)));
    }
  }
  if (session.data) return board(root, session.data);
  const form = el("form");
  const key = el("input");
  key.type = "text";
  key.maxLength = 40;
  key.placeholder = "닉네임#a1b2c3";
  key.spellcheck = false;
  key.required = true;
  key.autocomplete = "off";
  const go = el("button", "", "조회");
  go.type = "submit";
  const msg = el("p", "msg");
  msg.setAttribute("role", "alert");
  form.append(key, go);
  const gate = card("키로 조회", el("p", "hint", "애드온에서 업로드할 때 받은 닉네임#6자리 키를 입력하세요. 닉네임만으로는 조회할 수 없습니다."), form, msg);
  gate.classList.add("gate");
  mount(root, gate);
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    msg.textContent = "";
    try {
      const k = key.value.trim();
      session.data = await post("/api/profile", { key: k });
      session.key = k;
      renderMe(root);
    } catch (err) { msg.textContent = err.message; }
  });
}
