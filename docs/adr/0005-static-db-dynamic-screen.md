# 0005 Facts come from the static DB, the screen supplies keys

Status: Accepted

## Context
OCR is slow, noisy and language dependent. Dungeon facts and student types change with patches, not with the
screen. An early raid path let on-screen armor and attack words override the database, and the drum crab
case (the screen shows two armors, the DB had one) looked like a reason to trust the screen.

## Decision
Dynamic data, read from the screen: which screen, which mission, stage, boss, difficulty and mode, the
squad's names and slots, selection state. Static data, from the database only: enemy armor and attack mix,
levels, terrain, student types, terrain adaptation, roles, the type table. The add-on keeps
screen words about types only as a cross-check. It reports a mismatch, which means the DB model is incomplete.

## Alternatives
- Letting the screen win breaks silently on OCR noise and hides DB gaps.
- Using the screen only gives no facts for screens that show none (formation).

## Consequences
- A lookup is deterministic given its keys (tested with wrong or missing screen words).
- The developer fixes a mismatch in the DB model (ADR 0006) instead of special-casing the screen.

## Evidence
`docs/enemy-weighting.md` (static and dynamic table), `assist/src/raid_lookup.rs`, property test over screen
words, 15 of 15 mock bosses match the DB.

## Revisit when
A screen shows data the DB cannot have, such as live event modifiers.
