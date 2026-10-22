# Parity batch: ui-timeline-menus-groups

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 17) with both windows 908 px wide; Fusion's design had Sketch1,
Extrude1, Fillet1, Sketch2..4 and Plane1, FissionOxide's (built from `main` at `f1f9ed56`) Sketch1,
Extrude1, Fillet1, Plane1, Sketch2, Sketch3, Fillet2 and RectangularPattern1. The batch
`ui-timeline-rollback-play` also edits `crates/ui/src/panels.rs`'s timeline (other functions);
rebase if both are in flight. See `parity/reports/2026-10-22-timeline-browser.md`.

## Sketch item's timeline menu (P1)
- key: `ui:timeline:sketch-item-menu`
- Fusion: right-click on a sketch's timeline icon: Create Selection Set / (separator) Extrude (E) / (separator) Edit Sketch / Redefine Sketch Plane / Select Sketch Plane / Configure / (separator) Delete (Del) / Rename / (separator) Roll Timeline Marker Here / Convert to DM Feature / Suppress Features / (separator) Find in Browser / Find in Window.
- FissionOxide: Create Selection Set (greyed) / Edit Feature / Configure (greyed) / Delete / Rename / Roll Timeline Marker Here / Suppress Features / Create Group / Find in Browser / Find in Window: no Extrude, "Edit Feature" instead of "Edit Sketch", no Redefine Sketch Plane, Select Sketch Plane or Convert to DM Feature.
- Start here: `crates/ui/src/panels.rs:3833` (`timeline_entries`: add the sketch entries; the browser's sketch row menu already has Extrude, Redefine and Select Sketch Plane, reuse its actions from `crates/ui/src/panels/sketch_rows.rs`) and `crates/ui/src/panels.rs:3855` (`timeline_menu`).

## Single item's menu: no Create Group, Find in Browser enabled (P2)
- key: `ui:timeline:single-item-menu-extras`
- Fusion: a single feature's menu (Extrude1, Fillet1, Plane1) has no Create Group; Create Group appears only when two or more items are selected. Find in Browser is enabled on Extrude1 (it finds the body), and a separator follows Create Selection Set.
- FissionOxide: every single item's menu ends its middle block with "Create Group"; on Extrude1 Find in Browser is greyed; no separator after Create Selection Set.
- Start here: `crates/ui/src/panels.rs:3850` (the `E::Groups` entry pushed for every item) and `crates/ui/src/panels.rs:3933` (`E::FindInBrowser`: Extrude1's body was not found).

## Timeline multi-selection with Shift and Ctrl (P1)
- key: `ui:timeline:multi-select`
- Fusion: click Sketch2's icon, Shift+click Sketch4's: the three icons are selected (blue), the status bar reads "3 Features", the canvas highlights all three; right-click lists Create Selection Set / Extrude (E) / Edit Sketch / Configure (greyed) / (separator) Create Group / (separator) Delete (Del) / Rename / (separator) Roll Timeline Marker Here / Convert to DM Feature / Suppress Features.
- FissionOxide: Shift+click and Ctrl+click on a second icon (sent by the automation tool as a quick modifier-down, click, modifier-up) replace the selection with that one item; the status bar names only it. `click_feature` supports ranges, but the click reads `ui.input(|i| i.modifiers)` after the frame's events, when the modifier is already up; Fusion took the same clicks.
- Start here: `crates/ui/src/panels.rs:3686` (read the modifiers from the click's own `egui::Event::PointerButton` instead of the frame-end state), `crates/ui/src/panels/timeline_groups.rs:59` (`click_feature`), and a menu for a multi-selection in `crates/ui/src/panels.rs:3855`.

## Group icon and bracket (P2)
- key: `ui:timeline:group-icon`
- Fusion: Create Group collapses the items into one group icon (three small blue bars) with no name prompt; under it the timeline's track line carries a small "+" box. Expand Group puts the member icons back with no group icon in the row and a bracket under them with a "-" box at its middle; clicking the box collapses the group again.
- FissionOxide: a collapsed group is a folder icon; expanded, an open-folder icon stays in the row before the members, with a thin blue bracket under them and no +/- box.
- Start here: `crates/ui/src/panels/timeline_groups.rs:142` (`group_icon`) and the `bracket` drawing in the same file; `crates/ui/src/panels.rs:3611` (where an open group still allocates its icon).

## Group tooltip lists its members (P2)
- key: `ui:timeline:group-tooltip`
- Fusion: hovering a collapsed group shows its dark tooltip with one line per member (its icon, its name, a green check mark for a healthy feature), a divider, and the group's name "Group1".
- FissionOxide: hovering the collapsed group shows no tooltip.
- Start here: `crates/ui/src/panels/timeline_groups.rs:142` (`group_icon`'s response); share the tooltip frame with `ui:timeline:item-tooltip` (batch ui-timeline-rollback-play) if that lands first.

## Group's right-click menu (P2)
- key: `ui:timeline:group-menu`
- Fusion: right-click on a collapsed group of sketches: Create Selection Set / Extrude (E) / Edit Sketch / Configure / (separator) Expand Group / (separator) a red-X "Edit Group" entry with the Del hint / Rename / (separator) Suppress Features; the status bar reads "4 Features" (the group and its three members are selected).
- FissionOxide: Expand Group / Delete (Del) / Rename / Roll Timeline Marker Here / Suppress Features / Ungroup; the status bar names nothing new.
- Start here: `crates/ui/src/panels/timeline_groups.rs:195` (`group_menu`). Fusion's red-X entry was not clicked (it may delete the members); keep Ungroup's action behind it only if Fusion's turns out to ungroup.
