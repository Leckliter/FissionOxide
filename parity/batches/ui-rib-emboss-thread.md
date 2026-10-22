# Parity batch: ui-rib-emboss-thread

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 8) by opening each command from SOLID > CREATE in both apps,
FissionOxide built from `main` at `0c453dd1`. Fusion's Rib and Web are no longer refused on this
account. These dialogs are forms in `crates/ui/src/forms.rs` (THREAD 725, RIB 798, WEB 815,
THICKEN 886, EMBOSS 2161, BOUNDARY FILL 2255) and `crates/ui/src/dialogs/solid.rs:1112`.

## Rib rows (P2)
- key: `ui:dialogs:rib-rows`
- Fusion: RIB: Profile (Select chip; prompt "Select open sketch profile to create rib"), Thickness Direction (two icon toggles), Extent Type ("To Next" dropdown with icon), Flip Direction (icon button); OK disabled until a profile is picked.
- FissionOxide: RIB: Sketch (dropdown), "Lines (default all)", Thickness (2 mm), "Depth (empty = to next)", Flip direction checkbox; OK enabled with a red "the rib doesn't meet the body (give it a depth)".
- Start here: `crates/ui/src/forms.rs:798` (`RIB`).

## Web rows (P2)
- key: `ui:dialogs:web-rows`
- Fusion: WEB: Rib's rows plus Extend Curves (checked).
- FissionOxide: WEB copies RIB exactly, including the error text "the rib doesn't meet the body".
- Start here: `crates/ui/src/forms.rs:815` (`WEB`).

## Emboss rows (P2)
- key: `ui:dialogs:emboss-rows`
- Fusion: EMBOSS opens with Sketch Profiles (Select chip), Faces (Select chip), Tangent Chain (checked), Effect (two icon toggles: emboss, deboss); OK disabled. The depth and placement rows come after the picks.
- FissionOxide: EMBOSS: Sketch (dropdown), Profiles ("All"), Face, Depth (1 mm), Deboss and "Wrap around a cylinder" checkboxes, Horizontal distance, Vertical distance, Rotation, and a Preview checkbox in the footer.
- Start here: `crates/ui/src/forms.rs:2161` (`EMBOSS`).

## Thread rows (P1)
- key: `ui:dialogs:thread-rows`
- Fusion: THREAD (after picking a cylinder face, prompt "Specify size, or hold Ctrl/CMD to modify selections"): Faces, Modeled (unchecked), Full Length (checked), Threads Library (cloud button), Thread Type ("ANSI Unified Screw Threads" dropdown), Size ("1.0 in"), Designation ("1-8 UNC"), Class ("3A"), Direction ("Right hand"), Remember Size; the blue thread preview with the radius shown; status "1 Face | Radius: 0.496 in".
- FissionOxide: THREAD: Faces, "Modeled (cut the helix)", "Pitch (empty = ISO)", "Length (empty = full)", Offset, Left-handed and "Start from the other end" checkboxes, "Size (M10x1.5; empty = as drawn)" and "Class (empty = 6g / 6H)" text fields, Preview checkbox; status "1 Face | Area".
- Start here: `crates/ui/src/forms.rs:725` (`THREAD`): dropdowns fed by the thread tables already in the engine.

## Thicken rows (P2)
- key: `ui:dialogs:thicken-rows`
- Fusion: THICKEN: Faces / Surface Bodies (Select chip; prompt "Select Surfaces to Thicken"), Chain Selection (checked), Direction ("One Side" dropdown), Thicken Type (two icon toggles), Operation; the thickness row comes after the pick.
- FissionOxide: THICKEN: Surface bodies, Thickness (1 mm), Symmetric and Flip checkboxes, Operation, "Thicken type" dropdown (Sharp), Preview checkbox.
- Start here: `crates/ui/src/forms.rs:886` (`THICKEN`).

## Boundary Fill rows (P2)
- key: `ui:dialogs:boundary-fill-rows`
- Fusion: BOUNDARY FILL: Select Tools (chip; prompt "Select bodies & planes"), Select Cells (chip), Operation, Remove Tools (unchecked).
- FissionOxide: BOUNDARY FILL: Tools, a Plane dropdown (None), Cells "(no closed cells)", Operation, "Remove tools", Preview checkbox.
- Start here: `crates/ui/src/forms.rs:2255` and `crates/ui/src/dialogs/solid.rs:1112`.
