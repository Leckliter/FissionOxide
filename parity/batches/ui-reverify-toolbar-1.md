# Parity batch: ui-reverify-toolbar-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 5) re-checking the toolbar, workspaces, menus, documents and shortcuts keys in the rebuilt app (`main` at `c6232d9f`), with Fusion open beside it (SOLID ribbon at 920 px, workspace list, CONSTRUCT list, document tabs). All six are reopened keys.
Every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## responsive collapse (P2)
- key: `ui:toolbar:responsive-collapse`
- Fusion: At a 907 px window: CREATE 5 icons ending with New Component (purple), MODIFY 4 (Press Pull, Fillet, Shell, Combine), every panel through SELECT visible.
- FissionOxide: At 907 px SOLID CREATE shows Create Sketch, Extrude, Revolve, Hole, Rectangular Pattern; no New Component (purple). Fusion at 920 px shows Create Sketch, Revolve, Hole, Rectangular Pattern, New Component (Extrude was unpinned there by an earlier prober), so its last quick slot is New Component. MODIFY 4 and every panel through SELECT match.
- Start here: `crates/ui/src/menus.rs:729`

## workspace menu style (P2)
- key: `ui:toolbar:workspace-menu-style`
- Fusion: The DESIGN switcher lists DESIGN, GENERATIVE DESIGN (lock icon), RENDER, ANIMATION, SIMULATION (lock icon), MANUFACTURE, DRAWING ▸, ELECTRONICS ▸ in upper case, with a "Change Workspace" tooltip.
- FissionOxide: List and tooltip match, but Fusion draws GENERATIVE DESIGN and SIMULATION in grey with a blue round subscription badge after the label; FissionOxide draws them black with a black padlock (seen in both 2026-10-22).
- Start here: `crates/ui/src/panels.rs:1061`

## Workspace menu row look (P2)
- key: `ui:workspaces:menu-row-style`
- Fusion: rows about 26 px tall (screenshot px) in a semi-bold upper-case face, the list about 110 px wide hugging the labels, with the ▸ arrows for DRAWING and ELECTRONICS close to the text; the current workspace has no fill.
- FissionOxide: FissionOxide's rows are about 24 px tall in a regular-weight face and the list is about 133 px wide (screenshot px); Fusion's rows are about 26 px in a bold upper-case face and the list about 116 px, with the arrows for DRAWING / ELECTRONICS at its right edge.
- Start here: `crates/ui/src/panels.rs:1061`

## Construct menu icons (P2)
- key: `ui:menus:construct-icons`
- Fusion: Every CONSTRUCT entry has its own icon (Offset Plane two green-orange planes, Plane at Angle, Tangent Plane, Midplane and so on each drawn differently; axes in green with a cylinder, face, planes, points or edge; points in orange with a vertex, edges, planes, circle, edge and plane or path).
- FissionOxide: Every entry has its own icon, but all are amber; Fusion's (seen 2026-10-22) use green and orange planes, green axes and orange-green points.
- Start here: `crates/ui/src/icons.rs:1041`

## Tab overflow: minimum tab width, scrolling and the "v" list (P2)
- key: `ui:documents:tab-overflow-menu`
- Fusion: tabs keep a minimum width (about 150 px in a 908 px window); when they do not fit, the strip scrolls to keep the active tab in view and a "v" chevron appears left of "+". The chevron opens a list of every open document (an orange design icon each; the active one shows a check mark instead), and picking one switches to it and scrolls it into view. Closing the active rightmost tab activates its left neighbour.
- FissionOxide: Chevron, list with check mark, pick-to-switch and close-activates-left-neighbour work, but tabs shrink to about 109 screenshot px (TAB_MIN_W 150 is in screen px); Fusion's tabs stay about 147 screenshot px (about 194 screen px) in the same window, so four tabs already scroll in Fusion. The list's design icons are grey where Fusion's are orange.
- Start here: `crates/ui/src/documents.rs:401`

## Sketch keys work outside a sketch (P1)
- key: `ui:shortcuts:sketch-keys-from-3d`
- Fusion: in 3D with nothing selected, the sketch keys start their sketch tool and ask for a plane: C (Center Diameter Circle), L (Line), R (2-Point Rectangle), T (Trim) and P (Project) show the cursor prompt "Select a plane or planar face"; O opens the OFFSET dialog (Curves, Chain Selection, Direction, Flip) with the same prompt. Picking a plane creates the sketch with that tool running.
- FissionOxide: C and L give 'Select a plane or planar face' and picking a plane starts the tool; O gives the same prompt but opens no OFFSET dialog (Fusion opens OFFSET: Curves, Chain Selection, Direction, Flip).
- Start here: `crates/ui/src/shortcuts.rs`
