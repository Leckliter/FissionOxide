//! The command set. Each command takes JSON parameters and returns JSON.

use std::path::PathBuf;

use clf_kernel::glam::DVec3;
use clf_model::{
  AxisRef, BendPosition, BodyId, BodySel, CornerMode, Design, EdgeRef, Extent, ExtentKind, FaceRef, FeatureId,
  FeatureKind, HoleKind, LoftSection, Operation, OriginPlane, ParamKind, PlaneRef, Primitive, ReliefShape,
  SheetMetalRule,
};
use clf_sketch::{ConstraintId, ConstraintKind, CurveId, CurveKind, Dim, PointId, PtSpec, Sketch};
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
  "open" => "Open a .clf design", "{path}";
  "save" => "Save the design (.clf JSON)", "{path?}";
  "export" => "Export bodies: .step/.stp, .stl, .obj (format from the extension)", "{path, bodies?: [id]}";
  "import" => "Import solids from a STEP file as a new feature", "{path}";
  "undo" => "Undo", "{}";
  "redo" => "Redo", "{}";
  "param.add" => "Add a user parameter", "{name, expr, kind?: length|angle|unitless, comment?}";
  "param.set" => "Change a parameter's expression", "{name, expr}";
  "param.rename" => "Rename a parameter (rewrites expressions using it)", "{name, to}";
  "param.remove" => "Delete an unused parameter", "{name}";
  "sketch.create" => "New sketch on a plane and start editing it", "{plane: \"xy\"|\"xz\"|\"yz\"|{feature}|{body, face}|{point: [x,y,z], normal?}}";
  "sketch.edit" => "Edit an existing sketch", "{sketch}";
  "sketch.finish" => "Stop editing the sketch", "{}";
  "sketch.line" => "Line or polyline (consecutive points share endpoints)", "{points: [[x,y],...], closed?: bool, sketch?}";
  "sketch.rect" => "Two-point rectangle, or centre rectangle with {center, corner}", "{corner1, corner2} | {center, corner}";
  "sketch.circle" => "Circle", "{center: [x,y], radius | diameter}";
  "sketch.arc" => "Arc through three points or around a centre", "{start, mid, end} | {center, start, end}";
  "sketch.polygon" => "Regular polygon inscribed in a circle", "{center, vertex, sides}";
  "sketch.slot" => "Centre-to-centre slot", "{start, end, width}";
  "sketch.point" => "Sketch point", "{at: [x,y]}";
  "sketch.constrain" => "Add a geometric constraint", "{type: coincident|horizontal|vertical|parallel|perpendicular|collinear|equal|tangent|concentric|midpoint|point_on_curve|symmetric|fix, points?: [id], curves?: [id]}";
  "sketch.dimension" => "Add a driving dimension", "{type: distance|horizontal|vertical|radius|diameter|angle|point_line, points?, curves?, value: expr}";
  "sketch.set_dimension" => "Change a dimension's value (expression)", "{constraint, value}";
  "sketch.delete" => "Delete sketch geometry or constraints", "{curves?, points?, constraints?}";
  "sketch.construction" => "Toggle construction geometry", "{curves: [id], on?: bool}";
  "sketch.drag" => "Drag a sketch point (solver keeps constraints)", "{point, to: [x,y]}";
  "sketch.info" => "Sketch geometry, constraints, profiles and degrees of freedom", "{sketch?}";
  "extrude" => "Extrude sketch profiles", "{sketch?, profiles?: [[x,y]] (points inside regions) | indices, distance, extent?: one_side|symmetric|two_sides|through_all, distance2?, flip?, operation?: new_body|join|cut|intersect}";
  "revolve" => "Revolve sketch profiles around an axis", "{sketch?, profiles?, axis: \"x\"|\"y\"|\"z\"|{line: curve}|{body, edge}, angle?, operation?}";
  "loft" => "Loft through profiles of several sketches, in order", "{sections: [sketch id | {sketch, profile?: [x,y]}], ruled?, closed?, operation?}";
  "sweep" => "Sweep sketch profiles along a path (open lines/arcs of another sketch)", "{sketch?, profiles?, path: sketch id, path_curves?: [id], operation?}";
  "hole" => "Holes at sketch points (into the face the sketch is on)", "{sketch?, points?: [id], diameter, depth? | through_all?, kind?: simple|counterbore|countersink, head_diameter?, head_depth?, head_angle?, tip_angle?, flip?}";
  "split" => "Split bodies along a plane", "{plane, bodies?}";
  "draft" => "Taper faces about a neutral plane", "{faces: [{body, face} | {point}], plane, angle, flip?}";
  "fillet" => "Round edges (an edge is {body, edge} or {point: [x,y,z]} on it)", "{edges: [{body, edge} | {point}], radius}";
  "chamfer" => "Bevel edges", "{edges: [{body, edge}], distance}";
  "shell" => "Hollow bodies, opening the given faces (a face is {body, face} or {point, normal?})", "{faces: [{body, face} | {point}], thickness}";
  "box" => "Box primitive (base centred on a plane point)", "{plane?, at?, width, depth, height, operation?}";
  "cylinder" => "Cylinder primitive", "{plane?, at?, radius, height, operation?}";
  "sphere" => "Sphere primitive", "{plane?, at?, radius, operation?}";
  "torus" => "Torus primitive", "{plane?, at?, major, minor, operation?}";
  "plane.offset" => "Construction plane offset from a plane or planar face", "{plane, offset}";
  "mirror" => "Mirror bodies across a plane", "{plane, bodies?, join?}";
  "pattern.rectangular" => "Rectangular pattern of bodies", "{direction: [x,y,z], count, spacing, direction2?, count2?, spacing2?, bodies?, join?}";
  "pattern.circular" => "Circular pattern of bodies", "{axis, count, angle?, bodies?, join?}";
  "move" => "Move/rotate bodies", "{bodies?, translate?: [x,y,z], axis?, angle?}";
  "combine" => "Join/cut/intersect bodies", "{target, tools: [id], operation, keep_tools?}";
  "body.remove" => "Remove bodies (as a timeline feature)", "{bodies: [id]}";
  "body.rename" => "Rename a body", "{body, name}";
  "body.visible" => "Show or hide a body", "{body, visible}";
  "feature.edit" => "Change feature properties (fields merged into the feature)", "{feature, set: {...}}";
  "feature.delete" => "Delete a feature and the features that depend on it", "{feature}";
  "feature.suppress" => "Suppress or unsuppress a feature", "{feature, suppressed?: bool}";
  "feature.rename" => "Rename a feature", "{feature, name}";
  "feature.move" => "Reorder a feature in the timeline", "{feature, to: index}";
  "timeline.marker" => "Roll the timeline back to after N features (null = end)", "{at: number|null}";
  "select" => "Set the selection", "{items: [Sel]}";
  "sheet.rule" => "Add or change a sheet metal rule (the first rule is the default)", "{name?, thickness?, bend_radius?, k_factor?, relief?: rectangular|round|none, relief_width?, relief_depth?, gap?}";
  "sheet.rules" => "List the sheet metal rules", "{}";
  "sheet.rule_remove" => "Delete an unused sheet metal rule", "{name}";
  "sheet.flange" => "Sheet metal flange from closed sketch profiles (a flat sheet of the rule's thickness)", "{sketch?, profiles?, rule?, flip?, operation?: new_body|join}";
  "sheet.contour_flange" => "Sheet metal from an open sketch path: corners become bends, extruded by distance", "{sketch?, curves?: [id], distance, symmetric?, flip?, flip_direction?, rule?}";
  "sheet.edge_flange" => "Bend a flange up from straight sheet edges", "{edges: [{body, edge} | {point}], height, angle?, flip?, position?: adjacent|inside|outside, offset_start?, offset_end?, corners?: miter|open}";
  "sheet.hem" => "Fold sheet edges back 180° (flat hem)", "{edges: [{body, edge} | {point}], length, gap?, flip?}";
  "sheet.bend" => "Bend a flat sheet along a sketch line on one of its faces", "{sketch?, line: curve id, angle, flip?, flip_side?}";
  "sheet.convert" => "Turn a thin solid into sheet metal (thickness measured from the body); sharp corners become bends", "{body, face?: {body, face} | {point}, rule?, bends?}";
  "sheet.rip" => "Slit a solid along edges (e.g. a shelled box's corners) so it can unfold", "{edges: [{body, edge} | {point}], gap?}";
  "sheet.refold" => "Fold an unfolded sheet back up, carrying cuts made while flat", "{body?}";
  "sheet.unfold" => "Flatten a sheet metal body onto its stationary face (timeline feature)", "{body?, face?, keep_folded?}";
  "sheet.flat_pattern" => "Flat pattern of a sheet metal body: size and bends; writes .dxf/.step/.stl when given a path", "{body?, face?, path?}";
}

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  match name {
    "help" => {
      Ok(json!(COMMANDS.iter().map(|c| json!({"id": c.id, "help": c.help, "params": c.params})).collect::<Vec<_>>()))
    }
    "info" => Ok(info(s)),
    "new" => {
      *s = Session::new(Design::new(opt_str(p, "name").unwrap_or("Untitled")));
      Ok(json!({}))
    }
    "open" => {
      let path = PathBuf::from(req_str(p, "path")?);
      *s = Session::open(&path)?;
      Ok(json!({"features": s.design.features.len()}))
    }
    "save" => {
      let path = match opt_str(p, "path") {
        Some(p) => PathBuf::from(p),
        None => s.path.clone().ok_or(CmdError::new("no path: pass {\"path\": ...}"))?,
      };
      s.save_to(&path)?;
      Ok(json!({"path": path}))
    }
    "export" => export(s, p),
    "import" => {
      let path = PathBuf::from(req_str(p, "path")?);
      let shapes = clf_kernel::read_step(&path)?;
      let breps: Result<Vec<String>, _> = shapes.iter().map(|sh| sh.to_brep()).collect();
      let breps = breps?;
      let file_name = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
      let n = breps.len();
      let id = s.edit(|d| Ok(d.add_feature(FeatureKind::Import { file_name, breps })?))?;
      Ok(json!({"feature": id.0, "solids": n}))
    }
    "undo" => Ok(json!({"done": s.undo()})),
    "redo" => Ok(json!({"done": s.redo()})),
    "param.add" => {
      let (name, expr) = (req_str(p, "name")?.to_string(), req_expr(p, "expr")?);
      let kind = param_kind(opt_str(p, "kind"))?;
      let comment = opt_str(p, "comment").unwrap_or("").to_string();
      clf_expr::eval(&expr, kind, &params_scope(s))?;
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
      let id = s.edit(|d| Ok(d.add_feature(FeatureKind::Sketch { plane, sketch: Sketch::new() })?))?;
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
    "sketch.line" | "sketch.rect" | "sketch.circle" | "sketch.arc" | "sketch.polygon" | "sketch.slot"
    | "sketch.point" => draw(s, name, p),
    "sketch.constrain" => constrain(s, p),
    "sketch.dimension" => dimension(s, p),
    "sketch.set_dimension" => {
      let sk = sketch_id(s, p)?;
      let c = ConstraintId(req_u32(p, "constraint")?);
      let expr = req_expr(p, "value")?;
      let angle = s.design.sketch(sk)?.constraint(c).ok_or(CmdError::new("no such constraint"))?.kind.is_angle();
      let kind = if angle { clf_expr::Kind::Angle } else { clf_expr::Kind::Length };
      let v = clf_expr::eval(&expr, kind, &params_scope(s))?;
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
    "extrude" => {
      let sk = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sk, p)?;
      let kind = match opt_str(p, "extent").unwrap_or("one_side") {
        "one_side" | "distance" => ExtentKind::OneSide,
        "symmetric" => ExtentKind::Symmetric,
        "two_sides" => ExtentKind::TwoSides,
        "through_all" | "all" => ExtentKind::ThroughAll,
        other => return Err(CmdError::new(format!("unknown extent '{other}'"))),
      };
      let distance = if kind == ExtentKind::ThroughAll { "0".to_string() } else { req_expr(p, "distance")? };
      let extent = Extent {
        kind,
        distance,
        distance2: opt_expr(p, "distance2").unwrap_or_else(|| "0".into()),
        flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
      };
      let operation = operation(p)?;
      add_feature(s, FeatureKind::Extrude { sketch: sk, profiles, extent, operation })
    }
    "revolve" => {
      let sk = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sk, p)?;
      let axis = axis_ref(s, p.get("axis").ok_or(CmdError::new("missing 'axis'"))?)?;
      let angle = opt_expr(p, "angle").unwrap_or_else(|| "360 deg".into());
      add_feature(s, FeatureKind::Revolve { sketch: sk, profiles, axis, angle, operation: operation(p)? })
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
      add_feature(
        s,
        FeatureKind::Loft { sections, ruled: flag("ruled"), closed: flag("closed"), operation: operation(p)? },
      )
    }
    "sweep" => {
      let path_sketch = FeatureId(req_u32(p, "path")?);
      s.design.sketch(path_sketch)?;
      let sk = sketch_for_feature(s, p)?;
      let profiles = profile_points(s, sk, p)?;
      let path_curves = ids(p, "path_curves").into_iter().map(CurveId).collect();
      add_feature(s, FeatureKind::Sweep { sketch: sk, profiles, path_sketch, path_curves, operation: operation(p)? })
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
      add_feature(s, FeatureKind::Draft { faces, plane, angle: req_expr(p, "angle")?, flip })
    }
    "fillet" => {
      let edges = edge_refs(s, p)?;
      add_feature(s, FeatureKind::Fillet { edges, radius: req_expr(p, "radius")? })
    }
    "chamfer" => {
      let edges = edge_refs(s, p)?;
      add_feature(s, FeatureKind::Chamfer { edges, distance: req_expr(p, "distance")? })
    }
    "shell" => {
      let mut faces = Vec::new();
      for f in p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))? {
        faces.push(face_ref(s, f)?);
      }
      add_feature(s, FeatureKind::Shell { faces, thickness: req_expr(p, "thickness")? })
    }
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
      add_feature(s, FeatureKind::Primitive { shape, plane, at, operation: operation(p)? })
    }
    "plane.offset" => {
      let base = plane_ref(s, p.get("plane").ok_or(CmdError::new("missing 'plane'"))?)?;
      add_feature(s, FeatureKind::ConstructionPlane { base, offset: req_expr(p, "offset")? })
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
      let removed = s.edit(|d| Ok(d.remove_feature(id)?))?;
      if s.edit_sketch.is_some_and(|e| removed.contains(&e)) {
        s.edit_sketch = None;
      }
      Ok(json!({"removed": removed.iter().map(|f| f.0).collect::<Vec<_>>()}))
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
      s.edit(|d| Ok(d.move_feature(id, to)?))?;
      Ok(json!({}))
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
      let info = clf_model::sheet::eval_rule(&rule, &params_scope(s)).map_err(CmdError::new)?;
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
            let v = clf_model::sheet::eval_rule(r, &scope);
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
      add_feature(s, FeatureKind::Flange { sketch: sk, profiles, rule, flip, operation })
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
          corners: match opt_str(p, "corners").unwrap_or("miter") {
            "miter" => CornerMode::Miter,
            "open" => CornerMode::Open,
            o => return Err(CmdError::new(format!("unknown corner mode '{o}' (miter, open)"))),
          },
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
    _ => Err(CmdError::new(format!("unknown command '{name}' (try 'help')"))),
  }
}

