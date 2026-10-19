# Fuzzing finds

Scripts (`fissionoxide-cli run FILE`) that `crates/engine/tests/fuzz_features.rs` found crashing
the process inside OpenCASCADE. Each segfaults (no C++ exception, so `FOX_TRY`/`FOX_CATCH` can't
turn it into a failed feature) at the last command:

| Script | Operation | Crash site |
|---|---|---|
| `shell-offset-segfault.jsonl` | shell of an already shelled body | `BRepOffset_MakeOffset::IntersectEdges` → `BRep_Tool::CurveOnSurface` |
| `unify-same-domain-segfault.jsonl` | cut (simplifying the boolean result) | `ShapeUpgrade_UnifySameDomain::IntUnifyFaces` → `FindCoordBounds` |
| `fillet-corner-segfault.jsonl` | fillet meeting a corner | `ChFi3d_Builder::IntersectMoreCorner` → `BRepAdaptor_Surface::Initialize` |

All three need the kernel to survive a crash in OpenCASCADE: turning signals into exceptions
(`OSD::SetSignal` with `OCC_CATCH_SIGNALS` in the guarded calls) or running such operations out
of process. The fuzzer avoids the first by shelling once per sequence (set `FOX_FUZZ_RESHELL=1` to
allow more), and its default seeds avoid the others.

When one of these is fixed, the script should run to the end; the fuzzer seed that found it can
then join `fuzz_found_bugs`.
