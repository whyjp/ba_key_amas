// Everything that crosses the upload boundary is parsed here into plain typed rows. Unknown fields are
// dropped, numbers are range-checked and strings are length-capped. Nothing from the body reaches SQL or
// the page except through these types.

export const MAX_BODY_BYTES = 3_000_000;
export const MAX_STUDENTS = 400;
export const MAX_PROGRESS_BYTES = 200_000;

export interface StudentRow {
  name: string;
  level: number | null;
  stars: number | null;
  ex_skill: number | null;
  normal_skill: number | null;
  passive_skill: number | null;
  sub_skill: number | null;
  bond: number | null;
  uw_stars: number | null;
  uw_level: number | null;
  role: string | null;
  pinned: boolean | null;
  memorial: boolean | null;
}

export interface Upload {
  nickname: string;
  key: string | null;
  /** Show this user on the leaderboard. Off unless the uploader asks. */
  public: boolean;
  students: StudentRow[];
  account: Record<string, unknown> | null;
  progress: unknown | null;
}

export class Invalid extends Error {}

type Obj = Record<string, unknown>;

function isObj(v: unknown): v is Obj {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function int(v: unknown, lo: number, hi: number): number | null {
  return typeof v === "number" && Number.isInteger(v) && v >= lo && v <= hi ? v : null;
}

function str(v: unknown, max: number): string | null {
  return typeof v === "string" && v.length > 0 && v.length <= max ? v.normalize("NFC") : null;
}

const ROLES = new Set(["STRIKER", "SPECIAL"]);

export function parseStudent(v: unknown): StudentRow | null {
  if (!isObj(v)) return null;
  const name = str(v.name, 40);
  if (!name) return null;
  return {
    name,
    level: int(v.level, 1, 100),
    stars: int(v.rarity_stars, 1, 5),
    ex_skill: int(v.ex_skill_level, 1, 5),
    normal_skill: int(v.normal_skill_level, 1, 10),
    passive_skill: int(v.passive_skill_level, 1, 10),
    sub_skill: int(v.sub_skill_level, 1, 10),
    bond: int(v.bond_rank, 1, 100),
    uw_stars: int(v.unique_weapon_stars, 1, 4),
    uw_level: int(v.unique_weapon_level, 1, 60),
    role: typeof v.combat_role === "string" && ROLES.has(v.combat_role) ? v.combat_role : null,
    pinned: typeof v.is_pinned === "boolean" ? v.is_pinned : null,
    memorial: null,
  };
}

function parseAccount(v: unknown): Obj | null {
  if (!isObj(v)) return null;
  const out: Obj = {};
  const credits = int(v.credits, 0, 2 ** 40);
  const pyroxene = int(v.pyroxene, 0, 2 ** 31);
  if (credits !== null) out.credits = credits;
  if (pyroxene !== null) out.pyroxene = pyroxene;
  if (typeof v.ap === "string" && /^\d{1,4}\/\d{1,4}$/.test(v.ap)) out.ap = v.ap;
  return out;
}

/** The duty menu scan lists every owned student with the memorial lobby icon state. */
function memorialByName(progress: unknown): Map<string, boolean> {
  const out = new Map<string, boolean>();
  const duty = isObj(progress) && isObj(progress.duty) ? progress.duty : null;
  if (!duty || !Array.isArray(duty.students)) return out;
  for (const d of duty.students) {
    if (isObj(d) && typeof d.name === "string" && typeof d.memorial_lobby === "boolean") out.set(d.name.normalize("NFC"), d.memorial_lobby);
  }
  return out;
}

/** `body` is `{nickname, key?, public?, roster: {entries: [{student}], account?}, progress?}`. */
export function parseUpload(body: unknown): Upload {
  if (!isObj(body)) throw new Invalid("본문이 JSON 객체가 아닙니다");
  const nickname = typeof body.nickname === "string" ? body.nickname : "";
  const key = typeof body.key === "string" && body.key.length <= 64 ? body.key : null;
  const roster = isObj(body.roster) ? body.roster : null;
  if (!roster || !Array.isArray(roster.entries)) throw new Invalid("roster.entries 가 없습니다");
  if (roster.entries.length > MAX_STUDENTS) throw new Invalid("학생 수가 너무 많습니다");
  const students: StudentRow[] = [];
  const seen = new Set<string>();
  for (const e of roster.entries) {
    const s = isObj(e) ? parseStudent(e.student) : null;
    if (s && !seen.has(s.name)) {
      seen.add(s.name);
      students.push(s);
    }
  }
  if (students.length === 0) throw new Invalid("읽을 수 있는 학생이 없습니다");
  let progress: unknown | null = null;
  if (body.progress !== undefined && body.progress !== null) {
    if (!isObj(body.progress)) throw new Invalid("progress 가 JSON 객체가 아닙니다");
    if (JSON.stringify(body.progress).length > MAX_PROGRESS_BYTES) throw new Invalid("progress 가 너무 큽니다");
    progress = body.progress;
  }
  const memorial = memorialByName(progress);
  for (const s of students) s.memorial = memorial.get(s.name) ?? null;
  return { nickname, key, public: body.public === true, students, account: parseAccount(roster.account), progress };
}
