//! The command set. Each command takes JSON parameters and returns JSON.

use std::path::PathBuf;

use fox_kernel::glam::DVec3;
use fox_model::{
  AttrTarget, AxisRef, BendPosition, BodyId, BodySel, Design, EdgeRef, Extent, ExtentKind, FaceRef, FeatureId,
  FeatureKind, HoleKind, LoftSection, Operation, OriginPlane, ParamKind, PlaneRef, Primitive, ReliefShape,
  SheetMetalRule, ThinSide,
};
use fox_sketch::{ConstraintId, ConstraintKind, CurveId, CurveKind, Dim, PointId, PtSpec, Sketch};
use serde_json::{Value, json};

use crate::{CmdError, CmdResult, Sel, Session};

/// Snap radius (mm) for joining new sketch geometry to existing points.
const SNAP: f64 = 1e-6;

pub struct CommandSpec {
  pub id: &'static str,
  pub help: &'static str,
  pub params: &'static str,
}

macro_rules! cmds {
  ($( $id:literal => $help:literal, $params:literal; )*) => {
    pub const COMMANDS: &[CommandSpec] = &[ $( CommandSpec { id: $id, help: $help, params: $params } ),* ];
  };
}

cmds! {
  "help" => "List commands", "{}";
  "info" => "Design summary: parameters, timeline with status, bodies with mass properties", "{}";
  "new" => "Start a new design", "{name?}";
  "open" => "Open a design (.fox, .foxz) or any importable file as a new design", "{path}";
  "save" => "Save the design to a file (.fox JSON, or .foxz archive)", "{path?}";
  "export" => "Export by extension: .step .iges .brep (exact), .stl .obj .3mf .ply .glb .usdz (mesh), .dxf .svg (sketch), .fox .foxz", "{path, bodies?: [id], sketch?, ascii?: bool}";
  "import" => "Import into this design: STEP, IGES, BRep, STL, OBJ, 3MF, PLY (bodies), DXF, SVG (new sketch), .fox/.foxz (their bodies)", "{path, merge_coplanar?: bool, plane?}";
  "formats" => "Every file format with import/export support", "{}";
  "inspect" => "Describe a file without opening it (Fusion .f3d contents, mesh sizes, DXF entities)", "{path}";
  "hub.info" => "Data hub location and totals; the hub item the design is linked to", "{}";
  "hub.projects" => "List projects", "{}";
  "hub.project.create" => "New project", "{name}";
  "hub.project.rename" => "Rename a project", "{project, name}";
  "hub.project.delete" => "Move a project to the trash", "{project}";
  "hub.project.restore" => "Restore a trashed project", "{project}";
  "hub.folder.create" => "New folder", "{project?, parent?, name}";
  "hub.folder.rename" => "Rename a folder", "{folder, name}";
  "hub.folder.delete" => "Delete an empty folder", "{folder}";
  "hub.list" => "Folders and items in a project folder", "{project?, folder?}";
  "hub.save" => "Save the design to the hub: a new version of its item, or a new item", "{project?, folder?, name?, description?, milestone?, new?: bool}";
  "hub.open" => "Open a hub item (latest or a given version)", "{item, version?}";
  "hub.versions" => "Version history of an item", "{item?}";
  "hub.promote" => "Make an old version the latest (copies it as a new version)", "{item?, version}";
  "hub.milestone" => "Mark a version as a milestone (no name clears it)", "{item?, version, name?}";
  "hub.describe" => "Set a version's description", "{item?, version, description}";
  "hub.version.delete" => "Delete an old, non-milestone version", "{item?, version}";
  "hub.compare" => "What changed between two versions of a design", "{item?, a, b?}";
  "hub.item.rename" => "Rename an item", "{item?, name}";
  "hub.item.move" => "Move an item to another project/folder", "{item?, project?, folder?}";
  "hub.item.copy" => "Copy an item version to a new item", "{item?, version?, project?, folder?, name?}";
  "hub.item.delete" => "Move an item to the trash", "{item?}";
  "hub.item.restore" => "Restore an item from the trash", "{item}";
  "hub.item.purge" => "Permanently delete a trashed item", "{item}";
  "hub.trash" => "Trashed items and projects", "{project?}";
  "hub.recent" => "Recently opened or saved items", "{limit?}";
  "hub.search" => "Find items by name, version description or milestone", "{query}";
  "hub.activity" => "Activity feed", "{project?, item?, limit?}";
  "hub.upload" => "Upload a file: designs and importable formats become designs, others (e.g. .f3d) are stored as files", "{path, project?, folder?, name?}";
  "hub.download" => "Write an item version to a file; the extension picks the format (.foxz includes history)", "{item?, version?, path, ...export options}";
  "compute_all" => "Recompute the whole timeline from scratch", "{}";
  "undo" => "Undo", "{}";
  "redo" => "Redo", "{}";
  "transaction.begin" => "Group the following commands into one undo step (for scripts and plugins)", "{}";
  "transaction.commit" => "End the transaction, keeping its changes as one undo step", "{}";
  "transaction.rollback" => "End the transaction, undoing everything done since it began", "{}";
  "param.add" => "Add a user parameter", "{name, expr, kind?: length|angle|unitless, comment?}";
  "param.set" => "Change a parameter's expression", "{name, expr}";
  "param.rename" => "Rename a parameter (rewrites expressions using it)", "{name, to}";
  "param.remove" => "Delete an unused parameter", "{name}";
  "sketch.create" => "New sketch on a plane and start editing it", "{plane: \"xy\"|\"xz\"|\"yz\"|{feature}|{body, face}|{point: [x,y,z], normal?} (face there)|{origin: [x,y,z], normal, x?} (free plane)}";
  "sketch.edit" => "Edit an existing sketch", "{sketch}";
  "sketch.finish" => "Stop editing the sketch", "{}";
  "sketch.line" => "Line or polyline (consecutive points share endpoints), or a midpoint line: mirrored about mid, which is held at its middle", "{points: [[x,y],...], closed?: bool, sketch?} | {mid: [x,y], end: [x,y], sketch?}";
  "sketch.rect" => "Two-point rectangle, centre rectangle, or 3-point (rotated) rectangle", "{corner1, corner2} | {center, corner} | {points: [a, b, c]}";
  "sketch.circle" => "Circle: centre and size, through 2 points (a diameter) or 3 points", "{center: [x,y], radius | diameter} | {through: [[x,y], [x,y], [x,y]?]}";
  "sketch.arc" => "Arc through three points, around a centre, or tangent to a curve's end", "{start, mid, end} | {center, start, end, clockwise?: bool} | {center, start, sweep: angle (negative = clockwise)} | {tangent: curve, start, end}";
  "sketch.polygon" => "Regular polygon: inscribed (vertex), circumscribed (edge midpoint) or from one edge", "{center, vertex, sides, circumscribed?} | {edge: [a, b], sides}";
  "sketch.slot" => "Slot: centre to centre, overall, centre point, or along an arc", "{start, end, width, kind?: center_to_center|overall|center_point} | {center, end, width} | {arc_center, start, end, width, ccw?: bool (default: the shorter arc)} | {start, mid, end, width}";
  "sketch.point" => "Sketch point", "{at: [x,y]}";
  "sketch.constrain" => "Add a geometric constraint", "{type: coincident|horizontal|vertical|parallel|perpendicular|collinear|equal|tangent|concentric|midpoint|point_on_curve|symmetric|fix|curvature|polygon, points?: [id], curves?: [id]} (symmetric: points [a, b] + curves [axis], or curves [a, b, axis] for two lines or circles/arcs; fix takes a point or curves; delete its constraints to unfix) (polygon: lines already forming a regular polygon; returns its centre point)";
  "sketch.dimension" => "Add a driving dimension", "{type: distance|horizontal|vertical|radius|diameter|angle|point_line, points?, curves?, value: expr}";
  "sketch.set_dimension" => "Change a dimension's value (expression)", "{constraint, value}";
  "sketch.delete" => "Delete sketch geometry or constraints", "{curves?, points?, constraints?}";
  "sketch.construction" => "Toggle construction geometry", "{curves: [id], on?: bool}";
  "sketch.drag" => "Drag a sketch point (solver keeps constraints)", "{point, to: [x,y]}";
  "sketch.transform" => "Move, rotate and/or scale sketch curves (scale and rotation about 'center'); dimensions inside the moved geometry scale with it", "{sketch?, curves?: [id] (default all), translate?: [x,y], angle?, scale?, center?: [x,y]}";
  "sketch.insert" => "Copy curves into a sketch from another sketch, a DXF or SVG file or a sketch in a .fox/.foxz design, transformed", "{sketch?, from: {sketch: id} | {path, sketch?: name|id}, curves?: [id], translate?, angle?, scale?, center?}";
  "sketch.trace" => "Trace an image (photo, logo, scan) into closed sketch curves: one colour's outlines, or each of N colours", "{sketch?, image: path, mode?: outline|colors, threshold?: 0-255 (auto), invert?, colors?: 2-16, layers?: [index] (default all but the background), blur?: px, specks?: px² ignored, holes?: px² filled, tolerance?: px, corners?: deg, fit?: lines|arcs|curves, width? | height? (expressions) | scale?: mm per px (default 100 mm wide), at?: [x,y] (image centre), angle?} → ink: inside points of the traced shapes (extrude's 'profiles')";
  "trace.info" => "What tracing an image would give (size, colour layers, shape counts) without changing the design", "{image, ...sketch.trace options}";
  "sketch.visible" => "Show or hide a sketch (null = automatic: hidden once a feature uses it)", "{sketch, visible: bool | null}";
  "sketch.info" => "Sketch geometry (with the sketch plane, curve lengths and boxes), constraints, profiles (box, perimeter, bounding curves), curves outside every profile, degrees of freedom", "{sketch?, samples?: bool (points along each curve), loops?: bool (profile boundary segments)}";
  "sketch.fillet" => "Round the corner where two lines meet", "{curves: [line, line], radius}";
  "sketch.chamfer" => "Bevel the corner where two lines meet", "{curves: [line, line], distance, distance2?}";
  "sketch.spline" => "Smooth curve through points", "{points: [[x,y], ...]}";
  "sketch.control_spline" => "Cubic B-spline from control points", "{points: [[x,y], ...]}";
  "sketch.ellipse" => "Ellipse from its centre, the end of the major axis and the minor radius", "{center, major: [x,y], minor}";
  "sketch.conic" => "Exact conic curve from start to end whose end tangents meet at the apex; rho shapes it (below 0.5 elliptical, 0.5 a parabola, above 0.5 hyperbolic)", "{start, end, apex, rho?: 0.01 to 0.99}";
  "sketch.text" => "Editable text: closed glyph outlines you can extrude, regenerated when the text changes (explode: plain curves)", "{at: [x,y] | point id, text, height?, angle?: deg, font?: sans|mono|font file path|system font name, bold?, italic?, underline?, halign?: left|center|right, valign?: baseline|top|middle|bottom, spacing?: % of height, explode?}";
  "sketch.offset" => "Offset a chain of lines/arcs or circles (positive = outward / left)", "{curves: [id], distance}";
  "sketch.trim" => "Remove the piece of a curve between its nearest crossings", "{curve?, at: [x,y]}";
  "sketch.extend" => "Extend a line to the next curve", "{curve?, at: [x,y]}";
  "sketch.mirror" => "Mirror curves across a line (symmetry-constrained)", "{curves: [id], line}";
  "sketch.pattern_rect" => "Rectangular pattern of sketch curves", "{curves, direction?: [x,y] | line id, count, spacing, symmetric?, direction2?: [x,y] | line id (default +Y), count2?, spacing2?, symmetric2?, distance_type?: spacing|extent (extent: spacing is the whole pattern size)}";
  "sketch.pattern_circ" => "Circular pattern of sketch curves", "{curves, center?: [x,y], count, angle?, symmetric?}";
  "sketch.project" => "Project a body's edges (or one face's edges) into the sketch; they follow the body", "{body, face?, associative?}";
  "extrude" => "Extrude sketch profiles", "{sketch?, profiles?: [[x,y]] (points inside regions) | indices, distance, extent?: one_side|symmetric|two_sides|through_all|through_all_both|to_face|to_body, to?: face (flat or curved) | {body}, start?: offset from the sketch plane, start_face?: flat face parallel to the sketch to start at, distance2?, flip?, taper?: angle (positive narrows), taper2?: angle for the second side of two_sides (default taper), thin?: thickness, thin_side?: inside|outside|center, operation?: new_body|join|cut|intersect, targets?: [body] (bodies join/cut may change)}";
  "revolve" => "Revolve sketch profiles around an axis", "{sketch?, profiles?, axis: \"x\"|\"y\"|\"z\"|{line: curve}|{body, edge}, angle?, operation?, targets?: [body] (bodies join/cut may change)}";
  "loft" => "Loft through profiles of several sketches, in order", "{sections: [sketch id | {sketch, profile?: [x,y]}], ruled?, closed?, rails?: [sketch id | {sketch, curves?}], centerline?: sketch id | {sketch, curves?}, operation?, targets?: [body] (bodies join/cut may change)}";
  "sweep" => "Sweep sketch profiles along a path (open lines/arcs of another sketch)", "{sketch?, profiles?, path: sketch id, path_curves?: [id], rail?: sketch id | {sketch, curves?} (guide rail), twist?: angle, operation?, targets?: [body] (bodies join/cut may change)}";
  "hole" => "Holes at sketch points (into the face the sketch is on)", "{sketch?, points?: [id], diameter, depth? | through_all?, kind?: simple|counterbore|countersink, head_diameter?, head_depth?, head_angle?, tip_angle?, flip?}";
  "split" => "Split bodies along a plane", "{plane, bodies?}";
  "draft" => "Taper faces about a neutral plane; symmetric or angle2 split the faces at the plane and taper both sides away from it (angle2 on the side against the normal)", "{faces: [{body, face} | {point}], plane, angle, flip?, symmetric?: bool, angle2?}";
  "fillet" => "Round edges (an edge is {body, edge} or {point: [x,y,z]} on it); radius2 makes it variable; chord makes radius the round's width across the edge; setback re-blends corners where three edges meet; faces round all their edges; tangent_chain adds the edges that continue them smoothly; sets give each selection its own radius", "{edges?: [{body, edge} | {point}], faces?: [{body, face}], radius, sets?: [{edges?, faces?, radius}] (instead of edges, faces and radius), tangent_chain?: bool, radius2?, chord?: bool, setback?}";
  "chamfer" => "Bevel edges: equal distance, two distances, or distance and angle", "{edges: [{body, edge}], distance, distance2? | angle?}";
  "press_pull" => "Push (+) or pull (−) flat faces along their normals", "{faces: [{body, face} | {point}], distance}";
  "split_by_body" => "Split bodies with another body", "{tool, bodies?, remove_tool?}";
  "shell" => "Hollow bodies, opening the given faces (a face is {body, face} or {point, normal?}; tangent_chain adds the faces that continue them smoothly) or whole bodies (closed, hollow inside); walls go inside the surface, outside it, or both (thickness inside, outside_thickness outside); walls meet in sharp corners unless corners is round", "{faces?: [{body, face} | {point}], bodies?: [body], thickness, direction?: \"inside\"|\"outside\"|\"both\", outside_thickness?, tangent_chain?: bool, corners?: \"sharp\"|\"round\"}";
  "box" => "Box primitive (base centred on a plane point)", "{plane?, at?, width, depth, height, operation?, targets?: [body] (bodies join/cut may change)}";
  "cylinder" => "Cylinder primitive", "{plane?, at?, radius, height, operation?, targets?: [body] (bodies join/cut may change)}";
  "sphere" => "Sphere primitive", "{plane?, at?, radius, operation?, targets?: [body] (bodies join/cut may change)}";
  "torus" => "Torus primitive", "{plane?, at?, major, minor, operation?, targets?: [body] (bodies join/cut may change)}";
  "plane.offset" => "Construction plane offset from a plane or planar face; a plane can also be {origin: [x,y,z], normal, x?} anywhere a plane is taken", "{plane, offset?}";
  "plane" => "Construction plane by method: angle {axis, plane, angle}, midplane {a, b}, three_points {a, b, c}, two_edges {a, b}, tangent {face, near}, along_path {edge, t}, perpendicular {plane, through?: point, distance?, direction?: u|v} (normal along the plane's U or V direction)", "{method, ...}";
  "axis" => "Construction axis by method: face {face} (cylinder/cone/torus), normal {face, near}, two_planes {a, b}, two_points {a, b}, edge {edge} (a body edge {body, edge}|{point}, a sketch line {sketch, line} or {point} on it, or an axis {axis})", "{method, ...}";
  "point" => "Construction point by method: vertex {point}, two_edges {a, b}, three_planes {a, b, c}, center {face}, edge_plane {edge, plane}, along_path {edge, t}; a point is {at}|{vertex: edge, end?}|{center: edge}|{point: id}", "{method, ...}";
  "thread" => "Thread cylindrical faces: cosmetic, or modeled (a helical ISO groove); pitch defaults to ISO coarse", "{faces, pitch?, length?, offset?, modeled?, left?, flip?}";
  "coil" => "Helical coil around a plane's normal at a plane point", "{plane?, at?: [x,y], diameter, pitch, height | revolutions, section?: circular|square|triangle_out|triangle_in, section_size, left?, operation?}";
  "pipe" => "Tube along a path sketch: round, square or triangular, solid or hollow with a wall, along all or the first fraction of the path", "{path: sketch, path_curves?, diameter | size, section?: circular|square|triangular, wall?, distance?: 0..1, operation?}";
  "rib" => "Rib from sketch lines: thick across the sketch plane, grown to the body (or a depth)", "{sketch?, curves?, thickness, depth?, flip?}";
  "web" => "Web from sketch lines: thick in the sketch plane, grown along its normal to the body (or a depth)", "{sketch?, curves?, thickness, depth?, flip?}";
  "scale" => "Scale bodies about a point, uniformly or per axis", "{bodies?|component?, center?: [x,y,z], factor | factors: [x,y,z]}";
  "split_face" => "Split faces along a plane (the body stays one piece)", "{faces, tool: plane}";
  "remove_faces" => "Remove faces and heal the body (fillets, holes, bosses)", "{faces}";
  "thicken" => "Thicken faces or surface bodies into solids", "{faces? | bodies?, thickness, symmetric?: thickness on each side, flip?, operation?, corners?: \"sharp\"|\"round\"}";
  "surface.extrude" => "Surface body extruded from sketch curves", "{sketch?, curves?, distance, symmetric?, flip?}";
  "surface.revolve" => "Surface body revolved from sketch curves", "{sketch?, curves?, axis, angle?}";
  "surface.patch" => "Surface filling a closed boundary of body edges or a sketch", "{edges? | sketch?}";
  "surface.stitch" => "Sew surface bodies together (into a solid when they close)", "{bodies, tolerance?}";
  "surface.unstitch" => "Break bodies into one surface body per face", "{bodies}";
  "surface.reverse" => "Flip surface normals", "{bodies}";
  "surface.ruled" => "Ruled surface between two edges", "{a: edge, b: edge}";
  "surface.offset" => "New surfaces offset from faces or bodies; face_distances give faces distances of their own (variable offset)", "{faces? | bodies?, distance, face_distances?: [{face, distance}]}";
  "surface.trim" => "Trim surface bodies with a plane (keeps the side behind it), a body (keeps what's outside a solid or behind a surface) or a sketch projected onto them (cuts its profiles away, or keeps the right of its curves); flip keeps the other side, keep points pick the pieces to keep", "{bodies, tool: plane | {body} | {sketch, curves?}, flip?, keep?: [[x,y,z]]}";
  "mesh.reduce" => "Fewer triangles on mesh bodies", "{bodies, keep?: 0.05..1}";
  "mesh.smooth" => "Smooth mesh bodies", "{bodies, passes?}";
  "mesh.convert" => "Mesh bodies to clean B-rep (coplanar triangles merged into faces)", "{bodies}";
  "body.appearance" => "Set the appearance of bodies (a library name, [r,g,b] or \"#rrggbb\"; nothing clears it)", "{body?|bodies?|component?, appearance? | color?}";
  "appearances" => "The appearance library", "{}";
  "face.appearance" => "Set the appearance of single faces, over their body's (a library name, [r,g,b] or \"#rrggbb\"; nothing clears it)", "{faces: [{body, face} | {point}], appearance? | color?}";
  "cam.setup" => "New milling setup: stock around a body (or every body), work origin at the stock top's min X/Y corner; post-processor and units for its G-code", "{name?, body?, stock_side?, stock_top?, post?: fanuc|haas|grbl|linuxcnc|mach3, units?: mm|inch}";
  "cam.face" => "Facing operation: take the stock down to the part's top", "{setup?, tool_number? (library) | tool: {diameter, number?, kind?, rpm?, feed?, plunge?, ...} | tool_diameter, stepover?: fraction, stepdown?}";
  "cam.pocket" => "2D pocket: clear down to flat floor faces (holes in them stay as islands); rest: only what earlier operations left", "{setup?, floors: [face], tool_number? | tool | tool_diameter, stepover?, stepdown?, stock_to_leave?, rest?, entry?: plunge|ramp|helix, ramp_angle?, helix_diameter?}";
  "cam.contour" => "2D contour around the outline of faces (default the part's bottom), down through the part, with holding tabs if asked", "{setup?, faces?, inside?, tool_number? | tool | tool_diameter, stepdown?, stock_to_leave?, entry?: plunge|ramp, ramp_angle?, tabs?: count | {count, width?, height?}, tab_width?, tab_height?}";
  "cam.drill" => "Drill vertical holes (default every hole); peck > 0 pecks", "{setup?, holes?: [face], tool_number? | tool | tool_diameter, peck?}";
  "cam.generate" => "Compute a setup's toolpaths (shown in the viewport): moves, length and time", "{setup?}";
  "cam.post" => "Write a setup's G-code with its post-processor (default Fanuc, mm, G54); without a path, returns it", "{setup?, path?, post?: fanuc|haas|grbl|linuxcnc|mach3, units?: mm|inch}";
  "cam.list" => "Setups and their operations", "{}";
  "cam.delete" => "Delete a setup, or one of its operations", "{setup?, operation?}";
  "cam.adaptive" => "2D adaptive clearing down to flat floor faces: grows out from a helix by the step-over (optimal load), keeping the tool's engagement about constant", "{setup?, floors: [face], tool_number? | tool | tool_diameter, stepover?: fraction (default 0.1), stepdown?, stock_to_leave?, entry?: helix|plunge, ramp_angle?, helix_diameter?}";
  "cam.parallel" => "3D parallel finishing: rows across the part (or faces) with the tool dropped onto its surface", "{setup?, faces?, angle?: degrees, tool_number? | tool | tool_diameter, stepover?}";
  "cam.scallop" => "3D scallop finishing: rings a constant distance apart over the surface, from the outline of the faces in", "{setup?, faces?, tool_number? | tool | tool_diameter, stepover?}";
  "cam.simulate" => "Simulate the stock removal of a setup (shown in the viewport): removed volume, gouges, rapids into the stock, cuts deeper than the flutes", "{setup?}";
  "cam.tools" => "The hub's tool library (tools.json): flat, ball, bull-nose, drill and chamfer tools with feeds and speeds", "{}";
  "cam.tool.add" => "Add or change a tool in the library", "{number?, name?, kind?: flat|ball|bull_nose|drill|chamfer, diameter, corner_radius?, angle?, tip_diameter?, flute_length?, flutes?, rpm?, feed?, plunge?}";
  "cam.tool.delete" => "Remove a tool from the library", "{number}";
  "cam.posts" => "The post-processors G-code can be written for", "{}";
  "mirror" => "Mirror bodies across a plane", "{plane, bodies?, join?}";
  "pattern.rectangular" => "Rectangular pattern of bodies", "{direction: [x,y,z], count, spacing, direction2?, count2?, spacing2?, bodies?, join?}";
  "pattern.circular" => "Circular pattern of bodies", "{axis, count, angle?, symmetric?, bodies?, join?}";
  "move" => "Move/rotate bodies or a component", "{bodies? | component?, translate?: [x,y,z], axis?, angle?}";
  "combine" => "Join/cut/intersect bodies", "{target, tools: [id], operation, keep_tools?}";
  "body.remove" => "Remove bodies (as a timeline feature)", "{bodies: [id]}";
  "body.rename" => "Rename a body", "{body, name}";
  "body.visible" => "Show or hide a body", "{body, visible}";
  "body.topology" => "A body's faces and edges: surface/curve kind, area, normal, centre, endpoints, length and which faces meet at each edge", "{body, faces?: bool, edges?: bool}";
  "feature.edit" => "Change feature properties (fields merged into the feature)", "{feature, set: {...}}";
  "feature.delete" => "Delete a feature and the features that depend on it; a deleted sketch's features stay, built from its cached geometry with a warning, unless cascade", "{feature, cascade?: bool}";
  "feature.suppress" => "Suppress or unsuppress a feature", "{feature, suppressed?: bool}";
  "feature.rename" => "Rename a feature", "{feature, name}";
  "feature.move" => "Reorder a feature in the timeline (refused when it would come before what it references)", "{feature, to: index}";
  "timeline.marker" => "Roll the timeline back to after N features (null = end)", "{at: number|null}";
  "attribute.set" => "Attach a named value to the design, a body, component, feature or sketch curve/point", "{target?: \"design\" | {body} | {component} | {feature} | {sketch} | {sketch, curve} | {sketch, point}, group, name, value}";
  "attribute.get" => "Attributes on a target", "{target?, group?, name?}";
  "attribute.find" => "Attributes anywhere in the design, by group, name and/or value (only on things that still exist)", "{group?, name?, value?}";
  "attribute.delete" => "Remove attributes from a target (no name = the whole group)", "{target?, group, name?}";
  "view.render" => "Render the model to a PNG file: a standard view or yaw/pitch, ortho or perspective, fitted to the bodies shown; sketches and grid off unless asked", "{path, width?, height?, view?: home|top|bottom|front|back|left|right, yaw?, pitch?, fov?, ortho?, target?: [x,y,z], distance?, bodies?: [id] (only these), sketches?, grid?, edges?, background?: \"#rrggbb\"}";
  "view.named.save" => "Save a camera as a named view (an existing name is replaced)", "{name? (default Named View N), target: [x,y,z], yaw, pitch, distance, fov?, ortho?}";
  "view.named.list" => "The design's named views", "{}";
  "view.named.get" => "One named view's camera", "{name}";
  "view.named.rename" => "Rename a named view", "{name, to}";
  "view.named.delete" => "Delete a named view", "{name}";
  "select" => "Set the selection", "{items: [Sel]}";
  "selection" => "The current selection", "{}";
  "select.all" => "Select every body (or every face, edge, or curve of the edited sketch)", "{kind?: bodies|faces|edges|sketch}";
  "select.invert" => "Select what isn't selected (of the selected kind)", "{}";
  "measure" => "Measure one entity (properties) or the distance and angle between two; an entity is {body}, {body, face}, {body, edge} or {point, snap?: face|edge}; default the selection", "{a?, b?}";
  "inspect.center_of_mass" => "Centre of mass, volume and mass of bodies (default all visible)", "{bodies?}";
  "component.create" => "New component from bodies (taken out of other components)", "{name?, bodies: [id]}";
  "component.copy" => "Another instance of a component, rebuilt from it on every change", "{component, translate?: [x,y,z], name?}";
  "component.add_bodies" => "Move bodies into a component", "{component, bodies: [id]}";
  "component.delete" => "Delete a component (its bodies go back to the root)", "{component}";
  "component.rename" => "Rename a component", "{component, name}";
  "component.ground" => "Ground (fix) or unground a component", "{component, grounded?}";
  "component.material" => "Set a component's physical material (for mass and the BOM)", "{component, material}";
  "joint" => "Joint: mate a moving component's origin to a fixed one; motion stays free to drive", "{moving, moving_origin: {face}|{edge}|{at, z?, x?}, fixed?, fixed_origin, kind?: rigid|revolute|slider|cylindrical|pin_slot|planar|ball|screw, lead? (screw: advance per turn), angle?, offset?, flip?, as_built?, rotation?, slide?, slide2?, pitch?, limits?: {rotation?: [min, max], slide?: [min, max]}}";
  "joint.drive" => "Set a joint's motion values", "{joint, rotation?, slide?, slide2?, pitch?}";
  "motion.study" => "Step a joint through a range and report interference at each step", "{joint, motion?: rotation|slide|slide2|pitch, from, to, steps?}";
  "assembly.interference" => "Overlapping bodies of different components", "{}";
  "assembly.bom" => "Bill of materials: components, materials, volumes and masses; indented by sub-assembly level, top level only, or parts only", "{view?: indented|top|parts}";
  "drawing.create" => "New drawing with standard views (front, top, right, iso by default)", "{name?, views?: [front|back|top|bottom|left|right|iso | {section: x|y|z, offset, flip?} | {flat: body} | {detail: view, center: [x,y,z], radius, factor?}], sheet?: a4..a0|letter|tabloid, scale?: \"1:2\"|0.5|auto, projection?: third_angle|first_angle, bodies?, auto_dimensions?}";
  "drawing.set" => "Change a drawing's sheet, scale, projection or auto dimensions", "{drawing?, sheet?, scale?, projection?, auto_dimensions?, parts_list?, rename?, standard?: asme|iso, auto_balloons?}";
  "drawing.add_view" => "Add a view to a drawing", "{drawing?, kind, at?: [x,y] sheet mm, hidden_lines?, sheet?}";
  "drawing.remove_view" => "Remove a view", "{drawing?, view}";
  "drawing.move_view" => "Place a view (sheet mm; null = automatic), or move it to another sheet", "{drawing?, view, at, sheet?}";
  "drawing.dimension" => "Dimension two model points in a view", "{drawing?, view, a: [x,y,z], b: [x,y,z], orientation?: aligned|horizontal|vertical, offset?}";
  "drawing.note" => "Text note on the sheet", "{drawing?, at: [x,y], text, sheet?}";
  "drawing.title" => "Fill in the title block", "{drawing?, title?, author?, part_number?, material?, revision?, company?}";
  "drawing.info" => "A drawing's views, scale and text", "{drawing?}";
  "drawing.list" => "List drawings", "{}";
  "drawing.delete" => "Delete a drawing", "{drawing?}";
  "drawing.export" => "Write a drawing as .svg, .pdf or .dxf: every sheet (PDF pages, DXF side by side, name-N.svg files) or one", "{drawing?, path, sheet?}";
  "drawing.break" => "Broken view: leave a stretch of a view out (model mm along the view's horizontal or vertical); dimensions keep the true length", "{drawing?, view, from, to, direction?: horizontal|vertical, gap?: sheet mm}";
  "drawing.remove_breaks" => "Remove a view's breaks", "{drawing?, view}";
  "drawing.ordinate" => "Ordinate dimensions: distances of points from an origin along the view's horizontal or vertical", "{drawing?, view?, origin: [x,y,z], points: [[x,y,z], ...] | \"x y z; ...\", orientation?: horizontal|vertical, offset?}";
  "drawing.angle" => "Angular dimension at a vertex between the edges to two points", "{drawing?, view?, vertex: [x,y,z], a: [x,y,z], b: [x,y,z], radius?: sheet mm}";
  "drawing.feature_control" => "GD&T feature control frame on a model point (per the drawing's standard)", "{drawing?, view?, point: [x,y,z], offset?: [x,y] sheet mm, characteristic: straightness|flatness|circularity|cylindricity|profile_of_line|profile_of_surface|angularity|perpendicularity|parallelism|position|concentricity|symmetry|circular_runout|total_runout, tolerance, diameter?, modifier?: none|mmc|lmc, datums?: [\"A\", ...], lower?: {tolerance, diameter?, modifier?, datums?} (composite frame)}";
  "drawing.datum" => "Datum feature identifier on a model point (next free letter by default)", "{drawing?, view?, point: [x,y,z], offset?: [x,y], label?}";
  "drawing.surface_finish" => "Surface texture symbol (Ra, µm) on a model point", "{drawing?, view?, point: [x,y,z], offset?: [x,y], roughness, process?: any|removal|no_removal}";
  "drawing.balloon" => "Balloon with a component's parts list item number (follows the parts list)", "{drawing?, view?, component: id|name, point?: [x,y,z], offset?: [x,y]}";
  "drawing.remove_dimension" => "Remove a dimension, note or annotation by index", "{drawing?, index}";
  "sheet.rule" => "Add or change a sheet metal rule (the first rule is the default)", "{name?, thickness?, bend_radius?, k_factor?, relief?: rectangular|round|none, relief_width?, relief_depth?, gap?, corner_relief?: none|round|square|tear, corner_relief_size?}";
  "sheet.rules" => "List the sheet metal rules", "{}";
  "sheet.rule_remove" => "Delete an unused sheet metal rule", "{name}";
  "sheet.flange" => "Sheet metal flange from closed sketch profiles (a flat sheet of the rule's thickness)", "{sketch?, profiles?, rule?, flip?, operation?: new_body|join, targets?: [body] (bodies join/cut may change)}";
  "sheet.contour_flange" => "Sheet metal from an open sketch path: corners become bends, extruded by distance", "{sketch?, curves?: [id], distance, symmetric?, flip?, flip_direction?, rule?}";
  "sheet.edge_flange" => "Bend a flange up from straight sheet edges", "{edges: [{body, edge} | {point}], height, angle?, flip?, position?: adjacent|inside|outside, offset_start?, offset_end?, corners?: miter|open|closed|overlap|butt}";
  "sheet.hem" => "Fold sheet edges back 180° (flat hem)", "{edges: [{body, edge} | {point}], length, gap?, flip?}";
  "sheet.bend" => "Bend a flat sheet along a sketch line on one of its faces", "{sketch?, line: curve id, angle, flip?, flip_side?}";
  "sheet.convert" => "Turn a thin solid into sheet metal (thickness measured from the body); sharp corners become bends", "{body, face?: {body, face} | {point}, rule?, bends?}";
  "sheet.rip" => "Slit a solid along edges (e.g. a shelled box's corners) so it can unfold", "{edges: [{body, edge} | {point}], gap?}";
  "sheet.refold" => "Fold an unfolded sheet back up, carrying cuts made while flat", "{body?}";
  "sheet.unfold" => "Flatten a sheet metal body onto its stationary face (timeline feature)", "{body?, face?, keep_folded?}";
  "sheet.flat_pattern" => "Flat pattern of a sheet metal body: size and bends; writes .dxf/.step/.stl when given a path", "{body?, face?, path?}";
  "sheet.check" => "Check a sheet metal body's flat pattern against the rules shops cut by: hole sizes, material between holes and edges, holes near bends, and the sheet size; defaults follow the body's thickness and bend radius", "{body?, face?, min_hole?, min_web?, bend_clearance?, sheet?: [width, height]}";
  "emboss" => "Raise (or with deboss, sink) sketch profiles or text on a face, projected or wrapped around a cylinder", "{sketch?, profiles?, face, depth, deboss?, wrap?, horizontal?, vertical? (move the profiles along the sketch x / the face's up), rotation?: deg (about the profiles' centre)}";
  "deboss" => "Sink sketch profiles or text into a face (emboss with deboss: true)", "{sketch?, profiles?, face, depth, wrap?, horizontal?, vertical?, rotation?: deg}";
  "boundary_fill" => "Solids from the closed cells bodies and planes bound; cells are points inside the cells to keep (default all)", "{tools: [body], planes?: [plane], cells?: [[x,y,z]], operation?, remove_tools?}";
  "boundary_fill.cells" => "The cells a boundary fill would offer: a point inside each and its volume", "{tools: [body], planes?: [plane]}";
  "replace_face" => "Replace flat faces with a surface (a face of another body) or a plane; neighbours extend or trim", "{faces: [face], target?: face, plane?}";
  "surface.extend" => "Extend open surface edges: natural (the surface carries on), tangent or perpendicular", "{edges, distance, mode?: natural|tangent|perpendicular}";
  "surface.untrim" => "Untrim faces: remove holes (inner), restore the untrimmed outer boundary (outer) or both (all)", "{faces? | bodies?, loops?: all|inner|outer}";
  "sheet.lofted_flange" => "Sheet metal between two profiles (closed profiles are opened by the rule's gap); die-formed (smooth) or press-brake (facets and bends)", "{from: sketch | {sketch, curves?}, to: sketch | {sketch, curves?}, method?: die_formed|press_brake, bends?, flip?, rule?, rounded?: bool (press brake: round the bends)}";
  "sheet.miter_flange" => "Sweep an open sketch profile (drawn across the first edge, starting on it) along a chain of sheet edges with mitred corners", "{edges: [{body, edge} | {point}], sketch?, curves?: [id], corners?: miter|open|closed|overlap|butt}";
  "sheet.flat_sketch" => "The flat pattern as an editable sketch (outline, bend lines as construction lines) on the stationary face, or on XY at the origin", "{body?, face?, on?: face|xy}";
  "sheet.corner_closure" => "How an edge or miter flange (default the last) closes its corners, and the rule's corner relief", "{feature?, corners?: miter|open|closed|overlap|butt, relief?: none|round|square|tear, relief_size?, rule?}";
  "joint_origin" => "Save a joint origin on a component (joints use it as {origin: id|name}); moves with the component", "{component?, face?|edge?|at?: [x,y,z], z?, x?, angle?, offset?, offset_x?, offset_y?, flip?, name?}";
  "rigid_group" => "Make components move as one: a joint moving any of them moves them all", "{components?: [id], bodies?: [id], name?}";
  "motion_link" => "Drive one joint from another: when the leader moves leader_value, the follower moves follower_value", "{leader, follower, leader_motion?: rotation|slide|slide2|pitch, follower_motion?, leader_value?, follower_value?, reverse?, name?}";
  "contact_set.create" => "Contact set: components that can't pass through each other; driving a joint stops at contact", "{components?: [id], bodies?: [id], name?}";
  "contact_set.enable" => "Turn a contact set on or off", "{contact_set, enabled?}";
  "contact_set.delete" => "Delete a contact set", "{contact_set}";
  "contact_set.list" => "List the contact sets", "{}";
  "component.insert" => "Insert a hub design as a component; linked (default) it can be updated to the source's newest version", "{item, version?, name?, translate?: [x,y,z], link?: bool}";
  "component.links" => "Inserted components: source item, version and whether a newer version is in the hub", "{}";
  "component.update_link" => "Update linked components (one or all) to their source's newest version", "{component?}";
  "component.break_link" => "Break an inserted component's link to its source (keeps the bodies as they are)", "{component}";
  "sketch.text_edit" => "Change a text's string, height, angle, font or position (its glyphs regenerate)", "{text_id? | curve?, text?, height?, angle?: deg, font?, bold?, italic?, underline?, halign?, valign?, spacing?, at?: [x,y]}";
  "sketch.text_explode" => "Turn a text into plain curves", "{text_id? | curve?}";
  "sketch.break" => "Split a curve without removing anything: at its crossings around a point, or at the point", "{curve?, at: [x,y], at_point?, snap?}";
  "sketch.tangent_circle" => "Circle tangent to two curves (with a radius) or three; the one nearest 'near'", "{curves: [id, id, id?], radius?, near?: [x,y]}";
  "sketch.blend" => "Blend curve: Fusion's spline joining the ends of two curves, tangent (g1, the default) or curvature continuous (g2)", "{curves: [id, id], near?: [[x,y], [x,y]], continuity?: g1|g2}";
  "config.list" => "Configurations (named variants), the active one and whether design history is captured", "{}";
  "config.create" => "Add a configuration, empty or copying another's overrides", "{name, copy?, activate?: bool}";
  "config.delete" => "Delete a configuration", "{name}";
  "config.rename" => "Rename a configuration", "{name, new_name}";
  "config.activate" => "Make a configuration active (no name: the design as modelled)", "{name?}";
  "config.set" => "Set one cell of a configuration; a null value clears it back to the design's own", "{configuration, parameter, expr} | {configuration, feature, suppressed} | {configuration, body, appearance? | color?} | {configuration, joint, motion?: rotation|slide|slide2|pitch, value}";
  "config.table" => "The configuration table: a row per configuration, a column per overridden parameter, feature, appearance or joint", "{}";
  "design.history" => "Capture design history on or off; turning it off collapses the timeline into a base feature", "{capture: bool}";
  "base.start" => "Add an empty base feature at the marker: direct edits go into it", "{}";
  "base.edit" => "Edit a base feature: roll the marker back to just after it", "{feature}";
  "base.finish" => "Finish editing a base feature: move the marker back to the end", "{}";
  "direct.move_face" => "Move flat faces by a vector (direct edit in a base feature, not parametric)", "{faces: [{body, face} | {point}], translate: [x,y,z]}";
  "direct.press_pull" => "Push (−) or pull (+) faces by a fixed distance (direct edit in a base feature)", "{faces: [{body, face} | {point}], distance}";
  "direct.delete_face" => "Delete faces and heal the body (direct edit in a base feature)", "{faces: [{body, face} | {point}]}";
  "hub.comment" => "Comment on a version (default: the open one, else the latest)", "{item?, version?, text}";
  "hub.comments" => "Comments on an item's versions", "{item?, version?}";
  "hub.comment.delete" => "Delete a comment", "{item?, comment}";
  "hub.diff" => "Show two versions in the viewport: added geometry green, removed red, unchanged ghosted", "{item?, a, b?, clear?: bool}";
  "sheet.check_dxf" => "Check a flat pattern DXF (a file, or a sheet metal body's flat pattern) against shop rules: closed contours, no duplicates, minimum feature size, holes clear of bends", "{path? | body?, face?, min_feature?, hole_to_bend?}";
  "sketch.fonts" => "Fonts sketch text can use: built in, and font files in the system's font folders (FOX_FONT_DIR first); a text's font may also be a font file's path", "{}";
  "sketch.elliptical_arc" => "Elliptical arc: the ellipse from its centre, major axis end and minor radius, counter-clockwise from the point nearest start to the one nearest end", "{center, major: [x,y], minor, start: [x,y], end: [x,y]}";
  "cam_follower" => "Cam motion: a joint's slide keeps its follower in contact with a cam component as the cam moves", "{cam: component | bodies: [body], follower: joint id or name, flat?: bool, clearance?, name?}";
  "component.flexible" => "Solve an inserted sub-assembly's own joints in this design (flexible) or keep the source's positions (rigid)", "{component, flexible?: bool}";
  "component.sub_joints" => "The joints inside an inserted sub-assembly, with their motions", "{component}";
  "component.drive" => "Drive a joint inside a flexible inserted sub-assembly", "{component, joint: name, rotation?, slide?, slide2?, pitch?}";
  "component.parent" => "Move a component into another one (a sub-assembly), or back to the top level", "{component, parent?: component | null}";
  "drawing.add_sheet" => "Add a sheet to a drawing (its own size and scale, or the drawing's)", "{drawing?, name?, size?: a4..a0|letter|tabloid, scale?: \"1:2\"|0.5|auto}";
  "drawing.set_sheet" => "Rename a sheet or change its size or scale", "{drawing?, sheet, name?, size?, scale?}";
  "drawing.remove_sheet" => "Delete a sheet with its views and notes (a drawing keeps one)", "{drawing?, sheet}";
  "drawing.baseline" => "Baseline dimensions: points measured from one origin along the view's horizontal or vertical, stacked", "{drawing?, view?, origin: [x,y,z], points: [[x,y,z], ...] | \"x y z; ...\", orientation?: horizontal|vertical, offset?, spacing?: sheet mm}";
  "drawing.chain" => "Chain dimensions: consecutive distances between points on one line", "{drawing?, view?, points: [[x,y,z], ...] | \"x y z; ...\", orientation?: horizontal|vertical, offset?}";
  "drawing.weld" => "Welding symbol on a joint (AWS A2.4 or ISO 2553 per the drawing's standard)", "{drawing?, view?, point: [x,y,z], kind: fillet|square|v|bevel|u|j|plug|spot|seam, side?: arrow|other|both, size?, length?: \"50\"|\"50-150\", all_around?, field?, tail?, offset?: [x,y]}";
  "drawing.datum_target" => "Datum target (A1, B2, ...) on a model point; area > 0 for an area target", "{drawing?, view?, point: [x,y,z], label, area?: model mm, offset?: [x,y]}";
  "drawing.revision" => "Add a row to the revision table (next letter by default)", "{drawing?, rev?, description?, date?, by?}";
  "drawing.remove_revision" => "Remove a revision table row", "{drawing?, index}";
  "cam.adaptive3d" => "3D adaptive roughing: clear the stock around the part level by level, growing in from the open side (or a helix) by the step-over so the engagement stays about constant; flat areas get a level of their own", "{setup?, tool_number? | tool | tool_diameter, stepover?: fraction (default 0.15), stepdown?, stock_to_leave? (default 0.3), entry?: helix|plunge, ramp_angle?, helix_diameter?}";
  "cam.pencil" => "3D pencil finishing: a pass along each concave corner or fillet tighter than the tool", "{setup?, faces?, tool_number? | tool | tool_diameter, tool_kind?}";
  "cam.steep_shallow" => "3D steep and shallow finishing: waterline passes a step-down apart on areas steeper than the threshold, parallel rows a step-over apart on the rest", "{setup?, faces?, threshold?: degrees (default 45), tool_number? | tool | tool_diameter, stepover?, stepdown?}";
  "cam.contour3d" => "3D contour (waterline) finishing: passes at constant heights a step-down apart on areas at least min_slope steep", "{setup?, faces?, min_slope?: degrees, tool_number? | tool | tool_diameter, stepdown?}";
  "cut.machines" => "Laser, plasma and waterjet machine profiles (built in, and the hub's machines.json) with their processes", "{}";
  "cut.machine.add" => "Add or replace a machine profile in the hub: G-code templates and any number of processes", "{machine: {id, name, kind?: laser|plasma|waterjet|dxf, extension?, arcs?, header?, pierce?, off?, footer?, rapid?, line?, arc_cw?, arc_ccw?, max_power?, rapid_rate?, processes?: [{name, kerf?, feed?, power?, pierce_delay?, ...}]}}";
  "cut.machine.delete" => "Delete a machine profile from the hub", "{id}";
  "cut.contours" => "Closed contours a 2D cut would follow: outlines and holes of a flat pattern (default), flat faces or a sketch", "{body?, face?, faces?: [face], sketch?, origin?: corner|model}";
  "cut.gcode" => "2D cut for a laser, plasma or waterjet: kerf offset to the scrap side with true arcs, leads in the scrap, holes before outlines, nearest-first order, micro-joints; G-code or R12 DXF; without a path, returns it", "{body?, face?, faces?: [face], sketch?, machine?: grbl_laser|fluidnc_laser|qtplasmac|plasma_z|wazer|dxf|id|{profile}, process?: name|index, kerf?, feed?, power?: %, pierce_delay?: s, pierce_height?, cut_height?, safe_height?, lead_in?: arc|line|none|length|{kind, length}, lead_out?, overcut?, tabs?: count, tab_width?, tabs_on_holes?, compensate?, part_on_right?, small_hole_diameter?, small_hole_feed?, material?, units?: mm|inch, origin?: corner|model, name?, path?}";
  "sketch.include" => "Include 3D Geometry: body edges and earlier sketches' curves at their real positions; off-plane ones stay 3D (sweep paths, rails), in-plane ones form profiles; they follow their source", "{sketch?, edges?: [{body, edge} | {point}], curves?: [{sketch, curve}], associative?}";
  "form.box" => "Create Form with a box: a T-spline body, the smooth limit surface of a box control cage (centred on 'at', standing on the plane) with the given faces per side", "{plane?, at?: [x, y], length, width, height, length_faces?, width_faces?, height_faces? (default 2), operation?}";
  "form.cage" => "Create Form from a control cage: vertices (plane coordinates) and closed quad faces, counter-clockwise seen from outside; the body is the cage's smooth limit surface", "{plane?, vertices: [[x, y, z]], faces: [[a, b, c, d]], operation?}";
  "form.move_vertex" => "Pull a form's control vertex (by index, or the one nearest a point) to a position or by an offset, in the form's plane coordinates", "{feature? (default the last form), vertex? | point?: [x, y, z], to?: [x, y, z] | by?: [dx, dy, dz]}";
}

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  if let Some(r) = crate::assembly::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::drawings::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::views::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::inspect::run(s, name, p) {
    return r;
  }
  // before features: it takes surface.trim with a body or sketch tool
  if let Some(r) = crate::surfaces::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::features::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::cam::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::solid::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::sheet2::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::config::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::cut::run(s, name, p) {
    return r;
  }
  if let Some(r) = crate::form::run(s, name, p) {
    return r;
  }
  match name {
    "help" => {
      Ok(json!(COMMANDS.iter().map(|c| json!({"id": c.id, "help": c.help, "params": c.params})).collect::<Vec<_>>()))
    }
    "info" => Ok(info(s)),
    "new" => {
      let root = s.store_root.take();
      *s = Session::new(Design::new(opt_str(p, "name").unwrap_or("Untitled")));
      s.store_root = root;
      Ok(json!({}))
    }
    "open" => {
      let path = PathBuf::from(req_str(p, "path")?);
      let root = s.store_root.take();
      let opened = Session::open(&path);
      s.store_root = root.clone();
      *s = opened?;
      s.store_root = root;
      Ok(json!({"features": s.design.features.len(), "bodies": s.model.bodies.len()}))
    }
    "save" => {
      let path = match opt_str(p, "path") {
        Some(p) => PathBuf::from(p),
        None => s.path.clone().ok_or(CmdError::new("no path: pass {\"path\": ...}"))?,
      };
      s.save_to(&path)?;
      Ok(json!({"path": path}))
    }
    "export" => crate::files::export(s, p),
    "import" => {
      let path = PathBuf::from(req_str(p, "path")?);
      crate::files::import(s, &path, p)
    }
    "formats" => Ok(json!(fox_exchange::FORMATS)),
    "inspect" => crate::files::inspect(&PathBuf::from(req_str(p, "path")?)),
    n if n.starts_with("hub.") => crate::hub::run(s, n, p),
    "compute_all" => {
      s.regen.clear();
      s.rebuild();
      Ok(json!({"errors": s.model.errors()}))
    }
    "undo" => Ok(json!({"done": s.undo()})),
    "redo" => Ok(json!({"done": s.redo()})),
    "transaction.begin" => {
      s.begin_transaction()?;
      Ok(json!({}))
    }
    "transaction.commit" => Ok(json!({"changed": s.commit_transaction()?})),
    "transaction.rollback" => Ok(json!({"changed": s.rollback_transaction()?})),
    "param.add" => {
      let (name, expr) = (req_str(p, "name")?.to_string(), req_expr(p, "expr")?);
      let kind = param_kind(opt_str(p, "kind"))?;
      let comment = opt_str(p, "comment").unwrap_or("").to_string();
      fox_expr::eval(&expr, kind, &params_scope(s))?;
      s.edit(|d| Ok(d.add_parameter(&name, &expr, kind, &comment)?))?;
      Ok(json!({"value": s.model.params.get(&name)}))
    }
    "param.set" => {
      let (name, expr) = (req_str(p, "name")?.to_string(), req_expr(p, "expr")?);
      s.edit(|d| Ok(d.set_parameter(&name, &expr)?))?;
      if let Some(e) = s.model.param_errors.get(&name).cloned() {
        s.undo();
        s.redo.clear();
        return Err(CmdError::new(format!("{name}: {e}")));
      }
      Ok(json!({"value": s.model.params.get(&name)}))
    }
    "param.rename" => {
      let (a, b) = (req_str(p, "name")?.to_string(), req_str(p, "to")?.to_string());
      s.edit(|d| Ok(d.rename_parameter(&a, &b)?))?;
      Ok(json!({}))
    }
    "param.remove" => {
      let name = req_str(p, "name")?.to_string();
      s.edit(|d| Ok(d.remove_parameter(&name)?))?;
      Ok(json!({}))
    }
    "sketch.create" => {
      let plane = plane_ref(s, p.get("plane").unwrap_or(&json!("xy")))?;
      let id =
        s.edit(|d| Ok(d.add_feature(FeatureKind::Sketch { plane, sketch: Sketch::new(), links: Vec::new() })?))?;
      s.edit_sketch = Some(id);
      Ok(json!({"sketch": id.0}))
    }
    "sketch.edit" => {
      let id = FeatureId(req_u32(p, "sketch")?);
      s.design.sketch(id)?;
      s.edit_sketch = Some(id);
      Ok(json!({}))
    }
    "sketch.finish" => {
      s.edit_sketch = None;
      Ok(json!({}))
    }
    "sketch.line"
    | "sketch.rect"
    | "sketch.circle"
    | "sketch.arc"
    | "sketch.polygon"
    | "sketch.slot"
    | "sketch.point"
    | "sketch.spline"
    | "sketch.control_spline"
    | "sketch.ellipse"
    | "sketch.conic" => draw(s, name, p),
    "sketch.constrain" => constrain(s, p),
    "sketch.dimension" => dimension(s, p),
    "sketch.set_dimension" => {
      let sk = sketch_id(s, p)?;
      let c = ConstraintId(req_u32(p, "constraint")?);
      let expr = req_expr(p, "value")?;
      let angle = s.design.sketch(sk)?.constraint(c).ok_or(CmdError::new("no such constraint"))?.kind.is_angle();
      let kind = if angle { fox_expr::Kind::Angle } else { fox_expr::Kind::Length };
      let v = fox_expr::eval(&expr, kind, &params_scope(s))?;
      s.edit(|d| {
        let sketch = d.sketch_mut(sk)?;
        sketch.set_dim(c, Dim::expr(expr, v))?;
        let r = sketch.solve();
        if r.ok() { Ok(()) } else { Err(CmdError::new("that value conflicts with other constraints")) }
      })?;
      Ok(json!({"value": v}))
    }
    "sketch.delete" => {
      let sk = sketch_id(s, p)?;
      let curves = ids(p, "curves").into_iter().map(CurveId).collect::<Vec<_>>();
      let points = ids(p, "points").into_iter().map(PointId).collect::<Vec<_>>();
      let cons = ids(p, "constraints").into_iter().map(ConstraintId).collect::<Vec<_>>();
      s.edit(|d| {
        let sketch = d.sketch_mut(sk)?;
        for c in cons {
          sketch.remove_constraint(c)?;
        }
        for c in curves {
          sketch.remove_curve(c)?;
        }
        for pt in points {
          sketch.remove_point(pt)?;
        }
        Ok(())
      })?;
      Ok(json!({}))
    }
    "sketch.construction" => {
      let sk = sketch_id(s, p)?;
      let curves = ids(p, "curves");
      let on = p.get("on").and_then(Value::as_bool);
      s.edit(|d| {
        let sketch = d.sketch_mut(sk)?;
        for c in curves {
          let cur = sketch.curve(CurveId(c)).map(|c| c.construction).unwrap_or(false);
          sketch.set_construction(CurveId(c), on.unwrap_or(!cur))?;
        }
        Ok(())
      })?;
      Ok(json!({}))
    }
    "sketch.drag" => {
      let sk = sketch_id(s, p)?;
      let pt = PointId(req_u32(p, "point")?);
      let to = req_pt(p, "to")?;
      let r = s.edit(|d| {
        let r = d.sketch_mut(sk)?.drag(pt, to);
        if r.ok() { Ok(r) } else { Err(CmdError::new("the point can't move there")) }
      })?;
      Ok(json!({"dof": r.dof}))
    }
    "sketch.info" => sketch_info(s, p),
    "sketch.transform" => {
      let sk = sketch_id(s, p)?;
      let t = xform(s, p)?;
      let mut curves: Vec<CurveId> = ids(p, "curves").into_iter().map(CurveId).collect();
      if curves.is_empty() {
        curves = s.design.sketch(sk)?.curves.iter().map(|c| c.id).collect();
      }
      let r = s.edit(|d| {
        let sketch = d.sketch_mut(sk)?;
        let r = sketch.transform(&curves, &t)?;
        if sketch.solve().ok() {
          Ok(r)
        } else {
          Err(CmdError::new("the moved geometry conflicts with its constraints"))
        }
      })?;
      Ok(json!({
        "removed_constraints": r.removed.iter().map(|c| c.0).collect::<Vec<_>>(),
        "unlinked_dimensions": r.unlinked.iter().map(|c| c.0).collect::<Vec<_>>(),
      }))
    }
    "sketch.insert" => {
      let sk = sketch_id(s, p)?;
      let t = xform(s, p)?;
      let from = p.get("from").ok_or(CmdError::new("missing 'from'"))?;
      let (src, warning) = insert_source(s, from)?;
      let curves: Vec<CurveId> = ids(p, "curves").into_iter().map(CurveId).collect();
      let added = s.edit(|d| Ok(d.sketch_mut(sk)?.insert_from(&src, &curves, &t)?))?;
      Ok(json!({"curves": added.iter().map(|c| c.0).collect::<Vec<_>>(), "warning": warning}))
    }
    "sketch.trace" => {
      let sk = sketch_id(s, p)?;
      let at = opt_pt(p, "at")?.unwrap_or([0.0, 0.0]);
      let angle = opt_expr(p, "angle")
        .map(|e| fox_expr::eval(&e, fox_expr::Kind::Angle, &params_scope(s)))
        .transpose()?
        .unwrap_or(0.0);
      let (src, mut out, placed) = crate::trace::sketch(p, at, angle, &params_scope(s))?;
      let t = fox_sketch::Xform2 { angle, center: at, ..Default::default() };
      let added = s.edit(|d| Ok(d.sketch_mut(sk)?.insert_from(&src, &[], &t)?))?;
      out["curves"] = json!(added.iter().map(|c| c.0).collect::<Vec<_>>());
      let profiles = s.model.sketches.get(&sk).map(|r| r.profiles.as_slice()).unwrap_or_default();
      let ink = crate::trace::ink_profiles(&placed, profiles);
      out["ink"] = json!(ink.iter().map(|i| profiles[*i].interior_point()).collect::<Vec<_>>());
      out["ink_profiles"] = json!(ink);
      out["profiles"] = json!(profiles.len());
      out["at"] = json!(at);
      out["angle"] = json!(angle);
      Ok(out)
    }
    "trace.info" => crate::trace::info(p, &params_scope(s)),
    "sketch.fillet"
    | "sketch.chamfer"
    | "sketch.offset"
    | "sketch.trim"
    | "sketch.extend"
    | "sketch.mirror"
    | "sketch.pattern_rect"
    | "sketch.pattern_circ" => modify(s, name, p),
    "sketch.project" => project(s, p),
    "extrude" => {
      let sk = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sk, p)?;
      let kind = match opt_str(p, "extent").unwrap_or("one_side") {
        "one_side" | "distance" => ExtentKind::OneSide,
        "symmetric" => ExtentKind::Symmetric,
        "two_sides" => ExtentKind::TwoSides,
        "through_all" | "all" => ExtentKind::ThroughAll,
        "through_all_both" | "all_both" => ExtentKind::ThroughAllBoth,
        "to_face" | "to" => ExtentKind::ToFace,
        "to_body" | "to_object" => ExtentKind::ToBody,
        other => return Err(CmdError::new(format!("unknown extent '{other}'"))),
      };
      let distance = match kind {
        ExtentKind::ThroughAll | ExtentKind::ThroughAllBoth => "0".to_string(),
        ExtentKind::ToFace | ExtentKind::ToBody => opt_expr(p, "distance").unwrap_or_else(|| "0".into()),
        _ => req_expr(p, "distance")?,
      };
      let to = match (kind, p.get("to")) {
        (ExtentKind::ToFace, Some(f)) => Some(face_ref(s, f)?),
        (ExtentKind::ToFace, None) => return Err(CmdError::new("to_face needs 'to': a face")),
        (ExtentKind::ToBody, Some(b)) => Some(crate::solid::body_face_ref(s, b)?),
        (ExtentKind::ToBody, None) => return Err(CmdError::new("to_body needs 'to': a body")),
        _ => None,
      };
      let thin_side = match opt_str(p, "thin_side").unwrap_or("inside") {
        "inside" => ThinSide::Inside,
        "outside" => ThinSide::Outside,
        "center" => ThinSide::Center,
        o => return Err(CmdError::new(format!("unknown thin side '{o}' (inside, outside, center)"))),
      };
      let extent = Extent {
        kind,
        distance,
        distance2: opt_expr(p, "distance2").unwrap_or_else(|| "0".into()),
        flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
        taper: opt_expr(p, "taper").unwrap_or_default(),
        taper2: opt_expr(p, "taper2").unwrap_or_default(),
        thin: opt_expr(p, "thin").unwrap_or_default(),
        thin_side,
        to,
        start: opt_expr(p, "start").unwrap_or_default(),
        start_face: p.get("start_face").map(|f| face_ref(s, f)).transpose()?,
      };
      let operation = operation(p)?;
      add_op_feature(s, FeatureKind::Extrude { sketch: sk, profiles, extent, operation }, p)
    }
    "revolve" => {
      let sk = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sk, p)?;
      let axis = axis_ref(s, p.get("axis").ok_or(CmdError::new("missing 'axis'"))?)?;
      let angle = opt_expr(p, "angle").unwrap_or_else(|| "360 deg".into());
      add_op_feature(s, FeatureKind::Revolve { sketch: sk, profiles, axis, angle, operation: operation(p)? }, p)
    }
    "loft" => {
      let list = p.get("sections").and_then(Value::as_array).ok_or(CmdError::new("missing 'sections'"))?;
      let mut sections = Vec::new();
      for v in list {
        let (sketch, profile) = match v.as_u64() {
          Some(id) => (id, None),
          None => (
            v.get("sketch")
              .and_then(Value::as_u64)
              .ok_or(CmdError::new("a section is a sketch id or {sketch, profile?}"))?,
            opt_pt(v, "profile")?,
          ),
        };
        let sketch = FeatureId(u32::try_from(sketch).map_err(|_| CmdError::new("bad sketch id"))?);
        s.design.sketch(sketch)?;
        sections.push(LoftSection { sketch, profile });
      }
      s.edit_sketch = None;
      let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
      let mut rails = Vec::new();
      for v in p.get("rails").and_then(Value::as_array).into_iter().flatten() {
        rails.push(crate::solid::path_ref(s, v)?);
      }
      let centerline = p.get("centerline").map(|v| crate::solid::path_ref(s, v)).transpose()?;
      add_op_feature(
        s,
        FeatureKind::Loft {
          sections,
          ruled: flag("ruled"),
          closed: flag("closed"),
          operation: operation(p)?,
          rails,
          centerline,
        },
        p,
      )
    }
    "sweep" => {
      let path_sketch = FeatureId(req_u32(p, "path")?);
      s.design.sketch(path_sketch)?;
      let sk = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sk, p)?;
      let path_curves = ids(p, "path_curves").into_iter().map(CurveId).collect();
      let rail = p.get("rail").map(|v| crate::solid::path_ref(s, v)).transpose()?;
      add_op_feature(
        s,
        FeatureKind::Sweep {
          sketch: sk,
          profiles,
          path_sketch,
          path_curves,
          operation: operation(p)?,
          rail,
          twist: opt_expr(p, "twist").unwrap_or_default(),
        },
        p,
      )
    }
    "hole" => {
      let sk = sketch_for_feature(s, p)?;
      let kind = match opt_str(p, "kind").unwrap_or("simple") {
        "simple" => HoleKind::Simple,
        "counterbore" => HoleKind::Counterbore,
        "countersink" => HoleKind::Countersink,
        o => return Err(CmdError::new(format!("unknown hole kind '{o}' (simple, counterbore, countersink)"))),
      };
      let through_all = p.get("through_all").and_then(Value::as_bool).unwrap_or(false);
      add_feature(
        s,
        FeatureKind::Hole {
          sketch: sk,
          points: ids(p, "points").into_iter().map(PointId).collect(),
          kind,
          diameter: req_expr(p, "diameter")?,
          depth: if through_all { "0".into() } else { req_expr(p, "depth")? },
          through_all,
          tip_angle: opt_expr(p, "tip_angle").unwrap_or_else(|| "118 deg".into()),
          head_diameter: opt_expr(p, "head_diameter").unwrap_or_else(|| "0".into()),
          head_depth: opt_expr(p, "head_depth").unwrap_or_else(|| "0".into()),
          head_angle: opt_expr(p, "head_angle").unwrap_or_else(|| "90 deg".into()),
          flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
        },
      )
    }
    "split" => {
      let plane = plane_ref(s, p.get("plane").ok_or(CmdError::new("missing 'plane'"))?)?;
      add_feature(s, FeatureKind::SplitBody { bodies: body_sel(p), plane })
    }
    "draft" => {
      let mut faces = Vec::new();
      for f in p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))? {
        faces.push(face_ref(s, f)?);
      }
      let plane = plane_ref(s, p.get("plane").ok_or(CmdError::new("missing 'plane'"))?)?;
      let flip = p.get("flip").and_then(Value::as_bool).unwrap_or(false);
      let symmetric = p.get("symmetric").and_then(Value::as_bool).unwrap_or(false);
      let angle2 = opt_expr(p, "angle2");
      add_feature(s, FeatureKind::Draft { faces, plane, angle: req_expr(p, "angle")?, flip, symmetric, angle2 })
    }
    "fillet" => {
      let (edges, radius, sets) = crate::fillet::selection_sets(s, p)?;
      add_feature(
        s,
        FeatureKind::Fillet {
          edges,
          radius,
          radius2: opt_expr(p, "radius2"),
          setback: opt_expr(p, "setback"),
          chord: p.get("chord").and_then(Value::as_bool).unwrap_or(false),
          radii: crate::fillet::radius_points(p)?,
          sets,
        },
      )
    }
    "chamfer" => {
      let edges = edge_refs(s, p)?;
      add_feature(
        s,
        FeatureKind::Chamfer {
          edges,
          distance: req_expr(p, "distance")?,
          distance2: opt_expr(p, "distance2"),
          angle: opt_expr(p, "angle"),
        },
      )
    }
    "press_pull" => {
      let mut faces = Vec::new();
      for f in p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))? {
        faces.push(face_ref(s, f)?);
      }
      add_feature(s, FeatureKind::OffsetFaces { faces, distance: req_expr(p, "distance")? })
    }
    "split_by_body" => {
      let tool = BodyId(req_u32(p, "tool")?);
      let remove_tool = p.get("remove_tool").and_then(Value::as_bool).unwrap_or(false);
      add_feature(s, FeatureKind::SplitByBody { bodies: body_sel(p), tool, remove_tool })
    }
    "shell" => crate::solid::shell(s, p),
    "box" | "cylinder" | "sphere" | "torus" => {
      let shape = match name {
        "box" => {
          Primitive::Box { width: req_expr(p, "width")?, depth: req_expr(p, "depth")?, height: req_expr(p, "height")? }
        }
        "cylinder" => Primitive::Cylinder { radius: radius_expr(p)?, height: req_expr(p, "height")? },
        "sphere" => Primitive::Sphere { radius: radius_expr(p)? },
        _ => Primitive::Torus { major: req_expr(p, "major")?, minor: req_expr(p, "minor")? },
      };
      let plane = plane_ref(s, p.get("plane").unwrap_or(&json!("xy")))?;
      let at = opt_pt(p, "at")?.unwrap_or([0.0, 0.0]);
      add_op_feature(s, FeatureKind::Primitive { shape, plane, at, operation: operation(p)? }, p)
    }
    "plane.offset" => {
      let base = plane_ref(s, p.get("plane").ok_or(CmdError::new("missing 'plane'"))?)?;
      let offset = if p.get("offset").is_some() { req_expr(p, "offset")? } else { "0".into() };
      add_feature(s, FeatureKind::ConstructionPlane { base, offset })
    }
    "mirror" => {
      let plane = plane_ref(s, p.get("plane").ok_or(CmdError::new("missing 'plane'"))?)?;
      let join = p.get("join").and_then(Value::as_bool).unwrap_or(true);
      add_feature(s, FeatureKind::Mirror { bodies: body_sel(p), plane, join })
    }
    "pattern.rectangular" => {
      let direction = req_vec3(p, "direction")?;
      let direction2 = p.get("direction2").map(|_| req_vec3(p, "direction2")).transpose()?;
      add_feature(
        s,
        FeatureKind::RectangularPattern {
          bodies: body_sel(p),
          direction,
          count: req_expr(p, "count")?,
          spacing: req_expr(p, "spacing")?,
          direction2,
          count2: opt_expr(p, "count2").unwrap_or_else(|| "1".into()),
          spacing2: opt_expr(p, "spacing2").unwrap_or_else(|| "0".into()),
          join: p.get("join").and_then(Value::as_bool).unwrap_or(true),
        },
      )
    }
    "pattern.circular" => {
      let axis = axis_ref(s, p.get("axis").unwrap_or(&json!("z")))?;
      add_feature(
        s,
        FeatureKind::CircularPattern {
          bodies: body_sel(p),
          axis,
          count: req_expr(p, "count")?,
          angle: opt_expr(p, "angle").unwrap_or_else(|| "360 deg".into()),
          symmetric: p.get("symmetric").and_then(Value::as_bool).unwrap_or(false),
          join: p.get("join").and_then(Value::as_bool).unwrap_or(true),
        },
      )
    }
    "move" => {
      let t = match p.get("translate").and_then(Value::as_array) {
        Some(a) if a.len() == 3 => [expr_of(&a[0])?, expr_of(&a[1])?, expr_of(&a[2])?],
        Some(_) => return Err(CmdError::new("translate needs [x, y, z]")),
        None => ["0".into(), "0".into(), "0".into()],
      };
      let axis = p.get("axis").map(|a| axis_ref(s, a)).transpose()?;
      let angle = opt_expr(p, "angle").unwrap_or_else(|| "0".into());
      add_feature(s, FeatureKind::Move { bodies: body_sel(p), translate: t, axis, angle })
    }
    "combine" => {
      let target = BodyId(req_u32(p, "target")?);
      let tools = ids(p, "tools").into_iter().map(BodyId).collect();
      let operation = operation(p)?;
      let keep_tools = p.get("keep_tools").and_then(Value::as_bool).unwrap_or(false);
      add_feature(s, FeatureKind::Combine { target, tools, operation, keep_tools })
    }
    "body.remove" => {
      let bodies = ids(p, "bodies").into_iter().map(BodyId).collect();
      add_feature(s, FeatureKind::RemoveBodies { bodies })
    }
    "body.rename" => {
      let b = BodyId(req_u32(p, "body")?);
      let n = req_str(p, "name")?.to_string();
      s.edit(|d| {
        d.body_settings_mut(b).name = Some(n);
        Ok(())
      })?;
      Ok(json!({}))
    }
    "body.visible" => {
      let b = BodyId(req_u32(p, "body")?);
      let v = p.get("visible").and_then(Value::as_bool).unwrap_or(true);
      s.design.body_settings_mut(b).visible = v;
      s.dirty = true;
      Ok(json!({}))
    }
    "body.topology" => body_topology(s, p),
    "sketch.visible" => {
      let sk = FeatureId(req_u32(p, "sketch")?);
      s.design.sketch(sk)?;
      let v = p.get("visible").and_then(Value::as_bool);
      s.edit(|d| {
        d.feature_mut(sk).ok_or(CmdError::new("no such sketch"))?.visible = v;
        Ok(())
      })?;
      Ok(json!({}))
    }
    "feature.edit" => {
      let id = FeatureId(req_u32(p, "feature")?);
      let set = p.get("set").and_then(Value::as_object).ok_or(CmdError::new("missing 'set' object"))?.clone();
      s.edit(|d| {
        let f = d.feature_mut(id).ok_or(CmdError::new("no such feature"))?;
        let mut v = serde_json::to_value(&f.kind)?;
        if let Some(obj) = v.as_object_mut() {
          for (k, val) in set {
            if k == "type" {
              continue;
            }
            obj.insert(k, val);
          }
        }
        f.kind = serde_json::from_value(v).map_err(|e| CmdError::new(format!("invalid change: {e}")))?;
        Ok(())
      })?;
      Ok(feature_status(s, id))
    }
    "feature.delete" => {
      let id = FeatureId(req_u32(p, "feature")?);
      let cascade = p.get("cascade").and_then(Value::as_bool).unwrap_or(false);
      // Fusion's Delete on a sketch keeps the features that use it, on cached geometry
      let solved = match s.design.feature(id).map(|f| &f.kind) {
        Some(FeatureKind::Sketch { .. }) if !cascade => s.model.sketches.get(&id).map(|r| {
          let pl = r.plane;
          (
            PlaneRef::Free { origin: pl.origin.into(), normal: pl.normal().into(), x: Some(pl.x.into()) },
            r.sketch.clone(),
          )
        }),
        _ => None,
      };
      let (removed, kept) = match solved {
        Some((plane, sketch)) => (vec![id], s.edit(|d| Ok(d.remove_sketch_keep_dependents(id, plane, &sketch)?))?),
        None => (s.edit(|d| Ok(d.remove_feature(id)?))?, Vec::new()),
      };
      if s.edit_sketch.is_some_and(|e| removed.contains(&e)) {
        s.edit_sketch = None;
      }
      Ok(json!({
        "removed": removed.iter().map(|f| f.0).collect::<Vec<_>>(),
        "cached": kept.iter().map(|f| f.0).collect::<Vec<_>>(),
      }))
    }
    "feature.suppress" => {
      let id = FeatureId(req_u32(p, "feature")?);
      let on = p.get("suppressed").and_then(Value::as_bool);
      s.edit(|d| {
        let f = d.feature_mut(id).ok_or(CmdError::new("no such feature"))?;
        f.suppressed = on.unwrap_or(!f.suppressed);
        Ok(())
      })?;
      Ok(json!({}))
    }
    "feature.rename" => {
      let id = FeatureId(req_u32(p, "feature")?);
      let n = req_str(p, "name")?.to_string();
      s.edit(|d| {
        d.feature_mut(id).ok_or(CmdError::new("no such feature"))?.name = n;
        Ok(())
      })?;
      Ok(json!({}))
    }
    "feature.move" => {
      let id = FeatureId(req_u32(p, "feature")?);
      let to = req_u32(p, "to")? as usize;
      // references to bodies, faces and edges only show up when the timeline recomputes: a move
      // that makes a feature fail that computed before is undone, like Fusion refusing the drop
      let failing = |s: &Session| -> Vec<FeatureId> {
        s.design
          .features
          .iter()
          .map(|f| f.id)
          .filter(|f| s.model.status(*f).is_some_and(|st| st.error.is_some()))
          .collect()
      };
      let before = failing(s);
      s.edit(|d| Ok(d.move_feature(id, to)?))?;
      let broken: Vec<String> = failing(s)
        .into_iter()
        .filter(|f| !before.contains(f))
        .filter_map(|f| s.design.feature(f).map(|f| f.name.clone()))
        .collect();
      if !broken.is_empty() {
        s.undo();
        s.redo.pop();
        let name = s.design.feature(id).map(|f| f.name.clone()).unwrap_or_default();
        return Err(CmdError::new(format!("moving {name} there would break {}", broken.join(", "))));
      }
      Ok(json!({"index": s.design.index_of(id)}))
    }
    "timeline.marker" => {
      let at = p.get("at").and_then(Value::as_u64).map(|v| v as usize);
      s.edit(|d| {
        d.set_marker(at);
        Ok(())
      })?;
      Ok(json!({"active": s.design.active_len()}))
    }
    "select" => {
      let items: Vec<Sel> = serde_json::from_value(p.get("items").cloned().unwrap_or(json!([])))?;
      s.selection = items;
      Ok(json!({}))
    }
    "selection" => Ok(json!(s.selection)),
    "view.render" => {
      let path = PathBuf::from(req_str(p, "path")?);
      let png = crate::scene::render_png(s, p)?;
      std::fs::write(&path, &png)?;
      Ok(json!({"path": path, "bytes": png.len()}))
    }
    "attribute.set" => {
      let target = attr_target(s, p.get("target"))?;
      let (group, name) = (req_str(p, "group")?.to_string(), req_str(p, "name")?.to_string());
      let value = p.get("value").cloned().ok_or(CmdError::new("missing 'value'"))?;
      s.edit(|d| {
        d.set_attribute(target, &group, &name, value);
        Ok(())
      })?;
      Ok(json!({}))
    }
    "attribute.get" | "attribute.find" => {
      let target = if name == "attribute.get" { Some(attr_target(s, p.get("target"))?) } else { None };
      let (group, aname, value) = (opt_str(p, "group"), opt_str(p, "name"), p.get("value"));
      let found: Vec<Value> = s
        .design
        .attributes
        .iter()
        .filter(|a| target.is_none_or(|t| a.target == t) && attr_target_exists(s, &a.target))
        .filter(|a| group.is_none_or(|g| a.group == g) && aname.is_none_or(|n| a.name == n))
        .filter(|a| value.is_none_or(|v| &a.value == v))
        .map(|a| json!({"target": attr_target_json(&a.target), "group": a.group, "name": a.name, "value": a.value}))
        .collect();
      Ok(json!(found))
    }
    "attribute.delete" => {
      let target = attr_target(s, p.get("target"))?;
      let group = req_str(p, "group")?.to_string();
      let aname = opt_str(p, "name").map(str::to_string);
      let n = s.edit(|d| Ok(d.remove_attributes(target, &group, aname.as_deref())))?;
      Ok(json!({"removed": n}))
    }
    "sheet.rule" => {
      let name = opt_str(p, "name").unwrap_or("Default").to_string();
      let mut rule = s
        .design
        .sheet_metal_rules
        .iter()
        .find(|r| r.name == name)
        .cloned()
        .unwrap_or_else(|| SheetMetalRule { name: name.clone(), ..s.design.sheet_rule("").unwrap_or_default() });
      for (k, slot) in [
        ("thickness", &mut rule.thickness),
        ("bend_radius", &mut rule.bend_radius),
        ("k_factor", &mut rule.k_factor),
        ("relief_width", &mut rule.relief_width),
        ("relief_depth", &mut rule.relief_depth),
        ("gap", &mut rule.gap),
        ("corner_relief_size", &mut rule.corner_relief_size),
      ] {
        if let Some(e) = opt_expr(p, k) {
          *slot = e;
        }
      }
      if let Some(r) = opt_str(p, "relief") {
        rule.relief = match r {
          "rectangular" => ReliefShape::Rectangular,
          "round" => ReliefShape::Round,
          "none" => ReliefShape::None,
          o => return Err(CmdError::new(format!("unknown relief '{o}' (rectangular, round, none)"))),
        };
      }
      if let Some(r) = opt_str(p, "corner_relief") {
        rule.corner_relief = match r {
          "none" => fox_model::CornerRelief::None,
          "round" => fox_model::CornerRelief::Round,
          "square" => fox_model::CornerRelief::Square,
          "tear" => fox_model::CornerRelief::Tear,
          o => return Err(CmdError::new(format!("unknown corner relief '{o}' (none, round, square, tear)"))),
        };
      }
      let info = fox_model::sheet::eval_rule(&rule, &params_scope(s)).map_err(CmdError::new)?;
      s.edit(|d| Ok(d.set_sheet_rule(rule)?))?;
      Ok(json!({"name": name, "thickness": info.thickness, "bend_radius": info.bend_radius, "k_factor": info.k_factor}))
    }
    "sheet.rules" => {
      let scope = params_scope(s);
      let mut rules = s.design.sheet_metal_rules.clone();
      if rules.is_empty() {
        rules.push(SheetMetalRule::default());
      }
      Ok(json!(
        rules
          .iter()
          .map(|r| {
            let v = fox_model::sheet::eval_rule(r, &scope);
            json!({
              "name": r.name, "thickness": r.thickness, "bend_radius": r.bend_radius, "k_factor": r.k_factor,
              "values": v.as_ref().ok().map(|i| [i.thickness, i.bend_radius, i.k_factor]),
              "error": v.err(),
            })
          })
          .collect::<Vec<_>>()
      ))
    }
    "sheet.rule_remove" => {
      let name = req_str(p, "name")?.to_string();
      s.edit(|d| Ok(d.remove_sheet_rule(&name)?))?;
      Ok(json!({}))
    }
    "sheet.flange" => {
      let sk = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sk, p)?;
      let operation = operation(p)?;
      if !matches!(operation, Operation::NewBody | Operation::Join) {
        return Err(CmdError::new("a flange makes a new body or joins one"));
      }
      let rule = rule_name(s, p)?;
      let flip = p.get("flip").and_then(Value::as_bool).unwrap_or(false);
      add_op_feature(s, FeatureKind::Flange { sketch: sk, profiles, rule, flip, operation }, p)
    }
    "sheet.contour_flange" => {
      let sk = sketch_for_feature(s, p)?;
      let curves = ids(p, "curves").into_iter().map(CurveId).collect();
      let rule = rule_name(s, p)?;
      let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
      add_feature(
        s,
        FeatureKind::ContourFlange {
          sketch: sk,
          curves,
          distance: req_expr(p, "distance")?,
          symmetric: flag("symmetric"),
          flip: flag("flip"),
          flip_direction: flag("flip_direction"),
          rule,
        },
      )
    }
    "sheet.edge_flange" => {
      let edges = edge_refs(s, p)?;
      let position = match opt_str(p, "position").unwrap_or("adjacent") {
        "adjacent" => BendPosition::Adjacent,
        "inside" => BendPosition::Inside,
        "outside" => BendPosition::Outside,
        o => return Err(CmdError::new(format!("unknown bend position '{o}' (adjacent, inside, outside)"))),
      };
      add_feature(
        s,
        FeatureKind::EdgeFlange {
          edges,
          height: req_expr(p, "height")?,
          angle: opt_expr(p, "angle").unwrap_or_else(|| "90 deg".into()),
          flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
          position,
          offset_start: opt_expr(p, "offset_start").unwrap_or_else(|| "0".into()),
          offset_end: opt_expr(p, "offset_end").unwrap_or_else(|| "0".into()),
          corners: crate::sheet2::corner_mode(p)?,
        },
      )
    }
    "sheet.hem" => {
      let edges = edge_refs(s, p)?;
      add_feature(
        s,
        FeatureKind::Hem {
          edges,
          length: req_expr(p, "length")?,
          gap: opt_expr(p, "gap").unwrap_or_else(|| "0.5 mm".into()),
          flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
        },
      )
    }
    "sheet.bend" => {
      let sk = sketch_for_feature(s, p)?;
      let line = CurveId(req_u32(p, "line")?);
      let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
      add_feature(
        s,
        FeatureKind::Bend {
          sketch: sk,
          line,
          angle: req_expr(p, "angle")?,
          flip: flag("flip"),
          flip_side: flag("flip_side"),
        },
      )
    }
    "sheet.convert" => {
      let body = BodyId(req_u32(p, "body")?);
      let face = p.get("face").map(|f| face_ref(s, f)).transpose()?;
      let rule = rule_name(s, p)?;
      let bends = p.get("bends").and_then(Value::as_bool).unwrap_or(true);
      add_feature(s, FeatureKind::ConvertToSheetMetal { body, face, rule, bends })
    }
    "sheet.rip" => {
      let edges = edge_refs(s, p)?;
      add_feature(s, FeatureKind::Rip { edges, gap: opt_expr(p, "gap").unwrap_or_default() })
    }
    "sheet.refold" => {
      let body = match p.get("body") {
        Some(_) => BodyId(req_u32(p, "body")?),
        None => {
          let mut it = s.model.bodies.iter().filter(|b| b.unfolded.is_some());
          match (it.next(), it.next()) {
            (Some(b), None) => b.id,
            (None, _) => return Err(CmdError::new("there is no unfolded body")),
            _ => return Err(CmdError::new("there are several unfolded bodies; pass 'body'")),
          }
        }
      };
      add_feature(s, FeatureKind::Refold { body })
    }
    "sheet.unfold" => {
      let body = sheet_body(s, p)?;
      let face = p.get("face").map(|f| face_ref(s, f)).transpose()?;
      let keep_folded = p.get("keep_folded").and_then(Value::as_bool).unwrap_or(false);
      add_feature(s, FeatureKind::Unfold { body, face, keep_folded })
    }
    "sheet.flat_pattern" => flat_pattern(s, p),
    "sheet.check" => sheet_check(s, p),
    "sketch.text"
    | "sketch.text_edit"
    | "sketch.text_explode"
    | "sketch.break"
    | "sketch.tangent_circle"
    | "sketch.blend" => sketch_tools::run(s, name, p),
    "sketch.elliptical_arc" | "sketch.fonts" => sketch_curves::run(s, name, p),
    "sketch.include" => sketch_include::include(s, p),
    _ => Err(CmdError::new(format!("unknown command '{name}' (try 'help')"))),
  }
}

