//! Declarative command dialogs: what a command picks from the selection and which values it
//! asks for. They run through the same preview machinery as the hand-built dialogs in
//! [`crate::dialogs`] (a preview is the command applied; OK keeps it, Cancel withdraws it).

use clf_engine::model::{BodyId, FeatureId, FeatureKind};
use clf_engine::{Sel, Session};
use serde_json::{Map, Value, json};

use crate::icons::Icon;

#[derive(Debug, PartialEq, Eq)]
pub struct Form {
  /// Dialog title (upper case, as Fusion's).
  pub title: &'static str,
  /// Command name in menus and tooltips.
  pub name: &'static str,
  pub hint: &'static str,
  pub icon: Icon,
  pub cmd: &'static str,
  /// Constant parameters, e.g. a construction plane's method.
  pub fixed: &'static [(&'static str, &'static str)],
  pub inputs: &'static [Input],
  /// Picks model geometry that the preview changes: preview on demand (see `Kind::picks_model`).
  pub picks_model: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
  /// Every selected face.
  Faces {
    key: &'static str,
    label: &'static str,
  },
  /// Selected faces, if any (left out of the command when none are).
  FacesOpt {
    key: &'static str,
    label: &'static str,
  },
  /// The next selected face; with `near`, also its centre under that key.
  Face {
    key: &'static str,
    label: &'static str,
    near: Option<&'static str>,
  },
  Edges {
    key: &'static str,
    label: &'static str,
  },
  /// The next selected edge.
  Edge {
    key: &'static str,
    label: &'static str,
  },
  /// Selected bodies (or the bodies of selected faces and edges); `required` = at least one.
  Bodies {
    key: &'static str,
    label: &'static str,
    required: bool,
  },
  /// Selected sketch curves; their sketch goes under "sketch" (optional: empty = all curves).
  Curves {
    key: &'static str,
    label: &'static str,
  },
  /// A sketch: the selected curves' or profiles', the one being edited, or the last one.
  Sketch {
    key: &'static str,
    label: &'static str,
  },
  /// The next selected planar face or construction plane, else an origin plane.
  Plane {
    key: &'static str,
    label: &'static str,
  },
  /// The next selected straight or circular edge or construction axis, else a world axis.
  Axis {
    key: &'static str,
    label: &'static str,
  },
  Expr {
    key: &'static str,
    label: &'static str,
    default: &'static str,
  },
  Text {
    key: &'static str,
    label: &'static str,
    default: &'static str,
  },
  Check {
    key: &'static str,
    label: &'static str,
  },
  Choice {
    key: &'static str,
    label: &'static str,
    options: &'static [(&'static str, &'static str)],
  },
  /// Three numbers: a point or a vector.
  Vec3 {
    key: &'static str,
    label: &'static str,
    default: [&'static str; 3],
  },
  /// Selected sketch profiles, as points inside them (left out when none: every profile).
  Profiles {
    key: &'static str,
    label: &'static str,
  },
  /// The last selected face on another body than the first selected face (a replacement
  /// surface); a form with one keeps its `Faces` to the first face's body.
  FaceOther {
    key: &'static str,
    label: &'static str,
  },
  /// An optional construction or origin plane from a drop-down (never from picks).
  PlaneOpt {
    key: &'static str,
    label: &'static str,
  },
  /// Boundary fill cells of the bodies in the `tools` input, ticked to keep: points inside them
  /// (left out when none are ticked: every cell).
  Cells {
    key: &'static str,
    label: &'static str,
    tools: &'static str,
  },
}

impl Input {
  pub fn key(&self) -> &'static str {
    match self {
      Input::Faces { key, .. }
      | Input::FacesOpt { key, .. }
      | Input::Face { key, .. }
      | Input::Edges { key, .. }
      | Input::Edge { key, .. }
      | Input::Bodies { key, .. }
      | Input::Curves { key, .. }
      | Input::Sketch { key, .. }
      | Input::Plane { key, .. }
      | Input::Axis { key, .. }
      | Input::Expr { key, .. }
      | Input::Text { key, .. }
      | Input::Check { key, .. }
      | Input::Choice { key, .. }
      | Input::Vec3 { key, .. }
      | Input::Profiles { key, .. }
      | Input::FaceOther { key, .. }
      | Input::PlaneOpt { key, .. }
      | Input::Cells { key, .. } => key,
    }
  }
}

/// A form's current values, one per input.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
  pub values: Vec<Value>,
  /// Planes/axes chosen from the drop-down (when nothing is picked for them).
  pub chosen: Vec<Option<Value>>,
  /// Centres of picked faces, for inputs that want one.
  near: Vec<Option<[f64; 3]>>,
  /// Boundary fill cells found for the current tools: a point inside each and its volume.
  pub cells: Vec<([f64; 3], f64)>,
  /// The tools those cells were found for.
  pub cells_for: Value,
}

