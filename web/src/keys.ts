// The key is `nickname#hex6`. The nickname is unique (case and width folded). The tag is random and the
// whole key is the secret, so it is never stored, only its HMAC.

const TAG_RE = /^[0-9a-f]{6}$/;
const FORBIDDEN = /[\p{C}\p{Zl}\p{Zp}#]/u; // controls, format and bidi characters, line separators, '#'

export interface ParsedKey {
  nickname: string;
  tag: string;
}

export function normalizeNickname(raw: string): string | null {
  const nick = raw.normalize("NFC").trim();
  const len = [...nick].length;
  if (len < 1 || len > 16 || FORBIDDEN.test(nick)) return null;
  return nick;
}

/** The duplicate-check form: nicknames that fold to the same string are the same user. */
export function foldNickname(nick: string): string {
  return nick.normalize("NFKC").toLowerCase();
}

export function parseKey(raw: string): ParsedKey | null {
  const i = raw.lastIndexOf("#");
  if (i < 0) return null;
  const nickname = normalizeNickname(raw.slice(0, i));
  const tag = raw.slice(i + 1).trim().toLowerCase();
  if (!nickname || !TAG_RE.test(tag)) return null;
  return { nickname, tag };
}

export function formatKey(k: ParsedKey): string {
  return `${k.nickname}#${k.tag}`;
}

export function newTag(): string {
  const b = new Uint8Array(3);
  crypto.getRandomValues(b);
  return [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
}

const enc = new TextEncoder();

function hex(buf: ArrayBuffer): string {
  return [...new Uint8Array(buf)].map((x) => x.toString(16).padStart(2, "0")).join("");
}

/** HMAC of the folded nickname plus tag, so `Soda#abc123` and `soda#abc123` are one key. */
export async function hashKey(pepper: string, k: ParsedKey): Promise<string> {
  const key = await crypto.subtle.importKey("raw", enc.encode(pepper), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return hex(await crypto.subtle.sign("HMAC", key, enc.encode(`${foldNickname(k.nickname)}#${k.tag}`)));
}

export async function keyMatches(pepper: string, k: ParsedKey, storedHash: string): Promise<boolean> {
  const got = enc.encode(await hashKey(pepper, k));
  const want = enc.encode(storedHash);
  if (got.length !== want.length) return false;
  let d = 0;
  for (let i = 0; i < got.length; i++) d |= (got[i] ?? 0) ^ (want[i] ?? 0);
  return d === 0;
}
