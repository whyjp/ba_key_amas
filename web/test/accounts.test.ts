import assert from "node:assert/strict";
import { test } from "node:test";
import { prepareWorkerImport, MemoryD1 } from "./helpers/d1.ts";
import { MIN_PROTOCOL, PROTOCOL } from "../src/version.ts";

prepareWorkerImport();
const { default: worker } = await import("../src/index.ts");

const A = "a1".repeat(32);
const B = "b2".repeat(32);
const PEPPER = "test-pepper";
const ADDON = { "content-type": "application/json", "x-amas-protocol": String(PROTOCOL), "x-amas-addon": "0.1.0" };

function roster(...names: string[]) {
  return { schema_version: 3, entries: names.map((name) => ({ student: { name, level: 80, rarity_stars: 3 } })) };
}

class Server {
  d1 = new MemoryD1();
  logs: string[] = [];
  pepper: string;
  constructor(pepper = PEPPER) {
    this.pepper = pepper;
  }
  async call(method: string, path: string, body?: unknown, headers: Record<string, string> = ADDON): Promise<{ status: number; text: string; json: any }> {
    const env = { DB: this.d1, ASSETS: { fetch: async () => new Response("asset") }, KEY_PEPPER: this.pepper } as never;
    const orig = [console.error, console.log, console.warn];
    console.error = console.log = console.warn = (...a: unknown[]) => void this.logs.push(a.map(String).join(" "));
    try {
      const req = new Request("http://srv.test" + path, { method, headers: method === "GET" ? {} : headers, body: body === undefined ? undefined : JSON.stringify(body) });
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
