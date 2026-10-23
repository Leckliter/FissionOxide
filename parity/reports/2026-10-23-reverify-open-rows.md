# 2026-10-23: open-row pass (verifier 8)

Built `main` at `6c5c8b6b` on Windows (no build fix needed) and launched it at x 0..727 (half the
1920 px screen) with the Data Panel closed; Fusion at x 0..1215.

## What was waiting

No `fixing-merged` key could be checked: the only four left are the usual ones that can't be checked
here (`ui:marking-menu:flick`, `ui:chrome:comments-expand`, `ui:sketch:vertical-on-horizontal-line`,
`ui:timeline:feature-owner-component`). No open `mismatch` row has a batch that is `done` in
`index.jsonl`; the open rows whose batch is `fixing` / `ready` were left to their fixers. So the pass
took the 13 open rows that had no batch at all, confirmed what could be confirmed in the two apps,
recorded Fusion's side where a row said it was unrecorded, and batched all of them (wave 45).
`ui:assemble:joint-glyphs` (`untested`) is left for the ASSEMBLY prober.

## Confirmed in the apps

- Length display (`ui:chrome:unit-display-hide-trailing-zeroes`, now P2): in a mm design at default
  preferences Fusion's sketch dimension reads "12.346" for 12.3456, "12.30" for 12.3 and "10.00" for
  10 (three decimals, trailing zeroes trimmed to two). FissionOxide reads "12.35" and "10.00".
- Preferences pages (`ui:chrome:preferences-unrecorded-pages`): every page the row listed is now
  recorded from Fusion (appendix below). FissionOxide's Optional Features and Electronics pages are
  titles only; Manufacture has group names only, ticks Enable Cloud Libraries and lacks CAM Data;
  Drawing greys every row, has Edge as a check box, unticked Include Mesh / Surface Bodies and stops
  at Large Text Height.
- Check box label column (`ui:dialogs:checkbox-label-column`): MOVE/COPY's Create Copy box carries
  its label in the value column; Fusion's label sits in the label column.
- Patch Interior Rails/Points (`ui:surface:patch-interior-rails`): still greyed.
- Lights (`ui:render:light-editing`): Fusion's Lights command, glyph, live preview, Lights browser
  folder and EDIT LIGHT dialog recorded; FissionOxide places a light with none of them.
- Solid Holder (`ui:manufacture:solid-holder-submenu`): Fusion's submenu is one entry, "Milling Tool
  Holder", with its tooltip recorded; FissionOxide has a guessed "Create Solid Holder".

## New rows

- `ui:dialogs:move-rows-after-pick` (P2): Fusion's 3D MOVE/COPY shows only Move Object, Selection,
  Move Type and Set Pivot until a body is picked; FissionOxide shows the six boxes and Create Copy at
  once (`crates/ui/src/dialogs/move_copy.rs:887`).
- `ui:sketch:dim-edit-tooltip-style` (P2): the dimension edit box's parameter tooltip ("d1 : 10 mm")
  is a light box; Fusion's ("d25 : 10 mm") is dark with white text (`crates/ui/src/viewport.rs:3127`).

## Not re-checked (batched as found)

`ui:manufacture:show-tool-on-cursor` (needs a setup with a toolpath in Fusion),
`ui:navbar:user-coordinate-systems`, `ui:animation:commands`, `ui:dialogs:chamfer-corner-geometry`,
`ui:sheet:butt-corner-relief-flat`. `cargo test --release -p fox-engine --test reference_parts`
passes on Windows (2 passed, 256 s), so `ui:sketch:text-reference-parts-font` and
`ui:sketch:reference-parts-other-failures` fail only in the cloud container; 467-470's `part.jsonl`
import DXFs by absolute paths on another machine.

## Matches seen on the way (no row change)

- Finish Sketch returns to the view from before the sketch in both apps.
- FissionOxide's Preferences tree has Fusion's top-level order (General with API, Design, Manufacture,
  Electronics, Render, Drawing, Simulation and Generative Design; Tokens; Material; Graphics; Network;
  Data Collection and Use; Unit and Value Display; Default Units; Preview Features; Compatibility &
  Troubleshooting).

