import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { test } from "node:test";
import { accountHash, foldNickname, normalizeNickname, parseAccountId, parsePublicId, publicId } from "../src/keys.ts";

const ID = "ab".repeat(32);

test("nicknames reject controls and bidi marks but allow #, which is part of no key", () => {
  assert.equal(normalizeNickname("a​b"), null, "format characters stay refused");
  assert.equal(normalizeNickname("a#b"), "a#b");
  assert.equal(normalizeNickname("#1 소다"), "#1 소다");
  assert.equal(normalizeNickname("a‮b"), null);
  assert.equal(normalizeNickname("a\nb"), null);
  assert.equal(normalizeNickname(""), null);
  assert.equal(normalizeNickname("x".repeat(17)), null);
  assert.equal(normalizeNickname("  소다팝 "), "소다팝");
});

test("the search form of a nickname ignores case and width", () => {
  assert.equal(foldNickname("Soda"), foldNickname("ＳＯＤＡ"));
});

test("an account id is 64 hex digits, a public id 16", () => {
  assert.equal(parseAccountId(ID.toUpperCase()), ID);
  assert.equal(parseAccountId(ID.slice(1)), null);
  assert.equal(parseAccountId(ID + "0"), null);
  assert.equal(parseAccountId("g".repeat(64)), null);
  assert.equal(parseAccountId(null), null);
  assert.equal(parseAccountId(12345), null);
  assert.equal(parsePublicId("0123456789abcdef"), "0123456789abcdef");
  assert.equal(parsePublicId("0123456789ABCDEF"), null);
  assert.equal(parsePublicId("0123456789abcde"), null);
});

test("the stored hashes are HMAC-SHA256 under the pepper with separate prefixes (checked against node:crypto)", async () => {
  const h = (m: string) => createHmac("sha256", "pep").update(m).digest("hex");
  assert.equal(await accountHash("pep", ID), h("account:" + ID));
  assert.equal(await publicId("pep", ID), h("public:" + ID).slice(0, 16));
  assert.notEqual((await accountHash("pep", ID)).slice(0, 16), await publicId("pep", ID), "the public id does not reveal the account hash");
});

test("the hashes depend on the pepper and on the account", async () => {
  assert.notEqual(await accountHash("a", ID), await accountHash("b", ID));
  assert.notEqual(await accountHash("a", ID), await accountHash("a", "cd".repeat(32)));
  assert.notEqual(await publicId("a", ID), await publicId("b", ID));
  await assert.rejects(accountHash("", ID), /KEY_PEPPER/);
});
