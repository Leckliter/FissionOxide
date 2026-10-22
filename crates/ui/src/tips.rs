//! The toolbar tooltips' texts: a command's one-line description and the fuller explanation shown
//! after resting on it, keyed by the toolbar it sits on and its label.

/// Which toolbar a command sits on: a label can name different commands (the sketch's Trim
/// isn't the surface Trim).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
  Design,
  Sketch,
  Drawing,
  Manufacture,
  /// The Form workspace's FORM tab.
  Form,
  /// The Animation workspace's ANIMATION tab.
  Animation,
}

/// A command's tooltip text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tip {
  /// One line under the name.
  pub line: &'static str,
  /// The usage paragraphs (a blank line between them); empty for a command whose Fusion tooltip
  /// has only the description, such as Press Pull.
  pub more: &'static str,
}

/// The name a command's tooltip shows, where Fusion's differs from its menu row.
pub(crate) fn title(label: &'static str) -> &'static str {
  match label {
    "Light" => "Lights",
    "Capture Image..." => "Capture Image",
    _ => label,
  }
}

/// The tooltip for the command `label` on the `scope` toolbar.
pub(crate) fn tip(scope: Scope, label: &str) -> Option<Tip> {
  use Scope::*;
  if scope == Form {
    let (line, more) = crate::form_ui::tip(label)?;
    return Some(Tip { line, more });
  }
  let (line, more) = match (scope, label) {
    // ANIMATION
    (Animation, "New Storyboard") => (
      "Creates a new storyboard.",
      "A storyboard is one animation of the design: its own timeline of actions. Choose Clean to start \
       from the assembled design, or Start from end of previous to carry on from where the last \
       storyboard ends.",
    ),
    (Animation, "Transform Components") => (
      "Moves or rotates components over time.",
      "Select components, then drag the manipulator to move or rotate them. The change is recorded as an \
       action from the playhead, and plays back with the storyboard.",
    ),
    (Animation, "Restore Home") => (
      "Puts components back where they are in the design.",
      "Records actions that return the selected components (or all of them) to their assembled \
       positions from the playhead.",
    ),
    (Animation, "Auto Explode: One Level") => (
      "Explodes the selected components one level down.",
      "Spreads the components of the first level apart along their assembly directions, recording an \
       action for each. Adjust the spacing on the slider that appears.",
    ),
    (Animation, "Auto Explode: All Levels") => (
      "Explodes all levels of the selected components.",
      "Spreads every component, down through the sub-assemblies, apart along their assembly directions, \
       recording an action for each. Adjust the spacing on the slider that appears.",
    ),
    (Animation, "Manual Explode") => (
      "Explodes the selected components along a direction you pick.",
      "Select components, then drag the arrow along the direction to explode them. Each component \
       moves by its share of the distance.",
    ),
    (Animation, "Show/Hide") => (
      "Shows or hides components from the playhead.",
      "Select components to record an action that hides the shown ones and shows the hidden ones as \
       the storyboard plays.",
    ),
    (Animation, "Appearance") => (
      "Changes the opacity of components over time.",
      "Select components and set their opacity; the change fades in over the action's length.",
    ),
    (Animation, "Create Callout") => (
      "Adds a text callout attached to the model.",
      "Click a point on the model, then type the text. The callout shows in the storyboard from the \
       playhead.",
    ),
    (Animation, "View") => (
      "Records the current camera position as a view action.",
      "Orbit, pan or zoom, then run View to record the camera's move from the playhead, so the \
       storyboard plays it back.",
    ),
    (Animation, "Publish Video") => (
      "Publishes the storyboards as a video.",
      "Choose the storyboards, the video resolution and where to save it. The video is rendered from \
       the storyboards' views.",
    ),
    // SKETCH CREATE
    (_, "Line") => (
      "Draws a chain of connected straight lines.",
      "Click to place each point; every click starts the next segment from the last one. Press Esc or \
       double-click to stop the chain. Type a length or angle while drawing to set it exactly.",
    ),
    (_, "Midpoint Line") => (
      "Draws a line outwards from its midpoint.",
      "Click the midpoint, then one end; the other end mirrors it. Handy for a line centred on the \
       origin or on another point, as the midpoint stays tied to where you first clicked.",
    ),
    (_, "2-Point Rectangle") => (
      "Draws a rectangle from two opposite corners.",
      "Click one corner, then the diagonally opposite one; the sides line up with the sketch's axes. \
       Type the width and height while dragging to set them exactly.",
    ),
    (_, "3-Point Rectangle") => (
      "Draws a rectangle at any angle from one side and a height.",
      "Click the two ends of one side, which sets the angle, then click to set how far the opposite \
       side lies. Use it for rectangles that don't line up with the sketch's axes.",
    ),
    (_, "Center Rectangle") => (
      "Draws a rectangle from its centre and a corner.",
      "Click the centre, then a corner; the rectangle grows symmetrically around the centre. Start on \
       the origin to get a part that stays centred when you change its size.",
    ),
    (_, "Center Diameter Circle") => (
      "Draws a circle from its centre and a point on its edge.",
      "Click the centre, then drag out the size or type a diameter. The most common way to sketch a \
       hole, boss or round profile.",
    ),
    (_, "2-Point Circle") => (
      "Draws a circle from the two ends of a diameter.",
      "Click two points; they become opposite ends of the circle's diameter. Useful when you know where \
       the circle's edges must sit rather than where its centre is.",
    ),
    (_, "3-Point Circle") => (
      "Draws a circle through three points.",
      "Click three points on the circle's edge; the centre and size follow from them. Snap to existing \
       points to fit a circle through them.",
    ),
    (_, "2-Tangent Circle") => (
      "Draws a circle of a given radius touching two lines.",
      "Pick two lines, then place the circle on the side you want and set its radius. The circle stays \
       tangent to both lines.",
    ),
    (_, "3-Tangent Circle") => (
      "Draws a circle touching three curves.",
      "Pick three lines or curves; the circle that touches all three nearest to your picks is drawn. \
       Its centre and size follow from the curves, so there's nothing to dimension.",
    ),
    (_, "3-Point Arc") => (
      "Draws an arc through a start point, an end point and a third point.",
      "Click the start and the end, then a point the arc passes through, which sets how far it bulges. \
       Snap the ends to existing geometry to close a profile.",
    ),
    (_, "Center Point Arc") => (
      "Draws an arc from its centre, a start point and an end point.",
      "Click the centre, then the start, which sets the radius, then sweep round to the end. Good for \
       arcs that must share a centre with a circle or hole.",
    ),
    (_, "Tangent Arc") => (
      "Draws an arc that continues smoothly from the end of a line or arc.",
      "Click the end of an existing line or arc, then click where the arc should finish. The arc leaves \
       the curve in the same direction, so the join has no corner.",
    ),
    (_, "Circumscribed Polygon") => (
      "Draws a regular polygon from its centre and the middle of a side.",
      "Click the centre, then the midpoint of one side; the sides then sit on a circle of that radius. \
       Use it when you know the distance across the flats, such as for a hex nut.",
    ),
    (_, "Inscribed Polygon") => (
      "Draws a regular polygon from its centre and one corner.",
      "Click the centre, then a corner; the corners then sit on a circle of that radius. Use it when \
       you know the distance across the corners.",
    ),
    (_, "Edge Polygon") => (
      "Draws a regular polygon from the two ends of one side.",
      "Click the two ends of one side, then the side the polygon grows towards. Useful when a side must \
       sit exactly along existing geometry.",
    ),
    (_, "Center to Center Slot") => (
      "Draws a slot from the centres of its two round ends and a width.",
      "Click the centre of one end, then the other, then set the width. The usual choice when the \
       slot's travel between its end centres is what matters.",
    ),
    (_, "Overall Slot") => (
      "Draws a slot from its overall length and its width.",
      "Click the two extreme ends of the slot, then set the width; the round ends fit inside that \
       length. Use it when the total length is the dimension you care about.",
    ),
    (_, "Center Point Slot") => (
      "Draws a slot from its centre, one end and its width.",
      "Click the slot's centre, then the centre of one end, which mirrors to the other, then set the \
       width. Handy for slots centred on a point.",
    ),
    (_, "Three Point Arc Slot") => (
      "Draws a curved slot whose centreline passes through three points.",
      "Click the start and end of the slot's centreline and a point it passes through, then set the \
       width. Use it for slots that follow an arc, such as an adjustment slot.",
    ),
    (_, "Center Point Arc Slot") => (
      "Draws a curved slot around a centre point.",
      "Click the centre of the arc, then the start and end of the slot's centreline, then set the \
       width. Good for slots that rotate about a pivot or a bolt circle.",
    ),
    (_, "Fit Point Spline") => (
      "Draws a smooth curve that passes through the points you click.",
      "Click points in order; the curve passes through each one. Double-click or press Esc to finish. \
       Drag the points afterwards to reshape it.",
    ),
    (_, "Control Point Spline") => (
      "Draws a smooth curve shaped by control points it doesn't pass through.",
      "Click the control points in order; the curve is pulled towards each one but only touches the \
       first and last. It gives smoother, more predictable shapes than a fit point spline.",
    ),
    (_, "Ellipse") => (
      "Draws an ellipse from its centre, its major axis and its minor radius.",
      "Click the centre, then the end of the major axis, which sets its length and angle, then the \
       minor radius. Its axes can be dimensioned afterwards.",
    ),
    (_, "Conic Curve") => (
      "Draws a curve from a start to an end point, bent towards an apex.",
      "Click the start and end, then the apex, then set how full the curve is (its rho value). Low \
       values give a flatter, elliptical bend; high values a sharper, hyperbolic one.",
    ),
    (_, "Point") => (
      "Places a sketch point.",
      "Click to place a point. Points mark hole centres, the ends of paths and references for \
       dimensions and constraints; Hole uses a sketch's points as its locations.",
    ),
    (Sketch, "Text") => (
      "Adds editable text to the sketch whose letter outlines you can extrude.",
      "Click where the text starts, then type it and choose the font, height and angle. The letters \
       make closed profiles you can extrude or emboss. Use Edit Text to change it later.",
    ),
    (_, "Trace Image") => (
      "Traces a photo, logo or scan into closed sketch curves.",
      "Pick an image file; its outlines become sketch curves at the size you set. Adjust the threshold \
       and smoothing until the trace is clean, then extrude or emboss the profiles.",
    ),
    (Sketch, "Mirror") => (
      "Mirrors sketch curves across a line.",
      "Pick the curves, then the mirror line. The copies stay symmetric with the originals, so editing \
       one side updates the other.",
    ),
    (Sketch, "Circular Pattern") => (
      "Copies sketch curves around a centre point.",
      "Pick the curves and the centre, then set the number of copies and the angle they span. Use it \
       for bolt circles, spokes and teeth.",
    ),
    (Sketch, "Rectangular Pattern") => (
      "Copies sketch curves in rows and columns.",
      "Pick the curves, then set the count and spacing in each of two directions. Good for grids of \
       holes or vents.",
    ),
    (_, "Sketch Dimension") => (
      "Adds a driving dimension to sketch geometry.",
      "Click a line, arc or circle, or two items, then place the dimension and type its value. The \
       geometry moves to match. Typing a parameter name or expression links the size to it.",
    ),

    // SKETCH PROJECT
    (_, "Project") => (
      "Projects edges of the model into the sketch.",
      "Pick bodies, faces or edges; their outlines are projected flat onto the sketch plane. The \
       projected curves follow the model when it changes, so the sketch stays in step.",
    ),
    (_, "Intersect") => (
      "Adds the curves where bodies cut through the sketch plane.",
      "Pick bodies or faces; the outlines of their cross-section on the sketch plane are added as \
       curves. Use it to sketch from a section through existing geometry.",
    ),
    (_, "Include 3D Geometry") => (
      "Brings model edges into the sketch at their real 3D positions.",
      "Pick edges or vertices; they're added without being flattened, so they can serve as sweep paths \
       or rails. They follow the model when it changes.",
    ),
    (_, "Project To Surface") => (
      "Wraps or projects sketch curves onto a curved face.",
      "Pick the faces and the curves, then project them along a direction or along the face normals. \
       Use the result to split faces or to guide a sweep on a curved part.",
    ),
    (_, "Intersection Curve") => (
      "Makes 3D curves where sketch curves, pushed along the sketch normal, cross faces.",
      "Pick sketch curves and the faces they should meet; the curves are pushed straight out of their \
       sketch plane and the lines where they cross the faces become 3D curves. A pipe or sweep can \
       follow them, and they update when the body changes.",
    ),
    (_, "Spun Profile") => (
      "Draws the outline bodies sweep through as they turn about an axis.",
      "Pick the bodies and an axis; the profile of everything they pass through while turning is \
       drawn on the sketch plane, on their side of the axis (Flip Result for the other). The outline \
       follows the bodies when they change.",
    ),
    (_, "Isoparametric Curve") => (
      "Makes a 3D sketch curve along a face at a constant U or V.",
      "Pick a face; a curve of constant parameter runs across it, for a sweep path or a split line. \
       Not available yet in FissionOxide: the dialog opens, but the kernel can't evaluate a face's \
       iso-curves.",
    ),

    // SKETCH CHAMFER
    (_, "Equal Distance Chamfer") => (
      "Creates a chamfer at the vertex, intersection, or apparent intersection of two lines in a \
       sketch, based on an equal distance to each side.",
      "Select a vertex or two lines, then specify the distance value for the chamfer. The chamfer is applied at once with the \
       last value; the floating box and arrow beside it change it, and Enter commits and ends the tool.",
    ),
    (_, "Distance and Angle Chamfer") => (
      "Creates a chamfer at the vertex, intersection, or apparent intersection of two lines in a \
       sketch, based on a single distance and an angle.",
      "Select a vertex or two lines, then specify the distance and angle values for the chamfer. The chamfer is applied at once with the \
       last value; the floating box and arrow beside it change it, and Enter commits and ends the tool.",
    ),
    (_, "Two Distance Chamfer") => (
      "Creates a chamfer at the vertex, intersection, or apparent intersection of two lines in a \
       sketch, based on a different distance to each side.",
      "Select a vertex or two lines, then specify the two distance values for the chamfer. The chamfer is applied at once with the \
       last value; the floating box and arrow beside it change it, and Enter commits and ends the tool.",
    ),

    // SKETCH MODIFY
    // Fusion's own wording: the first line, then the instruction paragraph and, where Fusion shows
    // it, "Press Ctrl+/ for more help." as a third paragraph (a blank line between each)
    (Sketch, "Fillet") => (
      "Places an arc of a specified radius at the intersection of two lines or arcs.",
      "Select the vertex or the two lines or arcs. Specify a radius for the fillet.\n\nPress Ctrl+/ for \
       more help.",
    ),
    (_, "Blend Curve") => (
      "Creates a tangent or curvature continuous blend curve between the endpoints of two sketch curves \
       or edges.",
      "Select two sketch curves or edges in the canvas to blend.",
    ),
    (Sketch, "Offset") => (
      "Copies the selected sketch curves a specified distance from the original curves.",
      "Select the curves to offset then specify the offset distance.\n\nPress Ctrl+/ for more help.",
    ),
    (Sketch, "Trim") => (
      "Trims a sketch curve to the nearest intersecting curve or boundary geometry.",
      "Pause the cursor over the curve to preview the portion to be trimmed. Select the curve to \
       trim.\n\nPress Ctrl+/ for more help.",
    ),
    (Sketch, "Extend") => (
      "Extends a curve to the nearest intersecting curve or boundary geometry.",
      "Pause the cursor over the curve to preview the portion to be extended. Select the curve to \
       extend it.\n\nPress Ctrl+/ for more help.",
    ),
    (_, "Break") => (
      "Breaks a sketch entity into two or more segments where it intersects other sketch entities.",
      "Pause the cursor over the sketch entity to preview where it will be broken, then click to break \
       it.",
    ),
    (_, "Sketch Scale") => (
      "Scales sketch geometry.",
      "Select sketch geometry to scale, select the point from which to scale, then drag the manipulator \
       handle in the canvas or specify an exact Scale Factor.",
    ),
    (Sketch, "Remove Duplicates") => (
      "Deletes curves lying on top of another of the same kind.",
      "Pick a tolerance (Standard 0.001\" by default, or Exact, Tight, Loose or your own). Copies of \
       lines, arcs, circles, splines and ellipses go and the first stays; a line overlapping another \
       part way is cut back to its end, and two lines meeting at a corner are left alone. The dialog \
       says what will go before you click OK.",
    ),
    (Sketch, "Clean Up") => (
      "Makes an imported or projected sketch ready for a DXF.",
      "Breaks projection links, deletes the constraints on projected geometry (or all of them) and \
       optionally the dimensions, removes points no curve uses, turns construction into normal \
       curves, closes small gaps between curve ends and turns splines into arcs or lines.",
    ),
    (Sketch, "Move/Copy") => (
      "Moves the selected face, body, sketch, or construction geometry a specified distance or angle.",
      "Select the objects to modify then specify the distance or angle to move them.\n\nPress Ctrl+/ for \
       more help.",
    ),
    (_, "Edit Text") => (
      "Changes the wording, font or size of a sketch text.",
      "Select one of the text's letters, then edit it in the dialog. Its outlines regenerate, and \
       features built on them update.",
    ),
    (_, "Explode Text") => (
      "Turns a sketch text into plain lines and splines.",
      "Select one of the text's letters; the text becomes ordinary curves you can edit, trim or \
       constrain. It can no longer be edited as text afterwards.",
    ),

    // SKETCH CONSTRAINTS
    (_, "Horizontal/Vertical") => (
      "Makes a line horizontal or vertical in the sketch.",
      "Select lines; each snaps to whichever of horizontal or vertical is closer. Select two points to \
       line them up horizontally or vertically instead.",
    ),
    (_, "Coincident") => (
      "Makes a point lie on another point or on a curve.",
      "Select a point and a point, line, arc or circle. The point moves onto it and stays there. Use it \
       to close gaps so a profile becomes closed.",
    ),
    (_, "Tangent") => (
      "Makes a curve meet another smoothly, with no corner.",
      "Select a line and an arc or circle, or two arcs. They're kept touching in the same direction. \
       Common where a line runs into a fillet.",
    ),
    (_, "Equal") => (
      "Makes curves the same length or radius.",
      "Select two or more lines to give them equal length, or arcs and circles for equal radius. \
       Dimension one of them and the others follow.",
    ),
    (Sketch, "Parallel") => (
      "Keeps two lines parallel.",
      "Select two lines; the second turns to match the first. They stay parallel as the sketch changes.",
    ),
    (_, "Perpendicular") => (
      "Keeps two lines at right angles.",
      "Select two lines; the second turns to meet the first at 90 degrees. They stay that way as the \
       sketch changes.",
    ),
    (_, "Fix/UnFix") => (
      "Locks geometry where it is, or releases it again.",
      "Select points or curves to fix them in place; selecting fixed geometry frees it. Fixing is a \
       quick way to stop things moving, but dimensions keep the design easier to change.",
    ),
    (_, "MidPoint") => (
      "Puts a point at the middle of a line or arc.",
      "Select a point, then a line or arc. The point moves to the curve's midpoint and stays there, \
       which keeps features centred.",
    ),
    (_, "Concentric") => (
      "Makes arcs and circles share a centre.",
      "Select two arcs, circles or ellipses; the second moves so their centres coincide. Use it for \
       counterbores, washers and rings.",
    ),
    (_, "Collinear") => (
      "Puts lines on the same infinite line.",
      "Select two lines; they're kept on one straight line, though they can still have a gap between \
       them. Use it to line up edges of separate features, such as the tops of two slots.",
    ),
    (_, "Symmetry") => (
      "Makes two curves or points mirror each other across a line.",
      "Select the two items, then the line of symmetry. They're kept as mirror images, so changing one \
       side changes the other.",
    ),
    (_, "Curvature") => (
      "Joins two curves so their curvature matches as well as their direction.",
      "Select a spline and the curve it meets. The join becomes smoother than a tangent one, which \
       shows as an unbroken reflection on the finished faces.",
    ),
    (_, "Polygon") => (
      "Keeps lines forming a regular polygon.",
      "Select the lines of a closed shape; they're kept at equal length and equal angles. Dimension a \
       single side to size the whole polygon.",
    ),

    // SKETCH FINISH
    (_, "Finish Sketch") => (
      "Finishes the sketch and returns to the model.",
      "The sketch's closed profiles are then ready for Extrude, Revolve and other features. You can \
       reopen it from the browser or timeline to edit it.",
    ),
    (_, "Finish with AutoConstrain") => (
      "Finishes the sketch after adding constraints it can work out.",
      "Constraints and dimensions are added so that more of the sketch is fully defined, then the \
       sketch is closed. Check the result, as the guesses may not match your design intent.",
    ),

    // DRAWING CREATE
    (_, "Base View") => (
      "Places a standard view of the design on the drawing sheet.",
      "Pick the orientation, such as front or top, the scale and the style, then click to place it. \
       Projected, section and detail views are made from base views.",
    ),
    (_, "Projected View") => (
      "Places a view projected from an existing view.",
      "Pick a view, then click above, below, beside or diagonally from it. The new view lines up with \
       its parent and shares its scale.",
    ),
    (_, "Section View") => (
      "Places a view of the part cut along a line through another view.",
      "Pick a view, draw the cutting line across it, then place the section. Cut faces are hatched, \
       showing the inside of the part.",
    ),
    (_, "Detail View") => (
      "Places an enlarged view of a circular area of another view.",
      "Pick a view, click the centre of the area and its size, then place the detail and set its scale. \
       Use it for small features that are hard to dimension at the main scale.",
    ),
    (_, "Flat Pattern View") => (
      "Places a sheet metal body's flat pattern on the drawing.",
      "The part's flat pattern is shown with its bend lines. Use it to dimension the blank for cutting \
       before it's bent.",
    ),
    (_, "Create Drawing") => (
      "Starts a new drawing of the design with standard views.",
      "Choose the sheet size, standard and scale; base and projected views are laid out for you. Add \
       dimensions, notes and tables afterwards.",
    ),
    (_, "Break View") => (
      "Shortens a long view by leaving out a stretch of it.",
      "Pick a view and the two lines between which it's broken. Dimensions across the break still show \
       the true length, so long parts fit the sheet at a readable scale.",
    ),

    // DRAWING MODIFY
    (_, "Move") => (
      "Moves a drawing view to a new place on the sheet.",
      "Pick a view and click where it should go. Views projected from it keep their alignment.",
    ),
    (Drawing, "Delete") => (
      "Deletes the selected views, dimensions or annotations from the drawing.",
      "Select items on the sheet and delete them. Deleting a view also deletes the dimensions and \
       annotations attached to it.",
    ),
    (_, "Sheet Settings") => (
      "Sets the sheet's size, scale, projection and tables.",
      "Choose the paper size, default scale and first- or third-angle projection. The settings apply to \
       the current sheet.",
    ),
    (_, "New Sheet") => (
      "Adds another sheet to the drawing.",
      "The new sheet gets the same size and title block as the current one. Use more sheets for \
       details, sections and parts lists that don't fit.",
    ),
    (_, "Edit Sheet") => (
      "Edits the current sheet's border and title block.",
      "Change the sheet's frame and the fields of its title block, such as the title, drawer and date. \
       Changes appear on that sheet only; other sheets keep their own frames.",
    ),

    // DRAWING DIMENSIONS
    (_, "Dimension") => (
      "Dimensions edges, points or circles in a view.",
      "Pick an edge, two points or a circle; the dimension type follows from what you pick. Drag to \
       place it, then edit its text or tolerance if needed.",
    ),
    (_, "Ordinate Dimension") => (
      "Dimensions points by their distance from a common origin.",
      "Pick the origin, then the points to dimension along the view's horizontal or vertical. Each \
       value is a distance from the origin, which avoids tolerance stack-up.",
    ),
    (_, "Angular Dimension") => (
      "Dimensions the angle between two edges.",
      "Pick the vertex and a point on each edge, then place the arc. The dimension shows the angle \
       between the edges.",
    ),
    (_, "Baseline Dimension") => (
      "Adds a stack of dimensions all measured from one baseline.",
      "Pick the base edge, then each feature in turn; the dimensions stack outwards from the baseline. \
       Each is measured from the same edge, so errors don't add up.",
    ),
    (_, "Chain Dimension") => (
      "Adds a row of dimensions, each measured from the previous feature.",
      "Pick the first edge, then each feature along the row; the dimensions line up end to end. Use it \
       when the spacing between features matters most.",
    ),

    // DRAWING TEXT
    (Drawing, "Text") => (
      "Adds a note to the drawing sheet.",
      "Click where it goes and type the note. Use it for general notes, material callouts and \
       instructions that don't belong to a dimension.",
    ),

    // DRAWING TABLES
    (_, "Parts List") => (
      "Adds a table of the design's components and their quantities.",
      "Place the table on the sheet; it lists each component with its quantity and material. It updates \
       when the design changes, and balloons take their numbers from it.",
    ),
    (_, "Title Block") => (
      "Fills in the drawing's title block.",
      "Enter the title, part number, drawer, dates and other fields. The text appears in the block in \
       the sheet's corner.",
    ),
    (_, "Balloon") => (
      "Labels a component with its parts list item number.",
      "Click a component's edge in a view, then place the balloon. Its number comes from the parts list \
       and updates with it.",
    ),
    (_, "Auto Balloon") => (
      "Adds balloons to every component in a view at once.",
      "Pick a view; a balloon is placed for each component that's visible in it. Tidy their positions \
       afterwards.",
    ),
    (_, "Revision Table") => (
      "Adds a table that records the drawing's revisions.",
      "Place the table, then add a row for each change with its letter, description, date and approver. \
       Keep it up to date so anyone holding a print can tell whether it's current.",
    ),

    // DRAWING SYMBOLS
    (_, "Feature Control Frame") => (
      "Adds a geometric tolerance to a feature.",
      "Pick the feature, then choose the tolerance type, its value and the datums it refers to; tick \
       Composite for a lower tier that refines a position or profile tolerance. It follows the \
       drawing's standard.",
    ),
    (_, "Datum Identifier") => (
      "Marks a feature as a datum for geometric tolerances.",
      "Pick an edge or face and place the symbol. Leave the letter empty to use the next free one.",
    ),
    (_, "Surface Texture") => (
      "Adds a surface finish symbol to an edge.",
      "Pick an edge and set the roughness and the machining requirement. The symbol follows the \
       drawing's standard.",
    ),
    (_, "Weld Symbol") => (
      "Adds a welding symbol with an arrow to a joint.",
      "Click the joint on the sheet and set the weld type, size and side, a contour with its finish, \
       or a back weld or melt-through behind a groove weld. Use it to tell the fabricator how parts \
       are to be joined.",
    ),
    (_, "Datum Target") => (
      "Marks a point, line or area used as a datum on a part.",
      "Pick the location on a view and set the target's shape, size and label. Use it on castings and \
       rough faces where a whole face can't serve as a datum.",
    ),

    // DRAWING EXPORT
    (_, "PDF Export") => (
      "Writes the drawing as a PDF.",
      "Every sheet is written to one PDF, ready to print or send. The file doesn't change when the \
       design does, so export again after edits.",
    ),
    (_, "DXF Export") => (
      "Writes the drawing as a DXF file.",
      "Lines, text and dimensions are written as 2D entities that other CAD and cutting software can \
       open. Use it to hand drawings to a supplier or to pull outlines into a laser cutter's software.",
    ),
    (_, "SVG Export") => (
      "Writes the drawing as an SVG image.",
      "The sheet is saved as scalable vector graphics, which stay sharp at any size in a web page or \
       document. Use it for documentation or the web, where a PDF is awkward.",
    ),
    (_, "Finish Drawing") => (
      "Closes the drawing and returns to the design.",
      "The drawing is kept and updates when the design changes. Reopen it from the data panel.",
    ),

    // MANUFACTURE SETUP
    (_, "New Setup") => (
      "Starts a machining setup: the stock, the work origin and the machine.",
      "Pick the part, size the stock around it and set the work coordinate system that matches how the \
       part sits on the machine. Every operation belongs to a setup.",
    ),
    (_, "Fixtures") => (
      "Picks the bodies that hold the part: vise jaws, clamps, a fixture plate.",
      "Fixtures are never machined and don't count for the stock. Tools and their holders are \
       checked against them while toolpaths are generated and in Simulate, with a warning where they \
       would run into one. Pick nothing to clear them.",
    ),
    (_, "Tool Library") => (
      "Opens the cutting tools with their sizes, feeds and speeds.",
      "Browse, add and edit tools. Operations pick their tool from here, and its feeds and speeds \
       become their defaults.",
    ),

    // MANUFACTURE 2D
    (_, "Face") => (
      "Machines the top of the stock flat, down to the top of the part.",
      "Pick a tool and the stock to leave; passes sweep across the whole top. Usually the first \
       operation of a setup.",
    ),
    (_, "2D Pocket") => (
      "Clears pockets down to their flat floors.",
      "Pick the pocket floors or their outlines and a tool; the area is cleared in steps down to the \
       floor. Set step-downs and stock to leave for a finishing pass.",
    ),
    (_, "2D Contour") => (
      "Cuts around the outline of faces or edges.",
      "Pick the edges or faces to follow and a tool; the tool runs round the outline, offset by its \
       radius. Add tabs to hold a part that is cut free of the stock.",
    ),
    (_, "2D Adaptive Clearing") => (
      "Clears pockets with a light, constant tool engagement.",
      "Pick the pockets and a tool; the toolpath keeps the load on the cutter even, allowing deep cuts \
       at high feed. Follow it with a finishing contour.",
    ),
    (_, "Slot") => (
      "Mills slots with a tool as wide as the slot.",
      "Pick the slot's centreline or edges; the tool runs along it at full width. Use it for keyways \
       and narrow grooves.",
    ),
    (_, "Trace") => (
      "Runs the tool along edges or curves exactly as drawn.",
      "Pick the curves; the tool centre follows them, with no radius offset. Use it for chamfers, \
       grooves and marking lines.",
    ),
    (Manufacture, "Thread") => (
      "Mills a thread with a thread-milling tool.",
      "Pick the hole or boss and a thread mill; the tool spirals round to cut the thread. Set the pitch \
       and whether it's internal or external.",
    ),
    (_, "Bore") => (
      "Mills a hole to size by spiralling a tool down it.",
      "Pick round holes or bosses and a tool smaller than them; the tool spirals down to cut them to an \
       accurate diameter. It's more accurate than a drill for large holes and needs no tool of the \
       exact size.",
    ),
    (_, "Engrave") => (
      "Engraves text and lines with a pointed tool.",
      "Pick the curves and a V-shaped tool; the depth varies so the cut follows the width of the shape. \
       Use it for lettering and decoration.",
    ),

    // GENERATIVE DESIGN
    (_, "New Generative Study") => (
      "Starts a generative design study.",
      "A study holds the design space, load cases, objectives, materials and manufacturing methods, \
       and the outcomes it generates. The Generative Design window sets them up.",
    ),
    (_, "Preserve Geometry") => (
      "Marks the selected bodies as geometry every outcome keeps.",
      "Select bosses, pads and mounting faces' bodies first. Loads and constraints go on their faces; \
       the design space is the box around them plus the margin.",
    ),
    (_, "Obstacle Geometry") => (
      "Marks the selected bodies as space the outcomes stay out of.",
      "Use bodies filling bolt holes, tool access and clearance for other parts. Inserting an outcome \
       as B-rep cuts them out exactly, so holes come out round.",
    ),
    (_, "Starting Shape") => (
      "Uses the selected body as the shape the optimisation starts from.",
      "Its bounds join the design space. Starting from a rough version of the part can steer the \
       outcome towards it.",
    ),
    (_, "Structural Loads") => (
      "Puts a force or a pressure on the selected faces.",
      "Set the force (N, X Y Z) or pressure (MPa) in the Generative Design window, select faces of \
       preserve geometry, then run it. Loads go into the load case picked in the window.",
    ),
    (_, "Structural Constraints") => (
      "Holds the selected faces fixed.",
      "Pick which axes are fixed in the Generative Design window. A constraint without a load case \
       named goes into every case.",
    ),
    (_, "Study Settings") => (
      "Objectives, limits, materials and manufacturing methods of the study.",
      "Maximise stiffness at a mass target or minimise mass within a safety factor; pick materials \
       and unrestricted, 3-axis or 2.5-axis milling, symmetry planes and a minimum member size.",
    ),
    (_, "Pre-check") => (
      "Checks the study is ready to generate.",
      "Shows the grid and element counts and warns about loads or constraints that touch no \
       geometry and load cases without loads.",
    ),
    (Design, "Generate") => (
      "Generates the study's outcomes in the background.",
      "Every material with every manufacturing method is one outcome, found by topology \
       optimisation. Watch the progress bar in the window; Stop keeps what's done.",
    ),
    (_, "Explore") => (
      "Compares the outcomes and inserts one into the design.",
      "The explorer lists each outcome's mass, volume, displacement and safety factor, and whether \
       it meets the limits. Insert one as a mesh body or as B-rep joined with the preserve geometry.",
    ),

    // MANUFACTURE 3D
    (Manufacture, "Parallel") => (
      "Finishes 3D surfaces with parallel passes across the part.",
      "Pick the area and a tool, usually ball-nosed, and the stepover between rows. Works best on \
       shallow, gently curved areas.",
    ),
    (_, "Scallop") => (
      "Finishes 3D surfaces with passes a constant distance apart over the surface.",
      "Pick the area and a tool; the rings follow the shape so the stepover is even on slopes as well \
       as flats. Good for an even finish on mixed shapes.",
    ),
    (_, "Adaptive Clearing") => (
      "Roughs the stock away around the part with a light, constant engagement.",
      "Pick a tool and the stock to leave; the stock is removed level by level while the cutter's load \
       stays steady. Usually the first 3D operation, followed by finishing passes. Rest machining cuts \
       only what earlier, larger tools left.",
    ),
    (_, "Steep and Shallow") => (
      "Finishes steep areas with level passes and shallow areas with rows.",
      "Pick the area and a tool and set the angle where steep turns to shallow. It combines two \
       finishing strategies in one toolpath.",
    ),
    (_, "Contour") => (
      "Finishes steep walls with passes at constant heights.",
      "Pick the area and a tool; it cuts around the part level by level. Works best on steep walls; \
       pair it with Parallel or Scallop for the flatter areas.",
    ),
    (_, "Pencil") => (
      "Cleans out concave corners and fillets tighter than the tool.",
      "Pick the area and a tool; the tool runs along the inside corners left by earlier passes, and \
       more passes either side a step-over apart if asked. Use it as the last finishing step.",
    ),

    // MANUFACTURE DRILLING
    (_, "Drill") => (
      "Drills holes straight down from the top.",
      "Pick holes, or leave the selection empty to drill every hole; choose the cycle, such as peck \
       drilling or tapping, and the depth. Pick a drill matching the hole size from the tool library.",
    ),

    // MANUFACTURE MULTI-AXIS
    (_, "Rotary") => (
      "Wraps passes around the part as it turns on a 4th axis.",
      "Needs a setup on a rotary 4th axis. The tool stays square to the A axis and drops onto the part \
       as it turns: passes along X a step-over apart around it, or rings around it, in levels from the \
       stock down.",
    ),
    (_, "Swarf") => (
      "Cuts drafted or conical walls with the side of the tool, all five axes moving.",
      "Needs a setup on a 5-axis machine. Pick the walls; a flat end mill runs along them with its axis \
       on each wall's line from bottom to top, in passes a step-down apart. The post writes A/B and C \
       with inverse-time feeds.",
    ),

    // MANUFACTURE TURNING
    (_, "New Turning Setup") => (
      "Holds a bar in the lathe's chuck about the part's axis.",
      "The bar is the part's largest diameter plus the side stock; leave stock in front to face off and \
       a gap before the chuck to part off in. Z0 goes on the part's front face (or the stock front, or \
       the back face) and X0 on the spindle axis.",
    ),
    (_, "Turning Face") => (
      "Faces the front of the bar down to the part.",
      "Uses the library's turning insert unless you pick one; passes a depth apart from the stock front \
       to the part's front face, out past the centre.",
    ),
    (_, "Profile Roughing") => (
      "Roughs the outside down to the part in passes along the axis.",
      "Each pass takes the depth of cut off the diameter, climbs the shoulder it ends at and backs off at \
       45°, leaving the stock to leave for finishing. Undercuts behind a larger diameter are left for \
       grooving.",
    ),
    (_, "Profile Finishing") => (
      "Finishes the front face and the outside in one pass.",
      "Takes the finishing insert (the smallest nose radius in the library) from the centre up the face \
       and along the profile, compensated for the nose radius.",
    ),
    (_, "Groove") => (
      "Plunges the grooves and undercuts the profile passes can't reach.",
      "A grooving insert plunges across each groove, overlapping by the step-over and pecking; grooves \
       narrower than the insert are reported.",
    ),
    (_, "Turning Thread") => (
      "Cuts an outside thread on a cylindrical face.",
      "Pick the face and give the pitch; passes of equal chip area reach the ISO depth (or the depth \
       given) with a spring pass at the end, stopping short of a shoulder.",
    ),
    (_, "Part Off") => (
      "Parts the finished part off the bar at its back face.",
      "A part-off blade plunges through to the centre in pecks; it must fit in the gap before the chuck.",
    ),

    // MANUFACTURE ACTIONS
    (_, "Generate") => (
      "Computes the setup's toolpaths and machining time.",
      "Run it after changing operations or the design; operations show a warning until they're \
       regenerated. Check the result with Simulate before posting.",
    ),
    // RENDER
    (_, "Scene Settings") => (
      "Controls the environment, background, ground plane and camera for the rendered scene.",
      "Settings sets the brightness, position and background of the environment, the ground plane's \
       flattening and reflections, and the camera's projection, focal length, exposure and depth of \
       field. Environment Library sets the lighting environment, or attaches a custom image.",
    ),
    (_, "Light") => (
      "Adds light to control illumination, shadows and highlights.",
      "Combine lights with environment lighting to light the scene. You can add up to eight lights \
       per design.",
    ),
    (_, "Texture Map Controls") => (
      "Adjusts how a texture is mapped onto faces and bodies.",
      "Choose the projection (planar, box, spherical, cylindrical or automatic), then move, turn and \
       scale the texture in the canvas.",
    ),
    (_, "Dielectric Priority Control") => (
      "Sets which transparent material wins where two bodies overlap.",
      "Give each glass or liquid body a priority (0, 1, 2 ...). Where dielectric bodies meet, the higher \
       priority's material is rendered, as for a liquid in a glass.",
    ),
    (_, "In-canvas Render Settings") => (
      "Sets the quality and speed of the in-canvas render.",
      "Choose the quality the in-canvas render stops at, and whether it uses advanced ray tracing.",
    ),
    (_, "Capture Image...") => (
      "Saves an image of the canvas.",
      "Set the image size, resolution and anti-aliasing, and whether the background is transparent, then \
       save the canvas as a picture.",
    ),
    (_, "In-canvas Render") => (
      "Renders the design in the canvas with its appearances and scene settings.",
      "The render refines while it runs; sketches and planes are hidden until you stop it. The bar at \
       the lower right shows the quality reached, the elapsed time and the iteration number, and pauses \
       or resumes the render.",
    ),
    (_, "In-canvas Render Stop") => {
      ("Stops the in-canvas render.", "The canvas goes back to the ordinary shaded view.")
    }
    (_, "Render") => (
      "Renders an image of the design with its appearances and scene settings.",
      "Choose the image size and quality; the render refines on every core and is denoised at the end. \
       Finished renders are kept in the Rendering Gallery at the bottom of the window. The design must \
       be saved first.",
    ),
    (_, "Simulate") => (
      "Cuts the stock with the toolpaths and checks for gouges and collisions.",
      "Play the toolpaths and watch the stock being removed. Problems such as gouges or holder \
       collisions are flagged on the timeline.",
    ),
    (_, "Post Process") => (
      "Writes the setup's toolpaths as G-code for the machine.",
      "Pick the post-processor that matches your machine's controller and the output file. Check the \
       code in a simulator before running it.",
    ),
    (_, "Delete Setup") => (
      "Deletes the last setup and its operations.",
      "The setup's stock, operations and toolpaths go; the design itself is untouched. Make a new setup \
       to start machining from a different side.",
    ),
    (_, "2D Cut (Laser, Plasma, Waterjet)") => (
      "Cuts sheet profiles with a laser, plasma or waterjet.",
      "Pick the outlines to cut; the beam follows them with a kerf offset and lead-ins. Inner cuts are \
       done before the outside so the part stays in place.",
    ),
    (_, "Cut Service Check") => (
      "Checks a cut file for SendCutSend, OSH Cut, Ponoko or your own shop.",
      "Checks the sketch being edited, the sheet metal body's flat pattern or a DXF against the \
       service's rules: text not outlined, open contours, duplicate or overlapping cuts, features under \
       its minimum and parts it can't take. Zoom shows each issue; the cut estimate (area, cut length, \
       pierces) exports as CSV or JSON for a quote.",
    ),
    (_, "All Flat Patterns on One Sheet") => (
      "Lays every sheet metal part's flat pattern out in one Cut File sketch.",
      "Each flat pattern's outline, with its bend lines as construction, goes into a new sketch on XY, \
       in rows a gap apart that wrap before a part would run past the sheet width. Save the sketch as a \
       DXF to send all the parts in one file.",
    ),

    // SOLID CREATE
    (_, "Create Sketch") => (
      "Enters Sketch mode, where you create geometric profiles that define the foundation of a design. Then use \
       commands like Extrude, Revolve, and Loft to create 3D bodies from sketches.",
      "Select a construction plane, then create lines, arcs, or points to create sketch and construction geometry. \
       Constrain sketches with dimensions and constraints. Select Finish Sketch to exit Sketch mode.",
    ),
    (_, "Create Form") => (
      "Starts a freeform modelling space for sculpted T-spline shapes.",
      "Begin from a primitive such as a box or sphere and push, pull and crease it into organic shapes. \
       Finish the form to turn it back into a body.",
    ),
    (_, "Derive") => (
      "Brings bodies, sketches or parameters from another design into this one.",
      "Pick the design and the items to bring across. They stay linked to the source and update when it \
       changes.",
    ),
    (_, "Extrude") => (
      "Adds depth to open or closed sketch profiles or faces.",
      "Select an extrusion type, select sketch profiles or faces to extrude, then specify the distance and taper \
       angle. For the Thin Extrude type, specify the wall thickness.",
    ),
    (_, "Revolve") => (
      "Revolves a sketch profile or planar face around a selected axis.",
      "Select a profile or planar face and an axis to revolve around, then specify the extent type, direction, and \
       operation.",
    ),
    (_, "Sweep") => (
      "Sweeps a profile along a path.",
      "Pick the profile, then a path sketch of connected curves. Set how the profile twists or scales \
       along the way. Use it for handles, tubes and rails.",
    ),
    (_, "Loft") => (
      "Blends between profiles on two or more sketches.",
      "Pick the profiles in order; the shape transitions smoothly from one to the next. Add rails to \
       control the sides between them.",
    ),
    (_, "Rib") => (
      "Adds a thin stiffening wall from an open sketch line to the body.",
      "Pick an open sketch curve and set the thickness; the rib grows until it meets the body. Use it \
       to strengthen walls in moulded and printed parts.",
    ),
    (_, "Web") => (
      "Adds a network of thin walls from sketch lines.",
      "Pick open sketch lines; walls grow along the sketch normal until they meet the body. Use it for \
       crossing stiffeners inside a shell.",
    ),
    (_, "Emboss") => (
      "Raises or sinks sketch profiles or text on a face, flat or curved.",
      "Pick the profiles, then the face; set the depth and whether it's raised or recessed. The shape \
       wraps onto curved faces.",
    ),
    (_, "Hole") => (
      "Creates a hole based on user-specified values and selections.",
      "Select a face to place the hole then select edges to position the hole on the face, or select sketch points \
       to place multiple holes. Specify the hole type, tap type and size values.",
    ),
    (_, "Thread") => (
      "Adds a thread to cylindrical faces.",
      "Pick round faces; the size matches the diameter, using metric coarse unless you give a pitch. \
       Choose whether it's modelled as real geometry, which you'll want for 3D printing.",
    ),
    (_, "Box") => (
      "Creates a solid box.",
      "Pick a plane or face, draw the base rectangle, then set the height. Join it to, cut it from or \
       intersect it with bodies, or make it a new body.",
    ),
    (_, "Cylinder") => (
      "Creates a solid cylinder.",
      "Pick a plane or face, place the centre and the diameter, then set the height. Use it for pins, \
       bosses or quick round cuts.",
    ),
    (_, "Sphere") => (
      "Creates a solid sphere.",
      "Pick a plane or face, place the centre, then set the diameter. Join, cut or intersect it with \
       existing bodies, or make it a new body.",
    ),
    (_, "Torus") => (
      "Creates a solid torus, a ring with a round section.",
      "Pick a plane or face, place the centre and the ring's diameter, then the section's diameter. \
       Good for O-rings and handles.",
    ),
    (_, "Coil") => (
      "Creates a helical coil, such as a spring.",
      "Pick a plane and the centre, then set the diameter, pitch, number of turns and section shape. \
       Use it as a body or cut it to make a groove.",
    ),
    (_, "Pipe") => (
      "Creates a tube along a sketch path.",
      "Pick a path of lines and arcs, then set the section's shape, size and wall thickness. Faster \
       than sketching a profile and sweeping it.",
    ),
    (_, "Mirror") => (
      "Mirrors bodies, faces or features across a plane.",
      "Pick what to mirror, then the mirror plane or a planar face. Model half of a symmetric part and \
       mirror it to save work.",
    ),
    (_, "Thicken") => (
      "Thickens faces or surface bodies into solids.",
      "Pick faces or surfaces and set the thickness and the side to grow. Use it to make a solid wall \
       from a shaped surface.",
    ),
    (_, "Boundary Fill") => (
      "Makes solids from the closed spaces between bodies and planes.",
      "Pick the bodies and planes that bound the space, then choose which enclosed cells to fill. Use \
       it to fill or cut regions that are awkward to sketch.",
    ),
    (_, "Create Base Feature") => (
      "Starts a block of direct edits that aren't recorded in the history.",
      "Edits made until you finish the base feature are kept as one step in the timeline. Use it to \
       edit imported bodies that have no history.",
    ),
    (_, "Joint Origin") => (
      "Places a point and axes that joints can snap to.",
      "Pick a face, edge or point and adjust the position and direction. Use it where a joint needs a \
       location that isn't on existing geometry.",
    ),

    // PATTERN
    (_, "Rectangular Pattern") => (
      "Duplicates faces, bodies, features, or components in a design and distributes them along linear axes in rows \
       and columns to create a rectangular pattern.",
      "Select the object type, then select objects to pattern in the canvas, Browser, or Timeline. Select one or \
       more axes to distribute the pattern instances along, then adjust Suppression, quantity, and distance. \
       Check Suppression, then uncheck instances in the canvas to remove them from the pattern.",
    ),
    (_, "Circular Pattern") => (
      "Copies bodies, faces or features around an axis.",
      "Pick what to copy and the axis, then the number of copies and the angle they span. Use it for \
       bolt circles, spokes and teeth.",
    ),
    (_, "Pattern on Path") => (
      "Copies bodies, faces or features along a curve.",
      "Pick what to copy and a path, then set the number of copies and their spacing. The copies can \
       turn to follow the path's direction.",
    ),

    // SOLID MODIFY
    (_, "Press Pull") => (
      "Modifies the selected geometry using offset, extrude, or fillet commands. The operation depends on the \
       geometry selected.",
      "",
    ),
    (_, "Fillet") => (
      "Rounds the edges of a solid body by adding material to interior edges and removing material from exterior \
       edges.",
      "Select edges, faces, or features, then specify a radius. Use the Rule Fillet type to add fillets based on \
       specified rules. Use the Full Round Fillet type to round the edges over three adjacent faces.",
    ),
    (_, "Chamfer") => (
      "Bevels the selected edges.",
      "Pick edges and set an equal distance, two distances or a distance and an angle. Common on \
       outside edges for comfort and assembly.",
    ),
    (_, "Shell") => (
      "Offsets the walls of a solid body to create a hollow cavity with walls of a specified thickness.",
      "Select a body to shell or faces to remove to create an opening, then specify a type, direction, and wall \
       thickness.",
    ),
    (_, "Draft") => (
      "Applies a fixed or parting line draft angle to planar faces.",
      "Select a pull direction, parting tool, and faces to draft, then specify the draft angle.\n\nPress \
       Ctrl+/ for more help.",
    ),
    (_, "Scale") => (
      "Scales bodies about a point.",
      "Pick bodies and a point, then set a uniform factor or a different one per axis. Useful for \
       shrinkage allowances or resizing imported parts.",
    ),
    (_, "Combine") => (
      "Joins, cuts, or intersects two or more solid bodies to create a single solid body.",
      "Select the target body, select one or more tool bodies to combine with the target body, then select the \
       operation to use to combine the bodies together.",
    ),
    (_, "Offset Face") => (
      "Moves faces outwards or inwards along their normals.",
      "Pick faces, flat or curved, and set the distance. Neighbouring faces stretch to follow. Use it \
       to add clearance or thicken a wall.",
    ),
    (_, "Replace Face") => (
      "Replaces faces with a face of another body or a plane.",
      "Pick the faces to replace, then the new face. Neighbouring faces extend or trim to meet it.",
    ),
    (_, "Split Face") => (
      "Splits faces along a plane, face or sketch, keeping the body whole.",
      "Pick the faces and the splitting tool. The new faces can take their own appearance or be pushed \
       and pulled separately.",
    ),
    (_, "Split Body") => (
      "Cuts bodies into separate bodies along a plane or face.",
      "Pick the bodies and the splitting tool. Use it to divide a part for printing or to make the \
       halves of an enclosure.",
    ),
    (_, "Silhouette Split") => (
      "Splits faces along their outline as seen from a direction.",
      "Pick a direction and the body; faces are split where they turn away from that view. Use it to \
       find a mould's parting line.",
    ),
    (_, "Move/Copy") => (
      "Moves, rotates or copies bodies.",
      "Pick bodies, then drag the handles or type distances and angles. Turn on copy to leave the \
       originals in place.",
    ),
    (_, "Align") => (
      "Moves a body so that one of its faces or edges lines up with another.",
      "Pick geometry on the body to move, then the geometry to line it up with. Flip the direction if \
       it lands the wrong way round.",
    ),
    (_, "Delete") => (
      "Deletes the selection.",
      "Bodies, sketches, features and faces can be deleted. Deleting a feature removes it from the \
       timeline, and features that depend on it may fail.",
    ),
    (_, "Remove") => (
      "Removes bodies from the model while keeping a step in the timeline.",
      "The bodies disappear from that point in the timeline onwards. Unlike Delete, rolling back before \
       the step brings them back.",
    ),
    (_, "Physical Material") => (
      "Sets the material of bodies or components.",
      "Pick from the material library; density sets the mass, and the bill of materials lists the \
       material. It also gives a matching appearance.",
    ),
    (_, "Appearance") => (
      "Changes how bodies or faces look in the viewport.",
      "Drag an appearance onto bodies or faces, or pick them and choose one. This changes only the \
       look, not the material or the mass.",
    ),
    (_, "Manage Materials") => (
      "Edits the libraries of physical materials and appearances.",
      "Create and edit materials, change their properties and organise them into libraries you can \
       share. Materials you add then appear in Physical Material and Appearance.",
    ),
    (_, "Change Parameters") => (
      "Displays the Parameters dialog, where you can create and manage equations and relationships to \
       control the size of objects in the design.",
      "Model parameters list every dimension and feature value; add user parameters with expressions \
       that other values can reference.\n\nPress Ctrl+/ for more help.",
    ),
    (_, "Compute All") => (
      "Computes all of the parameters, features, and assembly relationships in a design.",
      "Use at major milestones in the design process and after making significant changes to features \
       in the Timeline, to ensure all assembly relationships are calculated properly.\n\nPress Ctrl+/ \
       for more help.",
    ),
    // (Fusion's Bill of Materials tooltip is its bare name: no text here)

    // DIRECT EDIT
    (_, "Move Face") => (
      "Moves or rotates faces directly, without editing sketches.",
      "Pick faces and drag the handles; neighbouring faces stretch to follow. Good for editing imported \
       bodies.",
    ),
    (_, "Delete Face") => (
      "Deletes faces and heals the body around the gap.",
      "Pick faces such as fillets, holes or bosses; neighbouring faces grow to close the gap. If they \
       can't close it, the body becomes a surface.",
    ),
    (_, "Finish Base Feature") => (
      "Finishes the block of direct edits and returns to the timeline.",
      "The edits made since the base feature started are kept as one step. Later features build on top \
       of it.",
    ),
    (_, "Capture Design History") => (
      "Turns on the timeline so features are recorded from now on.",
      "Features made after this become steps you can edit and reorder. The existing bodies become the \
       starting point.",
    ),

    // CONFIGURE
    (_, "Configure") => (
      "Opens the Configuration Table, enters Configuration Mode, and highlights configurable features and objects \
       in the Timeline and Browser.",
      "Add Configurations as rows in the table to create different variants of a design. Click highlighted features \
       and objects to add their configurable aspects as columns in the table.",
    ),
    (_, "Display Configuration Table") => {
      ("Shows or hides the Configure dialog without entering Configuration Mode.", "")
    }
    (_, "Create Configuration Rules") => (
      "Creates logical Configuration Rules that drive or limit configured aspect values in a Configured Design, to \
       ensure only valid configurations can be created.",
      "Drag and drop blocks into the Configuration Rules canvas to create logical rules, then apply them to the set \
       of configurations.",
    ),

    // ASSEMBLE
    (_, "Add To Assembly") => (
      "Inserts the active design into an Assembly Design as an external component.",
      "Select an Assembly Design to insert into, then use the manipulators in the canvas or Initial Position fields \
       to place the component in the target design.",
    ),
    (_, "New Component") => (
      "Makes a new component, or one from the selected bodies.",
      "Components hold their own bodies, sketches and origin, and can be jointed and moved. Make each \
       separate part a component.",
    ),
    (_, "Joint") => (
      "Joins two components at a face, edge or point of each.",
      "Pick a point on the component that moves, then one on the other. Choose how they can move, such \
       as rigid, revolute or slider, and check the motion in the preview.",
    ),
    (_, "As-built Joint") => (
      "Joins two components where they already sit.",
      "Pick the two components and where the joint goes; neither of them moves. Choose how they can \
       move, such as rigid, revolute or slider.",
    ),
    (_, "Drive Joints") => (
      "Sets a joint's rotation or slide to a value.",
      "Pick a joint and type the angle or distance. Components follow, so you can check positions and \
       clearances.",
    ),
    (_, "Motion Study") => (
      "Steps joints through a range of positions.",
      "Pick the joints and the values to step through, then play it. Interference between components is \
       flagged along the way.",
    ),
    (_, "Duplicate Component") => (
      "Adds another copy of a component that follows the original.",
      "The copy shares the original's design, so edits to either appear in both. Joint it or move it \
       into place.",
    ),
    (_, "Rigid Group") => (
      "Locks components together so they move as one.",
      "Pick the components; they keep their relative positions without needing joints between each \
       pair. Use it for subassemblies, such as a bolt, washer and nut, that never move relative to one \
       another.",
    ),
    (_, "Motion Link") => (
      "Links the motion of two joints, as gears or a rack and pinion do.",
      "Pick two joints and set how far one moves for a given motion of the other. Driving one then \
       drives the other.",
    ),
    (_, "Contact Set") => (
      "Makes components stop when they touch rather than pass through each other.",
      "Pick the components that should collide. When you drag or drive joints, their motion stops at \
       contact.",
    ),
    (_, "Insert Component") => (
      "Inserts another design as a linked component.",
      "Pick a design from the data panel; it's placed as a component that updates when the original \
       changes. Joint it into place.",
    ),
    (_, "Cam Follower") => (
      "Makes a follower ride along the profile of a turning cam.",
      "Pick the cam and the follower's joint, and a roller face if it has one. As the cam turns, the \
       follower slides, or swings as a rocker arm, to stay in contact.",
    ),

    // CONSTRUCT
    (_, "User Coordinate System (UCS)") => (
      "Places a local origin and axes for measuring and positioning.",
      "Pick an origin and directions for the axes. Use it as a reference for coordinates, sketches and \
       moves that don't line up with the main origin.",
    ),
    (_, "Offset Plane") => (
      "Creates a Construction Plane that is offset from the selected face or plane.",
      "Select a face, plane or sketch profile, then specify the offset distance.",
    ),
    (_, "Plane at Angle") => (
      "Creates a plane through an edge or axis, turned by an angle.",
      "Pick a straight edge, sketch line or axis, then set the angle. Good for sketching on slanted \
       features.",
    ),
    (_, "Tangent Plane") => (
      "Creates a plane tangent to a round face.",
      "Pick a cylinder, cone or sphere face at the point where the plane should touch, and an angle if \
       needed. Use it to sketch on the side of a round part.",
    ),
    (_, "Midplane") => (
      "Creates a plane halfway between two planes or faces.",
      "Pick two parallel or angled planes or faces. Use it as a mirror plane for symmetric parts.",
    ),
    (_, "Perpendicular Plane") => (
      "Creates a plane perpendicular to a face or plane.",
      "Pick a planar face or plane, choose which of its edge directions the new plane stands square \
       to, and set the distance along it.",
    ),
    (_, "Plane Through Two Edges") => (
      "Creates a plane through two straight edges.",
      "Pick two straight edges or lines that lie in one plane, such as opposite edges of a face. Use it \
       to sketch on a slanted plane defined by existing geometry.",
    ),
    (_, "Plane Through Three Points") => (
      "Creates a plane through three points.",
      "Pick three vertices, sketch points or construction points that don't lie on one line. The plane \
       follows the points if the model changes.",
    ),
    (_, "Plane Along Path") => (
      "Creates a plane across an edge, part of the way along it.",
      "Pick an edge or path and set the distance along it as a fraction. Use it to place sweep profiles \
       or sections partway along.",
    ),
    (_, "Axis Through Cylinder/Cone/Torus") => (
      "Creates an axis along the centre of a round face.",
      "Pick a cylindrical, conical or toroidal face. Use the axis for revolves, circular patterns and \
       joints.",
    ),
    (_, "Axis Perpendicular To Face") => (
      "Creates an axis normal to a face through a point.",
      "Pick a face, then the point the axis should pass through. Use it as a pivot for a revolve or a \
       circular pattern on a face.",
    ),
    (_, "Axis Through Two Planes") => (
      "Creates an axis where two planes meet.",
      "Pick two planes or planar faces that aren't parallel. Use it as a hinge line or a revolve axis \
       where no edge exists.",
    ),
    (_, "Axis Through Two Points") => (
      "Creates an axis through two points.",
      "Pick two vertices, sketch points or construction points. The axis follows the points if the \
       model changes.",
    ),
    (_, "Axis Through Edge") => (
      "Creates an axis along a straight edge or through a circle's centre.",
      "Pick a straight edge or sketch line, or a circular edge for its axis. Use it for revolves and \
       circular patterns about existing geometry.",
    ),
    (_, "Point At Vertex") => (
      "Creates a construction point at a vertex.",
      "Pick a corner of the model or an end of an edge. Use it as a reference for joints, planes and \
       measurements.",
    ),
    (_, "Point Through Two Edges") => (
      "Creates a point where two edges meet or cross.",
      "Pick two edges or lines; the point goes where they meet, or would if extended. Useful for \
       locating a sharp corner that a fillet has rounded away.",
    ),
    (_, "Point Through Three Planes") => (
      "Creates a point where three planes meet.",
      "Pick three planes or planar faces that meet in a single point. Use it as a reference for planes, \
       axes and joints.",
    ),
    (_, "Point At Center Of Circle/Sphere/Torus") => (
      "Creates a point at the centre of a round face or edge.",
      "Pick a circular edge, sphere or torus. Use the point for joints, measurements or patterns.",
    ),
    (_, "Point At Edge And Plane") => (
      "Creates a point where an edge crosses a plane.",
      "Pick an edge and a plane or planar face. The point follows the model if either changes.",
    ),
    (_, "Point Along Path") => (
      "Creates a point part of the way along an edge.",
      "Pick an edge or path and set the distance along it as a fraction. Use 0.5 for the middle of the \
       edge.",
    ),

    // INSPECT
    (_, "Measure") => (
      "Measures the distance, angle, area, or position of selected objects.",
      "Select an object to measure it, and optionally select a second object to measure it and the distance between \
       them.",
    ),
    (_, "Interference") => (
      "Finds where bodies of different components overlap.",
      "Pick the components to check, or check them all. The overlaps are shown with their volumes so \
       you can fix clashes before making parts.",
    ),
    (_, "Section Analysis") => (
      "Creates a view that cuts through all visible objects in a design to reveal interior details and obscured \
       features.",
      "Select a planar object to define the cut plane, then adjust the distance and angles to position it in the \
       design. In the Browser, the Section Analysis displays in the Analysis folder, where you can adjust its \
       visibility, edit it, or delete it.",
    ),
    (_, "Curvature Comb Analysis") => (
      "Shows how curvature varies along edges or curves as a comb.",
      "Pick curves or edges; the comb's spikes show the curvature at each point. Jumps in the comb mark \
       where a curve isn't smooth.",
    ),
    (_, "Curvature Map Analysis") => (
      "Colours faces by how sharply they curve.",
      "Pick bodies or faces and set the colour range. Sudden colour changes reveal bumps and flat spots \
       on smooth surfaces.",
    ),
    (_, "Draft Analysis") => (
      "Colours faces by whether they have enough draft for moulding.",
      "Pick the pull direction and the angle range. Faces that would stick in the mould stand out.",
    ),
    (_, "Environment Map Analysis") => (
      "Reflects a surrounding image on the faces to judge smoothness.",
      "Pick the bodies and an image; how the reflection flows across joins shows how smooth they are. \
       Wavy or broken reflections show where a surface needs work.",
    ),
    (_, "Isocurve Analysis") => (
      "Shows the internal grid lines of faces.",
      "Pick faces and set the density. The lines show how each face is parameterised, which helps \
       explain odd fillets or surfaces.",
    ),
    (_, "Zebra Analysis") => (
      "Reflects stripes on faces to show how smoothly they join.",
      "Pick the bodies and the stripe direction. Stripes that break at an edge mean a sharp join; \
       stripes that kink mean tangent; smooth stripes mean curvature continuous.",
    ),
    (_, "Accessibility Analysis") => (
      "Colours faces by whether a tool can reach them from a direction.",
      "Pick the direction the tool approaches from. Hidden faces stand out, which helps with machining \
       and moulding.",
    ),
    (_, "Center of Mass") => (
      "Shows the centre of mass of the selected bodies, or of all of them.",
      "A marker is placed at the centre of mass. Set materials first, as the result depends on density.",
    ),
    (_, "Minimum Radius Analysis") => (
      "Highlights where faces curve tighter than a radius.",
      "Pick the bodies and the smallest radius you can make, such as your tool's. Areas below it are \
       highlighted.",
    ),
    (_, "Navigator") => (
      "Opens a panel for finding and jumping between items in a large design.",
      "Search and filter the design's components and features, then click one to zoom to it. Useful in \
       assemblies with many components.",
    ),
    (_, "Display Component Colors") => (
      "Colours each component differently in the viewport.",
      "Toggle it to tell components apart in a busy assembly. Their real appearances return when you \
       turn it off.",
    ),
    (_, "Display Mesh Face Groups") => (
      "Colours a mesh body's face groups.",
      "Toggle it to see how a mesh is divided into groups, which affects how it converts to a solid. \
       Each group gets its own colour.",
    ),
    (_, "History") => (
      "Shows the design's saved versions and their changes.",
      "Browse earlier versions, compare them and promote one to be the current version. Every save \
       makes a new version you can return to.",
    ),
    (_, "Sustainability Insights") => (
      "Estimates the environmental impact of the design's materials.",
      "Shows estimates of the carbon footprint of the materials in the design, so you can compare \
       alternatives. The figures are estimates for comparing options, not certified values.",
    ),

    // INSERT
    (_, "Decal") => (
      "Places an image on a face like a sticker.",
      "Pick an image and a face, then position, scale and rotate it. It follows the face's curvature \
       and changes only the appearance.",
    ),
    (_, "Canvas") => {
      ("Places an image on a planar face or sketch plane.", "Select a face then select an image to import.")
    }
    (_, "Insert SVG") => (
      "Inserts an SVG file's shapes as sketch curves.",
      "Pick the file and the plane or face, then scale and position it. Closed shapes become profiles \
       you can extrude.",
    ),
    (_, "Insert DXF") => (
      "Inserts a DXF file's 2D geometry as sketch curves.",
      "Pick the file, the plane and the units. Use it for outlines drawn in other CAD software.",
    ),
    (_, "Insert Mesh") => (
      "Inserts an existing STL, OBJ, or 3MF file as a mesh body into the active component in a design.",
      "Select a file to insert. The contents of the file display as a mesh body in the canvas and the \
       browser.\n\nFor STL or OBJ files, select the unit type that matches the units from the original design \
       that generated the STL or OBJ.",
    ),
    (_, "Insert Fastener") => (
      "Inserts standard screws, nuts and washers from a library.",
      "Pick the fastener type and size, then place it on a hole. It comes in as a component.",
    ),
    (_, "Duplicate With Joints") => (
      "Copies a component onto several places, jointing each copy.",
      "Pick the component and the holes or edges to place it on. Each copy gets its own joint, which \
       suits fasteners.",
    ),
    (_, "Insert Derive") => (
      "Pulls and links design features from another design into the active design. When you update the derived \
       features in the original design, the changes are reflected in the destination design.",
      "Before you start, activate the component that you want to contain the derived design features. Select a \
       design, then select a set of components, bodies, sketches, construction geometry, flat patterns, or \
       parameters to derive into the current design.",
    ),
    (_, "Insert McMaster-Carr Component") => (
      "Inserts a part from the McMaster-Carr catalogue.",
      "Browse the supplier's online catalogue, pick a part and download its model straight into the \
       design as a component. Use it for screws, bearings and other bought-in parts.",
    ),
    (_, "Insert a manufacturer part") => (
      "Inserts a model of a part from a manufacturer's catalogue.",
      "Search by manufacturer or part number and insert the part's model as a component. Use it to \
       model with the real geometry of bought-in parts.",
    ),
    (_, "Insert TraceParts Supplier Components") => (
      "Inserts a supplier part from the TraceParts catalogue.",
      "Browse the catalogue of standard components, pick a part and size, and insert it as a component. \
       Use it for bearings, fasteners and other bought-in parts.",
    ),
    (_, "Insert STEP") => (
      "Inserts a STEP file as bodies.",
      "Pick the file; its solids come in as bodies without history. Use direct editing tools such as \
       Press Pull to change them.",
    ),

    // SELECT
    (_, "Select") => (
      "Selects faces, edges, bodies and profiles by clicking.",
      "Click to select and Ctrl+click or Shift+click to add or remove. Selection priority and filters \
       decide what a click picks.",
    ),
    (_, "Window Selection") => (
      "Selects with a box drag.",
      "Drag left to right to select what's entirely inside the box, or right to left to select what it \
       touches. The selection priority and filters still decide what kinds of item are picked.",
    ),
    (_, "Freeform Selection") => (
      "Selects with a freehand outline.",
      "Draw around the items; everything inside the outline is selected. Useful for irregular groups.",
    ),
    (_, "Paint Selection") => (
      "Selects whatever the cursor passes over while dragging.",
      "Hold the button and brush over faces or edges to select them. Useful for many small faces.",
    ),
    (_, "Select All") => (
      "Includes or excludes every object type.",
      "Checked, every selection filter is on; uncheck it to turn them all off, then check only the \
       types you want to select.",
    ),
    (_, "Select Through") => (
      "Selects hidden objects during a window or freeform selection.",
      "With it on, a window or freeform selection also takes objects hidden behind others. Off, only \
       what is visible is selected.",
    ),
    (_, "Select By Name") => (
      "Selects bodies and components by name.",
      "Type a name, with * for any run of characters and ? for one, and OK selects every visible \
       body or component whose name matches, ignoring case.",
    ),
    (_, "Select By Boundary") => (
      "Selects the faces inside a closed chain of edges.",
      "Select edges that close a boundary on a body, then OK selects the faces on the smaller side of \
       it. Useful for picking a patch of faces at once.",
    ),
    (_, "Select By Size") => (
      "Selects bodies or faces up to a size.",
      "Enter the largest size to take: bodies by the diagonal of their bounding box, faces by the \
       square root of their area. Useful for finding slivers and small parts.",
    ),
    (_, "Seed And Boundary") => (
      "Selects the faces connected to a seed face, up to boundary edges.",
      "Select a seed face and the edges that bound the region, then OK selects every face reachable \
       from the seed without crossing the boundary.",
    ),
    (_, "Body Faces") => (
      "Allows selecting faces.",
      "Turn it off to stop faces being picked when you want edges or bodies. The filters combine with \
       the selection priority.",
    ),
    (_, "Body Edges") => (
      "Allows selecting edges.",
      "Turn it off to stop edges being picked when you want faces or bodies. The filters combine with \
       the selection priority.",
    ),
    (_, "Body Vertices") => (
      "Allows selecting vertices.",
      "Turn it off to stop corner points being picked by accident. The filters combine with the \
       selection priority.",
    ),
    (_, "Sketch Curves") => (
      "Allows selecting sketch curves.",
      "Turn it off to click through sketches to the model behind them. The filters combine with the \
       selection priority.",
    ),
    (_, "Sketch Points") => (
      "Allows selecting sketch points.",
      "Turn it off to click through sketch points to the curves and model behind them. The filters \
       combine with the selection priority.",
    ),
    (_, "Construction Geometry") => (
      "Allows selecting construction planes, axes and points.",
      "Turn it off to click through construction geometry to the model. The filters combine with the \
       selection priority.",
    ),
    (_, "Components") => (
      "Allows selecting components.",
      "Turn it off to stop whole components being picked under the component priority. The filters \
       combine with the selection priority.",
    ),
    (_, "Coordinate Systems") => (
      "Allows selecting coordinate systems.",
      "Turn it off to click through user coordinate systems to the model. The filters combine with \
       the selection priority.",
    ),
    (_, "Custom Graphics") => (
      "Allows selecting custom graphics.",
      "Turn it off to click through graphics drawn by add-ins to the model. The filters combine with \
       the selection priority.",
    ),
    (_, "Features") => (
      "Allows selecting features.",
      "Turn it off to stop timeline features being picked in the canvas. The filters combine with \
       the selection priority.",
    ),
    (_, "Joint Origins") => (
      "Allows selecting joint origins.",
      "Turn it off to click through joint origin glyphs to the model. The filters combine with the \
       selection priority.",
    ),
    (_, "Joints") => (
      "Allows selecting joints.",
      "Turn it off to click through joint glyphs to the model. The filters combine with the \
       selection priority.",
    ),
    (_, "Mesh Bodies") => (
      "Allows selecting mesh bodies.",
      "Turn it off to stop mesh bodies being picked when you want solids. The filters combine with \
       the selection priority.",
    ),
    (_, "Mesh Face Groups") => (
      "Allows selecting mesh face groups.",
      "Turn it off to pick single mesh faces instead of the groups they belong to. The filters \
       combine with the selection priority.",
    ),
    (_, "Mesh Faces") => (
      "Allows selecting mesh faces.",
      "Turn it off to stop single mesh faces being picked. The filters combine with the selection \
       priority.",
    ),
    (_, "PMI") => (
      "Allows selecting product manufacturing information.",
      "Turn it off to click through PMI annotations to the model. The filters combine with the \
       selection priority.",
    ),
    (_, "Sketch Geometry Constraint") => (
      "Allows selecting sketch constraints.",
      "Turn it off to click through constraint glyphs to the geometry under them. The filters combine \
       with the selection priority.",
    ),
    (_, "Sketch Surface") => (
      "Allows selecting sketch surfaces.",
      "Turn it off to click through surfaces made from sketches to what is behind them. The filters \
       combine with the selection priority.",
    ),
    (_, "T-Spline Body") => (
      "Allows selecting T-Spline bodies.",
      "Turn it off to stop form bodies being picked when you want solids. The filters combine with \
       the selection priority.",
    ),
    (_, "Text Selection") => (
      "Allows selecting sketch text.",
      "Turn it off to click through sketch text to the geometry under it. The filters combine with \
       the selection priority.",
    ),
    (_, "Invert Selection") => (
      "Selects what isn't selected, and clears what is.",
      "Handy for selecting everything except a few items: select those, then invert. The current \
       filters decide what can be selected.",
    ),
    (_, "Select Body Priority") => (
      "Makes clicks and boxes select whole bodies.",
      "Use it when you want to move, combine or delete bodies without picking a face by accident. \
       Clicking a face selects the body it belongs to.",
    ),
    (_, "Select Face Priority") => (
      "Makes clicks select faces, and edges near them.",
      "Use it for face operations such as Press Pull, Offset Face and appearances. Edges are still \
       picked when you click close to them.",
    ),
    (_, "Select Edge Priority") => (
      "Makes clicks and boxes select edges only.",
      "Use it to box-select many edges for fillets and chamfers. Faces and bodies are ignored while \
       it's on.",
    ),
    (_, "Select Component Priority") => (
      "Makes clicks select every body of a component.",
      "Use it in assemblies to move or hide whole components at once. Clicking any body selects the \
       component that holds it.",
    ),
    (_, "Bodies") => (
      "Allows selecting bodies.",
      "Turn it off to make clicks pass through to other kinds of item. The filters combine with the \
       selection priority.",
    ),
    (_, "Faces") => (
      "Allows selecting faces.",
      "Turn it off to stop faces being picked when you want edges or bodies. The filters combine with \
       the selection priority.",
    ),
    (_, "Edges") => (
      "Allows selecting edges.",
      "Turn it off to stop edges being picked when you want faces or bodies. The filters combine with \
       the selection priority.",
    ),
    (_, "Vertices") => (
      "Allows selecting vertices.",
      "Turn it off to stop corner points being picked by accident. The filters combine with the \
       selection priority.",
    ),
    (_, "Sketches") => (
      "Allows selecting sketch curves.",
      "Turn it off to click through sketches to the model behind them. The filters combine with the \
       selection priority.",
    ),
    (_, "Sketch Profiles") => (
      "Allows selecting closed sketch profiles.",
      "Turn it off to click through profiles to the faces behind them. The filters combine with the \
       selection priority.",
    ),
    (_, "Construction") => (
      "Allows selecting construction planes, axes and points.",
      "Turn it off to click through construction geometry to the model. The filters combine with the \
       selection priority.",
    ),

    // SURFACE CREATE
    (_, "Patch") => (
      "Creates a planar or 3D surface within the boundary of a specified closed loop.",
      "Select the loop to patch then apply connected (G0), tangent (G1), and curvature (G2) boundary \
       conditions.",
    ),
    (_, SURFACE_EXTRUDE) => (
      "Adds depth to open or closed sketch profiles or planar faces.",
      "Select the profile or planar face then specify the distance to extrude.\n\nPress Ctrl+/ for more \
       help.",
    ),
    (_, SURFACE_SWEEP) => (
      "Sweeps a sketch profile or planar face along a selected path.",
      "Select the profile then select the sweep path. The distance for the sweep is a decimal percentage \
       of the selected path.",
    ),
    (_, "Ruled") => (
      "Creates a ruled surface from an edge.",
      "Pick edges and set the distance and direction, such as along the normal or tangent. Use it to \
       extend a flange or make a run-off.",
    ),
    (_, "Offset") => (
      "Creates new surfaces offset from faces along their normals.",
      "Pick faces and set the distance and side. The originals stay; use the copies as walls or \
       references.",
    ),

    // SURFACE MODIFY
    (_, "Trim") => (
      "Removes the selected area of a surface body using the specified cutting tool.",
      "Select the cutting tool to use then select faces or surface bodies to remove.",
    ),
    (_, "Untrim") => (
      "Removes holes from faces or restores their original boundary.",
      "Pick a face and the loops to remove, or untrim the face completely back to its full shape; an \
       extension grows the restored boundary outward. Use it to start a face's outline again, or to \
       close holes in a surface.",
    ),
    (_, "Extend") => (
      "Enlarges a surface by extending its edges a specified distance.",
      "Select the edges of a surface, select an extension type, then specify the distance to extend. \
       Select an edge alignment option to control the curvature continuity between faces.",
    ),
    (_, "Stitch") => (
      "Joins surfaces to make a single surface body or solid body.",
      "Select the surfaces to join. Edges within the tolerance are highlighted green.",
    ),
    (_, "Unstitch") => (
      "Breaks joined surfaces into individual surfaces.",
      "Select faces to unstitch. The selected face is unhooked from the parent body and the result body \
       contains the unhooked face.",
    ),
    (_, "Reverse Normal") => (
      "Flips which way faces point.",
      "Pick surface or mesh bodies; their inside and outside swap. Fix it when thickening or printing \
       goes the wrong way.",
    ),

    // MESH
    (_, "Tessellate") => (
      "Converts an existing solid or surface body into a mesh body.",
      "Select the solid or surface body to tessellate, then adjust the mesh refinement settings to \
       convert the body to a mesh body.",
    ),
    (_, "Create Mesh Section Sketch") => (
      "Creates a sketch that contains a section line where a plane intersects a mesh body.",
      "Pick the mesh and a plane; the section's outline becomes sketch curves. Use them to rebuild a \
       scanned part as a solid.",
    ),
    (_, "Create Base Mesh Feature") => (
      "Starts a block of mesh edits that aren't recorded in history.",
      "Mesh edits made until you finish it are kept as one step in the timeline. Use it on imported \
       meshes, which have no history of their own.",
    ),
    (_, "Repair") => (
      "Fixes holes, flipped triangles and other problems in a mesh.",
      "Pick the mesh and a repair type, from closing holes to rebuilding it. Repair scans before \
       printing or converting them.",
    ),
    (_, "Generate Face Groups") => (
      "Divides a mesh into groups of faces that belong together.",
      "Pick the mesh and the accuracy; flat and curved regions become groups. Good groups make cleaner \
       solids when converting.",
    ),
    (_, "Combine Face Groups") => (
      "Merges mesh face groups into one.",
      "Pick the groups to merge. Use it to tidy grouping before converting a mesh to a solid.",
    ),
    (_, "Direct Edit") => (
      "Edits a mesh's triangles directly.",
      "Select triangles, then delete, move or fill them. Use it for small fixes to scanned meshes.",
    ),
    (_, "Finish Direct Editing") => (
      "Finishes direct mesh editing and returns to the MESH tab.",
      "The edited mesh is kept as a new body named after the original with _Edited, in the base \
       feature the editing started.",
    ),
    (_, "Texture Extrude") => (
      "Extrudes a texture into or out of the selected mesh faces.",
      "Select mesh faces and a texture image; the faces are pushed in or out by the image's \
       brightness.",
    ),
    (_, "Close Cracks") => (
      "Closes narrow cracks between mesh faces.",
      "Select the mesh body; open edges that nearly touch are joined so the mesh closes.",
    ),
    (_, "Erase And Fill") => (
      "Deletes the selected mesh faces and fills the hole they leave.",
      "Select faces with the Mesh Selection Palette; the hole is filled with a smooth patch.",
    ),
    (_, "Bridge") => (
      "Connects two open boundaries of a mesh with new faces.",
      "Select an open edge on each side; the faces between them are filled in.",
    ),
    (_, "Mesh Align") => (
      "Aligns a mesh body to another body.",
      "Select points on the mesh and matching points on the target; the mesh moves to fit them.",
    ),
    (_, "Remesh") => (
      "Refines or coarsens faces on a mesh body or face groups.",
      "Select a mesh body or face groups on a mesh body, then adjust the settings to regenerate the mesh faces.",
    ),
    (_, "Reduce") => (
      "Reduces the number of triangles on mesh bodies.",
      "Pick the meshes and how far to reduce them. Use it to make large scans lighter to work with, \
       keeping their shape.",
    ),
    (_, "Plane Cut") => (
      "Cuts a mesh body along a plane.",
      "Pick the mesh and the plane, then keep one side, both or split it. Fill the cut to keep the mesh \
       closed.",
    ),
    (_, "Smooth") => (
      "Smooths the surface of mesh bodies.",
      "Pick the meshes and the amount. It evens out noise from scans, at the cost of some detail.",
    ),
    (_, "Separate") => (
      "Splits a mesh into a body per disconnected piece or face group.",
      "Pick the mesh. Use it to work on loose shells or regions one at a time.",
    ),
    (_, "Scale Mesh") => (
      "Scales mesh bodies.",
      "Pick the meshes and set a factor, or the size in each direction. Use it to fix scans imported in \
       the wrong units.",
    ),
    (_, "Convert Mesh") => (
      "Converts mesh bodies to solid bodies.",
      "Flat regions become flat faces. Use it for simple scanned or imported parts, then edit them with \
       solid tools.",
    ),

    // EXPORT
    (_, "3D Print") => (
      "Exports bodies as a mesh file for 3D printing.",
      "Pick the bodies and the refinement; the result is written as an STL file for your slicer. Check \
       that the bodies are closed solids first.",
    ),

    // SHEET METAL CREATE
    (_, "Flange (Base/Edge/Contour)") => (
      "Creates sheet metal from a profile, an open path or sheet edges.",
      "Pick a closed profile for a flat base, an open sketch for a bent contour, or edges of a sheet to \
       add a flange. The thickness and bend radius come from the sheet metal rule.",
    ),
    (_, "Hem") => (
      "Folds sheet edges back on themselves.",
      "Pick edges and choose the hem type and gap. Hems stiffen edges and remove sharp ones.",
    ),
    (_, "Lofted Flange") => (
      "Creates sheet metal blending between two profiles.",
      "Pick a profile in each of two sketches; the transition is made of bendable sheet. Closed \
       profiles get a seam so the part can unfold.",
    ),
    (_, "Miter Flange") => (
      "Sweeps a flange along a chain of sheet edges, mitred at the corners.",
      "Sketch an open profile across the first edge, starting on it, then pick the edges to run along. \
       Corners are closed as set by the rule.",
    ),
    (_, "Fold") => (
      "Bends a flat sheet along a sketch line.",
      "Pick the sheet and a line on it, then set the angle and which side stays put. The bend uses the \
       rule's radius.",
    ),
    (_, "Convert to Sheet Metal") => (
      "Turns a thin solid into sheet metal.",
      "Pick a face to keep fixed; edges between faces become bends. The body must have an even \
       thickness.",
    ),
    (_, "Create Flat Pattern") => (
      "Unfolds a sheet metal body into its flat blank.",
      "Pick the face that stays fixed. The flat pattern opens in its own environment with its bend \
       lines; export it as a DXF or STEP for cutting, then Finish Flat Pattern.",
    ),
    (_, "Export Flat Pattern") => (
      "Exports the flat pattern as a DXF or STEP file.",
      "Choose the file type and units, and whether bend center lines and extent lines go in the file.",
    ),
    (_, "Finish Flat Pattern") => (
      "Finishes the flat pattern and returns to the folded model.",
      "The flat pattern stays in the browser; Activate it there to come back.",
    ),
    (_, "Flat Pattern Sketch") => (
      "Makes an editable sketch of the flat pattern.",
      "The outline and bend lines become sketch curves. Use them to dimension or export the blank.",
    ),

    // SHEET METAL MODIFY
    (_, "Unfold") => (
      "Flattens bends of a sheet metal body onto a face.",
      "Pick the fixed face and the bends to unfold. Add cuts while flat, then use Refold in the REFOLD \
       panel to bend it back.",
    ),
    (_, "Refold") => (
      "Folds the unfolded bends back up, keeping cuts made while flat.",
      "Runs at once on the bends the last Unfold flattened. Cuts made across them follow the bends.",
    ),
    (_, "Sheet Metal Rules") => (
      "Sets the thickness, bend radius and reliefs of sheet metal.",
      "Pick or edit a rule for the material. Changing the rule updates every sheet metal body that uses \
       it.",
    ),
    (_, "Corner Closure") => (
      "Sets how bent flanges close at the corners.",
      "Pick the corners and the closure type, gap and relief. Use it to make tidy corners for welding \
       or to avoid tearing.",
    ),
    (_, "Join by bend") => (
      "Joins two flat sheet faces with a bend.",
      "Pick an edge on each sheet; a bend is added to connect them, sized by the rule. Use it to \
       connect flanges made separately.",
    ),
    (_, "Rip") => (
      "Slits a solid along edges so it can unfold.",
      "Pick the edges or a point to cut the body along. Use it before converting a closed box shape \
       into sheet metal.",
    ),
    (_, "Sheet Metal Fillet") => (
      "Rounds the corner edges of sheet metal through its thickness.",
      "Pick corner edges and set the radius. The rounding follows the flat pattern correctly.",
    ),
    (_, "Sheet Metal Chamfer") => (
      "Bevels the corner edges of sheet metal through its thickness.",
      "Pick corner edges and set the distance. The bevel follows the flat pattern correctly.",
    ),

    // SIMPLIFY
    (_, "Remove Faces") => (
      "Removes faces such as fillets, holes or bosses and heals the body.",
      "Pick the faces; neighbouring faces grow to close the gap. Use it to simplify a model for \
       analysis or to share it.",
    ),
    (_, "Remove Features") => (
      "Finds small features by size and removes them.",
      "Set the size limits for holes, fillets and chamfers; features below them are removed. Use it to \
       simplify a detailed part.",
    ),
    (_, "Replace with Primitives") => (
      "Replaces bodies with simple boxes or cylinders that enclose them.",
      "Pick the bodies and the shape. Use it to make a light stand-in for detailed purchased parts.",
    ),

    // CREATE PCB
    (_, "Associative PCB") => (
      "Creates a circuit board linked to an electronics design.",
      "Sketch or pick the board outline; the outline and component positions stay in sync with the \
       electronics design in both directions. Changes made on either side can be pushed to the other.",
    ),
    (_, "Create 3D PCB") => (
      "Creates a 3D circuit board body from a sketch outline.",
      "Pick a closed profile and set the board thickness. Use it to fit enclosures around a board.",
    ),

    // UTILITY
    (_, "Manage Threads Library") => (
      "Edits the thread sizes that Thread and Hole offer.",
      "Add custom thread standards or sizes so they appear in the thread lists. Use it for non-standard \
       pitches or in-house thread forms.",
    ),

    // ADD-INS
    (_, "Scripts and Add-Ins...") => (
      "Installs and manages plugins that extend the app.",
      "Install a .foxplugin file (or drop it on the window) or one shared in your data hub, update it, \
       turn it off, change what it may do and read its log. Its commands get their own toolbar tab.",
    ),
    (_, "Design and Make Marketplace") => (
      "Browses an online store of add-ins and content.",
      "Find, install and update add-ins, libraries and content made by other developers. Installed \
       add-ins are then managed with Scripts and Add-Ins.",
    ),

    // INSPECT > Properties
    (_, "Properties") => (
      "Shows a panel with the properties of what is selected.",
      "Docks on the right and follows the selection: type, id, component and geometry (area, length, \
       normal, box, volume, mass, sketch plane), and the overall width, height and thickness of the \
       selected bodies, or of all of them when nothing is selected. Copy or export them as JSON, text or CSV.",
    ),

    // FissionOxide's own sketch tools
    (_, "Scale to Size") => (
      "Scales sketch geometry to a width or height, or to fit in a box.",
      "The size is the curves' real outline, arcs and splines included. In a box, Rotate to fit turns the \
       shape to the angle that lets it grow most; a margin keeps it that far inside each side.",
    ),
    (_, "Sketch Library...") => (
      "Places standard parts from the sketch library into the sketch.",
      "Pick a part, set its angle, then click in the sketch to drop it there, as often as you like. Parts \
       are DXF, SVG or .fox files in the hub's library folder, folders from settings or an add-in's pack.",
    ),
    // an Assembly Design's ASSEMBLY tab
    (_, "Create Linked PCB") => (
      "Creates a printed circuit board linked to an electronics design.",
      "Pick the board's outline and plane; the board stays linked to its electronics design and updates \
       when the design changes. FissionOxide doesn't link boards to electronics designs yet.",
    ),
    (_, "Constrain Components") => (
      "Positions components relative to each other with constraints.",
      "Pick faces, edges or points on two components and a relation between them, such as mate, flush \
       or offset. FissionOxide places components with joints instead.",
    ),
    (_, "Align Constraint") => (
      "Lines up faces, edges or axes of two components.",
      "Pick a face, edge or axis on the component that moves, then one on the other; choose flush, \
       coincident or concentric. The constraint stays in the timeline.",
    ),
    (_, "Angle Constraint") => (
      "Holds two components at an angle to each other.",
      "Pick a face or edge on each component and type the angle between them. FissionOxide places \
       components with joints instead.",
    ),
    (_, "Center Constraint") => (
      "Centres one component's cylindrical face or axis on another's.",
      "Pick a round face, edge or axis on each component; their centre lines are made to coincide. \
       FissionOxide places components with joints instead.",
    ),
    (_, "Tangent Constraint") => (
      "Keeps a face of one component touching a curved face of another.",
      "Pick the two faces; the moving component rolls or slides along the other while staying in \
       contact. FissionOxide places components with joints instead.",
    ),
    (_, "Enable Contact Sets") => (
      "Turns contact sets on, so the components in them stop where they touch.",
      "With contact sets on, dragging or driving joints stops a component when it meets another in the \
       same set. FissionOxide's contact sets are always on.",
    ),
    (_, "Enable All Contact") => (
      "Makes every component stop where it touches any other.",
      "Treats all components as one contact set while you drag or drive joints. FissionOxide checks \
       contact only within the contact sets you make.",
    ),
    _ => return None,
  };
  Some(Tip { line, more })
}

/// The SURFACE tab's Extrude and Sweep: Fusion's surface commands of those names, whose
/// tooltips differ from the SOLID ones.
pub(crate) const SURFACE_EXTRUDE: &str = "Surface Extrude";
pub(crate) const SURFACE_SWEEP: &str = "Surface Sweep";

/// The label the SURFACE tab's own version of the command `label` has its tooltip under.
pub(crate) fn surface_label(label: &'static str) -> &'static str {
  match label {
    "Extrude" => SURFACE_EXTRUDE,
    "Sweep" => SURFACE_SWEEP,
    other => other,
  }
}

/// What a dropdown row's tooltip says, as Fusion's: its paragraphs, then "Press Ctrl+/ for more
/// help." where Fusion shows it. The SKETCH > CONSTRAINTS rows carry Fusion's own descriptions;
/// other rows say what the toolbar tooltip says, without its title and shortcut.
pub(crate) fn menu_paragraphs(scope: Scope, label: &str) -> Vec<&'static str> {
  let fusion: Option<&[&'static str]> = match (scope, label) {
    (Scope::Sketch, "Horizontal/Vertical") => Some(&[
      "Constrains a single line, or two points, to lie on either the vertical or horizontal axis, whichever is \
       closer to the current alignment.",
    ]),
    (Scope::Sketch, "Coincident") => Some(&[
      "Constrains the position of two points or a point and a line or curve together.",
      "This could be a point constrained along a line, two points constrainted to each other, or a point \
       constrained along a curve.",
    ]),
    (Scope::Sketch, "Tangent") => {
      Some(&["Constrains a curve and another object so that they touch at a single point but never cross each other."])
    }
    (Scope::Sketch, "Equal") => {
      Some(&["Constrains similar objects so that their sizes are identical. When the size of one object changes, the \
       others adjust, too."])
    }
    (Scope::Sketch, "Parallel") => {
      Some(&["Constrains two lines so that they extend in the same direction and never intersect."])
    }
    (Scope::Sketch, "Perpendicular") => {
      Some(&["Constrains two objects so that they lie perpendicular (at a 90-degree angle) to each other."])
    }
    (Scope::Sketch, "Fix/UnFix") => Some(&["Locks the size and location of a point or object."]),
    (Scope::Sketch, "MidPoint") => Some(&["Constrains a point or object to the midpoint of another object."]),
    (Scope::Sketch, "Concentric") => {
      Some(&["Constrains two or more arcs, circles, or ellipses to the same center point."])
    }
    (Scope::Sketch, "Collinear") => Some(&[
      "Constrains two or more objects so that they share a common line.",
      "This could be a line and a rectangle, where the line and one edge of the rectangle lie along the same line.",
    ]),
    (Scope::Sketch, "Symmetry") => {
      Some(&["Constrains two or more objects so that they are symmetrical (identical to each other in relation to a \
       common axis)."])
    }
    (Scope::Sketch, "Curvature") => {
      Some(&["Constrains two or more objects to create a smooth, continuous curvature between them (G2 continuity)."])
    }
    (Scope::Sketch, "Polygon") => {
      Some(&["Constrains a closed sketch profile with sides of the same length and equal angles as a regular polygon."])
    }
    _ => None,
  };
  match fusion {
    Some(p) => p.iter().copied().chain(std::iter::once(MORE_HELP)).collect(),
    None => tip(scope, label).map(|t| t.paragraphs().collect()).unwrap_or_default(),
  }
}

/// What the workspace menu's tooltip for `workspace` says under its "<Name> Workspace" title.
/// Render's and Generative Design's are Fusion's words; the rest say the same kind of thing.
pub(crate) fn workspace(workspace: &str) -> Option<&'static str> {
  Some(match workspace {
    "Design" => "Creates and edits 3D models of parts and assemblies.",
    "Generative Design" => {
      "Creates multiple designs that meet your geometric, performance, and manufacturing requirements."
    }
    "Render" => "Generates realistic renderings of the design.",
    "Animation" => "Creates exploded views and animations of the design's assemblies.",
    "Simulation" => "Tests how the design performs under loads, heat, and other real-world conditions.",
    "Manufacture" => "Creates toolpaths to machine the design and builds to 3D print it.",
    "Drawing" => "Creates 2D drawings of the design or of an animation.",
    "Electronics" => "Creates schematics, PCBs, and electronics libraries.",
    _ => return None,
  })
}

/// The last line of Fusion's tooltips for commands with a help page.
pub(crate) const MORE_HELP: &str = "Press Ctrl+/ for more help.";

impl Tip {
  /// The description, then each usage paragraph.
  pub(crate) fn paragraphs(&self) -> impl Iterator<Item = &'static str> {
    std::iter::once(self.line).chain(self.more.split("\n\n").filter(|p| !p.is_empty()))
  }
}

/// Whether the ribbon tooltip of `label` grows, after resting on it, with Fusion's illustration
/// (a rendered picture of the command's result; FissionOxide paints the command's icon large
/// until pictures exist). Fusion has none for Display Configuration Table, Offset Plane and Add
/// To Assembly, which show "Press Ctrl+/ for more help." at once and never grow, nor for the
/// commands whose usage text already ends with that line.
pub(crate) fn illustrated(scope: Scope, label: &str) -> bool {
  match tip(scope, label) {
    None => false,
    Some(t) => {
      !matches!(label, "Display Configuration Table" | "Offset Plane" | "Add To Assembly")
        && !t.more.ends_with(MORE_HELP)
    }
  }
}
