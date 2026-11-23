# 0003 Desktop control in process through Win32, not the HCU helper

Status: Accepted. Details: [hcu-native.md](../hcu-native.md).

## Context
The first Rust version drove the game through Microsoft's Hybrid Computer Use helper (`computer-use.exe`
plus a .NET provider). Every call started a process: about 1.35 s per input and 0.4 to 1.5 s per capture, a
console window flash in a windowless exe, and two extra executables to ship (about 255 MB with models).

## Decision
`hcu` implements the few operations the project uses (capture, zoom, click, scroll, window list, activate)
directly on Win32 and answers the same request protocol through the `Runner` trait, so callers did not
change. Coordinates and zoom geometry follow the MIT reference. The helper stays selectable with
`BAOCR_HCU=process` until a live scan parity run removes it.

## Alternatives
- A Rust client for the HCU daemon: still ships the Go core and the .NET provider, so the single-exe goal
  fails, and the daemon targets general agents (UIA, semantic actions) while the game has no UIA tree.
- Embedding the helper with `include_bytes!`: one file, same process cost.
- Go as a C shared library or .NET NativeAOT: two runtimes inside the process for a few Win32 calls.
- A C++ library: no C++ source exists upstream, and Rust reaches Win32 equally well.

## Consequences
- Capture about 0.1 s measured, against 0.4 to 1.5 s with the helper.
- We own the guards the helper had (layout staleness, partial input): ADR 0004.
- Only the used subset is ported; AX, text input, key chords, the daemon and MCP are deliberately absent.

## Evidence
`cli/examples/backend_diff.rs`: frame size and detected region identical, static area mean difference
1.6 of 765. `docs/hcu-native.md` records the upstream commit used as reference.

## Revisit when
The scan needs key or text input, or the game starts rejecting `SendInput`.
