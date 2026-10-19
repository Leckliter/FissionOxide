# Fuzzing finds

Scripts (`fissionoxide-cli run FILE`) on which `crates/engine/tests/fuzz_features.rs` crashed the
process inside OpenCASCADE, at the last command:

| Script | Operation | Crash site |
|---|---|---|
| `shell-offset-segfault.jsonl` | shell of an already shelled body | `BRepOffset_MakeOffset::IntersectEdges` → `BRep_Tool::CurveOnSurface` |
| `unify-same-domain-segfault.jsonl` | cut (simplifying the boolean result) | `ShapeUpgrade_UnifySameDomain::IntUnifyFaces` → `FindCoordBounds` |
| `fillet-corner-segfault.jsonl` | fillet meeting a corner | `ChFi3d_Builder::IntersectMoreCorner` → `BRepAdaptor_Surface::Initialize` |

The kernel now catches OCCT's signals (`OCC_CATCH_SIGNALS` in every entry point), so these end in
an error (a red feature) instead of killing the app; `crash_scripts_fail_cleanly` keeps it so. The
operations themselves still fail where they could succeed, which is OCCT's to fix or ours to work
around.
