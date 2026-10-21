# Parity batch: ui-preferences-full

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## preferences layout (P1)
- key: `ui:chrome:preferences-layout`
- Fusion: Preferences is a centred modal (about 750 x 450) with a tree on the left (General ▸ API, Design ▸, Manufacture ▸, Electronics ▸, Render, Drawing, Simulation and Generative Design; Tokens; Material; Graphics ▸; Network; Data Collection and Use; Unit and Value Display ▸; Default Units ▸; Preview Features; Compatibility & Troubleshooting), a page title bar ("Preferences controlling general UI behavior"), and Restore Defaults, an info icon, Apply, OK and Cancel at the bottom. Changes apply on OK/Apply.
- FissionOxide: A full-height window docked at the left of the canvas (about 425 px wide) with five page tabs (General, Navigation, Design, Sketch, Graphics) and no OK, Cancel, Apply or Restore Defaults; changes apply immediately.
- Start here: `crates/ui/src/preferences.rs:390` (`window`). Centre it, size it like Fusion's, use a tree with Fusion's section names (empty sections can say "No settings yet"), and add the button row.

## preferences general settings (P2)
- key: `ui:chrome:preferences-general-settings`
- Fusion: General has User language, Theme, Graphics driver, Start-up experience, Offline cache period, Purpose for using Fusion, Default design type, "New document tab (+) creates default design", "Create new shortcuts when updating", Automatic save on close, Automatic recovery backup interval, Default modeling orientation, Show tooltips, Show command prompt, Show default measure, Show in-command errors and warnings, Mouse setup (pan, zoom, orbit), Default Orbit type, Reverse zoom direction, Enable camera pivot, ViewCube size, Use gesture-based view navigation, Use Windows touchpad for gestures, Spacebar repeats last command. General > Design has Default units and the capture-history switch.
- FissionOxide: General has Default units, Default modeling orientation and Spacebar repeats; Navigation has the preset, Reverse zoom and Zoom toward cursor; Design has Capture design history; Sketch has Auto look at sketch, Show dimensions, Show constraints, Snap to grid; Graphics has GPU renderer.
- Start here: `crates/ui/src/preferences.rs`. Add Theme (light/dark/match device), Show tooltips, Show command prompt (the new cursor prompts), Default Orbit type (Constrained/Free, already a nav-bar setting), Enable camera pivot, ViewCube size (Automatic/Tiny/Small/Normal/Large), Automatic recovery interval; move Default units under General > Design to match Fusion's tree.
