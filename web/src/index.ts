import backgrounds from "./backgrounds.json";
import catalog from "./catalog.json";
import { entryFor, indexByName, type Catalog } from "./catalog.ts";
import { formatKey, foldNickname, hashKey, keyMatches, newTag, normalizeNickname, parseKey, type ParsedKey } from "./keys.ts";
import { BadJson, json, readJson, sameOriginJson, TooLarge } from "./security.ts";
import { aggregate, analyze, SCORE_VERSION, summarize, type Analysis, type CharSummary, type OwnedStudent } from "./score.ts";
import { checkClient, checkSchemas, MIN_PROTOCOL, PROTOCOL, SERVER_VERSION, type Client } from "./version.ts";
import { Invalid, MAX_BODY_BYTES, parseUpload, type StudentRow, type Upload } from "./validate.ts";

interface Env {
  DB: D1Database;
  ASSETS: Fetcher;
  /** `wrangler secret put KEY_PEPPER`: keys are stored only as HMACs under this secret. */
  KEY_PEPPER: string;
  LIMITER?: RateLimit;
  /** Where the add-on is installed from (a GitHub release page). Not decided yet, so it is a variable. */
  ADDON_URL?: string;
}

interface UserRow {
  id: number;
  nickname: string;
  key_hash: string;
  fail_count: number;
  locked_until: number;
  account_json: string | null;
  progress_json: string | null;
  updated_at: number;
  public: number;
}

const DEFAULT_ADDON_URL = "https://github.com/whyjp/ba_key_amas/releases/latest";
const ADDON_ONLY = new Set(["/api/upload", "/api/visibility", "/api/rekey", "/api/delete"]);
const MAX_FAILS = 5;
const LOCK_SECONDS = 15 * 60;
const CATALOG = catalog as Catalog;
const BY_NAME = indexByName(CATALOG);
const studentEntry = (name: string, id: number | null) => entryFor(CATALOG, BY_NAME, name, id);
const now = () => Math.floor(Date.now() / 1000);

type Auth = { ok: true; user: UserRow; key: ParsedKey } | { ok: false; res: Response };

/** One failure message for a wrong tag and an unknown nickname, so a nickname cannot be probed. */
async function authenticate(env: Env, raw: unknown): Promise<Auth> {
  const bad = { ok: false as const, res: json(403, { error: "키가 맞지 않습니다" }) };
  const key = typeof raw === "string" ? parseKey(raw) : null;
  if (!key) return bad;
  const user = await env.DB.prepare("SELECT * FROM users WHERE nick_norm = ?1").bind(foldNickname(key.nickname)).first<UserRow>();
  if (!user) return bad;
  if (user.locked_until > now()) return { ok: false, res: json(429, { error: "잠시 후 다시 시도하세요" }, { "retry-after": String(user.locked_until - now()) }) };
  if (!(await keyMatches(env.KEY_PEPPER, key, user.key_hash))) {
    const fails = user.fail_count + 1;
    await env.DB.prepare("UPDATE users SET fail_count = ?1, locked_until = ?2 WHERE id = ?3").bind(fails >= MAX_FAILS ? 0 : fails, fails >= MAX_FAILS ? now() + LOCK_SECONDS : 0, user.id).run();
    return bad;
  }
  if (user.fail_count > 0) await env.DB.prepare("UPDATE users SET fail_count = 0 WHERE id = ?1").bind(user.id).run();
  return { ok: true, user, key };
}

const flag = (b: boolean | null): number | null => (b === null ? null : Number(b));

function owned(s: StudentRow): OwnedStudent {
  return { ...s, student_id: studentEntry(s.name, s.student_id)?.id ?? null };
}

/** The leaderboard row stored with a user: the score, the count columns and the three best characters. */
function boardColumns(a: Analysis) {
  const top = a.top.map((c) => ({ base: c.base, total: c.total, outfits: c.outfits.slice(0, 4) }));
  return { score: a.score, stats: JSON.stringify(a.stats), top: JSON.stringify(top), chars: JSON.stringify(summarize(a)) };
}

const BACKGROUNDS = backgrounds as string[];

interface CardBackground {
  bg: string;
  third: number;
}

/** A random background and a random third of it (left, centre, right), so cards of one teacher do not all look alike.
 *  The server picks them, so the page only ever shows names from this list. */