// ---------------------------------------------------------------- parameters helpers

fn req_str<'a>(p: &'a Value, k: &str) -> Result<&'a str, CmdError> {
  p.get(k).and_then(Value::as_str).ok_or_else(|| CmdError::new(format!("missing '{k}'")))
}

fn opt_str<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
  p.get(k).and_then(Value::as_str)
}

fn req_u32(p: &Value, k: &str) -> Result<u32, CmdError> {
  p.get(k)
    .and_then(Value::as_u64)
    .and_then(|v| u32::try_from(v).ok())
    .ok_or_else(|| CmdError::new(format!("missing or invalid '{k}'")))
}

/// An expression: a JSON number or a string ("40 mm", "width/2").
fn expr_of(v: &Value) -> Result<String, CmdError> {
  match v {
    Value::String(s) if !s.trim().is_empty() => Ok(s.clone()),
    Value::Number(n) => {
      n.as_f64().filter(|f| f.is_finite()).map(clf_expr::trim_number).ok_or(CmdError::new("bad number"))
    }
    _ => Err(CmdError::new("expected a number or an expression string")),
  }
}

fn req_expr(p: &Value, k: &str) -> Result<String, CmdError> {
  expr_of(p.get(k).ok_or_else(|| CmdError::new(format!("missing '{k}'")))?)
}

