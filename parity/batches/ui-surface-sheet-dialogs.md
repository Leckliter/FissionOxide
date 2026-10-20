# Parity batch: ui-surface-sheet-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## surface dialogs (P1)
- key: `ui:dialogs:surface-dialogs`
- Fusion: SURFACE dialogs (before any selection): PATCH (Feature/Analysis tabs; Boundary Edges, Group Edges, Interior Rails/Points, Enable Chaining, Flip, Operation; continuity per edge after a pick). RULED (Feature/Analysis; Edges, Chain Selection, Type Normal/Direction/Tangent, Operation; Distance and Angle after a pick: one edge set swept out to a ruled strip). OFFSET (Faces/Surfaces, Chain Selection, Offset Type, Operation; Distance with an arrow after a pick). TRIM (Trim Tool first, then pick the cells to remove from a list). UNTRIM (Faces, Type All Edges/Internal/External, Extend). EXTEND (Feature/Analysis; Edges, Tangent Chain, Type Natural/Tangent/Perpendicular, Edge Alignment Free Edges/...; Distance after a pick). STITCH (Stitch Surfaces, Tolerance box plus slider, a Stitch Result section, an Unstitched Edges table with Edge/Max Gap and zoom-to buttons). THICKEN (Faces/Surfaces, Chain Selection, Direction One Side/Symmetric, Thicken Type, Operation). BOUNDARY FILL (Select Tools, Select Cells, Operation, Remove Tools).
- FissionOxide: forms.rs differs. RULED takes 'First edge' and 'Second edge' (a surface between two edges), while Fusion's Ruled sweeps one chained edge set out by Distance and Angle with Type Normal/Direction/Tangent; that is a different feature (ledger api:ruledSurfaceFeatures). PATCH has only Boundary edges, with no Group Edges, Interior Rails, Enable Chaining, Flip, Operation or continuity. OFFSET lacks Chain Selection, Offset Type and Operation. TRIM takes Surfaces plus a plane with 'Keep the other side', where Fusion picks a Trim Tool and then the cells to remove. EXTEND lacks Tangent Chain and Edge Alignment. STITCH lacks the result section and the Unstitched Edges table. THICKEN uses a Symmetric checkbox instead of Direction. BOUNDARY FILL has an extra Plane row. None of these dialogs has the Feature/Analysis tabs.
- Start here: `crates/ui/src/forms.rs`

## sheet metal dialogs (P2)
- key: `ui:dialogs:sheet-metal-dialogs`
- Fusion: SHEET METAL dialogs: FLANGE and HEM are one command with Type icons (Flange / Hem / Contour). Each has a table of selection sets (+ / x), one row per edge set with its own settings, and an Edges/Profiles pick. Height Datum, Bend Position, Angle and Corners follow a pick. FOLD: Reference pick plus a table of fold lines (+ / x); angle, Bend Position and Flip after a pick, with the bend line picked in the canvas. RIP: one Face/Edge/Vertex pick. CORNER CLOSURE: Edges pick, then corner options.
- FissionOxide: FissionOxide's Flange uses a Type dropdown and a single set of fields, without the per-edge-set table, and Hem is a separate dialog (Edges, Length, Gap, Flip). Fusion switches Flange/Hem/Contour with Type icons in one dialog and keeps a row per edge set. Fold picks its sketch and bend line from dropdowns (Sketch, Bend line) instead of a Reference canvas pick with a fold-line table. dialogs.rs fields(), Kind::Flange/Hem/Fold.
- Start here: `crates/ui/src/dialogs.rs`
