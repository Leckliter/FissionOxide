# Parity batch: ui-reverify-chrome-3

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by the incremental re-verification pass (verifier 7) on `main` at `905a4701`, with
Fusion and FissionOxide side by side at the same window width (about 920 screenshot px, 1215 px; Fusion
will not go narrower). Both rows are reopened from `ui-reverify-toolbar-1`. Every row is already in
`parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this batch); read each row's
`history` before starting.

## ui-reverify-chrome-3 (P2)

- key: `ui:toolbar:workspace-menu-style`
- Fusion: in the DESIGN switcher, GENERATIVE DESIGN and SIMULATION are greyed with a blue round badge holding a white up arrow; both badges sit right-aligned in one column at the list's right edge (same x), not after the names.
- FissionOxide: the badge is a plain blue dot drawn right after each name, so the two dots sit at different x.
- Start here: `crates/ui/src/panels.rs:1057` (the badge room and radius), `crates/ui/src/panels.rs:1073`

- key: `ui:documents:tab-overflow-menu`
- Fusion: with four design tabs open in a 1215 px window all four fit (each about 142 screenshot px, about 187 px) beside the quick-access buttons, with '+' and no chevron.
- FissionOxide: at the same width the tabs keep about 148 screenshot px (TAB_MIN_W 196), so the strip scrolls, the first tab shows as 'ntitled' and the 'v' appears. The 'v' list itself matches. Fusion's true minimum was not measured: let tabs shrink to at least 187 px before overflowing, and check that four tabs fit at 1215 px.
- Start here: `crates/ui/src/documents.rs:421` (`TAB_MIN_W`), `crates/ui/src/documents.rs:469`