// ---------------------------------------------------------------- parameters helpers

pub(crate) fn req_str<'a>(p: &'a Value, k: &str) -> Result<&'a str, CmdError> {
  p.get(k).and_then(Value::as_str).ok_or_else(|| CmdError::new(format!("missing '{k}'")))
}

pub(crate) fn opt_str<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
  p.get(k).and_then(Value::as_str)
}

pub(crate) fn req_u32(p: &Value, k: &str) -> Result<u32, CmdError> {
  p.get(k)
    .and_then(Value::as_u64)
    .and_then(|v| u32::try_from(v).ok())
    .ok_or_else(|| CmdError::new(format!("missing or invalid '{k}'")))
}

/// An expression: a JSON number or a string ("40 mm", "width/2").
pub(crate) fn expr_of(v: &Value) -> Result<String, CmdError> {
  match v {
    Value::String(s) if !s.trim().is_empty() => Ok(s.clone()),
    Value::Number(n) => {
      n.as_f64().filter(|f| f.is_finite()).map(fox_expr::trim_number).ok_or(CmdError::new("bad number"))
    }
    _ => Err(CmdError::new("expected a number or an expression string")),
  }
}

pub(crate) fn req_expr(p: &Value, k: &str) -> Result<String, CmdError> {
  expr_of(p.get(k).ok_or_else(|| CmdError::new(format!("missing '{k}'")))?)
}

