// Leaderboard scoring. Pure: a list of owned students in, characters and totals out.
//
// A character is the name without its outfit, so `리오` and `리오(무장)` are one character. The outfits' points
// are added with a falling coefficient (OUTFIT_DECAY), best outfit at full weight. The weights below are a judgement about effort, not a game formula. Bond rank, the pinned flag and
// the memorial lobby count in full. Growth (level, stars, unique weapon, skills) counts at GROWTH_WEIGHT,
// because the leaderboard is meant to reward long relationships over raw investment.
//
// Bond: up to rank 30 each rank is cheap (2 points). From 31 to 59 each rank costs far more bond experience
// (3 points). Rank 60 is a milestone that takes a very large effort of its own, so it adds a 100 point bonus
// and every rank above it is worth 6. Unique weapon is the hardest growth item, so owning one adds a flat
// 30 on top of 15 per weapon star and 0.6 per weapon level.

export const SCORE_VERSION = 2;
/** Each further outfit of a character counts DECAY times the one before it (best outfit first), so a wardrobe adds
 *  credit without letting a student with many outfits run away from one with a single, well loved outfit. */
export const OUTFIT_DECAY = 0.4;
export const GROWTH_WEIGHT = 0.6;
export const PIN_POINTS = 40;
export const MEMORIAL_POINTS = 50;
export const HIGH_LEVEL = 80;
export const BOND_HIGH = 30;
export const BOND_TOP = 60;

export interface OwnedStudent {
  name: string;
  student_id: number | null;
  level: number | null;
  stars: number | null;
  ex_skill: number | null;
  normal_skill: number | null;
  passive_skill: number | null;
  sub_skill: number | null;
  bond: number | null;
  uw_stars: number | null;
  uw_level: number | null;
  pinned: boolean | null;
  memorial: boolean | null;
}

export interface Outfit {
  name: string;
  id: number | null;
}

export interface Character {
  base: string;
  outfits: Outfit[];
  bond: number;
  bondPoints: number;
  pinPoints: number;
  memorialPoints: number;
  growthPoints: number;
  total: number;
  pinned: boolean;
  memorial: boolean;
  maxLevel: number;
}

export interface Stats {
  characters: number;
  bond30: number;
  bond60: number;
  levelHigh: number;
  pinned: number;
  memorial: number;
}

export interface Analysis {
  score: number;
  stats: Stats;
  characters: Character[];
  top: Character[];
}

/** `리오(무장)` → `리오`. Only a trailing parenthesis is an outfit; `시로코*테러` is its own character. */
export function baseName(name: string): string {
  const base = name.replace(/\s*\([^)]*\)\s*$/, "").trim();
  return base || name;
}

export function bondPoints(rank: number): number {
  const r = Math.max(0, rank);
  return 2 * Math.min(r, BOND_HIGH) + 3 * Math.min(Math.max(r - BOND_HIGH, 0), BOND_TOP - BOND_HIGH) + 6 * Math.max(r - BOND_TOP, 0) + (r >= BOND_TOP ? 100 : 0);
}

export function growthPoints(s: OwnedStudent): number {
  const level = (s.level ?? 0) * 0.5;
  const stars = Math.max((s.stars ?? 1) - 1, 0) * 4;
  const weapon = s.uw_stars ? 30 + 15 * s.uw_stars + 0.6 * (s.uw_level ?? 0) : 0;
  const skills = Math.max((s.ex_skill ?? 1) - 1, 0) * 6 + (s.ex_skill === 5 ? 8 : 0) + 2 * (Math.max((s.normal_skill ?? 1) - 1, 0) + Math.max((s.passive_skill ?? 1) - 1, 0) + Math.max((s.sub_skill ?? 1) - 1, 0));
  return level + stars + weapon + skills;
}

/** The character's own growth counts at GROWTH_WEIGHT; the relationship parts count in full. */
function weighted(c: Pick<Character, "bondPoints" | "pinPoints" | "memorialPoints" | "growthPoints">): number {
  return c.bondPoints + c.pinPoints + c.memorialPoints + GROWTH_WEIGHT * c.growthPoints;
}

