import assert from "node:assert/strict";
import { test } from "node:test";
import { aggregate, analyze, baseName, BOND_EXP, bondPoints, growthPoints, OUTFIT_DECAY, starPoints, summarize, upgradePoints, weaponPoints, type OwnedStudent } from "../src/score.ts";
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

test("the bond table is the community's experience table (checked at its published totals)", () => {
  assert.equal(BOND_EXP.length, 101, "rank 0 to 100");
  assert.deepEqual([BOND_EXP[10], BOND_EXP[20], BOND_EXP[30], BOND_EXP[50], BOND_EXP[100]], [300, 1845, 6180, 29175, 240225]);
});

test("bond points follow the experience a rank takes, with a bonus on crossing 30 and 60", () => {
  assert.ok(bondPoints(10) < bondPoints(20) && bondPoints(20) < bondPoints(30));
  assert.ok(bondPoints(30) - bondPoints(29) > 25, "rank 30 is a milestone");
  assert.ok(bondPoints(60) - bondPoints(59) > 55, "rank 60 is a bigger one");
  // a rank past 60 costs far more experience than one before 30, and is worth more
  assert.ok(bondPoints(61) - bondPoints(60) > bondPoints(31) - bondPoints(30));
  assert.ok(bondPoints(100) > bondPoints(60) && bondPoints(100) < 4 * bondPoints(60), "rank 100 is the top, not the whole score");
  assert.equal(bondPoints(0), 0);
  assert.equal(bondPoints(500), bondPoints(100), "a misread rank above the top is the top");
});

test("promoting to 4 and 5 stars is worth far more than the cheap early stars", () => {
  assert.equal(starPoints(3, 3), 0, "nothing above the rarity the student came with");
  assert.ok(starPoints(5, 3) >= 90, "3 stars to 5 stars");
  assert.ok(starPoints(4, 3) >= 40);
  assert.ok(starPoints(3, 1) < 20, "1 star to 3 stars is cheap");
  assert.equal(starPoints(5, 1), starPoints(3, 1) + starPoints(5, 3), "a 1 star student pays the early steps and then the same dear ones");
  assert.equal(starPoints(5, null), starPoints(5, 3), "an unknown rarity is taken as the common 3 stars");
});

test("unique weapon stars cost more with every star, and the weapon level adds", () => {
  assert.equal(weaponPoints(null, null), 0);
  const [one, two, three, four] = [1, 2, 3, 4].map((n) => weaponPoints(n, 30));
  assert.ok(one !== undefined && two !== undefined && three !== undefined && four !== undefined);
  assert.ok(two - one >= 50 && three - two > two - one && four - three > three - two, "each star costs more shards than the last");
  assert.ok(weaponPoints(1, 60) > weaponPoints(1, 30));
});

test("levels past 80 (the level limit) are worth more each than the ones before", () => {
  const at = (level: number) => growthPoints(s("a", { level }));
  assert.ok(at(90) - at(80) > 4 * (at(80) - at(70)));
});

test("the costly items count in full and the rest at the growth weight", () => {
  const base = analyze([s("a", { level: 80, stars: 3, base_stars: 3 })]).characters[0];
  const five = analyze([s("a", { level: 80, stars: 5, base_stars: 3 })]).characters[0];
  const weapon = analyze([s("a", { level: 80, stars: 5, base_stars: 3, uw_stars: 4, uw_level: 60 })]).characters[0];
  assert.ok(five && base && weapon);
  assert.equal(Math.round((five.total - base.total) * 10) / 10, starPoints(5, 3), "the promotion adds its points unscaled");
  assert.equal(Math.round((weapon.total - five.total) * 10) / 10, Math.round(weaponPoints(4, 60) * 10) / 10);
});

test("outfits share one character, the plain outfit first", () => {
  const a = analyze([s("리오(무장)", { bond: 10, student_id: 2 }), s("리오", { bond: 20, student_id: 1 }), s("아루", { bond: 5 })]);
  const rio = a.characters.find((c) => c.base === "리오");
  assert.deepEqual(rio?.outfits.map((o) => o.name), ["리오", "리오(무장)"]);
  assert.equal(a.stats.characters, 2);
});

test("a further outfit adds credit with a coefficient, not in full and not as an average", () => {
  const single = analyze([s("리오", { bond: 20 })]).characters[0]?.total ?? 0;
  const second = analyze([s("리오", { bond: 20 }), s("리오(무장)", { bond: 20 })]).characters[0]?.total ?? 0;
  assert.ok(second > single, "the second outfit still counts");
  assert.ok(second < single * 1.5, "but far less than doubling");
  assert.ok(Math.abs(second - single * (1 + OUTFIT_DECAY)) < 0.2);
});

test("many outfits cannot outrun one deeply loved outfit", () => {
  const wardrobe = analyze(Array.from({ length: 6 }, (_, i) => s(`옷장(${i})`, { bond: 12 })));
  const loved = analyze([s("한벌", { bond: 40, pinned: true, memorial: true })]);
  assert.ok((loved.characters[0]?.total ?? 0) > (wardrobe.characters[0]?.total ?? 0));
});

test("a bond at the top beats a fully grown student, and a full growth beats a middling bond", () => {
  const grown = s("육성형", { level: 90, stars: 5, base_stars: 3, ex_skill: 5, normal_skill: 10, passive_skill: 10, sub_skill: 10, uw_stars: 4, uw_level: 60, bond: 1 });
  assert.equal(analyze([grown, s("인연형", { bond: 100, pinned: true, memorial: true })]).top[0]?.base, "인연형");
  assert.equal(analyze([grown, s("인연형", { bond: 60, pinned: true, memorial: true })]).top[0]?.base, "육성형");
});

test("owning a unique weapon adds its own points", () => {
  const plain = upgradePoints(s("a", { level: 90, stars: 5 }));
  const weapon = upgradePoints(s("a", { level: 90, stars: 5, uw_stars: 1, uw_level: 30 }));
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
  assert.equal(parseUpload({ nickname: "x", roster: { entries: [{ student: { name: "a" } }] } }).public, null, "not said: a stored choice is kept");
});

test("the community sum adds a character over the teachers who own it, outfits merged, without stats", () => {
  const a = analyze([s("리오", { bond: 20, student_id: 1 }), s("리오(무장)", { bond: 10, student_id: 2 }), s("아루", { bond: 5 })]);
  const b = analyze([s("리오", { bond: 30, student_id: 1 })]);
  const sum = aggregate([summarize(a), summarize(b)]);
  const rio = sum.find((c) => c.base === "리오");
  assert.equal(rio?.owners, 2);
  assert.deepEqual(rio?.outfits.map((o) => o.name), ["리오", "리오(무장)"]);
  assert.equal(rio?.total, Math.round(((a.characters.find((c) => c.base === "리오")?.total ?? 0) + (b.characters[0]?.total ?? 0)) * 10) / 10);
  assert.equal(sum[0]?.base, "리오", "best first");
  assert.deepEqual(Object.keys(rio ?? {}).sort(), ["base", "outfits", "owners", "total"], "no stats of a single student leave");
});