pub(crate) fn opt_expr(p: &Value, k: &str) -> Option<String> {
  p.get(k).and_then(|v| expr_of(v).ok())
}

fn radius_expr(p: &Value) -> Result<String, CmdError> {
  if let Some(r) = opt_expr(p, "radius") {
    return Ok(r);
  }
  let d = req_expr(p, "diameter").map_err(|_| CmdError::new("missing 'radius' or 'diameter'"))?;
  Ok(format!("({d}) / 2"))
}

fn pt_of(v: &Value) -> Option<[f64; 2]> {
  let a = v.as_array()?;
  match a.as_slice() {
    [x, y] => {
      let (x, y) = (x.as_f64()?, y.as_f64()?);
      (x.is_finite() && y.is_finite()).then_some([x, y])
    }
    _ => None,
  }
}

fn req_pt(p: &Value, k: &str) -> Result<[f64; 2], CmdError> {
  p.get(k).and_then(pt_of).ok_or_else(|| CmdError::new(format!("'{k}' must be [x, y]")))
}

fn opt_pt(p: &Value, k: &str) -> Result<Option<[f64; 2]>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => pt_of(v).map(Some).ok_or_else(|| CmdError::new(format!("'{k}' must be [x, y]"))),
  }
}

pub(crate) fn req_vec3(p: &Value, k: &str) -> Result<[f64; 3], CmdError> {
  let a = p.get(k).and_then(Value::as_array).ok_or_else(|| CmdError::new(format!("'{k}' must be [x, y, z]")))?;
  match a.as_slice() {
    [x, y, z] => match (x.as_f64(), y.as_f64(), z.as_f64()) {
      (Some(x), Some(y), Some(z)) if x.is_finite() && y.is_finite() && z.is_finite() => Ok([x, y, z]),
      _ => Err(CmdError::new(format!("'{k}' must be numbers"))),
    },
    _ => Err(CmdError::new(format!("'{k}' must be [x, y, z]"))),
  }
}

