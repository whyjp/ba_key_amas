import assert from "node:assert/strict";
import { test } from "node:test";
import { analyze, baseName, bondPoints, type OwnedStudent } from "../src/score.ts";
import { parseUpload } from "../src/validate.ts";

const s = (name: string, o: Partial<OwnedStudent> = {}): OwnedStudent => ({
  name, student_id: null, level: 1, stars: 1, ex_skill: 1, normal_skill: 1, passive_skill: 1, sub_skill: 1,
  bond: 1, uw_stars: null, uw_level: null, pinned: false, memorial: false, ...o,
});

test("only a trailing parenthesis is an outfit", () => {
  assert.equal(baseName("리오(무장)"), "리오");
  assert.equal(baseName("리오"), "리오");
  assert.equal(baseName("시로코*테러"), "시로코*테러");
  assert.equal(baseName("(이상)"), "(이상)");
});

test("bond points grow with rank and jump at 60", () => {
  assert.equal(bondPoints(30), 60);
  assert.ok(bondPoints(31) - bondPoints(30) > bondPoints(30) - bondPoints(29), "a rank past 30 is worth more than one before it");
  assert.ok(bondPoints(60) - bondPoints(59) >= 100, "rank 60 is a milestone");
  assert.ok(bondPoints(61) - bondPoints(60) > bondPoints(31) - bondPoints(30));
});

test("outfits add up under one name, with the plain outfit first", () => {
  const a = analyze([s("리오(무장)", { bond: 10, student_id: 2 }), s("리오", { bond: 20, student_id: 1 }), s("아루", { bond: 5 })]);
  const rio = a.characters.find((c) => c.base === "리오");
  assert.deepEqual(rio?.outfits.map((o) => o.name), ["리오", "리오(무장)"]);
  assert.equal(rio?.bondPoints, bondPoints(10) + bondPoints(20));
  assert.equal(a.stats.characters, 2);
});

test("relationship counts more than growth", () => {
  const grown = s("육성형", { level: 90, stars: 5, ex_skill: 5, normal_skill: 10, passive_skill: 10, sub_skill: 10, uw_stars: 4, uw_level: 60, bond: 1 });
  const bonded = s("인연형", { bond: 60, pinned: true, memorial: true });
  const a = analyze([grown, bonded]);
  assert.equal(a.top[0]?.base, "인연형");
});

test("a unique weapon is the biggest growth item", () => {
  const plain = analyze([s("a", { level: 90, stars: 5 })]).characters[0]?.growthPoints ?? 0;
  const weapon = analyze([s("a", { level: 90, stars: 5, uw_stars: 1, uw_level: 30 })]).characters[0]?.growthPoints ?? 0;
  assert.ok(weapon - plain >= 40);
});

test("counts are per character", () => {
  const a = analyze([
    s("리오", { bond: 60, level: 85, pinned: true }),
    s("리오(무장)", { bond: 5, memorial: true }),
    s("아루", { bond: 30 }),
    s("하루", { bond: 29 }),
  ]);
  assert.deepEqual(a.stats, { characters: 3, bond30: 2, bond60: 1, levelHigh: 1, pinned: 1, memorial: 1 });
  assert.equal(a.top.length, 3);
});

test("the duty scan decides memorial lobby, by name", () => {
  const up = parseUpload({
    nickname: "x",
    public: true,
    roster: { entries: [{ student: { name: "아루" } }, { student: { name: "하루" } }, { student: { name: "무츠키" } }] },
    progress: { duty: { students: [{ name: "아루", memorial_lobby: true }, { name: "하루", memorial_lobby: false }] } },
  });
  assert.equal(up.public, true);
  assert.deepEqual(up.students.map((x) => x.memorial), [true, false, null]);
  assert.equal(parseUpload({ nickname: "x", roster: { entries: [{ student: { name: "a" } }] } }).public, false);
});
