# Parity batch: ui-open-and-save-bugs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## open forwarded file (P0)
- key: `ui:chrome:open-forwarded-file`
- Fusion: Opening a file while the app is running brings the window forward and shows the model right away.
- FissionOxide: Single-instance forwarding works (file opens in the running window, title changes), but the body is not drawn: the canvas shows only axes and grid until Body1's visibility is toggled. F6 and Home don't help. The CLI renders the same .fox correctly, so the GUI isn't building meshes for a document opened through the forwarded path.
- Start here: `crates/desktop`, `apps/fissionoxide/src/main.rs`, `crates/ui/src/lib.rs`

## no phantom saves (P1)
- key: `ui:chrome:no-phantom-saves`
- Fusion: Fusion only adds a Data panel entry when the user saves; unsaved untitled designs never appear there.
- FissionOxide: The hub has 117 items, almost all empty 'Untitled' designs (0 features, origin 'saved', 193-byte .fox). Overnight they appeared at 02:53, 02:56, 02:57, 03:01 (x4), 03:06, 03:07, 03:08 and 03:09, each logged as 'created Untitled'. Not caused by idling (90 s, none), a click on the canvas, launching a second instance, or fissionoxide-cli info/render. Some UI path is calling hub.save (data_panel.rs:933) on an empty untitled design; it lines up with the window being activated or a file being forwarded. Fix the trigger, and don't hub.save an empty, unnamed design without the user asking.
- Start here: `crates/ui/src/data_panel.rs`, `crates/ui/src/lib.rs`, `crates/engine/src/hub.rs`
