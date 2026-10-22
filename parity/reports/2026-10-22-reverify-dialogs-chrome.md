# 2026-10-22: re-verification sweep, dialogs and chrome (verifier 2)

Re-checked every `fixing-merged` key in the dialogs and chrome areas in the rebuilt app. `main` did
not build on Windows at the start: `crates/kernel/cpp/surface3.cpp` named a variable `far`, a
Windows header macro (from the cloud fixer's `45b37c4b`). Renamed in `b399613d`, then rebuilt.

Fusion was looked at only where a row's text was unclear or never recorded: Replace with Primitives
(its rows were never probed), Chamfer (does Fusion have a Preview checkbox? no) and the COMMENTS
header (does not expand on an unsaved tab, so `ui:chrome:comments-expand` stays unchecked).

## Counts

| Area | fixing-merged at start | verified | reopened | left |
|---|---|---|---|---|
| dialogs | 56 | 40 | 16 (+1 new row) | 0 |
| chrome | 31 (+4 merged meanwhile) | 21 | 8 | 2 (+4) |

## Batches (wave 38, all `ready`)

| Batch | Items | Highest |
|---|---|---|
| `ui-reverify-dialogs-1` | Thicken does not open, Hole, Chamfer, Move/Copy, Extrude from a face, Draft rows, Preview checkbox (new), Boundary Fill layout | P1 |
| `ui-reverify-dialogs-2` | Edit Mirror old layout, body chips light one face, mirror originals, Edit Combine, Edit Draft, Edit Hole | P1 |
| `ui-reverify-dialogs-3` | Split Face types, Remove Features types, Replace with Primitives rows | P2 |
| `ui-reverify-chrome-1` | dark theme, project opens on one click, Text Commands, View extras, list view rows, Open tree, IPT/FBX export, glyph boxes | P1 |

## What matched

Create Sketch, Extrude (all profile cases), Revolve with auto Cut, Sweep, Loft, Emboss, Thread,
Mirror, the pattern Type switch, Base Feature, Joint Origin, Compute All, Parameters, Scale, Offset
Face, Combine (rows, whole-body picks, preview browser, Cancel, disjoint names), Replace Face, Split
Body, Silhouette Split, Align, Remove, the material palettes and Material Browser, every Edit Feature
of Fillet, Chamfer, Extrude, Move and Press Pull, the fillet error toast, Press Pull on a primitive.
Chrome: status readouts, File menu, Save / Image Options / Upload windows, Escape rules, Undo list,
Notifications, Help, Data Panel header, project view, file menu and tooltip.

## Not covered

Two faces at an angle in the status bar (modifier clicks do not reach FissionOxide from the
automation tool); COMMENTS on a saved design; the Thread face preview; Rectangular Pattern (its own
row `ui:dialogs:pattern-fields`, still `mismatch`).
