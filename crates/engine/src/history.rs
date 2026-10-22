//! The undo history by name, for the Undo and Redo drop-downs on the quick-access toolbar. As in
//! Fusion, a step is named by the command that made it ("Line", "Sketch Dimension", "Fix",
//! "Change Units", "Extrude"); commands without a name of their own are named from what they
//! changed in the design ("Delete Fillet2", "Suppress Extrude1"). Several steps can be undone or
//! redone at once.

use fox_model::Design;
use serde_json::Value;

use crate::Session;

impl Session {
  /// The steps Undo goes back through, newest first, by name.
  pub fn undo_history(&self) -> Vec<String> {
    let start = self.txn.unwrap_or(0).min(self.undo.len());
    let stack = &self.undo[start..];
    (0..stack.len())
      .rev()
      .map(|i| {
        let after = stack.get(i + 1).map_or(&self.design, |s| &s.design);
        stack[i].name.clone().unwrap_or_else(|| step_label(&stack[i].design, after))
      })
      .collect()
  }

  /// The steps Redo goes forward through, next first, by name.
  pub fn redo_history(&self) -> Vec<String> {
    let mut before = &self.design;
    let mut out = Vec::with_capacity(self.redo.len());
    for step in self.redo.iter().rev() {
      out.push(step.name.clone().unwrap_or_else(|| step_label(before, &step.design)));
      before = &step.design;
    }
    out
  }

  /// Undoes `n` steps (regenerating once). Returns how many were undone.
  pub fn undo_steps(&mut self, n: usize) -> usize {
    let mut done = 0;
    let mut reopen = None;
    while done < n && self.can_undo() {
      let Some(step) = self.undo.pop() else { break };
      reopen = step.sketch;
      self.redo.push(step.swap(&mut self.design));
      done += 1;
    }
    if done > 0 {
      self.after_history();
      self.reopen_sketch(reopen, true);
    }
    done
  }

  /// Redoes `n` steps (regenerating once). Returns how many were redone.
  pub fn redo_steps(&mut self, n: usize) -> usize {
    let mut done = 0;
    let mut finished = None;
    while done < n {
      let Some(step) = self.redo.pop() else { break };
      finished = step.sketch;
      self.undo.push(step.swap(&mut self.design));
      done += 1;
    }
    if done > 0 {
      self.after_history();
      self.reopen_sketch(finished, false);
    }
    done
  }
}

