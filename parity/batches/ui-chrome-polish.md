# Parity batch: ui-chrome-polish

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## canvas calibrate (P2)
- key: `ui:insert:canvas-calibrate`
- Fusion: Canvas: place an image on a plane, then right-click Calibrate by picking two points and typing a distance; opacity slider.
- FissionOxide: Canvas inserts an image on a plane, but there's no Calibrate (pick two points, type the real distance, image rescales) and no right-click Edit Canvas with opacity. Nothing in crates/ui mentions calibrate.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/panels.rs`

## help menu (P2)
- key: `ui:chrome:help-menu`
- Fusion: Help (?): Learning and documentation, Quick Setup, keyboard shortcuts, What's new, Diagnostic log.
- FissionOxide: Help (?) shows Check for Updates, a one-line navigation note, an inline shortcut list and the version. Fusion's Help has Learning and Documentation, Quick Setup, a full Keyboard Shortcuts window, What's New, Support and a diagnostic log.
- Start here: `crates/ui/src/panels.rs`

## menus close on escape (P2)
- key: `ui:chrome:menus-close-on-escape`
- Fusion: Toolbar menus (File, Help, panel dropdowns) close on Escape or when another command starts.
- FissionOxide: The Help menu stayed open after Escape and while New Component ran on top of it; the File menu likewise stayed open after Escape. Panel dropdowns (CREATE, CONSTRUCT...) do close on Escape.
- Start here: `crates/ui/src/panels.rs`
