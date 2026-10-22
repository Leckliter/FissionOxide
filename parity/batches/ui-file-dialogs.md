# Parity batch: ui-file-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) from the File menu (the document icon in the application bar,
tooltip "File" in both) of an unsaved design in each app; FissionOxide built from `main` at
`42dec181`. Both menus grey Add To Assembly, Open Recovered Documents (0), Save As Latest, Create
Version, Share File Link and View Details on Web on an unsaved design.

## file menu items (P2)
- key: `ui:chrome:file-menu-items`
- Fusion: New..., Add To Assembly, Open... Ctrl+O, Open Recovered Documents (0), Upload... | Save Ctrl+S, Save As (greyed on an unsaved design), Save As Latest, Export... (a dialog) | Create Version | 3D Print, Capture Image..., Share File Link | View Details on Web, View >. Hovering an entry shows its rich tooltip (Open: "Opens a design or drawing from your projects or local computer." / "Select a design or drawing from the project browser and click Open, or click 'Open from my computer' and select a supported file." / "Press Ctrl+/ for more help.").
- FissionOxide: The same labels, but Save As is enabled on an unsaved design, "Export..." is a submenu of eight file types, the separators sit after Upload..., Save As Latest, Capture Image... and View Details on Web, and the entries have no tooltips.
- Start here: `crates/ui/src/panels.rs:243` (`FILE_MENU`: the separators and the Save As enabled flag), `crates/ui/src/panels.rs:282` (the Export submenu, see `ui:chrome:export-dialog`), `crates/ui/src/tips.rs` (tooltips).

## open dialog (P1)
- key: `ui:chrome:open-dialog`
- Fusion: Open... shows Fusion's own white "Open" window: a project tree at the left (Default Project selected, New Project, a bold "Samples" group: Workshops & Events, CAM Samples, Electronics Samples, Simulation Samples, Community Samples, Design Samples ...), a breadcrumb and a NAME / LAST UPDATED table at the right, an "Open from my computer..." button at the lower left, Cancel and a disabled Open at the lower right.
- FissionOxide: Open... shows the Windows Open dialog at once ("All supported files (*.fox;*.clf;...)").
- Start here: `crates/ui/src/lib.rs:378` (`open_file`) and the File menu action in `crates/ui/src/panels.rs:282`; the hub listing in `crates/ui/src/data_panel.rs:349` already knows the projects and designs for the tree and table, with "Open from my computer..." calling the file picker.

## export dialog (P1)
- key: `ui:chrome:export-dialog`
- Fusion: Export... opens a white modal "Export": Name (selected), Type dropdown "Autodesk Fusion Archive Files (*.f3d)" listing 3MF Files (*.3mf), Autodesk Inventor 2025 Files (*.ipt), DWG Files (*.dwg) (greyed), DXF Files (*.dxf) (greyed), FBX Files (*.fbx), IGES Files (*.igs *.iges) (greyed), OBJ Files (*.obj), SAT Files (*.sat) (greyed), SketchUp Files (*.skp), SMT Files (*.smt), STEP Files (*.stp *.step), STL Files (*.stl), USD Files (*.usdz); Location "C:/Users/<user>/Downloads" with a "..." button; a link "Need to export to IGES, SAT, DWG or DXF?"; Cancel and a blue Export. Escape closes an open Type dropdown but not the dialog.
- FissionOxide: Export... is a submenu: FissionOxide design (*.fox, *.clf), FissionOxide archive (*.foxz, *.clfz), STEP (*.step, *.stp), IGES (*.iges, *.igs), OpenCASCADE BRep (*.brep), STL mesh (*.stl), OBJ mesh (*.obj), 3MF (*.3mf), each going straight to a Windows save dialog.
- Start here: `crates/ui/src/panels.rs:282` (the `menu_button("Export...")`): one Export window with Name, a Type dropdown of the supported formats (grey the unsupported Fusion ones), a Location row with "...", then the file writer.

