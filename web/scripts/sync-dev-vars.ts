// Writes web/.dev.vars (git ignores it) for `wrangler dev` from the repository's local `.env`:
//   ADDON_SIGNING_KEY     the add-on signing key, when `.env` has BAOCR_ADDON_SIGNING_KEY
//   ALLOW_UNSIGNED_ADDON  =1 only when there is no key, so a checkout without secrets can still run the server locally
//   KEY_PEPPER            kept from the existing file, else a fresh random value (local data only)
// Run by `npm run dev`. Prints which mode it chose and never prints a value.
import { randomBytes } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { findEnvFile, parseEnv, readEnvFile } from "./envfile.ts";

const web = join(dirname(fileURLToPath(import.meta.url)), "..");
const target = join(web, ".dev.vars");

const envPath = process.env.BAOCR_ENV_FILE || findEnvFile(web);
const fromEnvFile = envPath ? readEnvFile(envPath) : {};
const key = process.env.BAOCR_ADDON_SIGNING_KEY || fromEnvFile.BAOCR_ADDON_SIGNING_KEY || "";

const existing = existsSync(target) ? parseEnv(readFileSync(target, "utf8")) : {};
const pepper = existing.KEY_PEPPER || randomBytes(32).toString("hex");

const lines = [`KEY_PEPPER=${pepper}`];
if (key.length >= 16) lines.push(`ADDON_SIGNING_KEY=${key}`);
else lines.push("ALLOW_UNSIGNED_ADDON=1");
writeFileSync(target, lines.join("\n") + "\n");

console.log(key.length >= 16 ? `dev vars: signing key from ${envPath ?? "the environment"}, unsigned requests are refused` : "dev vars: no signing key (.env missing or empty), unsigned add-on requests are accepted, local only");
