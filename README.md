# Blue Archive add-on

An add-on app for Blue Archive (Windows, Global/Korean, 4:3 windowed). Run `baocr` with no arguments:
it finds the game window whenever it appears (start it before or after the game, or restart the
game), docks a small card against the window's left or right side (drag its header to move it; it
snaps to the nearest side and remembers where), and hides while another window covers the game.
It only looks at the game (screenshots + OCR): guide cards and icons (below) never send input.
The student scan is one of its features, started by hand from the card's 설정 menu (the only
thing that sends input to the game): the menu shows when the stored student list was last updated,
the scan reads every student, and "상세 포함" also reads the stat popup and the two tabs (slower).
Output is JSON only (`students.json`, `index.json`, `details/NNN/student.json`) in
`%APPDATA%\baocr\students` (or `--out`). Settings: `%APPDATA%\baocr\overlay.json`.

# Blue Archive student data extractor

## Rust implementation (current)

`rust/` is a Cargo workspace that replaces the Go program. One binary, no Python,
no C compiler, no Tesseract: PP-OCRv5 (Korean) runs in process through ONNX Runtime.

```text
# one-time: put the OCR models in models/ (see models/README.md), then
cd rust
cargo build --release
target/release/baocr --image ../fixtures/detail-shun.png --out out-one        # parse a saved screenshot
target/release/baocr                                                          # the add-on (see above)
target/release/baocr --scan-arrows --from-first --out scan                    # scan the roster from the command line (HCU drives the game)
target/release/baocr --scan-arrows --from-first --fast --out scan             # skip the menu pass
target/release/baocr --reextract scan [--ids 003,007]                         # retry from saved frames, no game input
target/release/baocr --watch --out db                                         # passive background updater
cargo run -p baocr-namedict                                                   # refresh the dictionary after a patch
cargo run -p baocr-wikidb --release                                           # build ../wikidb/wiki.db (missions, enemies, students)
target/release/baocr --assist                                                 # guide card beside the game window (no input)
target/release/baocr --learn shots                                            # save every new screen + its text while you play (no input)
```

Build output goes to `CARGO_TARGET_DIR` (use a drive with space; C: was nearly full).

How names and the bond number are read: the ink-isolated line is scored against
every dictionary name (or 1..=100) with the recogniser's own CTC probabilities
and the most probable candidate wins, so the result can only be a student; a
read that is not clearly ahead stays empty. On the 106 saved frames the Rust
extractor matches the Go scan on name, bond, the four stats, skills, rarity,
pin, role and weapon stars (all 106), and reads the level of the 67 students
the Go/Tesseract path left empty. Known gap: equipment item names are only
verified for tier 4 (icon catalog); Go and Rust disagree on tier 5/6 slots.

## Guide card (`--assist`)

A passive assistant: it only takes screenshots (no clicks, keys or scrolls). It
recognises the screen from the text OCR reads (main menu, 업무, 임무 list, 임무
정보 popup, hex map, 편성, 현상수배 / 특별의뢰 / 학원교류회 lists, student list and
profile), remembers the mission shown on the 임무 정보 popup (the hex map and the
squad screen do not name it), and draws a card **beside** the game window (right,
else left; never on it) with the screen, the mission, its terrain, the enemies'
attack and armor types and the student attack types that do best against them.
The card window is click-through and excluded from screen capture; if the OS
cannot exclude it, it is not shown. Mission data comes from the wiki DB
(`wikidb/schema.sql`, built by `baocr-wikidb` from SchaleDB's Global/Korean data).
`wiki.db` is a local build artefact, not redistributed.

In the 빠른 편성 popup it also draws small icons **on** each student portrait (the
mission terrain's adaptation face as the game shows it, attack type, armor type) from a
second click-through layer over the game, and the card gains a squad verdict (유리 /
평균 / 불리) with attack, defense and terrain bars. The verdict is a rough guide: it
compares the squad's attack and armor types and terrain adaptation with the mission's
enemies, relative to the average released student (adaptation grades are approximated as
80/90/100/110/120 %; it is not a battle simulation). The squad is read from the 편성
screen's name labels and the lime-outlined cards of the popup.
Icons also go on the squad strip at the bottom of the popup, and cards are found by colour
and read closed-set against the student dictionary (cross-checked with free OCR), so
scrolling the grid works.

**Settings menu.** The add-on's main card sits beside the game window and is clickable
(it never takes focus from the game): "설정" opens a menu with a master switch for
everything drawn inside the game and, per widget (임무 카드, 편성 판정, 초상화 아이콘,
슬롯 스탯), where it goes: beside the game, inside it, or off. The choice is saved in
`%APPDATA%\baocr\overlay.json`.

**Structure** (`baocr-assist`): `observe` (frame in, `Observation` out), `widgets` (each a
`Widget`: observation in, rows or icons out), `settings`, `surface` (the output trait the
overlay implements; tests use a fake), `assistant` (connects them). A widget can be added
without touching the observer, and the overlay can be replaced without touching widgets.

Limits: students are identified by the name written on the card, so the popup must be in
its list view (the compact view shows no names; the card says so); the mission is known
only if `--assist` was running when its 임무 정보 popup was opened; the squad is known
from the 편성 screen or from the cards outlined in the popup (a squad edited while
`--assist` was not running is read from the visible cards and the specials' names).

Workspace crates: `core` (schema, OCR trait), `names`, `pixels`, `equipment`,
`export` (CSV/HTML/store/index), `ocr` (engine), `extract` (profile + menu pass),
`pipeline` (async workers, re-extraction), `hcu` (game control, scan loop, watcher),
`cli`, `namedict`, `wikidb` (reference DB: build + read API), `overlay` (banner and guide card). Module contracts: `rust/PORTING.md`.

The Go program below is kept only until the Rust scan and watcher have been
verified live against the game; it is superseded.

## Requirements and limits

- Game in **4:3, windowed**, SDR. The window position is found automatically
  (`--region auto`); pass `--region x0,y0,x1,y1` only to override. 16:9 and
  fullscreen are not supported. **Turn Windows HDR off**: with HDR the game is
  captured washed-out and the colour-based readers (locks, badges, terrain
  faces, ink isolation) stop working.
- Microsoft HCU (`vendor/hcu/computer-use.exe`, or `$HCU_BIN`) drives clicks; the
  OCR models live in `models/` (see `models/README.md`).
- Tested live: full-roster scan (106 students, 7.4 min fast / ~26 s per student
  with the menu pass), passive `--watch`, window auto-detection.
- Guide card verified live for campaign 19-3 (임무 정보, 임무 지도, 편성) and a 현상수배
  stage popup; 특별의뢰 and 학원교류회 screens are recognised from recorded frames only.
  Event missions, 총력전 and 종합전술시험 are not covered yet.
- Not verified: 3-digit bond rank (no sample), 능력개방 unlocked state, equipment
  icons beyond tier 4, a live level-up appearing in `--watch`.

## History

The first implementation was Go + Tesseract (commit `2847447`) with Python
prototypes before it (both removed; see git history). The Rust port replaced the Go program after it matched it on all 106
saved frames and in a live full scan (names, bond, stats, skills, rarity, pin,
role identical; levels the Go version could not read are now read).
