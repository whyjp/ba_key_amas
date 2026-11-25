"""The uncropped memorial lobby pictures of blue-utils.me (scripts/.cache/lobby-bu/<id>.webp), which extract-lobby.py prefers.

The wiki's pictures (extract-lobby.py's other source) are 16:9 or 3:2 cuts of the game's scene; these are the whole scene, mostly
1.44:1 and taller, so a card-shaped cut of them shows more of the character. The site's robots.txt allows /static and asks
for 5 seconds between requests, which this keeps (about 25 minutes for 270 pictures; pictures already fetched are kept).
Rights stay with the game's rights holders (Nexon, Nexon Games, Yostar) and the picture's artists.

    python -I scripts/fetch-lobby-blueutils.py [--only 10133,20041]
"""
import json
import pathlib
import re
import sys
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
CACHE = ROOT / "scripts" / ".cache" / "lobby-bu"
SITE = "https://blue-utils.me"
UA = "ba-key-amas-assets (build-time; keeps the 5 s crawl delay)"
DELAY = 5.0


def get(url: str, binary: bool = False):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as r:
        data = r.read()
    return data if binary else data.decode("utf-8")


# SchaleDB's English outfit word -> the site's
SYNONYMS = {"cycling": "riding", "hot spring": "onsen", "cheer squad": "oendan", "track": "gym", "pop idol": "idol", "pajamas": "pajama", "part-timer": "arbeit"}


def norm(s: str) -> str:
    return re.sub(r"[^a-z0-9]", "", s.lower())


def pages() -> dict:
    """normalised 'slug outfit' -> the picture's file stem, from the site map."""
    urls = re.findall(r"<loc>https://blue-utils\.me/student/([^<]+)</loc>", get(f"{SITE}/sitemap.xml"))
    return {norm(u): u.replace("/", "_") for u in urls if u != "list"}


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    only = {int(x) for x in sys.argv[sys.argv.index("--only") + 1].split(",")} if "--only" in sys.argv else set()
    catalog = json.loads((ROOT / "src" / "catalog.json").read_text(encoding="utf-8"))
    ids = sorted({e["id"] for e in catalog.values()})
    english = json.loads(get("https://schaledb.com/data/en/students.min.json"))
    stems = pages()
    CACHE.mkdir(parents=True, exist_ok=True)
    missing, got, kept = [], 0, 0
    for sid in ids:
        if only and sid not in only:
            continue
        name = english.get(str(sid), {}).get("Name", "")
        low = name.lower()
        stem = stems.get(norm(name)) or next((stems[norm(low.replace(a, b))] for a, b in SYNONYMS.items() if a in low and norm(low.replace(a, b)) in stems), None)
        if not stem:
            missing.append(f"{sid} {name}")
            continue
        out = CACHE / f"{sid}.webp"
        if out.exists() and out.stat().st_size > 20000:
            kept += 1
            continue
        time.sleep(DELAY)
        try:
            out.write_bytes(get(f"{SITE}/static/img/memorylobby/original/{stem}.webp", binary=True))
            got += 1
        except urllib.error.HTTPError as e:
            missing.append(f"{sid} {name} ({stem}: {e.code})")
    print(f"{got} fetched, {kept} already there, {len(missing)} without: {'; '.join(missing[:40])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