pub(crate) fn ids(p: &Value, k: &str) -> Vec<u32> {
  p.get(k)
    .and_then(Value::as_array)
    .map(|a| a.iter().filter_map(|v| v.as_u64().and_then(|v| u32::try_from(v).ok())).collect())
    .unwrap_or_default()
}

fn param_kind(s: Option<&str>) -> Result<ParamKind, CmdError> {
  Ok(match s.unwrap_or("length") {
    "length" => ParamKind::Length,
    "angle" => ParamKind::Angle,
    "unitless" | "number" | "count" => ParamKind::Unitless,
    o => return Err(CmdError::new(format!("unknown parameter kind '{o}'"))),
  })
}

fn params_scope(s: &Session) -> std::collections::HashMap<String, f64> {
  s.model.params.clone()
}

pub(crate) fn operation(p: &Value) -> Result<Operation, CmdError> {
  Ok(match opt_str(p, "operation").unwrap_or("new_body") {
    "new_body" | "new" => Operation::NewBody,
    "join" | "union" => Operation::Join,
    "cut" | "subtract" => Operation::Cut,
    "intersect" => Operation::Intersect,
    o => return Err(CmdError::new(format!("unknown operation '{o}'"))),
  })
}

pub(crate) fn body_sel(p: &Value) -> BodySel {
  if let Some(c) = p.get("component").and_then(Value::as_u64).and_then(|c| u32::try_from(c).ok()) {
    return BodySel::Component { id: fox_model::ComponentId(c) };
  }
  let v = ids(p, "bodies");
  if v.is_empty() { BodySel::All } else { BodySel::Bodies { ids: v.into_iter().map(BodyId).collect() } }
}

