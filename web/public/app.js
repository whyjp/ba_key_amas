import { renderBoard } from "./board.js";
import { renderMe } from "./me.js";
import { renderUpload } from "./upload.js";
import { card, el, mount } from "./ui.js";

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
    tile("#/upload", "업로드", "스캔한 학생 데이터를 올리고 내 키를 받습니다. 애드온에서 바로 올릴 수도 있습니다."),
    tile("#/me", "내 정보", "키를 입력하면 보유 학생, 대표 3인, 스토리, 모모톡, 당번을 볼 수 있습니다."),
    tile("#/board", "리더보드", "인연, 고정, 메모리얼 로비, 육성을 합산한 유저 순위입니다."),
  );
  mount(app, card(null, el("h1", "lead", "블루 아카이브 보유 학생 기록"), el("p", "hint", "ba-key-AMAS 스캐너가 읽은 데이터를 저장하고 다시 꺼내 봅니다.")), grid);
}

const routes = { "": landing, upload: renderUpload, me: renderMe, board: renderBoard };

function route() {
  const name = location.hash.replace(/^#\/?/, "").split("/")[0] ?? "";
  for (const a of document.querySelectorAll("nav.top a")) a.toggleAttribute("aria-current", a.dataset.route === name);
  (routes[name] ?? landing)(app);
}

addEventListener("hashchange", route);
route();
