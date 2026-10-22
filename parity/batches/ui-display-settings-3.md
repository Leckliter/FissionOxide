# Parity batch: ui-display-settings-3

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) in the navigation bar's Display Settings menu of each app;
FissionOxide built from `main` at `42dec181`. The menu's eight entries and the Camera submenu
already match (`ui:navbar:display-settings-2` verified); the submenus below do not. Everything is
in `crates/ui/src/navbar.rs` (`Environment` at line 125, `GraphicsPreset` at 194, Effects at 285,
Object Visibility at 288, Ground Plane Offset at 310 and 335).

## visual style list (P2)
- key: `ui:navbar:visual-style-list`
- Fusion: Visual Style lists six radio rows with shortcut hints: Shaded Ctrl+4 (default), Shaded with Hidden Edges Ctrl+5, Shaded with Visible Edges Only Ctrl+6, Wireframe Ctrl+7, Wireframe with Hidden Edges Ctrl+8, Wireframe with Visible Edges Only Ctrl+9; the shortcuts work from the canvas (Ctrl+7 drew the body as a wireframe, Ctrl+5 shaded it with hidden edges, Ctrl+4 restored it).
- FissionOxide: Eight rows in another order with no hints: Shaded, Shaded with Visible Edges Only (the default), Shaded with Hidden Edges, Wireframe, Wireframe with Hidden Edges, Wireframe with Visible Edges Only, Hidden Line, Visible Lines Only; Ctrl+7 and Ctrl+4 from the canvas change nothing.
- Start here: `crates/ui/src/navbar.rs` (the Visual Style submenu: Fusion's six rows in Fusion's order, Shaded as the default, Ctrl+4..9 hints and key bindings; drop or hide Hidden Line and Visible Lines Only).

## environment list (P2)
- key: `ui:navbar:environment-list`
- Fusion: Environment lists a radio "Theme (default)", then the header "Theme Environments" with Photo Booth, River Rubicon, Dark Sky, then the header "Legacy Environments" with Infinity Pool, Grey Room, Tranquility Blue.
- FissionOxide: A flat list of about sixteen radio rows (Photobooth, Sharp Highlights, ..., Soft Light, Dry Lake Bed, Dim Field, Field, Grey Room, Infinity Pool, Plaza, Rim Highlights, Snow Field, Tranquility Blue, Warm Light, Cool Light) with no Theme row and no headers.
- Start here: `crates/ui/src/navbar.rs:125` (`Environment` variants and labels) and its submenu: Theme (default) first, two grey headers, Fusion's six names (Photo Booth with a space).

## effects and graphics preset (P2)
- key: `ui:navbar:effects-list`
- Fusion: Graphics Preset lists Performance, Quality, Custom (Custom checked here); Effects starts with a greyed checked row "Graphics Preset: Custom", a separator, then checkboxes Environment Dome (off), Ground Plane (on), Ground Shadow (on), Ground Reflection (off), Object Shadow (off), Ambient Occlusion (on), Anti-Aliasing (on).
- FissionOxide: Graphics Preset defaults to Performance; Effects lists only Ground Shadow, Reflections and Ground Plane, all unchecked.
- Start here: `crates/ui/src/navbar.rs:285` (the Effects checkboxes) and `crates/ui/src/navbar.rs:194` (`GraphicsPreset`): add the preset row, Environment Dome, Ground Reflection, Object Shadow, Ambient Occlusion, Anti-Aliasing with Fusion's defaults.

## object visibility list (P2)
- key: `ui:navbar:object-visibility-list`
- Fusion: Object Visibility lists checkboxes All Work Features, a separator, Origin Planes, Origin Plane Names, Origin Axes, Origin Points, Construction Planes, Construction Plane Names, Construction Axes, Construction Points, Sketches, User Coordinate Systems, Joint Origins, Joint Origin Axes, Joints (all on).
- FissionOxide: Five checked rows: Origins, Sketches, Construction Geometry, Joints, Bodies.
- Start here: `crates/ui/src/navbar.rs:288` (the Object Visibility submenu and the `ObjectVisibility` flags at line 11).

## ground plane offset dialog (P2)
- key: `ui:navbar:ground-plane-offset-dialog`
- Fusion: Ground Plane Offset opens a docked dialog "GROUND PLANE OFFSET" with one row, "Adaptive" (checked), and OK / Cancel; Escape closes it.
- FissionOxide: A large floating window "Ground Plane Offset" with an X, an "Offset 0.0 mm" field, the hint "How far below the model the ground plane sits.", OK / Cancel and a big empty area.
- Start here: `crates/ui/src/navbar.rs:335` (the Ground Plane Offset window): dock it, add the Adaptive checkbox (the offset field only when Adaptive is off), size it to its rows.