impl State {
  pub fn new(form: &Form) -> State {
    let values = form
      .inputs
      .iter()
      .map(|i| match i {
        Input::Expr { default, .. } | Input::Text { default, .. } => json!(default),
        Input::Check { .. } => json!(false),
        Input::Choice { options, .. } => json!(options.first().map(|o| o.0).unwrap_or("")),
        Input::Vec3 { default, .. } => json!(default),
        Input::Faces { .. }
        | Input::FacesOpt { .. }
        | Input::Edges { .. }
        | Input::Bodies { .. }
        | Input::Curves { .. }
        | Input::Profiles { .. }
        | Input::Cells { .. } => {
          json!([])
        }
        Input::Plane { .. } => json!("xy"),
        Input::Axis { .. } => json!("z"),
        _ => Value::Null,
      })
      .collect();
    State {
      values,
      chosen: vec![None; form.inputs.len()],
      near: vec![None; form.inputs.len()],
      cells: Vec::new(),
      cells_for: Value::Null,
    }
  }

  /// Refills the pick inputs from the selection. Returns whether anything changed.
  pub fn sync(&mut self, form: &Form, s: &Session, last_sketch: Option<FeatureId>) -> bool {
    let before = self.values.clone();
    let faces: Vec<Value> = s
      .selection
      .iter()
      .filter_map(|x| if let Sel::Face { body, face } = x { Some(json!({"body": body.0, "face": face})) } else { None })
      .collect();
    let edges: Vec<Value> = s
      .selection
      .iter()
      .filter_map(|x| if let Sel::Edge { body, edge } = x { Some(json!({"body": body.0, "edge": edge})) } else { None })
      .collect();
    let mut bodies: Vec<u32> = Vec::new();
    for x in &s.selection {
      if let Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } = x
        && !bodies.contains(&body.0)
      {
        bodies.push(body.0);
      }
    }
    let curves: Vec<(FeatureId, u32)> = s
      .selection
      .iter()
      .filter_map(|x| if let Sel::SketchCurve { sketch, curve } = x { Some((*sketch, curve.0)) } else { None })
      .collect();
    let profile_sketch =
      s.selection.iter().find_map(|x| if let Sel::Profile { sketch, .. } = x { Some(*sketch) } else { None });
    // planes: planar faces and construction plane features, in pick order
    let mut planes: Vec<Value> = Vec::new();
    let mut axes: Vec<Value> = Vec::new();
    for x in &s.selection {
      match x {
        Sel::Face { body, face } if planar(s, *body, *face) => planes.push(json!({"body": body.0, "face": face})),
        Sel::Edge { body, edge } => axes.push(json!({"body": body.0, "edge": edge})),
        Sel::Feature { feature } => match s.design.feature(*feature).map(|f| &f.kind) {
          Some(FeatureKind::ConstructionPlane { .. } | FeatureKind::PlaneBy { .. }) => {
            planes.push(json!({"feature": feature.0}))
          }
          Some(FeatureKind::ConstructionAxis { .. }) => axes.push(json!({"axis": feature.0})),
          _ => {}
        },
        _ => {}
      }
    }
    // a replacement surface is a face on another body; the faces to replace stay on the first
    let mut faces = faces;
    let mut other = Value::Null;
    if form.inputs.iter().any(|i| matches!(i, Input::FaceOther { .. })) && !faces.is_empty() {
      let first = faces[0]["body"].clone();
      other = faces.iter().rev().find(|f| f["body"] != first).cloned().unwrap_or(Value::Null);
      faces.retain(|f| f["body"] == first);
    }
    let profiles: Vec<[f64; 2]> = s
      .selection
      .iter()
      .filter_map(|x| match x {
        Sel::Profile { sketch, index } => {
          s.model.sketches.get(sketch)?.profiles.get(*index).map(|p| p.interior_point())
        }
        _ => None,
      })
      .collect();
    let (mut nf, mut ne, mut np, mut na) = (0, 0, 0, 0);
    for (i, input) in form.inputs.iter().enumerate() {
      let v = match input {
        Input::Profiles { .. } => json!(profiles),
        Input::FaceOther { .. } => other.clone(),
        Input::Faces { .. } | Input::FacesOpt { .. } => json!(faces),
        Input::Face { .. } => {
          nf += 1;
          let f = faces.get(nf - 1).cloned().unwrap_or(Value::Null);
          self.near[i] = face_center(s, &f);
          f
        }
        Input::Edges { .. } => json!(edges),
        Input::Edge { .. } => {
          ne += 1;
          edges.get(ne - 1).cloned().unwrap_or(Value::Null)
        }
        Input::Bodies { .. } => json!(bodies),
        Input::Curves { .. } => json!(curves.iter().map(|c| c.1).collect::<Vec<_>>()),
        Input::Sketch { .. } => {
          let chosen = self.chosen[i].as_ref().and_then(Value::as_u64).map(|f| FeatureId(f as u32));
          let sk = curves.first().map(|c| c.0).or(profile_sketch).or(chosen).or(s.edit_sketch).or(last_sketch);
          sk.map(|f| json!(f.0)).unwrap_or(Value::Null)
        }
        Input::Plane { .. } => {
          np += 1;
          planes.get(np - 1).cloned().or_else(|| self.chosen[i].clone()).unwrap_or(json!("xy"))
        }
        Input::Axis { .. } => {
          na += 1;
          axes.get(na - 1).cloned().or_else(|| self.chosen[i].clone()).unwrap_or(json!("z"))
        }
        _ => continue,
      };
      self.values[i] = v;
    }
    self.values != before
  }

  /// Is everything the command needs there?
  pub fn missing(&self, form: &Form) -> Option<&'static str> {
    for (input, v) in form.inputs.iter().zip(&self.values) {
      let empty = v.is_null() || v.as_array().is_some_and(Vec::is_empty);
      match input {
        Input::Faces { label, .. }
        | Input::Edges { label, .. }
        | Input::Face { label, .. }
        | Input::Edge { label, .. }
          if empty =>
        {
          return Some(label);
        }
        Input::Bodies { label, required: true, .. } | Input::Sketch { label, .. } if empty => return Some(label),
        _ => {}
      }
    }
    None
  }

  /// The command's parameters.
  pub fn params(&self, form: &Form) -> Value {
    let mut p = Map::new();
    for (k, v) in form.fixed {
      p.insert(k.to_string(), json!(v));
    }
    for (i, (input, v)) in form.inputs.iter().zip(&self.values).enumerate() {
      let key = input.key();
      match input {
        Input::Curves { .. } | Input::FacesOpt { .. } | Input::Profiles { .. } | Input::Cells { .. }
          if v.as_array().is_some_and(Vec::is_empty) =>
        {
          continue;
        }
        Input::Bodies { required: false, .. } if v.as_array().is_some_and(Vec::is_empty) => continue,
        Input::Expr { .. } | Input::Text { .. } if v.as_str().is_some_and(|t| t.trim().is_empty()) => continue,
        Input::Face { near: Some(n), .. } => {
          if let Some(c) = self.near[i] {
            p.insert(n.to_string(), json!(c));
          }
          p.insert(key.to_string(), v.clone());
        }
        Input::Vec3 { .. } => {
          let nums: Vec<f64> = v
            .as_array()
            .map(|a| a.iter().map(|x| x.as_str().and_then(|t| t.trim().parse().ok()).unwrap_or(0.0)).collect())
            .unwrap_or_default();
          p.insert(key.to_string(), json!(nums));
        }
        _ if v.is_null() => {}
        _ => {
          p.insert(key.to_string(), v.clone());
        }
      }
    }
    Value::Object(p)
  }
}