fn opt_expr(p: &Value, k: &str) -> Option<String> {
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

fn req_vec3(p: &Value, k: &str) -> Result<[f64; 3], CmdError> {
  let a = p.get(k).and_then(Value::as_array).ok_or_else(|| CmdError::new(format!("'{k}' must be [x, y, z]")))?;
  match a.as_slice() {
    [x, y, z] => match (x.as_f64(), y.as_f64(), z.as_f64()) {
      (Some(x), Some(y), Some(z)) if x.is_finite() && y.is_finite() && z.is_finite() => Ok([x, y, z]),
      _ => Err(CmdError::new(format!("'{k}' must be numbers"))),
    },
    _ => Err(CmdError::new(format!("'{k}' must be [x, y, z]"))),
  }
}

fn ids(p: &Value, k: &str) -> Vec<u32> {
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

fn operation(p: &Value) -> Result<Operation, CmdError> {
  Ok(match opt_str(p, "operation").unwrap_or("new_body") {
    "new_body" | "new" => Operation::NewBody,
    "join" | "union" => Operation::Join,
    "cut" | "subtract" => Operation::Cut,
    "intersect" => Operation::Intersect,
    o => return Err(CmdError::new(format!("unknown operation '{o}'"))),
  })
}

fn body_sel(p: &Value) -> BodySel {
  let v = ids(p, "bodies");
  if v.is_empty() { BodySel::All } else { BodySel::Bodies { ids: v.into_iter().map(BodyId).collect() } }
}

fn add_feature(s: &mut Session, kind: FeatureKind) -> CmdResult {
  let id = s.edit(|d| Ok(d.add_feature(kind)?))?;
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
  })
}