## Retractions and traps

- Once a click landed while Windows reported an "Unidentified app" in front (FissionOxide's title bar
  faded after Escape in a dimension edit box); `open_application` then started a second FissionOxide
  instance. The first instance was responding (`Get-Process`), so the second was killed and the
  first brought back from the taskbar. Not an app hang.
- A first R after editing a dimension drew no rectangle; the second try worked. Not reproduced, no row.

## Batches (wave 45, `map-wave45.json`, all `ready`)

| Batch | Items | Keys |
|---|---|---|
| `ui-reverify-prefs-1` | 2 | unit-display-hide-trailing-zeroes, preferences-unrecorded-pages |
| `ui-reverify-dialogs-8` | 4 | move-rows-after-pick (new), checkbox-label-column, patch-interior-rails, dim-edit-tooltip-style (new) |
| `ui-reverify-workspaces-3` | 4 | render light-editing, solid-holder-submenu, show-tool-on-cursor, user-coordinate-systems |
| `ui-animation-actions-1` | 1 | animation:commands (P1) |
| `ui-kernel-tests-1` | 4 | reference-parts-other-failures, text-reference-parts-font, chamfer-corner-geometry, butt-corner-relief-flat |

## Fusion state left

"Untitled*" switched to mm units (Set as default unticked), with Sketch6 holding a 12.3 mm line;
"Untitled*(1)" visited Manufacture and Render, a light placed and undone (it is back in DESIGN);
Preferences opened and cancelled (the Preview Features filter "plastic" was cleared with Reset
filters). Nothing saved.

## Appendix: Fusion Preferences pages

