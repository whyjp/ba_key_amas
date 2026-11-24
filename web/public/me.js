import { card, el, empty, mount, num, outfitStack, portrait, post, session } from "./ui.js";

const ROLE = { STRIKER: "스트라이커", SPECIAL: "스페셜" };
const COLS = [
  ["name", "학생", "l"], ["level", "Lv"], ["stars", "★"], ["uw_stars", "고유★"], ["uw_level", "고유Lv"],
  ["ex_skill", "EX"], ["normal_skill", "기본"], ["passive_skill", "강화"], ["sub_skill", "서브"],
  ["bond", "인연"], ["pinned", "고정"], ["memorial", "메모리얼"], ["duty", "당번"],
  ["role", "역할", "l"], ["school", "학교", "l"], ["attack", "공격", "l"], ["armor", "방어", "l"],
];

function dutySet(d) {
  return new Set((d.progress?.duty?.students ?? []).filter((s) => s.selected === true).map((s) => s.name));
}

function cell(s, col, duty) {
  if (col === "duty") return duty.has(s.name) ? "예" : "";
  const v = s[col];
  if (v === null || v === undefined) return null;
  if (col === "role") return ROLE[v] || v;
  if (col === "pinned" || col === "memorial") return v ? "예" : "";
  return v;
}

function sortable(onPick, sort, col, cls, label) {
  const th = el("th", cls || "", label);
  th.tabIndex = 0;
  if (sort.col === col) th.setAttribute("aria-sort", sort.dir > 0 ? "ascending" : "descending");
  const pick = () => onPick(col, cls === "l");
  th.addEventListener("click", pick);
  th.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); pick(); } });
  return th;
}

function studentTab(d, mode) {
  const duty = dutySet(d);
  const box = el("div");
  const filter = el("input");
  filter.type = "search";
  filter.placeholder = "학생 이름 검색";
  filter.maxLength = 20;
  const count = el("span", "hint");
  const bar = el("div", "bar");
  bar.append(filter, count);
  const out = el("div");
  box.append(bar, out);
  let sort = { col: "bond", dir: -1 };

  const rows = () => {
    const q = filter.value.trim();
    const list = d.students.filter((s) => !q || s.name.includes(q));
    list.sort((a, b) => {
      const x = cell(a, sort.col, duty), y = cell(b, sort.col, duty);
      const nx = x === null || x === "", ny = y === null || y === "";
      if (nx && ny) return a.name.localeCompare(b.name, "ko");
      if (nx) return 1;
      if (ny) return -1;
      const c = typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y), "ko");
      return c * sort.dir || a.name.localeCompare(b.name, "ko");
    });
    return list;
  };

  const tile = (s) => {
    const t = el("div", "tile");
    t.append(portrait(s.student_id, "tile-img"));
    if (s.level !== null) t.append(el("span", "lv", "Lv." + s.level));
    if (s.stars !== null) t.append(el("span", "st", "★" + s.stars));
    const ft = el("div", "ft");
    if (s.uw_stars !== null) ft.append(el("span", "chip uw", "고유 ★" + s.uw_stars));
    if (s.ex_skill !== null) ft.append(el("span", "chip", "EX" + s.ex_skill));
    if (s.bond !== null) ft.append(el("span", "chip", "인연" + s.bond));
    if (s.pinned) ft.append(el("span", "chip pin", "고정"));
    if (s.memorial) ft.append(el("span", "chip mem", "메모리얼"));
    if (duty.has(s.name)) ft.append(el("span", "chip duty", "당번"));
    t.append(ft, el("div", "name", s.name));
    return t;
  };

  const draw = () => {
    const list = rows();
    count.textContent = list.length + "명";
    if (mode === "tile") {
      const g = el("div", "tiles");
      g.append(...list.map(tile));
      out.replaceChildren(g);
      return;
    }
    const head = el("tr");
    for (const [col, label, cls] of COLS) {
      head.append(sortable((c, asc) => { sort = { col: c, dir: sort.col === c ? -sort.dir : asc ? 1 : -1 }; draw(); }, sort, col, cls, label));
    }
    const thead = el("thead");
    thead.append(head);
    const tb = el("tbody");
    for (const s of list) {
      const tr = el("tr");
      for (const [col, , cls] of COLS) {
        const v = cell(s, col, duty);
        tr.append(el("td", (cls || "") + (v === null ? " null" : ""), v === null ? "-" : String(v)));
      }
      tb.append(tr);
    }
    const table = el("table");
    table.append(thead, tb);
    const wrap = el("div", "listwrap");
    wrap.append(table);
    out.replaceChildren(wrap);
  };
  filter.addEventListener("input", draw);
  draw();
  return box;
}

