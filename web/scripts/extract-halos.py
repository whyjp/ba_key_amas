"""Downloads each student's halo image (public/halos/<id>.png) and writes public/halos.json (the ids that have one).

Source: the HALO list page of the Blue Archive wiki (bluearchive.wikiru.jp), where the halos are drawn from the
students' standing art and 3D models. Entries there are keyed by the Japanese name, so each is matched to a
student through the Japanese name in SchaleDB (an outfit falls back to its base character's halo, because the
halo identifies the character). The images are not committed: this runs in the deploy build, after
extract-assets.py. Rights stay with the wiki's authors and the game's rights holders (Nexon, Nexon Games, Yostar).

The page names its files loosely, and this script reads them that way:
  - `HALO_リオ_仮v2.png`, `HALO_ツクヨ2.png`, `HALO_アコ_700px.png`: a variant tail is stripped; the plain file wins,
    then a numbered or sized one, then a provisional (仮) one.
  - `HALO_icon_カズサ.png`: an `icon_` prefix is stripped.
  - `HALO_御坂.png` for 御坂美琴: a wiki name that is the start of exactly one student's name matches that student.
  - A row's picture is at `attach2/<hex>.png` or at `./?plugin=ref&page=...&src=<file>`; both are read.
Run audit-assets.py afterwards to see who still has no halo.

Run from web/:  python -I scripts/extract-halos.py
"""
import html as htmllib
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
TAIL = re.compile(r"(_?仮|_?v\d+|_?\d+px|_?\d+)$")


def get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def canonical(file: str) -> tuple[str, int]:
    """`HALO_リオ_仮v2.png` -> (`リオ`, preference). 0 plain, 1 numbered or sized, 2 provisional."""
    stem = file[len("HALO_"):-len(".png")]
    stem = re.sub(r"^icon_", "", stem)
    pref = 0
    while True:
        m = TAIL.search(stem)
        if not m or m.start() == 0:
            break
        pref = max(pref, 2 if "仮" in m.group(0) else 1)
        stem = stem[:m.start()]
    return stem, pref


def base(name: str) -> str:
    return re.sub(r"[（(][^）)]*[）)]$", "", name).strip()


def rows(page: str) -> dict[str, str]:
    """file name -> picture URL, for every row of the list."""
    out = {}
    for f, src in re.findall(r'openfile=(HALO_[^"&]+?\.png)"[^>]*>\s*<img[^>]*data-src="([^"]+)"', page):
        path = htmllib.unescape(src)
        out[urllib.parse.unquote(f)] = WIKI + (path[2:] if path.startswith("./") else path)
    return out


def match(name: str, best: dict[str, tuple[int, str]]) -> str | None:
    """The wiki key for a student's Japanese name: exact, then without the outfit, then a unique prefix."""
    for candidate in (name, base(name)):
        if candidate in best:
            return candidate
    b = base(name)
    near = [k for k in best if len(k) >= 2 and b.startswith(k)]
    return near[0] if len(near) == 1 else None


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    url_of = rows(get(WIKI + "?" + urllib.parse.quote(PAGE)).decode("utf-8", "replace"))
    best: dict[str, tuple[int, str]] = {}
    for f in sorted(url_of):
        name, pref = canonical(f)
        if name not in best or pref < best[name][0]:
            best[name] = (pref, f)
    print(f"{len(url_of)} halo pictures for {len(best)} names")

    catalog = json.loads((ROOT / "src" / "catalog.json").read_text(encoding="utf-8"))
    ids = sorted({e["id"] for e in catalog.values()})
    jp = json.loads(get("https://schaledb.com/data/jp/students.min.json"))
    out = ROOT / "public" / "halos"
    out.mkdir(parents=True, exist_ok=True)
    fetched: dict[str, bytes] = {}
    have, missing = [], []
    for sid in ids:
        name = jp.get(str(sid), {}).get("Name", "")
        key = match(name, best)
        if key is None:
            missing.append(name or str(sid))
            continue
        dest = out / f"{sid}.png"
        if not dest.exists() or dest.read_bytes()[:4] != PNG:
            file = best[key][1]
            if file not in fetched:
                time.sleep(PAUSE)
                data = get(url_of[file])
                if data[:4] != PNG:
                    raise RuntimeError(f"not a PNG: {file} ({url_of[file]})")
                fetched[file] = data
            dest.write_bytes(fetched[file])
        have.append(sid)
    (ROOT / "public" / "halos.json").write_text(json.dumps(have), encoding="utf-8")
    print(f"{len(have)} students with a halo, {len(missing)} without: {', '.join(missing[:12])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
