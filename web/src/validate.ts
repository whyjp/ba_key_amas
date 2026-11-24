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
  detail: Detail | null;
}

export type Detail = Record<string, unknown>;

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


const BIG = 2_000_000_000;

function pick(v: unknown, keys: string[], lo = 0, hi = BIG): Obj | null {
  if (!isObj(v)) return null;
  const out: Obj = {};
  for (const k of keys) {
    const n = int(v[k], lo, hi);
    if (n !== null) out[k] = n;
  }
  return Object.keys(out).length ? out : null;
}

function bools(v: unknown, keys: string[]): Obj | null {
  if (!isObj(v)) return null;
  const out: Obj = {};
  for (const k of keys) if (typeof v[k] === "boolean") out[k] = v[k];
  return Object.keys(out).length ? out : null;
}

const POSITIONS = new Set(["FRONT", "MIDDLE", "BACK"]);
const STAT_KEYS = ["max_hp", "attack", "defense", "healing"];
const DETAIL_KEYS = ["accuracy", "evasion", "critical", "critical_damage_pct", "defense_pierce", "stability", "range", "cost_recovery", "cc_power", "cc_resist", "type_bonus_pct", "max_cost_bonus"];
const MYSTIC_KEYS = ["stars", "next_stars", "max_bond_rank", "next_max_bond_rank", "next_max_hp", "next_attack", "next_healing"];
const TERRAIN_KEYS = ["urban", "outdoor", "indoor"];

function parseEquipment(v: unknown): Obj[] {
  if (!Array.isArray(v)) return [];
  const out: Obj[] = [];
  for (const e of v.slice(0, 6)) {
    if (!isObj(e)) continue;
    const row: Obj = {};
    const slot = int(e.slot, 1, 6);
    if (slot === null) continue;
    row.slot = slot;
    row.unique = e.slot_kind === "unique_gear";
    if (typeof e.equipped === "boolean") row.equipped = e.equipped;
    const kind = str(e.type, 20);
    if (kind) row.type = kind;
    const name = str(e.item_name, 60);
    if (name) row.item_name = name;
    const tier = int(e.tier, 1, 12);
    if (tier !== null) row.tier = tier;
    const level = int(e.level, 1, 100);
    if (level !== null) row.level = level;
    out.push(row);
  }
  return out;
}

/** The scanned numbers beyond the table columns. Every field is whitelisted, ranged and typed here. */
export function parseDetail(v: Obj): Detail | null {
  const d: Detail = {};
  const stats = pick(v.stats, STAT_KEYS);
  if (stats) d.stats = stats;
  const exp = pick(v.exp, ["current", "required"]);
  if (exp) d.exp = exp;
  if (isObj(v.loadout)) {
    const pos = typeof v.loadout.position === "string" && POSITIONS.has(v.loadout.position) ? v.loadout.position : null;
    const weapon = typeof v.loadout.weapon_class === "string" && /^[A-Z]{2,3}$/.test(v.loadout.weapon_class) ? v.loadout.weapon_class : null;
    if (pos) d.position = pos;
    if (weapon) d.weapon_class = weapon;
  }
  const fixed = isObj(v.fixed_meta) ? v.fixed_meta.terrain_base : null;
  const terrain = pick(v.terrain, TERRAIN_KEYS, 0, 4) ?? pick(fixed, TERRAIN_KEYS, 0, 4);
  if (terrain) d.terrain = terrain;
  const equipment = parseEquipment(v.equipment);
  if (equipment.length) d.equipment = equipment;
  const locked = bools(v.skill_locked, ["ex", "basic", "passive", "sub"]);
  if (locked) d.skill_locked = locked;
  const locks = bools(v.locks, ["ability_unlock_locked", "simsang_bloom_locked"]);
  if (locks) d.locks = locks;
  const deep = isObj(v.deep) ? v.deep : null;
  if (deep) {
    const sd = isObj(deep.stat_detail) ? deep.stat_detail : null;
    const more = sd ? pick(sd, DETAIL_KEYS, -BIG, BIG) : null;
    if (more) d.stat_detail = more;
    const base = pick(deep.base_stats, STAT_KEYS);
    if (base) d.base_stats = base;
    const mystic = pick(deep.mystic, MYSTIC_KEYS);
    if (mystic) d.mystic = mystic;
  }
  return Object.keys(d).length ? d : null;
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
    detail: parseDetail(v),
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
