import { renderBoard } from "./board.js";
import { renderMe } from "./me.js";
import { card, el, get, mount } from "./ui.js";

const app = document.getElementById("app");

function landing() {
  const tile = (href, title, text) => {
    const a = el("a", "entry");
    a.href = href;
    a.append(el("h2", "", title), el("p", "", text));
    return a;
  };
  const grid = el("div", "entries");
  grid.append(
    tile("#/me", "내 정보", "키를 입력하면 보유 학생, 스토리, 모모톡, 당번을 볼 수 있습니다."),
    tile("#/board", "리더보드", "공개를 선택한 선생님들의 순위입니다."),
  );
  const guide = card("업로드는 애드온에서", el("p", "hint", "스캔한 데이터는 ba-key-AMAS 애드온에서만 올릴 수 있습니다. 처음 올리면 닉네임#6자리 키를 받고, 그 키로 내 정보를 봅니다."));
  const link = el("a", "button", "애드온 받기");
  link.href = "https://github.com/whyjp/ba_key_amas/releases/latest";
  link.rel = "noopener noreferrer";
  link.target = "_blank";
  guide.append(link);
  get("/api/version").then((v) => { if (typeof v.download === "string" && v.download.startsWith("https://")) link.href = v.download; }).catch(() => {});
  mount(app, card(null, el("h1", "lead", "블루 아카이브 보유 학생 기록"), el("p", "hint", "ba-key-AMAS가 읽은 선생님의 데이터를 저장하고 다시 꺼내 봅니다.")), grid, guide);
}

const routes = { "": landing, me: renderMe, board: renderBoard };

function route() {
  const [name = "", arg] = location.hash.replace(/^#\/?/, "").split("/");
  for (const a of document.querySelectorAll("nav.top a")) a.toggleAttribute("aria-current", a.dataset.route === (name === "u" ? "board" : name));
  app.classList.toggle("fit", name === "me" || name === "u");
  if (name === "u" && arg) return void renderMe(app, decodeURIComponent(arg));
  (routes[name] ?? landing)(app);
}

addEventListener("hashchange", route);
route();