pub(crate) fn add_feature(s: &mut Session, kind: FeatureKind) -> CmdResult {
  let id = s.edit(|d| Ok(d.add_feature(kind)?))?;
  Ok(feature_status(s, id))
}

/// Adds a join/cut/intersect feature, limited to the bodies in `targets` when given.
fn add_op_feature(s: &mut Session, kind: FeatureKind, p: &Value) -> CmdResult {
  let targets: Vec<BodyId> = ids(p, "targets").into_iter().map(BodyId).collect();
  if let Some(b) = targets.iter().find(|b| s.model.body(**b).is_none()) {
    return Err(CmdError::new(format!("no body {}", b.0)));
  }
  let id = s.edit(|d| {
    let id = d.add_feature(kind)?;
    if let Some(f) = d.feature_mut(id) {
      f.targets = targets;
    }
    Ok(id)
  })?;
  Ok(feature_status(s, id))
}

fn feature_status(s: &Session, id: FeatureId) -> Value {
  let st = s.model.status(id);
  json!({
    "feature": id.0,
    "name": s.design.feature(id).map(|f| f.name.clone()),
    "error": st.and_then(|x| x.error.clone()),
    "warning": st.and_then(|x| x.warning.clone()),
    "bodies": s.model.bodies.len(),
    // bodies this feature created or changed
    "result_bodies": s.model.bodies.iter().filter(|b| b.feature == id).map(|b| b.id.0).collect::<Vec<_>>(),
  })
}

/// An attribute target: absent or `"design"`, `{body}`, `{component}`, `{feature}`, `{sketch}`
/// (the sketch feature), `{sketch, curve}` or `{sketch, point}`.
fn attr_target(s: &Session, v: Option<&Value>) -> Result<AttrTarget, CmdError> {
  let v = match v {
    None | Some(Value::Null) => return Ok(AttrTarget::Design),
    Some(Value::String(d)) if d == "design" => return Ok(AttrTarget::Design),
    Some(v) => v,
  };
  let id = |k: &str| v.get(k).and_then(Value::as_u64).and_then(|x| u32::try_from(x).ok());
  let t = if let Some(sk) = id("sketch") {
    let sketch = FeatureId(sk);
    match (id("curve"), id("point")) {
      (Some(c), _) => AttrTarget::SketchCurve { sketch, curve: CurveId(c) },
      (None, Some(pt)) => AttrTarget::SketchPoint { sketch, point: PointId(pt) },
      (None, None) => AttrTarget::Feature { feature: sketch },
    }
  } else if let Some(b) = id("body") {
    AttrTarget::Body { body: BodyId(b) }
  } else if let Some(c) = id("component") {
    AttrTarget::Component { component: fox_model::ComponentId(c) }
  } else if let Some(f) = id("feature") {
    AttrTarget::Feature { feature: FeatureId(f) }
  } else {
    return Err(CmdError::new(format!("not an attribute target: {v}")));
  };
  if !attr_target_exists(s, &t) {
    return Err(CmdError::new(format!("{v} does not exist")));
  }
  Ok(t)
}

fn attr_target_exists(s: &Session, t: &AttrTarget) -> bool {
  match *t {
    AttrTarget::Design => true,
    AttrTarget::Body { body } => s.model.body(body).is_some(),
    AttrTarget::Component { component } => s.design.components.iter().any(|c| c.id == component),
    AttrTarget::Feature { feature } => s.design.feature(feature).is_some(),
    AttrTarget::SketchCurve { sketch, curve } => s.design.sketch(sketch).is_ok_and(|k| k.curve(curve).is_some()),
    AttrTarget::SketchPoint { sketch, point } => s.design.sketch(sketch).is_ok_and(|k| k.point(point).is_some()),
  }
}

fn attr_target_json(t: &AttrTarget) -> Value {
  match *t {
    AttrTarget::Design => json!("design"),
    AttrTarget::Body { body } => json!({"body": body.0}),
    AttrTarget::Component { component } => json!({"component": component.0}),
    AttrTarget::Feature { feature } => json!({"feature": feature.0}),
    AttrTarget::SketchCurve { sketch, curve } => json!({"sketch": sketch.0, "curve": curve.0}),
    AttrTarget::SketchPoint { sketch, point } => json!({"sketch": sketch.0, "point": point.0}),
  }
}

/// `translate`, `angle`, `scale` and `center` of a sketch transform.
fn xform(s: &Session, p: &Value) -> Result<fox_sketch::Xform2, CmdError> {
  let scope = params_scope(s);
  let num = |k: &str, kind| -> Result<Option<f64>, CmdError> {
    opt_expr(p, k).map(|e| fox_expr::eval(&e, kind, &scope)).transpose().map_err(CmdError::from)
  };
  Ok(fox_sketch::Xform2 {
    scale: num("scale", fox_expr::Kind::Unitless)?.unwrap_or(1.0),
    angle: num("angle", fox_expr::Kind::Angle)?.unwrap_or(0.0),
    center: opt_pt(p, "center")?.unwrap_or([0.0, 0.0]),
    translate: opt_pt(p, "translate")?.unwrap_or([0.0, 0.0]),
  })
}

/// The sketch `sketch.insert` copies from.
fn insert_source(s: &Session, from: &Value) -> Result<(Sketch, Option<String>), CmdError> {
  let Some(path) = opt_str(from, "path") else {
    let id = FeatureId(req_u32(from, "sketch")?);
    let sk = s.model.sketches.get(&id).map(|r| r.sketch.clone());
    return Ok((sk.map_or_else(|| s.design.sketch(id).cloned(), Ok)?, None));
  };
  let path = PathBuf::from(path);
  let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
  if ext == "dxf" || ext == "svg" {
    return crate::files::dxf_sketch(&path);
  }
  if !matches!(ext.as_str(), "fox" | "foxz" | "clf" | "clfz") {
    return Err(CmdError::new("insert from a .dxf, .svg, .fox or .foxz file"));
  }
  let other = Session::open(&path)?;
  let want = from.get("sketch");
  let found = other.design.features.iter().find(|f| {
    matches!(f.kind, FeatureKind::Sketch { .. })
      && match want {
        None => true,
        Some(Value::String(n)) => &f.name == n,
        Some(v) => v.as_u64() == Some(u64::from(f.id.0)),
      }
  });
  let f = found.ok_or_else(|| CmdError::new(format!("{} has no such sketch", path.display())))?;
  let sk = other.model.sketches.get(&f.id).map(|r| r.sketch.clone());
  Ok((sk.map_or_else(|| other.design.sketch(f.id).cloned(), Ok)?, None))
}

/// `"#rrggbb"` or `[r, g, b]` (0–255).
pub(crate) fn parse_color(v: &Value) -> Result<[u8; 3], CmdError> {
  let bad = || CmdError::new(format!("a colour is \"#rrggbb\" or [r, g, b], not {v}"));
  if let Some(hex) = v.as_str() {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 || !h.is_ascii() {
      return Err(bad());
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| bad());
    return Ok([byte(0)?, byte(2)?, byte(4)?]);
  }
  let a = v.as_array().filter(|a| a.len() == 3).ok_or_else(bad)?;
  let mut c = [0u8; 3];
  for (slot, x) in c.iter_mut().zip(a) {
    *slot = x.as_u64().and_then(|x| u8::try_from(x).ok()).ok_or_else(bad)?;
  }
  Ok(c)
}

fn body_topology(s: &Session, p: &Value) -> CmdResult {
  let id = BodyId(req_u32(p, "body")?);
  let body = s.model.body(id).ok_or(CmdError::new(format!("no body {}", id.0)))?;
  let want = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(true);
  let v3 = |v: DVec3| json!([v.x, v.y, v.z]);
  let mut out = serde_json::Map::new();
  if want("faces") {
    let faces: Vec<Value> = body
      .faces()
      .iter()
      .map(|f| {
        let edges: Vec<u32> = body.edges().iter().filter(|e| e.faces.contains(&f.id)).map(|e| e.id).collect();
        json!({
          "id": f.id,
          "kind": format!("{:?}", f.kind).to_ascii_lowercase(),
          "area": f.area,
          "origin": v3(f.origin),
          "normal": v3(f.normal),
          "xdir": v3(f.xdir),
          "center": v3(f.center),
          "radius": f.radius,
          "edges": edges,
        })
      })
      .collect();
    out.insert("faces".into(), json!(faces));
  }
  if want("edges") {
    let edges: Vec<Value> = body
      .edges()
      .iter()
      .map(|e| {
        json!({
          "id": e.id,
          "kind": format!("{:?}", e.kind).to_ascii_lowercase(),
          "start": v3(e.start),
          "end": v3(e.end),
          "mid": v3(e.mid),
          "center": v3(e.center),
          "axis": v3(e.axis),
          "radius": e.radius,
          "length": e.length,
          "faces": e.faces.iter().copied().filter(|f| *f != 0).collect::<Vec<_>>(),
        })
      })
      .collect();
    out.insert("edges".into(), json!(edges));
  }
  Ok(Value::Object(out))
}

/// `rule`: a rule name that must exist (empty / absent = the default rule).
pub(crate) fn rule_name(s: &Session, p: &Value) -> Result<String, CmdError> {
  let name = opt_str(p, "rule").unwrap_or("").to_string();
  if s.design.sheet_rule(&name).is_none() {
    return Err(CmdError::new(format!("there is no sheet metal rule '{name}'")));
  }
  Ok(name)
}

/// `body`, or the only sheet metal body.
pub(crate) fn sheet_body(s: &Session, p: &Value) -> Result<BodyId, CmdError> {
  if p.get("body").is_some() {
    return Ok(BodyId(req_u32(p, "body")?));
  }
  let mut sheets = s.model.bodies.iter().filter(|b| b.sheet.as_ref().is_some_and(|i| !i.flat));
  match (sheets.next(), sheets.next()) {
    (Some(b), None) => Ok(b.id),
    (None, _) => Err(CmdError::new("there is no sheet metal body")),
    _ => Err(CmdError::new("there are several sheet metal bodies; pass 'body'")),
  }
}

/// The flat pattern of the sheet metal body in `p` (`body?`, stationary `face?`).
fn flat_of<'a>(
  s: &'a Session,
  p: &Value,
) -> Result<(BodyId, &'a fox_model::regen::Body, fox_model::sheet::FlatPattern), CmdError> {
  let id = sheet_body(s, p)?;
  let body = s.model.body(id).ok_or(CmdError::new(format!("no body {}", id.0)))?;
  let face = match p.get("face") {
    Some(f) => {
      let r = face_ref(s, f)?;
      if r.body != id {
        return Err(CmdError::new("the stationary face must be on the body"));
      }
      Some(r.resolve(body.faces(), body.mesh(), body.diagonal()).ok_or(CmdError::new("no such face"))?)
    }
    None => None,
  };
  let flat = body.flat_pattern(face).map_err(CmdError::new)?;
  Ok((id, body, flat))
}

/// Checks a sheet metal body's flat pattern against the rules shops cut by.
fn sheet_check(s: &Session, p: &Value) -> CmdResult {
  let (id, body, flat) = flat_of(s, p)?;
  let info = body.sheet.as_ref().ok_or(CmdError::new("not a sheet metal body"))?;
  let mut rules = fox_model::cutcheck::Rules::for_sheet(info.thickness, info.bend_radius);
  let scope = params_scope(s);
  let len = |k: &str| -> Result<Option<f64>, CmdError> {
    match p.get(k) {
      Some(v) => Ok(Some(fox_expr::eval(&expr_of(v)?, fox_expr::Kind::Length, &scope)?)),
      None => Ok(None),
    }
  };
  if let Some(v) = len("min_hole")? {
    rules.min_hole = v;
  }
  if let Some(v) = len("min_web")? {
    rules.min_web = v;
  }
  if let Some(v) = len("bend_clearance")? {
    rules.bend_clearance = v;
  }
  if let Some(a) = p.get("sheet").and_then(Value::as_array) {
    let side = |k: usize| a.get(k).ok_or(CmdError::new("sheet is [width, height]")).and_then(expr_of);
    let (w, h) = (side(0)?, side(1)?);
    rules.sheet =
      Some([fox_expr::eval(&w, fox_expr::Kind::Length, &scope)?, fox_expr::eval(&h, fox_expr::Kind::Length, &scope)?]);
  }
  let (contours, bends) = fox_model::sheet::flat_contours(&flat).map_err(CmdError::new)?;
  let findings = fox_model::cutcheck::check(&contours, &bends, &rules);
  Ok(json!({
    "body": id.0,
    "ok": findings.is_empty(),
    "findings": findings,
    "rules": {
      "min_hole": rules.min_hole,
      "min_web": rules.min_web,
      "bend_clearance": rules.bend_clearance,
      "sheet": rules.sheet,
    },
  }))
}

