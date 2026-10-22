# Parity batch: ui-documents-window-tabs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 (prober 20) while probing start-up, document tabs and closing (FissionOxide
`main` at `379c2ae2`). Mostly `crates/ui/src/documents.rs`, which the `ui-documents-tabs-close`
batch also edits (tab label, close prompt): if that batch has not landed, rebase onto it first and
reuse its Save dialog for the window close. See `parity/reports/2026-10-22-startup-preferences-marking.md`.

## Window close asks to save unsaved designs (P1)
- key: `ui:documents:window-close-unsaved`
- Fusion: the window's own X with unsaved tabs opens the same "Save" dialog as a tab close (Name "Untitled" selected, Location "Default Project", Don't Save / Cancel / Save) for the active unsaved design; Cancel keeps Fusion open with every tab as it was.
- FissionOxide: the window's X quits at once with an unsaved sketch in the only tab ("Untitled*" in the title): no prompt, the work is lost.
- Start here: `crates/ui/src/documents.rs:215` `request_close` and the eframe close in `apps/fissionoxide/src/main.rs:180`; intercept the viewport close request (`ctx.input(|i| i.viewport().close_requested())` + `ViewportCommand::CancelClose`) while any slot is dirty, walk the dirty documents through the close prompt, then close.

## Tab overflow: minimum tab width, scrolling and the "v" list (P2)
- key: `ui:documents:tab-overflow-menu`
- Fusion: tabs keep a minimum width (about 150 px in a 908 px window); when they do not fit, the strip scrolls to keep the active tab in view and a "v" chevron appears left of "+". The chevron opens a list of every open document (an orange design icon each; the active one shows a check mark instead), and picking one switches to it and scrolls it into view. Closing the active rightmost tab activates its left neighbour.
- FissionOxide: tabs share the strip with no minimum: seven tabs are 72 px each ("Untitl"), at six tabs the lock glyph overlaps the close X, and there is no chevron or list.
- Start here: `tab_strip` and `document_tab` in `crates/ui/src/documents.rs:251` and `:289`; give tabs a minimum width, a scroll offset that follows `docs.active`, and a chevron popup listing the documents.

## Ctrl+Tab cycles the document tabs (P2)
- key: `ui:documents:ctrl-tab-switch`
- Fusion: Ctrl+Tab activates the next document tab (wrapping from the last to the first) and Ctrl+Shift+Tab the previous one.
- FissionOxide: Ctrl+Tab and Ctrl+Shift+Tab do nothing; the active tab stays.
- Start here: the global key handling in `crates/ui/src/lib.rs` (where Ctrl+N is handled) and `switch_to` in `crates/ui/src/documents.rs:161`.

## Window title is the tab label and the product (P2)
- key: `ui:chrome:window-title-format`
- Fusion: the title bar reads "<the active tab's label> - Autodesk Fusion Personal (Not for Commercial Use)", e.g. "Untitled*(3) - ...", and "Home - ..." on the Home page; no user name.
- FissionOxide: "Untitled (Leckliter) - FissionOxide" / "Untitled* (Leckliter) - FissionOxide": the user name is added and the label differs from the tab's.
- Start here: `window_title` in `crates/ui/src/panels.rs` (called from `App::title`, `crates/ui/src/lib.rs:900`); build it from `documents::tab_label` + " - FissionOxide".

## New design chooser details (P2)
- key: `ui:documents:new-chooser-details`
- Fusion: File > New... and "+" open "What do you want to design?" (about 640 x 413, separators under the title and above the footer, "Learn More" a blue link). Tiles about 104 x 145: a white picture area, a grey label band, a small coloured badge. The right pane puts each label left of its control on one row ("Type" [Standard][Sheet Metal] icons, "Unit System" dropdown). Type tooltips: "Standard" / "Creates a standard container for design elements like sketches and bodies."; "Sheet Metal" / "Creates a component that is subject to sheet metal rules and can be used to create flat patterns." Descriptions: Part "Create sketches and bodies to model a single component to manufacture or use in an assembly."; Assembly "Create or insert parts and other assemblies as external components, then define relationships between them to create an assembly."; Hybrid "Create and model internal components, insert external components, and define relationships between them."; Drawing "Create views and annotations to document the active design for manufacturing." with Type radio Drawing / Template, an Automate checkbox (on, lightning icon) and Design "Untitled"; Electronics Design "Create and manage schematics, 2D PCB, and 3D PCB documents."; Electronics Library "Create and manage components for your electronics designs."
- FissionOxide: about 577 x 248 with 100 x 80 tiles, its own descriptions, labels stacked above controls, Type tooltips "Part" / "Sheet Metal Part", "Learn More" as a button; Drawing and Electronics Library are greyed with a note.
- Start here: the chooser window and `NewKind::description` in `crates/ui/src/documents.rs:380` to `:560`.
