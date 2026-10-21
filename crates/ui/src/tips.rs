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
}

/// A command's tooltip text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tip {
  /// One line under the name.
  pub line: &'static str,
  /// The fuller explanation shown after resting on the command.
  pub more: &'static str,
}

/// The tooltip for the command `label` on the `scope` toolbar.
pub(crate) fn tip(scope: Scope, label: &str) -> Option<Tip> {
  use Scope::*;
  let (line, more) = match (scope, label) {
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

    // SKETCH CHAMFER
    (_, "Equal Distance Chamfer") => (
      "Bevels a sketch corner by the same distance along both lines.",
      "Click the corner where two lines meet, or each line, then set the distance. A straight line \
       replaces the corner and both lines are trimmed back.",
    ),
    (_, "Distance-Distance Chamfer") => (
      "Bevels a sketch corner by a different distance along each line.",
      "Click the two lines, then set a distance for each. The first distance goes on the first line you \
       pick.",
    ),
    (_, "Distance and Angle Chamfer") => (
      "Bevels a sketch corner by a distance and an angle.",
      "Click the two lines, then set the distance along the first one and the angle of the bevel from \
       it. Use it when a drawing gives the chamfer as, say, 2 mm at 30 degrees.",
    ),

    // SKETCH MODIFY
    (Sketch, "Fillet") => (
      "Rounds the corner where two sketch curves meet.",
      "Click the corner, or the two curves, then set the radius. An arc tangent to both replaces the \
       corner and the curves are trimmed to it.",
    ),
    (_, "Blend Curve") => (
      "Joins the ends of two curves with a smooth spline.",
      "Pick near the ends of two curves; a spline bridges the gap, matching their direction or their \
       curvature too. Use it for flowing transitions that an arc can't make.",
    ),
    (Sketch, "Offset") => (
      "Copies a chain of sketch curves at a constant distance.",
      "Pick a curve; the connected chain is offset, then drag or type the distance and flip the side if \
       needed. Use it for walls of even thickness or a clearance around a shape.",
    ),
    (Sketch, "Trim") => (
      "Removes the piece of a sketch curve you click, up to where it crosses others.",
      "Hover to preview the piece that will go, then click it. Drag across several curves to trim them \
       all at once. A curve with no crossings is deleted entirely.",
    ),
    (Sketch, "Extend") => (
      "Lengthens a sketch curve until it meets the next curve.",
      "Click near the end you want to extend; it grows to the nearest curve in its path. Use it to \
       close a gap so a profile becomes closed.",
    ),
    (_, "Break") => (
      "Splits a sketch curve where it crosses other curves, keeping every piece.",
      "Click a curve; it's cut into separate pieces at its crossings. Unlike Trim nothing is removed, \
       so you can then constrain or delete the pieces one by one.",
    ),
    (_, "Sketch Scale") => (
      "Scales sketch geometry about a point.",
      "Pick the curves and a base point, then set the scale factor. Dimensions and constraints are \
       kept, so the shape stays the same while its size changes.",
    ),
    (Sketch, "Move/Copy") => (
      "Moves, rotates or copies sketch geometry.",
      "Pick the curves, then drag the handles or type distances and angles. Turn on copy to keep the \
       originals. Constraints that fight the move may need removing first.",
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
      "Pick the feature, then choose the tolerance type, its value and the datums it refers to. It \
       follows the drawing's standard.",
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
      "Pick the joint and set the weld type, size and side. Use it to tell the fabricator how parts are \
       to be joined.",
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
       stays steady. Usually the first 3D operation, followed by finishing passes.",
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
      "Pick the area and a tool; the tool runs along the inside corners left by earlier passes. Use it \
       as the last finishing step.",
    ),

    // MANUFACTURE DRILLING
    (_, "Drill") => (
      "Drills holes straight down from the top.",
      "Pick holes, or leave the selection empty to drill every hole; choose the cycle, such as peck \
       drilling or tapping, and the depth. Pick a drill matching the hole size from the tool library.",
    ),

    // MANUFACTURE ACTIONS
    (_, "Generate") => (
      "Computes the setup's toolpaths and machining time.",
      "Run it after changing operations or the design; operations show a warning until they're \
       regenerated. Check the result with Simulate before posting.",
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

    // SOLID CREATE
    (_, "Create Sketch") => (
      "Starts a new sketch on a plane or planar face.",
      "Pick an origin plane, a construction plane or a flat face of the model. Draw closed profiles in \
       the sketch, then finish it to use them in features.",
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
      "Adds depth to a closed sketch profile or a planar face.",
      "Pick profiles or faces, then drag the arrow or type the distance. Choose to join, cut, intersect \
       or make a new body, and set taper, two sides or extents up to a face. On the Surface tab it \
       makes surfaces from open or closed curves.",
    ),
    (_, "Revolve") => (
      "Spins a sketch profile around an axis.",
      "Pick the profile, then the axis: a sketch line, an edge or a construction axis. Set a full turn \
       or an angle. Good for shafts, knobs and turned parts.",
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
      "Drills holes, with counterbores, countersinks or threads.",
      "Pick a face and click, or pick sketch points to place several. Set the diameter, depth, tip \
       angle and head type. Turn on thread for tapped holes.",
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
      "Copies bodies, faces or features in rows and columns.",
      "Pick what to copy, then one or two directions, the number of copies and their spacing. Turn off \
       copies you don't need by clicking them.",
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
      "Pushes or pulls faces along their normals.",
      "Pick a face and drag; flat faces offset, edges become fillets and profiles extrude. A quick way \
       to adjust a model without opening its sketches.",
    ),
    (_, "Fillet") => (
      "Rounds the selected edges.",
      "Pick edges or faces and drag or type the radius. Add more sets with different radii in one go. \
       Fillets on edges that meet at a corner are blended together.",
    ),
    (_, "Chamfer") => (
      "Bevels the selected edges.",
      "Pick edges and set an equal distance, two distances or a distance and an angle. Common on \
       outside edges for comfort and assembly.",
    ),
    (_, "Shell") => (
      "Hollows a body to a wall thickness, removing the faces you pick.",
      "Pick the faces to open up and set the thickness inside or outside. Pick no faces to make a \
       closed hollow body.",
    ),
    (_, "Draft") => (
      "Tapers faces about a pull direction.",
      "Pick the pull direction and the faces, then set the angle. Moulded and cast parts need draft to \
       release from the mould.",
    ),
    (_, "Scale") => (
      "Scales bodies about a point.",
      "Pick bodies and a point, then set a uniform factor or a different one per axis. Useful for \
       shrinkage allowances or resizing imported parts.",
    ),
    (_, "Combine") => (
      "Joins, cuts or intersects bodies.",
      "Pick the target body, then the tool bodies, and choose join, cut or intersect. Turn on keep \
       tools to leave the tool bodies in place.",
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
      "Edits the named parameters that drive the design.",
      "Change the values of dimensions and add user parameters with expressions such as width / 2. \
       Everything that uses them updates.",
    ),
    (_, "Compute All") => (
      "Recomputes the whole timeline.",
      "Every feature is rebuilt in order. Use it after changing linked data or when something looks out \
       of date.",
    ),
    (_, "Bill of Materials") => (
      "Lists the components with their quantities, materials and masses.",
      "The list updates with the design. Set each component's material first for the masses to be \
       right.",
    ),

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
      "Sets up variants of the design, such as sizes, in one table.",
      "Each row is a configuration with its own parameter values, suppressed features and materials. \
       Switch between them to see each variant.",
    ),
    (_, "Display Configuration Table") => (
      "Shows the table of the design's configurations.",
      "Edit the values for each configuration in the table, or add rows and columns. Click a row to \
       switch the design to that configuration.",
    ),
    (_, "Create Configuration Rules") => (
      "Sets rules that work out configuration values from others.",
      "Write a condition and the values it sets, such as a larger fillet when the height passes a \
       limit. The rules apply whenever a configuration is chosen.",
    ),

    // ASSEMBLE
    (_, "Add To Assembly") => (
      "Places this design in an assembly as a component.",
      "Pick the assembly; the design goes into it as a linked component that updates when you change it \
       here. Joint it into place in the assembly once it's there.",
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
      "Pick the cam faces and the follower. As the cam turns, the follower moves to stay in contact.",
    ),

    // CONSTRUCT
    (_, "User Coordinate System (UCS)") => (
      "Places a local origin and axes for measuring and positioning.",
      "Pick an origin and directions for the axes. Use it as a reference for coordinates, sketches and \
       moves that don't line up with the main origin.",
    ),
    (_, "Offset Plane") => (
      "Creates a plane parallel to a plane or face at a distance.",
      "Pick a plane or planar face and drag or type the distance. Use it to sketch at a height above \
       the model.",
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
      "Creates a plane square to an edge or path.",
      "Pick an edge or sketch curve; the plane is placed across it. Use it to sketch a profile for a \
       sweep.",
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
      "Pick a face at the point the axis should pass through. Use it as a pivot for a revolve or a \
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
      "Measures distances, angles, areas and lengths of the selection.",
      "Pick one item for its size or two for the distance and angle between them. Click a value to copy \
       it.",
    ),
    (_, "Interference") => (
      "Finds where bodies of different components overlap.",
      "Pick the components to check, or check them all. The overlaps are shown with their volumes so \
       you can fix clashes before making parts.",
    ),
    (_, "Section Analysis") => (
      "Cuts the view open along a plane to show the inside.",
      "Pick a plane or face and drag to move the cut. The model isn't changed; only the view is cut.",
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
    (_, "Canvas") => (
      "Places a reference image on a plane to sketch over.",
      "Pick an image and a plane, then size it with a known distance. Use it to trace products from \
       photos.",
    ),
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
      "Inserts an STL, OBJ or 3MF file as a mesh body.",
      "Pick the file, then set its units and position. Use the mesh tools to clean it up or convert it \
       to a solid.",
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
      "Brings bodies, sketches or parameters in from another design, linked.",
      "Pick the design and the items to bring in; they update when the source changes. Use it to share \
       a skeleton or parameters between related designs.",
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
      "Selects every body, or every curve in a sketch.",
      "Selects everything that the current filters allow. In a sketch it selects every curve. Combine \
       it with Invert Selection to pick everything except a few items.",
    ),
    (_, "Select Other") => (
      "Picks among the items under the last click, front to back.",
      "When the wrong item was selected, use this to step through everything under the cursor. Holding \
       the mouse button or Alt+click does the same.",
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
      "Creates a surface filling a closed boundary of edges.",
      "Pick the edges that form a closed loop; a surface spans them. Set how it meets the neighbouring \
       faces, such as tangent, to close holes smoothly.",
    ),
    (_, "Patch from Sketch") => (
      "Creates a flat surface filling a closed sketch profile.",
      "Pick closed profiles; each becomes a surface body. Use it for caps and flat panels.",
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
    (_, "Variable Offset") => (
      "Offsets surface bodies, with chosen faces at distances of their own.",
      "Pick the body and the base distance, then add faces with their own distances. Neighbouring faces \
       adjust to keep it joined.",
    ),

    // SURFACE MODIFY
    (_, "Trim") => (
      "Trims surface bodies where they cross a plane or another surface.",
      "Pick the cutting tool, then click the pieces to remove. Use it to cut surfaces back to where \
       they meet.",
    ),
    (_, "Trim with Body") => (
      "Trims surface bodies with a solid or another surface.",
      "Pick the surfaces and the body to cut with, then the pieces to keep or remove. Use it when the \
       cutting shape is already modelled as a body.",
    ),
    (_, "Trim with Sketch") => (
      "Trims surface bodies with sketch curves projected onto them.",
      "Pick the surfaces and the curves; the curves are projected onto the surfaces to cut them. Use it \
       for openings and outlines on curved surfaces.",
    ),
    (_, "Untrim") => (
      "Removes holes from faces or restores their original boundary.",
      "Pick a face and the loops to remove, or untrim the face completely back to its full shape. Use \
       it to start a face's outline again, or to close holes in a surface.",
    ),
    (_, "Extend") => (
      "Extends open surface edges by a distance.",
      "Pick the edges, set the distance and choose whether the extension continues the curvature or \
       goes straight. Use it so surfaces overlap before trimming.",
    ),
    (_, "Stitch") => (
      "Sews surface bodies together along their edges.",
      "Pick the surfaces and set the gap tolerance. When the surfaces close up completely, the result \
       becomes a solid.",
    ),
    (_, "Unstitch") => (
      "Breaks bodies apart into one surface per face.",
      "Pick the bodies or faces. Use it to replace or rebuild individual faces, then stitch them back \
       together.",
    ),
    (_, "Reverse Normal") => (
      "Flips which way faces point.",
      "Pick surface or mesh bodies; their inside and outside swap. Fix it when thickening or printing \
       goes the wrong way.",
    ),

    // MESH
    (_, "Tessellate") => (
      "Converts solid or surface bodies into mesh bodies.",
      "Pick the bodies and the refinement. Use it to edit a body with mesh tools or export it as \
       triangles.",
    ),
    (_, "Create Mesh Section Sketch") => (
      "Sketches the cross-section of a mesh along a plane.",
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
    (_, "Remesh") => (
      "Rebuilds a mesh with evenly sized triangles.",
      "Pick the mesh and the density. An even mesh smooths and converts more reliably.",
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
      "Pick the face that stays fixed. The flat pattern shows bend lines and its size, and can be \
       exported as a DXF for cutting.",
    ),
    (_, "Flat Pattern Sketch") => (
      "Makes an editable sketch of the flat pattern.",
      "The outline and bend lines become sketch curves. Use them to dimension or export the blank.",
    ),

    // SHEET METAL MODIFY
    (_, "Unfold") => (
      "Flattens bends of a sheet metal body onto a face.",
      "Pick the fixed face and the bends to unfold. Add cuts while flat, then use Refold Faces to bend \
       it back.",
    ),
    (_, "Refold Faces") => (
      "Folds an unfolded sheet back up, keeping cuts made while flat.",
      "Pick the unfolded bends to refold. Cuts made across them follow the bends.",
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
    _ => return None,
  };
  Some(Tip { line, more })
}