fn planar(s: &Session, body: BodyId, face: u32) -> bool {
  s.model
    .body(body)
    .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| f.kind == clf_engine::kernel::SurfaceKind::Plane))
    .unwrap_or(false)
}

fn face_center(s: &Session, v: &Value) -> Option<[f64; 3]> {
  let body = BodyId(v.get("body")?.as_u64()? as u32);
  let face = v.get("face")?.as_u64()? as u32;
  s.model.body(body)?.faces().iter().find(|f| f.id == face).map(|f| f.center.to_array())
}

const OPS: &[(&str, &str)] = &[("new_body", "New body"), ("join", "Join"), ("cut", "Cut"), ("intersect", "Intersect")];

pub static THREAD: Form = Form {
  title: "THREAD",
  name: "Thread",
  hint: "Thread cylindrical faces (ISO metric coarse unless you give a pitch)",
  icon: Icon::Thread,
  cmd: "thread",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Faces" },
    Input::Check { key: "modeled", label: "Modeled (cut the helix)" },
    Input::Expr { key: "pitch", label: "Pitch (empty = ISO)", default: "" },
    Input::Expr { key: "length", label: "Length (empty = full)", default: "" },
    Input::Expr { key: "offset", label: "Offset", default: "0 mm" },
    Input::Check { key: "left", label: "Left-handed" },
    Input::Check { key: "flip", label: "Start from the other end" },
  ],
  picks_model: true,
};

pub static COIL: Form = Form {
  title: "COIL",
  name: "Coil",
  hint: "A helical coil (spring) around a plane's normal",
  icon: Icon::Coil,
  cmd: "coil",
  fixed: &[],
  inputs: &[
    Input::Plane { key: "plane", label: "Plane" },
    Input::Expr { key: "diameter", label: "Diameter", default: "20 mm" },
    Input::Expr { key: "pitch", label: "Pitch", default: "5 mm" },
    Input::Expr { key: "revolutions", label: "Revolutions", default: "5" },
    Input::Choice {
      key: "section",
      label: "Section",
      options: &[
        ("circular", "Circular"),
        ("square", "Square"),
        ("triangle_out", "Triangular (external)"),
        ("triangle_in", "Triangular (internal)"),
      ],
    },
    Input::Expr { key: "section_size", label: "Section size", default: "2 mm" },
    Input::Check { key: "left", label: "Left-handed" },
    Input::Choice { key: "operation", label: "Operation", options: OPS },
  ],
  picks_model: false,
};