function randomBackground(): CardBackground | null {
  if (BACKGROUNDS.length === 0) return null;
  const n = new Uint32Array(2);
  crypto.getRandomValues(n);
  return { bg: BACKGROUNDS[(n[0] ?? 0) % BACKGROUNDS.length] as string, third: (n[1] ?? 0) % 3 };
}

function studentStatements(env: Env, userId: number, students: StudentRow[], kept: Map<string, CardBackground> = new Map()): D1PreparedStatement[] {
  const ins = env.DB.prepare(
    "INSERT INTO students (user_id, name, student_id, level, stars, ex_skill, normal_skill, passive_skill, sub_skill, bond, uw_stars, uw_level, role, pinned, memorial, detail_json, bg, bg_third) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
  );
  return students.map((s) => {
    const card = kept.get(s.name) ?? randomBackground();
    return ins.bind(userId, s.name, studentEntry(s.name, s.student_id)?.id ?? null, s.level, s.stars, s.ex_skill, s.normal_skill, s.passive_skill, s.sub_skill, s.bond, s.uw_stars, s.uw_level, s.role, flag(s.pinned), flag(s.memorial), s.detail ? JSON.stringify(s.detail) : null, card?.bg ?? null, card?.third ?? null);
  });
}

async function upload(env: Env, up: Upload, client: Client): Promise<Response> {
  const nickname = normalizeNickname(up.nickname);
  if (!nickname) return json(400, { error: "닉네임은 1~16자이며 # 과 제어 문자를 쓸 수 없습니다" });
  const nick = foldNickname(nickname);
  const existing = await env.DB.prepare("SELECT id FROM users WHERE nick_norm = ?1").bind(nick).first<{ id: number }>();
  const board = boardColumns(analyze(up.students.map(owned)));
  const accountJson = up.account ? JSON.stringify(up.account) : null;
  const progressJson = up.progress ? JSON.stringify(up.progress) : null;
  if (existing) {
    const auth = await authenticate(env, up.key);
    if (!auth.ok) return json(409, { error: "이미 쓰이는 닉네임입니다. 올린 적이 있다면 key 를 함께 보내세요" });
    // a student who was uploaded before keeps the background the card already has
    const before = await env.DB.prepare("SELECT name, bg, bg_third FROM students WHERE user_id = ?1 AND bg IS NOT NULL AND bg_third IS NOT NULL").bind(auth.user.id).all<{ name: string; bg: string; bg_third: number }>();
    const kept = new Map(before.results.map((r) => [r.name, { bg: r.bg, third: r.bg_third }]));
    await env.DB.batch([
      env.DB.prepare("UPDATE users SET account_json = ?1, progress_json = ?2, updated_at = ?3, public = ?4, score = ?5, stats_json = ?6, top_json = ?7, score_version = ?8, addon_version = ?9, protocol = ?10, chars_json = ?11 WHERE id = ?12").bind(accountJson, progressJson, now(), Number(up.public), board.score, board.stats, board.top, SCORE_VERSION, client.addon, client.protocol, board.chars, auth.user.id),
      env.DB.prepare("DELETE FROM students WHERE user_id = ?1").bind(auth.user.id),
      ...studentStatements(env, auth.user.id, up.students, kept),
    ]);
    return json(200, { key: formatKey(auth.key), created: false, students: up.students.length, public: up.public, score: board.score });
  }
  const key: ParsedKey = { nickname, tag: newTag() };
  const hash = await hashKey(env.KEY_PEPPER, key);
  const t = now();
  try {
    const res = await env.DB.prepare("INSERT INTO users (nick_norm, nickname, key_hash, account_json, progress_json, created_at, updated_at, public, score, stats_json, top_json, score_version, addon_version, protocol, chars_json) VALUES (?1,?2,?3,?4,?5,?6,?6,?7,?8,?9,?10,?11,?12,?13,?14)").bind(nick, nickname, hash, accountJson, progressJson, t, Number(up.public), board.score, board.stats, board.top, SCORE_VERSION, client.addon, client.protocol, board.chars).run();
    const id = res.meta.last_row_id;
    await env.DB.batch(studentStatements(env, id, up.students));
  } catch {
    return json(409, { error: "이미 쓰이는 닉네임입니다" });
  }
  return json(201, { key: formatKey(key), created: true, students: up.students.length, public: up.public, score: board.score });
}