### Manufacture > Optional Features
Header: "Preferences that allow optional capabilities in Manufacturing to be disabled if necessary".
Grey info box: "These settings improve functionality for some Fusion features. If you need additional time to adjust your workflows to any of the improvements, you may choose to change a setting to revert to the original, older functionality." / "In future updates, these improvements will be Fusion's default behavior and you will no longer be able to change settings here."
Check boxes (label left, box right), defaults: Automatically get latest Posts, Machines and Print Settings [x]; Disable discarding unused CAD references [ ]; Disable legacy face selection conversion [ ]; Enable legacy dependency analysis [ ]; Flow (Old) [ ]; Legacy Chaining [ ]; Legacy Contour Compensation for Wrap [ ]; Legacy Drilling [ ]; Legacy flat area detection in 3D Offset Roughing (Pocket) [ ]; Legacy handling of ambiguous references [ ]; Legacy surface tessellation [ ]; Legacy toolpath calculation format [ ]; Modifications for 3D Adaptive Roughing and 3D Offset Roughing [ ]; Old DXF Export for Nesting [ ]; Old Profile Turning [ ]; Re-enable the use of the Model Override [ ]; Skip Collision Detection for Visual Stock Simulation [x]; Spun Profile [x]; Tool Library - 3D Preview [x].
### Manufacture (main page)
Header: "Preferences controlling behavior in the Manufacture workspace". Rows: Post processor editor [empty box][...]; NC file editor [empty box][...]; Enable Cloud Libraries [ ]; Default workspace tab [Last Used v]; Default operation type [From Active Tab v]; Default operation tool [From previous operation v].
Bold "Browser Display Properties": Show tool number [x]; Show tool compensation offsets [ ]; Show operation machining time [ ]; Show tool description [ ]; Show setup work offset and spindle [ ]; Show cycle type [x]; Show manual NC [x]; Show pattern type [x]; Highlight toolpaths in canvas [x]; Show toolpath when editing [ ]; Show toolpaths in NC Program [ ].
Bold "Prompts": Confirm generation when already valid [x]; Automatically generate toolpath on operation change [x]; Show overridden parameters after reference document update [x]; Show the Save As dialog when posting from NC Programs [ ].
Bold "Additive": Generate Additive Arrange on creation or edit [x]; Generate Supports on creation or edit [x]; Generate Optimized Orientations on creation or edit [x]; Generate Additive Toolpath on Setup creation [ ]; Generate Interference Analysis on creation or edit [x].
Bold "Inspection Results": Use project-to-CAD calculation [x]; Projection search distance (mm) [1.00 spinner]; Results display type [Cylinder v].
Bold "Colors": Tolerance Colors (above upper tolerance / below lower tolerance) [Blue (above) / Red (below) Default v].
Bold "Tool Library": Enable logging [ ]; Reset 3D tool preview [ ]; Show validation errors and warnings in tool list [ ].
Bold "Manufacturing Model": Enable component sources [ ].
Bold "Turning": Diameter Programming [ ].
Bold "IPS and Remaining Stock": Processing Mode [Standard (computer processor) v]; In-Process Stock - Mesh Check Safety [x].
Bold "CAM Data": Enable CAM data import [x].
### Electronics > Color
Header "Color Preferences for Electronics". Group "Theme": a 6-column swatch grid, columns Board, Grid Dots, Grid Lines, Canvas, Dimmed, Indicator; rows Dark (black, mid grey, near-black, near-black, grey, dark gold) and Light (white, dark grey, light grey/white, light grey, grey, light gold); button "Reset to default colors" and a blue link "Preview".
Group "Highlight": slider Darker ... Brighter at "50 %"; [ ] High Contrast; Hue: (greyed slider); button Reset.
Group "PCB Constraints and Routing": [x] Use different fill style for unfinished routes; Fill style of unfinished routes [black swatch button]; [x] Live DRC; Fill style of error polygons [hatched swatch button]. (Underlined access keys on labels.)
### Electronics > Misc
Header "Misc Preferences for Electronics". Left column check boxes (box left of label): Beep [x]; Check connects [x]; Optimizing [x]; Ratsnest processes polygons [ ]; Multi-threaded copper pouring [ ]; Display pad names [x]; Display signal names on pads [x]; Display signal names on traces [x]; Display via lengths [x]; Display drills [x]; Auto end net and bus [x]; Auto set junction [x]; Group command default on [x]; Enable GPU rendering [x].
Right column label/box/unit: Min. visible text size [3] pixel; Min. visible grid size [5] pixel; Catch factor [5] %; Select factor [2] %; Snap length [20 mil]; Xref label format [%F%N/%S.%C%R]; Xref part format [/%S.%C%R].
### Electronics > Default Grid and Units
Header "Preferences that set default grid settings and units used in new electronics designs". Bold "Schematic & Symbols": Display Grid for Schematic [ ]; Display Grid for Symbols [x]; Size [0.1 spinner] [inch v] link "Finest"; Multiple [1]; Alt [0.01 spinner] [inch v] "Finest". Bold "2D PCB & Footprints": Display Grid for 2D PCB [ ]; Display Grid for Footprints [x]; Size [50] [mil v] "Finest"; Multiple [1]; Alt [5] [mil v] "Finest".
### Electronics > Default Values
Header "Preferences that set defaults values used in electronics command input". Bold "Design Layers": [x] Display layer IDs; [ ] Display vias and pads in their respective routable layers. Then groups each holding tables with columns Default (radio) / Value / Unit (mil), a + and x under each table and a reset arrow at its right:
Traces/Lines: Width (6 default, 7.87402, 8, ...). VIAs: Diameter (auto default, 19.68504, 21.65354, ...); Drill Sizes (13.77953 default, 15.74803, 17.71654, ...). Hole (NPTH): Drill Sizes. PTH Pad: Diameter (auto, 10, 12, ...); Drill Sizes (23.62205 default, 25.59055, 27.55906, ...). SMD Pad: Size (50 x 25 default, 50 x 26, 56 x 28, ...); Roundness [0 % v]. Polygons: Isolate (0 default, 10, 12, ...); Spacing (50 default, 56, 66, ...). Miter: Radius (0 default, 10, 12.5, ...). Text: Size (70 default, 76, 86, ...). Dimension: Line Width (5.11811 default, 10, 10.23622, ...); Extension Width (auto default, 3.93701, 5, ...); Extension Length (auto default, 39.37008, 50, ...).
### Electronics > Drill Legend
Header "Preferences that control drill symbol defaults used in new electronics designs". Left "Symbol Assignments": table # / Symbol / mm / mil, rows 1 + 0.2 7.9; 2 x 0.25 9.8; 3 (square) 0.3 11.8; 4 (circle) 0.35 13.8; 5 (hourglass) 0.4 15.7; 6 (bowtie) 0.45 17.7; 7 0.5 19.7; 8 0.55 21.7; 9 0.6 23.6; 10 0.65 25.6; 11 0.7 27.6; 12 0.75 29.5 ... (blue glyphs), + and x below, reset arrow. Right "Symbol Appearance": Diameter [80 mil]; Width [1 mil].
### Electronics > Directory
Header "Preferences that sets directories used in electronics documents". Rows (path box + "..." button): Directory for Library (...Applications/Electron/LibEagle/examples/libraries/examples); Directory for Design Rules (.../design rules/examples); Use Library.io for Design Rules by default [x]; Directory for ULP (.../ulps/examples); Use Library.io for ULP by default [x]; Directory for Scripts (.../scripts/examples); Use Library.io for Scripts by default [x]; Directory for CAM Jobs (.../cam/examples); Use Library.io for CAM Jobs by default [x]; Directory for Simulation (.../LibEagle/ngspice/bin); Directory for SPICE Models (.../spice/examples).
### Electronics > 3D PCB
Header "3D PCB Preferences for Electronics". One row: Open 3D PCB automatically when an electronics design file is opened [ ].
### Drawing
Header "Preferences for the Drawing and Drawing Template "From Scratch" option". Rows: Standard [ISO v]; Units [in v]; Sheet Size [A3 Landscape v]; Delimiter [Decimal v]; Edge [(empty) v]; Override or Restore Format Defaults Below [ ]; then greyed until that box is ticked: Include Mesh Bodies [x]; Include Surface Bodies [x]; Projection Angle [First Angle v]; Text Height Group [Medium v]; Small Text Height 2.5mm, Medium Text Height 3.5mm, Large Text Height 5mm (greyed static text); Small Text Height [1.8mm v], Medium Text Height [1.8mm v], Large Text Height [1.8mm v] (dropdowns, dark text); Font [Arial v]; Dimension Format [Decimal v]; Linear Dimension Precision [0.12 v]; Angular Dimension Precision [0.12 v]; Display Dimension Units [ ]; Display Trailing Zeros [ ]; Display Leading Zeros [x]; Line Width Group [Thin v]; Thin Line Width 0.18mm, Medium Line Width 0.25mm, Thick Line Width 0.35mm (static); Thin Line Width [0.25mm v], Medium Line Width [0.35mm v], Thick Line Width [0.50mm v]; Display Line Widths [ ]. End of page.
### Network (tree row between Graphics and Data Collection and Use)
Header "These are a set of options that are used for network access." Windows network proxy setting [Automatic v]; Proxy Host [empty]; Proxy Port [0 spinner]; Server Verification [Warn when accessing through an intermediate server v].
### Data Collection and Use
Header "Data Collection and Use". Bold "Improve your Fusion experience", two short paragraphs (what usage data is for; manage privacy here or in your profile, with links "analytics programs page" and "Privacy Statement"), then three grey cards, each a check box and a bold title followed by one sentence: [x, greyed] "Required data collection."; [x] "Help develop new products and services."; [x] "Personalize our offers."
### Preview Features
Header "Preferences to try Preview Functionality". Grey info box (feedback on preview features; you need to be a Fusion Insider, link "Get more information about becoming a Fusion Insider"), a sentence on enabling with the link "Autodesk Terms of Use", then a filter bar: [All workspaces v], three toggle icon buttons (green eye, orange lock, green tick), a Search box and a "Reset filters" link. Sections (collapsible, bold title + one-line description): General: [x] Enable Close Speedup Experience (green eye icon), "Enable document close speedup experience". Design: [ ] Preview Volumetric Modeling Features (orange lock), "Enables Volumetric Texture and Modify Lattice commands. Available under Solid/Mesh > Modify Tab". Simulation: [x] New Simulation results experience (orange lock), "Activates improved post-processing features and a new visual style for Linear Static Stress, Thermal and Thermal Stress Studies." With a filter that matches nothing: "-- No features are displayed due to filter settings. --".
