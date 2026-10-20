# Parity batch: ui-surface-sweep-loft

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## surface sweep loft (P1)
- key: `ui:tabs:surface-sweep-loft`
- Fusion: SURFACE > CREATE > Sweep and Loft make surface bodies: same dialogs as the solid ones but the output is a surface (open profiles allowed, no Operation join/cut).
- FissionOxide: Sweep and Loft are inert on the SURFACE tab (menus.rs:340-341). The solid Sweep/Loft dialogs exist; the surface versions need the surface output (ledger toolbar:SurfaceSweep/SurfaceLoft untested).
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/forms.rs`
