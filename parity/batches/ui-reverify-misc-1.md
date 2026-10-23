# Parity batch: ui-reverify-misc-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by verifier 10 on `main` at `588d30a9`, re-checking the keys the wave-42..45
fixers marked `fixing-merged` (one reopened) and in passing (two new rows; the Patch one was
checked in Fusion the same session). Every row is already in `parity/ui-features.jsonl` (do not
run `ledger_from_batches.py` for this batch); read each row's `history` before starting.

## ui-reverify-misc-1 (P2)

- key: `ui:manufacture:solid-holder-submenu` (P2, reopened)
- Fusion: MILLING > MANAGE > Solid Holder opens a submenu with one entry, "Milling Tool Holder" (holder icon). Resting on it shows the tooltip "Creates a milling tool holder from a solid body." with the paragraph on the revolved cross-section and the Tool Library, the line that the design needs one solid body, and a picture of a holder; the submenu stays open beside it.
- FissionOxide: the entry and its icon are right, but the tooltip is text only (the three paragraphs and "Not in FissionOxide yet") however long the pointer rests, and the Solid Holder submenu closes while the tooltip is drawn over the MANAGE list.
- Start here: `crates/ui/src/tips.rs:2412` (`illustrated`), `crates/ui/src/ribbon_tip.rs:61`, `crates/ui/src/menus.rs:1171`

- key: `ui:surface:patch-sketch-boundary` (P2, new)
- Fusion: SURFACE > Patch takes a closed sketch curve as its boundary: clicking a sketch circle lists "Curve 1" under Boundary Edges, previews the cap and lights OK.
- FissionOxide: clicking a closed sketch circle with PATCH open selects it (status "1 Sketch Circle", the circle dashed) but Boundary Edges stays empty, nothing is previewed and OK stays grey. A loop of body edges works.
- Start here: `crates/ui/src/dialogs/surface.rs:143` (`sync_rails`, where picks are routed), `crates/ui/src/forms.rs:2057` (PATCH's `Input::Edges`)

- key: `ui:chrome:preferences-inert-controls-greyed` (P3, new)
- Fusion: every control on the Manufacture, Network and Preview Features preference pages is live (normal text, dropdowns, check boxes and spinners you can change).
- FissionOxide: those pages show Fusion's rows and defaults, but every control on them is drawn greyed (Manufacture's editors, combos and check boxes; Network's proxy setting, Proxy Port and Server Verification; Preview Features' workspace filter and the three feature boxes). Data Collection's second and third cards are live. Make them live and keep their values in the preferences, as the Drawing page already does.
- Start here: `crates/ui/src/preferences/pages.rs:226`, `crates/ui/src/preferences/pages.rs:412`
