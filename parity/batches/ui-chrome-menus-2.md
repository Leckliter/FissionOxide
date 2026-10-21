# Parity batch: ui-chrome-menus-2

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## file menu items (P2)
- key: `ui:chrome:file-menu-items`
- Fusion: New..., Add To Assembly, Open... Ctrl+O, Open Recovered Documents (0), Upload..., Save Ctrl+S, Save As, Save As Latest, Export..., Create Version, 3D Print, Capture Image..., Share File Link, View Details on Web, View ▸. Cloud-only entries are greyed on an unsaved design.
- FissionOxide: New Design Ctrl+N, Open... Ctrl+O, Open Recent ▸, Show Data Panel, Save, Save As... Ctrl+Shift+S, Save to File..., New Drawing from Design, Insert into Design..., Export ▸, View ▸, Preferences....
- Start here: `crates/ui/src/panels.rs:204` (`file_menu`). Add 3D Print (export STL with a dialog) and Capture Image... (save the canvas as PNG), Open Recovered Documents (greyed), and show the cloud-only ones greyed; Preferences belongs in the profile menu (it is already there).

## notifications panel (P2)
- key: `ui:chrome:notifications-panel`
- Fusion: The bell opens a Notifications panel docked under the icon with one card per notice (title, body, links, age, an x) and a Dismiss all button.
- FissionOxide: The bell has an unread dot but opens nothing unless an update is pending; then it opens the update window.
- Start here: `crates/ui/src/panels.rs:130` and `crates/ui/src/updates.rs:100`. A small panel listing the update notice (and "No notifications") is enough.

## undo step names 2 (P2)
- key: `ui:chrome:undo-step-names-2`
- Fusion: The Undo dropdown lists "Undo Fillet, Undo Extrude, Undo Finish Sketch, Undo 2-Point Rectangle, Undo 2-Point Rectangle, Undo Create Sketch", each with the command's icon.
- FissionOxide: An "Undo" header then "Fillet, Extrude, 2-Point Rectangle, Sketch" with no icons and no Finish Sketch step.
- Start here: `crates/ui/src/panels.rs:161` (`history_menu`). Prefix "Undo "/"Redo ", add the command icon, name the sketch steps "Create Sketch" / "Finish Sketch".

## comments expand (P2)
- key: `ui:chrome:comments-expand`
- Fusion: Clicking the COMMENTS header opens a panel above it ("No comments" with a + to add one); the header collapses it again.
- FissionOxide: Clicking the COMMENTS header or its chevron changes nothing.
- Start here: `crates/ui/src/panels.rs:2056` (`comments`, `float_header(ui, "COMMENTS", &mut open, ...)`): the `open` flag is not kept across frames.

## display settings 2 (P2)
- key: `ui:navbar:display-settings-2`
- Fusion: Display Settings lists Visual Style ▸, Environment ▸, Graphics Preset ▸, Effects ▸, Object Visibility ▸, Camera ▸ (Orthographic / Perspective / Perspective with Ortho Faces), Ground Plane Offset, Enter Full Screen Ctrl+Shift+F.
- FissionOxide: Visual Style ▸, Object Visibility ▸, Effects ▸, then an inline Camera group (Orthographic / Perspective), ViewCube, Full Screen, GPU Renderer. Missing Environment, Graphics Preset, Perspective with Ortho Faces in this menu, Ground Plane Offset and the shortcut hint; ViewCube and GPU Renderer are extras.
- Start here: `crates/ui/src/navbar.rs:156`. Match the order, make Camera a submenu with the third option (it exists in the ViewCube menu), add the shortcut, and move ViewCube/GPU to Preferences > Graphics.

## viewcube size menu (P2)
- key: `ui:viewcube:menu-size`
- Fusion: The ViewCube menu ends with "ViewCube Size ▸" (Automatic, Tiny, Small, Normal, Large).
- FissionOxide: The menu matches Fusion up to Reset Front but has no ViewCube Size submenu; the cube is one fixed, larger size.
- Start here: `crates/ui/src/viewcube.rs:571` and the cube size constant in the same file; store the choice in Preferences.

## timeline hover highlights feature (P2)
- key: `ui:timeline:hover-highlights-feature`
- Fusion: Hovering a timeline icon highlights only what that feature made: the fillet's faces for Fillet1, the extruded body for Extrude1, the sketch curves for Sketch1.
- FissionOxide: Hovering Fillet1 tints the whole body.
- Start here: `crates/ui/src/panels.rs:2578` (`timeline_items`, the hover branch). The engine knows each feature's faces (`feature.faces` or the equivalent in `crates/model`); pass those to the highlight instead of the body.

## default appearance (P2)
- key: `ui:canvas:default-appearance`
- Fusion: New bodies are a warm light grey (Fusion's default "Steel" look) with soft shading.
- FissionOxide: Bodies are a cooler mid grey (186, 190, 196) with darker sides.
- Start here: `crates/engine/src/scene.rs:14` (`BODY_COLOR`) and the lighting in `crates/render/src/lib.rs`. Sample Fusion's default on screen (about 205, 200, 190 on the top face).
