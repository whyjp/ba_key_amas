import assert from "node:assert/strict";
import { test } from "node:test";
import { prepareWorkerImport, MemoryD1 } from "./helpers/d1.ts";
import { MIN_PROTOCOL, PROTOCOL } from "../src/version.ts";
import { addonSignature, SIGNATURE_WINDOW_SECONDS } from "../src/security.ts";

prepareWorkerImport();
const { default: worker } = await import("../src/index.ts");

const A = "a1".repeat(32);
const B = "b2".repeat(32);
const PEPPER = "test-pepper";
const SIGNING_KEY = "test-signing-key-0123456789abcdef";
const ADDON = { "content-type": "application/json", "x-amas-protocol": String(PROTOCOL), "x-amas-addon": "0.1.0" };

function roster(...names: string[]) {
  return { schema_version: 3, entries: names.map((name) => ({ student: { name, level: 80, rarity_stars: 3 } })) };
}

/** Like the Cloudflare binding: `limit` successes per key, then refusals (the window never resets inside a test). */
class FakeLimiter {
  private used = new Map<string, number>();
  private max: number;
  constructor(max: number) {
    this.max = max;
  }
  async limit({ key }: { key: string }): Promise<{ success: boolean }> {
    const n = (this.used.get(key) ?? 0) + 1;
    this.used.set(key, n);
    return { success: n <= this.max };
  }
}

const accountId = (n: number) => n.toString(16).padStart(64, "0");

class Server {
  d1 = new MemoryD1();
  logs: string[] = [];
  pepper: string;
  limiter?: FakeLimiter;
  createLimiter?: FakeLimiter;
  constructor(pepper = PEPPER) {
    this.pepper = pepper;
  }
  /** Server settings for the add-on signature; a test changes them per case. */
  signing: Record<string, string | undefined> = { ADDON_SIGNING_KEY: SIGNING_KEY };
  /** `call` signs like the official add-on unless the headers already carry a signature or `sign` is false. */
  sign = true;
  async call(method: string, path: string, body?: unknown, headers: Record<string, string> = ADDON): Promise<{ status: number; text: string; json: any }> {
    const env = { DB: this.d1, ASSETS: { fetch: async () => new Response("asset") }, KEY_PEPPER: this.pepper, LIMITER: this.limiter, CREATE_LIMITER: this.createLimiter, ...this.signing } as never;
    const payload = body === undefined ? undefined : JSON.stringify(body);
    if (method !== "GET" && this.sign && payload !== undefined && headers["x-amas-addon"] && !headers["x-amas-sig"]) {
      const ts = String(Math.floor(Date.now() / 1000));
      headers = { ...headers, "x-amas-ts": ts, "x-amas-sig": await addonSignature(SIGNING_KEY, ts, path, new TextEncoder().encode(payload)) };
    }
    const orig = [console.error, console.log, console.warn];
    console.error = console.log = console.warn = (...a: unknown[]) => void this.logs.push(a.map(String).join(" "));
    try {
      const req = new Request("http://srv.test" + path, { method, headers: method === "GET" ? {} : headers, body: payload });
      const res = await (worker as { fetch(r: Request, e: never): Promise<Response> }).fetch(req, env);
      const text = await res.text();
      return { status: res.status, text, json: text.startsWith("{") ? JSON.parse(text) : null };
    } finally {
      [console.error, console.log, console.warn] = orig;
    }
  }
  upload(account_id: string, nickname: string, students: string[], extra: Record<string, unknown> = {}) {
    return this.call("POST", "/api/upload", { account_id, nickname, roster: roster(...students), ...extra });
  }
  users() {
    return this.d1.rows("SELECT * FROM users ORDER BY id");
  }
}