pub static PIPE: Form = Form {
  title: "PIPE",
  name: "Pipe",
  hint: "A round tube along a sketch path of lines and arcs",
  icon: Icon::Pipe,
  cmd: "pipe",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "path", label: "Path sketch" },
    Input::Curves { key: "path_curves", label: "Path curves (default all)" },
    Input::Expr { key: "diameter", label: "Diameter", default: "10 mm" },
    Input::Expr { key: "wall", label: "Wall (empty = solid)", default: "" },
    Input::Choice { key: "operation", label: "Operation", options: OPS },
  ],
  picks_model: false,
};

pub static RIB: Form = Form {
  title: "RIB",
  name: "Rib",
  hint: "A rib grown from open sketch lines until it meets the body",
  icon: Icon::Rib,
  cmd: "rib",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Curves { key: "curves", label: "Lines (default all)" },
    Input::Expr { key: "thickness", label: "Thickness", default: "2 mm" },
    Input::Expr { key: "depth", label: "Depth (empty = to next)", default: "" },
    Input::Check { key: "flip", label: "Flip direction" },
  ],
  picks_model: false,
};

pub static WEB: Form = Form {
  title: "WEB",
  name: "Web",
  hint: "A web of thin walls grown from sketch lines along the sketch normal",
  icon: Icon::Web,
  cmd: "web",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Curves { key: "curves", label: "Lines (default all)" },
    Input::Expr { key: "thickness", label: "Thickness", default: "2 mm" },
    Input::Expr { key: "depth", label: "Depth (empty = to next)", default: "" },
    Input::Check { key: "flip", label: "Flip direction" },
  ],
  picks_model: false,
};

pub static SCALE: Form = Form {
  title: "SCALE",
  name: "Scale",
  hint: "Scale bodies about a point",
  icon: Icon::Scale,
  cmd: "scale",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies (default all)", required: false },
    Input::Vec3 { key: "center", label: "Point", default: ["0", "0", "0"] },
    Input::Expr { key: "factor", label: "Scale factor", default: "2" },
  ],
  picks_model: false,
};

pub static SPLIT_FACE: Form = Form {
  title: "SPLIT FACE",
  name: "Split Face",
  hint: "Split faces along a plane; the body stays in one piece",
  icon: Icon::SplitFace,
  cmd: "split_face",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Faces to split" },
    Input::Plane { key: "tool", label: "Splitting tool" },
  ],
  picks_model: true,
};

pub static REMOVE_FACES: Form = Form {
  title: "REMOVE FACES",
  name: "Remove Faces",
  hint: "Remove faces (fillets, holes, bosses) and heal the body",
  icon: Icon::RemoveBody,
  cmd: "remove_faces",
  fixed: &[],
  inputs: &[Input::Faces { key: "faces", label: "Faces" }],
  picks_model: true,
};

pub static OFFSET_FACE: Form = Form {
  title: "OFFSET FACE",
  name: "Offset Face",
  hint: "Offset faces, flat or curved, along their normals",
  icon: Icon::OffsetFace,
  cmd: "press_pull",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Faces" },
    Input::Expr { key: "distance", label: "Distance", default: "1 mm" },
  ],
  picks_model: true,
};

pub static THICKEN: Form = Form {
  title: "THICKEN",
  name: "Thicken",
  hint: "Thicken faces or surface bodies into solids",
  icon: Icon::Thicken,
  cmd: "thicken",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Surface bodies", required: true },
    Input::Expr { key: "thickness", label: "Thickness", default: "1 mm" },
    Input::Check { key: "symmetric", label: "Symmetric" },
    Input::Check { key: "flip", label: "Flip" },
    Input::Choice { key: "operation", label: "Operation", options: OPS },
  ],
  picks_model: true,
};

pub static PLANE_ANGLE: Form = Form {
  title: "PLANE AT ANGLE",
  name: "Plane at Angle",
  hint: "A plane through an edge or axis, turned by an angle",
  icon: Icon::PlaneAlt,
  cmd: "plane",
  fixed: &[("method", "angle")],
  inputs: &[
    Input::Axis { key: "axis", label: "Line" },
    Input::Plane { key: "plane", label: "Reference plane" },
    Input::Expr { key: "angle", label: "Angle", default: "45 deg" },
  ],
  picks_model: false,
};

pub static MIDPLANE: Form = Form {
  title: "MIDPLANE",
  name: "Midplane",
  hint: "A plane halfway between two planes or faces",
  icon: Icon::Plane,
  cmd: "plane",
  fixed: &[("method", "midplane")],
  inputs: &[Input::Plane { key: "a", label: "First plane" }, Input::Plane { key: "b", label: "Second plane" }],
  picks_model: false,
};

