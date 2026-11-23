# 0009 Build settings that keep RAM and disk bounded

Status: Accepted

## Context
Parallel `cargo test --workspace` runs in several worktrees linked about a dozen test binaries at about 2 GB
each. The PC ran out of RAM, the game and the terminal crashed, and the D: drive filled up (target folders of
30 GB and more).

## Decision
- `[profile.dev] debug = "line-tables-only"` and `[build] jobs = 4` in the repository.
- Work in worktrees on D: only, each with its own target dir, one cargo command at a time per worktree, and
  delete `debug/incremental` and the target dir when a branch is merged.
- Agents run with `CARGO_BUILD_JOBS=3` and debug info off, and use `-p <crate>` while iterating.

## Alternatives
- Shared target dir: lock contention and mixed feature sets.
- Fewer worktrees: slower, not needed once the limits hold.

## Consequences
- Backtraces keep line numbers, not variable inspection.
- The portable build still uses `--release` with `--features directml` (`scripts/package-portable.ps1`).

## Evidence
The crash and the cleanup of about 100 GB of build output in this repository's history.

## Revisit when
A machine with more RAM and disk is the norm for contributors.
