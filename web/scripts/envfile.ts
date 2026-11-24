// Reading the repository's local `.env` (git ignores it). Shared by sync-dev-vars.ts and its test.
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";

/** `NAME=value` lines; `#` comments and blank lines are skipped; one pair of quotes around a value is dropped. */
export function parseEnv(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq < 1) continue;
    const name = line.slice(0, eq).trim();
    let value = line.slice(eq + 1).trim();
    if (value.length >= 2 && (value[0] === '"' || value[0] === "'") && value.at(-1) === value[0]) value = value.slice(1, -1);
    out[name] = value;
  }
  return out;
}

/** The nearest `.env` at or above `from` (up to `levels` directories), or null. */
export function findEnvFile(from: string, levels = 5): string | null {
  let dir = from;
  for (let i = 0; i <= levels; i++) {
    const candidate = join(dir, ".env");
    if (existsSync(candidate)) return candidate;
    const up = dirname(dir);
    if (up === dir) break;
    dir = up;
  }
  return null;
}

export function readEnvFile(path: string): Record<string, string> {
  return parseEnv(readFileSync(path, "utf8"));
}
