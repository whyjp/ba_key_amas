// Version management between this server and the add-on (ba-key-AMAS).
//
// PROTOCOL is the upload contract this server speaks. The add-on sends the contract it was built for in
// the `x-amas-protocol` header. A server accepts add-ons from MIN_PROTOCOL up to its own PROTOCOL: an older
// add-on keeps working while the change is backward compatible, and the server bumps MIN_PROTOCOL when it
// drops a field the old add-on relied on. An add-on newer than the server is refused too, because it may send
// data this server would silently drop. The scan files carry their own schema numbers, checked the same way.
//
// This identifies the add-on, it does not authenticate it: anything can send these headers. What it does
// prevent is the website, old add-ons and honest mistakes uploading data this server cannot read.

export const SERVER_VERSION = "0.2.0";
export const PROTOCOL = 1;
export const MIN_PROTOCOL = 1;
export const MIN_ROSTER_SCHEMA = 3;
export const MIN_PROGRESS_SCHEMA = 2;

export interface Client {
  addon: string;
  protocol: number;
}

export interface Refusal {
  status: number;
  error: string;
}

/** Browsers always send Sec-Fetch-* and, for a POST, Origin. The add-on sends neither. */
export function fromBrowser(h: Headers): boolean {
  return h.has("origin") || h.has("sec-fetch-site") || h.has("sec-fetch-mode");
}

export function checkClient(h: Headers): Client | Refusal {
  if (fromBrowser(h)) return { status: 403, error: "업로드는 ba-key-AMAS 애드온에서만 할 수 있습니다" };
  const protocol = Number(h.get("x-amas-protocol"));
  const addon = (h.get("x-amas-addon") ?? "").slice(0, 32);
  if (!Number.isInteger(protocol) || !/^[\w.+-]{1,32}$/.test(addon)) return { status: 426, error: "ba-key-AMAS 애드온에서만 업로드할 수 있습니다" };
  if (protocol < MIN_PROTOCOL) return { status: 426, error: `애드온이 너무 오래되었습니다. 업데이트하세요 (업로드 규약 ${protocol}, 필요 ${MIN_PROTOCOL} 이상)` };
  if (protocol > PROTOCOL) return { status: 426, error: `서버가 아직 이 애드온을 받을 수 없습니다 (업로드 규약 ${protocol}, 서버 ${PROTOCOL})` };
  return { addon, protocol };
}

export function checkSchemas(roster: unknown, progress: unknown): Refusal | null {
  const schema = (v: unknown) => (typeof v === "object" && v !== null ? (v as { schema_version?: unknown }).schema_version : undefined);
  const r = schema(roster);
  if (typeof r !== "number" || r < MIN_ROSTER_SCHEMA) return { status: 426, error: `학생 명단 형식이 오래되었습니다. 애드온을 업데이트해 다시 스캔하세요 (형식 ${r ?? "없음"}, 필요 ${MIN_ROSTER_SCHEMA} 이상)` };
  if (progress !== null && progress !== undefined) {
    const p = schema(progress);
    if (typeof p !== "number" || p < MIN_PROGRESS_SCHEMA) return { status: 426, error: `진행 상태 형식이 오래되었습니다. 애드온을 업데이트해 다시 스캔하세요 (형식 ${p ?? "없음"}, 필요 ${MIN_PROGRESS_SCHEMA} 이상)` };
  }
  return null;
}