test("the same account id updates one row: nickname refreshed, students replaced, same public id", async () => {
  const s = new Server();
  const first = await s.upload(A, "소다팝", ["아루", "하루"]);
  assert.equal(first.status, 201, s.logs.join(" | "));
  assert.equal(first.json.created, true);
  assert.match(first.json.public_id, /^[0-9a-f]{16}$/);
  const second = await s.upload(A, "새이름", ["아루", "카린", "네루"]);
  assert.equal(second.status, 200);
  assert.equal(second.json.created, false);
  assert.equal(second.json.public_id, first.json.public_id);
  const users = s.users();
  assert.equal(users.length, 1);
  assert.equal(users[0]?.nickname, "새이름");
  assert.deepEqual(s.d1.rows("SELECT name FROM students ORDER BY name").map((r) => r.name), ["네루", "아루", "카린"]);
});

test("two accounts may share a nickname", async () => {
  const s = new Server();
  const one = await s.upload(A, "소다팝", ["아루"], { public: true });
  const two = await s.upload(B, "소다팝", ["하루"], { public: true });
  assert.equal(two.status, 201);
  assert.notEqual(one.json.public_id, two.json.public_id);
  assert.equal(s.users().length, 2);
  const found = await s.call("GET", "/api/search?q=" + encodeURIComponent("소다"));
  assert.equal(found.json.matches.length, 2, "both are offered as candidates");
  assert.deepEqual(found.json.matches.map((m: { nickname: string }) => m.nickname), ["소다팝", "소다팝"]);
});

test("the public flag is kept unless the request sets it", async () => {
  const s = new Server();
  await s.upload(A, "a", ["아루"], { public: true });
  assert.equal((await s.upload(A, "a", ["아루"])).json.public, true);
  assert.equal(s.users()[0]?.public, 1);
  assert.equal((await s.upload(A, "a", ["아루"], { public: false })).json.public, false);
  assert.equal(s.users()[0]?.public, 0);
  const fresh = await s.upload(B, "b", ["아루"]);
  assert.equal(fresh.json.public, false, "a new account starts private");
});

test("a private account is not listed or searchable, but its public id link opens it", async () => {
  const s = new Server();
  const up = await s.upload(A, "비공개", ["아루"]);
  assert.deepEqual((await s.call("GET", "/api/leaderboard")).json.entries, []);
  assert.deepEqual((await s.call("GET", "/api/search?q=" + encodeURIComponent("비공개"))).json.matches, []);
  const page = await s.call("GET", "/api/public?id=" + up.json.public_id);
  assert.equal(page.status, 200);
  assert.equal(page.json.nickname, "비공개");
  assert.equal(page.json.public, false);
  assert.equal((await s.call("GET", "/api/public?id=" + "0".repeat(16))).status, 404);
  assert.equal((await s.call("GET", "/api/public?id=" + encodeURIComponent("비공개"))).status, 404, "a nickname is no longer a way in");
});

test("a public account is on the leaderboard under its public id", async () => {
  const s = new Server();
  const up = await s.upload(A, "공개", ["아루"], { public: true });
  const board = (await s.call("GET", "/api/leaderboard")).json.entries;
  assert.equal(board.length, 1);
  assert.equal(board[0].public_id, up.json.public_id);
});

test("search matches a nickname prefix with case and width folded, the exact name first", async () => {
  const s = new Server();
  await s.upload(A, "Sodapop", ["아루"], { public: true });
  await s.upload(B, "Soda", ["아루"], { public: true });
  const found = (await s.call("GET", "/api/search?q=" + encodeURIComponent("ＳＯＤＡ"))).json.matches;
  assert.deepEqual(found.map((m: { nickname: string }) => m.nickname), ["Soda", "Sodapop"]);
  assert.deepEqual((await s.call("GET", "/api/search?q=")).json.matches, []);
});

