export const API_HEADERS: Record<string, string> = {
  "content-type": "application/json; charset=utf-8",
  "cache-control": "no-store",
  "x-content-type-options": "nosniff",
  "referrer-policy": "no-referrer",
  "cross-origin-resource-policy": "same-origin",
};

export function json(status: number, body: unknown, extra: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), { status, headers: { ...API_HEADERS, ...extra } });
}

/**
 * A browser sends Origin on every cross-site POST, so a request without one is not from a page (the scanner).
 * The body must be JSON, which a cross-site form cannot send without a preflight this server never answers.
 */
export function sameOriginJson(req: Request): string | null {
  const origin = req.headers.get("origin");
  if (origin && origin !== new URL(req.url).origin) return "다른 출처의 요청입니다";
  const ct = req.headers.get("content-type") ?? "";
  if (!ct.toLowerCase().startsWith("application/json")) return "Content-Type 은 application/json 이어야 합니다";
  return null;
}

export class TooLarge extends Error {}
export class BadJson extends Error {}

/** The request body as bytes, refused when it is larger than `maxBytes` (declared or actual). */
export async function readBody(req: Request, maxBytes: number): Promise<Uint8Array> {
  if (Number(req.headers.get("content-length") ?? "0") > maxBytes) throw new TooLarge();
  const buf = await req.arrayBuffer();
  if (buf.byteLength > maxBytes) throw new TooLarge();
  return new Uint8Array(buf);
}

export function parseJsonBytes(bytes: Uint8Array): unknown {
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true, ignoreBOM: false }).decode(bytes));
  } catch {
    throw new BadJson();
  }
}

export async function readJson(req: Request, maxBytes: number): Promise<unknown> {
  return parseJsonBytes(await readBody(req, maxBytes));
}

const enc = new TextEncoder();
const toHex = (b: ArrayBuffer) => [...new Uint8Array(b)].map((x) => x.toString(16).padStart(2, "0")).join("");

/** How far the add-on's clock may differ from the server's. A signed request is useless to replay after this. */
export const SIGNATURE_WINDOW_SECONDS = 300;

/**
 * The signature the official add-on puts on every request that changes an account:
 * HMAC-SHA256(key, `${ts}
${path}
${sha256(body)}`) in hex. The key is compiled into the official release build
 * from a CI secret and is not in the source, so a client built from the source cannot produce it.
 */
export async function addonSignature(key: string, ts: string, path: string, body: Uint8Array): Promise<string> {
  const digest = toHex(await crypto.subtle.digest("SHA-256", body));
  const k = await crypto.subtle.importKey("raw", enc.encode(key), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return toHex(await crypto.subtle.sign("HMAC", k, enc.encode(`${ts}
${path}
${digest}`)));
}

function sameBytes(a: string, b: string): boolean {
  const x = enc.encode(a);
  const y = enc.encode(b);
  if (x.length !== y.length) return false;
  let d = 0;
  for (let i = 0; i < x.length; i++) d |= (x[i] ?? 0) ^ (y[i] ?? 0);
  return d === 0;
}

export interface SigningEnv {
  ADDON_SIGNING_KEY?: string;
  /** The previous key, accepted while a new add-on release is rolled out. */
  ADDON_SIGNING_KEY_PREV?: string;
  /** "1" only for local development: accept unsigned requests. Never set it in production. */
  ALLOW_UNSIGNED_ADDON?: string;
}

/**
 * Null when the request carries a valid signature of the add-on (or unsigned requests are explicitly allowed for
 * local development). Fails closed: a server with no signing key refuses everything instead of accepting everything.
 */
export async function verifyAddonSignature(env: SigningEnv, h: Headers, path: string, body: Uint8Array, nowSeconds = Math.floor(Date.now() / 1000)): Promise<{ status: number; error: string } | null> {
  if (env.ALLOW_UNSIGNED_ADDON === "1") return null;
  const keys = [env.ADDON_SIGNING_KEY, env.ADDON_SIGNING_KEY_PREV].filter((k): k is string => typeof k === "string" && k.length >= 16);
  if (keys.length === 0) return { status: 500, error: "서버에 애드온 서명 키가 설정되어 있지 않습니다" };
  const ts = h.get("x-amas-ts") ?? "";
  const sig = (h.get("x-amas-sig") ?? "").toLowerCase();
  if (!/^\d{9,12}$/.test(ts) || !/^[0-9a-f]{64}$/.test(sig)) return { status: 401, error: "공식 ba-key-AMAS 애드온에서만 보낼 수 있는 요청입니다 (서명이 없습니다)" };
  if (Math.abs(nowSeconds - Number(ts)) > SIGNATURE_WINDOW_SECONDS) return { status: 401, error: "요청 시각이 서버와 5분 넘게 다릅니다. PC의 시계를 확인하세요" };
  for (const key of keys) {
    if (sameBytes(await addonSignature(key, ts, path, body), sig)) return null;
  }
  return { status: 401, error: "공식 ba-key-AMAS 애드온에서만 보낼 수 있는 요청입니다 (서명이 맞지 않습니다)" };
}
