# Parity batch: ui-cursor-prompts

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## cursor prompts (P1)
- key: `ui:canvas:cursor-prompts`
- Fusion: A small tooltip follows the cursor with the current step of the command: "Place first corner", "Specify size of rectangle" (Rectangle); "Select sketch profiles or planar faces to extrude", "Specify distance and taper angle", "Hold Ctrl to modify selection" (Extrude); "Select edges, faces, or features to fillet", "Specify fillet radius" (Fillet). It sits just below-right of the pointer and hides while the pointer is over a dialog.
- FissionOxide: Only Create Sketch has a cursor prompt (`PICK_PLANE_HINT`). Sketch tools and features use the lower-left status hint ("Rectangle: click two corners") and no cursor prompt.
- Start here: `crates/ui/src/dialogs.rs:25` (`PICK_PLANE_HINT` and where it is shown with a pointer tooltip), `crates/ui/src/viewport.rs:2499` (the per-tool hint strings). Add a `prompt()` per dialog kind and selection state and per sketch tool step, and draw it at the pointer with `egui::show_tooltip_at_pointer`. Keep the status hint too.

## status dev messages (P2)
- key: `ui:chrome:status-dev-messages`
- Fusion: The status area holds only the selection readout ("Face", "1 Profile | Area: 2.00 in^2"); nothing is written after a command completes.
- FissionOxide: The lower-left shows "extrude done" / "fillet done" after each command and keeps it until the next command.
- Start here: `crates/ui/src/dialogs.rs:2038` (`app.status = Some((format!("{} done", ...)))`). Drop the message, or keep it only for the log.

## sketch hint (P2)
- key: `ui:chrome:sketch-hint`
- Fusion: No hint text in the canvas corner while sketching.
- FissionOxide: The lower-left reads "Sketching — draw with the Create tools; Finish Sketch when done" for the whole sketch session.
- Start here: `crates/ui/src/lib.rs:1536` (`SKETCHING`) and `left_sketch` at line 1213. Remove it once cursor prompts exist, or show it only for the first few seconds.
