"""Builds the student catalog (src/catalog.json, keyed by student id: names are not unique) and the portrait assets (public/portraits/<id>.webp).

Source: SchaleDB's Korean student table, restricted to students released on the Global server (the
same rule as the scanner's wiki.db). Neither output is committed: this runs in the deploy build
(`npm run assets`), so the repository holds no game images.

It also writes public/meta.json (Korean names of schools, attack and armor types, and the Global story events in
release order) and downloads the small student icons and the school logos.

Run from web/:  python -I scripts/extract-assets.py
"""
import json
import pathlib
import sys
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASE = "https://schaledb.com"
GLOBAL_FLAG = 1  # IsReleased = [JP, Global, CN]
EQUIPMENT_TYPES = {"Hat", "Gloves", "Shoes", "Bag", "Badge", "Hairpin", "Charm", "Watch", "Necklace"}


def get(url: str) -> bytes:
    """One polite request; a dropped connection is retried with a growing pause."""
    req = urllib.request.Request(url, headers={"User-Agent": "ba-key-amas-assets"})
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return r.read()
        except OSError:
            if attempt == 4:
                raise
            time.sleep(2 * (attempt + 1))
    raise RuntimeError("unreachable")


def is_image(data: bytes) -> bool:
    """PNG, JPEG or WebP by its first bytes. SchaleDB answers a missing image with its HTML page and status 200, which
    must never be saved under an image name."""
    return data[:4] == bytes([137, 80, 78, 71]) or data[:3] == bytes([255, 216, 255]) or (data[:4] == b"RIFF" and data[8:12] == b"WEBP")


def main() -> int:
    students = json.loads(get(f"{BASE}/data/kr/students.min.json"))
    catalog = {}
    backgrounds = set()
    for s in students.values():
        released = s.get("IsReleased") or []
        if len(released) <= GLOBAL_FLAG or not released[GLOBAL_FLAG]:
            continue
        backgrounds.add(s.get("CollectionBG"))
        catalog[str(s["Id"])] = {
            "id": s["Id"],
            "name": s["Name"],
            "role": s.get("TacticRole"),
            "school": s.get("School"),
            "attack": s.get("BulletType"),
            "armor": s.get("ArmorType"),
            "squad": s.get("SquadType"),
            "star": s.get("StarGrade"),
            "position": (s.get("Position") or "").upper() or None,
            "equipment": s.get("Equipment"),
        }
    (ROOT / "src").mkdir(exist_ok=True)
    (ROOT / "src" / "catalog.json").write_text(json.dumps(catalog, ensure_ascii=False, sort_keys=True), encoding="utf-8")

    pub = ROOT / "public"
    fetched = 0
    for folder, url, suffix in (
        ("portraits", "images/student/collection/{id}", "webp"),
        ("icons", "images/student/icon/{id}", "webp"),
        ("standing", "images/student/portrait/{id}", "webp"),
        ("lobby", "images/student/lobby/{id}", "webp"),
    ):
        out = pub / folder
        out.mkdir(parents=True, exist_ok=True)
        for entry in catalog.values():
            dest = out / f"{entry['id']}.{suffix}"
            if dest.exists():
                continue
            data = get(f"{BASE}/{url.format(id=entry['id'])}.{suffix}")
            if not is_image(data):
                print(f"no {folder} image for {entry['id']}")
                continue
            dest.write_bytes(data)
            fetched += 1
    # backgrounds for the cards: the collection backgrounds the students use, 56 or so
    names = sorted(b for b in backgrounds if b)
    (pub / "backgrounds").mkdir(parents=True, exist_ok=True)
    for b in names:
        dest = pub / "backgrounds" / f"{b}.jpg"
        if not dest.exists():
            data = get(f"{BASE}/images/background/{b}.jpg")
            if is_image(data):
                dest.write_bytes(data)
                fetched += 1
            else:
                print(f"no background image {b}")
    (ROOT / "src" / "backgrounds.json").write_text(json.dumps(names), encoding="utf-8")
    schools = sorted({e["school"] for e in catalog.values() if e["school"]})
    (pub / "schools").mkdir(parents=True, exist_ok=True)
    for school in schools:
        dest = pub / "schools" / f"{school}.png"
        if not dest.exists():
            data = get(f"{BASE}/images/schoolicon/{school}.png")
            if is_image(data):
                dest.write_bytes(data)
                fetched += 1
            else:
                print(f"no school logo for {school}")

    loc = json.loads(get(f"{BASE}/data/kr/localization.min.json"))
    config = json.loads(get(f"{BASE}/data/config.min.json"))
    event_ids = [e for e in config["Regions"][GLOBAL_FLAG]["Events"] if e < 10000]  # 10000+ are reruns
    # one item per (type, tier): the name a scan never has to read. Blueprints and tier 0 are not worn items.
    equipment: dict[str, dict[str, dict]] = {}
    for item in json.loads(get(f"{BASE}/data/kr/equipment.min.json")).values():
        if item["Category"] in EQUIPMENT_TYPES and 1 <= item["Tier"] <= 10 and "설계도" not in item["Name"]:
            equipment.setdefault(item["Category"], {})[str(item["Tier"])] = {"name": item["Name"], "max_level": item.get("MaxLevel")}
    # characters, outfits merged by name: what the card grades are measured against
    import re
    characters = {re.sub(r"\s*\([^)]*\)\s*$", "", e["name"]).strip() or e["name"] for e in catalog.values()}
    meta = {
        "character_count": len(characters),
        "school_logos": sorted(p.stem for p in (pub / "schools").glob("*.png")),
        "equipment": equipment,
        "schools": loc["School"],
        "attack": loc["BulletType"],
        "armor": loc["ArmorType"],
        "events": [{"id": e, "name": loc["EventName"].get(str(e), str(e))} for e in event_ids],
        "backgrounds": names,
        "bond_max": config["Regions"][GLOBAL_FLAG]["BondMaxLevel"],
        "level_max": config["Regions"][GLOBAL_FLAG]["StudentMaxLevel"],
    }
    (pub / "meta.json").write_text(json.dumps(meta, ensure_ascii=False), encoding="utf-8")
    print(f"{len(catalog)} students, {fetched} portraits downloaded")
    return 0


if __name__ == "__main__":
    sys.exit(main())
