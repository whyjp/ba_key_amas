import assert from "node:assert/strict";
import { test } from "node:test";
import { checkClient, checkSchemas, MIN_PROTOCOL, PROTOCOL } from "../src/version.ts";

const h = (o: Record<string, string>) => new Headers(o);

test("an add-on inside the accepted range is let in", () => {
  assert.deepEqual(checkClient(h({ "x-amas-protocol": String(PROTOCOL), "x-amas-addon": "0.1.0" })), { addon: "0.1.0", protocol: PROTOCOL });
  assert.ok(!("status" in checkClient(h({ "x-amas-protocol": String(MIN_PROTOCOL), "x-amas-addon": "0.1.0" }))));
});

test("an add-on older or newer than the range is refused with 426", () => {
  const old = checkClient(h({ "x-amas-protocol": String(MIN_PROTOCOL - 1), "x-amas-addon": "0.0.1" }));
  const newer = checkClient(h({ "x-amas-protocol": String(PROTOCOL + 1), "x-amas-addon": "9.0.0" }));
  assert.equal("status" in old && old.status, 426);
  assert.equal("status" in newer && newer.status, 426);
});

test("a request without the add-on headers is refused", () => {
  assert.equal((checkClient(h({})) as { status: number }).status, 426);
  assert.equal((checkClient(h({ "x-amas-protocol": "x", "x-amas-addon": "0.1.0" })) as { status: number }).status, 426);
});

test("a browser is refused even with the headers", () => {
  const base = { "x-amas-protocol": String(PROTOCOL), "x-amas-addon": "0.1.0" };
  for (const extra of [{ origin: "https://x.example" }, { "sec-fetch-site": "same-origin" }, { "sec-fetch-mode": "cors" }]) {
    assert.equal((checkClient(h({ ...base, ...extra })) as { status: number }).status, 403);
  }
});

test("old scan files are refused by schema number", () => {
  assert.equal(checkSchemas({ schema_version: 3 }, { schema_version: 2 }), null);
  assert.equal(checkSchemas({ schema_version: 2 }, null)?.status, 426);
  assert.equal(checkSchemas({}, null)?.status, 426);
  assert.equal(checkSchemas({ schema_version: 3 }, { schema_version: 1 })?.status, 426);
  assert.equal(checkSchemas({ schema_version: 3 }, undefined), null);
});
