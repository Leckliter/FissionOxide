# Parity batch: ui-kernel-tests-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Batched on 2026-10-23 by the open-row pass (verifier 8) on `main` at `6c5c8b6b`: four rows found by
fixers that had no batch. `cargo test --release -p fox-engine --test reference_parts` passes on
Windows at that commit (2 passed, 256 s), so the two reference-part rows are failures of the cloud
container only; make the parts pass there too. Every row is already in `parity/ui-features.jsonl`
(do not run `ledger_from_batches.py`); read each row's `history` before starting.

## ui-kernel-tests-1 (P2)

- key: `ui:sketch:reference-parts-other-failures`
- Fusion: every reference part regenerates to the measurements recorded from Fusion (`reference_parts` passes everywhere).
- FissionOxide: in the cloud container ten cases fail (222, 225, 280, 294, 383, 385, 467-470). 467-470's `part.jsonl` import DXFs by absolute paths on another machine (`C:/Users/colli/OneDrive/.../parity/probes/importdxffilecommand/<name>/part.dxf`): check those DXFs into `tests/reference/<case>/` and reference them relatively (and find out why the Windows run passes without them). Look at the other six in the container (OCCT version or platform differences).
- Start here: `crates/engine/tests/reference_parts.rs`, `tests/reference/467-mm-plate-lines-arc-circle/part.jsonl`

- key: `ui:sketch:text-reference-parts-font`
- Fusion: text bodies keep the outlines the reference parts were measured with.
- FissionOxide: since cd2aa0b (text Font starts on Arial, a metric-compatible stand-in where Arial isn't installed) 253-256 fail in the cloud container (253: volume 38.359 vs 38.148; 255: 27 faces vs 28). Pin the font the parts were measured with, or ship it, so they pass there.
- Start here: `crates/engine/tests/reference_parts.rs`, `tests/reference/253-text-single-glyph`

- key: `ui:dialogs:chamfer-corner-geometry`
- Fusion: CHAMFER's Corner Type changes the corner where three chamfered edges meet: Chamfer a flat patch, Miter the chamfer faces extended to a point, Blend a smooth patch.
- FissionOxide: Corner Type is chosen and stored, but the kernel (`BRepFilletAPI_MakeChamfer`) closes every such corner with a flat triangular patch, so Miter and Blend look like Chamfer.
- Start here: `crates/kernel/cpp/kernel.cpp` (chamfer), `crates/model/src/regen.rs`

- key: `ui:sheet:butt-corner-relief-flat`
- Fusion: a tray of four edge flanges with butt corners under a rule with a Round 2-bend corner relief (Fusion's default) flattens to a closed outline with the relief cut at each corner.
- FissionOxide: `tests/reference/69-sm-tray` with corner relief Round gives a flat pattern whose DXF check reports no closed cut contour and four cut entities meeting at each corner.
- Start here: `crates/model/src/sheet.rs`
