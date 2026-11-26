import backgrounds from "./backgrounds.json";
import catalog from "./catalog.json";
import { entryFor, indexByName, traitsOf, type Catalog } from "./catalog.ts";
import { accountHash, foldNickname, normalizeNickname, parseAccountId, parsePublicId, publicId } from "./keys.ts";
import { BadJson, json, parseJsonBytes, readBody, sameOriginJson, TooLarge, verifyAddonSignature, type SigningEnv } from "./security.ts";
import { aggregate, analyze, SCORE_VERSION, summarize, type Analysis, type CharSummary, type OwnedStudent } from "./score.ts";
import { checkClient, checkSchemas, MIN_PROTOCOL, PROTOCOL, SERVER_VERSION, type Client } from "./version.ts";
import { Invalid, MAX_BODY_BYTES, parseUpload, type StudentRow, type Upload } from "./validate.ts";

interface Env extends SigningEnv {
  DB: D1Database;
  ASSETS: Fetcher;
  /** `wrangler secret put KEY_PEPPER`: account ids are stored only as HMACs under this secret. */
  KEY_PEPPER: string;
  LIMITER?: RateLimit;
  /** A stricter allowance per IP for making a NEW account (updating one only spends LIMITER). */
  CREATE_LIMITER?: RateLimit;
  /** Where the add-on is installed from (a GitHub release page). Not decided yet, so it is a variable. */
  ADDON_URL?: string;
}

interface UserRow {
  id: number;
  public_id: string;
  nickname: string;
  account_json: string | null;
  progress_json: string | null;
  updated_at: number;
  public: number;
}

const DEFAULT_ADDON_URL = "https://github.com/whyjp/ba_key_amas/releases/latest";
const ADDON_ONLY = new Set(["/api/upload", "/api/visibility", "/api/delete"]);
const MAX_MATCHES = 20;
const CATALOG = catalog as Catalog;
const BY_NAME = indexByName(CATALOG);
const studentEntry = (name: string, id: number | null) => entryFor(CATALOG, BY_NAME, name, id);
const BAD_ACCOUNT = "account_id 는 64자리 16진수여야 합니다";
const now = () => Math.floor(Date.now() / 1000);

type Found = { ok: true; user: UserRow } | { ok: false; res: Response };

/**
 * The account the add-on proves it owns by sending its account id. The id is a 256-bit hash of the member code, so
 * there is nothing to lock out: a wrong id is simply an account that does not exist. Only the IP rate limit applies.
 */
async function ownAccount(env: Env, raw: unknown): Promise<Found> {
  const id = parseAccountId(raw);
  if (!id) return { ok: false, res: json(400, { error: BAD_ACCOUNT }) };
  const user = await env.DB.prepare("SELECT * FROM users WHERE account_hash = ?1").bind(await accountHash(env.KEY_PEPPER, id)).first<UserRow>();
  return user ? { ok: true, user } : { ok: false, res: json(404, { error: "이 계정으로 올린 기록이 없습니다" }) };
}

const flag = (b: boolean | null): number | null => (b === null ? null : Number(b));

