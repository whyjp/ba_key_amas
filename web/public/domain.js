// Game-domain names and the small visual parts shared by the views. `meta.json` is generated from the
// public SchaleDB data by `npm run assets`; if it is missing the keys are shown as they are.
import { el } from "./ui.js";

let metaPromise;
export function loadMeta() {
  metaPromise ??= Promise.all([
    fetch("/meta.json").then((r) => (r.ok ? r.json() : {})).catch(() => ({})),
    fetch("/halos.json").then((r) => (r.ok ? r.json() : [])).catch(() => []),
    fetch("/card-lobby.json").then((r) => (r.ok ? r.json() : [])).catch(() => []),
  ]).then(([meta, halos, lobby]) => ({ ...meta, halos: new Set(halos), hiresLobby: new Set(lobby) }));
  return metaPromise;
}

/** A student's halo (the ring over the head, which is what identifies the student), or null when there is none. */
export function halo(meta, id, cls = "halo") {
  if (!id || !meta.halos?.has(id)) return null;
  const img = el("img", cls);
  img.src = "/halos/" + encodeURIComponent(id) + ".png";
  img.alt = "";
  img.loading = "lazy";
  return img;
}

export const ROLE = { STRIKER: "스트라이커", SPECIAL: "스페셜" };

export const name = {
  school: (meta, k) => meta.schools?.[k] ?? k ?? "-",
  attack: (meta, k) => meta.attack?.[k] ?? k ?? "-",
  armor: (meta, k) => meta.armor?.[k] ?? k ?? "-",
};

const ATTACK_COLOR = { Explosion: "#c84b3c", Pierce: "#c8963c", Mystic: "#2f7fc4", Sonic: "#7c5cc4", Chemical: "#4aa070", Normal: "#7a8499" };
const ARMOR_COLOR = { LightArmor: "#c84b3c", HeavyArmor: "#c8963c", Unarmed: "#2f7fc4", ElasticArmor: "#7c5cc4", CompositeArmor: "#4aa070", Normal: "#7a8499" };
export const attackColor = (k) => ATTACK_COLOR[k] ?? "#7a8499";
export const armorColor = (k) => ARMOR_COLOR[k] ?? "#7a8499";

export function icon(id, cls = "ico") {
  if (!id) return el("span", cls + " ph", "?");
  const img = el("img", cls);
  img.src = "/icons/" + encodeURIComponent(id) + ".webp";
  img.alt = "";
  img.loading = "lazy";
  return img;
}

export function schoolLogo(key, cls = "school-logo") {
  if (!key) return el("span", cls);
  const img = el("img", cls);
  img.src = "/schools/" + encodeURIComponent(key) + ".png";
  img.alt = "";
  img.loading = "lazy";
  return img;
}

/** Stars as shaped marks: gold for the student's stars, sky blue for the unique weapon's. */
export function stars(n, kind = "student", max = 5) {
  const row = el("span", "stars " + kind);
  row.setAttribute("role", "img");
  row.setAttribute("aria-label", `${n}성`);
  for (let i = 0; i < max; i++) row.append(el("i", i < n ? "on" : "off"));
  return row;
}

/** A row of segments, `value` of `max` lit. */
export function meter(value, max, cls = "") {
  const m = el("span", "meter " + cls);
  for (let i = 1; i <= max; i++) m.append(el("i", i <= value ? "on" : ""));
  return m;
}

export function bondBadge(rank, cls = "bond") {
  const b = el("span", cls);
  b.append(el("i", "heart"), el("b", "", rank === null || rank === undefined ? "-" : String(rank)));
  b.title = "인연 랭크";
  return b;
}