/// The name Fusion gives the undo step a command makes, or `None` to name it from what changed.
pub fn command_label(name: &str, p: &Value) -> Option<String> {
  let has = |k: &str| p.get(k).is_some();
  let ty = p.get("type").and_then(Value::as_str).unwrap_or("");
  let label = match name {
    "design.units" if has("unit") => "Change Units",
    "design.type" if has("type") => "Convert Design Type",
    "sketch.create" => "Create Sketch",
    "sketch.line" => "Line",
    "sketch.rect" if has("center") => "Center Rectangle",
    "sketch.rect" if has("points") => "3-Point Rectangle",
    "sketch.rect" => "2-Point Rectangle",
    "sketch.circle" => match p.get("through").and_then(Value::as_array).map(Vec::len) {
      Some(3) => "3-Point Circle",
      Some(_) => "2-Point Circle",
      None => "Center Diameter Circle",
    },
    "sketch.arc" if has("tangent") => "Tangent Arc",
    "sketch.arc" if has("center") => "Center Point Arc",
    "sketch.arc" => "3-Point Arc",
    "sketch.polygon" if has("edge") => "Edge Polygon",
    "sketch.polygon" if p.get("circumscribed").and_then(Value::as_bool) == Some(true) => "Circumscribed Polygon",
    "sketch.polygon" => "Inscribed Polygon",
    "sketch.slot" if has("arc_center") || has("mid") => "Arc Slot",
    "sketch.slot" if has("center") || p.get("kind").and_then(Value::as_str) == Some("center_point") => {
      "Center Point Slot"
    }
    "sketch.slot" if p.get("kind").and_then(Value::as_str) == Some("overall") => "Overall Slot",
    "sketch.slot" => "Center to Center Slot",
    "sketch.point" => "Point",
    "sketch.constrain" => match ty {
      "coincident" | "point_on_curve" => "Coincident",
      "horizontal" | "vertical" => "Horizontal/Vertical",
      "parallel" => "Parallel",
      "perpendicular" => "Perpendicular",
      "collinear" => "Collinear",
      "equal" => "Equal",
      "tangent" => "Tangent",
      "concentric" => "Concentric",
      "midpoint" => "Midpoint",
      "symmetric" => "Symmetry",
      "fix" => "Fix",
      "curvature" => "Curvature",
      "polygon" => "Polygon",
      _ => "Constraint",
    },
    "sketch.dimension" => "Sketch Dimension",
    "sketch.set_dimension" => "Edit Sketch Dimension",
    "sketch.delete" => "Delete",
    "sketch.construction" => "Normal/Construction",
    "sketch.centerline" => "Centerline",
    "sketch.drag" => "Drag",
    "sketch.transform" => "Move/Copy",
    "sketch.insert" => "Insert",
    "sketch.scale_to" => "Scale to Size",
    "sketch.library.insert" => "Sketch Library",
    "sketch.trace" => "Trace Image",
    "sketch.fillet" => "Fillet",
    "sketch.chamfer" => "Chamfer",
    "sketch.spline" => "Fit Point Spline",
    "sketch.control_spline" => "Control Point Spline",
    "sketch.ellipse" => "Ellipse",
    "sketch.elliptical_arc" => "Elliptical Arc",
    "sketch.text" => "Text",
    "sketch.text_edit" => "Edit Text",
    "sketch.text_explode" => "Explode Text",
    "sketch.conic" => "Conic Curve",
    "sketch.offset" => "Offset",
    "sketch.trim" => "Trim",
    "sketch.extend" => "Extend",
    "sketch.break" => "Break",
    "sketch.mirror" => "Mirror",
    "sketch.pattern_rect" => "Rectangular Pattern",
    "sketch.pattern_circ" => "Circular Pattern",
    "sketch.project" => "Project",
    "sketch.project_to_surface" => "Project To Surface",
    "sketch.intersection_curve" => "Intersection Curve",
    "sketch.include" => "Include 3D Geometry",
    "sketch.spun_profile" => "Spun Profile",
    "sketch.intersect" => "Intersect",
    "sketch.isoparametric_curve" => "Isoparametric Curve",
    "sketch.tangent_circle" => "Tangent Circle",
    "sketch.blend" => "Blend Curve",
    "timeline.group" => "Create Group",
    "timeline.ungroup" => "Ungroup",
    "timeline.group_add" => "Move to Group",
    "timeline.group_rename" => "Rename Group",
    "timeline.group_suppress" => "Suppress Group",
    "timeline.group_delete" => "Delete Group",
    "gen.study" => "Generative Study",
    "gen.delete" => "Delete Generative Study",
    "gen.preserve" => "Preserve Geometry",
    "gen.obstacle" => "Obstacle Geometry",
    "gen.starting_shape" => "Starting Shape",
    "gen.design_space" => "Design Space",
    "gen.case" => "Load Case",
    "gen.load" => "Structural Load",
    "gen.constraint" => "Structural Constraint",
    "gen.objective" => "Objectives",
    "gen.materials" => "Study Materials",
    "gen.manufacturing" => "Manufacturing",
    "gen.generate" => "Generate",
    "gen.insert" => "Create Design from Outcome",
    "mesh.tessellate" => "Tessellate",
    "mesh.repair" => "Repair",
    "mesh.face_groups" => "Generate Face Groups",
    "mesh.combine_face_groups" => "Combine Face Groups",
    "mesh.section_sketch" => "Create Mesh Section Sketch",
    "mesh.edit.start" => "Direct Edit",
    "mesh.edit.finish" => "Finish Direct Editing",
    _ => return None,
  };
  Some(label.to_string())
}