function ownedFromRows(rows: Record<string, unknown>[]): OwnedStudent[] {
  return rows.map((r) => ({
    name: String(r.name), student_id: (r.student_id as number | null) ?? null, level: r.level as number | null, stars: r.stars as number | null,
    ex_skill: r.ex_skill as number | null, normal_skill: r.normal_skill as number | null, passive_skill: r.passive_skill as number | null, sub_skill: r.sub_skill as number | null,
    bond: r.bond as number | null, uw_stars: r.uw_stars as number | null, uw_level: r.uw_level as number | null,
    pinned: r.pinned === null ? null : Boolean(r.pinned), memorial: r.memorial === null ? null : Boolean(r.memorial),
  }));
}

async function profile(env: Env, user: UserRow, view: "owner" | "public"): Promise<Response> {
  const rows = await env.DB.prepare("SELECT * FROM students WHERE user_id = ?1").bind(user.id).all();
  const students = rows.results.map((r) => {
    const c = studentEntry(String(r.name), (r.student_id as number | null) ?? null);
    const detail = typeof r.detail_json === "string" ? JSON.parse(r.detail_json) : null;
    return { ...r, user_id: undefined, detail_json: undefined, detail, school: c?.school ?? null, attack: c?.attack ?? null, armor: c?.armor ?? null, tactic: c?.role ?? null, position: c?.position ?? null, equipment_slots: c?.equipment ?? null };
  });
  const analysis = analyze(ownedFromRows(rows.results));
  return json(200, {
    nickname: user.nickname,
    updated_at: user.updated_at,
    public: Boolean(user.public),
    view,
    account: view === "owner" && user.account_json ? JSON.parse(user.account_json) : null,
    progress: user.progress_json ? JSON.parse(user.progress_json) : null,
    students,
    analysis: {
      score: analysis.score,
      stats: analysis.stats,
      top: analysis.top.map((c) => ({ base: c.base, total: c.total, outfits: c.outfits })),
      characters: analysis.characters.map((c) => ({ base: c.base, total: c.total, outfits: c.outfits })),
    },
  });
}

/** The view a leaderboard row links to: only users who chose to be public, and without the money counters. */
async function publicProfile(env: Env, nickname: string): Promise<Response> {
  const nick = normalizeNickname(nickname);
  const user = nick ? await env.DB.prepare("SELECT * FROM users WHERE nick_norm = ?1 AND public = 1").bind(foldNickname(nick)).first<UserRow>() : null;
  return user ? profile(env, user, "public") : json(404, { error: "공개된 유저가 아닙니다" });
}

/**
 * Every public teacher's characters added up (outfits merged by name). A teacher uploaded before the characters were
 * kept gets them computed here once and stored. Only owners and totals leave, never a single student's own stats.
 */
async function communityStudents(env: Env): Promise<Response> {
  const users = await env.DB.prepare("SELECT id, chars_json FROM users WHERE public = 1").all<{ id: number; chars_json: string | null }>();
  const lists: CharSummary[][] = [];
  for (const u of users.results) {
    let chars = u.chars_json;
    if (!chars) {
      const rows = await env.DB.prepare("SELECT * FROM students WHERE user_id = ?1").bind(u.id).all();
      chars = JSON.stringify(summarize(analyze(ownedFromRows(rows.results))));
      await env.DB.prepare("UPDATE users SET chars_json = ?1 WHERE id = ?2").bind(chars, u.id).run();
    }
    lists.push(JSON.parse(chars) as CharSummary[]);
  }
  return json(200, { teachers: users.results.length, characters: aggregate(lists) }, { "cache-control": "public, max-age=60" });
}

/** Public users only, best first. Reads columns stored at upload, so it never touches the students table. */
async function leaderboard(env: Env): Promise<Response> {
  const rows = await env.DB.prepare("SELECT nickname, score, stats_json, top_json, updated_at FROM users WHERE public = 1 ORDER BY score DESC, updated_at ASC LIMIT 100").all<{
    nickname: string; score: number; stats_json: string | null; top_json: string | null; updated_at: number;
  }>();
  const entries = rows.results.map((r, i) => ({
    rank: i + 1, nickname: r.nickname, score: r.score, updated_at: r.updated_at,
    stats: r.stats_json ? JSON.parse(r.stats_json) : null, top: r.top_json ? JSON.parse(r.top_json) : [],
  }));
  return json(200, { entries }, { "cache-control": "public, max-age=60" });
}