function owned(s: StudentRow): OwnedStudent {
  const entry = studentEntry(s.name, s.student_id);
  return { ...s, student_id: entry?.id ?? null, base_stars: entry?.star ?? null };
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

/** The rows are tied to their user by the account hash, so they can sit in the same batch as the insert of that user. */
function studentStatements(env: Env, hash: string, students: StudentRow[], kept: Map<string, CardBackground> = new Map()): D1PreparedStatement[] {
  const ins = env.DB.prepare(
    "INSERT INTO students (user_id, name, student_id, level, stars, ex_skill, normal_skill, passive_skill, sub_skill, bond, uw_stars, uw_level, role, pinned, memorial, detail_json, bg, bg_third) VALUES ((SELECT id FROM users WHERE account_hash = ?1),?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
  );
  return students.map((s) => {
    const card = kept.get(s.name) ?? randomBackground();
    return ins.bind(hash, s.name, studentEntry(s.name, s.student_id)?.id ?? null, s.level, s.stars, s.ex_skill, s.normal_skill, s.passive_skill, s.sub_skill, s.bond, s.uw_stars, s.uw_level, s.role, flag(s.pinned), flag(s.memorial), s.detail ? JSON.stringify(s.detail) : null, card?.bg ?? null, card?.third ?? null);
  });
}

async function upload(env: Env, up: Upload, accountId: string, client: Client, ip: string): Promise<Response> {
  const nickname = normalizeNickname(up.nickname);
  if (!nickname) return json(400, { error: "닉네임은 1~16자이며 제어 문자를 쓸 수 없습니다" });
  const search = foldNickname(nickname);
  const hash = await accountHash(env.KEY_PEPPER, accountId);
  const pub = await publicId(env.KEY_PEPPER, accountId);
  const board = boardColumns(analyze(up.students.map(owned)));
  const accountJson = up.account ? JSON.stringify(up.account) : null;
  const progressJson = up.progress ? JSON.stringify(up.progress) : null;
  // a second attempt only happens when another upload of the same account created the row between our read and write
  for (let attempt = 0; ; attempt++) {
    const existing = await env.DB.prepare("SELECT id, public FROM users WHERE account_hash = ?1").bind(hash).first<{ id: number; public: number }>();
    if (existing) {
      // a student who was uploaded before keeps the background the card already has
      const before = await env.DB.prepare("SELECT name, bg, bg_third FROM students WHERE user_id = ?1 AND bg IS NOT NULL AND bg_third IS NOT NULL").bind(existing.id).all<{ name: string; bg: string; bg_third: number }>();
      const kept = new Map(before.results.map((r) => [r.name, { bg: r.bg, third: r.bg_third }]));
      await env.DB.batch([
        env.DB.prepare("UPDATE users SET nickname = ?1, nick_search = ?2, account_json = ?3, progress_json = ?4, updated_at = ?5, public = COALESCE(?6, public), score = ?7, stats_json = ?8, top_json = ?9, score_version = ?10, addon_version = ?11, protocol = ?12, chars_json = ?13 WHERE id = ?14").bind(nickname, search, accountJson, progressJson, now(), up.public === null ? null : Number(up.public), board.score, board.stats, board.top, SCORE_VERSION, client.addon, client.protocol, board.chars, existing.id),
        env.DB.prepare("DELETE FROM students WHERE user_id = ?1").bind(existing.id),
        ...studentStatements(env, hash, up.students, kept),
      ]);
      return json(200, { created: false, public_id: pub, students: up.students.length, public: up.public ?? Boolean(existing.public), score: board.score });
    }
    if (attempt === 0 && env.CREATE_LIMITER && !(await env.CREATE_LIMITER.limit({ key: ip })).success) return json(429, { error: "새 계정을 너무 많이 만들었습니다. 잠시 뒤에 다시 시도해 주세요" }, { "retry-after": "60" });
    try {
      const t = now();
      await env.DB.batch([
        env.DB.prepare("INSERT INTO users (account_hash, public_id, nickname, nick_search, account_json, progress_json, created_at, updated_at, public, score, stats_json, top_json, score_version, addon_version, protocol, chars_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?7,?8,?9,?10,?11,?12,?13,?14,?15)").bind(hash, pub, nickname, search, accountJson, progressJson, t, Number(up.public ?? false), board.score, board.stats, board.top, SCORE_VERSION, client.addon, client.protocol, board.chars),
        ...studentStatements(env, hash, up.students),
      ]);
      return json(201, { created: true, public_id: pub, students: up.students.length, public: up.public ?? false, score: board.score });
    } catch (e) {
      if (attempt >= 1) throw e;
    }
  }
}

function ownedFromRows(rows: Record<string, unknown>[]): OwnedStudent[] {
  return rows.map((r) => ({
    base_stars: studentEntry(String(r.name), (r.student_id as number | null) ?? null)?.star ?? null,
    name: String(r.name), student_id: (r.student_id as number | null) ?? null, level: r.level as number | null, stars: r.stars as number | null,
    ex_skill: r.ex_skill as number | null, normal_skill: r.normal_skill as number | null, passive_skill: r.passive_skill as number | null, sub_skill: r.sub_skill as number | null,
    bond: r.bond as number | null, uw_stars: r.uw_stars as number | null, uw_level: r.uw_level as number | null,
    pinned: r.pinned === null ? null : Boolean(r.pinned), memorial: r.memorial === null ? null : Boolean(r.memorial),
  }));
}

async function profile(env: Env, user: UserRow): Promise<Response> {
  const rows = await env.DB.prepare("SELECT * FROM students WHERE user_id = ?1").bind(user.id).all();
  const students = rows.results.map((r) => {
    const c = studentEntry(String(r.name), (r.student_id as number | null) ?? null);
    const detail = typeof r.detail_json === "string" ? JSON.parse(r.detail_json) : null;
    return { ...r, user_id: undefined, detail_json: undefined, detail, school: c?.school ?? null, attack: c?.attack ?? null, armor: c?.armor ?? null, tactic: c?.role ?? null, position: c?.position ?? null, equipment_slots: c?.equipment ?? null };
  });
  const analysis = analyze(ownedFromRows(rows.results));
  return json(200, {
    nickname: user.nickname,
    public_id: user.public_id,
    updated_at: user.updated_at,
    public: Boolean(user.public),
    progress: user.progress_json ? JSON.parse(user.progress_json) : null,
    students,
    analysis: {
      score: analysis.score,
      stats: analysis.stats,
      top: analysis.top.map((c) => ({ base: c.base, total: c.total, outfits: c.outfits })),
      characters: analysis.characters.map((c) => ({ base: c.base, total: c.total, outfits: c.outfits, ...traitsOf(CATALOG, BY_NAME, c.outfits) })),
    },
  });
}

/**
 * The page a leaderboard row, a search result or the add-on's link opens. The public id is unguessable and never listed
 * for a private account, so knowing it is what lets one open a private account. Credits and AP are never shown.
 */
async function publicProfile(env: Env, id: string): Promise<Response> {
  const pid = parsePublicId(id);
  const user = pid ? await env.DB.prepare("SELECT * FROM users WHERE public_id = ?1").bind(pid).first<UserRow>() : null;
  return user ? profile(env, user) : json(404, { error: "찾을 수 없는 선생님입니다" });
}

/** Public teachers whose nickname starts with the text (folded), the exact name first. Several can share a nickname. */
async function search(env: Env, text: string): Promise<Response> {
  const q = foldNickname(text.trim()).slice(0, 16);
  if (!q) return json(200, { matches: [] });
  const rows = await env.DB.prepare("SELECT public_id, nickname, score, updated_at FROM users WHERE public = 1 AND nick_search >= ?1 AND substr(nick_search, 1, ?2) = ?1 ORDER BY (nick_search = ?1) DESC, score DESC LIMIT ?3").bind(q, [...q].length, MAX_MATCHES).all<{ public_id: string; nickname: string; score: number; updated_at: number }>();
  return json(200, { matches: rows.results });
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
  return json(200, { teachers: users.results.length, characters: aggregate(lists).map((c) => ({ ...c, ...traitsOf(CATALOG, BY_NAME, c.outfits) })) }, { "cache-control": "public, max-age=60" });
}

/** Users the leaderboard last scored with an older rule. */
const RESCORE_BATCH = 20;

/** Scores again, from the stored students, the users whose score was made by an older SCORE_VERSION, so a change of the
 *  rule reaches the leaderboard without every teacher uploading again. A few per request: the rest follow on the next ones. */
async function rescoreStale(env: Env): Promise<void> {
  const stale = await env.DB.prepare("SELECT id FROM users WHERE score_version < ?1 LIMIT ?2").bind(SCORE_VERSION, RESCORE_BATCH).all<{ id: number }>();
  for (const { id } of stale.results) {
    const rows = await env.DB.prepare("SELECT * FROM students WHERE user_id = ?1").bind(id).all();
    const board = boardColumns(analyze(ownedFromRows(rows.results)));
    await env.DB.prepare("UPDATE users SET score = ?1, stats_json = ?2, top_json = ?3, chars_json = ?4, score_version = ?5 WHERE id = ?6").bind(board.score, board.stats, board.top, board.chars, SCORE_VERSION, id).run();
  }
}

/** Public users only, best first. Reads columns stored at upload (or by `rescoreStale`), so it never reads the students table itself. */
async function leaderboard(env: Env): Promise<Response> {
  await rescoreStale(env);
  const rows = await env.DB.prepare("SELECT public_id, nickname, score, stats_json, top_json, updated_at FROM users WHERE public = 1 ORDER BY score DESC, updated_at ASC LIMIT 100").all<{
    public_id: string; nickname: string; score: number; stats_json: string | null; top_json: string | null; updated_at: number;
  }>();
  const entries = rows.results.map((r, i) => ({
    rank: i + 1, public_id: r.public_id, nickname: r.nickname, score: r.score, updated_at: r.updated_at,
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
  if (req.method === "GET" && path === "/api/version") return json(200, { server: SERVER_VERSION, score: SCORE_VERSION, protocol: PROTOCOL, min_protocol: MIN_PROTOCOL, download: env.ADDON_URL ?? DEFAULT_ADDON_URL });
  if (req.method === "GET" && path === "/api/public") return (await limited(req, env)) ?? publicProfile(env, new URL(req.url).searchParams.get("id") ?? "");
  if (req.method === "GET" && path === "/api/search") return (await limited(req, env)) ?? search(env, new URL(req.url).searchParams.get("q") ?? "");
  if (req.method !== "POST") return json(405, { error: "POST 만 받습니다" }, { allow: "POST, GET" });
  let client: Client | null = null;
  // what changes an account (upload, public or private, delete) is for the add-on only
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
    const bytes = await readBody(req, MAX_BODY_BYTES);
    // only the official add-on can sign: the key is in its release build, not in the source
    if (client) {
      const refused = await verifyAddonSignature(env, req.headers, path, bytes);
      if (refused) return json(refused.status, { error: refused.error, server_time: Math.floor(Date.now() / 1000) });
    }
    body = parseJsonBytes(bytes);
  } catch (e) {
    return e instanceof TooLarge ? json(413, { error: "본문이 너무 큽니다" }) : json(400, { error: "JSON 이 아닙니다" });
  }
  const accountId = (body as { account_id?: unknown } | null)?.account_id;
  switch (path) {
    case "/api/upload":
      try {
        const id = parseAccountId(accountId);
        if (!id) return json(400, { error: BAD_ACCOUNT });
        const up = parseUpload(body);
        const old = checkSchemas((body as { roster?: unknown }).roster, up.progress);
        if (old) return json(old.status, { error: old.error, server: SERVER_VERSION, download: env.ADDON_URL ?? DEFAULT_ADDON_URL });
        return await upload(env, up, id, client as Client, req.headers.get("cf-connecting-ip") ?? "unknown");
      } catch (e) {
        if (e instanceof Invalid) return json(400, { error: e.message });
        throw e;
      }
    case "/api/visibility": {
      const a = await ownAccount(env, accountId);
      if (!a.ok) return a.res;
      const on = (body as { public?: unknown }).public === true;
      await env.DB.prepare("UPDATE users SET public = ?1 WHERE id = ?2").bind(Number(on), a.user.id).run();
      return json(200, { public: on, public_id: a.user.public_id });
    }
    case "/api/delete": {
      const a = await ownAccount(env, accountId);
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
