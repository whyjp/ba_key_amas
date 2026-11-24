import catalog from "./catalog.json";
import { formatKey, foldNickname, hashKey, keyMatches, newTag, normalizeNickname, parseKey, type ParsedKey } from "./keys.ts";
import { BadJson, json, readJson, sameOriginJson, TooLarge } from "./security.ts";
import { Invalid, MAX_BODY_BYTES, parseUpload, type StudentRow, type Upload } from "./validate.ts";

interface Env {
  DB: D1Database;
  ASSETS: Fetcher;
  /** `wrangler secret put KEY_PEPPER`: keys are stored only as HMACs under this secret. */
  KEY_PEPPER: string;
  LIMITER?: RateLimit;
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
}

const MAX_FAILS = 5;
const LOCK_SECONDS = 15 * 60;
const CATALOG = catalog as Record<string, { id: number; school: string | null; attack: string | null; armor: string | null; squad: string | null }>;
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

function studentStatements(env: Env, userId: number, students: StudentRow[]): D1PreparedStatement[] {
  const ins = env.DB.prepare(
    "INSERT INTO students (user_id, name, student_id, level, stars, ex_skill, normal_skill, passive_skill, sub_skill, bond, uw_stars, uw_level, role, pinned) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
  );
  return students.map((s) =>
    ins.bind(userId, s.name, CATALOG[s.name]?.id ?? null, s.level, s.stars, s.ex_skill, s.normal_skill, s.passive_skill, s.sub_skill, s.bond, s.uw_stars, s.uw_level, s.role, s.pinned === null ? null : Number(s.pinned)),
  );
}

async function upload(env: Env, up: Upload): Promise<Response> {
  const nickname = normalizeNickname(up.nickname);
  if (!nickname) return json(400, { error: "닉네임은 1~16자이며 # 과 제어 문자를 쓸 수 없습니다" });
  const nick = foldNickname(nickname);
  const existing = await env.DB.prepare("SELECT id FROM users WHERE nick_norm = ?1").bind(nick).first<{ id: number }>();
  const accountJson = up.account ? JSON.stringify(up.account) : null;
  const progressJson = up.progress ? JSON.stringify(up.progress) : null;
  if (existing) {
    const auth = await authenticate(env, up.key);
    if (!auth.ok) return json(409, { error: "이미 쓰이는 닉네임입니다. 올린 적이 있다면 key 를 함께 보내세요" });
    await env.DB.batch([
      env.DB.prepare("UPDATE users SET account_json = ?1, progress_json = ?2, updated_at = ?3 WHERE id = ?4").bind(accountJson, progressJson, now(), auth.user.id),
      env.DB.prepare("DELETE FROM students WHERE user_id = ?1").bind(auth.user.id),
      ...studentStatements(env, auth.user.id, up.students),
    ]);
    return json(200, { key: formatKey(auth.key), created: false, students: up.students.length });
  }
  const key: ParsedKey = { nickname, tag: newTag() };
  const hash = await hashKey(env.KEY_PEPPER, key);
  const t = now();
  try {
    const res = await env.DB.prepare("INSERT INTO users (nick_norm, nickname, key_hash, account_json, progress_json, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?6)").bind(nick, nickname, hash, accountJson, progressJson, t).run();
    const id = res.meta.last_row_id;
    await env.DB.batch(studentStatements(env, id, up.students));
  } catch {
    return json(409, { error: "이미 쓰이는 닉네임입니다" });
  }
  return json(201, { key: formatKey(key), created: true, students: up.students.length });
}

async function profile(env: Env, user: UserRow): Promise<Response> {
  const rows = await env.DB.prepare("SELECT * FROM students WHERE user_id = ?1").bind(user.id).all();
  const students = rows.results.map((r) => {
    const c = CATALOG[String(r.name)];
    return { ...r, user_id: undefined, school: c?.school ?? null, attack: c?.attack ?? null, armor: c?.armor ?? null };
  });
  return json(200, {
    nickname: user.nickname,
    updated_at: user.updated_at,
    account: user.account_json ? JSON.parse(user.account_json) : null,
    progress: user.progress_json ? JSON.parse(user.progress_json) : null,
    students,
  });
}

async function handleApi(req: Request, env: Env, path: string): Promise<Response> {
  if (req.method !== "POST") return json(405, { error: "POST 만 받습니다" }, { allow: "POST" });
  const refused = sameOriginJson(req);
  if (refused) return json(400, { error: refused });
  if (env.LIMITER) {
    const ip = req.headers.get("cf-connecting-ip") ?? "unknown";
    if (!(await env.LIMITER.limit({ key: ip })).success) return json(429, { error: "요청이 너무 많습니다" }, { "retry-after": "60" });
  }
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
        return await upload(env, parseUpload(body));
      } catch (e) {
        if (e instanceof Invalid) return json(400, { error: e.message });
        throw e;
      }
    case "/api/profile": {
      const a = await authenticate(env, key);
      return a.ok ? profile(env, a.user) : a.res;
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
