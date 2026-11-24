import assert from "node:assert/strict";
import { test } from "node:test";
import { entryFor, indexByName, type Catalog } from "../src/catalog.ts";
import { parseUpload } from "../src/validate.ts";

const e = (id: number, name: string, role: string) => ({ id, name, role, school: "Abydos", attack: "Mystic", armor: "HeavyArmor", squad: "Main" });
const catalog: Catalog = { "10098": e(10098, "호시노(무장)", "Tanker"), "10099": e(10099, "호시노(무장)", "DamageDealer"), "10005": e(10005, "호시노", "DamageDealer") };
const byName = indexByName(catalog);

test("the scanner's id picks the mode of a student that has two", () => {
  assert.equal(entryFor(catalog, byName, "호시노(무장)", 10098)?.role, "Tanker");
  assert.equal(entryFor(catalog, byName, "호시노(무장)", 10099)?.role, "DamageDealer");
});

test("an old upload without an id falls back to the first student of the name", () => {
  assert.equal(entryFor(catalog, byName, "호시노(무장)", null)?.id, 10098);
  assert.equal(entryFor(catalog, byName, "호시노", null)?.id, 10005);
  assert.equal(entryFor(catalog, byName, "없는학생", null), null);
});

test("an id that belongs to another name is not trusted", () => {
  assert.equal(entryFor(catalog, byName, "호시노", 10098)?.id, 10005);
});

test("a student with two modes is one student: one row, the scanned mode picks the picture", () => {
  const up = parseUpload({ nickname: "x", roster: { entries: [
    { student: { name: "호시노(무장)", reference: { id: 10098 } } },
    { student: { name: "호시노(무장)", reference: { id: 10099 } } },
  ] } });
  assert.deepEqual(up.students.map((s) => [s.name, s.student_id]), [["호시노(무장)", 10098]]);
});
