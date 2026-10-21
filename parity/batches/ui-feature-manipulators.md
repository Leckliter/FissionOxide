# Parity batch: ui-feature-manipulators

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## floating value box (P1)
- key: `ui:dialogs:floating-value-box`
- Fusion: As soon as Extrude has a profile, a blue distance arrow, a circular taper handle and a floating value box ("0.00 in" with a ⋮ menu) appear at the manipulator. The box has keyboard focus, so typing goes straight into it and the dialog's Distance row mirrors it; Enter is OK. Fillet shows a radius arrow on the picked edges and the same floating box ("0.00 in"). Edit Feature from the timeline shows the arrow and the box again with the current value.
- FissionOxide: Typed digits do reach the dialog's Distance / Radius field, but there is no floating box, no fillet radius arrow and no taper handle. The extrude arrow is a small triangle at the top face with no label until it is dragged.
- Start here: `crates/ui/src/dialogs.rs:1696` (`Dialog::manipulator`, one arrow per dialog) and `crates/ui/src/viewport.rs:2365` (the sketch HUD's focused floating `TextEdit`, which can be reused for features). Add a fillet manipulator (arrow normal to the first picked edge's adjacent face) and an extrude taper handle.

## preview color (P2)
- key: `ui:canvas:preview-color`
- Fusion: Feature previews draw in the body's own appearance, with only the picked faces or profile in selection blue.
- FissionOxide: The whole preview body draws in the blue selection colour.
- Start here: `crates/engine/src/scene.rs` (preview body colour) and the preview path in `crates/ui/src/dialogs.rs` near line 2038.

## fillet fields 2 (P2)
- key: `ui:dialogs:fillet-fields-2`
- Fusion: FILLET has a Type dropdown (Fillet / Rule Fillet / Full Round Fillet), a selection-set table (one row per set: "1 Face | 0.00 in | setback | Tangent (G1)"), + and x, Radius Type, Edges/Faces/Features Select, Tangent Chain, Tangency Weight, Corner Type. There is no Preview checkbox: it always previews.
- FissionOxide: Edges/Faces chip, radius field and "+", Tangent chain, Radius type (Constant / Chord length / Variable), Corner type, Preview checkbox (on by default), OK, Cancel. Missing the Type dropdown, the per-set table with continuity, Tangency Weight. The Preview checkbox has no Fusion counterpart.
- Start here: `crates/ui/src/dialogs.rs` (`Kind::Fillet` rows). Full Round came with the driver session's #97; Rule Fillet has no engine support, so show it greyed.
