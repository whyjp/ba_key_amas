# 0001 A resident add-on, passive first, scan by hand

Status: Accepted

## Context
The project began as a one-shot roster scanner. Players want guidance while they play (mission facts, squad
fit) more than another export, and the game window appears, closes and restarts at any time.

## Decision
`baocr` with no arguments runs as an add-on. It finds the game window whenever it exists, docks a card
against it, and reads the screen passively. The roster scan is one feature, started from the card by the
player, and is the only thing that sends input to the game. Output of the scan is JSON only.

## Alternatives
- A separate overlay program and a separate scanner: two processes to start and keep consistent.
- Scanning automatically on a schedule. Rejected, because input to the game must be an explicit act.

## Consequences
- One process, one settings file, one tray icon that can always close it.
- The loop must survive the game being absent, restarted or covered (`forget`, region re-detection).

## Evidence
README add-on section; `cli/src/assist.rs`; started before and after the game in live runs.

## Revisit when
The game offers an official overlay or API.