function summaryTab(d) {
  const a = d.analysis;
  const grid = el("div", "summary");
  const add = (v, label) => { const x = el("div"); x.append(el("b", "", String(v)), el("span", "", label)); grid.append(x); };
  const acc = d.account || {};
  add(d.progress?.account?.level ?? "-", "계정 레벨");
  add(a.stats.characters, "보유 캐릭터");
  add(d.students.length, "보유 학생(의상 포함)");
  add(num(a.score), "리더보드 점수");
  if (acc.credits !== undefined) add(num(acc.credits), "크레딧");
  if (acc.pyroxene !== undefined) add(num(acc.pyroxene), "청휘석");
  if (acc.ap) add(acc.ap, "AP");
  add(d.progress?.account?.circle ?? "-", "서클");
  add(new Date(d.updated_at * 1000).toLocaleString("ko-KR"), "마지막 업로드");
  const stat = el("div", "summary");
  for (const [v, l] of [[a.stats.bond60, "인연 60+"], [a.stats.bond30, "인연 30+"], [a.stats.levelHigh, "Lv 80+"], [a.stats.pinned, "고정"], [a.stats.memorial, "메모리얼 로비"]]) {
    const x = el("div");
    x.append(el("b", "", String(v)), el("span", "", l));
    stat.append(x);
  }
  return card("요약", grid, el("h3", "", "리더보드 기준 수치"), stat, visibility(d));
}

function visibility(d) {
  const box = el("div", "vis");
  const status = el("span", "hint", d.public ? "리더보드에 공개 중입니다." : "리더보드에 공개하지 않았습니다.");
  const btn = el("button", "ghost", d.public ? "비공개로 바꾸기" : "공개하기");
  btn.addEventListener("click", async () => {
    try {
      const r = await post("/api/visibility", { key: session.key, public: !d.public });
      d.public = r.public;
      status.textContent = d.public ? "리더보드에 공개 중입니다." : "리더보드에 공개하지 않았습니다.";
      btn.textContent = d.public ? "비공개로 바꾸기" : "공개하기";
    } catch (e) { status.textContent = e.message; }
  });
  box.append(status, btn);
  return box;
}

function topTab(d) {
  const top = d.analysis.top;
  if (top.length === 0) return card("대표 3인", empty("보유 학생이 없습니다."));
  const list = el("ol", "top3");
  top.forEach((c, i) => {
    const li = el("li", "top3-item");
    const info = el("div", "top3-info");
    info.append(el("h3", "", `${i + 1}위 ${c.base}`), el("p", "hint", c.outfits.map((o) => o.name).join(" · ")));
    const parts = el("div", "chips");
    for (const [label, v] of [["인연 " + c.bond, c.bondPoints], ["고정", c.pinPoints], ["메모리얼", c.memorialPoints], ["육성", Math.round(c.growthPoints * 0.6)]]) {
      if (v > 0) parts.append(el("span", "chip", `${label} ${Math.round(v)}점`));
    }
    info.append(parts, el("p", "strong", `합계 ${num(c.total)}점`));
    li.append(outfitStack(c.outfits, "pt lg"), info);
    list.append(li);
  });
  return card("대표 3인", el("p", "hint", "인연, 고정, 메모리얼 로비에 더 큰 점수를 주고 육성은 그보다 작게 칩니다. 의상이 다른 같은 학생은 한 줄로 합칩니다."), list);
}

function storyTab(d) {
  const s = d.progress?.story;
  const groups = [["메인 스토리", s?.main], ["미니 스토리", s?.mini], ["그룹 스토리", s?.group]];
  const wrap = [];
  for (const [title, vols] of groups) {
    if (!vols || vols.length === 0) { wrap.push(card(title, empty("아직 스캔한 기록이 없습니다."))); continue; }
    const list = el("div", "vols");
    for (const v of vols) {
      const row = el("div", "vol");
      row.append(el("b", "", v.volume), el("span", v.cleared ? "ok" : "no", v.cleared === null ? "미확인" : v.cleared ? "클리어" : "진행 중"));
      const chs = el("div", "chips");
      for (const c of v.chapters ?? []) chs.append(el("span", "chip " + (c.cleared ? "uw" : ""), c.chapter));
      row.append(chs);
      list.append(row);
    }
    wrap.push(card(title, list));
  }
  return fragmentOf(wrap);
}

function fragmentOf(nodes) {
  const box = el("div", "col");
  box.append(...nodes);
  return box;
}

function momoTab(d) {
  const m = d.progress?.momotalk;
  if (!m) return card("모모톡", empty("아직 스캔한 기록이 없습니다."));
  const box = [];
  const head = el("div", "summary");
  const x = el("div");
  x.append(el("b", "", num(m.unread_total)), el("span", "", "안 읽은 메시지"));
  head.append(x);
  box.push(head);
  if ((m.unread ?? []).length === 0) box.push(empty("안 읽은 메시지가 있는 학생이 없습니다."));
  else {
    const ul = el("div", "chips");
    for (const u of m.unread) ul.append(el("span", "chip", `${u.name} ${u.count}`));
    box.push(ul);
  }
  return card("모모톡", ...box);
}

