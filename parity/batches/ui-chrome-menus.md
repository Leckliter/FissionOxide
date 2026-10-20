# Parity batch: ui-chrome-menus

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## file menu (P2)
- key: `ui:chrome:file-menu`
- Fusion: File menu: New... (picks the new document type), Add To Assembly, Open... (Ctrl+O), Open Recovered Documents (n), Upload... | Save (Ctrl+S), Save As, Save As Latest, Export... | Create Version | 3D Print, Capture Image..., Share File Link | View Details on Web, View > (toolbar and panel visibility).
- FissionOxide: FissionOxide's File menu has New Design, Open, Open Recent, Show/Hide Data Panel, Save, Save As, Save to File, New Drawing from Design, Insert into Design and Export. Missing from Fusion's: Add To Assembly, Open Recovered Documents (auto-recovery), Upload, Save As Latest, Create Version, 3D Print (it now sits only under UTILITIES/MESH), Capture Image (save the viewport as PNG at a chosen size), and the View submenu (show/hide Browser, Comments, Timeline, Text Commands, ViewCube, Navigation Bar, Data Panel). Share File Link and View Details on Web are cloud-only and can stay out.
- Start here: `crates/ui/src/lib.rs`

## settings menu (P2)
- key: `ui:timeline:settings-menu`
- Fusion: The gear at the timeline's right end opens: Change to Direct Modeling (stops capturing history, after a confirmation), Component Color Swatch (a colour strip per component under the icons), Hide all inactive features (checked by default).
- FissionOxide: FissionOxide's timeline gear (panels.rs ~2547) offers 'Roll to end' and 'Roll to beginning', which aren't in Fusion's menu. Fusion's has Change to Direct Modeling (the same switch as Capture Design History, with a confirmation), Component Color Swatch (a colour strip per component under the icons) and Hide all inactive features (checked by default).
- Start here: `crates/ui/src/panels.rs`

## face (P2)
- key: `ui:context-menus:face`
- Fusion: Right-click a face: the ring has Repeat <last>, Delete, Press Pull, Undo, Redo, Move/Copy, Hole and Sketch. The list has OK, Cancel, Create Selection Set, Create Sketch, Extrude (E), Offset Plane, Shell | Edit Feature, Edit Profile Sketch | Appearance (A), Texture Map Controls, Properties | Show/Hide (V) | Selectable/Unselectable | Find in Browser, Find in Window. No navigation items (Pan/Zoom/Orbit/Fit/Home), no Remove/Show All/Measure/Workspace, and no recent commands on a face.
- FissionOxide: FissionOxide's face list shows the wrong shortcut keys: Extrude 'Q' (should be E; Q is Press Pull) and Properties 'I' (Fusion shows none; I is Measure). It's missing Create Selection Set, Texture Map Controls and Selectable/Unselectable. It adds Look At, the navigation items (Pan, Zoom, Constrained Orbit, Fit F6, Home View), Remove, Show All, Hide All Bodies, Measure, Workspace and the recent-command list, which Fusion keeps for the empty-canvas menu, not a face.
- Start here: `crates/ui/src/panels.rs`, `crates/ui/src/marking.rs`