test("visibility and delete need an account id that exists", async () => {
  const s = new Server();
  const up = await s.upload(A, "a", ["아루"]);
  assert.equal((await s.call("POST", "/api/visibility", { account_id: B, public: true })).status, 404);
  assert.equal((await s.call("POST", "/api/visibility", { account_id: "nope", public: true })).status, 400);
  assert.equal((await s.call("POST", "/api/visibility", { account_id: A, public: true })).json.public, true);
  assert.equal(s.users()[0]?.public, 1);
  assert.equal((await s.call("POST", "/api/delete", { account_id: B })).status, 404);
  assert.equal(s.users().length, 1);
  assert.equal((await s.call("POST", "/api/delete", { account_id: A })).json.deleted, true);
  assert.equal(s.users().length, 0);
  assert.equal(s.d1.rows("SELECT * FROM students").length, 0);
  assert.equal((await s.call("GET", "/api/public?id=" + up.json.public_id)).status, 404);
});

test("the old key flows are gone", async () => {
  const s = new Server();
  assert.equal((await s.call("POST", "/api/rekey", { account_id: A })).status, 404);
  assert.equal((await s.call("POST", "/api/profile", { account_id: A })).status, 404);
});

test("an upload without a valid account id is refused", async () => {
  const s = new Server();
  for (const bad of [undefined, "", "abc", "z".repeat(64), A.slice(1), 5]) {
    const r = await s.call("POST", "/api/upload", { account_id: bad, nickname: "a", roster: roster("아루") });
    assert.equal(r.status, 400);
  }
  assert.equal(s.users().length, 0);
});