async function limited(req: Request, env: Env): Promise<Response | null> {
  if (!env.LIMITER) return null;
  const ip = req.headers.get("cf-connecting-ip") ?? "unknown";
  return (await env.LIMITER.limit({ key: ip })).success ? null : json(429, { error: "요청이 너무 많습니다" }, { "retry-after": "60" });
}

async function handleApi(req: Request, env: Env, path: string): Promise<Response> {
  if (req.method === "GET" && path === "/api/leaderboard") return (await limited(req, env)) ?? leaderboard(env);
  if (req.method === "GET" && path === "/api/leaderboard/students") return (await limited(req, env)) ?? communityStudents(env);
  if (req.method === "GET" && path === "/api/version") return json(200, { server: SERVER_VERSION, protocol: PROTOCOL, min_protocol: MIN_PROTOCOL, download: env.ADDON_URL ?? DEFAULT_ADDON_URL });
  if (req.method === "GET" && path === "/api/public") return (await limited(req, env)) ?? publicProfile(env, new URL(req.url).searchParams.get("nickname") ?? "");
  if (req.method !== "POST") return json(405, { error: "POST 만 받습니다" }, { allow: "POST, GET" });
  let client: Client | null = null;
  // what changes an account (upload, public or private, new key, delete) is for the add-on only
  if (ADDON_ONLY.has(path)) {
    const c = checkClient(req.headers);
    if ("status" in c) return json(c.status, { error: c.error, server: SERVER_VERSION, protocol: PROTOCOL, min_protocol: MIN_PROTOCOL, download: env.ADDON_URL ?? DEFAULT_ADDON_URL });
    client = c;
    if (!(req.headers.get("content-type") ?? "").toLowerCase().startsWith("application/json")) return json(400, { error: "Content-Type 은 application/json 이어야 합니다" });
  } else {
    const refused = sameOriginJson(req);
    if (refused) return json(400, { error: refused });
  }
  const tooMany = await limited(req, env);
  if (tooMany) return tooMany;
  let body: unknown;
  try {
    body = await readJson(req, MAX_BODY_BYTES);
  } catch (e) {
    return e instanceof TooLarge ? json(413, { error: "본문이 너무 큽니다" }) : json(400, { error: "JSON 이 아닙니다" });
  }
  const key = (body as { key?: unknown } | null)?.key;
  switch (path) {
    case "/api/upload":
      try {
        const up = parseUpload(body);
        const old = checkSchemas((body as { roster?: unknown }).roster, up.progress);
        if (old) return json(old.status, { error: old.error, server: SERVER_VERSION, download: env.ADDON_URL ?? DEFAULT_ADDON_URL });
        return await upload(env, up, client as Client);
      } catch (e) {
        if (e instanceof Invalid) return json(400, { error: e.message });
        throw e;
      }
    case "/api/profile": {
      const a = await authenticate(env, key);
      return a.ok ? profile(env, a.user, "owner") : a.res;
    }
    case "/api/visibility": {
      const a = await authenticate(env, key);
      if (!a.ok) return a.res;
      const on = (body as { public?: unknown }).public === true;
      await env.DB.prepare("UPDATE users SET public = ?1 WHERE id = ?2").bind(Number(on), a.user.id).run();
      return json(200, { public: on });
    }
    case "/api/rekey": {
      const a = await authenticate(env, key);
      if (!a.ok) return a.res;
      const next: ParsedKey = { nickname: a.user.nickname, tag: newTag() };
      await env.DB.prepare("UPDATE users SET key_hash = ?1, fail_count = 0, locked_until = 0 WHERE id = ?2").bind(await hashKey(env.KEY_PEPPER, next), a.user.id).run();
      return json(200, { key: formatKey(next) });
    }
    case "/api/delete": {
      const a = await authenticate(env, key);
      if (!a.ok) return a.res;
      await env.DB.batch([env.DB.prepare("DELETE FROM students WHERE user_id = ?1").bind(a.user.id), env.DB.prepare("DELETE FROM users WHERE id = ?1").bind(a.user.id)]);
      return json(200, { deleted: true });
    }
    default:
      return json(404, { error: "없는 경로입니다" });
  }
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const path = new URL(req.url).pathname;
    if (path.startsWith("/api/")) {
      try {
        return await handleApi(req, env, path);
      } catch (e) {
        if (e instanceof BadJson) return json(400, { error: "JSON 이 아닙니다" });
        console.error(e);
        return json(500, { error: "서버 오류" });
      }
    }
    return env.ASSETS.fetch(req);
  },
} satisfies ExportedHandler<Env>;