pub static TANGENT_PLANE: Form = Form {
  title: "TANGENT PLANE",
  name: "Tangent Plane",
  hint: "A plane tangent to a cylinder, cone or sphere at the picked point",
  icon: Icon::PlaneAlt,
  cmd: "plane",
  fixed: &[("method", "tangent")],
  inputs: &[Input::Face { key: "face", label: "Face", near: Some("near") }],
  picks_model: false,
};

pub static PLANE_TWO_EDGES: Form = Form {
  title: "PLANE THROUGH TWO EDGES",
  name: "Plane Through Two Edges",
  hint: "A plane through two coplanar straight edges",
  icon: Icon::PlaneAlt,
  cmd: "plane",
  fixed: &[("method", "two_edges")],
  inputs: &[Input::Edge { key: "a", label: "First edge" }, Input::Edge { key: "b", label: "Second edge" }],
  picks_model: false,
};

pub static PLANE_THREE_POINTS: Form = Form {
  title: "PLANE THROUGH THREE POINTS",
  name: "Plane Through Three Points",
  hint: "A plane through three points",
  icon: Icon::PlaneAlt,
  cmd: "plane.points",
  fixed: &[],
  inputs: &[
    Input::Vec3 { key: "a", label: "Point 1", default: ["0", "0", "0"] },
    Input::Vec3 { key: "b", label: "Point 2", default: ["10", "0", "0"] },
    Input::Vec3 { key: "c", label: "Point 3", default: ["0", "10", "0"] },
  ],
  picks_model: false,
};

pub static PLANE_ALONG_PATH: Form = Form {
  title: "PLANE ALONG PATH",
  name: "Plane Along Path",
  hint: "A plane across an edge, a fraction of the way along it",
  icon: Icon::PlaneAlt,
  cmd: "plane",
  fixed: &[("method", "along_path")],
  inputs: &[
    Input::Edge { key: "edge", label: "Path" },
    Input::Expr { key: "t", label: "Distance (0–1)", default: "0.5" },
  ],
  picks_model: false,
};

pub static AXIS_FACE: Form = Form {
  title: "AXIS THROUGH CYLINDER/CONE/TORUS",
  name: "Axis Through Cylinder/Cone/Torus",
  hint: "The axis of a round face",
  icon: Icon::Axis,
  cmd: "axis",
  fixed: &[("method", "face")],
  inputs: &[Input::Face { key: "face", label: "Face", near: None }],
  picks_model: false,
};

pub static AXIS_NORMAL: Form = Form {
  title: "AXIS PERPENDICULAR TO FACE",
  name: "Axis Perpendicular to Face",
  hint: "An axis normal to a face through the picked point",
  icon: Icon::Axis,
  cmd: "axis",
  fixed: &[("method", "normal")],
  inputs: &[Input::Face { key: "face", label: "Face", near: Some("near") }],
  picks_model: false,
};

pub static AXIS_TWO_PLANES: Form = Form {
  title: "AXIS THROUGH TWO PLANES",
  name: "Axis Through Two Planes",
  hint: "The line where two planes meet",
  icon: Icon::Axis,
  cmd: "axis",
  fixed: &[("method", "two_planes")],
  inputs: &[Input::Plane { key: "a", label: "First plane" }, Input::Plane { key: "b", label: "Second plane" }],
  picks_model: false,
};

pub static AXIS_EDGE: Form = Form {
  title: "AXIS THROUGH EDGE",
  name: "Axis Through Edge",
  hint: "An axis along a straight edge (or a circle's axis)",
  icon: Icon::Axis,
  cmd: "axis",
  fixed: &[("method", "edge")],
  inputs: &[Input::Edge { key: "edge", label: "Edge" }],
  picks_model: false,
};

pub static AXIS_TWO_POINTS: Form = Form {
  title: "AXIS THROUGH TWO POINTS",
  name: "Axis Through Two Points",
  hint: "An axis through two points",
  icon: Icon::Axis,
  cmd: "axis.points",
  fixed: &[],
  inputs: &[
    Input::Vec3 { key: "a", label: "Point 1", default: ["0", "0", "0"] },
    Input::Vec3 { key: "b", label: "Point 2", default: ["0", "0", "10"] },
  ],
  picks_model: false,
};

pub static POINT_VERTEX: Form = Form {
  title: "POINT AT VERTEX",
  name: "Point at Vertex",
  hint: "A point at an end of an edge",
  icon: Icon::CPoint,
  cmd: "point.vertex",
  fixed: &[],
  inputs: &[
    Input::Edge { key: "edge", label: "Edge" },
    Input::Choice { key: "end", label: "End", options: &[("start", "Start"), ("end", "End")] },
  ],
  picks_model: false,
};

pub static POINT_TWO_EDGES: Form = Form {
  title: "POINT THROUGH TWO EDGES",
  name: "Point Through Two Edges",
  hint: "Where two edges meet",
  icon: Icon::CPoint,
  cmd: "point",
  fixed: &[("method", "two_edges")],
  inputs: &[Input::Edge { key: "a", label: "First edge" }, Input::Edge { key: "b", label: "Second edge" }],
  picks_model: false,
};