test("an add-on that speaks the old protocol, or no protocol, is told to update", async () => {
  const s = new Server();
  assert.ok(MIN_PROTOCOL >= 2);
  const old = await s.call("POST", "/api/upload", { account_id: A, nickname: "a", roster: roster("아루") }, { ...ADDON, "x-amas-protocol": "1" });
  assert.equal(old.status, 426);
  assert.match(old.json.download, /^https:\/\//);
  assert.equal((await s.call("POST", "/api/upload", { nickname: "a", key: "a#123456", roster: roster("아루") }, { "content-type": "application/json" })).status, 426);
  assert.equal(s.users().length, 0);
});

test("a browser cannot upload", async () => {
  const s = new Server();
  const r = await s.call("POST", "/api/upload", { account_id: A, nickname: "a", roster: roster("아루") }, { ...ADDON, origin: "http://srv.test" });
  assert.equal(r.status, 403);
});

test("no account id appears in a response, in the database or in the logs", async () => {
  const s = new Server();
  const seen: string[] = [];
  const keep = async (p: Promise<{ text: string }>) => void seen.push((await p).text);
  await keep(s.upload(A, "a", ["아루"], { public: true }));
  await keep(s.upload(A, "a2", ["아루"]));
  await keep(s.call("POST", "/api/visibility", { account_id: A, public: true }));
  await keep(s.call("POST", "/api/visibility", { account_id: B, public: true }));
  await keep(s.call("POST", "/api/upload", { account_id: B }));
  await keep(s.call("GET", "/api/leaderboard"));
  await keep(s.call("GET", "/api/search?q=a"));
  await keep(s.call("GET", "/api/public?id=" + s.users()[0]?.public_id));
  const broken = new Server("");
  await keep(broken.upload(A, "a", ["아루"]));
  s.logs.push(...broken.logs);
  assert.ok(broken.logs.length > 0, "the failing request was logged");
  for (const id of [A, B]) {
    for (const t of seen) assert.ok(!t.includes(id), "response");
    for (const l of s.logs) assert.ok(!l.includes(id), "log");
    for (const table of ["users", "students"]) assert.ok(!JSON.stringify(s.d1.rows(`SELECT * FROM ${table}`)).includes(id), "database");
  }
  const row = s.users()[0] as { account_hash: string; public_id: string };
  assert.match(row.account_hash, /^[0-9a-f]{64}$/);
  assert.notEqual(row.account_hash, A);
});

test("the same id under another pepper is another account", async () => {
  const one = new Server("pepper-1");
  const two = new Server("pepper-2");
  const a = await one.upload(A, "a", ["아루"]);
  const b = await two.upload(A, "a", ["아루"]);
  assert.notEqual(a.json.public_id, b.json.public_id);
  assert.notEqual(one.users()[0]?.account_hash, two.users()[0]?.account_hash);
});

test("creating an account is limited per IP, updating one is not", async () => {
  const s = new Server();
  s.createLimiter = new FakeLimiter(2);
  const from = (ip: string) => ({ ...ADDON, "cf-connecting-ip": ip });
  const up = (id: string, ip: string) => s.call("POST", "/api/upload", { account_id: id, nickname: "a", roster: roster("아루") }, from(ip));
  assert.equal((await up(accountId(1), "1.1.1.1")).status, 201);
  assert.equal((await up(accountId(2), "1.1.1.1")).status, 201);
  const refused = await up(accountId(3), "1.1.1.1");
  assert.equal(refused.status, 429);
  assert.match(refused.json.error, /계정/);
  assert.equal(s.users().length, 2, "the refused upload created nothing");
  assert.equal((await up(accountId(1), "1.1.1.1")).status, 200, "an existing account can still be updated from that IP");
  assert.equal((await up(accountId(2), "1.1.1.1")).status, 200);
  assert.equal((await up(accountId(3), "2.2.2.2")).status, 201, "another IP has its own allowance");
});

test("only the creation path spends the creation allowance", async () => {
  const s = new Server();
  s.createLimiter = new FakeLimiter(1);
  assert.equal((await s.upload(A, "a", ["아루"])).status, 201);
  for (let i = 0; i < 5; i++) assert.equal((await s.upload(A, "a", ["아루"])).status, 200);
  assert.equal((await s.upload(B, "b", ["아루"])).status, 429, "the one allowance went to the first creation");
});

test("a new account is created whole or not at all: a failing students insert leaves no empty account behind", async () => {
  const s = new Server();
  s.d1.db.exec("CREATE TRIGGER boom BEFORE INSERT ON students BEGIN SELECT RAISE(ABORT, 'injected'); END");
  const failed = await s.upload(A, "a", ["아루"]);
  assert.equal(failed.status, 500);
  assert.equal(s.users().length, 0, "the user row was rolled back with the students");
  s.d1.db.exec("DROP TRIGGER boom");
  const retry = await s.upload(A, "a", ["아루"]);
  assert.equal(retry.status, 201, "the retry creates the account, it does not find an empty one");
  assert.equal(s.d1.rows("SELECT * FROM students").length, 1);
});

test("a failing update keeps the previous students", async () => {
  const s = new Server();
  await s.upload(A, "a", ["아루", "하루"]);
  s.d1.db.exec("CREATE TRIGGER boom BEFORE INSERT ON students BEGIN SELECT RAISE(ABORT, 'injected'); END");
  assert.equal((await s.upload(A, "b", ["카린"])).status, 500);
  assert.deepEqual(s.d1.rows("SELECT name FROM students ORDER BY name").map((r) => r.name), ["아루", "하루"]);
  assert.equal(s.users()[0]?.nickname, "a");
});

test("visibility and delete are for the add-on only: no add-on headers or a browser origin changes nothing", async () => {
  const s = new Server();
  await s.upload(A, "a", ["아루"], { public: true });
  const before = JSON.stringify(s.users());
  const jsonOnly = { "content-type": "application/json" };
  const browser = { ...ADDON, origin: "http://srv.test" };
  const fetchMode = { ...ADDON, "sec-fetch-site": "same-origin" };
  for (const path of ["/api/visibility", "/api/delete"]) {
    const body = { account_id: A, public: false };
    assert.equal((await s.call("POST", path, body, jsonOnly)).status, 426, `${path} without the add-on headers`);
    assert.equal((await s.call("POST", path, body, { ...ADDON, "x-amas-protocol": "1" })).status, 426, `${path} with an old protocol`);
    assert.equal((await s.call("POST", path, body, browser)).status, 403, `${path} from a page of this site`);
    assert.equal((await s.call("POST", path, body, { ...ADDON, origin: "http://evil.test" })).status, 403, `${path} from another site`);
    assert.equal((await s.call("POST", path, body, fetchMode)).status, 403, `${path} with browser fetch metadata`);
    assert.equal((await s.call("POST", path, body, { ...ADDON, "content-type": "text/plain" })).status, 400, `${path} with a form content type`);
  }
  assert.equal(JSON.stringify(s.users()), before, "the account is exactly as it was");
  assert.equal(s.d1.rows("SELECT * FROM students").length, 1);
});

test("a body over the size cap is refused with 413 and creates nothing", async () => {
  const s = new Server();
  const big = await s.call("POST", "/api/upload", { account_id: A, nickname: "a", roster: roster("아루"), pad: "x".repeat(3_100_000) });
  assert.equal(big.status, 413);
  const declared = await s.call("POST", "/api/upload", { account_id: A }, { ...ADDON, "content-length": "3000001" });
  assert.equal(declared.status, 413, "a declared length over the cap is refused before the body is read");
  assert.equal((await s.call("POST", "/api/visibility", { account_id: A, pad: "x".repeat(3_100_000) })).status, 413);
  assert.equal(s.users().length, 0);
  assert.equal((await s.upload(A, "a", ["아루"])).status, 201, "a normal upload still works");
});

test("visibility makes the account private unless the request says public: true", async () => {
  const s = new Server();
  await s.upload(A, "a", ["아루"], { public: true });
  for (const body of [{ account_id: A }, { account_id: A, public: false }, { account_id: A, public: null }, { account_id: A, public: "true" }, { account_id: A, public: 1 }]) {
    await s.call("POST", "/api/visibility", { account_id: A, public: true });
    assert.equal(s.users()[0]?.public, 1);
    const r = await s.call("POST", "/api/visibility", body);
    assert.equal(r.json.public, false, JSON.stringify(body));
    assert.equal(s.users()[0]?.public, 0, `stored private for ${JSON.stringify(body)}`);
  }
  assert.equal((await s.call("POST", "/api/visibility", { account_id: A, public: true })).json.public, true);
});

/** A signed request built by hand, so a test can break one part of it. */
async function signed(path: string, body: unknown, over: { ts?: number; key?: string; path?: string; body?: unknown } = {}) {
  const payload = JSON.stringify(body);
  const ts = String(over.ts ?? Math.floor(Date.now() / 1000));
  const sig = await addonSignature(over.key ?? SIGNING_KEY, ts, over.path ?? path, new TextEncoder().encode(JSON.stringify(over.body ?? body)));
  return { payload, headers: { ...ADDON, "x-amas-ts": ts, "x-amas-sig": sig } };
}

async function post(s: Server, path: string, h: Record<string, string>, payload: string) {
  const env = { DB: s.d1, ASSETS: { fetch: async () => new Response("asset") }, KEY_PEPPER: PEPPER, ...s.signing } as never;
  const res = await (worker as { fetch(r: Request, e: never): Promise<Response> }).fetch(new Request("http://srv.test" + path, { method: "POST", headers: h, body: payload }), env);
  const text = await res.text();
  return { status: res.status, json: text.startsWith("{") ? JSON.parse(text) : null };
}

test("a request with the add-on headers but no signature is refused: the headers alone prove nothing", async () => {
  const s = new Server();
  s.sign = false;
  for (const path of ["/api/upload", "/api/visibility", "/api/delete"]) {
    const r = await s.call("POST", path, { account_id: A, nickname: "a", roster: roster("아루") }, ADDON);
    assert.equal(r.status, 401, `${path} without a signature`);
    assert.match(r.json.error, /서명/);
  }
  assert.equal(s.users().length, 0, "nothing was written");
});

test("only a signature made with the server key is accepted", async () => {
  const s = new Server();
  const body = { account_id: A, nickname: "a", roster: roster("아루") };
  const good = await signed("/api/upload", body);
  assert.equal((await post(s, "/api/upload", good.headers, good.payload)).status, 201);
  const wrongKey = await signed("/api/upload", body, { key: "another-key-0123456789abcdef" });
  assert.equal((await post(s, "/api/upload", wrongKey.headers, wrongKey.payload)).status, 401, "a different key");
  const otherPath = await signed("/api/upload", body, { path: "/api/delete" });
  assert.equal((await post(s, "/api/upload", otherPath.headers, otherPath.payload)).status, 401, "a signature for another endpoint");
  const tampered = await signed("/api/upload", body, { body: { ...body, nickname: "someone else" } });
  assert.equal((await post(s, "/api/upload", tampered.headers, tampered.payload)).status, 401, "a body changed after signing");
  const junk = { ...good.headers, "x-amas-sig": "0".repeat(64) };
  assert.equal((await post(s, "/api/upload", junk, good.payload)).status, 401, "a made-up signature");
  assert.equal((await post(s, "/api/upload", { ...good.headers, "x-amas-sig": "xyz" }, good.payload)).status, 401, "a malformed signature");
});

test("a signed request is only good for five minutes either side of the server clock", async () => {
  const s = new Server();
  const body = { account_id: A, nickname: "a", roster: roster("아루") };
  const now = Math.floor(Date.now() / 1000);
  for (const [label, ts, want] of [["old", now - SIGNATURE_WINDOW_SECONDS - 30, 401], ["future", now + SIGNATURE_WINDOW_SECONDS + 30, 401], ["fresh", now - 60, 201]] as const) {
    const r = await signed("/api/upload", body, { ts });
    const res = await post(s, "/api/upload", r.headers, r.payload);
    assert.equal(res.status, want, label);
    if (want === 401) assert.match(res.json.error, /시각/);
  }
});

test("the server fails closed: no signing key configured refuses every add-on request", async () => {
  const s = new Server();
  s.signing = {};
  const r = await s.call("POST", "/api/upload", { account_id: A, nickname: "a", roster: roster("아루") });
  assert.equal(r.status, 500);
  assert.match(r.json.error, /서명 키/);
  assert.equal(s.users().length, 0);
});

test("a key that is too short counts as no key", async () => {
  const s = new Server();
  s.signing = { ADDON_SIGNING_KEY: "short" };
  assert.equal((await s.call("POST", "/api/upload", { account_id: A, nickname: "a", roster: roster("아루") })).status, 500);
});

test("the previous key is accepted while a new one is rolled out", async () => {
  const s = new Server();
  s.signing = { ADDON_SIGNING_KEY: "the-new-key-0123456789abcdef", ADDON_SIGNING_KEY_PREV: SIGNING_KEY };
  const body = { account_id: A, nickname: "a", roster: roster("아루") };
  const old = await signed("/api/upload", body);
  assert.equal((await post(s, "/api/upload", old.headers, old.payload)).status, 201, "signed with the previous key");
  const fresh = await signed("/api/upload", body, { key: "the-new-key-0123456789abcdef" });
  assert.equal((await post(s, "/api/upload", fresh.headers, fresh.payload)).status, 200, "signed with the new key");
});

test("unsigned requests are only accepted when local development says so", async () => {
  const s = new Server();
  s.sign = false;
  s.signing = { ALLOW_UNSIGNED_ADDON: "1" };
  assert.equal((await s.call("POST", "/api/upload", { account_id: A, nickname: "a", roster: roster("아루") }, ADDON)).status, 201);
  const t = new Server();
  t.sign = false;
  t.signing = { ALLOW_UNSIGNED_ADDON: "true", ADDON_SIGNING_KEY: SIGNING_KEY };
  assert.equal((await t.call("POST", "/api/upload", { account_id: A, nickname: "a", roster: roster("아루") }, ADDON)).status, 401, "only the exact value 1 opens it");
});

test("the signature does not replace the other checks: a browser origin and an old protocol are still refused", async () => {
  const s = new Server();
  const body = { account_id: A, nickname: "a", roster: roster("아루") };
  const good = await signed("/api/upload", body);
  assert.equal((await post(s, "/api/upload", { ...good.headers, origin: "http://evil.test" }, good.payload)).status, 403);
  assert.equal((await post(s, "/api/upload", { ...good.headers, "x-amas-protocol": "1" }, good.payload)).status, 426);
});
