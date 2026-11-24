// Uploads the scanner's local output to a running server (until the scanner has its own upload step).
//   node scripts/upload-local.mjs <server-url> <nickname> [key]
// Reads %APPDATA%/baocr/students/students.json and, when present, progress/latest.json.
import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const [url, nickname, key] = process.argv.slice(2);
if (!url || !nickname) {
  console.error("usage: node scripts/upload-local.mjs <server-url> <nickname> [key]");
  process.exit(2);
}
const base = join(process.env.APPDATA ?? ".", "baocr");
const roster = JSON.parse(readFileSync(join(base, "students", "students.json"), "utf8"));
const progressPath = join(base, "progress", "latest.json");
const progress = existsSync(progressPath) ? JSON.parse(readFileSync(progressPath, "utf8")) : undefined;

const res = await fetch(new URL("/api/upload", url), {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ nickname, key, roster, progress }),
});
const out = await res.json();
if (!res.ok) {
  console.error(res.status, out.error);
  process.exit(1);
}
console.log(out.created ? "새 키" : "갱신", out.key, `(${out.students}명)`);
