import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { findEnvFile, parseEnv } from "../scripts/envfile.ts";

test("parseEnv reads NAME=value lines and ignores comments, blanks and junk", () => {
  const got = parseEnv(["# a comment", "", "A=1", "  B = two words  ", "C=\"quoted\"", "D='single'", "E=", "=nokey", "no equals sign", "F=a=b"].join("\r\n"));
  assert.deepEqual(got, { A: "1", B: "two words", C: "quoted", D: "single", E: "", F: "a=b" });
});

test("a quote that is not paired stays part of the value", () => {
  assert.deepEqual(parseEnv('A="open\nB=it\'s'), { A: '"open', B: "it's" });
});

test("findEnvFile takes the nearest .env above a directory and nothing when there is none", () => {
  const base = mkdtempSync(join(tmpdir(), "baocr-env-"));
  try {
    const deep = join(base, "a", "b", "c");
    mkdirSync(deep, { recursive: true });
    assert.equal(findEnvFile(deep, 3), null, "no .env anywhere below the temp root within 3 levels");
    writeFileSync(join(base, ".env"), "X=1");
    assert.equal(findEnvFile(deep, 3), join(base, ".env"));
    writeFileSync(join(base, "a", ".env"), "X=2");
    assert.equal(findEnvFile(deep, 3), join(base, "a", ".env"), "the nearest one wins");
    assert.equal(findEnvFile(deep, 1), null, "the search is bounded");
  } finally {
    rmSync(base, { recursive: true, force: true });
  }
});
