# Parity batch: ui-render-workspace

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 18) with both windows 908 px wide; Fusion's Part Design "Untitled*"
(inches, Body1, four sketches, construction planes), FissionOxide built from `main` at `2d74c694`.
Fusion's workspace is reached with DESIGN > RENDER. See `parity/reports/2026-10-22-workspaces.md`.

## Render toolbar panels and dropdowns (P1)
- key: `ui:render:toolbar-panels`
- Fusion: one tab RENDER with four panels. SETUP: quick icons Appearance, Scene Settings, Lights (sun), Decal, Texture Map Controls, Move/Copy; dropdown Physical Material / Appearance (A) / Scene Settings / Light / Decal / Texture Map Controls / Move/Copy (M). OPTICAL OVERRIDES: quick icon of a glass with 0/1/2; dropdown Dielectric Priority Control. IN-CANVAS RENDER: quick icons In-canvas Render, In-canvas Render Settings, Capture Image; dropdown In-canvas Render / In-canvas Render Settings / Capture Image... . RENDER: teapot icon; dropdown Render. There is no EXIT panel; leaving is by the workspace dropdown.
- FissionOxide: SETUP holds only Appearance and Scene Settings, IN-CANVAS RENDER only In-Canvas Render, RENDER holds Render (a play arrow) and Render Gallery (a folder), and a fourth panel EXIT with a green Finish Render; no OPTICAL OVERRIDES panel, no Lights, Decal, Texture Map Controls, Move/Copy, Physical Material, In-canvas Render Settings or Capture Image.
- Start here: `crates/ui/src/menus.rs:955` (`RENDER_*` lists and the `Group`s at 968-973); `crates/ui/src/render_ui.rs:48` (icons and tips).

## Entering Render opens nothing and leaves In-canvas Render off (P1)
- key: `ui:render:entry-state`
- Fusion: switching to RENDER keeps the ordinary shaded view (sketches, planes and bodies still drawn) on a flat grey background (Scene Settings' Background = Solid Color); no palette opens and In-canvas Render is off until its button is pressed.
- FissionOxide: the Appearance palette opens at the upper left and In-Canvas Render is already on: an empty design shows a huge black ground plane over a dark grey background. Toggling In-Canvas Render off gives the Design workspace's white canvas, not Fusion's grey.
- Start here: `crates/ui/src/render_ui.rs:145` (`in_canvas: true`, `appearance_open: true` defaults) and the canvas background while `app.render.active`.

## Rendering Gallery replaces the timeline (P1)
- key: `ui:render:rendering-gallery`
- Fusion: in RENDER the timeline is hidden; a "RENDERING GALLERY" strip (collapsible header with a - at the left) fills the bottom of the window, showing "Use the Render tool [teapot icon] to save renderings to your gallery." while empty. The browser keeps the design's rows.
- FissionOxide: the design timeline stays at the bottom; the gallery is a separate Render Gallery command.
- Start here: `crates/ui/src/panels.rs:3414` (`timeline`, skip it while `app.render.active`), `crates/ui/src/render_ui.rs` (gallery window).

## Scene Settings is a docked dialog with Settings and Environment Library tabs (P1)
- key: `ui:render:scene-settings-dialog`
- Fusion: a docked right-hand dialog "SCENE SETTINGS" with tabs Settings | Environment Library and a Close button. Settings: Environment (Brightness "1000.000 lx" with a slider, Position (a move icon), Background "Solid Color" dropdown, Color swatch), Ground (Ground Plane checked, Flatten Ground, Reflections), Camera (Camera "Perspective" dropdown, Focal Length "90 mm" with a slider, Exposure "9.5 EV" with a slider, Depth of Field checkbox, Aspect Ratio "Viewport aspect ratio"), then Save as Default, Restore Saved, Restore Defaults. Environment Library: Current Environment (thumbnail "Sharp Highlights"), Library ("Fusion Library" dropdown) with thumbnail rows Cool Light, Grid Light, Rim Highlights, Sharp Highlights, Soft Light, Warm Light, Crossroads (download icon) ..., and "Attach Custom Environment" with a folder icon.
- FissionOxide: a floating window "Scene Settings" at the lower left: Environment (Lighting "Photo Booth" dropdown, Brightness 1.00, Rotation, Background "Environment", Ground plane, Shadows, Reflections 0.000, Ground offset (mm)), Camera (Projection Perspective/Orthographic, Field of view 30, Depth of field, Exposure (stops) 0.0, Tone mapping "Aces", Use Current View, Reset Camera); no tabs, no environment thumbnails, no Close button.
- Start here: `crates/ui/src/render_ui.rs:646` (`egui::Window::new("Scene Settings")`); docked dialogs elsewhere use `crates/ui/src/dialogs.rs`.

## In-canvas Render progress bar and stop button (P1)
- key: `ui:render:in-canvas-progress`
- Fusion: pressing In-canvas Render hides sketches and planes, turns the button into a stop icon (tooltip "In-canvas Render Stop") and shows a bar at the lower right of the canvas: a play/pause triangle, a blue progress line with marks "Excellent", "Final", "Infinite" above it, and "Elapsed time: 2 sec" and "Iteration number: 14" labels below.
- FissionOxide: In-Canvas Render is a toggle with no progress bar, no elapsed time or iteration count, and sketches stay as they are.
- Start here: `crates/ui/src/render_ui.rs:78` (the `InCanvas` toggle) and the canvas overlay drawing in `crates/ui/src/viewport.rs`.

## Lights command and New Light dialog (P2)
- key: `ui:render:lights-dialog`
- Fusion: the sun icon "Lights" (tooltip: adds light to control illumination, shadows and highlights; combine with environment lighting; up to eight lights per design, with a picture) opens a docked "NEW LIGHT" dialog: Light Type "Photometric Light" dropdown, Target "Select", OK greyed, Cancel.
- FissionOxide: no Lights command.
- Start here: `crates/ui/src/menus.rs:955` and `crates/ui/src/render_ui.rs`.

## Render needs a saved design (P2)
- key: `ui:render:render-needs-save`
- Fusion: Render (teapot) on an unsaved design shows a modal "Fusion" box "You need to save your design before using the Render tool." with OK; nothing renders.
- FissionOxide: Render opens its floating "Render" window (Image size HD (1280 x 720), Width x height, Quality (samples) Custom 128, Denoise, Camera Viewport / Stored render camera, Render, Save Image...) on an unsaved design.
- Start here: `crates/ui/src/render_ui.rs:918` (`render_window`); the gallery images belong to the saved document, as in Fusion.
