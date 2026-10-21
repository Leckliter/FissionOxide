# Parity batch: ui-browser-rows

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## root eye and origin eye (P2)
- key: `ui:browser:root-eye-and-origin-eye`
- Fusion: The root component row and the Origin folder each have an eye toggle; Origin starts hidden (grey eye). Named Views and Document Settings have none.
- FissionOxide: No eye on the root row or the Origin folder; only Bodies, Sketches and their children have one.
- Start here: `crates/ui/src/panels.rs:995` (`tree_row`, the `eye: Option<bool>` argument) and `browser_tree` at 1095. Origin visibility already exists (Object Visibility > Origins).

## document settings expanded (P2)
- key: `ui:browser:document-settings-expanded`
- Fusion: Document Settings starts expanded, showing "Units: in, ouncemass" and "Part Design" rows.
- FissionOxide: Document Settings starts collapsed and shows only "Units: mm" when opened.
- Start here: `crates/ui/src/panels.rs:1129` (the Document Settings row, `Some(false)` expander default). Expand by default and add the design-type row.

## root unsaved label (P2)
- key: `ui:browser:root-unsaved-label`
- Fusion: The root row reads "(Unsaved)" until the design is saved; the document tab reads "Untitled" with a lock icon; the window title "Untitled* (user) - Autodesk Fusion".
- FissionOxide: The root row reads "Untitled v1"; the tab "Untitled v1*"; the title "Untitled • — FissionOxide".
- Start here: `crates/ui/src/lib.rs:589` (`title`) and the root row in `browser_tree`.

## sketches folder after finish (P2)
- key: `ui:browser:sketches-folder-after-finish`
- Fusion: Finish Sketch leaves the Sketches folder collapsed and nothing selected.
- FissionOxide: Sketches is expanded and Sketch1 stays highlighted after Finish Sketch, which is why E then auto-fills the Extrude profile.
- Start here: `crates/ui/src/lib.rs:1203` (`finish_sketch` clears `selection` but the browser keeps the sketch row highlighted and expanded). Collapse the folder and clear the row highlight.

## folder context menus (P2)
- key: `ui:browser:folder-context-menus`
- Fusion: Right-click menus on the root (Add To Assembly, Create Drawing, Create Selection Set, Configure, Physical Material, Appearance A, Texture Map Controls, Properties, Export..., Save Copy As, Save As Mesh, History, Display Detail Control, Show/Hide V, Show All Components, Show All Bodies, Opacity Control, Find in Window, Change to Direct Modeling), on the Bodies folder (New Group, Create Selection Set, Show/Hide V, Show All, Selectable/Unselectable), on Origin (Create Selection Set, Show/Hide V, Show All, Show/Hide Planes, Show/Hide Axes, Find in Window) and on the Units row (Change Active Units). Hovering any row names it in the status bar.
- FissionOxide: Right-click on the root row, the Bodies folder, the Origin folder and the Units row opens nothing. Body1, Sketch1 and Named Views have menus.
- Start here: `crates/ui/src/panels.rs:1095` (`browser_tree`) beside the existing body and sketch menus. Grey the cloud-only entries.
