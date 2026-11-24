import { card, el, mount, post, session } from "./ui.js";

async function readJson(file) {
  return JSON.parse(await file.text());
}

export function renderUpload(root) {
  const form = el("form", "stack-form");
  form.autocomplete = "off";

  const field = (label, input, hint) => {
    const l = el("label", "field");
    l.append(el("span", "", label), input);
    if (hint) l.append(el("small", "hint", hint));
    return l;
  };
  const fileInput = (accept) => {
    const i = el("input");
    i.type = "file";
    i.accept = accept;
    return i;
  };

  const students = fileInput("application/json,.json");
  const progress = fileInput("application/json,.json");
  const nick = el("input");
  nick.type = "text";
  nick.maxLength = 16;
  nick.placeholder = "닉네임 (진행 상태 파일에서 자동으로 채웁니다)";
  const key = el("input");
  key.type = "text";
  key.maxLength = 40;
  key.placeholder = "처음이면 비워 두세요";
  const pub = el("input");
  pub.type = "checkbox";
  const pubLabel = el("label", "check");
  pubLabel.append(pub, el("span", "", "리더보드에 내 닉네임과 대표 학생을 공개합니다"));
  const go = el("button", "", "업로드");
  go.type = "submit";
  const out = el("div", "result");

  progress.addEventListener("change", async () => {
    try {
      const p = await readJson(progress.files[0]);
      if (!nick.value && p?.account?.nickname) nick.value = p.account.nickname;
    } catch {
      /* an unreadable file is reported on submit */
    }
  });

  form.append(
    field("학생 명단 (students.json)", students, "%APPDATA%\\baocr\\students\\students.json"),
    field("진행 상태 (latest.json, 선택)", progress, "%APPDATA%\\baocr\\progress\\latest.json. 메모리얼 로비와 스토리, 모모톡이 여기서 옵니다."),
    field("닉네임", nick),
    field("이전에 받은 키 (다시 올릴 때)", key, "같은 닉네임을 갱신할 때만 필요합니다."),
    pubLabel,
    go,
    out,
  );

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    out.replaceChildren();
    try {
      if (!students.files[0]) throw new Error("학생 명단 파일을 고르세요");
      const body = { nickname: nick.value.trim(), key: key.value.trim() || undefined, public: pub.checked, roster: await readJson(students.files[0]) };
      if (progress.files[0]) body.progress = await readJson(progress.files[0]);
      go.disabled = true;
      const res = await post("/api/upload", body);
      session.key = res.key;
      const box = el("div", "keybox");
      box.append(el("p", "hint", res.created ? "새 키입니다. 지금 복사해 두세요. 다시 볼 수 없습니다." : "갱신했습니다. 키는 그대로입니다."), el("code", "key", res.key));
      const copy = el("button", "ghost", "복사");
      copy.type = "button";
      copy.addEventListener("click", () => navigator.clipboard?.writeText(res.key));
      const view = el("a", "button", "내 정보 보기");
      view.href = "#/me";
      box.append(copy, view, el("p", "hint", `학생 ${res.students}명, 점수 ${res.score}, 리더보드 ${res.public ? "공개" : "비공개"}`));
      out.append(box);
    } catch (err) {
      out.append(el("p", "msg", err.message));
    } finally {
      go.disabled = false;
    }
  });

  const addon = card(
    "애드온에서 바로 올리기",
    el("p", "hint", "ba-key-AMAS를 쓰면 파일을 고르지 않아도 됩니다. 닉네임은 스캔한 계정 이름을, 키는 첫 업로드 때 받은 값을 애드온이 기억합니다."),
    el("code", "block", "ba-key-AMAS --upload <이 사이트 주소> [--public]"),
  );
  mount(root, card("스캔 데이터 업로드", form), addon);
}
