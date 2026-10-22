# Parity batch: ui-feature-handles

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 10): Fillet, Shell, Press Pull (Offset Face) and Draft on a box in
each app; Fusion in inches, FissionOxide in mm, built from `main` at `2a6b280b`. See
`parity/reports/2026-10-22-manipulators.md`. The dialog rows of Draft and Offset Face are already
in `ui-draft-scale-dialogs`; these items are the canvas handles.

## The value box hides the arrow (P1)
- key: `ui:manip:value-box-covers-arrow`
- Fusion: Fillet puts a blue radius arrow on the picked edge or face (with a black-and-white setback handle at the edge's end), Shell a blue arrow on the removed face pointing into the wall, Offset Face a blue arrow on the face; the floating value box sits apart from the arrow (below and to the right of it), so the arrow stays visible and grabbable.
- FissionOxide: The box is placed at the arrow's head and covers it: Fillet, Shell and Offset Face show only a blue tick beside the box, so the arrow cannot be seen or dragged; Offset Face's box lands on a different face from the one picked.
- Start here: `crates/ui/src/viewport.rs:4859` (`manipulator`: `head_at`) and `crates/ui/src/handles.rs:182` (`label_at`): offset the box from the head.

## Arrow length follows large values off the canvas (P2)
- key: `ui:manip:arrow-length-offscreen`
- Fusion: With a fillet radius of 5 in on a 1 in box the arrow and the floating box stay near the edge; the box never leaves the canvas.
- FissionOxide: With a 100 mm fillet radius the arrow is drawn 100 mm long and the box follows its head off the canvas onto the timeline.
- Start here: `crates/ui/src/viewport.rs:4859` (cap the arrow's screen length and clamp the box into the canvas rect).

## Draft angle handle (P1)
- key: `ui:manip:draft-angle-handle`
- Fusion: Draft shows the pull direction as a hollow white arrow with a dark outline on the pull plane, and the angle handle as a white disc with a dark dot on a black arc above the face. Hovering the disc draws a dashed full circle round the face; dragging it turns disc and arc blue, tilts the preview live and opens a floating box with the angle ("35.0 deg", 5 deg steps).
- FissionOxide: A short solid blue arrow and a translucent blue ellipse with a "3 deg" label sit at the face's lower edge; pressing on the ellipse starts a window selection (an orange box) instead of turning the angle.
- Start here: `crates/ui/src/dialogs/manip.rs:452` (`draft_handles`), `crates/ui/src/handles.rs:171` (`hit`: catch the dial's ring and head) and `crates/ui/src/handles.rs:188` (`draw`).

## Fillet and Shell start at zero (P2)
- key: `ui:manip:zero-default-value`
- Fusion: After the first pick Fillet's floating box reads "0.00 in" (selected, focused) and Shell's "0.00 in"; nothing is previewed until a value is typed or the arrow dragged.
- FissionOxide: Fillet starts at "2 mm" and Shell at "2 mm" with the preview already applied.
- Start here: the `Kind::Fillet` / `Kind::Shell` defaults in `crates/ui/src/dialogs.rs` (`radius`) and `ok_enabled` at `crates/ui/src/dialogs.rs:2348`.
