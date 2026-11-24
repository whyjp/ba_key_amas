// Small line icons for the card: role, position, attack type, armor type. Drawn here (no image files), so they
// scale and take the card's colours. Each is a 24x24 SVG built with createElementNS (nothing is parsed from text).
const NS = "http://www.w3.org/2000/svg";

const SHIELD = "M12 2.5l8 3v6c0 5-3.5 8.8-8 10.5-4.5-1.7-8-5.5-8-10.5v-6z";

/** name -> list of [tag, attributes] */
const SHAPES = {
  // attack types
  Explosion: [["path", { d: "M12 2l2.3 6 6.2-1.3-3.9 5 5 3.7-6.3.5L12 22l-1.3-6.1-6.3-.5 5-3.7-3.9-5L9.7 8z", fill: "currentColor" }]],
  Pierce: [["path", { d: "M12 2l4.2 9H13.2v11h-2.4V11H7.8z", fill: "currentColor" }]],
  Mystic: [["path", { d: "M12 2l2.6 7.4L22 12l-7.4 2.6L12 22l-2.6-7.4L2 12l7.4-2.6z", fill: "currentColor" }]],
  Sonic: [["path", { d: "M2 12c2.2-6 4.2-6 6 0s3.8 6 6 0 3.8-6 8 0", fill: "none", stroke: "currentColor", "stroke-width": "2.4", "stroke-linecap": "round" }]],
  Chemical: [["path", { d: "M12 2s6.5 6.8 6.5 11.3a6.5 6.5 0 0 1-13 0C5.5 8.8 12 2 12 2z", fill: "currentColor" }]],
  Normal: [["circle", { cx: "12", cy: "12", r: "8", fill: "none", stroke: "currentColor", "stroke-width": "2.4" }]],
  // armor types: one shield, different fills
  LightArmor: [["path", { d: SHIELD, fill: "none", stroke: "currentColor", "stroke-width": "2" }]],
  HeavyArmor: [["path", { d: SHIELD, fill: "currentColor" }]],
  Unarmed: [["path", { d: SHIELD, fill: "none", stroke: "currentColor", "stroke-width": "2" }], ["circle", { cx: "12", cy: "11.5", r: "3", fill: "currentColor" }]],
  ElasticArmor: [["path", { d: SHIELD, fill: "none", stroke: "currentColor", "stroke-width": "2" }], ["path", { d: "M7.5 11l2.2-2.4 2.3 4.8 2.3-4.8 2.2 2.4", fill: "none", stroke: "currentColor", "stroke-width": "1.8", "stroke-linecap": "round", "stroke-linejoin": "round" }]],
  CompositeArmor: [["path", { d: SHIELD, fill: "none", stroke: "currentColor", "stroke-width": "2" }], ["path", { d: "M12 2.5v19c-4.5-1.7-8-5.5-8-10.5v-6z", fill: "currentColor" }]],
  Structure: [["rect", { x: "5", y: "5", width: "14", height: "14", rx: "2", fill: "none", stroke: "currentColor", "stroke-width": "2.2" }]],
  // roles
  STRIKER: [["path", { d: "M19.5 3.5l1 1-8.2 8.2-2.2-2.2z", fill: "currentColor" }], ["path", { d: "M8.6 11.6l3.8 3.8-3.4 3.4-2.1.3-.3-2.1z", fill: "currentColor" }], ["path", { d: "M4 20l3-3", stroke: "currentColor", "stroke-width": "2", "stroke-linecap": "round" }]],
  SPECIAL: [["path", { d: "M13 2l1.6 4.6L19 8l-4.4 1.4L13 14l-1.6-4.6L7 8l4.4-1.4z", fill: "currentColor" }], ["path", { d: "M6 21l9-9", stroke: "currentColor", "stroke-width": "2.2", "stroke-linecap": "round" }]],
};

function node(tag, attrs) {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  return e;
}

/** An icon for a type, role or armor key; unknown keys give a plain ring. */
export function glyph(key, size = 16) {
  const svg = node("svg", { viewBox: "0 0 24 24", width: String(size), height: String(size), "aria-hidden": "true" });
  for (const [tag, attrs] of SHAPES[key] ?? SHAPES.Normal) svg.append(node(tag, attrs));
  return svg;
}

/** Position icon: three columns (front, middle, back) with the student's column filled. */
export function positionGlyph(position, size = 16) {
  const svg = node("svg", { viewBox: "0 0 24 24", width: String(size), height: String(size), "aria-hidden": "true" });
  ["FRONT", "MIDDLE", "BACK"].forEach((p, i) => {
    const on = p === position;
    svg.append(node("rect", { x: String(3 + i * 7), y: "4", width: "5", height: "16", rx: "1.5", fill: on ? "currentColor" : "none", stroke: "currentColor", "stroke-width": "1.8", opacity: on ? "1" : ".55" }));
  });
  return svg;
}

/** A push pin, the sign of a pinned (고정) student. A word alone does not say what "고정" means. */
export function pinGlyph(size = 16) {
  const svg = node("svg", { viewBox: "0 0 24 24", width: String(size), height: String(size), "aria-hidden": "true" });
  svg.append(node("path", { d: "M15.5 2.5l6 6-1.7 1.7-2-.6-3.6 3.6.6 4.6-1.6 1.6-4-4-5.3 5.3-1.4-1.4 5.3-5.3-4-4 1.6-1.6 4.6.6 3.6-3.6-.6-2z", fill: "currentColor" }));
  return svg;
}
