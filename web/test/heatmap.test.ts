import assert from "node:assert/strict";
import { test } from "node:test";
// @ts-expect-error plain browser module without types
import { foldTail, heatLevel, resizeMatters, squarify, wantedHeight } from "../public/heatmap-math.js";

const items = (...v: number[]) => v.map((value) => ({ value }));

test("tiles fill the frame and each tile's area follows its value", () => {
  const out = squarify(items(60, 30, 10), 0, 0, 200, 100);
  const area = (t: { w: number; h: number }) => t.w * t.h;
  assert.equal(Math.round(out.reduce((n: number, t: { w: number; h: number }) => n + area(t), 0)), 20000);
  assert.equal(Math.round(area(out[0])), 12000);
});

test("only a tail of two or more tiny tiles is folded", () => {
  const area = 100 * 100;
  assert.deepEqual(foldTail(items(100, 1, 1), area, 1500).tail.length, 2);
  assert.deepEqual(foldTail(items(100, 50, 1), area, 1500).tail.length, 0, "a lone small tile stays as it is");
  assert.equal(foldTail(items(50, 50), area, 1500).tail.length, 0);
  assert.equal(foldTail([], area, 1500).kept.length, 0);
});

test("the heat level climbs with standing", () => {
  assert.deepEqual([0, 3, 10, 30, 80].map((r) => heatLevel(r, 100)), [4, 4, 3, 2, 0]);
});

test("a phone's address bar does not redraw the map, a width change does", () => {
  const prev = { w: 390, h: 800 };
  assert.equal(resizeMatters(prev, 390, 860), false);
  assert.equal(resizeMatters(prev, 390, 1000), true);
  assert.equal(resizeMatters(prev, 412, 800), true);
  assert.equal(resizeMatters(null, 390, 800), true);
});

test("a narrow window gets a taller map, capped", () => {
  assert.ok(wantedHeight(85, 390) > wantedHeight(85, 1200));
  assert.ok(wantedHeight(1000, 100) <= 1600);
});
