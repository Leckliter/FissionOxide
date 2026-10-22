# Parity batch: ui-extrude-manipulator

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 10): Extrude on a sketch profile in each app; Fusion in inches,
FissionOxide in mm, built from `main` at `2a6b280b`. Native-resolution crops of the manipulator
before, on hover, during a drag and after Tab. See `parity/reports/2026-10-22-manipulators.md`.

## Manipulator arrow look and drag feedback (P2)
- key: `ui:manip:arrow-style`
- Fusion: The distance handle is a solid 3D-looking blue arrow (fill about #1E9BD7 with a white outline) sitting on the far end of the extrusion, pointing along it; it does not change colour on hover and stays blue while dragged. During the drag a dashed blue-and-white track line runs along the drag axis through the arrow, and the taper handle fades. When the handle is not the active one (after Tab) the arrow is drawn light grey with a dark outline.
- FissionOxide: A flat 2 px line from the profile plane to the head with a flat triangle head (and a second triangle further on for the taper dial); it turns orange (255,140,20) on hover and while dragged; no track line; no grey inactive state.
- Start here: `crates/ui/src/viewport.rs:4859` (`manipulator`: colours, line, head) and `crates/ui/src/viewport.rs:4946` (`paint_arrow`, shared by other arrows).

## Taper handle: shape, place and grab (P1)
- key: `ui:manip:extrude-taper-handle`
- Fusion: The taper angle is a separate small handle beside the profile: a white disc with a black dot on a short grey arc that starts at the profile edge. It never overlaps the distance arrow. When the taper box is active (Tab) the disc turns blue in a white ring and the arc turns blue with small white tick marks.
- FissionOxide: The taper is a large translucent blue ellipse drawn through the arrow's tip with a "0 deg" text label, and its handle is the arrowhead at the tip of the distance arrow; a press near the tip drags the taper (the dialog's Taper angle went to -3 deg, ring and label "-3 deg" orange) instead of the distance, and the distance can only be grabbed at the inner arrowhead under the value box.
- Start here: `crates/ui/src/dialogs/manip.rs:426` (`taper_handle`: place it at the profile edge, not the arrow head), `crates/ui/src/handles.rs:188` (`draw`: disc-on-arc head, blue/grey states).

## Distance dimension shown in the preview (P2)
- key: `ui:manip:distance-dimension-label`
- Fusion: While Extrude is open, a thin dark dimension line runs along the extrusion inside the preview with the value as grey text ("0.601", "1.30", "2.00", unitless); Shell shows "0.10" by its arrow the same way, Box "1.00" by its height arrow.
- FissionOxide: No dimension line or value text in the canvas; only the floating box.
- Start here: `crates/ui/src/viewport.rs:4859` (draw the dimension beside the arrow's axis with the value formatted without its unit).

## Drag readout and snapping (P2)
- key: `ui:manip:drag-snap-readout`
- Fusion: While dragging the arrow the floating box shows the live value ("1.30 in") with the text not selected; when the arrow passes a model vertex or edge a tooltip by the pointer reads "Snap To : 0.0767692" and the value snaps to it; after release the box's text is selected again so typing replaces it.
- FissionOxide: The box keeps its text selected while dragging and moves with the arrow head; the value steps by 1 mm (a tenth of the grid) and never snaps to model geometry; no "Snap To" tooltip.
- Start here: `crates/ui/src/viewport.rs:4918` (the `Drag::Manip` branch: geometry snap and tooltip), `crates/ui/src/value_box.rs:87` (selection state while dragging).

## Tab moves to the next value box (P1)
- key: `ui:manip:tab-cycles-boxes`
- Fusion: Tab in the floating box moves focus to the command's next value: Extrude distance -> taper angle (the box jumps to the taper handle showing "0.0 deg" selected, that handle turns blue, the distance arrow turns grey), Tab again returns to the distance. The typed value is applied on Tab.
- FissionOxide: Tab applies the typed value ("25") and drops focus: the box stays at the distance arrow unfocused and no box opens at the taper handle.
- Start here: `crates/ui/src/value_box.rs:87` (`show_nth`: Tab), `crates/ui/src/viewport.rs:4859` and `crates/ui/src/handles.rs:227` (which handle owns the box).

## Typed values get their unit (P2)
- key: `ui:manip:typed-value-units`
- Fusion: Typing "2" into the floating box shows "2 in" in the dialog's Distance row at once (the unit is appended); a later Edit Feature shows the value with its unit.
- FissionOxide: Typing "25" leaves "25" in the Distance row and in Edit Extrude ("25", no "mm"); while typing, the box grows spinner arrows (a quantity box's look) that Fusion's length box never has.
- Start here: `crates/ui/src/value_box.rs:87` (spinner only for whole-number quantity boxes, not lengths) and `crates/ui/src/dialogs.rs:5391` (`value_box_field`: normalise a bare number to the document unit as the sketch heads-up boxes do).

## The preview is not hover-highlighted (P2)
- key: `ui:manip:preview-hover-highlight`
- Fusion: While the Extrude preview is shown, moving the pointer over the preview body highlights nothing (only the manipulators react); the tooltip "Hold Ctrl to modify selection" follows the pointer.
- FissionOxide: The preview's faces and edges pre-highlight under the pointer (the top face turns lighter, an edge turns black) as if they were pickable model geometry.
- Start here: `crates/ui/src/viewport.rs` hover picking while a dialog previews (skip preview geometry in the hover pick) and `crates/ui/src/preview.rs`.