fn flat_pattern(s: &Session, p: &Value) -> CmdResult {
  let (id, body, flat) = flat_of(s, p)?;
  let m = flat.shape.mass()?;
  // size in the stationary plane
  let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
  for e in flat.shape.edge_lines(0.05)? {
    for q in e.points {
      let l = flat.plane.to_local(q);
      for k in 0..2 {
        lo[k] = lo[k].min(l[k]);
        hi[k] = hi[k].max(l[k]);
      }
    }
  }
  if let Some(path) = opt_str(p, "path") {
    let path = PathBuf::from(path);
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
      "dxf" => std::fs::write(&path, crate::sheet2::flat_dxf(body, &flat)?)?,
      "step" | "stp" => fox_kernel::write_step([&flat.shape], &path)?,
      "stl" => fox_kernel::write_stl(&[flat.shape.tessellate(0.02)?], &path)?,
      o => return Err(CmdError::new(format!("unknown flat pattern format '.{o}' (dxf, step, stl)"))),
    }
  }
  Ok(json!({
    "body": id.0,
    "size": [hi[0] - lo[0], hi[1] - lo[1]],
    "area_mm2": m.volume / body.sheet.as_ref().map(|i| i.thickness).unwrap_or(1.0),
    "volume_mm3": m.volume,
    "bends": flat.bends.iter().map(|b| json!({
      "angle_deg": b.angle.to_degrees(),
      "length": (b.end - b.start).length(),
      "direction": if b.inner { "up" } else { "down" },
      "start": flat.plane.to_local(b.start),
      "end": flat.plane.to_local(b.end),
    })).collect::<Vec<_>>(),
  }))
}

// ---------------------------------------------------------------- references

/// "xy" | "xz" | "yz" | {"feature": id} | {"body": id, "face": n}
pub(crate) fn plane_ref(s: &Session, v: &Value) -> Result<PlaneRef, CmdError> {
  if let Some(name) = v.as_str() {
    let plane = match name.to_ascii_lowercase().as_str() {
      "xy" | "top" => OriginPlane::Xy,
      "xz" | "front" => OriginPlane::Xz,
      "yz" | "right" => OriginPlane::Yz,
      o => return Err(CmdError::new(format!("unknown plane '{o}' (xy, xz, yz)"))),
    };
    return Ok(PlaneRef::Origin { plane });
  }
  if let Some(f) = v.get("feature").and_then(Value::as_u64) {
    let id = FeatureId(u32::try_from(f).map_err(|_| CmdError::new("bad feature"))?);
    return match s.design.feature(id).map(|f| &f.kind) {
      Some(FeatureKind::ConstructionPlane { .. } | FeatureKind::PlaneBy { .. }) => {
        Ok(PlaneRef::Construction { feature: id })
      }
      _ => Err(CmdError::new("that feature is not a construction plane")),
    };
  }
  if v.get("origin").is_some() {
    let normal = req_vec3(v, "normal")?;
    if DVec3::from(normal).length() < 1e-12 {
      return Err(CmdError::new("the plane's normal is zero"));
    }
    let x = v.get("x").map(|_| req_vec3(v, "x")).transpose()?;
    return Ok(PlaneRef::Free { origin: req_vec3(v, "origin")?, normal, x });
  }
  let face = face_ref(s, v)?;
  if !face.planar {
    return Err(CmdError::new("that face is not planar"));
  }
  Ok(PlaneRef::Face { face })
}

/// {"body", "face"} by index, or {"point": [x,y,z], "normal"?: [x,y,z]} — the face at a point.
pub(crate) fn face_ref(s: &Session, v: &Value) -> Result<FaceRef, CmdError> {
  if v.get("point").is_some() {
    let (body, face) =
      face_at(s, DVec3::from(req_vec3(v, "point")?), v.get("normal").map(|_| req_vec3(v, "normal")).transpose()?)?;
    let b = s.model.body(body).ok_or(CmdError::new("no such body"))?;
    let info = b.faces().iter().find(|f| f.id == face).ok_or(CmdError::new("no such face"))?;
    return Ok(FaceRef::from_face(body, info, DVec3::from(req_vec3(v, "point")?)));
  }
  let body = BodyId(req_u32(v, "body")?);
  let face = req_u32(v, "face")?;
  let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
  let info = b.faces().iter().find(|f| f.id == face).ok_or(CmdError::new(format!("body has no face {face}")))?;
  Ok(FaceRef::from_face(body, info, info.center))
}

/// The face containing a world point (within 1e-4 mm), optionally with a matching normal.
pub fn face_at(s: &Session, p: DVec3, normal: Option<[f64; 3]>) -> Result<(BodyId, u32), CmdError> {
  let want = normal.map(DVec3::from).and_then(DVec3::try_normalize);
  let mut best: Option<(BodyId, u32, f64)> = None;
  for b in &s.model.bodies {
    let Some(mesh) = b.mesh() else { continue };
    for f in b.faces() {
      if let Some(n) = want
        && f.normal.dot(n) < 0.999
      {
        continue;
      }
      let d = if f.kind == fox_kernel::SurfaceKind::Plane {
        let off = (p - f.origin).dot(f.normal).abs();
        let q = p - f.normal * (p - f.origin).dot(f.normal);
        if off > 1e-3 || !fox_model::refs::point_in_face(mesh, f.id, q) {
          continue;
        }
        off
      } else {
        // curved faces: near the face's triangles (they sag inside the true surface), then the
        // exact distance where the surface is known
        let sag = (b.diagonal() * 0.01).clamp(0.05, 2.0);
        let dt = nearest_triangle_dist(mesh, f.id, p);
        if dt > sag {
          continue;
        }
        let radial = |o: DVec3| {
          let v = p - o;
          (v - f.normal * v.dot(f.normal)).length()
        };
        match f.kind {
          fox_kernel::SurfaceKind::Cylinder => (radial(f.origin) - f.radius).abs(),
          fox_kernel::SurfaceKind::Sphere => ((p - f.origin).length() - f.radius).abs(),
          _ => dt.min(nearest_vertex_dist(mesh, f.id, p)) * 0.5,
        }
      };
      if d < 0.05 && best.is_none_or(|(_, _, bd)| d < bd) {
        best = Some((b.id, f.id, d));
      }
    }
  }
  best.map(|(b, f, _)| (b, f)).ok_or_else(|| CmdError::new(format!("no face at {:?}", p.to_array())))
}

/// Distance from a point to the nearest triangle of a face's tessellation.
fn nearest_triangle_dist(m: &fox_kernel::TriMesh, face: u32, p: DVec3) -> f64 {
  let v = |i: u32| m.positions.get(i as usize).map(|q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64));
  let mut best = f64::INFINITY;
  for (t, tri) in m.indices.chunks_exact(3).enumerate() {
    if m.tri_face.get(t) != Some(&face) {
      continue;
    }
    if let (Some(a), Some(b), Some(c)) = (v(tri[0]), v(tri[1]), v(tri[2])) {
      best = best.min(point_triangle_dist(p, a, b, c));
    }
  }
  best
}

fn point_triangle_dist(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> f64 {
  let n = (b - a).cross(c - a);
  let n2 = n.length_squared();
  if n2 > 1e-24 {
    // inside the triangle's prism: the distance to its plane
    let q = p - n * ((p - a).dot(n) / n2);
    let inside = [(a, b), (b, c), (c, a)].iter().all(|(u, w)| (*w - *u).cross(q - *u).dot(n) >= 0.0);
    if inside {
      return (p - q).length();
    }
  }
  seg_dist3(a, b, p).min(seg_dist3(b, c, p)).min(seg_dist3(c, a, p))
}

fn nearest_vertex_dist(m: &fox_kernel::TriMesh, face: u32, p: DVec3) -> f64 {
  let mut best = f64::INFINITY;
  for (t, tri) in m.indices.chunks_exact(3).enumerate() {
    if m.tri_face.get(t) != Some(&face) {
      continue;
    }
    for &i in tri {
      if let Some(q) = m.positions.get(i as usize) {
        best = best.min((DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64) - p).length());
      }
    }
  }
  best
}

/// The edge passing nearest a world point.
pub fn edge_at(s: &Session, p: DVec3) -> Result<(BodyId, u32), CmdError> {
  let mut best: Option<(BodyId, u32, f64)> = None;
  for b in &s.model.bodies {
    let tol = (b.diagonal() * 0.0005).clamp(0.001, 0.05);
    let Ok(lines) = b.shape.edge_lines(tol) else { continue };
    for e in lines {
      for w in e.points.windows(2) {
        let d = seg_dist3(w[0], w[1], p);
        if best.is_none_or(|(_, _, bd)| d < bd) {
          best = Some((b.id, e.id, d));
        }
      }
    }
  }
  match best {
    Some((b, e, d)) if d < 0.05 => Ok((b, e)),
    _ => Err(CmdError::new(format!("no edge at {:?}", p.to_array()))),
  }
}

fn seg_dist3(a: DVec3, b: DVec3, p: DVec3) -> f64 {
  let ab = b - a;
  let l2 = ab.length_squared();
  let t = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
  (a + ab * t - p).length()
}

/// The sketch line passing nearest a world point, newest sketch first.
pub(crate) fn sketch_line_at(s: &Session, p: DVec3) -> Result<(FeatureId, CurveId), CmdError> {
  let mut best: Option<(FeatureId, CurveId, f64)> = None;
  for (id, r) in s.model.sketches.iter().rev() {
    for c in &r.sketch.curves {
      let CurveKind::Line { a, b } = c.kind else { continue };
      let (Some(pa), Some(pb)) = (r.sketch.pos(a), r.sketch.pos(b)) else { continue };
      let d = seg_dist3(r.plane.to_world(pa), r.plane.to_world(pb), p);
      if best.is_none_or(|(_, _, bd)| d < bd - 1e-9) {
        best = Some((*id, c.id, d));
      }
    }
  }
  match best {
    Some((sk, c, d)) if d < 0.05 => Ok((sk, c)),
    _ => Err(CmdError::new(format!("no edge or sketch line at {:?}", p.to_array()))),
  }
}

/// {"body", "edge"} or {"point": [x,y,z]} on the edge.
pub(crate) fn edge_ref_of(s: &Session, e: &Value) -> Result<EdgeRef, CmdError> {
  let (body, edge) = if e.get("point").is_some() {
    edge_at(s, DVec3::from(req_vec3(e, "point")?))?
  } else {
    (BodyId(req_u32(e, "body")?), req_u32(e, "edge")?)
  };
  let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
  let info = b.edges().iter().find(|x| x.id == edge).ok_or(CmdError::new(format!("body has no edge {edge}")))?;
  Ok(EdgeRef::from_edge(body, info, b.faces()))
}

pub(crate) fn edge_refs(s: &Session, p: &Value) -> Result<Vec<EdgeRef>, CmdError> {
  let list = p.get("edges").and_then(Value::as_array).ok_or(CmdError::new("missing 'edges'"))?;
  let mut out = Vec::new();
  for e in list {
    out.push(edge_ref_of(s, e)?);
  }
  if out.is_empty() {
    return Err(CmdError::new("no edges given"));
  }
  Ok(out)
}

pub(crate) fn axis_ref(s: &Session, v: &Value) -> Result<AxisRef, CmdError> {
  if let Some(n) = v.as_str() {
    return Ok(match n.to_ascii_lowercase().as_str() {
      "x" => AxisRef::X,
      "y" => AxisRef::Y,
      "z" => AxisRef::Z,
      o => return Err(CmdError::new(format!("unknown axis '{o}'"))),
    });
  }
  if let Some(c) = v.get("line").and_then(Value::as_u64) {
    return Ok(AxisRef::SketchLine { curve: CurveId(u32::try_from(c).map_err(|_| CmdError::new("bad curve"))?) });
  }
  if let Some(f) = v.get("axis").and_then(Value::as_u64) {
    let feature = FeatureId(u32::try_from(f).map_err(|_| CmdError::new("bad axis"))?);
    if !s.model.axes.contains_key(&feature) {
      return Err(CmdError::new("that feature is not a construction axis"));
    }
    return Ok(AxisRef::Construction { feature });
  }
  let body = BodyId(req_u32(v, "body")?);
  let edge = req_u32(v, "edge")?;
  let b = s.model.body(body).ok_or(CmdError::new("no such body"))?;
  let info = b.edges().iter().find(|x| x.id == edge).ok_or(CmdError::new("no such edge"))?;
  Ok(AxisRef::Edge { edge: EdgeRef::from_edge(body, info, b.faces()) })
}

/// The sketch a draw/constraint command targets: `sketch` param or the one being edited.
fn sketch_id(s: &Session, p: &Value) -> Result<FeatureId, CmdError> {
  let id = match p.get("sketch").and_then(Value::as_u64) {
    Some(v) => FeatureId(u32::try_from(v).map_err(|_| CmdError::new("bad sketch id"))?),
    None => s.edit_sketch.ok_or(CmdError::new("no sketch is being edited (sketch.create or pass 'sketch')"))?,
  };
  s.design.sketch(id)?;
  Ok(id)
}

/// The sketch an extrude/revolve uses: `sketch` param, the edited sketch, or the last sketch.
pub(crate) fn sketch_for_feature(s: &mut Session, p: &Value) -> Result<FeatureId, CmdError> {
  let id = match p.get("sketch").and_then(Value::as_u64) {
    Some(v) => FeatureId(u32::try_from(v).map_err(|_| CmdError::new("bad sketch id"))?),
    None => match s.edit_sketch {
      Some(e) => e,
      None => s
        .design
        .features
        .iter()
        .take(s.design.active_len())
        .rev()
        .find(|f| matches!(f.kind, FeatureKind::Sketch { .. }))
        .map(|f| f.id)
        .ok_or(CmdError::new("there is no sketch to use"))?,
    },
  };
  s.design.sketch(id)?;
  // Fusion finishes the sketch when you start a feature from it.
  s.edit_sketch = None;
  Ok(id)
}

/// `profiles`: absent/"all" → all; list of [x,y] interior points or region indices.
pub(crate) fn profile_points(s: &Session, sketch: FeatureId, p: &Value) -> Result<Vec<[f64; 2]>, CmdError> {
  let Some(list) = p.get("profiles").and_then(Value::as_array) else { return Ok(Vec::new()) };
  let regions = s.model.sketches.get(&sketch).map(|r| r.profiles.clone()).unwrap_or_default();
  let mut out = Vec::new();
  for v in list {
    if let Some(i) = v.as_u64() {
      let r = regions.get(i as usize).ok_or(CmdError::new(format!("the sketch has no profile {i}")))?;
      out.push(r.interior_point());
    } else {
      out.push(pt_of(v).ok_or(CmdError::new("profiles are region indices or [x, y] points"))?);
    }
  }
  Ok(out)
}

// ---------------------------------------------------------------- sketch modify

fn modify(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let scope = params_scope(s);
  let curves: Vec<CurveId> = ids(p, "curves").into_iter().map(CurveId).collect();
  let num =
    |k: &str, kind: fox_expr::Kind| -> Result<f64, CmdError> { Ok(fox_expr::eval(&req_expr(p, k)?, kind, &scope)?) };
  let count = |k: &str| -> Result<usize, CmdError> {
    p.get(k).and_then(Value::as_u64).map(|v| v as usize).ok_or(CmdError::new(format!("missing '{k}'")))
  };
  enum Op {
    Fillet(f64),
    Chamfer(f64, f64),
    Offset(f64),
    Trim(Option<CurveId>, [f64; 2]),
    Extend(Option<CurveId>, [f64; 2]),
    Mirror(CurveId),
    Rect(Box<RectPattern>),
    Circ([f64; 2], usize, f64, bool),
  }
  let curve = p.get("curve").and_then(Value::as_u64).map(|c| CurveId(c as u32));
  let op = match name {
    "sketch.fillet" => Op::Fillet(num("radius", fox_expr::Kind::Length)?),
    "sketch.chamfer" => {
      let d = num("distance", fox_expr::Kind::Length)?;
      let d2 = opt_expr(p, "distance2").map(|e| fox_expr::eval(&e, fox_expr::Kind::Length, &scope)).transpose()?;
      Op::Chamfer(d, d2.unwrap_or(d))
    }
    "sketch.offset" => Op::Offset(num("distance", fox_expr::Kind::Length)?),
    "sketch.trim" => Op::Trim(curve, req_pt(p, "at")?),
    "sketch.extend" => Op::Extend(curve, req_pt(p, "at")?),
    "sketch.mirror" => Op::Mirror(CurveId(req_u32(p, "line")?)),
    "sketch.pattern_rect" => Op::Rect(Box::new(rect_pattern(p, &scope)?)),
    _ => Op::Circ(
      opt_pt(p, "center")?.unwrap_or([0.0, 0.0]),
      count("count")?,
      opt_expr(p, "angle")
        .map(|e| fox_expr::eval(&e, fox_expr::Kind::Angle, &scope))
        .transpose()?
        .unwrap_or(std::f64::consts::TAU),
      p.get("symmetric").and_then(Value::as_bool).unwrap_or(false),
    ),
  };
  let made = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    let pick = |sketch: &Sketch, c: Option<CurveId>, at: [f64; 2]| -> Result<CurveId, CmdError> {
      match c {
        Some(c) => Ok(c),
        None => sketch.nearest_curve(at, 1.0).ok_or(CmdError::new("no curve there")),
      }
    };
    let made = match op {
      Op::Fillet(r) => {
        let [a, b] = curves.as_slice() else { return Err(CmdError::new("a fillet needs two lines")) };
        vec![sketch.fillet(*a, *b, r)?]
      }
      Op::Chamfer(d1, d2) => {
        let [a, b] = curves.as_slice() else { return Err(CmdError::new("a chamfer needs two lines")) };
        vec![sketch.chamfer(*a, *b, d1, d2)?]
      }
      Op::Offset(dist) => sketch.offset(&curves, dist)?,
      Op::Trim(c, at) => {
        let c = pick(sketch, c, at)?;
        sketch.trim(c, at)?
      }
      Op::Extend(c, at) => {
        let c = pick(sketch, c, at)?;
        sketch.extend(c, at)?;
        vec![c]
      }
      Op::Mirror(line) => sketch.mirror(&curves, line)?,
      Op::Rect(r) => {
        let RectPattern { mut a, mut b, line_a, line_b } = *r;
        if let Some(l) = line_a {
          a.direction = sketch.line_direction(l)?;
        }
        if let Some(l) = line_b {
          b.direction = sketch.line_direction(l)?;
        }
        sketch.pattern_rect(&curves, a, b)?
      }
      Op::Circ(c, n, a, sym) => sketch.pattern_circ(&curves, c, n, a, sym)?,
    };
    if !sketch.solve().ok() {
      return Err(CmdError::new("the result conflicts with the sketch's constraints"));
    }
    Ok(made)
  })?;
  let profiles = s.model.sketches.get(&sk).map(|r| r.profiles.len()).unwrap_or(0);
  Ok(json!({"curves": made.iter().map(|c| c.0).collect::<Vec<_>>(), "profiles": profiles}))
}

/// A parsed `sketch.pattern_rect`: its two directions, each either a vector or a sketch line.
struct RectPattern {
  a: fox_sketch::PatternDir,
  b: fox_sketch::PatternDir,
  line_a: Option<CurveId>,
  line_b: Option<CurveId>,
}

/// Parses `sketch.pattern_rect` as Fusion's sketch Rectangular Pattern sets it up: each direction
/// is an [x, y] vector or a line id (start to end); the first defaults to +X and the second to +Y
/// (as before these options existed). `distance_type` "extent" makes `spacing`/`spacing2` the
/// whole pattern's size; `symmetric`/`symmetric2` centre that direction's instances on the original.
fn rect_pattern(p: &Value, scope: &std::collections::HashMap<String, f64>) -> Result<RectPattern, CmdError> {
  let extent = match opt_str(p, "distance_type").unwrap_or("spacing") {
    "spacing" => false,
    "extent" => true,
    o => return Err(CmdError::new(format!("unknown distance type '{o}' (spacing or extent)"))),
  };
  let dir = |k: &str, default: [f64; 2]| -> Result<([f64; 2], Option<CurveId>), CmdError> {
    match p.get(k) {
      Some(v) if v.is_u64() => Ok(([0.0, 0.0], Some(CurveId(req_u32(p, k)?)))),
      _ => Ok((opt_pt(p, k)?.unwrap_or(default), None)),
    }
  };
  let count = |k: &str, default: Option<u64>| -> Result<usize, CmdError> {
    match p.get(k) {
      None => default.map(|d| d as usize).ok_or(CmdError::new(format!("missing '{k}'"))),
      Some(v) => v.as_u64().map(|v| v as usize).ok_or(CmdError::new(format!("'{k}' must be a whole number"))),
    }
  };
  let length = |k: &str| -> Result<Option<f64>, CmdError> {
    match p.get(k) {
      None => Ok(None),
      Some(v) => Ok(Some(fox_expr::eval(&expr_of(v)?, fox_expr::Kind::Length, scope)?)),
    }
  };
  let flag = |k: &str| -> Result<bool, CmdError> {
    match p.get(k) {
      None => Ok(false),
      Some(v) => v.as_bool().ok_or(CmdError::new(format!("'{k}' must be true or false"))),
    }
  };
  let (d1, line_a) = dir("direction", [1.0, 0.0])?;
  // Without `direction2` the second direction is +Y, whichever way the first points.
  let (d2, line_b) = dir("direction2", [0.0, 1.0])?;
  let a = fox_sketch::PatternDir {
    direction: d1,
    count: count("count", None)?,
    distance: length("spacing")?.ok_or(CmdError::new("missing 'spacing'"))?,
    extent,
    symmetric: flag("symmetric")?,
  };
  let b = fox_sketch::PatternDir {
    direction: d2,
    count: count("count2", Some(1))?,
    distance: length("spacing2")?.unwrap_or(0.0),
    extent,
    symmetric: flag("symmetric2")?,
  };
  Ok(RectPattern { a, b, line_a, line_b })
}