/// `rule`: a rule name that must exist (empty / absent = the default rule).
fn rule_name(s: &Session, p: &Value) -> Result<String, CmdError> {
  let name = opt_str(p, "rule").unwrap_or("").to_string();
  if s.design.sheet_rule(&name).is_none() {
    return Err(CmdError::new(format!("there is no sheet metal rule '{name}'")));
  }
  Ok(name)
}

/// `body`, or the only sheet metal body.
fn sheet_body(s: &Session, p: &Value) -> Result<BodyId, CmdError> {
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

fn flat_pattern(s: &Session, p: &Value) -> CmdResult {
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
      "dxf" => std::fs::write(&path, clf_model::sheet::flat_dxf(&flat).map_err(CmdError::new)?)?,
      "step" | "stp" => clf_kernel::write_step([&flat.shape], &path)?,
      "stl" => clf_kernel::write_stl(&[flat.shape.tessellate(0.02)?], &path)?,
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
fn plane_ref(s: &Session, v: &Value) -> Result<PlaneRef, CmdError> {
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
      Some(FeatureKind::ConstructionPlane { .. }) => Ok(PlaneRef::Construction { feature: id }),
      _ => Err(CmdError::new("that feature is not a construction plane")),
    };
  }
  let face = face_ref(s, v)?;
  if !face.planar {
    return Err(CmdError::new("that face is not planar"));
  }
  Ok(PlaneRef::Face { face })
}

