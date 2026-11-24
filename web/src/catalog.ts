// The student catalog is keyed by student id. A name can have more than one id: "호시노(무장)" is one student with
// a tank mode and a damage dealer mode, and the game lists each mode as its own record. They are one student (one
// persona, one row in a teacher's roster); the ids only say which mode's picture and slots to use. The scanner
// resolves the id from the screen (reference.id); the name is the fallback for old uploads.

export interface Entry {
  id: number;
  name: string;
  role?: string | null;
  school: string | null;
  attack: string | null;
  armor: string | null;
  position?: string | null;
  squad: string | null;
  equipment?: string[] | null;
}

export type Catalog = Record<string, Entry>;

export function indexByName(catalog: Catalog): Map<string, Entry[]> {
  const out = new Map<string, Entry[]>();
  for (const e of Object.values(catalog)) out.set(e.name, [...(out.get(e.name) ?? []), e]);
  return out;
}

/** The entry for a scanned student: the mode its id names, when the id belongs to a student of that name, else the first of the name. */
export function entryFor(catalog: Catalog, byName: Map<string, Entry[]>, name: string, id: number | null): Entry | null {
  const byId = id === null ? undefined : catalog[String(id)];
  if (byId && byId.name === name) return byId;
  return byName.get(name)?.[0] ?? null;
}
