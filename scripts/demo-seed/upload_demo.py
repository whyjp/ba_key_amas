"""Uploads the demo bodies made by build_demo.py to a ba-key-AMAS server, signed like the official add-on.

    python scripts/demo-seed/upload_demo.py <bodies-dir> [--server https://key-amas.cxx.co.kr] [--dry-run]

The signing key is read from BAOCR_ADDON_SIGNING_KEY, else from the repository's .env (git ignores it). New accounts
are paced to stay under the server's limit on creating accounts (5 a minute per address).
"""
import argparse
import hashlib
import hmac
import json
import os
import pathlib
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
PROTOCOL = "2"
ADDON_VERSION = "0.1.0"


def signing_key() -> str:
    key = os.environ.get("BAOCR_ADDON_SIGNING_KEY", "")
    env = ROOT / ".env"
    if not key and env.is_file():
        for line in env.read_text(encoding="utf-8").splitlines():
            if line.strip().startswith("BAOCR_ADDON_SIGNING_KEY="):
                key = line.split("=", 1)[1].strip().strip("\"'")
    if len(key) < 16:
        raise SystemExit("no signing key: set BAOCR_ADDON_SIGNING_KEY or put it in .env")
    return key


def post(server: str, path: str, body: dict, key: str):
    data = json.dumps(body, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    ts = str(int(time.time()))
    sig = hmac.new(key.encode(), f"{ts}\n{path}\n{hashlib.sha256(data).hexdigest()}".encode(), hashlib.sha256).hexdigest()
    req = urllib.request.Request(server + path, data=data, method="POST", headers={
        "content-type": "application/json", "user-agent": f"ba-key-AMAS/{ADDON_VERSION}", "x-amas-protocol": PROTOCOL, "x-amas-addon": ADDON_VERSION, "x-amas-ts": ts, "x-amas-sig": sig})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status, json.loads(r.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        text = e.read().decode("utf-8", "replace")
        try:
            return e.code, json.loads(text)
        except ValueError:
            return e.code, {"error": text[:120]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dir")
    ap.add_argument("--server", default="https://key-amas.cxx.co.kr")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--pause", type=float, default=13.0)
    a = ap.parse_args()
    key = signing_key()
    files = sorted(pathlib.Path(a.dir).glob("demo-*.json"))
    path = "/api/upload"
    for f in files:
        body = json.loads(f.read_text(encoding="utf-8"))
        if a.dry_run:
            print(f.name, "dry run", len(body["roster"]["entries"]), "students")
            continue
        for attempt in range(6):
            status, res = post(a.server, path, body, key)
            if status == 429:
                time.sleep(20)
                continue
            break
        print(f.name, status, json.dumps(res, ensure_ascii=False)[:160])
        if status not in (200, 201):
            raise SystemExit(f"stopped at {f.name}")
        time.sleep(a.pause)


if __name__ == "__main__":
    main()
