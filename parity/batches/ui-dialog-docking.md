# Parity batch: ui-dialog-docking

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## stale position after resize (P1)
- key: `ui:dialogs:stale-position-after-resize`
- Fusion: Every command dialog (and Edit Feature) opens docked at the viewport's top-right corner, whatever the window size.
- FissionOxide: The `feature-dialog` Area keeps its last position in egui memory. After maximising the window, CREATE SKETCH, EXTRUDE and FILLET all opened at window position (472, 265), in the middle of the canvas over the model, and a click meant for the profile hit the Direction dropdown. A dragged dialog also stays where it was dragged for every later command.
- Start here: `crates/ui/src/dialogs.rs:1967` (`default_pos(app.canvas.right_top() + vec2(-34, 190))`). Reset the Area position when a dialog opens (`Area::current_pos` or clear the memory for that id) and re-dock when `app.canvas` changes size.

## create sketch dialog (P2)
- key: `ui:dialogs:create-sketch-dialog`
- Fusion: Create Sketch has no dialog: the ribbon button stays pressed, the planes show, and the cursor tooltip "Select a plane or planar face" does the asking. Escape or the button cancels.
- FissionOxide: A CREATE SKETCH dialog opens with the prompt text and a Cancel button.
- Start here: `crates/ui/src/dialogs.rs:2123` (`Kind::PickPlane` body), `crates/ui/src/lib.rs:1149`. Keep the PickPlane state but draw no dialog for it.

## extrude auto profile (P2)
- key: `ui:dialogs:extrude-auto-profile`
- Fusion: E opens Extrude with Profiles = Select and no preview. Distance and Taper Angle rows appear only after a profile is picked, and OK is greyed until then.
- FissionOxide: With Sketch1 still selected in the browser after Finish Sketch, E opened with "1 selected" and a 10 mm preview body without any pick, and all rows shown from the start.
- Start here: `crates/ui/src/dialogs.rs:658-673` (`Dialog::open` pre-fills `profiles` from the selection). Only pre-fill from an explicitly picked profile, hide the value rows until there is one, grey OK. See also `ui:browser:sketches-folder-after-finish` in ui-browser-rows, which removes the lingering selection.

## type icon toggles (P2)
- key: `ui:dialogs:type-icon-toggles`
- Fusion: Extrude's Type row is a pair of icon toggles (Solid / Thin). Every dropdown value shows an icon before its text (Profile Plane, One Side, Distance, New Body).
- FissionOxide: Type is text radio buttons; dropdown values have no icons.
- Start here: `crates/ui/src/dialogs.rs:1720` (Extrude rows) and the shared dropdown helper. Icons already exist in `crates/ui/src/icons.rs`.

## extrude flip row (P2)
- key: `ui:dialogs:extrude-flip-row`
- Fusion: No Flip row. Direction flips by dragging the arrow the other way or typing a negative distance.
- FissionOxide: Extrude, Revolve and others show a "Flip" checkbox row.
- Start here: `crates/ui/src/dialogs.rs:2601, 2667, 2675, 2913` (`Self::check(ui, "Flip", ...)`). Keep the flag but drive it from the arrow and the sign of the value.

## feature icon during command (P2)
- key: `ui:timeline:feature-icon-during-command`
- Fusion: The feature's timeline icon appears only after OK.
- FissionOxide: The Extrude/Fillet icon is added to the timeline while the dialog is still open (the preview applies the feature to the session).
- Start here: `crates/ui/src/panels.rs:2578` (`timeline_items`): skip the feature a dialog is previewing, or mark preview features in the session so the timeline hides them. The dialog's preview/commit path is around `crates/ui/src/dialogs.rs:2038`.
