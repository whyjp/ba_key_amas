import assert from "node:assert/strict";
import { test } from "node:test";
import { foldNickname, hashKey, keyMatches, newTag, normalizeNickname, parseKey } from "../src/keys.ts";
import { Invalid, parseUpload } from "../src/validate.ts";

test("a key is nickname#hex6, split at the last #", () => {
  assert.deepEqual(parseKey("소다팝#A1b2C3"), { nickname: "소다팝", tag: "a1b2c3" });
  assert.equal(parseKey("소다팝"), null);
  assert.equal(parseKey("소다팝#12345"), null);
  assert.equal(parseKey("소다팝#12345g"), null);
});

test("nicknames reject controls, bidi marks and #", () => {
  assert.equal(normalizeNickname("a‮b"), null);
  assert.equal(normalizeNickname("a\nb"), null);
  assert.equal(normalizeNickname("a#b"), null);
  assert.equal(normalizeNickname(""), null);
  assert.equal(normalizeNickname("x".repeat(17)), null);
  assert.equal(normalizeNickname("  소다팝 "), "소다팝");
});

test("nicknames that differ only in case or width are one", () => {
  assert.equal(foldNickname("Soda"), foldNickname("ＳＯＤＡ"));
});

test("the hash ignores nickname case, depends on the tag and the pepper", async () => {
  const a = await hashKey("p", { nickname: "Soda", tag: "aaaaaa" });
  assert.equal(a, await hashKey("p", { nickname: "soda", tag: "aaaaaa" }));
  assert.notEqual(a, await hashKey("p", { nickname: "Soda", tag: "aaaaab" }));
  assert.notEqual(a, await hashKey("q", { nickname: "Soda", tag: "aaaaaa" }));
  assert.ok(await keyMatches("p", { nickname: "SODA", tag: "aaaaaa" }, a));
  assert.ok(!(await keyMatches("p", { nickname: "soda", tag: "000000" }, a)));
});

test("tags are 6 hex digits", () => {
  for (let i = 0; i < 50; i++) assert.match(newTag(), /^[0-9a-f]{6}$/);
});

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
  assert.deepEqual(up.students[0], { name: "아루", level: 80, stars: 3, ex_skill: null, normal_skill: null, passive_skill: null, sub_skill: null, bond: null, uw_stars: null, uw_level: null, role: "STRIKER", pinned: true, memorial: null });
  assert.equal(up.students[1]?.level, null);
  assert.deepEqual(up.account, { credits: 5, ap: "10/20" });
});

test("an upload without readable students is refused", () => {
  assert.throws(() => parseUpload({ nickname: "a", roster: { entries: [] } }), Invalid);
  assert.throws(() => parseUpload([]), Invalid);
});
