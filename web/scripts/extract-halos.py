"""Downloads each student's halo image (public/halos/<id>.png) and writes public/halos.json (the ids that have one).

Source: the HALO list page of the Blue Archive wiki (bluearchive.wikiru.jp), where the halos are drawn from the
students' standing art and 3D models. Entries there are keyed by the Japanese name, so each is matched to a
student through the Japanese name in SchaleDB (an outfit falls back to its base character's halo, because the
halo identifies the character). The images are not committed: this runs in the deploy build, after
extract-assets.py. Rights stay with the wiki's authors and the game's rights holders (Nexon, Nexon Games, Yostar).

Run from web/:  python -I scripts/extract-halos.py
"""
import json
import pathlib
import re
import sys
import time
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
WIKI = "https://bluearchive.wikiru.jp/"
PAGE = "HALO一覧"
UA = "ba-key-amas-assets (build-time, one request per image)"
PAUSE = 0.3
PNG = bytes([137, 80, 78, 71])


def get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def entry_name(file: str) -> tuple[str, int]:
    """`HALO_セリカv2.png` -> (`セリカ`, preference). A plain file wins over a numbered, sized or provisional one."""
    stem = file[len("HALO_"):-len(".png")]
    plain = re.sub(r"(v\d+|_\d+px|_\d+|_仮)$", "", stem)
    return plain, 0 if plain == stem else 1


def base(name: str) -> str:
    return re.sub(r"[（(][^）)]*[）)]$", "", name).strip()


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    html = get(WIKI + "?" + urllib.parse.quote(PAGE)).decode("utf-8", "replace")
    # each row links the attach plugin and carries the real file path in the lazy-load `data-src`
    rows = re.findall(r'openfile=(HALO_[^"&]+?\.png)"[^>]*><img[^>]*data-src="(attach2/[^"]+\.png)"', html)
    files = sorted({urllib.parse.unquote(f) for f, _ in rows})
    path_of = {urllib.parse.unquote(f): src for f, src in rows}
    best: dict[str, tuple[int, str]] = {}
    for f in files:
        name, pref = entry_name(f)
        if name not in best or pref < best[name][0]:
            best[name] = (pref, f)
    print(f"{len(files)} halo files for {len(best)} names")

    catalog = json.loads((ROOT / "src" / "catalog.json").read_text(encoding="utf-8"))
    ids = sorted({e["id"] for e in catalog.values()})
    jp = json.loads(get("https://schaledb.com/data/jp/students.min.json"))
    out = ROOT / "public" / "halos"
    out.mkdir(parents=True, exist_ok=True)
    have, missing = [], []
    cache: dict[str, bool] = {}
    for sid in ids:
        name = jp.get(str(sid), {}).get("Name", "")
        key = name if name in best else base(name) if base(name) in best else None
        if key is None:
            missing.append(name or str(sid))
            continue
        dest = out / f"{sid}.png"
        file = best[key][1]
        if not dest.exists():
            if file not in cache:
                time.sleep(PAUSE)
                cache[file] = True
                data = get(WIKI + path_of[file])
                if data[:4] != PNG:
                    raise RuntimeError(f"not a PNG: {file}")
                dest.write_bytes(data)
            else:
                dest.write_bytes((out / f"{next(i for i in have if best_key(i, jp, best) == key)}.png").read_bytes())
        have.append(sid)
    (ROOT / "public" / "halos.json").write_text(json.dumps(have), encoding="utf-8")
    print(f"{len(have)} students with a halo, {len(missing)} without: {', '.join(missing[:12])}")
    return 0


def best_key(sid, jp, best):
    name = jp.get(str(sid), {}).get("Name", "")
    return name if name in best else base(name)


if __name__ == "__main__":
    sys.exit(main())