pub static POINT_THREE_PLANES: Form = Form {
  title: "POINT THROUGH THREE PLANES",
  name: "Point Through Three Planes",
  hint: "Where three planes meet",
  icon: Icon::CPoint,
  cmd: "point",
  fixed: &[("method", "three_planes")],
  inputs: &[
    Input::Plane { key: "a", label: "Plane 1" },
    Input::Plane { key: "b", label: "Plane 2" },
    Input::Plane { key: "c", label: "Plane 3" },
  ],
  picks_model: false,
};

pub static POINT_CENTER: Form = Form {
  title: "POINT AT CENTER",
  name: "Point at Center of Circle/Sphere/Torus",
  hint: "The centre of a round face",
  icon: Icon::CPoint,
  cmd: "point",
  fixed: &[("method", "center")],
  inputs: &[Input::Face { key: "face", label: "Face", near: None }],
  picks_model: false,
};

pub static POINT_EDGE_PLANE: Form = Form {
  title: "POINT AT EDGE AND PLANE",
  name: "Point at Edge and Plane",
  hint: "Where an edge crosses a plane",
  icon: Icon::CPoint,
  cmd: "point",
  fixed: &[("method", "edge_plane")],
  inputs: &[Input::Edge { key: "edge", label: "Edge" }, Input::Plane { key: "plane", label: "Plane" }],
  picks_model: false,
};

pub static POINT_ALONG_PATH: Form = Form {
  title: "POINT ALONG PATH",
  name: "Point Along Path",
  hint: "A point a fraction of the way along an edge",
  icon: Icon::CPoint,
  cmd: "point",
  fixed: &[("method", "along_path")],
  inputs: &[
    Input::Edge { key: "edge", label: "Path" },
    Input::Expr { key: "t", label: "Distance (0–1)", default: "0.5" },
  ],
  picks_model: false,
};

pub static SURFACE_EXTRUDE: Form = Form {
  title: "EXTRUDE",
  name: "Extrude",
  hint: "A surface extruded from sketch curves (open or closed)",
  icon: Icon::Extrude,
  cmd: "surface.extrude",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Curves { key: "curves", label: "Curves (default all)" },
    Input::Expr { key: "distance", label: "Distance", default: "10 mm" },
    Input::Check { key: "symmetric", label: "Symmetric" },
    Input::Check { key: "flip", label: "Flip" },
  ],
  picks_model: false,
};

pub static SURFACE_REVOLVE: Form = Form {
  title: "REVOLVE",
  name: "Revolve",
  hint: "A surface revolved from sketch curves",
  icon: Icon::Revolve,
  cmd: "surface.revolve",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Curves { key: "curves", label: "Curves (default all)" },
    Input::Axis { key: "axis", label: "Axis" },
    Input::Expr { key: "angle", label: "Angle", default: "360 deg" },
  ],
  picks_model: false,
};

pub static PATCH: Form = Form {
  title: "PATCH",
  name: "Patch",
  hint: "A surface filling a closed boundary of edges",
  icon: Icon::Patch,
  cmd: "surface.patch",
  fixed: &[],
  inputs: &[Input::Edges { key: "edges", label: "Boundary edges" }],
  picks_model: true,
};

pub static PATCH_SKETCH: Form = Form {
  title: "PATCH",
  name: "Patch (Sketch)",
  hint: "A surface filling a closed sketch",
  icon: Icon::Patch,
  cmd: "surface.patch",
  fixed: &[],
  inputs: &[Input::Sketch { key: "sketch", label: "Sketch" }],
  picks_model: false,
};

pub static STITCH: Form = Form {
  title: "STITCH",
  name: "Stitch",
  hint: "Sew surface bodies together (a solid when they close up)",
  icon: Icon::Stitch,
  cmd: "surface.stitch",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies", required: true },
    Input::Expr { key: "tolerance", label: "Tolerance", default: "0.01 mm" },
  ],
  picks_model: true,
};

pub static UNSTITCH: Form = Form {
  title: "UNSTITCH",
  name: "Unstitch",
  hint: "Break bodies into one surface per face",
  icon: Icon::Unstitch,
  cmd: "surface.unstitch",
  fixed: &[],
  inputs: &[Input::Bodies { key: "bodies", label: "Bodies", required: true }],
  picks_model: true,
};

pub static REVERSE_NORMAL: Form = Form {
  title: "REVERSE NORMAL",
  name: "Reverse Normal",
  hint: "Flip the normals of surface bodies",
  icon: Icon::ReverseNormal,
  cmd: "surface.reverse",
  fixed: &[],
  inputs: &[Input::Bodies { key: "bodies", label: "Bodies", required: true }],
  picks_model: true,
};

pub static MESH_REDUCE: Form = Form {
  title: "REDUCE",
  name: "Reduce",
  hint: "Fewer triangles on mesh bodies",
  icon: Icon::Reduce,
  cmd: "mesh.reduce",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies", required: true },
    Input::Expr { key: "keep", label: "Keep (fraction)", default: "0.5" },
  ],
  picks_model: true,
};

