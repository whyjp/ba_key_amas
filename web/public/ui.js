// Shared helpers. Server text only ever reaches the page through textContent or an attribute set by
// property (never innerHTML), so a nickname or a student name cannot become markup.

export function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

export function mount(root, ...nodes) {
  root.replaceChildren(...nodes);
}

async function call(path, init) {
  const res = await fetch(path, init);
  const out = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(out.error || "요청이 실패했습니다");
  return out;
}

export const get = (path) => call(path);

export function portrait(id, cls = "pt") {
  if (!id) return el("div", cls + " ph", "?");
  const img = el("img", cls);
  img.src = "/portraits/" + encodeURIComponent(id) + ".webp";
  img.alt = "";
  img.loading = "lazy";
  return img;
}

/** One character: all its outfits side by side, the plain outfit first. */
export function outfitStack(outfits, cls = "pt") {
  const box = el("div", "stack");
  for (const o of outfits) {
    const wrap = el("div", "stack-item");
    wrap.title = o.name;
    wrap.append(portrait(o.id, cls));
    box.append(wrap);
  }
  return box;
}

export const num = (n) => (n === null || n === undefined ? "-" : Number(n).toLocaleString("ko-KR"));

export function empty(text) {
  return el("p", "empty", text);
}

export function card(title, ...children) {
  const c = el("section", "card");
  if (title) c.append(el("h2", "", title));
  c.append(...children);
  return c;
}

export function toast(parent, text, bad = false) {
  const m = el("p", bad ? "msg" : "msg ok", text);
  parent.append(m);
  return m;
}
