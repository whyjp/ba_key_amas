// The student catalog is keyed by student id. Names are not unique: Global has two students called
// "호시노(무장)" (a tank and a damage dealer), so a name alone cannot say which one a scan means. The scanner
// resolves the id from the student's screen (reference.id); the name is only the fallback for old uploads.

export interface Entry {
  id: number;
  name: string;
  role?: string | null;
  school: string | null;
  attack: string | null;
  armor: string | null;
  squad: string | null;
  equipment?: string[] | null;
}

export type Catalog = Record<string, Entry>;

export function indexByName(catalog: Catalog): Map<string, Entry[]> {
  const out = new Map<string, Entry[]>();
  for (const e of Object.values(catalog)) out.set(e.name, [...(out.get(e.name) ?? []), e]);
  return out;
}

/** The entry for a scanned student: its id when the id belongs to a student of that name, else the first of the name. */
export function entryFor(catalog: Catalog, byName: Map<string, Entry[]>, name: string, id: number | null): Entry | null {
  const byId = id === null ? undefined : catalog[String(id)];
  if (byId && byId.name === name) return byId;
  return byName.get(name)?.[0] ?? null;
}

const ROLE_KO: Record<string, string> = { Tanker: "탱커", DamageDealer: "딜러", Healer: "힐러", Supporter: "서포터", Vehicle: "차량" };

/**
 * One teacher can own both students of a shared name. Rows are keyed by name, so the later one gets its role added
 * (`호시노(무장) · 딜러`) instead of silently replacing the first.
 */
export function uniqueNames<T extends { name: string; student_id: number | null }>(rows: T[], catalog: Catalog): T[] {
  const seen = new Set<string>();
  return rows.map((r) => {
    let name = r.name;
    if (seen.has(name)) {
      const role = r.student_id === null ? undefined : catalog[String(r.student_id)]?.role;
      name = `${r.name} · ${(role && ROLE_KO[role]) || r.student_id}`;
    }
    seen.add(name);
    return name === r.name ? r : { ...r, name };
  });
}
