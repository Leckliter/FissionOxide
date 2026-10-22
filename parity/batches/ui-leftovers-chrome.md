# Parity batch: ui-leftovers-chrome

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

These rows were recorded by probers and fixers between 2026-10-21 and 2026-10-22 but never got a
batch (most were found by a fixer while working on a neighbouring item). The coordinator grouped
them on 2026-10-22; the wording below is the ledger row's own. Re-read the row in
`parity/ui-features.jsonl` (its `history`) before starting, and where a row says Fusion's side was
not observed, check it in Fusion's documentation and say so in your ledger note.

## viewports (P2)
- key: `ui:navbar:viewports`
- Fusion: Viewports flyout: single / multiple views.
- FissionOxide: The Viewports flyout has one inert 'Single View' item. Fusion offers Single View and Multiple Views (4 linked viewports).
- Start here: `crates/ui/src/panels.rs`.

## flick (P2)
- key: `ui:marking-menu:flick`
- Fusion: Right-drag in a direction runs that slot without waiting for the menu (marking gesture).
- FissionOxide: Right-drag orbits (viewport.rs:296), so marking-menu flick gestures can't work. Fusion: right-drag toward a slot runs it. Needs a design call: keep right-drag orbit (Fusion uses Shift+middle) or adopt flicks.
- Start here: `crates/ui/src/marking.rs`, `crates/ui/src/viewport.rs`.

## text commands (P2)
- key: `ui:chrome:text-commands`
- Fusion: Text Commands panel at the bottom.
- FissionOxide: No Text Commands panel. Fusion has a bottom panel for Txt/Py/TS commands; FissionOxide's command engine (commands.rs) could back it.
- Start here: `crates/ui/src/panels.rs`.

## status bar units (P2)
- key: `ui:chrome:status-bar-units`
- Fusion: The status-bar selection readout (area, length) uses the design's units.
- FissionOxide: With the design in inches, selecting a profile still shows '1 Profile | Area: 800.00 mm^2' in the status bar. #119 converted dialogs, manipulators and measure but not this readout.
- Start here: `crates/ui/src/panels.rs`.

## dark theme (P2)
- key: `ui:chrome:dark-theme`
- Fusion: Preferences > General > Theme: Dark turns the whole UI dark (application bar, toolbar, browser, panels, dialogs, canvas background).
- FissionOxide: Only egui's own widgets follow Theme: Dark; the painted chrome stays light.
- Start here: `crates/ui/src/theme.rs`, `crates/ui/src/panels.rs`.

## sketch curve readout (P2)
- key: `ui:status:sketch-curve-readout`
- Fusion: Selecting a sketch curve from outside its sketch reads its type and size in the status bar: "1 Sketch Arc | Diameter : 0.50 in", "1 Sketch Line | Length : 1.00 in".
- FissionOxide: The status says "1 Sketch Curve" with no readout (panels::selection_text has no SketchCurve noun per curve kind and no length/diameter). Found while fixing ui:canvas:sketch-curve-priority.
- Start here: `crates/ui/src/panels.rs`.

## 3d print toolbar button (P2)
- key: `ui:toolbar:3d-print-button`
- Fusion: UTILITIES > MAKE > 3D Print (and MESH > EXPORT > 3D Print) opens the same docked "3D PRINT" dialog as File > 3D Print.
- FissionOxide: The toolbar's 3D Print writes an STL straight away (a save dialog) instead of opening the 3D PRINT dialog.
- Start here: `crates/ui/src/panels.rs`.

## Arrow glyph draws a box (P2)
- key: `ui:chrome:arrow-glyph-boxes`
- Fusion: No missing-glyph boxes anywhere in the UI.
- FissionOxide: Found by the ui-utilities-addins-make fixer: the UI font has no glyph for U+2192, so these strings draw a box where the arrow is: the data panel's version line 'Changes v{} → v{}' (crates/ui/src/data_panel.rs:1797) and the Motion Link and cam-follower browser names (crates/ui/src/panels/browser_assembly.rs:28-29). Use '>' or add the glyph to the font fallback.
- Start here: `crates/ui/src/data_panel.rs`, `crates/ui/src/panels/browser_assembly.rs`.