/// Projects body edges onto the sketch plane as fixed geometry. Straight and circular edges stay
/// linked to the body (they move when it changes) unless `associative` is false.
fn project(s: &mut Session, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let plane = s.model.sketches.get(&sk).map(|r| r.plane).ok_or(CmdError::new("the sketch failed to build"))?;
  let body = s.model.body(BodyId(req_u32(p, "body")?)).ok_or(CmdError::new("no such body"))?;
  let face = p.get("face").and_then(Value::as_u64).map(|f| f as u32);
  let associative = p.get("associative").and_then(Value::as_bool).unwrap_or(true);
  let n = plane.normal();
  let lines = body.shape.edge_lines((body.diagonal() * 0.001).clamp(0.005, 0.2))?;
  enum E {
    Line([f64; 2], [f64; 2]),
    Circle([f64; 2], f64),
    Arc([f64; 2], [f64; 2], [f64; 2]),
  }
  let mut todo: Vec<(E, Option<EdgeRef>)> = Vec::new();
  for e in body.edges().iter().filter(|e| face.is_none_or(|f| e.faces.contains(&f))) {
    let l = |v: fox_kernel::glam::DVec3| plane.to_local(v);
    let link = associative.then(|| EdgeRef::from_edge(body.id, e, body.faces()));
    match e.kind {
      fox_kernel::CurveKind::Line => todo.push((E::Line(l(e.start), l(e.end)), link)),
      fox_kernel::CurveKind::Circle if e.axis.dot(n).abs() > 0.999 => {
        if (e.start - e.end).length() < 1e-6 {
          todo.push((E::Circle(l(e.center), e.radius), link));
        } else {
          todo.push((E::Arc(l(e.start), l(e.mid), l(e.end)), link));
        }
      }
      fox_kernel::CurveKind::Degenerate => {}
      _ => {
        if let Some(pl) = lines.iter().find(|x| x.id == e.id) {
          for w in pl.points.windows(2) {
            todo.push((E::Line(l(w[0]), l(w[1])), None));
          }
        }
      }
    }
  }
  let made = s.edit(move |d| {
    let mut new_links = Vec::new();
    let sketch = d.sketch_mut(sk)?;
    let before: Vec<PointId> = sketch.points.iter().map(|p| p.id).collect();
    let at = |sketch: &Sketch, q: [f64; 2]| match sketch.nearest_point(q, 1e-6, &[]) {
      Some(id) if !before.contains(&id) => PtSpec::Point(id),
      _ => PtSpec::At(q[0], q[1]),
    };
    let mut made = Vec::new();
    for (e, link) in &todo {
      let id = match *e {
        E::Line(a, b) => {
          if (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-6 {
            continue;
          }
          let (sa, sb) = (at(sketch, a), at(sketch, b));
          sketch.add_line(sa, sb)?.0
        }
        E::Circle(c, r) => {
          let sc = at(sketch, c);
          let id = sketch.add_circle(sc, r)?;
          sketch.add_constraint(ConstraintKind::Radius { curve: id, value: Dim::new(r) })?;
          id
        }
        E::Arc(a, m, b) => {
          let (sa, sb) = (at(sketch, a), at(sketch, b));
          sketch.add_arc_3pt(sa, m, sb)?
        }
      };
      made.push(id);
      if let Some(edge) = link {
        new_links.push(fox_model::SketchLink { curve: id, edge: edge.clone() });
      }
    }
    for pt in sketch.points.iter_mut().filter(|p| !before.contains(&p.id)) {
      pt.fixed = true;
    }
    if let Some(FeatureKind::Sketch { links, .. }) = d.feature_mut(sk).map(|f| &mut f.kind) {
      links.extend(new_links);
    }
    Ok(made)
  })?;
  let profiles = s.model.sketches.get(&sk).map(|r| r.profiles.len()).unwrap_or(0);
  Ok(json!({"curves": made.iter().map(|c| c.0).collect::<Vec<_>>(), "profiles": profiles}))
}

// ---------------------------------------------------------------- sketch drawing

fn draw(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let scope = params_scope(s);
  let name = name.to_string();
  let p = p.clone();
  let created = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    let before: Vec<CurveId> = sketch.curves.iter().map(|c| c.id).collect();
    let snap = |sketch: &Sketch, at: [f64; 2]| -> PtSpec {
      match sketch.nearest_point(at, SNAP, &[]) {
        Some(id) => PtSpec::Point(id),
        None => PtSpec::At(at[0], at[1]),
      }
    };
    match name.as_str() {
      "sketch.line" if p.get("mid").is_some() => {
        // Fusion's Midpoint Line: the other end mirrors `end` through `mid`, and the midpoint
        // (an existing point it lands on, such as the origin, or a new one) stays on the middle
        let (m, e) = (req_pt(&p, "mid")?, req_pt(&p, "end")?);
        if (e[0] - m[0]).hypot(e[1] - m[1]) < 1e-9 {
          return Err(CmdError::new("the end is on the midpoint"));
        }
        let mid = snap(sketch, m);
        let a = [2.0 * m[0] - e[0], 2.0 * m[1] - e[1]];
        let (a, b) = (snap(sketch, a), snap(sketch, e));
        let (line, _) = sketch.add_line(a, b)?;
        let mid = sketch.resolve(mid)?;
        auto_hv(sketch, line);
        sketch.add_constraint(ConstraintKind::Midpoint { p: mid, line })?;
      }
      "sketch.line" => {
        let pts: Vec<[f64; 2]> = p
          .get("points")
          .and_then(Value::as_array)
          .ok_or(CmdError::new("missing 'points'"))?
          .iter()
          .map(|v| pt_of(v).ok_or(CmdError::new("points must be [x, y]")))
          .collect::<Result<_, _>>()?;
        if pts.len() < 2 || pts.len() > 10_000 {
          return Err(CmdError::new("a line needs 2 or more points"));
        }
        let closed = p.get("closed").and_then(Value::as_bool).unwrap_or(false);
        let mut first: Option<PointId> = None;
        let mut prev: Option<PointId> = None;
        let n = pts.len();
        for i in 1..n + usize::from(closed) {
          let a = match prev {
            Some(id) => PtSpec::Point(id),
            None => snap(sketch, pts[i - 1]),
          };
          let b = if closed && i == n {
            PtSpec::Point(first.ok_or(CmdError::new("bad polyline"))?)
          } else {
            snap(sketch, pts[i])
          };
          let (line, [pa, pb]) = sketch.add_line(a, b)?;
          first.get_or_insert(pa);
          prev = Some(pb);
          auto_hv(sketch, line);
        }
      }
      "sketch.rect" if p.get("points").is_some() => {
        let pts = pts_of(&p, "points")?;
        let [a, b, c] = pts.as_slice() else { return Err(CmdError::new("a 3-point rectangle needs 3 points")) };
        let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
        let l = ux.hypot(uy);
        if l < 1e-9 {
          return Err(CmdError::new("the first two points are the same"));
        }
        // the third point sets the height across the first side
        let (nx, ny) = (-uy / l, ux / l);
        let h = (c[0] - a[0]) * nx + (c[1] - a[1]) * ny;
        if h.abs() < 1e-9 {
          return Err(CmdError::new("the rectangle has no height"));
        }
        let corners = [*a, *b, [b[0] + nx * h, b[1] + ny * h], [a[0] + nx * h, a[1] + ny * h]];
        let mut ids = Vec::new();
        for q in corners {
          let spec = snap(sketch, q);
          ids.push(sketch.resolve(spec)?);
        }
        let mut lines = Vec::new();
        for i in 0..4 {
          lines.push(sketch.add_line(PtSpec::Point(ids[i]), PtSpec::Point(ids[(i + 1) % 4]))?.0);
        }
        for (x, y) in [(0, 1), (1, 2), (2, 3)] {
          sketch.add_constraint(ConstraintKind::Perpendicular { a: lines[x], b: lines[y] })?;
        }
        if (uy / l).abs() < 1e-12 || (ux / l).abs() < 1e-12 {
          auto_hv(sketch, lines[0]);
        }
      }
      "sketch.rect" => {
        if let Some(c) = opt_pt(&p, "center")? {
          let corner = req_pt(&p, "corner")?;
          let cs = snap(sketch, c);
          sketch.add_center_rectangle(cs, corner)?;
        } else {
          let lines = sketch.add_rectangle(req_pt(&p, "corner1")?, req_pt(&p, "corner2")?)?;
          // a corner landing on an existing point (e.g. the origin) becomes coincident
          let corners: Vec<PointId> = lines.iter().filter_map(|l| sketch.curve_points(*l).first().copied()).collect();
          for c in corners {
            if let Some(pos) = sketch.pos(c)
              && let Some(other) = sketch.nearest_point(pos, SNAP, &[c])
            {
              sketch.add_constraint(ConstraintKind::Coincident { a: c, b: other })?;
            }
          }
        }
      }
      "sketch.circle" if p.get("through").is_some() => {
        let pts = pts_of(&p, "through")?;
        let (c, r) = match pts.as_slice() {
          [a, b] => ([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], (a[0] - b[0]).hypot(a[1] - b[1]) / 2.0),
          [a, b, c] => {
            let o = fox_sketch::circumcenter(*a, *b, *c).ok_or(CmdError::new("the three points are in a line"))?;
            (o, (a[0] - o[0]).hypot(a[1] - o[1]))
          }
          _ => return Err(CmdError::new("'through' needs 2 or 3 points")),
        };
        let cs = snap(sketch, c);
        let id = sketch.add_circle(cs, r)?;
        // the circle passes through the clicked points that landed on existing ones
        for q in pts {
          if let Some(other) = sketch.nearest_point(q, SNAP, &[]) {
            sketch.add_constraint(ConstraintKind::PointOnCurve { p: other, curve: id })?;
          }
        }
      }
      "sketch.circle" => {
        let c = req_pt(&p, "center")?;
        // a number draws the circle; an expression ("hole", "d/2") also adds a driving dimension
        let (key, raw) = match (p.get("radius"), p.get("diameter")) {
          (Some(v), _) => ("radius", v.clone()),
          (None, Some(v)) => ("diameter", v.clone()),
          _ => return Err(CmdError::new("missing 'radius' or 'diameter'")),
        };
        let (value, dim) = match &raw {
          Value::String(e) => (fox_expr::eval(e, fox_expr::Kind::Length, &scope)?, Some(e.clone())),
          v => (v.as_f64().ok_or(CmdError::new("radius/diameter must be a number or expression"))?, None),
        };
        let r = if key == "radius" { value } else { value / 2.0 };
        let cs = snap(sketch, c);
        let id = sketch.add_circle(cs, r)?;
        if let Some(e) = dim {
          let d = Dim::expr(e, value);
          let kind = if key == "radius" {
            ConstraintKind::Radius { curve: id, value: d }
          } else {
            ConstraintKind::Diameter { curve: id, value: d }
          };
          sketch.add_constraint(kind)?;
        }
      }
      "sketch.arc" if p.get("tangent").is_some() => {
        let curve = CurveId(req_u32(&p, "tangent")?);
        let (st, en) = (req_pt(&p, "start")?, req_pt(&p, "end")?);
        let dir = match sketch.curve(curve).map(|c| c.kind.clone()) {
          Some(CurveKind::Line { a, b }) => {
            let (pa, pb) = (sketch.pos(a).unwrap_or_default(), sketch.pos(b).unwrap_or_default());
            // leave the line's end the arc starts at, carrying on its direction
            let at_b = (pb[0] - st[0]).hypot(pb[1] - st[1]) <= (pa[0] - st[0]).hypot(pa[1] - st[1]);
            if at_b { [pb[0] - pa[0], pb[1] - pa[1]] } else { [pa[0] - pb[0], pa[1] - pb[1]] }
          }
          Some(CurveKind::Arc { center, .. }) => {
            let c = sketch.pos(center).unwrap_or_default();
            // the arc's tangent at that end (counter-clockwise), pointing away from it
            let r = [st[0] - c[0], st[1] - c[1]];
            let ccw = [-r[1], r[0]];
            let ends = sketch.curve_points(curve);
            let at_end =
              ends.get(2).and_then(|e| sketch.pos(*e)).is_some_and(|e| (e[0] - st[0]).hypot(e[1] - st[1]) < 1e-6);
            if at_end { ccw } else { [-ccw[0], -ccw[1]] }
          }
          _ => return Err(CmdError::new("a tangent arc starts at the end of a line or arc")),
        };
        let l = dir[0].hypot(dir[1]);
        if l < 1e-12 {
          return Err(CmdError::new("the curve has no direction there"));
        }
        let n = [-dir[1] / l, dir[0] / l];
        let chord = [en[0] - st[0], en[1] - st[1]];
        let den = 2.0 * (chord[0] * n[0] + chord[1] * n[1]);
        if den.abs() < 1e-12 {
          return Err(CmdError::new("the end is straight ahead: draw a line instead"));
        }
        // centre on the start's normal, equidistant from both ends
        let t = (chord[0] * chord[0] + chord[1] * chord[1]) / den;
        let c = [st[0] + n[0] * t, st[1] + n[1] * t];
        let ss = snap(sketch, st);
        let ccw = t > 0.0;
        let arc = if ccw {
          sketch.add_arc_center(PtSpec::At(c[0], c[1]), ss, en)?
        } else {
          let es = snap(sketch, en);
          let a = sketch.add_arc_center(PtSpec::At(c[0], c[1]), es, st)?;
          // the start landed on a fresh point: tie it to the curve's end
          if let Some(e) = sketch.curve_points(a).get(2).copied()
            && let Some(other) = sketch.nearest_point(st, SNAP, &[e])
          {
            sketch.add_constraint(ConstraintKind::Coincident { a: e, b: other })?;
          }
          a
        };
        sketch.add_constraint(ConstraintKind::Tangent { a: curve, b: arc })?;
      }
      "sketch.arc" => {
        if let Some(c) = opt_pt(&p, "center")? {
          let st = req_pt(&p, "start")?;
          // Fusion's typed sweep angle: signed, negative runs clockwise
          let (en, clockwise) = match opt_expr(&p, "sweep") {
            Some(e) => {
              if p.get("end").is_some() {
                return Err(CmdError::new("give the arc's 'end' or its 'sweep', not both"));
              }
              let sweep = fox_expr::eval(&e, fox_expr::Kind::Angle, &scope)?;
              if !(sweep.abs() > 1e-9 && sweep.abs() < std::f64::consts::TAU - 1e-9) {
                return Err(CmdError::new("an arc sweeps more than 0 and less than 360 degrees"));
              }
              let a = (st[1] - c[1]).atan2(st[0] - c[0]) + sweep;
              ([c[0] + a.cos(), c[1] + a.sin()], sweep < 0.0)
            }
            None => (req_pt(&p, "end")?, p.get("clockwise").and_then(Value::as_bool).unwrap_or(false)),
          };
          let (cs, ss) = (snap(sketch, c), snap(sketch, st));
          sketch.add_arc_center_dir(cs, ss, en, clockwise)?;
        } else {
          let (st, mid, en) = (req_pt(&p, "start")?, req_pt(&p, "mid")?, req_pt(&p, "end")?);
          let (ss, es) = (snap(sketch, st), snap(sketch, en));
          sketch.add_arc_3pt(ss, mid, es)?;
        }
      }
      "sketch.polygon" => {
        let sides = p.get("sides").and_then(Value::as_u64).unwrap_or(6) as usize;
        let n = sides.max(3) as f64;
        let (c, vertex) = if p.get("edge").is_some() {
          // from one edge: the centre is on its left at the apothem
          let e = pts_of(&p, "edge")?;
          let [a, b] = e.as_slice() else { return Err(CmdError::new("'edge' needs two points")) };
          let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
          let side = dx.hypot(dy);
          let apothem = side / (2.0 * (std::f64::consts::PI / n).tan());
          let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
          ([m[0] - dy / side * apothem, m[1] + dx / side * apothem], *a)
        } else {
          let c = req_pt(&p, "center")?;
          let v = req_pt(&p, "vertex")?;
          if p.get("circumscribed").and_then(Value::as_bool).unwrap_or(false) {
            // the clicked point is an edge midpoint: turn half a side and reach the corner
            let half = std::f64::consts::PI / n;
            let (dx, dy) = ((v[0] - c[0]) / half.cos(), (v[1] - c[1]) / half.cos());
            (c, [c[0] + dx * half.cos() - dy * half.sin(), c[1] + dx * half.sin() + dy * half.cos()])
          } else {
            (c, v)
          }
        };
        let cs = snap(sketch, c);
        sketch.add_polygon(cs, vertex, sides)?;
      }
      "sketch.spline" => {
        let pts: Vec<[f64; 2]> = p
          .get("points")
          .and_then(Value::as_array)
          .ok_or(CmdError::new("missing 'points'"))?
          .iter()
          .map(|v| pt_of(v).ok_or(CmdError::new("points must be [x, y]")))
          .collect::<Result<_, _>>()?;
        if pts.len() < 2 {
          return Err(CmdError::new("a spline needs 2 or more points"));
        }
        let mut ids = Vec::new();
        for q in pts {
          let spec = snap(sketch, q);
          ids.push(sketch.resolve(spec)?);
        }
        sketch.add_curve(CurveKind::spline(ids))?;
      }
      "sketch.slot" if p.get("arc_center").is_some() || p.get("mid").is_some() => {
        let w = p.get("width").and_then(Value::as_f64).ok_or(CmdError::new("missing 'width'"))?;
        let (st, en) = (req_pt(&p, "start")?, req_pt(&p, "end")?);
        let (c, ccw) = match opt_pt(&p, "arc_center")? {
          // Fusion takes the shorter arc from start to end; 'ccw' picks the direction outright
          Some(c) => {
            let cross = (st[0] - c[0]) * (en[1] - c[1]) - (st[1] - c[1]) * (en[0] - c[0]);
            (c, p.get("ccw").and_then(Value::as_bool).unwrap_or(cross >= 0.0))
          }
          None => {
            let m = req_pt(&p, "mid")?;
            let c = fox_sketch::circumcenter(st, m, en).ok_or(CmdError::new("the three points are in a line"))?;
            // the arc through the middle point runs counter-clockwise from start to end when that
            // point is on the right of the chord from start to end
            let cross = (en[0] - st[0]) * (m[1] - st[1]) - (en[1] - st[1]) * (m[0] - st[0]);
            (c, cross < 0.0)
          }
        };
        // the start sets the radius; the end only sets the direction
        let r = (st[0] - c[0]).hypot(st[1] - c[1]);
        let re = (en[0] - c[0]).hypot(en[1] - c[1]);
        if re < 1e-12 {
          return Err(CmdError::new("the end is on the slot's centre"));
        }
        let (a0, a1) = (st, [c[0] + (en[0] - c[0]) * r / re, c[1] + (en[1] - c[1]) * r / re]);
        let (from, to) = if ccw { (a0, a1) } else { (a1, a0) };
        add_arc_slot(sketch, c, from, to, w)?;
      }
      "sketch.slot" => {
        let w = p.get("width").and_then(Value::as_f64).ok_or(CmdError::new("missing 'width'"))?;
        let (a, b) = match (opt_pt(&p, "center")?, opt_str(&p, "kind").unwrap_or("center_to_center")) {
          (Some(c), _) => {
            let e = req_pt(&p, "end")?;
            ([2.0 * c[0] - e[0], 2.0 * c[1] - e[1]], e)
          }
          (None, "overall") => {
            let (a, b) = (req_pt(&p, "start")?, req_pt(&p, "end")?);
            let l = (b[0] - a[0]).hypot(b[1] - a[1]);
            if l <= w {
              return Err(CmdError::new("the slot is shorter than its width"));
            }
            let k = w / 2.0 / l;
            let d = [(b[0] - a[0]) * k, (b[1] - a[1]) * k];
            ([a[0] + d[0], a[1] + d[1]], [b[0] - d[0], b[1] - d[1]])
          }
          _ => (req_pt(&p, "start")?, req_pt(&p, "end")?),
        };
        sketch.add_slot(a, b, w)?;
      }
      "sketch.ellipse" => {
        let c = req_pt(&p, "center")?;
        let m = req_pt(&p, "major")?;
        let minor = p.get("minor").and_then(Value::as_f64).ok_or(CmdError::new("missing 'minor'"))?;
        let cs = snap(sketch, c);
        let center = sketch.resolve(cs)?;
        let major = sketch.add_point(m[0], m[1]);
        sketch.add_curve(CurveKind::Ellipse { center, major, minor })?;
      }
      "sketch.control_spline" => {
        let pts = pts_of(&p, "points")?;
        if pts.len() < 2 {
          return Err(CmdError::new("a control point spline needs 2 or more points"));
        }
        let mut ids = Vec::new();
        for (i, q) in pts.iter().enumerate() {
          // only the ends snap to existing points: the inner control points float off the curve
          let spec = if i == 0 || i + 1 == pts.len() { snap(sketch, *q) } else { PtSpec::At(q[0], q[1]) };
          ids.push(sketch.resolve(spec)?);
        }
        sketch.add_curve(CurveKind::ControlSpline { points: ids })?;
      }
      "sketch.conic" => {
        let (st, en, apex) = (req_pt(&p, "start")?, req_pt(&p, "end")?, req_pt(&p, "apex")?);
        let rho = p.get("rho").and_then(Value::as_f64).unwrap_or(0.5);
        if !fox_sketch::RHO_RANGE.contains(&rho) {
          return Err(CmdError::new("rho must be between 0.01 and 0.99"));
        }
        let cross = (en[0] - st[0]) * (apex[1] - st[1]) - (en[1] - st[1]) * (apex[0] - st[0]);
        if cross.abs() <= 1e-9 * (en[0] - st[0]).hypot(en[1] - st[1]).powi(2) {
          return Err(CmdError::new("the apex must lie off the line from start to end"));
        }
        // the exact conic: the rational quadratic Bézier start, apex, end (rho 0.5 is a parabola)
        let (a, b) = (snap(sketch, st), snap(sketch, en));
        let (start, end) = (sketch.resolve(a)?, sketch.resolve(b)?);
        // the apex floats off the curve, like a control point: it never snaps
        let apex = sketch.add_point(apex[0], apex[1]);
        sketch.add_curve(CurveKind::Conic { start, end, apex, rho })?;
      }
      _ => {
        let at = req_pt(&p, "at")?;
        sketch.add_point(at[0], at[1]);
      }
    }
    let r = sketch.solve();
    if !r.ok() {
      return Err(CmdError::new("the new geometry conflicts with existing constraints"));
    }
    Ok(sketch.curves.iter().map(|c| c.id).filter(|c| !before.contains(c)).collect::<Vec<_>>())
  })?;
  let profiles = s.model.sketches.get(&sk).map(|r| r.profiles.len()).unwrap_or(0);
  Ok(json!({"curves": created.iter().map(|c| c.0).collect::<Vec<_>>(), "profiles": profiles}))
}

