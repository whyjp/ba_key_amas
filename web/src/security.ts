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

export async function readJson(req: Request, maxBytes: number): Promise<unknown> {
  if (Number(req.headers.get("content-length") ?? "0") > maxBytes) throw new TooLarge();
  const buf = await req.arrayBuffer();
  if (buf.byteLength > maxBytes) throw new TooLarge();
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true, ignoreBOM: false }).decode(buf));
  } catch {
    throw new BadJson();
  }
}