pub static MESH_SMOOTH: Form = Form {
  title: "SMOOTH",
  name: "Smooth",
  hint: "Smooth mesh bodies",
  icon: Icon::Smooth,
  cmd: "mesh.smooth",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies", required: true },
    Input::Expr { key: "passes", label: "Passes", default: "3" },
  ],
  picks_model: true,
};

pub static MESH_CONVERT: Form = Form {
  title: "CONVERT MESH",
  name: "Convert Mesh",
  hint: "Mesh bodies to clean B-rep: coplanar triangles become faces",
  icon: Icon::ConvertMesh,
  cmd: "mesh.convert",
  fixed: &[],
  inputs: &[Input::Bodies { key: "bodies", label: "Bodies", required: true }],
  picks_model: true,
};

pub static APPEARANCE: Form = Form {
  title: "APPEARANCE",
  name: "Appearance",
  hint: "Apply an appearance to bodies (A)",
  icon: Icon::Appearance,
  cmd: "body.appearance",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies", required: true },
    Input::Choice {
      key: "appearance",
      label: "Appearance",
      options: &[
        ("Steel - Satin", "Steel - Satin"),
        ("Aluminum - Brushed", "Aluminum - Brushed"),
        ("Aluminum - Anodized Black", "Aluminum - Anodized Black"),
        ("Aluminum - Anodized Red", "Aluminum - Anodized Red"),
        ("Aluminum - Anodized Blue", "Aluminum - Anodized Blue"),
        ("Brass - Polished", "Brass - Polished"),
        ("Copper", "Copper"),
        ("Gold - Polished", "Gold - Polished"),
        ("Plastic - Matte (White)", "Plastic - Matte (White)"),
        ("Plastic - Matte (Black)", "Plastic - Matte (Black)"),
        ("ABS (Red)", "ABS (Red)"),
        ("ABS (Yellow)", "ABS (Yellow)"),
        ("ABS (Green)", "ABS (Green)"),
        ("Rubber - Black", "Rubber - Black"),
        ("Wood - Oak", "Wood - Oak"),
        ("Glass - Clear", "Glass - Clear"),
      ],
    },
  ],
  picks_model: false,
};

pub static SK_TEXT: Form = Form {
  title: "TEXT",
  name: "Text",
  hint: "Text as sketch outlines you can extrude",
  icon: Icon::SkText,
  cmd: "sketch.text.form",
  fixed: &[],
  inputs: &[
    Input::Text { key: "text", label: "Text", default: "CLFusion" },
    Input::Expr { key: "height", label: "Height", default: "5" },
    Input::Expr { key: "x", label: "X", default: "0" },
    Input::Expr { key: "y", label: "Y", default: "0" },
    Input::Expr { key: "angle", label: "Angle (deg)", default: "0" },
  ],
  picks_model: false,
};

/// Commands a form runs that need their parameters reshaped first ("plane.points" →
/// "plane" with point references, …). Returns the engine command and parameters.
pub fn engine_command(form: &Form, p: Value) -> (&'static str, Value) {
  let pt = |k: &str| json!({"at": p.get(k).cloned().unwrap_or(json!([0, 0, 0]))});
  match form.cmd {
    "plane.points" => ("plane", json!({"method": "three_points", "a": pt("a"), "b": pt("b"), "c": pt("c")})),
    "axis.points" => ("axis", json!({"method": "two_points", "a": pt("a"), "b": pt("b")})),
    "point.vertex" => (
      "point",
      json!({"method": "vertex", "point": {"vertex": p.get("edge").cloned().unwrap_or(Value::Null), "end": p.get("end") == Some(&json!("end"))}}),
    ),
    "sketch.text.form" => {
      let num = |k: &str| p.get(k).and_then(Value::as_str).and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
      (
        "sketch.text",
        json!({"at": [num("x"), num("y")], "text": p.get("text").cloned().unwrap_or(json!("")), "height": num("height").max(0.01), "angle": num("angle")}),
      )
    }
    _ => (form.cmd, p),
  }
}

pub static RULED: Form = Form {
  title: "RULED",
  name: "Ruled",
  hint: "A ruled surface between two edges",
  icon: Icon::Ruled,
  cmd: "surface.ruled",
  fixed: &[],
  inputs: &[Input::Edge { key: "a", label: "First edge" }, Input::Edge { key: "b", label: "Second edge" }],
  picks_model: false,
};

pub static SURFACE_OFFSET: Form = Form {
  title: "OFFSET",
  name: "Offset",
  hint: "New surfaces offset from faces along their normals",
  icon: Icon::SurfOffset,
  cmd: "surface.offset",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Faces" },
    Input::Expr { key: "distance", label: "Distance", default: "2 mm" },
  ],
  picks_model: true,
};

