// Leaderboard scoring. Pure: a list of owned students in, characters and totals out.
//
// A character is the name without its outfit, so `리오` and `리오(무장)` are one character. The outfits' points
// are added with a falling coefficient (OUTFIT_DECAY), best outfit at full weight. The weights are a judgement about
// the effort an item takes, measured where the game publishes a number (see docs/scoring.md); they are not a game formula.
//
// What counts in full: bond rank, the pinned flag, the memorial lobby, and the items that cost the most to raise,
// the promotion to 4 and 5 stars and the unique weapon. What counts at GROWTH_WEIGHT: level up to 80 and the
// skill levels, which most accounts raise as a matter of course. Levels above 80 are the level limit being lifted
// and count at a rate of their own.
//
// Bond: points follow the experience a rank takes (the cumulative table, square-rooted so that rank 100 is a
// milestone and not the whole score), plus a bonus for crossing rank 30 and rank 60, where each rank starts to cost
// far more than the one before it.

export const SCORE_VERSION = 3;
/** Each further outfit of a character counts DECAY times the one before it (best outfit first), so a wardrobe adds
 *  credit without letting a student with many outfits run away from one with a single, well loved outfit. */
export const OUTFIT_DECAY = 0.4;
export const GROWTH_WEIGHT = 0.6;
export const PIN_POINTS = 40;
export const MEMORIAL_POINTS = 50;
export const HIGH_LEVEL = 80;
export const BOND_HIGH = 30;
export const BOND_TOP = 60;
/** Bond points per square root of the experience it took to reach the rank. */
export const BOND_SCALE = 0.8;
export const BOND_HIGH_BONUS = 30;
export const BOND_TOP_BONUS = 60;
/** Points of one level up to HIGH_LEVEL, and of one level past it (the level limit, 80 to 90). */
export const LEVEL_POINTS = 0.5;
export const LEVEL_LIMIT_POINTS = 4;
/** Promotion to the star on the left: 2 and 3 stars are cheap, 4 and 5 stars cost 100 and 120 of the student's own
 *  shards on top of the unlock (45 and 54 points, 0.45 per shard). */
export const STAR_POINTS: Record<number, number> = { 2: 5, 3: 9, 4: 45, 5: 54 };
/** Rarity a student is recruited at when the catalog does not say. */
export const DEFAULT_BASE_STARS = 3;
/** Unique weapon: unlocking it (at 5 stars), then 120, 180 and 200 shards for weapon stars 2, 3 and 4 (0.45 per shard). */
export const WEAPON_UNLOCK_POINTS = 20;
export const WEAPON_STAR_POINTS: Record<number, number> = { 2: 54, 3: 81, 4: 90 };
export const WEAPON_LEVEL_POINTS = 0.8;

/** Experience needed to go from bond rank n to n + 1, for n = 1..99 (the community's data-mined table;
 *  its running totals are 300 at rank 10, 1845 at 20, 6180 at 30, 29175 at 50 and 240225 at 100). */
const BOND_STEPS = [
  15, 30, 30, 35, 35, 35, 40, 40, 40, 60, 90, 105, 120, 140, 160, 180, 205, 230, 255, 285,
  315, 345, 375, 410, 445, 480, 520, 560, 600, 645, 690, 735, 780, 830, 880, 930, 985, 1040, 1095, 1155,
  1215, 1275, 1335, 1400, 1465, 1530, 1600, 1670, 1740, 1815, 1890, 1965, 2040, 2120, 2200, 2280, 2365, 2450, 2535, 2625,
  2715, 2805, 2895, 2990, 3085, 3180, 3280, 3380, 3480, 3585, 3690, 3795, 3900, 4010, 4120, 4230, 4345, 4460, 4575, 4695,
  4815, 4935, 5055, 5180, 5305, 5430, 5560, 5690, 5820, 5955, 6090, 6225, 6360, 6500, 6640, 6780, 6925, 7070, 7215,
];
/** BOND_EXP[r] is the experience it took to reach bond rank r (rank 1 is 0). */
export const BOND_EXP: number[] = [0, ...BOND_STEPS.reduce<number[]>((acc, step) => [...acc, (acc[acc.length - 1] ?? 0) + step], [0])];

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
  /** The rarity the student is recruited at, from the catalog; null when it is not known. */
  base_stars?: number | null;
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
  /** Level and skills (counted at GROWTH_WEIGHT). */
  growthPoints: number;
  /** Promotion to 4 and 5 stars and the unique weapon (counted in full). */
  upgradePoints: number;
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
  const r = Math.min(Math.max(Math.round(rank), 0), BOND_EXP.length - 1);
  return BOND_SCALE * Math.sqrt(BOND_EXP[r] ?? 0) + (r >= BOND_HIGH ? BOND_HIGH_BONUS : 0) + (r >= BOND_TOP ? BOND_TOP_BONUS : 0);
}

/** Points of the rarity: each promotion above the one the student is recruited at, the 4th and 5th star the dear ones. */
export function starPoints(stars: number | null, baseStars: number | null | undefined): number {
  const base = Math.min(Math.max(baseStars ?? DEFAULT_BASE_STARS, 1), 5);
  let n = 0;
  for (let k = base + 1; k <= Math.min(stars ?? 0, 5); k++) n += STAR_POINTS[k] ?? 0;
  return n;
}

export function weaponPoints(stars: number | null, level: number | null): number {
  if (!stars) return 0;
  let n = WEAPON_UNLOCK_POINTS + WEAPON_LEVEL_POINTS * (level ?? 0);
  for (let k = 2; k <= Math.min(stars, 4); k++) n += WEAPON_STAR_POINTS[k] ?? 0;
  return n;
}

/** Level and skill levels: what most accounts raise as a matter of course. */
export function growthPoints(s: OwnedStudent): number {
  const lv = s.level ?? 0;
  const level = LEVEL_POINTS * Math.min(lv, HIGH_LEVEL) + LEVEL_LIMIT_POINTS * Math.max(lv - HIGH_LEVEL, 0);
  const skills = Math.max((s.ex_skill ?? 1) - 1, 0) * 6 + (s.ex_skill === 5 ? 8 : 0) + 2 * (Math.max((s.normal_skill ?? 1) - 1, 0) + Math.max((s.passive_skill ?? 1) - 1, 0) + Math.max((s.sub_skill ?? 1) - 1, 0));
  return level + skills;
}

/** The items that cost the most to raise: the rarity and the unique weapon. */
export function upgradePoints(s: OwnedStudent): number {
  return starPoints(s.stars, s.base_stars) + weaponPoints(s.uw_stars, s.uw_level);
}

/** The character's level and skills count at GROWTH_WEIGHT; the relationship parts and the costly upgrades count in full. */
function weighted(c: Pick<Character, "bondPoints" | "pinPoints" | "memorialPoints" | "growthPoints" | "upgradePoints">): number {
  return c.bondPoints + c.pinPoints + c.memorialPoints + c.upgradePoints + GROWTH_WEIGHT * c.growthPoints;
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
        const part = { bondPoints: bondPoints(s.bond ?? 0), pinPoints: s.pinned ? PIN_POINTS : 0, memorialPoints: s.memorial ? MEMORIAL_POINTS : 0, growthPoints: growthPoints(s), upgradePoints: upgradePoints(s) };
        return { s, part, total: weighted(part) };
      })
      .sort((a, b) => b.total - a.total || (b.s.bond ?? 0) - (a.s.bond ?? 0) || a.s.name.localeCompare(b.s.name, "ko"));
    const sum = { bondPoints: 0, pinPoints: 0, memorialPoints: 0, growthPoints: 0, upgradePoints: 0 };
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