function dutyTab(d) {
  const du = d.progress?.duty;
  if (!du) return card("당번과 메모리얼 로비", empty("아직 스캔한 기록이 없습니다."));
  const byName = new Map(d.students.map((s) => [s.name, s]));
  const picked = du.students.filter((s) => s.selected === true);
  const memorial = du.students.filter((s) => s.memorial_lobby === true);
  const pickedBox = el("div", "tiles small");
  for (const s of picked) {
    const t = el("div", "tile");
    t.append(portrait(byName.get(s.name)?.student_id, "tile-img"), el("div", "name", s.name));
    pickedBox.append(t);
  }
  const memBox = el("div", "chips");
  for (const s of memorial) memBox.append(el("span", "chip mem", s.name));
  return fragmentOf([
    card(`당번 ${du.selected_count ?? picked.length}/${du.limit ?? 10}`, picked.length ? pickedBox : empty("지정한 당번이 없습니다.")),
    card(`메모리얼 로비 해금 ${memorial.length}명`, el("p", "hint", `당번 메뉴를 읽은 ${du.students.length}명 중 해금 아이콘이 켜진 학생입니다.`), memBox),
  ]);
}

function eventTab(d) {
  const ev = d.progress?.events;
  if (!ev) return card("이벤트", empty("아직 스캔한 기록이 없습니다."));
  const grid = el("div", "chips");
  for (const p of ev.past ?? []) grid.append(el("span", "chip " + (p.cleared ? "uw" : ""), String(p.index + 1)));
  const cleared = (ev.past ?? []).filter((p) => p.cleared).length;
  return card(`지난 이벤트 스토리 ${cleared}/${(ev.past ?? []).length}`, el("p", "hint", "번호는 이벤트 일람에서 오래된 순서입니다. 초록색이 클리어한 이벤트입니다."), grid);
}

function scheduleTab(d) {
  const sc = d.progress?.schedule;
  if (!sc) return card("스케줄", empty("아직 스캔한 기록이 없습니다."));
  const tb = el("tbody");
  for (const r of sc.regions ?? []) {
    const tr = el("tr");
    tr.append(el("td", "l", r.name), el("td", "", r.rank === null ? "-" : String(r.rank)));
    tb.append(tr);
  }
  const head = el("tr");
  head.append(el("th", "l", "지역"), el("th", "", "랭크"));
  const thead = el("thead");
  thead.append(head);
  const table = el("table");
  table.append(thead, tb);
  const wrap = el("div", "listwrap");
  wrap.append(table);
  return card(`스케줄 (총 랭크 ${sc.total_rank ?? "-"})`, wrap);
}

function missionTab(d) {
  const areas = d.progress?.missions?.areas ?? [];
  if (areas.length === 0) return card("임무", empty("아직 스캔한 기록이 없습니다."));
  const nodes = areas.map((a) => {
    const chips = el("div", "chips");
    for (const m of a.missions) chips.append(el("span", "chip " + (m.cleared ? "uw" : ""), `${m.label} ${"★".repeat(m.stars ?? 0)}`));
    return card(`${a.area ?? "?"}구역 ${a.mode === "Hard" ? "하드" : "노말"}`, chips);
  });
  return fragmentOf(nodes);
}

const TABS = [
  ["summary", "요약", summaryTab],
  ["top", "대표 3인", topTab],
  ["tile", "학생 타일", (d) => card("보유 학생", studentTab(d, "tile"))],
  ["list", "학생 리스트", (d) => card("보유 학생", studentTab(d, "list"))],
  ["story", "스토리", storyTab],
  ["events", "이벤트", eventTab],
  ["missions", "임무", missionTab],
  ["momotalk", "모모톡", momoTab],
  ["duty", "당번 · 메모리얼", dutyTab],
  ["schedule", "스케줄", scheduleTab],
];

function board(root, d) {
  const side = el("nav", "side");
  side.setAttribute("aria-label", "내 정보 탭");
  const body = el("div", "tabbody");
  const show = (id) => {
    for (const b of side.querySelectorAll("button")) b.setAttribute("aria-selected", String(b.dataset.tab === id));
    const node = TABS.find((t) => t[0] === id)[2](d);
    body.replaceChildren(node);
  };
  for (const [id, label] of TABS) {
    const b = el("button", "tab", label);
    b.dataset.tab = id;
    b.setAttribute("role", "tab");
    b.addEventListener("click", () => show(id));
    side.append(b);
  }
  const out = el("button", "ghost wide", "닫기");
  out.addEventListener("click", () => { session.key = null; session.data = null; renderMe(root); });
  const rekey = el("button", "ghost wide", "키 바꾸기");
  rekey.addEventListener("click", async () => {
    if (!confirm("키를 새로 만듭니다. 이전 키는 곧바로 쓸 수 없게 됩니다.")) return;
    try {
      const r = await post("/api/rekey", { key: session.key });
      session.key = r.key;
      prompt("새 키입니다. 지금 복사해 두세요.", r.key);
    } catch (e) { alert(e.message); }
  });
  side.append(rekey, out);
  const layout = el("div", "layout");
  layout.append(side, body);
  mount(root, layout);
  show("summary");
}

export async function renderMe(root) {
  const who = document.getElementById("who");
  if (session.data) {
    who.textContent = session.data.nickname;
    who.hidden = false;
    return board(root, session.data);
  }
  who.hidden = true;
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
  const gate = card("키로 조회", el("p", "hint", "업로드할 때 받은 닉네임#6자리 키를 입력하세요. 닉네임만으로는 조회할 수 없습니다."), form, msg);
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
