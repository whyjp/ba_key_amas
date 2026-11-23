# Architecture decision records

One file per decision: context, decision, alternatives that were rejected, consequences, evidence, and when to
revisit it. Status is Accepted, Superseded, or Pending (implemented on a branch, not merged).

| # | Decision | Status |
|---|---|---|
| [0001](0001-addon-shell.md) | A resident add-on, passive first, scan by hand | Accepted |
| [0002](0002-ocr-stack.md) | PP-OCR on ONNX Runtime in process, DirectML optional, closed-set reading for names | Accepted |
| [0003](0003-native-desktop-control.md) | Desktop control in process through Win32, not the HCU helper | Accepted |
| [0004](0004-input-policy.md) | Input only for the scan, guarded at one boundary, observe never intercept | Accepted |
| [0005](0005-static-db-dynamic-screen.md) | Facts come from the static DB, the screen supplies keys | Accepted |
| [0006](0006-enemy-weighting.md) | Enemy armor and attack mix weighted by HP | Pending (branch `wt-weights`) |
| [0007](0007-overlay-windows.md) | Layered click-through windows, change cache, bookmark card | Accepted |
| [0008](0008-raid-modelling.md) | Raids as difficulty rows with seasons, recognised by screen | Accepted |
| [0009](0009-build-limits.md) | Build settings that keep RAM and disk bounded | Accepted |

Template: Context, Decision, Alternatives, Consequences, Evidence, Revisit when.
