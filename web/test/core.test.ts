import assert from "node:assert/strict";
import { test } from "node:test";
import { normalizeNickname } from "../src/keys.ts";
import { canonicalName, Invalid, parseUpload } from "../src/validate.ts";

test("an upload keeps known fields only and drops duplicates and bad numbers", () => {
  const up = parseUpload({
    nickname: "소다팝",
    roster: { entries: [
      { student: { name: "아루", level: 80, rarity_stars: 3, combat_role: "STRIKER", is_pinned: true, evil: "<script>" } },
      { student: { name: "아루", level: 1 } },
      { student: { name: "하루", level: 999, ex_skill_level: "5" } },
      { nothing: true },
    ], account: { credits: 5, pyroxene: -1, ap: "10/20", extra: 1 } },
  });
  assert.equal(up.students.length, 2);
  assert.deepEqual(up.students[0], { name: "아루", student_id: null, level: 80, stars: 3, ex_skill: null, normal_skill: null, passive_skill: null, sub_skill: null, bond: null, uw_stars: null, uw_level: null, role: "STRIKER", pinned: true, memorial: null, detail: null });
  assert.equal(up.students[1]?.level, null);
  assert.deepEqual(up.account, { credits: 5, ap: "10/20" });
});

test("an upload without readable students is refused", () => {
  assert.throws(() => parseUpload({ nickname: "a", roster: { entries: [] } }), Invalid);
  assert.throws(() => parseUpload([]), Invalid);
});

test("scanned detail is whitelisted and ranged; terrain falls back to the dictionary", () => {
  const up = parseUpload({
    nickname: "x",
    roster: { entries: [{ student: {
      name: "리오", stats: { max_hp: 33907, attack: 3444, defense: -5, healing: 1, evil: 9 }, exp: { current: 41, required: 21790 },
      loadout: { position: "MIDDLE", weapon_class: "HG<script>" }, terrain: { urban: null, outdoor: null, indoor: null },
      fixed_meta: { terrain_base: { urban: 4, outdoor: 2, indoor: 0 } },
      equipment: [{ slot: 1, slot_kind: "standard", equipped: true, type: "Hat", item_name: "Mini Hat", tier: 6, level: 50 }, { slot: 9 }],
      skill_locked: { ex: false, basic: true, extra: true },
      deep: { stat_detail: { accuracy: 100, critical: 200, range: 650, bogus: 1 }, mystic: { stars: 2, status_text: "free text" } },
    } }] },
  });
  assert.deepEqual(up.students[0]?.detail, {
    stats: { max_hp: 33907, attack: 3444, healing: 1 },
    exp: { current: 41, required: 21790 },
    position: "MIDDLE",
    terrain: { urban: 4, outdoor: 2, indoor: 0 },
    equipment: [{ slot: 1, unique: false, equipped: true, type: "Hat", item_name: "Mini Hat", tier: 6, level: 50 }],
    skill_locked: { ex: false, basic: true },
    stat_detail: { accuracy: 100, critical: 200, range: 650 },
    mystic: { stars: 2 },
  });
});

test("names the progress scan misread are matched to the owned student they are nearest to", () => {
  const owned = ["숲", "시로코*테러", "아야네", "아카리", "하레", "세리카(새해)", "모에"];
  assert.equal(canonicalName("순", owned), "숲");
  assert.equal(canonicalName("시로코츠테러", owned), "시로코*테러");
  assert.equal(canonicalName("이카리", owned), "아카리");
  assert.equal(canonicalName("하례", owned), "하레");
  assert.equal(canonicalName("‘세리카(새해)", owned), "세리카(새해)");
  assert.equal(canonicalName("전혀다른이름", owned), "전혀다른이름", "far names are left alone");
  assert.equal(canonicalName("아네", ["아야네", "아카네"]), "아네", "two equally near students: no guess");
});

test("equipment keeps a tier only up to 10 and drops a level above the tier's cap", () => {
  const up = parseUpload({ nickname: "x", roster: { entries: [{ student: { name: "케이", equipment: [
    { slot: 1, slot_kind: "standard", equipped: true, type: "Shoes", item_name: "Casual Sneakers", tier: 7, level: 55 },
    { slot: 2, slot_kind: "standard", equipped: true, type: "Hairpin", tier: 6, level: 55 },
    { slot: 3, slot_kind: "standard", equipped: true, tier: 12, level: 40 },
  ] } }] } });
  const eq = (up.students[0]?.detail?.equipment ?? []) as { tier?: number; level?: number }[];
  assert.deepEqual([eq[0]?.tier, eq[0]?.level], [7, 55]);
  assert.deepEqual([eq[1]?.tier, eq[1]?.level], [6, undefined], "55 is above the T6 cap of 50");
  assert.deepEqual([eq[2]?.tier, eq[2]?.level], [undefined, 40], "T12 does not exist");
});
