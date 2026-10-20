# Parity batch: ui-preferences-undo

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## preferences (P1)
- key: `ui:chrome:preferences`
- Fusion: Preferences dialog: General (language, units, default orientation), Design, Sketch, Graphics, Navigation (pan/zoom/orbit button presets like Fusion/SolidWorks/Inventor).
- FissionOxide: No Preferences dialog. Fusion's covers default units, default modeling orientation (Y-up / Z-up, see ui:canvas:up-axis), navigation presets (Fusion / SolidWorks / Inventor / Alias mouse mappings), graphics and sketch options. Settings now live only in settings.ini and scattered toggles.
- Start here: `crates/ui/src/config_ui.rs`, `crates/ui/src/panels.rs`

## undo history (P2)
- key: `ui:chrome:undo-history`
- Fusion: Undo/Redo dropdowns on the quick-access toolbar list the operations to step back to.
- FissionOxide: The carets beside Undo and Redo open nothing; there's no undo history list. Fusion's dropdowns list the operations to jump back to.
- Start here: `crates/ui/src/panels.rs`