export function analyze(students: OwnedStudent[]): Analysis {
  const groups = new Map<string, OwnedStudent[]>();
  for (const s of students) {
    const key = baseName(s.name);
    groups.set(key, [...(groups.get(key) ?? []), s]);
  }
  const characters: Character[] = [];
  for (const [base, list] of groups) {
    const own = list
      .map((s) => {
        const part = { bondPoints: bondPoints(s.bond ?? 0), pinPoints: s.pinned ? PIN_POINTS : 0, memorialPoints: s.memorial ? MEMORIAL_POINTS : 0, growthPoints: growthPoints(s) };
        return { s, part, total: weighted(part) };
      })
      .sort((a, b) => b.total - a.total || (b.s.bond ?? 0) - (a.s.bond ?? 0) || a.s.name.localeCompare(b.s.name, "ko"));
    const sum = { bondPoints: 0, pinPoints: 0, memorialPoints: 0, growthPoints: 0 };
    let total = 0;
    own.forEach((o, i) => {
      const w = OUTFIT_DECAY ** i;
      total += w * o.total;
      for (const k of Object.keys(sum) as (keyof typeof sum)[]) sum[k] += w * o.part[k];
    });
    const ordered = [...list].sort((a, b) => Number(a.name !== base) - Number(b.name !== base) || (b.bond ?? 0) - (a.bond ?? 0) || a.name.localeCompare(b.name, "ko"));
    characters.push({
      base,
      outfits: ordered.map((s) => ({ name: s.name, id: s.student_id })),
      bond: Math.max(...list.map((s) => s.bond ?? 0)),
      ...sum,
      total: Math.round(total * 10) / 10,
      pinned: list.some((s) => s.pinned),
      memorial: list.some((s) => s.memorial),
      maxLevel: Math.max(...list.map((s) => s.level ?? 0)),
    });
  }
  characters.sort((a, b) => b.total - a.total || a.base.localeCompare(b.base, "ko"));
  const stats: Stats = {
    characters: characters.length,
    bond30: characters.filter((c) => c.bond >= BOND_HIGH).length,
    bond60: characters.filter((c) => c.bond >= BOND_TOP).length,
    levelHigh: characters.filter((c) => c.maxLevel >= HIGH_LEVEL).length,
    pinned: characters.filter((c) => c.pinned).length,
    memorial: characters.filter((c) => c.memorial).length,
  };
  const score = Math.round(characters.reduce((n, c) => n + c.total, 0) * 10) / 10;
  return { score, stats, characters, top: characters.slice(0, 3) };
}

export interface CharSummary {
  base: string;
  total: number;
  outfits: Outfit[];
}

/** A user's characters without the parts of the score: all the leaderboard keeps per character. */
export function summarize(a: Analysis): CharSummary[] {
  return a.characters.map((c) => ({ base: c.base, total: c.total, outfits: c.outfits.slice(0, 4) }));
}

export interface Community {
  base: string;
  /** The characters' totals added up over every user who owns the character. */
  total: number;
  owners: number;
  outfits: Outfit[];
}

/** Adds up the characters of many users: one entry per character name, best first. Nothing of a single student's
 *  own stats takes part, only who owns the character and what it is worth. */
export function aggregate(users: CharSummary[][]): Community[] {
  const by = new Map<string, Community>();
  for (const list of users) {
    for (const c of list) {
      const e = by.get(c.base) ?? { base: c.base, total: 0, owners: 0, outfits: [] };
      e.total += c.total;
      e.owners += 1;
      for (const o of c.outfits) if (!e.outfits.some((x) => x.name === o.name) && e.outfits.length < 4) e.outfits.push(o);
      by.set(c.base, e);
    }
  }
  return [...by.values()].map((e) => ({ ...e, total: Math.round(e.total * 10) / 10 })).sort((a, b) => b.total - a.total || a.base.localeCompare(b.base, "ko"));
}
