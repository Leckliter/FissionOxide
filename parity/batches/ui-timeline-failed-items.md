# Parity batch: ui-timeline-failed-items

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 11): a chamfer made to fail by an upstream edit (Fusion: Cylinder
height 0.05 in under a 0.1 in Chamfer1) or by its own value (FissionOxide: 50 mm on a 20 mm box),
then the error toast, the status-bar badge, the item's right-click menu, Suppress, Unsuppress and
Roll Timeline Marker Here in both apps; plus Hole, Combine and Press Pull results. FissionOxide built
from `main` at `ed3546c5`. Roll Timeline Marker Here on a failed item matches (the marker moves after
it, later items fade). The toast itself is `ui:timeline:error-toast` (batch `ui-edit-feature-errors`).
See `parity/reports/2026-10-22-edit-construct.md`.

## Error dialog from the badge and Review warning (P1)
- key: `ui:timeline:error-dialog`
- Fusion: Clicking the red X badge at the status bar's right end (or Review warning in the item's menu) opens a window titled "Error" (Fusion icon, close x) with a tree: the feature row "Chamfer1" (its icon with a red outline) > "Compute Failed" (red circled X, expandable) > the message "The fillet/chamfer could not be created at the requested size. Try adjusting the size, deselecting some of the edges (try disabling Tangent Chain), or using multiple separate operations." with a red X; a copy icon at the lower left and a Close button. The toast's "More Info" link closes the toast.
- FissionOxide: No badge and no Error window; the message is only in the timeline icon's tooltip ("chamfer: the chamfer could not be computed (distance too large?)").
- Start here: `crates/ui/src/panels.rs:2919` (status-bar badge) and a new window next to the toast at `crates/ui/src/panels.rs:3157`, listing every failed or warned feature from the timeline state (`crates/ui/src/panels.rs:3405`).

## Failed item's right-click menu (P2)
- key: `ui:timeline:failed-item-menu`
- Fusion: Right-click on a failed timeline item: Create Selection Set / Edit Feature / Configure / (separator) Delete (Del) / Rename / (separator) Roll Timeline Marker Here / Convert to DM Feature / Suppress Features / Review warning (red X icon) / (separator) Find in Browser / Find in Window.
- FissionOxide: Edit Feature / Delete / Rename / Roll Timeline Marker Here / Suppress Features / Create Group / Find in Browser (greyed) / Find in Window: no Review warning (and no Create Selection Set, Configure without a configuration table, Convert to DM Feature).
- Start here: `crates/ui/src/panels.rs:3439` (`timeline_menu`: add Review warning for a failed or warned item, opening `ui:timeline:error-dialog`).

## Suppressed item's menu and look (P2)
- key: `ui:timeline:suppressed-item-menu`
- Fusion: Suppressing the failed Chamfer1 turns its icon into a faded icon on a pink square, clears the status-bar error badge; its menu then has no Edit Feature and no Review warning and reads "Unsuppress Features". Unsuppress brings the error back with the toast ("1 error(s)", "Chamfer1").
- FissionOxide: The suppressed icon gets a red diagonal strike; its menu still offers Edit Feature.
- Start here: `crates/ui/src/panels.rs:3439` (`timeline_menu`: hide Edit Feature when `suppressed`).

## The "no editable properties" message stays (P2)
- key: `ui:chrome:stale-status-message`
- Fusion: Lower-left messages are transient and are replaced by the next command's prompt.
- FissionOxide: "This feature has no editable properties yet" stayed at the canvas's lower left for over three minutes, through Escape, Move, Combine, Press Pull, a chamfer edit and Suppress.
- Start here: `crates/ui/src/panels.rs:3632` (the message) and where `app.status` is cleared (clear it on the next command or after a few seconds).

## Hole adds no sketch item (P2)
- key: `ui:timeline:hole-extra-sketch`
- Fusion: A single hole placed on a face adds one timeline item, Hole1.
- FissionOxide: The same hole adds a Sketch item and then Hole1.
- Start here: `crates/ui/src/dialogs.rs` (the Hole dialog's command for a face placement) and `crates/engine/src/commands.rs:1036` (`hole` takes `face` and `at` without a sketch).

## Combine of separate bodies keeps the target's name (P2)
- key: `ui:dialogs:combine-disjoint-names`
- Fusion: Join of two spheres that do not touch leaves two bodies: the target keeps its name (Body3) and the tool's lump gets a new one (Body7).
- FissionOxide: Join of a box (Body1, target) and a cylinder (Body2) that only touch: the cylinder's lump is named Body1 and the box becomes Body4.
- Start here: `crates/engine/src/commands.rs:1237` and the Combine evaluation that names the result bodies (give the target's lump the target's name).

## Press Pull on a primitive edits the primitive (P2)
- key: `ui:dialogs:press-pull-edits-owner`
- Fusion: Press Pull (Q) on a cylinder primitive's top face opens "OFFSET FACE" with Distance "1 in" (the cylinder's height); typing 0.2 sets the cylinder's Height to 0.2 in (Edit Cylinder shows it) and adds no timeline item.
- FissionOxide: Press Pull on a cylinder's top face opens "OFFSET FACE" with Distance "2 mm" and adds a new PressPull1 item.
- Start here: `crates/ui/src/dialogs.rs` (the Press Pull entry: when the face is the end face of an extrude or primitive, edit that feature's distance or height instead).
