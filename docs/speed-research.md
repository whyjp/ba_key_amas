# Speed research: screen recognition and reaction time

Machine: Ryzen 7 5700X (8 cores), RTX 3080 Ti, desktop 3640x2160, the game's client 1289x967 native (4:3).
Other agents were building and running on the same machine, so every timing below is a median of 3 to 10
runs and can move by 10 to 25 percent between sessions (the same quick-formation OCR read 534, 557, 562 and
667 ms in four sessions). Differences smaller than that are noise. Nothing here sent input to the game; the
only desktop contact is read-only `BitBlt` captures.

## Tools (all rerunnable, none touches the game except read-only captures)

| tool | what it answers |
|---|---|
| `cargo run --release -p baocr-ocr --example speed_lab [--features directml] [variant ...]` | ONNX Runtime and pipeline options and DirectML: time and text equality against the committed `fixtures/screens/*.boxes.txt` (15 frames) and against the library defaults |
| `cargo run --release -p baocr --example speed_timeline` (`SLEEP=1`, `LIVE=1`, `WATCH=secs`) | per-stage costs offline, sleep granularity, live capture split (layout / BitBlt / resize), single-grab prototype, cheaper frame key, passive watching of the player's own play |
| `cargo run --release -p baocr --example speed_quick` | 편성 > 빠른 편성: card reading with OCR cut to the grid or to the label bands, recogniser vs CTC split, label-ink cache, popup-open test from pixels |
| `cargo run --release -p baocr --example speed_model` (env `CAPTURE KEY OCR OBSERVE GRID_OCR BAND_OCR DML_OCR CLICK_LAG`) | a model of the loop in `cli/src/assist.rs`: reaction time per policy for a click or a scroll stop, Monte Carlo over the loop's phase |
| `cargo test -p baocr-ocr --release [--features directml] --test fixtures` | the engine reads the fixtures within the committed bound on CPU and on DirectML |

## 1. Where the time goes (quick formation, before the changes marked done in section 7)

Reaction = from the screen reaching its final state to the card being on screen. Stage costs, measured:

