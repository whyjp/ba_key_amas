import { renderBoard } from "./board.js";
import { renderProfile } from "./me.js";
import { card, el, get, mount, post, session } from "./ui.js";

const app = document.getElementById("app");

/** A search is a nickname (public teachers) or a key `nickname#a1b2c3` (opens that teacher whether public or not). */
async function search(text) {
  const value = text.trim();
  if (!value) return;
  if (/#[0-9a-fA-F]{6}$/.test(value)) {
    try {
      session.data = await post("/api/profile", { key: value });
      session.key = value;
      if (location.hash === "#/p") route();
      else location.hash = "#/p";
    } catch (err) {
      notFound(err.message);
    }
    return;
  }
  session.data = null;
  const target = "#/u/" + encodeURIComponent(value);
  if (location.hash === target) route();
  else location.hash = target;
}

function notFound(message) {
  mount(app, card("찾을 수 없습니다", el("p", "msg", message), el("p", "hint", "공개를 선택한 선생님은 닉네임으로 검색합니다. 비공개인 선생님은 닉네임#6자리 키로 검색합니다.")));
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
  strip.append(text, actions);
  const board = el("div");
  mount(app, strip, board);
  renderBoard(board);
}

function route() {
  const [name = "", arg] = location.hash.replace(/^#\/?/, "").split("/");
  const profile = name === "u" || name === "p";
  for (const a of document.querySelectorAll("nav.top a")) a.toggleAttribute("aria-current", !profile);
  app.classList.toggle("fit", profile);
  if (name === "u" && arg) {
    const nick = decodeURIComponent(arg);
    return void renderProfile(app, nick, notFound);
  }
  if (name === "p" && session.data) {
    return void renderProfile(app, null, notFound);
  }
  main();
}

addEventListener("hashchange", route);
route();
