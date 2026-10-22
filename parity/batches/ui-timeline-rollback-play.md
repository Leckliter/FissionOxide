# Parity batch: ui-timeline-rollback-play

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 17) with both windows 908 px wide, FissionOxide built from `main` at
`f1f9ed56`. The batch `ui-timeline-menus-groups` also edits the timeline in
`crates/ui/src/panels.rs` and `crates/ui/src/panels/timeline_groups.rs`; rebase if both are in
flight. See `parity/reports/2026-10-22-timeline-browser.md`.

## Rolled-back features leave the browser (P1)
- key: `ui:timeline:rollback-browser-rows`
- Fusion: dragging the marker to just after Extrude1 removes the rows of everything after it from the browser (the Sketches folder lists only Sketch1; Sketch2..4 rows are gone, as is the Construction folder's plane); rolling forward brings them back. The same happens while Edit Feature has rolled the design back.
- FissionOxide: the rolled-back Sketch2 and Sketch3 rows stay in the Sketches folder in grey, and Plane1 stays under Construction in normal text (its plane is gone from the canvas), both after a marker drag and during Edit Feature on Extrude1.
- Start here: `crates/ui/src/panels/sketch_rows.rs:283` (rolled-back sketches drawn MUTED instead of skipped), the Construction folder at `crates/ui/src/panels.rs:2285` (skip rolled-back planes; drop the folder when none is left), and the body rows near `crates/ui/src/panels.rs:1961`.

## Marker drag updates the model live (P1)
- key: `ui:timeline:marker-drag-live`
- Fusion: while the marker is dragged, the canvas and browser follow it at each slot (the fillet and later sketches disappear as soon as the marker passes them) and a dark tooltip above the marker names the feature just left of it ("Extrude1" / "(Unsaved)"). Hovering the marker shows no tooltip.
- FissionOxide: during the drag only the icons after the slot fade; the canvas keeps every feature until the button is released; no tooltip follows the marker; resting on the marker shows "Drag to roll the timeline back or forward".
- Start here: `crates/ui/src/panels.rs:3731` (`timeline_drag`: run `timeline.marker` when the slot changes, inside one undo step), `crates/ui/src/panels.rs:3961` (`marker`'s hover text).

## Play replays from the start (P2)
- key: `ui:timeline:play-from-start`
- Fusion: with the marker after Fillet1 (third of six items), Play jumps the marker to the beginning and steps it forward to where it was (after Fillet1), then stops; while it plays the Play button is a stop square.
- FissionOxide: Play steps the marker from where it is to the end of the timeline, every 0.45 s; the button keeps its play triangle.
- Start here: `crates/ui/src/panels.rs:3396` (the `app.playing` stepping) and the Play button at `crates/ui/src/panels.rs:3429` (remember the start marker, draw a stop icon while `app.playing` is set; clicking it stops).

## Timeline settings menu (P2)
- key: `ui:timeline:settings-menu`
- Fusion: the gear at the timeline's right end opens: Change to Direct Modeling (icon) / a "Component Color Swatch" checkbox (off) / a "Hide all inactive features" checkbox (on).
- FissionOxide: the gear opens "Roll to end" and "Roll to beginning".
- Start here: `crates/ui/src/panels.rs:3448` (the gear's popup); Change to Direct Modeling is already on the browser root's menu (reuse its action).

## Timeline item tooltip (P2)
- key: `ui:timeline:item-tooltip`
- Fusion: resting on a timeline icon shows a dark rounded tooltip above it: the feature's icon and name ("Extrude1"), a divider, and the owning component's name ("(Unsaved)"). A suppressed feature's tooltip is the same (no "suppressed" text).
- FissionOxide: a light, square-cornered tooltip with just the name ("Sketch1"); a suppressed feature's reads "Fillet1" / "(suppressed)".
- Start here: `crates/ui/src/panels.rs:3635` (`hover` text built for `on_hover_text`); draw a custom tooltip like the ribbon's (`crates/ui/src/ribbon_tip.rs`) with the icon, name and component.

## Suppressed healthy feature's icon (P2)
- key: `ui:timeline:suppressed-healthy-icon`
- Fusion: suppressing a feature that computes (Fillet1) only fades its icon to light grey; there is no pink square (the pink square belongs to a failed feature that was suppressed, see `ui:timeline:suppressed-item-menu`).
- FissionOxide: the suppressed Fillet1 draws its faded icon on a pink square.
- Start here: `crates/ui/src/panels.rs:3672` (the `SUPPRESSED_FILL` square: draw it only when the feature also has an error).

## Drag-reorder feedback (P2)
- key: `ui:timeline:drag-feedback`
- Fusion: dragging Fillet1 along the timeline highlights the dragged icon, draws a black insertion bar in the slot under the pointer, and shows the dark tooltip of the feature left of the slot ("Sketch4" / "(Unsaved)"); slots where it cannot go show no bar. Nothing is left at the lower left after the drop.
- FissionOxide: a blue insertion line (red where the feature cannot go) and no tooltip; a refused drop writes "Can't move there: 'Plane1' is used by Sketch3, which would come before it" at the lower left, which stayed there through a later successful drag, a fillet and a pattern.
- Start here: `crates/ui/src/panels.rs:3731` (`timeline_drag`), `crates/ui/src/panels.rs:3778` (the refused-drop status; clear it on the next action or after a few seconds).