pub static TRIM: Form = Form {
  title: "TRIM",
  name: "Trim",
  hint: "Trim surface bodies with a plane",
  icon: Icon::Trim,
  cmd: "surface.trim",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Surfaces", required: true },
    Input::Plane { key: "tool", label: "Trimming tool" },
    Input::Check { key: "flip", label: "Keep the other side" },
  ],
  picks_model: true,
};

pub static CAM_SETUP: Form = Form {
  title: "SETUP",
  name: "New Setup",
  hint: "A milling setup: the stock around the part and its work origin",
  icon: Icon::Box,
  cmd: "cam.setup",
  fixed: &[],
  inputs: &[
    Input::Text { key: "name", label: "Name", default: "" },
    Input::Expr { key: "stock_side", label: "Stock side offset", default: "2" },
    Input::Expr { key: "stock_top", label: "Stock top offset", default: "1" },
  ],
  picks_model: false,
};

pub static CAM_FACE: Form = Form {
  title: "FACE",
  name: "Face",
  hint: "Face the stock down to the top of the part",
  icon: Icon::Extrude,
  cmd: "cam.face",
  fixed: &[],
  inputs: &[
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "20 mm" },
    Input::Expr { key: "stepover", label: "Step-over (of the tool)", default: "0.7" },
    Input::Expr { key: "stepdown", label: "Maximum step-down", default: "1 mm" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1500" },
  ],
  picks_model: false,
};

pub static CAM_POCKET: Form = Form {
  title: "2D POCKET",
  name: "2D Pocket",
  hint: "Clear pockets down to their flat floors",
  icon: Icon::Shell,
  cmd: "cam.pocket",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "floors", label: "Pocket floors" },
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "stepover", label: "Step-over (of the tool)", default: "0.4" },
    Input::Expr { key: "stepdown", label: "Maximum step-down", default: "2 mm" },
    Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1000" },
    Input::Expr { key: "plunge", label: "Plunge (mm/min)", default: "300" },
  ],
  picks_model: false,
};

pub static CAM_CONTOUR: Form = Form {
  title: "2D CONTOUR",
  name: "2D Contour",
  hint: "Cut around the outline of faces (default the part's bottom)",
  icon: Icon::SkOffset,
  cmd: "cam.contour",
  fixed: &[],
  inputs: &[
    Input::FacesOpt { key: "faces", label: "Outline faces (default bottom)" },
    Input::Check { key: "inside", label: "Inside the outline" },
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "stepdown", label: "Maximum step-down", default: "2 mm" },
    Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1000" },
    Input::Expr { key: "plunge", label: "Plunge (mm/min)", default: "300" },
  ],
  picks_model: false,
};

pub static CAM_DRILL: Form = Form {
  title: "DRILL",
  name: "Drill",
  hint: "Drill vertical holes (default every hole)",
  icon: Icon::Hole,
  cmd: "cam.drill",
  fixed: &[],
  inputs: &[
    Input::FacesOpt { key: "holes", label: "Holes (default all)" },
    Input::Expr { key: "tool_diameter", label: "Drill diameter", default: "6 mm" },
    Input::Expr { key: "peck", label: "Peck depth (0 = none)", default: "0" },
    Input::Expr { key: "plunge", label: "Feed (mm/min)", default: "200" },
  ],
  picks_model: false,
};

pub static EMBOSS: Form = Form {
  title: "EMBOSS",
  name: "Emboss",
  hint: "Raise or sink sketch profiles or text on a face, flat or curved",
  icon: Icon::Emboss,
  cmd: "emboss",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Profiles { key: "profiles", label: "Profiles" },
    Input::Face { key: "face", label: "Face", near: None },
    Input::Expr { key: "depth", label: "Depth", default: "1 mm" },
    Input::Check { key: "deboss", label: "Deboss (sink into the face)" },
    Input::Check { key: "wrap", label: "Wrap around a cylinder" },
  ],
  picks_model: true,
};

pub static BOUNDARY_FILL: Form = Form {
  title: "BOUNDARY FILL",
  name: "Boundary Fill",
  hint: "Solids from the closed cells that bodies and a plane bound",
  icon: Icon::BoundaryFill,
  cmd: "boundary_fill",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "tools", label: "Tools", required: true },
    Input::PlaneOpt { key: "planes", label: "Plane" },
    Input::Cells { key: "cells", label: "Cells", tools: "tools" },
    Input::Choice { key: "operation", label: "Operation", options: OPS },
    Input::Check { key: "remove_tools", label: "Remove tools" },
  ],
  picks_model: true,
};

pub static REPLACE_FACE: Form = Form {
  title: "REPLACE FACE",
  name: "Replace Face",
  hint: "Replace flat faces with a face of another body (or a plane); neighbours extend or trim",
  icon: Icon::ReplaceFace,
  cmd: "replace_face",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Source faces" },
    Input::FaceOther { key: "target", label: "Target face" },
    Input::PlaneOpt { key: "plane", label: "Or a plane" },
  ],
  picks_model: true,
};