fn pts_of(p: &Value, k: &str) -> Result<Vec<[f64; 2]>, CmdError> {
  p.get(k)
    .and_then(Value::as_array)
    .ok_or_else(|| CmdError::new(format!("missing '{k}'")))?
    .iter()
    .map(|v| pt_of(v).ok_or(CmdError::new("points must be [x, y]")))
    .collect()
}

/// A slot along an arc: centre `c`, counter-clockwise from `from` to `to` (both on the centre
/// line), `w` wide: outer and inner arcs joined by round caps.
fn add_arc_slot(sketch: &mut Sketch, c: [f64; 2], from: [f64; 2], to: [f64; 2], w: f64) -> Result<(), CmdError> {
  let r = (from[0] - c[0]).hypot(from[1] - c[1]);
  if w <= 0.0 || w / 2.0 >= r {
    return Err(CmdError::new("the slot is wider than its arc's radius allows"));
  }
  let (a0, a1) = ((from[1] - c[1]).atan2(from[0] - c[0]), (to[1] - c[1]).atan2(to[0] - c[0]));
  let at = |radius: f64, a: f64| [c[0] + radius * a.cos(), c[1] + radius * a.sin()];
  let (ro, ri) = (r + w / 2.0, r - w / 2.0);
  let pt = |s: &mut Sketch, q: [f64; 2]| s.add_point(q[0], q[1]);
  let (os, oe, is, ie) =
    (pt(sketch, at(ro, a0)), pt(sketch, at(ro, a1)), pt(sketch, at(ri, a0)), pt(sketch, at(ri, a1)));
  let (cc, c0, c1) = (pt(sketch, c), pt(sketch, at(r, a0)), pt(sketch, at(r, a1)));
  // outer arc forwards, the end cap round the far side, inner arc back (stored ccw), start cap
  let outer = sketch.add_curve(CurveKind::Arc { center: cc, start: os, end: oe })?;
  let cap1 = sketch.add_curve(CurveKind::Arc { center: c1, start: oe, end: ie })?;
  let inner = sketch.add_curve(CurveKind::Arc { center: cc, start: is, end: ie })?;
  let cap0 = sketch.add_curve(CurveKind::Arc { center: c0, start: is, end: os })?;
  for (a, b) in [(outer, cap1), (cap1, inner), (inner, cap0), (cap0, outer)] {
    sketch.add_constraint(ConstraintKind::Tangent { a, b })?;
  }
  sketch.add_constraint(ConstraintKind::Concentric { a: outer, b: inner })?;
  Ok(())
}

/// Exactly horizontal/vertical new lines get the matching constraint (Fusion's auto-constraints).
fn auto_hv(sketch: &mut Sketch, line: CurveId) {
  let pts = sketch.curve_points(line);
  let (Some(a), Some(b)) = (pts.first().and_then(|p| sketch.pos(*p)), pts.get(1).and_then(|p| sketch.pos(*p))) else {
    return;
  };
  let kind = if (a[1] - b[1]).abs() < 1e-9 {
    ConstraintKind::Horizontal { line }
  } else if (a[0] - b[0]).abs() < 1e-9 {
    ConstraintKind::Vertical { line }
  } else {
    return;
  };
  let _ = sketch.add_constraint(kind);
}

fn constrain(s: &mut Session, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let t = req_str(p, "type")?.to_string();
  let pts: Vec<PointId> = ids(p, "points").into_iter().map(PointId).collect();
  let cs: Vec<CurveId> = ids(p, "curves").into_iter().map(CurveId).collect();
  let ids = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    // fixing curves locks every point that defines them (and a circle's radius)
    if t == "fix" && pts.is_empty() && !cs.is_empty() {
      let mut added = Vec::new();
      for c in &cs {
        added.extend(sketch.fix_curve(*c)?);
      }
      let r = sketch.solve();
      if !r.ok() {
        return Err(CmdError::new("that constraint conflicts with the others (or over-constrains the sketch)"));
      }
      return Ok((added, None));
    }
    let need_p = |n: usize| -> Result<(), CmdError> {
      if pts.len() >= n { Ok(()) } else { Err(CmdError::new(format!("'{t}' needs {n} point(s)"))) }
    };
    let need_c = |n: usize| -> Result<(), CmdError> {
      if cs.len() >= n { Ok(()) } else { Err(CmdError::new(format!("'{t}' needs {n} curve(s)"))) }
    };
    let conflict = || CmdError::new("that constraint conflicts with the others (or over-constrains the sketch)");
    if t == "polygon" {
      // lines already forming a regular polygon; the constraint adds their centre point
      need_c(3)?;
      let (id, center) = sketch.add_polygon_constraint(&cs)?;
      return if sketch.solve().ok() { Ok((vec![id], Some(center))) } else { Err(conflict()) };
    }
    let kind = match t.as_str() {
      "coincident" => {
        need_p(2)?;
        ConstraintKind::Coincident { a: pts[0], b: pts[1] }
      }
      "fix" => {
        need_p(1)?;
        let pos = sketch.pos(pts[0]).ok_or(CmdError::new("no such point"))?;
        ConstraintKind::Fix { p: pts[0], x: pos[0], y: pos[1] }
      }
      "horizontal" if pts.len() >= 2 => ConstraintKind::HorizontalPoints { a: pts[0], b: pts[1] },
      "vertical" if pts.len() >= 2 => ConstraintKind::VerticalPoints { a: pts[0], b: pts[1] },
      "horizontal" => {
        need_c(1)?;
        ConstraintKind::Horizontal { line: cs[0] }
      }
      "vertical" => {
        need_c(1)?;
        ConstraintKind::Vertical { line: cs[0] }
      }
      "parallel" | "perpendicular" | "collinear" | "equal" | "tangent" | "concentric" => {
        need_c(2)?;
        let (a, b) = (cs[0], cs[1]);
        match t.as_str() {
          "parallel" => ConstraintKind::Parallel { a, b },
          "perpendicular" => ConstraintKind::Perpendicular { a, b },
          "collinear" => ConstraintKind::Collinear { a, b },
          "equal" => ConstraintKind::Equal { a, b },
          "tangent" => ConstraintKind::Tangent { a, b },
          _ => ConstraintKind::Concentric { a, b },
        }
      }
      "midpoint" => {
        need_p(1)?;
        need_c(1)?;
        ConstraintKind::Midpoint { p: pts[0], line: cs[0] }
      }
      "point_on_curve" | "on_curve" => {
        need_p(1)?;
        need_c(1)?;
        ConstraintKind::PointOnCurve { p: pts[0], curve: cs[0] }
      }
      // two lines or two circles/arcs, then the axis
      "symmetric" if pts.is_empty() && cs.len() >= 3 => {
        ConstraintKind::SymmetricCurves { a: cs[0], b: cs[1], line: cs[2] }
      }
      "symmetric" => {
        need_p(2)?;
        need_c(1)?;
        ConstraintKind::Symmetric { a: pts[0], b: pts[1], line: cs[0] }
      }
      "curvature" | "smooth" | "g2" => {
        need_c(2)?;
        ConstraintKind::Curvature { a: cs[0], b: cs[1] }
      }
      o => return Err(CmdError::new(format!("unknown constraint '{o}'"))),
    };
    let id = sketch.add_constraint(kind)?;
    let r = sketch.solve();
    if !r.ok() {
      return Err(conflict());
    }
    Ok((vec![id], None))
  })?;
  let (ids, center) = ids;
  // fixing a curve adds several constraints; "constraint" is the first
  let mut out = json!({
    "constraint": ids.first().map(|c| c.0),
    "constraints": ids.iter().map(|c| c.0).collect::<Vec<_>>(),
    "dof": s.model.sketches.get(&sk).map(|r| r.report.dof),
  });
  // a polygon constraint also returns the centre point it added
  if let Some(c) = center {
    out["center"] = json!(c.0);
  }
  Ok(out)
}

fn dimension(s: &mut Session, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let t = req_str(p, "type")?.to_string();
  let expr = req_expr(p, "value")?;
  let pts: Vec<PointId> = ids(p, "points").into_iter().map(PointId).collect();
  let cs: Vec<CurveId> = ids(p, "curves").into_iter().map(CurveId).collect();
  let kind = if t == "angle" { fox_expr::Kind::Angle } else { fox_expr::Kind::Length };
  let value = fox_expr::eval(&expr, kind, &params_scope(s))?;
  let dim = Dim::expr(expr, value);
  let id = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    // a line given instead of two points dimensions its endpoints
    let two_points = |sketch: &Sketch| -> Result<(PointId, PointId), CmdError> {
      if pts.len() >= 2 {
        return Ok((pts[0], pts[1]));
      }
      if let Some(c) = cs.first()
        && let Some(CurveKind::Line { a, b }) = sketch.curve(*c).map(|c| &c.kind)
      {
        return Ok((*a, *b));
      }
      Err(CmdError::new(format!("'{t}' needs two points or a line")))
    };
    let kind = match t.as_str() {
      "distance" | "length" => {
        let (a, b) = two_points(sketch)?;
        ConstraintKind::Distance { a, b, value: dim }
      }
      "horizontal" => {
        let (a, b) = two_points(sketch)?;
        ConstraintKind::HorizontalDistance { a, b, value: dim }
      }
      "vertical" => {
        let (a, b) = two_points(sketch)?;
        ConstraintKind::VerticalDistance { a, b, value: dim }
      }
      "radius" | "diameter" => {
        let c = *cs.first().ok_or(CmdError::new("needs a circle or arc"))?;
        if t == "radius" {
          ConstraintKind::Radius { curve: c, value: dim }
        } else {
          ConstraintKind::Diameter { curve: c, value: dim }
        }
      }
      "angle" => {
        if cs.len() < 2 {
          return Err(CmdError::new("an angle needs two lines"));
        }
        ConstraintKind::Angle { a: cs[0], b: cs[1], value: dim }
      }
      "point_line" => {
        let (Some(pt), Some(l)) = (pts.first(), cs.first()) else {
          return Err(CmdError::new("needs a point and a line"));
        };
        ConstraintKind::PointLineDistance { p: *pt, line: *l, value: dim }
      }
      o => return Err(CmdError::new(format!("unknown dimension '{o}'"))),
    };
    let id = sketch.add_constraint(kind)?;
    let r = sketch.solve();
    if !r.ok() {
      return Err(CmdError::new("that dimension conflicts with the existing constraints"));
    }
    Ok(id)
  })?;
  Ok(json!({"constraint": id.0, "dof": s.model.sketches.get(&sk).map(|r| r.report.dof)}))
}

fn sketch_info(s: &Session, p: &Value) -> CmdResult {
  let sk = match p.get("sketch").and_then(Value::as_u64) {
    Some(v) => FeatureId(v as u32),
    None => s.edit_sketch.ok_or(CmdError::new("pass 'sketch'"))?,
  };
  let sketch = s.design.sketch(sk)?;
  let r = s.model.sketches.get(&sk);
  let solved = r.map(|r| &r.sketch).unwrap_or(sketch);
  let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
  let (samples, loops) = (flag("samples"), flag("loops"));
  let v3 = |v: DVec3| json!([v.x, v.y, v.z]);
  let geometry: Vec<Value> = solved
    .curves
    .iter()
    .map(|c| {
      let poly = crate::scene::curve_polyline(solved, &c.kind);
      let length = match &c.kind {
        CurveKind::Circle { radius, .. } => std::f64::consts::TAU * radius,
        _ => poly.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum(),
      };
      let mut g = json!({"curve": c.id.0, "length": length, "bbox": bbox2(poly.iter().copied())});
      if samples {
        g["samples"] = json!(poly);
      }
      g
    })
    .collect();
  // which curves bound each profile: match every loop segment to the curve it lies on
  let curves_of = |l: &fox_sketch::Loop| {
    let mut ids: Vec<u32> = Vec::new();
    for seg in &l.segs {
      let tol = if matches!(seg, fox_sketch::Seg::Bezier { .. }) { 0.05 } else { 1e-5 };
      let at = seg.mid();
      let best = solved
        .curves
        .iter()
        .filter(|c| !c.construction)
        .filter_map(|c| solved.distance_to_curve(c.id, at).map(|d| (c.id.0, d)))
        .filter(|(_, d)| *d <= tol)
        .min_by(|a, b| a.1.total_cmp(&b.1));
      if let Some((id, _)) = best
        && !ids.contains(&id)
      {
        ids.push(id);
      }
    }
    ids
  };
  let mut used: Vec<u32> = Vec::new();
  let profiles = r.map(|r| {
    r.profiles
      .iter()
      .enumerate()
      .map(|(i, pr)| {
        let outer = curves_of(&pr.outer);
        let holes: Vec<Vec<u32>> = pr.holes.iter().map(&curves_of).collect();
        used.extend(outer.iter().chain(holes.iter().flatten()));
        let perimeter: f64 = std::iter::once(&pr.outer).chain(&pr.holes).map(loop_length).sum();
        let mut v = json!({
          "index": i, "area": pr.area(), "inside": pr.interior_point(), "holes": pr.holes.len(),
          "bbox": bbox2(pr.outer.polygon().into_iter()), "perimeter": perimeter,
          "curves": outer, "hole_curves": holes,
        });
        if loops {
          v["outer"] = json!(pr.outer.segs);
          v["hole_loops"] = json!(pr.holes.iter().map(|h| &h.segs).collect::<Vec<_>>());
        }
        v
      })
      .collect::<Vec<_>>()
  });
  let unused: Vec<u32> =
    solved.curves.iter().filter(|c| !c.construction && !used.contains(&c.id.0)).map(|c| c.id.0).collect();
  Ok(json!({
    "sketch": sk.0,
    "plane": r.map(|r| json!({"origin": v3(r.plane.origin), "x": v3(r.plane.x), "y": v3(r.plane.y), "normal": v3(r.plane.normal())})),
    "points": sketch.points.iter().map(|p| json!({"id": p.id.0, "at": [p.x, p.y], "fixed": p.fixed})).collect::<Vec<_>>(),
    "curves": sketch.curves,
    "geometry": geometry,
    "constraints": sketch.constraints,
    "profiles": profiles,
    "open_curves": unused,
    "dof": r.map(|r| r.report.dof),
    "solved": r.map(|r| r.report.ok()),
    "free_points": r.map(|r| r.report.free_points.iter().map(|p| p.0).collect::<Vec<_>>()),
    "texts": sketch.texts.iter().map(|t| json!({
      "id": t.id.0, "text": t.text, "font": t.font, "height": t.height, "angle": t.angle.to_degrees(),
      "anchor": t.anchor.0, "at": sketch.pos(t.anchor), "curves": t.curves.iter().map(|c| c.0).collect::<Vec<_>>()
    })).collect::<Vec<_>>(),
  }))
}

/// `[[min x, min y], [max x, max y]]`, or null for no points.
fn bbox2(pts: impl Iterator<Item = [f64; 2]>) -> Value {
  let mut b: Option<([f64; 2], [f64; 2])> = None;
  for q in pts {
    let (lo, hi) = b.get_or_insert((q, q));
    for k in 0..2 {
      lo[k] = lo[k].min(q[k]);
      hi[k] = hi[k].max(q[k]);
    }
  }
  b.map_or(Value::Null, |(lo, hi)| json!([lo, hi]))
}

fn loop_length(l: &fox_sketch::Loop) -> f64 {
  let mut pts = l.polygon();
  pts.extend(pts.first().copied());
  pts.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum()
}

// ---------------------------------------------------------------- info / export

fn info(s: &Session) -> Value {
  let bodies: Vec<Value> = s
    .model
    .bodies
    .iter()
    .map(|b| {
      let m = b.shape.mass().ok();
      json!({
        "id": b.id.0,
        "name": s.body_name(b.id),
        "visible": s.body_visible(b.id),
        "color": s.design.body_color(b.id).map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])),
        "volume_mm3": m.map(|m| m.volume),
        "area_mm2": m.map(|m| m.area),
        "bbox": m.map(|m| [m.bbox_min.to_array(), m.bbox_max.to_array()]),
        "faces": b.faces().len(),
        "edges": b.edges().len(),
        "sheet_metal": b.sheet.as_ref().map(|i| json!({
          "rule": i.rule, "thickness": i.thickness, "bend_radius": i.bend_radius, "k_factor": i.k_factor, "flat": i.flat,
        })),
      })
    })
    .collect();
  let timeline: Vec<Value> = s
    .design
    .features
    .iter()
    .zip(&s.model.status)
    .map(|(f, st)| {
      json!({
        "id": f.id.0,
        "name": f.name,
        "type": f.kind.base_name(),
        "suppressed": f.suppressed,
        "visible": f.visible,
        "rolled_back": st.rolled_back,
        "error": st.error,
        "warning": st.warning,
      })
    })
    .collect();
  let params: Vec<Value> = s
    .design
    .parameters
    .iter()
    .map(|p| json!({"name": p.name, "expr": p.expr, "value": s.model.params.get(&p.name), "error": s.model.param_errors.get(&p.name)}))
    .collect();
  json!({
    "name": s.design.name,
    "parameters": params,
    "timeline": timeline,
    "bodies": bodies,
    "editing_sketch": s.edit_sketch.map(|f| f.0),
    "components": s.design.components.iter().map(|c| json!({
      "id": c.id.0, "name": c.name, "bodies": c.bodies.iter().map(|b| b.0).collect::<Vec<_>>(),
      "grounded": c.grounded, "material": c.material,
    })).collect::<Vec<_>>(),
  })
}

/// Selection helpers shared with the UI.
#[allow(dead_code)]
pub fn selected_edges(s: &Session) -> Vec<Value> {
  s.selection
    .iter()
    .filter_map(|x| match x {
      Sel::Edge { body, edge } => Some(json!({"body": body.0, "edge": edge})),
      _ => None,
    })
    .collect()
}

mod sketch_tools;

mod sketch_curves;

mod sketch_include;
