# Parity batch: ui-inspect-and-views

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## measure (P2)
- key: `ui:inspect:measure`
- Fusion: Measure (I): hover shows readouts, pick one or two entities for distance/angle/area, a results panel with copy buttons and a precision selector.
- FissionOxide: Measure (I) on two faces gives Distance, dX/dY/dZ and Angle (90.000 deg), with a label in the canvas. Missing Fusion's Precision and units selector, copy-to-clipboard buttons, a selection filter row, and single-pick readouts (area / length / position).
- Start here: `crates/ui/src/dialogs.rs`

## named views (P2)
- key: `ui:chrome:named-views`
- Fusion: Named Views folder in the browser: Top/Front/Right/Home plus user views; double-click restores the view; New Named View in the context menu.
- FissionOxide: Named Views lists TOP, FRONT, RIGHT, HOME. No way to save the current view as a named view (Fusion: right-click Named Views > New Named View), and no user views.
- Start here: `crates/ui/src/panels.rs`

## appearance dialog (P2)
- key: `ui:chrome:appearance-dialog`
- Fusion: Appearance (A): library on the right, drag a swatch onto a face or body, In This Design list, apply to bodies/components or faces.
- FissionOxide: Appearance is a form that takes an appearance name for a body. Fusion's Appearance panel has a library of swatches you drag onto faces or bodies, an In This Design list, and Bodies/Components vs Faces apply modes.
- Start here: `crates/ui/src/dialogs.rs`

## isolate (P2)
- key: `ui:chrome:isolate`
- Fusion: Isolate hides everything else and shows an exit banner; Unisolate restores.
- FissionOxide: Isolate exists in the body menu (toggles to Unisolate), but nothing on screen shows isolate mode is on. Fusion shows the isolated state in the browser and offers Unisolate from the canvas menu.
- Start here: `crates/ui/src/panels.rs`