## save dialog layout (P2)
- key: `ui:chrome:save-dialog-layout`
- Fusion: The first Save is a white modal "Save" window with an X: "Name:" over a wide field with the name selected, "Location:" over a wide field "Default Project" with a dropdown button at its right, then Cancel and a blue Save at the lower right. No description or milestone fields on the first save.
- FissionOxide: A small floating "Save" window with the rows Name, Location (dropdown), Description "What changed (optional)", Milestone "Name it to mark a milestone (optional)" and Save / Cancel buttons at the lower left.
- Start here: `crates/ui/src/data_panel.rs:1024` (`save_window`): Fusion's layout for the first save, the description and milestone rows only for a later save (`ui:chrome:save-dialog` covers the versions).

## view submenu (P2)
- key: `ui:chrome:view-submenu`
- Fusion: File > View lists Hide ViewCube Ctrl+Alt+V, Hide Browser Ctrl+Alt+B, Hide Comments Ctrl+Alt+A, Hide Text Commands Ctrl+Alt+C, Hide Navigation Bar Ctrl+Alt+N, Hide Command Toolbar Ctrl+Alt+T, Show Data Panel Ctrl+Alt+P, Show Performance Overlay Ctrl+Alt+O, Reset to Default Layout Ctrl+Alt+R (the labels flip between Hide and Show).
- FissionOxide: Checkboxes Show Browser, Show Comments, Show Navigation Bar, Show ViewCube, then greyed Explode and Render Components..., no shortcuts, no Text Commands, Command Toolbar, Data Panel, Performance Overlay or Reset to Default Layout.
- Start here: `crates/ui/src/panels.rs:296` (the View submenu).

## 3d print dialog (P2)
- key: `ui:chrome:3d-print-dialog`
- Fusion: 3D Print opens a docked "3D PRINT" dialog: "Preparation Type" dropdown "Manufacture" and the text "Create a new additive setup in the Manufacture workspace to prepare your models for 3D printing, generate support structures, slice and export machine files such as G-Code or CLI.", an info icon, OK and Cancel. Escape closes it.
- FissionOxide: A large floating "3D Print" window with an X: Selection "1 visible body", Refinement dropdown "Medium", Format dropdown "STL (Binary)", a greyed "Send to 3D Print Utility" checkbox, the note "The mesh is tessellated at the design's display refinement.", OK / Cancel and a big empty area; Escape does not close it.
- Start here: `crates/ui/src/capture.rs:275` (the 3D Print window): dock it like the feature dialogs, size it to its rows, add the Preparation Type row.

## capture image dialog (P2)
- key: `ui:chrome:capture-image-dialog`
- Fusion: Capture Image... opens an "Image Options" window (Fusion icon in its title bar): the heading "Image Resolution", a centred dropdown "Current Document Window Size", rows Width "800" [Pixels dropdown] "W: 1200", Height "426" Pixels "H: 640", Resolution "72" Pixels/Inch, a greyed checked "Lock Aspect Ratio", then "Transparent Background" (off), "Enable Anti-aliasing" (on) and a single OK button; Escape closes it.
- FissionOxide: A large floating "Capture Image" window with an X: an "Image Resolution" row with the dropdown, Width "1003 px" and Height "731 px" greyed, Transparent Background, Enable Anti-aliasing, OK / Cancel and a big empty area; no Resolution row, no Lock Aspect Ratio, no unit dropdown; Escape does not close it.
- Start here: `crates/ui/src/capture.rs:198` (the Capture Image window).

## windows ignore escape (P1)
- key: `ui:chrome:windows-ignore-escape`
- Fusion: Escape closes the docked dialogs (3D PRINT), the Image Options window and the Data Panel's prompts; the modal Save / Open / Export windows close with their Cancel or X.
- FissionOxide: Escape closes none of the floating windows: Save, 3D Print, Capture Image, New project and New folder stayed open (stacked at the same place) until their Cancel buttons were clicked.
- Start here: `crates/ui/src/capture.rs:198`, `crates/ui/src/capture.rs:275`, `crates/ui/src/data_panel.rs:798`, `crates/ui/src/data_panel.rs:1024`: close each window when Escape is pressed while it is open (one helper for every egui `Window` with a Cancel).
