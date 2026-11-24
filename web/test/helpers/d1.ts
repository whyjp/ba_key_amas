// A D1Database look-alike over node:sqlite that applies the real migrations, so the worker's own SQL is what runs.
import { registerHooks } from "node:module";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { DatabaseSync, type SQLInputValue } from "node:sqlite";

const SRC = new URL("../../src/", import.meta.url);
const MIGRATIONS = new URL("../../migrations/", import.meta.url);

/**
 * The worker imports two generated JSON files that `npm run assets` makes (the bundler reads them without an import
 * attribute, plain Node does not), so the tests write empty ones when missing and let Node load JSON as JSON.
 */
export function prepareWorkerImport(): void {
  registerHooks({
    load(url, context, nextLoad) {
      return url.endsWith(".json") ? nextLoad(url, { ...context, format: "json", importAttributes: { ...context.importAttributes, type: "json" } }) : nextLoad(url, context);
    },
  });
  for (const [file, body] of [["catalog.json", "{}"], ["backgrounds.json", "[]"]] as const) {
    if (!existsSync(new URL(file, SRC))) writeFileSync(new URL(file, SRC), body);
  }
}

class Statement {
  private values: SQLInputValue[];
  private db: DatabaseSync;
  readonly sql: string;
  constructor(db: DatabaseSync, sql: string, values: SQLInputValue[] = []) {
    this.db = db;
    this.sql = sql;
    this.values = values;
  }
  /** Like D1, binding gives a new statement, so one prepared statement can be bound many times. */
  bind(...v: unknown[]): Statement {
    return new Statement(this.db, this.sql, v as SQLInputValue[]);
  }
  async first<T>(): Promise<T | null> {
    return (this.db.prepare(this.sql).get(...this.values) as T | undefined) ?? null;
  }
  async all<T>(): Promise<{ results: T[] }> {
    return { results: this.db.prepare(this.sql).all(...this.values) as T[] };
  }
  async run(): Promise<{ meta: { last_row_id: number; changes: number } }> {
    const r = this.db.prepare(this.sql).run(...this.values);
    return { meta: { last_row_id: Number(r.lastInsertRowid), changes: Number(r.changes) } };
  }
}

export class MemoryD1 {
  readonly db = new DatabaseSync(":memory:");
  constructor() {
    for (const f of readdirSync(MIGRATIONS).filter((n) => n.endsWith(".sql")).sort()) this.db.exec(readFileSync(new URL(f, MIGRATIONS), "utf8"));
  }
  prepare(sql: string): Statement {
    return new Statement(this.db, sql);
  }
  async batch(stmts: Statement[]): Promise<unknown[]> {
    this.db.exec("BEGIN");
    try {
      const out = [];
      for (const s of stmts) out.push(await s.run());
      this.db.exec("COMMIT");
      return out;
    } catch (e) {
      this.db.exec("ROLLBACK");
      throw e;
    }
  }
  rows(sql: string): Record<string, unknown>[] {
    return this.db.prepare(sql).all() as Record<string, unknown>[];
  }
}
