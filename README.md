# Blue Archive OCR prototypes

## Portable Go detail extractor (current)

`cmd/baocr` is a compiled Go program. It uses Microsoft Hybrid Computer Use
(HCU) for live screen capture/clicks and invokes local Tesseract with Korean and
English model files for OCR. **It does not call an LLM, game memory/files, an
account API, or a remote OCR service.** Its item matcher compares game pixels
with embedded equipment icons; the 90 icon/name references were collected from
[Blue Archive Wiki Equipment](https://bluearchive.wiki/wiki/Equipment). The
user-suggested [Models](https://bluearchive.wiki/wiki/Models) page hosts 3D
character models, not equipment data.

A 1280×960 real screenshot has been checked for Saori (Dress): all four skill
levels, bond, unique weapon, and equipment slots 1–3. Each equipped item has
`equipped`, `type`, `item_name`, `tier`, `level` and an icon-match error.
The fourth slot is separate *unique gear*; this screenshot visibly says EMPTY,
so `equipped=false` and its item fields are null. Unknown or ambiguous values
remain null, not inferred from student identity or a wiki profile. Raw OCR and
the screenshot are preserved as evidence. The current UI-coordinate calibration
and icon-match threshold have been tested on **one student** and must not be
assumed accurate for the full roster.

```text
# Native binaries/models supplied separately via explicit paths on this host:
TESSERACT_BIN=<path-to-tesseract> TESSDATA_DIR=<path-to-tessdata> go test -mod=mod ./...
go build -mod=mod -o dist-go/baocr.exe ./cmd/baocr
TESSERACT_BIN=<path-to-tesseract> TESSDATA_DIR=<path-to-tessdata> dist-go/baocr.exe --image fixtures/detail-saori.png --out output-go-equipment
# --live-one additionally requires HCU_BIN and a visible roster; it clicks one card.
```

The Go source embeds icon templates and the catalog, but **not** Tesseract,
language models or HCU providers; configure/bundle their platform-specific
executables and data. The existing root `vendor/` contains native dependencies,
so `-mod=mod` avoids Go's module-vendoring interpretation. `student.json`,
`student.csv`, `student.html`, and `detail.png` are written together. The HTML
shows all four equipment slots and their confirmed/unknown state.

## Collected metadata (schema)

Per student (`student.json`, `student.csv`, `student.html`, roster `students.json/html`):

| Group | Fields | Source |
|---|---|---|
| Identity | `name`, `identity.base_name` (원본 이름), `outfit` (무장/수영복/…), `outfit_key`, `is_distinct_character` (`*` names such as `시로코*테러` are a different character, not a costume) | OCR + dictionary |
| Basic panel | level, `rarity_stars`, 4 skill levels, bond rank, unique weapon stars/level, 4 equipment slots, `is_pinned` (셀렉트), `combat_role` STRIKER/SPECIAL | pixels/OCR |
| Stats | `stats.{max_hp,attack,defense,healing}`, `exp.{current,required}` | OCR |
| Loadout | `tactic_role` (딜러/탱커/힐러/서포터), `position`, `attack_type`, `armor_type`, `weapon_class` | role icon shape, badge colour, OCR |
| Terrain | `terrain.{urban,outdoor,indoor}` 4=S..0=D | face emoji classification |
| Badges / locks | orange notification dots on the 3 tabs and skill/weapon/equipment growth buttons; 능력개방 and 심상개화 lock icons | pixels |
| Menu pass (`deep`) | `stat_detail` (명중·회피·치명·치명 대미지·방어관통·안정·사거리·코스트 회복력·군중제어 강화/저항·특효·최대 코스트 증가), `base_stats` (레벨 업 tab, no gear/bond), `mystic` (신비 해방 tab) | menus |
| Account (once per run) | AP, credits, pyroxene | header |
| Reference | school, club, full name, birthday, height, hobby, designer, illustrator, CV, base star grade, weapon type | SchaleDB (never mixed into pixel fields) |

Reference data only produces `warnings` when it disagrees with pixels
(`*_mismatch`). Static profile text (학생 소개 popup) is taken from the
reference rather than OCR'd per student.

`--scan-arrows` runs the menu pass by default (about 25 s/student: it opens
상세, 레벨 업 and 신비 해방 and returns to 기본 정보; it never presses an
upgrade/unlock button). `--fast` skips it (about 6 s/student). Use
`--grab out.png` to save the current game region without input.

## Background updater and async scan

- `--watch --out <dir>`: passive updater. Takes screenshots only (no click/key/scroll),
  waits for a profile to hold still, reads the variable fields and stores what
  changed in `<dir>/students/*.json` plus `<dir>/history.jsonl`. Ctrl-C stops it.
- `--scan-arrows` extracts asynchronously (`--workers`, default 3); every student has an
  entry in `index.json` and its frames in `details/NNN/`. Retry offline with
  `--reextract <dir> [--ids 003,007]`.
- Architecture, OCR engine measurements and the overlay plan: `docs/ARCHITECTURE.md`.

## Collection stages (fixed vs variable)

| Stage | What | How | When |
|---|---|---|---|
| 1. Fixed meta | base name/outfit, role (딜러…), position, attack/armor type, weapon class, base terrain, equipment slot types, growth materials (names only), profile text | **dictionary** (`go run -mod=mod ./cmd/namedict`, SchaleDB) | only after a game patch |
| 2. Variable | level, exp, stars, skills, bond, weapon, equipment, 4 stats, pin, notification dots, menu pass (`deep`) | live scan | every collection |
| 3. Single update | one profile re-read, no clicks | `--detail-current` | any time |

Routine scans do **not** OCR the fixed groups; `student.json` carries them as
`fixed_meta` (`source: "dictionary"`). Add `--verify-fixed` to also read them
from pixels and get `*_mismatch` warnings (use it to audit the dictionary after
a patch). Material **quantities** and credit costs are never collected (they
depend on what the player did); only which materials a student needs.

## Screen setup (important)

All coordinates are calibrated on the game's **4:3** layout (1280x960 client)
in **windowed** mode. 16:9 moves the panels and shrinks the text; fullscreen has
not been verified. Set the game to a 4:3 screen ratio before scanning. The
scanner rejects a non-4:3 `--region`, stops if the first profile's panel is
unreadable (`layout_mismatch`), and records the capture conditions in the
report (`capture`).

## TODO / unverified

- 능력개방 (`PotentialMaterial`, the right-hand lock button): the author's
  account has none unlocked, so the unlocked state, its level and any related
  screen are **not implemented**. Only "locked" is detected (`locks`).
- Terrain face grade 1 and the T.S role icon have no sample yet (read as null).
- `weapon_growth` notification dot position is unverified (never seen lit).
- The unique weapon can raise a terrain grade (`fixed_meta.weapon_terrain*`);
  face grades are only audited against the dictionary base with that in mind.
- 16:9 and fullscreen layouts.

## Name correction dictionary

Student names are read by Tesseract, then snapped to a closed dictionary
(`cmd/baocr/student_names_kr.json`, 281 names, embedded). The dictionary is
generated by `go run -mod=mod ./cmd/namedict` from SchaleDB's Korean data;
bluearchive.wiki only has EN/JP names. Matching is by jamo edit distance with a
margin over the runner-up, so a near-tie becomes `ambiguous` (name left empty)
instead of a wrong student. Short names allow 1 jamo of slack, longer ones 2.
Each export keeps `raw_ocr.name` plus `name_match` (status exact / corrected /
ambiguous / unlisted / empty, distance, margin). The dictionary never fills
other fields; its squad role only raises a `role_mismatch` warning.
Names are canonical in the dictionary's spelling (e.g. `시로코*테러`, ASCII `*`).

Roster-card names (small portraits) are low quality; use the profile/arrow
path for names. Gold-set test: `TestGoldNamesFromRealScreens`.

## Legacy Windows roster prototype

Windows-only, Korean UI. Reads *screen pixels* using Windows built-in Korean OCR;
no game memory reads, game files, account tokens or network APIs. This is a
prototype calibrated against a 1280×960 game client on the **학생 리스트 → All**
page. Other resolutions are scaled, but different layouts require calibration.

## Output now verified

`dist/bluearchive-ocr.exe --image <roster-screenshot.png> --output output`

The existing game-window screenshot was used to generate `output/` with 11
visible, complete cards. The third row was cropped and is intentionally not
reported. `students.json`, Excel-friendly UTF-8-BOM `students.csv`, and
standalone sortable/filterable `students.html` are generated together.
Empty fields (`null`) mean the screen did not prove the value.

## Live mode (NOT validated yet)

**This sends foreground clicks and scroll events to the game.** Bring the
Blue Archive student roster to the front and leave it in `All`; then manually
run:

```text
dist\bluearchive-ocr.exe --scan-live --output output --max-pages 40
```

This first captures two full rows per page, deduplicates names, then clicks
each card to OCR its detail screen and returns with Escape. It stops when
no new names appear on three pages, if the game loses foreground, or if the
expected roster/detail transition fails. The script checkpoints CSV/JSON/HTML
and retains evidence screenshots. Use `--roster-only` to avoid opening details.
It does not infer icon-only stars (rarity or weapon) without a calibrated
visual detector: those columns stay null unless explicitly recognized text.
The detail-click/scroll behavior has **not** been exercised on this machine
because interactive input was not approved. Expect layout calibration.

## Build

```text
uv venv .venv --python 3.13
uv pip install --python .venv\Scripts\python.exe -r requirements.txt
.venv\Scripts\python.exe -m pytest -q
.venv\Scripts\python.exe -m PyInstaller --noconfirm --clean --onefile --name bluearchive-ocr --collect-all winrt --hidden-import winrt.windows.media.ocr --hidden-import winrt.windows.globalization --hidden-import winrt.windows.graphics.imaging --hidden-import winrt.windows.storage.streams bluearchive_ocr.py
```

Requires Windows Korean OCR language capability. The executable bundles the
Python runtime but relies on the host Windows OCR engine/language pack. Verify
locally with `dist\bluearchive-ocr.exe --image screenshot.png` before live mode.

## Schema

Name, level, rarity stars, EX/basic/passive/sub skill levels, bond rank,
unique-weapon stars/level, combat role, raw detail OCR, evidence path, status,
warnings. Unknown values are null. The viewer renders OCR as text, not HTML.
Student rarity and unique-weapon rarity are separate concepts. The four
Korean skill labels are EX, 기본, 강화 and 서브; 인연 랭크 is another separate
student-specific value. The official student-info guide confirms detail
pages are needed for skills and equipment:

- https://forum.nexon.com/bluearchive-en/board_view?allBoard=1&board=3222&thread=2523127
- https://forum.nexon.com/bluearchive/board_view?board=1076&thread=1749035
- https://forum.nexon.com/bluearchive/board_view?board=1076&thread=3141076

The last notice expanded unique weapons beyond old ★3/level-50 guides;
therefore no old weapon-rank ceiling is hard-coded. Star icons need a
separate calibrated detector and currently remain null unless OCR sees
explicit labeled text.

## Limitations

- Screenshot evidence only confirms 11 visible students, **not the whole account**.
- Live detail pages, scrolling, and icon-star calibration are unverified.
- No memory/process/file access to the game. OCR may misread Korean names;
  check evidence screenshots and warnings before treating exports as authoritative.
- The live foreground-input path is intended for the owner to run manually.
