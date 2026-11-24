import { card, el, empty, get, mount, num, outfitStack } from "./ui.js";

const COLS = [
  ["rank", "순위"], ["nickname", "유저"], ["top", "대표 3인", "nosort"], ["score", "점수"],
  ["bond60", "인연 60+"], ["bond30", "인연 30+"], ["levelHigh", "Lv 80+"], ["pinned", "고정"], ["memorial", "메모리얼"], ["characters", "보유"],
];

function value(e, col) {
  if (col === "rank") return e.rank;
  if (col === "nickname") return e.nickname;
  if (col === "score") return e.score;
  return e.stats ? e.stats[col] : null;
}

export async function renderBoard(root) {
  const holder = el("div");
  mount(root, card("리더보드", el("p", "hint", "공개를 선택한 유저만 보입니다. 점수는 의상을 이름 하나로 합쳐 계산합니다."), holder, explain()));
  let rows;
  try {
    rows = (await get("/api/leaderboard")).entries;
  } catch (err) {
    holder.append(el("p", "msg", err.message));
    return;
  }
  if (rows.length === 0) {
    holder.append(empty("아직 공개한 유저가 없습니다."));
    return;
  }
  let sort = { col: "score", dir: -1 };
  const draw = () => {
    const sorted = [...rows].sort((a, b) => {
      const x = value(a, sort.col), y = value(b, sort.col);
      const c = typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y), "ko");
      return c * sort.dir || a.rank - b.rank;
    });
    const table = el("table", "board");
    const head = el("tr");
    for (const [col, label, flag] of COLS) {
      const th = el("th", col === "nickname" || col === "top" ? "l" : "", label);
      if (!flag) {
        th.tabIndex = 0;
        if (sort.col === col) th.setAttribute("aria-sort", sort.dir > 0 ? "ascending" : "descending");
        const pick = () => { sort = { col, dir: sort.col === col ? -sort.dir : col === "nickname" ? 1 : -1 }; draw(); };
        th.addEventListener("click", pick);
        th.addEventListener("keydown", (ev) => { if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); pick(); } });
      }
      head.append(th);
    }
    const thead = el("thead");
    thead.append(head);
    const body = el("tbody");
    for (const e of sorted) {
      const tr = el("tr");
      tr.append(el("td", "", "#" + e.rank), el("td", "l strong", e.nickname));
      const top = el("td", "l");
      const tops = el("div", "tops");
      for (const c of e.top) {
        const one = el("div", "top1");
        one.append(outfitStack(c.outfits, "pt sm"), el("small", "", c.base));
        tops.append(one);
      }
      top.append(tops);
      tr.append(top, el("td", "strong", num(e.score)));
      for (const col of ["bond60", "bond30", "levelHigh", "pinned", "memorial", "characters"]) tr.append(el("td", "", num(e.stats?.[col])));
      body.append(tr);
    }
    table.append(thead, body);
    const wrap = el("div", "listwrap");
    wrap.append(table);
    holder.replaceChildren(wrap);
  };
  draw();
}

function explain() {
  const d = el("details", "how");
  d.append(el("summary", "", "점수는 이렇게 계산합니다"));
  const ul = el("ul");
  for (const t of [
    "캐릭터 하나는 이름에서 (의상)을 뺀 이름입니다. 리오와 리오(무장)은 한 캐릭터로 점수를 더합니다.",
    "인연 랭크는 30까지 한 랭크에 2점, 31부터 59까지 3점입니다. 60에 닿으면 100점을 더 받고 그 위는 한 랭크에 6점입니다.",
    "고정 40점, 메모리얼 로비 해금 50점입니다.",
    "육성은 레벨, 성급, 스킬, 고유무기를 합산하고 0.6배로 칩니다. 고유무기는 가진 것만으로 30점에 성급과 레벨 점수가 붙어 가장 큽니다.",
    "인연, 고정, 메모리얼이 육성보다 크게 반영됩니다. 던전, 전투, 레이드는 아직 점수에 넣지 않습니다.",
    "점수 기준은 노력의 양을 가늠한 값이며 게임이 정한 공식이 아닙니다.",
  ]) ul.append(el("li", "", t));
  d.append(ul);
  return d;
}
