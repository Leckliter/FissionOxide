# Parity batch: ui-sheet-rules-library

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 15) in a new unsaved Part Design document in each app (Fusion's made
with "+", millimeter, gram; FissionOxide in mm), FissionOxide built from `main` at `8b29a812`, on the
SHEET METAL tab. Fusion's rules come from a shared library; a design records the rule it uses and
the browser shows it. See `parity/reports/2026-10-22-sheet-metal-tab.md`.

## Sheet Metal Rules palette (P1)
- key: `ui:sheet:rules-palette`
- Fusion: MODIFY > Sheet Metal Rules opens a docked palette "SHEET METAL RULES" (no OK, only Close and an "i" icon) with a tree: "In this design" (the rules the design uses) and an expanded "Library" folder listing Steel (mm), Stainless Steel (mm), Aluminum (mm), Steel (in), Stainless Steel (in), Aluminum (in), Stainless Steel (0.0329 in), .050 Aluminum, 18 Gauge Steel, .125 Aluminum, .063 Aluminum (Default), .125 Steel, 10 Gauge Steel, .063 Steel, 16 Gauge Steel, 5/32 Single Wall Cardboard, .25 Aluminum, Stainless Steel (18 Gauge), Steel (in)(1). Each rule row expands to read-only values: Thickness, K Factor, Gap, then "Bend Conditions" (Bend radius, Relief shape, Relief width, Relief depth, Relief remnant) and "Corner Conditions" (2-Bend Intersection: Relief shape, Relief size, Relief placement; 3-Bend Intersection: Relief shape, Relief radius). Steel (mm) reads Thickness 2.50 mm, K Factor 0.44, Gap 2.50 mm; .063 Aluminum reads 0.063 in, 0.4, 0.063 in, bend radius 0.035 in, Round reliefs 0.063 / 0.032 / 0.126 in, 2-bend Round 0.252 in at Intersection, 3-bend Round with Radius 0.063 in. Hovering a row shows a pencil (edit) and a copy icon and a tooltip with the rule name; right-click gives Edit Rule, New Rule, Set as Default, Delete. Opening the palette does not dirty the document.
- FissionOxide: MODIFY > Sheet Metal Rules opens an OK/Cancel command dialog with a Rule dropdown holding only "Default", then Name, Thickness (1 mm), Bend radius (1 mm), K factor (0.44), Bend relief (Rectangular), Relief width, Relief depth, Gap, Corner relief (None), Corner relief size and "empty = from the thickness"; opening it marks the document unsaved ("Untitled*").
- Start here: `crates/ui/src/dialogs.rs:3336` (the SheetRules rows), `crates/ui/src/dialogs.rs:4361` (`load_rule`), `crates/model/src/lib.rs:217` (`SheetMetalRule`, default at :248); a library of named rules beside the model's rules and a palette like the Physical Material one in `crates/ui/src/panels.rs`.

## Edit Rule dialog (P1)
- key: `ui:sheet:edit-rule-dialog`
- Fusion: the row's pencil (or Edit Rule) opens a modal "Edit Rule" window: Rule Name (focused, ".063 Aluminum"), Thickness "0.063 in", K Factor "0.4", Gap "Thickness" (an expression), a collapsed "Bend Conditions" group (Bend radius ".035", Relief shape dropdown "Round", Relief width "Thickness", Relief depth "Thickness * 0.5", Relief remnant "Thickness * 2.0") and a collapsed "Corner Conditions" group with "2-Bend Intersection" (Relief shape "Round", Relief size "Thickness * 4.0", Relief placement "Intersection") and "3-Bend Intersection" (Relief shape "Round with Radius", Relief radius "Thickness"); Save is greyed until a value changes, then Cancel. Values default to expressions of Thickness.
- FissionOxide: no Edit Rule window; the rule's values are edited in the command dialog above, with Relief width / depth / Gap empty ("empty = from the thickness") and no Relief remnant, 2-bend placement or 3-bend rows.
- Start here: `crates/ui/src/dialogs.rs:3336`, `crates/model/src/sheet.rs:20` (`SheetInfo` lacks relief remnant and the 2/3-bend corner conditions), `crates/model/src/lib.rs:217`.

## Default rule and its values (P1)
- key: `ui:sheet:default-rule`
- Fusion: a new design's first flange uses the library default ".063 Aluminum (Default)" (0.063 in thick, K 0.4, bend radius .035 in, Round relief), also in a millimeter document; the Flange dialog's "Sheet Metal Rule" dropdown lists every library rule.
- FissionOxide: the only rule is "Default": 1 mm thick, 1 mm bend radius, K 0.44, Rectangular relief, Corner relief None.
- Start here: `crates/model/src/lib.rs:248` (the default rule), the rule list read by `crates/ui/src/dialogs.rs:3236` (Flange's Rule row).

## Rule row and Switch Rule in the browser (P1)
- key: `ui:sheet:browser-rule-row`
- Fusion: after the first flange the browser root's icon becomes a sheet metal component icon and a "Rule: .063 Aluminum" row (rules icon) appears under Document Settings, above Named Views; hovering it shows a "Switch Rule" icon button at its right, which opens a docked "SWITCH SHEET METAL RULE" dialog (Component "(Unsaved)", Sheet Metal Rule dropdown, OK greyed until changed, Cancel). Right-click on the row shows no menu. Sheet metal bodies get a sheet metal body icon in Bodies.
- FissionOxide: no Rule row, the root keeps its component icon and Body1 keeps the solid body icon after Flange.
- Start here: `crates/ui/src/panels.rs:1439` (the Document Settings rows of the browser tree), `crates/model/src/lib.rs:2065` (`sheet_rule()` of a body), the body icon choice in `crates/ui/src/panels.rs`.
