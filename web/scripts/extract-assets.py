"""Builds the student catalog (src/catalog.json) and the portrait assets (public/portraits/<id>.webp).

Source: SchaleDB's Korean student table, restricted to students released on the Global server (the
same rule as the scanner's wiki.db). Neither output is committed: this runs in the deploy build
(`npm run assets`), so the repository holds no game images.

Run from web/:  python -I scripts/extract-assets.py
"""
import json
import pathlib
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASE = "https://schaledb.com"
GLOBAL_FLAG = 1  # IsReleased = [JP, Global, CN]


def get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "ba-key-amas-assets"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def main() -> int:
    students = json.loads(get(f"{BASE}/data/kr/students.min.json"))
    catalog = {}
    for s in students.values():
        released = s.get("IsReleased") or []
        if len(released) <= GLOBAL_FLAG or not released[GLOBAL_FLAG]:
            continue
        catalog[s["Name"]] = {
            "id": s["Id"],
            "school": s.get("School"),
            "attack": s.get("BulletType"),
            "armor": s.get("ArmorType"),
            "squad": s.get("SquadType"),
        }
    (ROOT / "src").mkdir(exist_ok=True)
    (ROOT / "src" / "catalog.json").write_text(json.dumps(catalog, ensure_ascii=False, sort_keys=True), encoding="utf-8")

    out = ROOT / "public" / "portraits"
    out.mkdir(parents=True, exist_ok=True)
    fetched = 0
    for entry in catalog.values():
        dest = out / f"{entry['id']}.webp"
        if dest.exists():
            continue
        dest.write_bytes(get(f"{BASE}/images/student/collection/{entry['id']}.webp"))
        fetched += 1
    print(f"{len(catalog)} students, {fetched} portraits downloaded")
    return 0


if __name__ == "__main__":
    sys.exit(main())
