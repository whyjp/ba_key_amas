import { renderBoard } from "./board.js";
import { renderMe } from "./me.js";
import { card, el, get, mount } from "./ui.js";

const app = document.getElementById("app");

/** The main page: the leaderboard, with a short way into your own page and to the add-on. */
function main() {
  const strip = card(null);
  strip.classList.add("lead-strip");
  const text = el("div", "lead-text");
  text.append(el("h1", "lead", "블루 아카이브 선생님 리더보드"), el("p", "hint", "ba-key-AMAS가 읽은 보유 학생 기록입니다. 공개를 선택한 선생님만 보입니다."));
  const mine = el("a", "button", "내 정보 보기");
  mine.href = "#/me";
  const addon = el("a", "button ghost-link", "애드온 받기");
  addon.href = "https://github.com/whyjp/ba_key_amas/releases/latest";
  addon.rel = "noopener noreferrer";
  addon.target = "_blank";
  get("/api/version").then((v) => { if (typeof v.download === "string" && v.download.startsWith("https://")) addon.href = v.download; }).catch(() => {});
  const actions = el("div", "lead-actions");
  actions.append(mine, addon);
  strip.append(text, actions);
  const board = el("div");
  mount(app, strip, board);
  renderBoard(board);
}

const routes = { "": main, board: main, me: renderMe };

function route() {
  const [name = "", arg] = location.hash.replace(/^#\/?/, "").split("/");
  const home = name === "" || name === "board" || name === "u";
  for (const a of document.querySelectorAll("nav.top a")) a.toggleAttribute("aria-current", home ? a.dataset.route === "" : a.dataset.route === name);
  app.classList.toggle("fit", name === "me" || name === "u");
  if (name === "u" && arg) return void renderMe(app, decodeURIComponent(arg));
  (routes[name] ?? main)(app);
}

addEventListener("hashchange", route);
route();
