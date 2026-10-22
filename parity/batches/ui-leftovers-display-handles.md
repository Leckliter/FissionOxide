# Parity batch: ui-leftovers-display-handles

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

These rows were recorded by probers and fixers between 2026-10-21 and 2026-10-22 but never got a
batch (most were found by a fixer while working on a neighbouring item). The coordinator grouped
them on 2026-10-22; the wording below is the ledger row's own. Re-read the row in
`parity/ui-features.jsonl` (its `history`) before starting, and where a row says Fusion's side was
not observed, check it in Fusion's documentation and say so in your ledger note.

## shaded with visible edges only look (P3)
- key: `ui:navbar:shaded-visible-edges-look`
- Fusion: Visual Style > Shaded with Visible Edges Only (Ctrl+6) draws differently from Shaded (Ctrl+4); a prober should record how (edge colour, tangent edges).
- FissionOxide: FissionOxide draws Shaded with Visible Edges Only exactly as Shaded (light-grey visible edges); the difference in Fusion was not recorded.
- Start here: `crates/render/src/lib.rs`.

## effects not drawn (P3)
- key: `ui:navbar:effects-not-drawn`
- Fusion: Effects > Environment Dome, Object Shadow, Ambient Occlusion and Anti-Aliasing change the canvas (dome image behind the model, bodies shadowing each other, contact darkening, smooth edges).
- FissionOxide: FissionOxide keeps these four flags but its viewport renderer cannot draw them, so their rows are greyed.
- Start here: `crates/render/src/lib.rs`.

## plane names and user coordinate systems (P3)
- key: `ui:navbar:plane-names`
- Fusion: Fusion draws origin and construction plane names in the canvas (Object Visibility > Origin Plane Names, Construction Plane Names) and has user coordinate systems (User Coordinate Systems row).
- FissionOxide: FissionOxide draws no plane names and has no user coordinate systems; the three rows are greyed.
- Start here: `crates/ui/src/origin.rs`.

## ground plane offset applied (P3)
- key: `ui:navbar:ground-offset-applied`
- Fusion: With Adaptive off, Ground Plane Offset moves the ground plane (and its shadow) that far below the model.
- FissionOxide: FissionOxide keeps Adaptive and the offset but the effects' ground stays at the model's lowest point.
- Start here: `crates/render/src/lib.rs`.

## Fillet's setback handle (P2)
- key: `ui:manip:fillet-setback-handle`
- Fusion: Fillet's radius arrow has a black-and-white setback handle at the picked edge's end (seen with ui:manip:value-box-covers-arrow).
- FissionOxide: FissionOxide's Fillet shows only the radius arrow; no setback handle on the canvas.
- Start here: `crates/ui/src/dialogs/manip.rs`.

## Hem arrow and floating length box (P2)
- key: `ui:sheet:hem-manipulator`
- Fusion: after an edge pick, HEM shows an arrow and a floating value box "Thickness * 4" on the edge.
- FissionOxide: No arrow or floating length box on the hem's edge.
- Start here: `crates/ui/src/dialogs/manip.rs`, `crates/ui/src/dialogs/sheet.rs`.

## Solid commands don't take mesh bodies (P2)
- key: `ui:mesh:solid-commands-skip-mesh`
- Fusion: Solid commands do not see a mesh body as a solid until Convert Mesh: their body selections skip it.
- FissionOxide: Combine, Shell (whole bodies), patterns and the other solid commands that take whole bodies accept a mesh body (faces and edges of one can't be picked since 4790822). Found by the ui-mesh-body-and-insert fixer.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/forms.rs`.

## Timeline features belong to a component (P2)
- key: `ui:timeline:feature-owner-component`
- Fusion: each timeline feature belongs to the component it was made in: its tooltip names that component, Hide all inactive features hides the features of components other than the active one, and the Component Color Swatch colours each item by its component.
- FissionOxide: Timeline features don't record which component owns them, so the timeline tooltip always names the top level ((Unsaved)), Hide all inactive features has nothing to hide, and the Component Color Swatch only marks the items that made a component. Found by the ui-timeline-rollback-play fixer.
- Start here: `crates/model/src/lib.rs`, `crates/ui/src/panels/timeline_bar.rs`.
