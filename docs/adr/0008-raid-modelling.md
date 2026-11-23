# 0008 Raids as difficulty rows with seasons, recognised by screen

Status: Accepted. Details: [raid-data.md](../raid-data.md).

## Context
Total assault and similar raids were collected as one row each, with one attack and one armor type taken from
the Normal difficulty. The raw data has per-difficulty enemy lists, and attack types change from the Insane
difficulty (Binah Normal to Pierce, Hieronymus Normal to Explosion). The game shows several raid screens, and
the formation screen shows neither boss nor difficulty.

## Decision
- Database: `raid_difficulty` and `raid_difficulty_enemy` per raid kind, id and difficulty index (Normal,
  Hard, VeryHard, Hardcore, Extreme, Insane, Torment, Lunatic), seasons of both modes, a display name for
  time attacks, schema versioned with a clear error for an older file.
- Lookup: by name (all candidates are returned, never the first), difficulties, the season active at `now`
  or the nearest, and a conversion to the same brief that missions use.
- Screens: each raid screen is recognised from header and anchor words (select, mock boss list, room info,
  three formation modes, unrestricted raid pages, time attack, eliminate open or closed). A raid tracker
  remembers boss, difficulty and mode for the formation screen.
- A raid has up to four parties; the verdict is for the party being edited and says so.

## Alternatives
- Keep one row per raid: wrong from Insane and for multi-part bosses.
- Multi-party optimisation: no rules data exists; not built.

## Consequences
- 15 of 15 mock bosses resolve to DB armor consistent with the screen after the weighting model (ADR 0006).
- The terrain tab on the mock list is a colour, not text: shown as a choice, not resolved.
- Unlearned: scrolled Insane and higher rows, the real eliminate screen (closed until its season), the quick
  formation popup over a raid formation.

## Evidence
`fixtures/screens/raid-*`, `extract/src/raid.rs`, `assist/src/raid_lookup.rs`, `docs/raid-data.md`.

## Revisit when
Eliminate raids open, or the game changes the raid UI.
