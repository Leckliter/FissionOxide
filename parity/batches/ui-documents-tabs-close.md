# Parity batch: ui-documents-tabs-close

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 (prober 19) while checking Ctrl+N and closing tabs (FissionOxide `main` at
`c3d74430`). Ctrl+N and "+" now open a second tab in FissionOxide as in Fusion; these are what
differs around them. All in `crates/ui/src/documents.rs`. See
`parity/reports/2026-10-22-keyboard-shortcuts.md`.

## Tab label shows the unsaved star (P2)
- key: `ui:documents:tab-dirty-star`
- Fusion: a never-saved design with changes is labelled "Untitled*" on its tab (with the lock icon); an untouched one "Untitled".
- FissionOxide: the tab reads "Untitled" with the lock even after a sketch and an extrude; only the window title shows "Untitled*".
- Start here: `tab_label` in `crates/ui/src/documents.rs:82` returns `doc_name(s)` without the star while `unsaved(s)`; add "*" when `s.dirty`.

## New tabs are numbered (P2)
- key: `ui:documents:new-tab-numbering`
- Fusion: Ctrl+N opens a Part Design tab named "Untitled(4)" when "Untitled", "Untitled(1)" to "Untitled(3)" are open; the tab strip scrolls and shows a "v" overflow button when the tabs do not fit.
- FissionOxide: Ctrl+N's new tab is also "Untitled", so two tabs read the same.
- Start here: `doc_name` / `new_document` in `crates/ui/src/documents.rs`; number untitled documents "(n)" as Fusion does.

## Closing an unsaved design opens the Save dialog (P2)
- key: `ui:documents:close-unsaved-save-dialog`
- Fusion: clicking x on a never-saved tab with changes activates that tab, greys the window and opens the "Save" dialog (Name "Untitled" selected, Location "Default Project" with a dropdown, buttons Don't Save, Cancel, Save in blue); Cancel keeps the tab.
- FissionOxide: opens a small "Close" window, "Save changes to Untitled?" / "Your changes will be lost if you don't save them." with Don't Save, Cancel, Save.
- Start here: the close prompt in `crates/ui/src/documents.rs:591`; for a never-saved design open the Save dialog (`ui:chrome:save-dialog-layout`) with Don't Save added.
