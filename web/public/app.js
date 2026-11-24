import { renderBoard } from "./board.js";
import { renderProfile } from "./me.js";
import { card, el, get, mount, num } from "./ui.js";

const app = document.getElementById("app");

/** A search looks among the public teachers by nickname. One match opens it; several share the nickname, so they are listed. */
async function search(text) {
  const value = text.trim();
  if (!value) return;
  try {
    const { matches } = await get("/api/search?q=" + encodeURIComponent(value));
    if (matches.length === 0) return notFound("그 닉네임으로 공개한 선생님이 없습니다.");
    if (matches.length === 1) {
      location.hash = "#/u/" + matches[0].public_id;
      return;
    }
    candidates(value, matches);
  } catch (err) {
    notFound(err.message);
  }
}

function candidates(value, matches) {
  const table = el("table", "board");
  const head = el("tr");
  for (const [label, cls] of [["유저", "l"], ["점수", ""], ["마지막 업로드", ""]]) head.append(el("th", cls, label));
  const thead = el("thead");
  thead.append(head);
  const body = el("tbody");
  for (const m of matches) {
    const tr = el("tr", "clickable");
    const link = el("a", "nick", m.nickname);
    link.href = "#/u/" + m.public_id;
    const who = el("td", "l strong");
    who.append(link);
    tr.append(who, el("td", "strong", num(m.score)), el("td", "", new Date(m.updated_at * 1000).toLocaleDateString("ko-KR")));
    tr.addEventListener("click", () => { location.hash = link.getAttribute("href"); });
    body.append(tr);
  }
  table.append(thead, body);
  const wrap = el("div", "listwrap");
  wrap.append(table);
  const back = el("a", "button wide", "리더보드로");
  back.href = "#/";
  mount(app, card(`"${value}" 로 시작하는 선생님 ${matches.length}명`, el("p", "hint", "같은 닉네임을 쓰는 선생님이 여러 명입니다. 점수와 마지막 업로드로 골라 주세요."), wrap, back));
}

function notFound(message) {
  mount(app, card("찾을 수 없습니다", el("p", "msg", message), el("p", "hint", "공개를 선택한 선생님만 닉네임으로 검색됩니다. 공개하지 않은 선생님은 애드온이 알려 주는 링크로만 열 수 있습니다.")));
}

/** The main page: the leaderboard, with a search box and a way to the add-on. */
function main() {
  const strip = card(null);
  strip.classList.add("lead-strip");
  const text = el("div", "lead-text");
  text.append(el("p", "hint", "선생님의 닉네임을 검색하거나, 아래 순위에서 골라 보유 학생과 진행 상태를 볼 수 있습니다."));
  const form = el("form", "lead-search");
  const input = el("input");
  input.type = "search";
  input.maxLength = 40;
  input.placeholder = "선생님 닉네임 검색";
  input.setAttribute("aria-label", "선생님 닉네임 검색");
  const go = el("button", "", "검색");
  go.type = "submit";
  form.append(input, go);
  form.addEventListener("submit", (e) => { e.preventDefault(); search(input.value); });
  const addon = el("a", "button ghost-link", "애드온 받기");
  addon.href = "https://github.com/whyjp/ba_key_amas/releases/latest";
  addon.rel = "noopener noreferrer";
  addon.target = "_blank";
  get("/api/version").then((v) => { if (typeof v.download === "string" && v.download.startsWith("https://")) addon.href = v.download; }).catch(() => {});
  const actions = el("div", "lead-actions");
  actions.append(form, addon);
  // shown beside the add-on link: what the member code is used for, as the add-on says it too
  const note = el("p", "hint addon-note", "업로드할 때 애드온이 회원코드(게임 메뉴 모음 > 계정 > 고객센터 > 회원코드)를 한 번 물어봅니다. 입력한 회원코드는 이 PC에서 변환(해시)된 값으로만 서버에 올라가며, 원본은 저장되거나 전송되지 않습니다. 서버는 그 값을 다시 비밀 값으로 변환해 저장합니다.");
  strip.append(text, actions, note);
  const board = el("div");
  mount(app, strip, board);
  renderBoard(board);
}

function route() {
  const [name = "", arg] = location.hash.replace(/^#\/?/, "").split("/");
  const profile = name === "u" && Boolean(arg);
  for (const a of document.querySelectorAll("nav.top a")) a.toggleAttribute("aria-current", !profile);
  app.classList.toggle("fit", profile);
  if (name === "u" && arg) return void renderProfile(app, arg, notFound);
  main();
}

addEventListener("hashchange", route);
route();
