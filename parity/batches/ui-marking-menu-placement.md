# Parity batch: ui-marking-menu-placement

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## ring at cursor (P1)
- key: `ui:marking-menu:ring-at-cursor`
- Fusion: The ring is always centred on the right-click. When the list does not fit below the ring, it flips to above the ring (a right-click at y = 600 of a 790 px window put the list above). Near the right edge the list shifts left; the ring never moves.
- FissionOxide: The whole menu is shifted up so the list fits below. A right-click at (900, 600) put the ring at (900, 448); one at (650, 300) put it at (650, 208); one at (717, 430) put it at (717, 287). Marking menus depend on slot directions relative to the cursor, so a moved ring breaks muscle memory and makes the first flick land on the wrong slot.
- Start here: `crates/ui/src/marking.rs:732-772` (ring placement, `clamp`) and `show` at 831. Keep the ring at the pointer (clamped only so the ring itself is inside the canvas), then place the list below, else above, else to the side.

## face list contents (P1)
- key: `ui:context-menus:face-list-contents`
- Fusion: Right-click on a face lists OK (Return), Cancel (Esc), Create Selection Set, Create Sketch, Extrude E, Offset Plane, Shell, Edit Feature, Edit Profile Sketch, Appearance A, Texture Map Controls, Properties, Show/Hide V, Selectable/Unselectable, Find in Browser, Find in Window. No navigation items, no recent commands.
- FissionOxide: The face list starts right (Create Sketch, Extrude, Offset Plane, Shell, Edit Feature (Fillet1), Appearance A, Properties, Show/Hide V, Look At, Find in Browser, Find in Window) and then appends the empty-canvas section (Pan, Zoom, Constrained Orbit, Fit, Home View, Remove, Show All, Hide All Bodies, Measure, Workspace) and the recent commands (Extrude E, 2-Point Rectangle, Create Sketch), so it is 24 rows and Extrude and Create Sketch appear twice. The first Extrude row shows the shortcut "Q" because it runs Press Pull.
- Start here: `crates/ui/src/marking.rs:434-560` (`overflow_items`): the face branch pushes the Press Pull item labelled Extrude at line 494, then falls through to the generic section. Stop after the face items, label the shortcut E (or run Extrude), add Create Selection Set, Edit Profile Sketch and Selectable/Unselectable (greyed if unsupported). Keep the recent-commands section only for the empty-canvas menu, as Fusion does.
