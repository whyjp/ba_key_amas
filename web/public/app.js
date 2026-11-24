"use strict";
// All text from the server goes in through textContent. No innerHTML anywhere, so a nickname or a
// student name can never become markup. The key lives in this variable only (not storage, not the URL).
(() => {
  const $ = (id) => document.getElementById(id);
  let key = null;
  let data = null;
  let view = "tile";
  let sort = { col: "level", dir: -1 };

  const COLS = [
    ["name", "학생", "l"], ["level", "Lv"], ["stars", "★"], ["uw_stars", "고유★"], ["uw_level", "고유Lv"],
    ["ex_skill", "EX"], ["normal_skill", "기본"], ["passive_skill", "강화"], ["sub_skill", "서브"],
    ["bond", "인연"], ["role", "역할", "l"], ["school", "학교", "l"], ["attack", "공격", "l"], ["armor", "방어", "l"], ["pinned", "고정"],
  ];
  const ROLE = { STRIKER: "스트라이커", SPECIAL: "스페셜" };

  function el(tag, cls, text) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  }

  async function api(path, body) {
    const res = await fetch(path, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
    const out = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(out.error || "요청이 실패했습니다");
    return out;
  }

  function cell(s, col) {
    const v = s[col];
    if (col === "role") return v ? ROLE[v] || v : null;
    if (col === "pinned") return v === null || v === undefined ? null : v ? "예" : "";
    return v === null || v === undefined ? null : v;
  }

  function visible() {
    const q = $("filter").value.trim();
    const rows = data.students.filter((s) => !q || s.name.includes(q));
    const { col, dir } = sort;
    rows.sort((a, b) => {
      const x = cell(a, col), y = cell(b, col);
      if (x === null && y === null) return a.name.localeCompare(b.name, "ko");
      if (x === null) return 1; // empty values always last
      if (y === null) return -1;
      const c = typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y), "ko");
      return c * dir || a.name.localeCompare(b.name, "ko");
    });
    return rows;
  }

  function tile(s) {
    const t = el("div", "tile");
    if (s.student_id) {
      const img = el("img");
      img.src = "/portraits/" + encodeURIComponent(s.student_id) + ".webp";
      img.alt = "";
      img.loading = "lazy";
      t.append(img);
    } else {
      t.append(el("div", "ph", "초상화 없음"));
    }
    if (s.level !== null) t.append(el("span", "lv", "Lv." + s.level));
    if (s.stars !== null) t.append(el("span", "st", "★" + s.stars));
    const ft = el("div", "ft");
    if (s.uw_stars !== null) ft.append(el("span", "chip uw", "고유 ★" + s.uw_stars));
    if (s.ex_skill !== null) ft.append(el("span", "chip", "EX" + s.ex_skill));
    if (s.bond !== null) ft.append(el("span", "chip", "인연" + s.bond));
    if (s.pinned) ft.append(el("span", "chip pin", "고정"));
    t.append(ft, el("div", "name", s.name));
    return t;
  }

  function table(rows) {
    const t = el("table");
    const head = el("tr");
    for (const [col, label, cls] of COLS) {
      const th = el("th", cls || "", label);
      th.tabIndex = 0;
      if (sort.col === col) th.setAttribute("aria-sort", sort.dir > 0 ? "ascending" : "descending");
      const pick = () => {
        sort = { col, dir: sort.col === col ? -sort.dir : col === "name" || cls === "l" ? 1 : -1 };
        render();
      };
      th.addEventListener("click", pick);
      th.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); pick(); } });
      head.append(th);
    }
    const tb = el("tbody");
    for (const s of rows) {
      const tr = el("tr");
      for (const [col, , cls] of COLS) {
        const v = cell(s, col);
        const td = el("td", (cls || "") + (v === null ? " null" : ""), v === null ? "-" : String(v));
        tr.append(td);
      }
      tb.append(tr);
    }
    const thead = el("thead");
    thead.append(head);
    t.append(thead, tb);
    return t;
  }

  function summary() {
    const box = $("summary");
    box.replaceChildren();
    const add = (v, label) => { const d = el("div"); d.append(el("b", "", String(v)), el("span", "", label)); box.append(d); };
    add(data.students.length, "보유 학생");
    const a = data.account;
    if (a && a.credits !== undefined) add(a.credits.toLocaleString("ko-KR"), "크레딧");
    if (a && a.pyroxene !== undefined) add(a.pyroxene.toLocaleString("ko-KR"), "청휘석");
    if (a && a.ap) add(a.ap, "AP");
    add(new Date(data.updated_at * 1000).toLocaleString("ko-KR"), "마지막 업로드");
  }

  function render() {
    const rows = visible();
    $("count").textContent = rows.length + "명";
    $("v-tile").setAttribute("aria-selected", String(view === "tile"));
    $("v-list").setAttribute("aria-selected", String(view === "list"));
    $("tiles").hidden = view !== "tile";
    $("list").hidden = view !== "list";
    if (view === "tile") $("tiles").replaceChildren(...rows.map(tile));
    else $("list").replaceChildren(table(rows));
  }

  async function open(k) {
    data = await api("/api/profile", { key: k });
    key = k;
    $("who").textContent = data.nickname;
    $("who").hidden = false;
    $("gate").hidden = true;
    $("board").hidden = false;
    summary();
    render();
  }

  function close() {
    key = null;
    data = null;
    $("who").hidden = true;
    $("board").hidden = true;
    $("gate").hidden = false;
    $("key").value = "";
    $("gate-msg").textContent = "";
  }

  $("gate-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    $("gate-msg").textContent = "";
    try { await open($("key").value.trim()); } catch (err) { $("gate-msg").textContent = err.message; }
  });
  $("v-tile").addEventListener("click", () => { view = "tile"; render(); });
  $("v-list").addEventListener("click", () => { view = "list"; render(); });
  $("filter").addEventListener("input", render);
  $("logout").addEventListener("click", close);
  $("rekey").addEventListener("click", async () => {
    if (!confirm("키를 새로 만듭니다. 이전 키는 곧바로 쓸 수 없게 됩니다.")) return;
    try {
      const out = await api("/api/rekey", { key });
      key = out.key;
      prompt("새 키입니다. 지금 복사해 두세요.", out.key);
    } catch (err) { alert(err.message); }
  });
})();