/// A feature's name without its number: the command that made it ("Extrude1" → "Extrude").
fn command_of(feature: &str) -> &str {
  let base = feature.trim_end_matches(|c: char| c.is_ascii_digit());
  if base.is_empty() { feature } else { base }
}

/// What a step from `before` to `after` did, as the Undo list names it.
pub fn step_label(before: &Design, after: &Design) -> String {
  let had = |id| before.features.iter().any(|f| f.id == id);
  let has = |id| after.features.iter().any(|f| f.id == id);
  let added: Vec<&str> = after.features.iter().filter(|f| !had(f.id)).map(|f| f.name.as_str()).collect();
  if let Some(first) = added.first() {
    // named by the command, as in Fusion: "Extrude", not "Extrude3"
    return command_of(first).to_string();
  }
  if let Some(f) = before.features.iter().find(|f| !has(f.id)) {
    return format!("Delete {}", f.name);
  }
  if before.marker != after.marker {
    return "Move Timeline Marker".into();
  }
  if before.history_off != after.history_off {
    return if after.history_off { "Do Not Capture Design History" } else { "Capture Design History" }.into();
  }
  if before.parameters != after.parameters {
    let changed = after.parameters.iter().find(|p| !before.parameters.contains(p));
    return match changed {
      Some(p) if before.parameters.len() == after.parameters.len() => format!("Change Parameter {}", p.name),
      Some(p) => format!("Add Parameter {}", p.name),
      None => "Delete Parameter".into(),
    };
  }
  // features in timeline order: the first changed is the edited one (later ones only follow it)
  for (b, a) in before.features.iter().zip(&after.features) {
    if b.id != a.id {
      return format!("Reorder {}", a.name);
    }
    if b != a {
      return if b.name != a.name && b.kind == a.kind {
        format!("Rename {} to {}", b.name, a.name)
      } else if b.suppressed != a.suppressed && b.kind == a.kind {
        format!("{} {}", if a.suppressed { "Suppress" } else { "Unsuppress" }, a.name)
      } else if b.visible != a.visible && b.kind == a.kind {
        format!("{} {}", if a.visible == Some(false) { "Hide" } else { "Show" }, a.name)
      } else {
        format!("Edit {}", a.name)
      };
    }
  }
  let parts: [(bool, &str); 11] = [
    (before.units != after.units, "Change Units"),
    (before.timeline_groups != after.timeline_groups, "Edit Groups"),
    (before.bodies != after.bodies, "Change Body Display"),
    (before.components != after.components, "Edit Components"),
    (before.drawings != after.drawings, "Edit Drawing"),
    (before.configurations != after.configurations, "Edit Configurations"),
    (before.cam != after.cam, "Edit Manufacture Setup"),
    (before.sim != after.sim, "Edit Simulation Study"),
    (before.sheet_metal_rules != after.sheet_metal_rules, "Edit Sheet Metal Rules"),
    (before.contact_sets != after.contact_sets, "Edit Contact Sets"),
    (before.name != after.name, "Rename Design"),
  ];
  parts.iter().find(|(changed, _)| *changed).map_or("Edit", |(_, l)| l).to_string()
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use crate::Session;

  fn session() -> Session {
    let mut s = Session::default();
    s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    s.execute("param.add", &json!({"name": "w", "expr": "5 mm"})).unwrap();
    s
  }

  #[test]
  fn steps_are_named_newest_first() {
    let mut s = session();
    let names = s.undo_history();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(names[0], "Add Parameter w");
    assert!(!names[1].starts_with("Edit") && !names[1].is_empty(), "the box is named: {names:?}");
    assert!(s.redo_history().is_empty());
    assert!(s.undo());
    assert_eq!(s.redo_history(), ["Add Parameter w"]);
    assert_eq!(s.undo_history().len(), 1);
  }

  #[test]
  fn jumping_back_and_forward_several_steps() {
    let mut s = session();
    let full = s.design.clone();
    let names = s.undo_history();
    assert_eq!(s.undo_steps(5), 2, "only two steps exist");
    assert!(s.design.features.is_empty());
    assert!(!s.can_undo());
    // redo lists the same steps, oldest first
    let mut redo = s.redo_history();
    redo.reverse();
    assert_eq!(redo, names);
    assert_eq!(s.redo_steps(2), 2);
    assert_eq!(s.design, full);
    assert_eq!(s.undo_history(), names);
  }

  #[test]
  fn steps_are_named_by_command() {
    let mut s = Session::default();
    s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    let line = s.execute("sketch.line", &json!({"points": [[0, 0], [40, 0]]})).unwrap()["curves"][0].clone();
    s.execute("sketch.dimension", &json!({"type": "distance", "curves": [line], "value": "45"})).unwrap();
    s.execute("sketch.constrain", &json!({"type": "fix", "curves": [line]})).unwrap();
    s.execute("sketch.rect", &json!({"corner1": [0, 10], "corner2": [10, 20]})).unwrap();
    s.execute("design.units", &json!({"unit": "in"})).unwrap();
    s.execute("sketch.finish", &json!({})).unwrap();
    s.execute("extrude", &json!({"distance": 5})).unwrap();
    // finishing the sketch is a step of its own, as in Fusion
    assert_eq!(
      s.undo_history(),
      [
        "Extrude",
        "Finish Sketch",
        "Change Units",
        "2-Point Rectangle",
        "Fix",
        "Sketch Dimension",
        "Line",
        "Create Sketch"
      ]
    );
    // a name travels with its step through undo and redo
    assert_eq!(s.undo_steps(5), 5);
    assert_eq!(s.redo_history(), ["Fix", "2-Point Rectangle", "Change Units", "Finish Sketch", "Extrude"]);
    assert_eq!(s.undo_history(), ["Sketch Dimension", "Line", "Create Sketch"]);
    assert_eq!(s.redo_steps(1), 1);
    assert_eq!(s.undo_history()[0], "Fix");
  }

  #[test]
  fn undoing_finish_sketch_reopens_the_sketch() {
    let mut s = Session::default();
    let sk = s.execute("sketch.create", &json!({"plane": "xy"})).unwrap()["sketch"].as_u64().unwrap() as u32;
    s.execute("sketch.rect", &json!({"corner1": [0, 0], "corner2": [10, 20]})).unwrap();
    s.execute("sketch.finish", &json!({})).unwrap();
    assert_eq!(s.edit_sketch, None);
    assert_eq!(s.undo_history(), ["Finish Sketch", "2-Point Rectangle", "Create Sketch"]);
    assert!(s.undo());
    assert_eq!(s.edit_sketch.map(|f| f.0), Some(sk), "back in the sketch");
    assert_eq!(s.design.features.len(), 1, "the design is as it was");
    assert!(s.redo());
    assert_eq!(s.edit_sketch, None, "finished again");
    // finishing with no sketch open adds no step
    s.execute("sketch.finish", &json!({})).unwrap();
    assert_eq!(s.undo_history().len(), 3);
    // undoing past the Finish Sketch step and the sketch's creation leaves no sketch open
    assert_eq!(s.undo_steps(3), 3);
    assert_eq!(s.edit_sketch, None);
  }

  #[test]
  fn a_transaction_is_named_by_its_first_command() {
    let mut s = Session::default();
    s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    s.execute("transaction.begin", &json!({})).unwrap();
    let line = s.execute("sketch.line", &json!({"points": [[0, 0], [40, 0]]})).unwrap()["curves"][0].clone();
    s.execute("sketch.dimension", &json!({"type": "distance", "curves": [line], "value": "45"})).unwrap();
    s.execute("transaction.commit", &json!({})).unwrap();
    assert_eq!(s.undo_history(), ["Line", "Create Sketch"]);
  }
}