| stage | cost | note |
|---|---|---|
| wait for the loop to look (idle pause 300 ms + the loop's own work) | 0 to 410 ms, mean about 205 | the loop only looks every capture + key + 300 ms while idle |
| capture (`game_capture`) | 84 to 125 ms | two zooms. One `BitBlt` costs 15 to 17 ms whatever its size (a 64x48 grab costs 16.6 ms, the full client 15.4 to 16.5), so about 33 ms is the floor of two grabs; the rest is two CatmullRom resizes per half (to the HCU zoom size, then to 1280x480), conversion and stitching. `layout()` is 0.1 ms: not a cost |
| `frame_key` + distance | 6.4 to 9.0 ms | a CatmullRom resize of 1.2 MP to 32x32 |
| settle wait: pause + a second matching capture | 62 ms + capture + key, about 175 ms | `still >= 1` needs two equal frames |
| `ocr.boxes` (det + rec), quick-formation | 475 to 667 ms | see 2 |
| `read_card_names` | 85 to 115 ms | recogniser 75 to 83 ms (about 6.5 ms per label), CTC scoring over 281 names 13 to 22 percent |
| classify + `present` | under 1 ms | not a cost |
| overlay thread's next poll | 0 to 90 ms, mean 45 | now removed, see section 5 |
| draw (`UpdateLayeredWindow`) | a few ms | the render is cached |

`thread::sleep` is exact on this machine (30 ms lasts 30.3 ms, 60 ms lasts 60.3 ms): the 30 ms wait slices cost nothing extra.

Modelled reaction (`speed_model`, quick-formation costs: capture 110, key 7, OCR 534, cards 87, mean ms, 4000 runs each):

| policy | click, instant cut | click, 250 ms slide-in | scroll stop (inertia 100 ms) |
|---|---|---|---|
| today | 1125 | 1009 | 996 |
| overlay wake | 1079 | 963 | 950 |
| settle pause 20 ms | 1095 | 967 | 958 |
| fast frame key | 1106 | 994 | 982 |
| idle pause 150 ms | 1050 | 1011 | 1000 |
| DirectML (OCR 190 ms) | 781 | 665 | 652 |
| DirectML + wake + settle 20 + fast key | 686 | 561 | 552 |
| band-rec free text, when the scene is known (OCR 77 ms) | 668 | 552 | 539 |
| wake + band-rec + input hints | 471 | 420 | 384 |
| wake + band-rec + speculative OCR + hints + settle 20 + fast key | 295 | 324 | 339 |

The model is optimistic for speculative OCR, because it assumes the background OCR costs the loop nothing. It is also
optimistic for hints, because it assumes the guessed look times match the game's real animation. The 600 ms and the
250 ms inertia rows of `speed_model` show one wrong guess, where hints make things worse than the today row, up to 1.6 s. The click-lag, the
animation length and the scroll inertia are inputs, not measurements, so they need a live test.

## 2. OCR cost

All variants of `speed_lab`, 15 fixture frames, sum of per-frame median ms; "identical" = frames whose text boxes equal the
library-default output (committed fixtures: the default itself reads 13 of 15 frames as committed, with 2 boxes drifted).

| idea | sum ms (baseline 4750 to 5000) | quick-formation | identical frames | verdict |
|---|---|---|---|---|
| explicit empty ORT config | 4824 | 553 | 15/15 | same as default |
| intra threads 1 / 2 / 4 / 6 / 8 / 12 / 16 | 11694 / 7267 / 5251 / 5133 / 5149 / 4865 / 4849 | 1381 / 863 / 582 / 542 / 612 / 541 / 574 | 15/15 | the default (all cores) is already at the optimum; fewer threads is slower |
| graph optimisation off / L2 / L3 | 6569 / 6458 / 4775 | 756 / 789 / 565 | 15/15 | default equals L3 |
| memory pattern on / off | 4727 / 4763 | 554 / 561 | 15/15 | noise |
| parallel execution, inter 2 | 4821 | 561 | 15/15 | noise |
| rec batch 1 / 2 / 8 / 16 / 32 / 64 | 6751 / 6377 / 5593 / 8226 / 11652 / 12546 | 794 / 658 / 615 / 984 / 1423 / 1823 | 14 / 14 / 12 / 8 / 8 / 8 of 15 | slower and not equal: padding in a batch changes what a line reads as. The CPU default of 4 is the best and now fixed in code |
| det limit_side_len 800 / 736 / 640 / 512 | 4379 / 4261 / 3861 / 3496 | 491 / 428 / 364 / 325 | 1/15 each | faster (640: 25 percent less) but the text of nearly every frame changes: disqualified without a per-screen accuracy study |
| det at full resolution (1280) | 5949 | 578 | 1/15 | slower and different |
| DirectML, rec batch 64 (the GPU default) | 1447 | 152 | 8/15 | fast but reads 10 boxes differently |
| **DirectML, rec batch 4** | **1710 to 1735** | **185 to 190** | **15/15** | **2.7x faster, same text as the CPU on all 15 frames** |
| DirectML, rec batch 1 / 8 | 3002 / 1506 | 350 / 157 | 14 / 12 of 15 | worse than 4 |

DirectML cold run: the first call costs 444 ms once (kernel setup); later first-time frames cost the same as warm ones
(sum of cold runs 2123 ms against 1710 warm), so new screen shapes do not recompile. Session creation 400 to 560 ms
against 170 ms. The runtime copies `DirectML.dll` beside the exe (it must ship with it).
Per frame, lobby-idle falls from 87 to 104 ms to 19 to 21 ms, and quick-formation from 547 ms to 185 to 190 ms.

Not a win, measured:
- Three OCR strips (header, popup-title band, bottom bar) to classify the scene instead of the whole frame: 100 to 350 ms,
  not clearly cheaper than 100 to 560 ms of the full frame (per-call overhead), same scene on all frames that classify.
- Parallel `choose` over cards: 96 to 70 ms in one session, 249 to 113 ms in another. The recogniser session is behind a mutex,
  so only the CTC share (13 to 22 percent) parallelises. The gain is small.

Detector versus recogniser cost: sparse screens are detector dominated (lobby-idle 101 ms falls to 25 ms at limit 512), dense ones
split roughly 60/40 (quick-formation 562 -> 325). Running only the recogniser on known boxes of template screens would
therefore save up to 60 to 70 percent on them. It is not prototyped. It needs a template matcher and a safety check per screen,
and DirectML already takes the detector to a few tens of milliseconds.

## 3. 편성 > 빠른 편성, the three cases the user named

Everything for quick formation is in `speed_quick` (fixtures `quick-formation*`, 4 frames, 12 cards each).

1. Scroll stops. At baseline, the settle wait is about 175 ms, full-frame OCR 475 to 565 ms and `read_card_names` 85 to 115 ms.
   * OCR only the grid and its tabs (crop x 560, y 300, 720x360): 231 to 358 ms instead of 476 to 565 ms; the cards read are
     identical on 4 of 4 frames (`read_card_names` 82 to 95 ms).
   * Free text per label band only (one recogniser call on each 92 px wide label band, 75 to 78 ms for 12 labels, no detector):
     cards identical on 4 of 4 frames. Together with the card reading about 165 ms instead of about 620 ms (full OCR plus cards),
     measured offline. Reading cards with no free text at all differs on 1 of 4 frames (a card is missed), so the band read is
     what keeps it safe.
   * Both only apply when the scene is already known to be 빠른 편성 and the popup is still there (`quick_tab_is_striker` is
     `Some`); the first frame of a new screen still needs the whole frame.
   * Reusing names across scroll steps: the label ink of the same card at another position differs by a Jaccard distance of
     0.04 to 0.13 (same capture noise) while different cards are 0.36 or more apart (18 same-name pairs and 6 unmatched cards
     in the two fixture pairs). A cache keyed on the ink with a 0.2 threshold would skip about half the cards after a short
     scroll (6 of 12 here), about 40 ms. An exact hash does not work (0 hits even for two captures of the same view).
2. The popup closes (back on 편성). `quick_tab_is_striker` and `quick_card_labels` are pixel tests of 0.00 to 0.01 ms and 0.06 to 0.18 ms:
   `Some` on 4 of 4 popup frames and `None` on all 21 other fixture frames. That is a free "the popup is gone" signal, so
   the icons can come down in the first looked-at frame, and the scene can be read from the first still frame. There is no
   편성-screen PNG fixture (only `squad.boxes.txt`), so recognition of the closed state was not timed or reproduced;
   why it failed once is not known from this data. With DirectML a full read of a formation-like screen is 20 to 200 ms.
3. Leaving for a battle or the base formation. The `transition()` fix takes the icons down on the first changed frame,
   which the loop sees after 0 to 410 ms (idle wait) plus 125 ms (capture plus key). Input hints cut the first part to about
   the click time + the first-look delay.

## 4. Settle and poll policy

* Frame keys of two captures of the same view (quick-formation vs its `-live-a` recapture) are 1.46 apart against `STILL = 1.5`. The
  threshold is tight, so an exact-hash revisit cache is not possible, and any key change must keep the distances. The block-mean key
  (`fast_frame_key`) gives 1.38 for that pair, 5.19 against 5.28, 75.85 against 76.09: within 0.3 on every pair tried, same side of 1.5
  on all 15 pairs in the unit test.
* Pause lengths: settling 20 ms (60 ms when measured), idle 300 ms, battle 400 ms. A shorter idle pause cuts the wait for the first look (model: 150 ms
  saves about 75 ms for an instant cut, nothing for animated ones) and costs capture CPU all the time (about a quarter of a core now:
  125 ms every 410). Better: short idle only inside 빠른 편성, where the reaction matters. Not done (needs the scene in the loop's state).
* Speculative OCR on the first changed frame, confirmed by the next matching frame: model saves up to 180 ms for an instant cut,
  nothing for scrolls (every frame moves, jobs are wasted), and loads every core while the next capture runs. Worth trying
  only with DirectML (the GPU does the work). Not built.
* Capture: a single `BitBlt` of the client plus one CatmullRom resize measured 49 to 74 ms against 84 to 125 ms for `game_capture`,
  but the frame differs (mean abs difference 1.4 levels, key distance 0.2) and OCR read 27 boxes against 28 and 28 against 29 on
  the live frame. The ROI calibrations were made on `game_capture` frames, so this is not safe as is. A safer cut in `hcu/capture.rs`
  (a merged module, so a proposal): run the two half captures fully in parallel (two `BitBlt`s are 33 ms of the total), same numerics.

## 5. Overlay response

The overlay thread polled every 90 ms (16 ms while dragging): a state change waited 0 to 90 ms, mean 45. It now waits on a
condition variable that every state setter signals, and still wakes at the same intervals for window messages and clicks.
Unit tests cover the primitive (`overlay/src/wake.rs`); the saved 45 ms mean is arithmetic, not a pixel measurement
(pixels cannot be observed from the process). The card was already cached (`render_panel_hits` memo), so the draw is a few ms.

## 6. Input hints (experimental, off by default)

`overlay/src/hints.rs`. A passive `WH_MOUSE_LL` hook thread (`InputHints`) timestamps only left-button and wheel events that are
inside the game's client rectangle. The callback tests with atomics, takes no lock and allocates nothing. It drops events outside the
rectangle at once, ignores injected events, never looks at mouse moves, keys or anything else, and logs nothing. `try_send` on a bounded queue
never blocks. The hook always calls `CallNextHookEx`, so nothing is intercepted or forwarded. The pure part, `HintScheduler`, turns events into
requests. A click asks for a look after 80 ms and a trusted single-frame look after 450 ms (no second matching frame needed). A wheel burst
or a held-button drag asks for a trusted look 160 ms after the last turn. A click inside a configured hot zone (for example the close
button of 빠른 편성) asks for an immediate pre-empt that takes the old icons down. There are 8 unit tests; the real hook was not run.

Estimated gain (`speed_model`, same cost inputs): about 150 to 200 ms for clicks, 100 to 130 ms for scroll stops, on top of the other changes,
when the guessed delays fit the game. What needs a live test, and cannot be answered without input: the game's reaction lag after a click, the
length of its animations (an animation longer than 450 ms makes the trusted look read a half-drawn screen; the model shows +600 ms for that
case), the scroll inertia (a quiet time shorter than the inertia reads a moving list), whether a held drag also scrolls the grid, and the
hook's own cost under a high-polling-rate mouse. Recommendation: experiment, gated by a settings flag, after DirectML and band reading are in.
Hot zones need button positions for the screens that matter (none are known in this repository yet).

## 7. Ranked table

| # | idea | expected | measured | risk | effort | recommendation |
|---|---|---|---|---|---|---|
| 1 | OCR on the GPU (DirectML, rec batch 4) | 2 to 3x | 4750 -> 1710 ms over the fixtures, quick-formation 547 -> 187 ms, text identical on 15/15 | the game shares the GPU (a few tens of ms of load per read); `DirectML.dll` to ship; GPU-less machines fall back to CPU by probe (`BAOCR_OCR_DEVICE=cpu` forces it); not soaked for memory over hours | done: feature `directml` in `baocr-ocr` and `baocr` | shipped in the portable package (`scripts/package-portable.ps1`); the live check of the game's frame rate while reading is still open |
| 2 | quick-formation free text from the label bands only, when the popup is known to be up | reaction -300 to -450 ms for scrolls | 620 ms -> 165 ms offline, cards identical on 4/4 frames | four fixture frames only; wrong band read could miss a card (the full-frame read is the fallback on any doubt) | about 60 lines in the observer/loop (the assistant files belong to another agent: proposal) | **do next**, validate on more live scrolls |
| 3 | overlay woken on change | -45 ms mean | arithmetic (poll period 90 ms), unit-tested wake | none | done | shipped |
| 4 | block-mean frame key in the loop | -7 to -9 ms per look | 6.4 -> 0.6 ms, distances within 0.3 | threshold sides equal on 15 fixture pairs; tight 1.5 threshold means a live check | done | shipped |
| 5 | settling pause 60 -> 20 ms | about -30 to -40 ms | modelled only | more captures during animations (capture is already about 100 ms, so +25 percent) | done (one constant) | shipped, revert if CPU matters |
| 6 | input hints (click, wheel, hot zones) | -100 to -200 ms | modelled | wrong guesses read half-drawn screens; a global hook | scheduler and hook thread done, loop not wired | **experiment** behind a setting |
| 7 | speculative OCR on the first changed frame | up to -180 ms for instant cuts | modelled | CPU contention, wasted jobs while scrolling | medium | experiment, only with DirectML |
| 8 | idle pause shorter inside 빠른 편성 | about -75 ms | modelled | steady capture CPU there | small | maybe |
| 9 | label-ink cache across scroll steps | about -40 ms | 18/18 same-name pairs under 0.2, 6/6 others over 0.36 | threshold from 4 frames | small | after 2 |
| 10 | parallel half captures in `hcu/capture.rs` | -30 to -40 ms | not measured; two BitBlts are 33 ms of 84 to 125 | merged module | small | propose |
| 11 | single BitBlt + one resize | -35 to -50 ms | 49 to 74 vs 84 to 125 ms | frame numerics change, 1 OCR box differs | small | drop unless the ROIs are recalibrated |
| 12 | ORT thread, optimisation, memory options | none | all within noise or slower | none | none | drop, defaults are optimal |
| 13 | rec batch or det limit tuning | 25 to 35 percent | batch changes text; limit 640 changes 14/15 frames | accuracy | none | drop |
| 14 | rec only on known boxes of template screens | up to 60 to 70 percent on those screens | not prototyped | needs per-screen safety | large | skip while DirectML is on |
| 15 | whole-frame hash cache for revisits | skips OCR on back navigation | not tested live; same-view recaptures are already 1.46 apart | live numbers and timers change the frame | medium | skip |

## Top 3 recommendations

1. DirectML OCR (`--features directml`) is measured at 2.7x with identical text on all 15 fixtures, and the portable package is built with it (`scripts/package-portable.ps1`). The check of the game's frame rate while it reads is still open.
2. In 빠른 편성, once the scene is known and the popup pixel test passes, read the free text per label band instead of the whole frame:
   reaction for scrolls from about 1.0 s to about 0.55 s with the CPU alone (model), and the same pixel test takes icons down at once when the popup closes.
3. Try input hints as an experimental option; do it after 1 and 2, because the model shows the click and scroll look-times only pay off when OCR is cheap.

## What was not tested

* Anything with real input: the game's reaction lag, animation lengths, scroll inertia, the real hook (never installed), whether hints help in play.
* Recognition of the closed-popup (편성) screen: there is no fixture PNG for it; the earlier failure was not reproduced.
* DirectML over hours (memory), on another GPU or on an integrated GPU, with the game rendering at high load, and the effect on the game's frame rate.
* CPU use of the idle loop and of the overlay thread after the change (the earlier `OVERLAY=` lever in `perf_loop` exists, not rerun).
* Speculative OCR, the idle-pause change and the settle-pause change on the live loop (modelled only).
* DirectML for the closed-set recogniser session (`probs`/`choose`, 6.5 ms per label on CPU): it still runs on the CPU.
* Passive watching of a real transition: `WATCH=30` saw no motion (the game was idle), so no real animation length was recorded.
