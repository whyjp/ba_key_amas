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
  mount(root, card("리더보드", el("p", "hint", "공개를 선택한 선생님만 보입니다. 이름을 누르면 그 선생님의 정보를 볼 수 있습니다."), holder));
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
      const link = el("a", "nick", e.nickname);
      link.href = "#/u/" + encodeURIComponent(e.nickname);
      link.title = "이 선생님의 정보 보기";
      const who = el("td", "l strong");
      who.append(link);
      tr.append(el("td", "", "#" + e.rank), who);
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
