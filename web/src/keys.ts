// An account is identified by `account_id`, which the add-on computes on the player's PC as
// SHA-256("ba-key-amas/account/v1\0" + the game's member code). The member code itself never reaches this server.
// The server hashes the id once more under a secret pepper before it stores it, so a copy of the database
// cannot be used to guess member codes (the code space is small). Nicknames are display text and are not unique.

const FORBIDDEN = /[\p{C}\p{Zl}\p{Zp}]/u; // controls, format and bidi characters, line separators
const ACCOUNT_ID_RE = /^[0-9a-f]{64}$/;
const PUBLIC_ID_RE = /^[0-9a-f]{16}$/;

export function normalizeNickname(raw: string): string | null {
  const nick = raw.normalize("NFC").trim();
  const len = [...nick].length;
  if (len < 1 || len > 16 || FORBIDDEN.test(nick)) return null;
  return nick;
}

/** The search form of a nickname: case and width folded, so "Soda" finds "ＳＯＤＡ". */
export function foldNickname(nick: string): string {
  return nick.normalize("NFKC").toLowerCase();
}

/** The add-on's account id, 64 lower-case hex digits, or null. */
export function parseAccountId(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  const id = raw.trim().toLowerCase();
  return ACCOUNT_ID_RE.test(id) ? id : null;
}

export function parsePublicId(raw: unknown): string | null {
  return typeof raw === "string" && PUBLIC_ID_RE.test(raw) ? raw : null;
}

const enc = new TextEncoder();

function hex(buf: ArrayBuffer): string {
  return [...new Uint8Array(buf)].map((x) => x.toString(16).padStart(2, "0")).join("");
}

async function hmac(pepper: string, message: string): Promise<string> {
  if (!pepper) throw new Error("KEY_PEPPER is not set");
  const key = await crypto.subtle.importKey("raw", enc.encode(pepper), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return hex(await crypto.subtle.sign("HMAC", key, enc.encode(message)));
}

/** What the database stores to recognise an account. Separate prefixes keep it independent of `publicId`. */
export function accountHash(pepper: string, accountId: string): Promise<string> {
  return hmac(pepper, "account:" + accountId);
}

/** The stable, unguessable id that URLs and the leaderboard use. 64 bits, and it does not reveal the account hash. */
export async function publicId(pepper: string, accountId: string): Promise<string> {
  return (await hmac(pepper, "public:" + accountId)).slice(0, 16);
}
