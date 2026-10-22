# Parity batch: ui-insert-new-design

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 12), FissionOxide built from `main` at `c4d97aa9`. The INSERT and
ASSEMBLE lists themselves are `ui:menus:insert-list` and `ui:menus:assemble-part-design` in batch
`ui-inspect-insert-assemble`; this batch is what the entries open and the new-design flow that makes
an assembly document. Do the "+" item first: it loses work. See
`parity/reports/2026-10-22-construct-inspect-insert.md`.

## "+" replaces the open design without asking (P0)
- key: `ui:chrome:new-tab-replaces-design`
- Fusion: The "+" at the end of the document tabs opens "What do you want to design?" and Create New adds a second document tab; the first design stays open, unsaved, in its own tab.
- FissionOxide: "+" (tooltip "New Design (Ctrl+N)") calls `new_design`, which replaces the session: an unsaved design with a box, a sphere and a construction point vanished with no prompt, and the bar still shows a single "Untitled" tab.
- Start here: `crates/ui/src/panels.rs:134` and `crates/ui/src/lib.rs:786` (`new_design` swaps `self.s`); at the least ask to save when the design is dirty, ideally keep one session per tab.

## New design type chooser and the Assembly Design document (P1)
- key: `ui:assemble:assembly-design-document`
- Fusion: "+" opens a modal "What do you want to design?" with six tiles (Part Design, Assembly Design, Hybrid Design, Drawing, Electronics Design, Electronics Library), a right pane with the tile's description, Type (two icons) and Unit System ("Inch (in), ouncemass"), and Learn More, Cancel, Create New. An Assembly Design opens with tabs ASSEMBLY, MANAGE, UTILITIES and the panels INSERT (Insert Component, Insert Fastener, Duplicate With Joints, Insert Derive, Decal, Canvas | Add To Assembly | Insert McMaster-Carr Component), CREATE (New Component, Create Sketch, Pattern >, Create Linked PCB, Derive), RELATIONSHIPS (Constrain Components, Align Constraint, Angle Constraint, Center Constraint, Tangent Constraint | Joint J, As-Built Joint Shift+J, Joint Origin, Rigid Group), MOTION (Drive Joints, Motion Link, Motion Study), MODIFY (Move/Copy M, Align, Delete Del, Remove | Bill of Materials | Change Parameters, Physical Material, Appearance A | Manage Materials | Compute All Ctrl+B), CONTACT (Enable Contact Sets, Enable All Contact), CONFIGURE, CONSTRUCT, INSPECT, SELECT; the browser's Document Settings shows "Assembly Design".
- FissionOxide: Every new design is a Part Design with the SOLID / SURFACE / MESH / SHEET METAL / PLASTIC / MANAGE / UTILITIES tabs; there is no chooser, no Assembly Design type and no ASSEMBLY tab.
- Start here: `crates/ui/src/lib.rs:786` (`new_design`), `crates/ui/src/menus.rs:283` (the ASSEMBLE list) and the tab table in `crates/ui/src/menus.rs` (around `shared_groups!`, line 520).

## New Component in an assembly creates an external design (P2)
- key: `ui:assemble:assembly-new-component`
- Fusion: In an Assembly Design, NEW COMPONENT has Parent ("1 selected", with "(Unsaved)" under it), Design Type (four icon toggles: part, assembly, hybrid, electronics), Part Type (two icon toggles), Design Name "External1", Location (a folder chip "Default Project" with an x), Activate (icon), OK and Cancel.
- FissionOxide: Only the Part Design New Component exists (Type, Name, Part Number, Description, Parent, Create, Activate).
- Start here: the New Component dialog in `crates/ui/src/dialogs.rs` (`Kind::NewComponent`); depends on the Assembly Design item above.

## Insert entries open Fusion's Insert window (P1)
- key: `ui:insert:file-chooser`
- Fusion: Decal, Canvas and Insert SVG open a modal "Insert" window: a PROJECT list (the account's projects), a file table (NAME, LAST UPDATED), "Insert from my computer...", Cancel and Insert (disabled until a pick). Insert Mesh opens the same window with "Select from my computer..." and Select. Insert DXF opens a docked INSERT DXF dialog: Plane/Sketch ("Select"), Select DXF File (folder icon), Insert Mode "Single Sketch". Insert Derive opens "Select Source" (projects and files only, Cancel and Select).
- FissionOxide: Decal and Insert Derive show "isn't in FissionOxide yet". Insert SVG, Insert DXF and Insert Mesh all open the Windows file dialog "Insert into the design" filtered to "All supported files (*.f3d;*.cf...)", the same as Insert STEP, and the INSERT dropdown stays open behind it and swallows the next click. Canvas opens a floating "CANVAS" window in the middle of the canvas (Image "Select...", Plane dropdown "XY plane", Width (mm) "100", Opacity slider "0.50").
- Start here: `crates/ui/src/menus.rs:265` (the INSERT list maps SVG, DXF and Mesh to `Cmd::InsertStep`), `crates/ui/src/canvas_ui.rs:241` (the CANVAS window), `crates/ui/src/file_dialogs.rs` (Fusion's Open window, reusable for Insert).