/// {"body", "face"} by index, or {"point": [x,y,z], "normal"?: [x,y,z]} — the face at a point.
fn face_ref(s: &Session, v: &Value) -> Result<FaceRef, CmdError> {
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
      let d = if f.kind == clf_kernel::SurfaceKind::Plane {
        let off = (p - f.origin).dot(f.normal).abs();
        let q = p - f.normal * (p - f.origin).dot(f.normal);
        if off > 1e-3 || !clf_model::refs::point_in_face(mesh, f.id, q) {
          continue;
        }
        off
      } else {
        // curved faces: distance to the nearest tessellation vertex of the face
        nearest_vertex_dist(mesh, f.id, p)
      };
      if d < 0.05 && best.is_none_or(|(_, _, bd)| d < bd) {
        best = Some((b.id, f.id, d));
      }
    }
  }
  best.map(|(b, f, _)| (b, f)).ok_or_else(|| CmdError::new(format!("no face at {:?}", p.to_array())))
}

fn nearest_vertex_dist(m: &clf_kernel::TriMesh, face: u32, p: DVec3) -> f64 {
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

fn edge_refs(s: &Session, p: &Value) -> Result<Vec<EdgeRef>, CmdError> {
  let list = p.get("edges").and_then(Value::as_array).ok_or(CmdError::new("missing 'edges'"))?;
  let mut out = Vec::new();
  for e in list {
    let (body, edge) = if e.get("point").is_some() {
      edge_at(s, DVec3::from(req_vec3(e, "point")?))?
    } else {
      (BodyId(req_u32(e, "body")?), req_u32(e, "edge")?)
    };
    let b = s.model.body(body).ok_or(CmdError::new(format!("no body {}", body.0)))?;
    let info = b.edges().iter().find(|x| x.id == edge).ok_or(CmdError::new(format!("body has no edge {edge}")))?;
    out.push(EdgeRef::from_edge(body, info, b.faces()));
  }
  if out.is_empty() {
    return Err(CmdError::new("no edges given"));
  }
  Ok(out)
}

fn axis_ref(s: &Session, v: &Value) -> Result<AxisRef, CmdError> {
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
fn sketch_for_feature(s: &mut Session, p: &Value) -> Result<FeatureId, CmdError> {
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
fn profile_points(s: &Session, sketch: FeatureId, p: &Value) -> Result<Vec<[f64; 2]>, CmdError> {
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
      "sketch.circle" => {
        let c = req_pt(&p, "center")?;
        // a number draws the circle; an expression ("hole", "d/2") also adds a driving dimension
        let (key, raw) = match (p.get("radius"), p.get("diameter")) {
          (Some(v), _) => ("radius", v.clone()),
          (None, Some(v)) => ("diameter", v.clone()),
          _ => return Err(CmdError::new("missing 'radius' or 'diameter'")),
        };
        let (value, dim) = match &raw {
          Value::String(e) => (clf_expr::eval(e, clf_expr::Kind::Length, &scope)?, Some(e.clone())),
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
      "sketch.arc" => {
        if let Some(c) = opt_pt(&p, "center")? {
          let (st, en) = (req_pt(&p, "start")?, req_pt(&p, "end")?);
          let (cs, ss) = (snap(sketch, c), snap(sketch, st));
          sketch.add_arc_center(cs, ss, en)?;
        } else {
          let (st, mid, en) = (req_pt(&p, "start")?, req_pt(&p, "mid")?, req_pt(&p, "end")?);
          let (ss, es) = (snap(sketch, st), snap(sketch, en));
          sketch.add_arc_3pt(ss, mid, es)?;
        }
      }
      "sketch.polygon" => {
        let sides = p.get("sides").and_then(Value::as_u64).unwrap_or(6) as usize;
        let c = req_pt(&p, "center")?;
        let cs = snap(sketch, c);
        sketch.add_polygon(cs, req_pt(&p, "vertex")?, sides)?;
      }
      "sketch.slot" => {
        let w = p.get("width").and_then(Value::as_f64).ok_or(CmdError::new("missing 'width'"))?;
        sketch.add_slot(req_pt(&p, "start")?, req_pt(&p, "end")?, w)?;
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
  let id = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    let need_p = |n: usize| -> Result<(), CmdError> {
      if pts.len() >= n { Ok(()) } else { Err(CmdError::new(format!("'{t}' needs {n} point(s)"))) }
    };
    let need_c = |n: usize| -> Result<(), CmdError> {
      if cs.len() >= n { Ok(()) } else { Err(CmdError::new(format!("'{t}' needs {n} curve(s)"))) }
    };
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
      "symmetric" => {
        need_p(2)?;
        need_c(1)?;
        ConstraintKind::Symmetric { a: pts[0], b: pts[1], line: cs[0] }
      }
      o => return Err(CmdError::new(format!("unknown constraint '{o}'"))),
    };
    let id = sketch.add_constraint(kind)?;
    let r = sketch.solve();
    if !r.ok() {
      return Err(CmdError::new("that constraint conflicts with the others (or over-constrains the sketch)"));
    }
    Ok(id)
  })?;
  Ok(json!({"constraint": id.0, "dof": s.model.sketches.get(&sk).map(|r| r.report.dof)}))
}

fn dimension(s: &mut Session, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let t = req_str(p, "type")?.to_string();
  let expr = req_expr(p, "value")?;
  let pts: Vec<PointId> = ids(p, "points").into_iter().map(PointId).collect();
  let cs: Vec<CurveId> = ids(p, "curves").into_iter().map(CurveId).collect();
  let kind = if t == "angle" { clf_expr::Kind::Angle } else { clf_expr::Kind::Length };
  let value = clf_expr::eval(&expr, kind, &params_scope(s))?;
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
  Ok(json!({
    "sketch": sk.0,
    "points": sketch.points.iter().map(|p| json!({"id": p.id.0, "at": [p.x, p.y], "fixed": p.fixed})).collect::<Vec<_>>(),
    "curves": sketch.curves,
    "constraints": sketch.constraints,
    "profiles": r.map(|r| r.profiles.iter().enumerate().map(|(i, pr)| json!({
      "index": i, "area": pr.area(), "inside": pr.interior_point(), "holes": pr.holes.len()
    })).collect::<Vec<_>>()),
    "dof": r.map(|r| r.report.dof),
    "solved": r.map(|r| r.report.ok()),
    "free_points": r.map(|r| r.report.free_points.iter().map(|p| p.0).collect::<Vec<_>>()),
  }))
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
  })
}

fn export(s: &Session, p: &Value) -> CmdResult {
  let path = PathBuf::from(req_str(p, "path")?);
  let wanted: Vec<u32> = ids(p, "bodies");
  let bodies: Vec<&clf_model::regen::Body> = s
    .model
    .bodies
    .iter()
    .filter(|b| if wanted.is_empty() { s.body_visible(b.id) } else { wanted.contains(&b.id.0) })
    .collect();
  if bodies.is_empty() {
    return Err(CmdError::new("there are no bodies to export"));
  }
  let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
  match ext.as_str() {
    "step" | "stp" => clf_kernel::write_step(bodies.iter().map(|b| &b.shape), &path)?,
    "stl" => {
      let meshes: Vec<_> = bodies.iter().filter_map(|b| b.shape.tessellate(fine_tol(b)).ok()).collect();
      clf_kernel::write_stl(&meshes, &path)?
    }
    "obj" => {
      let meshes: Vec<_> =
        bodies.iter().filter_map(|b| b.shape.tessellate(fine_tol(b)).ok().map(|m| (s.body_name(b.id), m))).collect();
      clf_kernel::write_obj(&meshes, &path)?
    }
    "dxf" => {
      let [b] = bodies.as_slice() else {
        return Err(CmdError::new("DXF exports one sheet metal body's flat pattern; pass 'bodies': [id]"));
      };
      let flat = b.flat_pattern(None).map_err(CmdError::new)?;
      std::fs::write(&path, clf_model::sheet::flat_dxf(&flat).map_err(CmdError::new)?)?
    }
    o => return Err(CmdError::new(format!("unknown export format '.{o}' (step, stl, obj, dxf)"))),
  }
  Ok(json!({"path": path, "bodies": bodies.len()}))
}

/// Export tessellation: finer than display.
fn fine_tol(b: &clf_model::regen::Body) -> f64 {
  (b.diagonal() * 0.0005).clamp(0.002, 0.1)
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
