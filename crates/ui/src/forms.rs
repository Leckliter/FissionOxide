//! Declarative command dialogs: what a command picks from the selection and which values it
//! asks for. They run through the same preview machinery as the hand-built dialogs in
//! [`crate::dialogs`] (a preview is the command applied; OK keeps it, Cancel withdraws it).

use fox_engine::model::{BodyId, FeatureId, FeatureKind};
use fox_engine::{Sel, Session};
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
  /// A view of the open drawing (drawing commands get the drawing too).
  DrawingView {
    key: &'static str,
    label: &'static str,
  },
  /// A component of the design.
  Component {
    key: &'static str,
    label: &'static str,
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
      | Input::DrawingView { key, .. }
      | Input::Component { key, .. }
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
  /// The open drawing, for drawing commands.
  pub drawing: Option<u32>,
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
      drawing: None,
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
    // sketches of the picked curves, then of picked profiles (the n-th sketch input takes the n-th)
    let mut sketches: Vec<FeatureId> = curves.iter().map(|c| c.0).collect();
    sketches.extend(
      s.selection.iter().filter_map(|x| if let Sel::Profile { sketch, .. } = x { Some(*sketch) } else { None }),
    );
    sketches.dedup();
    let mut seen = Vec::new();
    sketches.retain(|f| {
      if seen.contains(f) {
        false
      } else {
        seen.push(*f);
        true
      }
    });
    let mut ns = 0;
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
          ns += 1;
          let sk = if ns == 1 {
            curves.first().map(|c| c.0).or(profile_sketch).or(chosen).or(s.edit_sketch).or(last_sketch)
          } else {
            sketches.get(ns - 1).copied().or(chosen)
          };
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
        Input::DrawingView { label, .. } | Input::Component { label, .. } if empty => return Some(label),
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
    if let Some(d) = self.drawing
      && form.cmd.starts_with("drawing.")
    {
      p.insert("drawing".into(), json!(d));
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

  /// Points drawing commands at the open drawing; views and components default to the first.
  pub fn set_drawing(&mut self, form: &Form, s: &Session, drawing: Option<u32>) {
    self.drawing = drawing;
    for (input, v) in form.inputs.iter().zip(self.values.iter_mut()) {
      match input {
        Input::DrawingView { .. } if v.is_null() => {
          if let Some(view) = drawing.and_then(|d| s.design.drawing(d)).and_then(|d| d.views.first()) {
            *v = json!(view.id);
          }
        }
        Input::Component { .. } if v.is_null() => {
          if let Some(c) = s.design.components.first() {
            *v = json!(c.id.0);
          }
        }
        _ => {}
      }
    }
  }
}

fn planar(s: &Session, body: BodyId, face: u32) -> bool {
  s.model
    .body(body)
    .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| f.kind == fox_engine::kernel::SurfaceKind::Plane))
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
  hint: "Thread cylindrical faces (an ISO metric size resizes the face; ISO coarse unless you give a pitch)",
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
    Input::Text { key: "designation", label: "Size (M10x1.5; empty = as drawn)", default: "" },
    Input::Text { key: "class", label: "Class (empty = 6g / 6H)", default: "" },
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
  hint: "A round, square or triangular tube along a sketch path of lines and arcs",
  icon: Icon::Pipe,
  cmd: "pipe",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "path", label: "Path sketch" },
    Input::Curves { key: "path_curves", label: "Path curves (default all)" },
    Input::Choice {
      key: "section",
      label: "Section",
      options: &[("circular", "Circular"), ("square", "Square"), ("triangular", "Triangular")],
    },
    Input::Expr { key: "diameter", label: "Size", default: "10 mm" },
    Input::Expr { key: "wall", label: "Wall (empty = solid)", default: "" },
    Input::Expr { key: "distance", label: "Distance (fraction of the path)", default: "1" },
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
    Input::Choice { key: "corners", label: "Thicken type", options: &[("sharp", "Sharp"), ("round", "Rounded")] },
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
  hint: "Editable text whose outlines you can extrude, at a point or along a picked curve",
  icon: Icon::SkText,
  cmd: "sketch.text.form",
  fixed: &[],
  inputs: &[
    Input::Text { key: "text", label: "Text", default: "FissionOxide" },
    Input::Choice { key: "placement", label: "Placement", options: TEXT_PLACEMENTS },
    Input::Expr { key: "x", label: "X", default: "0" },
    Input::Expr { key: "y", label: "Y", default: "0" },
    Input::Expr { key: "x2", label: "Corner 2 X (box)", default: "40" },
    Input::Expr { key: "y2", label: "Corner 2 Y (box)", default: "10" },
    Input::Expr { key: "height", label: "Height", default: "5" },
    Input::Expr { key: "angle", label: "Angle (deg)", default: "0" },
    Input::Choice { key: "font", label: "Font", options: TEXT_FONTS },
    Input::Check { key: "bold", label: "Bold" },
    Input::Check { key: "italic", label: "Italic" },
    Input::Check { key: "underline", label: "Underline" },
    Input::Choice { key: "halign", label: "Horizontal alignment", options: TEXT_HALIGN },
    Input::Choice { key: "valign", label: "Vertical alignment", options: TEXT_VALIGN },
    Input::Expr { key: "spacing", label: "Character spacing (%)", default: "0" },
    Input::Curves { key: "path", label: "Along path (optional)" },
    Input::Check { key: "below", label: "Below the path" },
    Input::Choice {
      key: "align",
      label: "Path alignment",
      options: &[("left", "Left"), ("center", "Center"), ("right", "Right")],
    },
  ],
  picks_model: false,
};

/// Sketch Text: at a point, or in a box between two corners (the alignment places it in the box).
const TEXT_PLACEMENTS: &[(&str, &str)] = &[("point", "Point"), ("box", "Two-corner box")];
const TEXT_HALIGN: &[(&str, &str)] = &[("left", "Left"), ("center", "Center"), ("right", "Right")];
const TEXT_VALIGN: &[(&str, &str)] =
  &[("baseline", "Baseline"), ("top", "Top"), ("middle", "Middle"), ("bottom", "Bottom")];

/// The `sketch.text` parameters of the Text form's values: a box's corners place the text by its
/// alignment (left, centre or right; top, middle or bottom of the box).
fn text_params(p: &Value) -> Value {
  let num = |k: &str| p.get(k).and_then(Value::as_str).and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
  let get = |k: &str, d: &str| p.get(k).and_then(Value::as_str).unwrap_or(d).to_string();
  let (halign, mut valign) = (get("halign", "left"), get("valign", "baseline"));
  let at = if get("placement", "point") == "box" {
    let (x0, x1) = (num("x").min(num("x2")), num("x").max(num("x2")));
    let (y0, y1) = (num("y").min(num("y2")), num("y").max(num("y2")));
    if valign == "baseline" {
      valign = "top".into();
    }
    let x = match halign.as_str() {
      "center" => (x0 + x1) / 2.0,
      "right" => x1,
      _ => x0,
    };
    let y = match valign.as_str() {
      "middle" => (y0 + y1) / 2.0,
      "bottom" => y0,
      _ => y1,
    };
    [x, y]
  } else {
    [num("x"), num("y")]
  };
  let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
  json!({
    "at": at,
    "text": p.get("text").cloned().unwrap_or(json!("")),
    "height": num("height").max(0.01),
    "angle": num("angle"),
    "font": p.get("font").cloned().unwrap_or(json!("sans")),
    "bold": flag("bold"),
    "italic": flag("italic"),
    "underline": flag("underline"),
    "halign": halign,
    "valign": valign,
    "spacing": num("spacing"),
  })
}

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
      let mut q = text_params(&p);
      // a picked curve lays the text along it instead of at the point
      if let Some(curve) = p.get("path").and_then(Value::as_array).and_then(|a| a.first()) {
        if let Some(o) = q.as_object_mut() {
          o.remove("at");
          o.remove("angle");
        }
        q["path"] = curve.clone();
        q["above"] = json!(p.get("below") != Some(&json!(true)));
        q["align"] = p.get("align").cloned().unwrap_or(json!("left"));
        if let Some(sk) = p.get("sketch") {
          q["sketch"] = sk.clone();
        }
      }
      ("sketch.text", q)
    }
    // drawing annotations: the frame's or symbol's offset from its point, in sheet mm
    c if c.starts_with("drawing.") && p.get("offset_x").is_some() => {
      let num = |k: &str| p.get(k).and_then(Value::as_str).and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
      let mut q = p.clone();
      if let Some(m) = q.as_object_mut() {
        m.remove("offset_x");
        m.remove("offset_y");
        m.insert("offset".into(), json!([num("offset_x"), num("offset_y")]));
      }
      (form.cmd, q)
    }
    "surface.trim.body" => {
      // the tool is the body of the picked face; it doesn't trim itself
      let tool = p.pointer("/tool/body").cloned().unwrap_or(Value::Null);
      let bodies: Vec<Value> =
        p.get("bodies").and_then(Value::as_array).into_iter().flatten().filter(|b| **b != tool).cloned().collect();
      (
        "surface.trim",
        json!({"bodies": bodies, "tool": {"body": tool}, "flip": p.get("flip").cloned().unwrap_or(json!(false))}),
      )
    }
    "surface.trim.sketch" => {
      let tool = json!({"sketch": p.get("sketch").cloned().unwrap_or(Value::Null), "curves": p.get("curves").cloned().unwrap_or(json!([]))});
      (
        "surface.trim",
        json!({"bodies": p.get("bodies").cloned().unwrap_or(json!([])), "tool": tool, "flip": p.get("flip").cloned().unwrap_or(json!(false))}),
      )
    }
    "surface.offset.variable" => {
      let d2 = p.get("distance2").cloned().unwrap_or(json!("0"));
      let fds: Vec<Value> = p
        .get("faces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|f| json!({"face": f, "distance": d2}))
        .collect();
      (
        "surface.offset",
        json!({"bodies": p.get("bodies").cloned().unwrap_or(json!([])), "distance": p.get("distance").cloned().unwrap_or(json!("0")), "face_distances": fds}),
      )
    }
    "sketch.text_edit.form" => {
      let mut q = p.clone();
      // the angle box is in degrees: a plain number is what the command takes
      if let Some(a) = p.get("angle").and_then(Value::as_str).and_then(|t| t.trim().parse::<f64>().ok()) {
        q["angle"] = json!(a);
      }
      // an empty spacing box keeps the text's spacing
      if p.get("spacing").and_then(Value::as_str).is_some_and(|t| t.trim().is_empty())
        && let Some(o) = q.as_object_mut()
      {
        o.remove("spacing");
      }
      ("sketch.text_edit", q)
    }
    "sketch.scale" => {
      let num = |k: &str| p.get(k).and_then(Value::as_str).and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
      let mut q = json!({"center": [num("x"), num("y")], "scale": p.get("scale").cloned().unwrap_or(json!("1"))});
      for k in ["sketch", "curves"] {
        if let Some(v) = p.get(k) {
          q[k] = v.clone();
        }
      }
      ("sketch.transform", q)
    }
    // Closest Point takes no direction
    "sketch.project_to_surface" => {
      let mut q = p.clone();
      if p.get("type") != Some(&json!("along_vector"))
        && let Some(o) = q.as_object_mut()
      {
        o.remove("direction");
      }
      (form.cmd, q)
    }
    "align.form" => {
      let face = |k: &str| json!({"face": p.get(k).cloned().unwrap_or(Value::Null)});
      let mut q = json!({"moving_geometry": face("moving"), "fixed_geometry": face("fixed")});
      for k in ["relation", "offset", "flip"] {
        if let Some(v) = p.get(k) {
          q[k] = v.clone();
        }
      }
      ("align", q)
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

const CAM_POSTS: &[(&str, &str)] =
  &[("fanuc", "Fanuc"), ("haas", "Haas"), ("grbl", "GRBL"), ("linuxcnc", "LinuxCNC"), ("mach3", "Mach3 / Mach4")];

const CAM_TOOL: Input = Input::Expr { key: "tool_number", label: "Library tool (0 = by diameter)", default: "0" };

pub static CAM_SETUP: Form = Form {
  title: "SETUP",
  name: "New Setup",
  hint: "A milling setup: the stock around the part, its work origin and post-processor",
  icon: Icon::Box,
  cmd: "cam.setup",
  fixed: &[],
  inputs: &[
    Input::Text { key: "name", label: "Name", default: "" },
    Input::Expr { key: "stock_side", label: "Stock side offset", default: "2" },
    Input::Expr { key: "stock_top", label: "Stock top offset", default: "1" },
    Input::Choice { key: "post", label: "Post-processor", options: CAM_POSTS },
    Input::Choice { key: "units", label: "G-code units", options: &[("mm", "Millimetres"), ("inch", "Inches")] },
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
    CAM_TOOL,
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
    CAM_TOOL,
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "stepover", label: "Step-over (of the tool)", default: "0.4" },
    Input::Expr { key: "stepdown", label: "Maximum step-down", default: "2 mm" },
    Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0" },
    Input::Check { key: "rest", label: "Rest machining (only what earlier operations left)" },
    Input::Choice {
      key: "entry",
      label: "Entry",
      options: &[("plunge", "Plunge"), ("ramp", "Ramp"), ("helix", "Helix")],
    },
    Input::Expr { key: "ramp_angle", label: "Ramp angle (°)", default: "5" },
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
    CAM_TOOL,
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "stepdown", label: "Maximum step-down", default: "2 mm" },
    Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0" },
    Input::Choice { key: "entry", label: "Entry", options: &[("plunge", "Plunge"), ("ramp", "Ramp")] },
    Input::Expr { key: "ramp_angle", label: "Ramp angle (°)", default: "5" },
    Input::Expr { key: "tabs", label: "Holding tabs per loop", default: "0" },
    Input::Expr { key: "tab_width", label: "Tab width", default: "5 mm" },
    Input::Expr { key: "tab_height", label: "Tab height", default: "2 mm" },
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
    CAM_TOOL,
    Input::Expr { key: "tool_diameter", label: "Drill diameter", default: "6 mm" },
    Input::Expr { key: "peck", label: "Peck depth (0 = none)", default: "0" },
    Input::Expr { key: "plunge", label: "Feed (mm/min)", default: "200" },
  ],
  picks_model: false,
};

// ---------------------------------------------------------------- drawing annotations

pub static DRAWING_BREAK: Form = Form {
  title: "BREAK VIEW",
  name: "Break View",
  hint: "Leave a stretch of a long view out; dimensions keep the true length",
  icon: Icon::BreakView,
  cmd: "drawing.break",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Choice {
      key: "direction",
      label: "Direction",
      options: &[("horizontal", "Horizontal"), ("vertical", "Vertical")],
    },
    Input::Expr { key: "from", label: "From (model mm)", default: "20" },
    Input::Expr { key: "to", label: "To (model mm)", default: "60" },
    Input::Expr { key: "gap", label: "Gap (sheet mm)", default: "6" },
  ],
  picks_model: false,
};

const TEXT_FONTS: &[(&str, &str)] = &[("sans", "Sans (Ubuntu Light)"), ("mono", "Mono (Hack)")];

pub static SK_TEXT_EDIT: Form = Form {
  title: "EDIT TEXT",
  name: "Edit Text",
  hint: "Change a sketch text (select one of its letters); its outlines regenerate",
  icon: Icon::SkText,
  cmd: "sketch.text_edit.form",
  fixed: &[],
  inputs: &[
    Input::Curves { key: "curves", label: "Letter of the text" },
    Input::Text { key: "text", label: "Text", default: "" },
    Input::Expr { key: "height", label: "Height", default: "" },
    Input::Expr { key: "angle", label: "Angle (deg)", default: "" },
    Input::Choice { key: "font", label: "Font", options: TEXT_FONTS },
    Input::Check { key: "bold", label: "Bold" },
    Input::Check { key: "italic", label: "Italic" },
    Input::Check { key: "underline", label: "Underline" },
    Input::Choice { key: "halign", label: "Horizontal alignment", options: TEXT_HALIGN },
    Input::Choice { key: "valign", label: "Vertical alignment", options: TEXT_VALIGN },
    Input::Expr { key: "spacing", label: "Character spacing (%)", default: "" },
  ],
  picks_model: false,
};

pub static CAM_ADAPTIVE: Form = Form {
  title: "2D ADAPTIVE CLEARING",
  name: "2D Adaptive Clearing",
  hint: "Clear pockets with a light, constant engagement, spiralling out from a helix",
  icon: Icon::Shell,
  cmd: "cam.adaptive",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "floors", label: "Pocket floors" },
    CAM_TOOL,
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "stepover", label: "Optimal load (of the tool)", default: "0.1" },
    Input::Expr { key: "stepdown", label: "Maximum step-down", default: "10 mm" },
    Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0" },
    Input::Choice { key: "entry", label: "Entry", options: &[("helix", "Helix"), ("plunge", "Plunge")] },
    Input::Expr { key: "ramp_angle", label: "Helix angle (°)", default: "5" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1500" },
    Input::Expr { key: "plunge", label: "Plunge (mm/min)", default: "300" },
  ],
  picks_model: false,
};

pub static LOFTED_FLANGE: Form = Form {
  title: "LOFTED FLANGE",
  name: "Lofted Flange",
  hint: "Sheet metal between two profiles (pick a profile in each sketch); closed profiles get a seam",
  icon: Icon::LoftedFlange,
  cmd: "sheet.lofted_flange",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "from", label: "First profile" },
    Input::Sketch { key: "to", label: "Second profile" },
    Input::Choice {
      key: "method",
      label: "Method",
      options: &[("die_formed", "Die formed"), ("press_brake", "Press brake")],
    },
    Input::Expr { key: "bends", label: "Bends (press brake)", default: "8" },
    Input::Check { key: "flip", label: "Flip side" },
    Input::Check { key: "rounded", label: "Rounded bends (press brake)" },
  ],
  picks_model: false,
};

const CAM_3D_TOOLS: &[(&str, &str)] = &[("ball", "Ball end mill"), ("flat", "Flat end mill")];

pub static CAM_PARALLEL: Form = Form {
  title: "PARALLEL",
  name: "Parallel",
  hint: "3D finishing in parallel rows over the part's surface",
  icon: Icon::Zebra,
  cmd: "cam.parallel",
  fixed: &[],
  inputs: &[
    Input::FacesOpt { key: "faces", label: "Faces (default all)" },
    CAM_TOOL,
    Input::Choice { key: "tool_kind", label: "Tool", options: CAM_3D_TOOLS },
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "stepover", label: "Step-over (of the tool)", default: "0.1" },
    Input::Expr { key: "angle", label: "Pass angle (°)", default: "0" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1500" },
  ],
  picks_model: false,
};

pub static DRAWING_ORDINATE: Form = Form {
  title: "ORDINATE DIMENSION",
  name: "Ordinate Dimension",
  hint: "Distances of points from an origin along the view's horizontal or vertical",
  icon: Icon::OrdinateDim,
  cmd: "drawing.ordinate",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Vec3 { key: "origin", label: "Origin (model)", default: ["0", "0", "0"] },
    Input::Text { key: "points", label: "Points (x y z; …)", default: "" },
    Input::Choice {
      key: "orientation",
      label: "Direction",
      options: &[("horizontal", "Horizontal"), ("vertical", "Vertical")],
    },
    Input::Expr { key: "offset", label: "Offset (sheet mm)", default: "10" },
  ],
  picks_model: false,
};

pub static DRAWING_ANGLE: Form = Form {
  title: "ANGULAR DIMENSION",
  name: "Angular Dimension",
  hint: "The angle at a vertex between the edges to two points",
  icon: Icon::AngleDim,
  cmd: "drawing.angle",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Vec3 { key: "vertex", label: "Vertex (model)", default: ["0", "0", "0"] },
    Input::Vec3 { key: "a", label: "Point on edge 1", default: ["10", "0", "0"] },
    Input::Vec3 { key: "b", label: "Point on edge 2", default: ["0", "0", "10"] },
    Input::Expr { key: "radius", label: "Arc radius (sheet mm)", default: "15" },
  ],
  picks_model: false,
};

const CHARACTERISTICS: &[(&str, &str)] = &[
  ("position", "Position"),
  ("flatness", "Flatness"),
  ("straightness", "Straightness"),
  ("circularity", "Circularity"),
  ("cylindricity", "Cylindricity"),
  ("profile_of_line", "Profile of a line"),
  ("profile_of_surface", "Profile of a surface"),
  ("angularity", "Angularity"),
  ("perpendicularity", "Perpendicularity"),
  ("parallelism", "Parallelism"),
  ("concentricity", "Concentricity (ISO)"),
  ("symmetry", "Symmetry (ISO)"),
  ("circular_runout", "Circular runout"),
  ("total_runout", "Total runout"),
];

pub static DRAWING_FCF: Form = Form {
  title: "FEATURE CONTROL FRAME",
  name: "Feature Control Frame",
  hint: "A GD&T tolerance on a feature, per the drawing's standard (ASME or ISO)",
  icon: Icon::FeatureControl,
  cmd: "drawing.feature_control",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Vec3 { key: "point", label: "Feature point (model)", default: ["0", "0", "0"] },
    Input::Choice { key: "characteristic", label: "Characteristic", options: CHARACTERISTICS },
    Input::Expr { key: "tolerance", label: "Tolerance", default: "0.1" },
    Input::Check { key: "diameter", label: "Diameter zone (Ø)" },
    Input::Choice {
      key: "modifier",
      label: "Material condition",
      options: &[("none", "None"), ("mmc", "MMC Ⓜ"), ("lmc", "LMC Ⓛ")],
    },
    Input::Text { key: "datums", label: "Datums", default: "A" },
    Input::Expr { key: "offset_x", label: "Frame offset X (sheet mm)", default: "12" },
    Input::Expr { key: "offset_y", label: "Frame offset Y (sheet mm)", default: "12" },
  ],
  picks_model: false,
};

pub static DRAWING_DATUM: Form = Form {
  title: "DATUM IDENTIFIER",
  name: "Datum Identifier",
  hint: "Mark a feature as a datum (the next free letter when left empty)",
  icon: Icon::DatumId,
  cmd: "drawing.datum",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Vec3 { key: "point", label: "Feature point (model)", default: ["0", "0", "0"] },
    Input::Text { key: "label", label: "Letter", default: "" },
    Input::Expr { key: "offset_x", label: "Offset X (sheet mm)", default: "0" },
    Input::Expr { key: "offset_y", label: "Offset Y (sheet mm)", default: "12" },
  ],
  picks_model: false,
};

pub static DRAWING_FINISH: Form = Form {
  title: "SURFACE TEXTURE",
  name: "Surface Texture",
  hint: "A surface finish symbol (Ra in µm), per the drawing's standard",
  icon: Icon::SurfaceTexture,
  cmd: "drawing.surface_finish",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Vec3 { key: "point", label: "Surface point (model)", default: ["0", "0", "0"] },
    Input::Expr { key: "roughness", label: "Roughness Ra (µm)", default: "1.6" },
    Input::Choice {
      key: "process",
      label: "Process",
      options: &[("any", "Any"), ("removal", "Material removal required"), ("no_removal", "No material removal")],
    },
    Input::Expr { key: "offset_x", label: "Offset X (sheet mm)", default: "0" },
    Input::Expr { key: "offset_y", label: "Offset Y (sheet mm)", default: "0" },
  ],
  picks_model: false,
};

pub static DRAWING_BALLOON: Form = Form {
  title: "BALLOON",
  name: "Balloon",
  hint: "A parts list item number on a component; it follows the parts list",
  icon: Icon::Balloon,
  cmd: "drawing.balloon",
  fixed: &[],
  inputs: &[
    Input::DrawingView { key: "view", label: "View" },
    Input::Component { key: "component", label: "Component" },
    Input::Expr { key: "offset_x", label: "Offset X (sheet mm)", default: "12" },
    Input::Expr { key: "offset_y", label: "Offset Y (sheet mm)", default: "12" },
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
    Input::Expr { key: "horizontal", label: "Horizontal distance", default: "0 mm" },
    Input::Expr { key: "vertical", label: "Vertical distance", default: "0 mm" },
    Input::Expr { key: "rotation", label: "Rotation", default: "0 deg" },
  ],
  picks_model: true,
};

pub static EXTEND: Form = Form {
  title: "EXTEND",
  name: "Extend",
  hint: "Extend open surface edges by a distance",
  icon: Icon::Extend,
  cmd: "surface.extend",
  fixed: &[],
  inputs: &[
    Input::Edges { key: "edges", label: "Edges" },
    Input::Expr { key: "distance", label: "Distance", default: "5 mm" },
    Input::Choice {
      key: "mode",
      label: "Extend type",
      options: &[("natural", "Natural"), ("tangent", "Tangent"), ("perpendicular", "Perpendicular")],
    },
  ],
  picks_model: true,
};

pub static UNTRIM: Form = Form {
  title: "UNTRIM",
  name: "Untrim",
  hint: "Remove holes from faces or restore their untrimmed boundary",
  icon: Icon::Untrim,
  cmd: "surface.untrim",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Faces" },
    Input::Choice {
      key: "loops",
      label: "Loop type",
      options: &[("all", "All loops"), ("inner", "Internal loops"), ("outer", "External loops")],
    },
  ],
  picks_model: true,
};

pub static TRIM_BODY: Form = Form {
  title: "TRIM",
  name: "Trim with Body",
  hint: "Trim surface bodies with a solid or another surface",
  icon: Icon::Trim,
  cmd: "surface.trim.body",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Surfaces", required: true },
    Input::Face { key: "tool", label: "Trimming tool (one of its faces)", near: None },
    Input::Check { key: "flip", label: "Keep the other side" },
  ],
  picks_model: true,
};

pub static TRIM_SKETCH: Form = Form {
  title: "TRIM",
  name: "Trim with Sketch",
  hint: "Trim surface bodies with sketch curves projected onto them",
  icon: Icon::Trim,
  cmd: "surface.trim.sketch",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Surfaces", required: true },
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Curves { key: "curves", label: "Curves (default the profiles)" },
    Input::Check { key: "flip", label: "Keep the other side" },
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

pub static VARIABLE_OFFSET: Form = Form {
  title: "OFFSET",
  name: "Variable Offset",
  hint: "Offset surface bodies, with picked faces at a distance of their own",
  icon: Icon::SurfOffset,
  cmd: "surface.offset.variable",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Surfaces", required: true },
    Input::Expr { key: "distance", label: "Distance", default: "2 mm" },
    Input::FacesOpt { key: "faces", label: "Faces at their own distance" },
    Input::Expr { key: "distance2", label: "Their distance", default: "4 mm" },
  ],
  picks_model: true,
};

pub static MITER_FLANGE: Form = Form {
  title: "MITER FLANGE",
  name: "Miter Flange",
  hint: "Sweep an open profile, drawn across the first edge and starting on it, along sheet edges",
  icon: Icon::Flange,
  cmd: "sheet.miter_flange",
  fixed: &[],
  inputs: &[
    Input::Edges { key: "edges", label: "Edges" },
    Input::Sketch { key: "sketch", label: "Profile sketch" },
    Input::Curves { key: "curves", label: "Profile curves (default all)" },
    Input::Choice { key: "corners", label: "Corners", options: CORNERS },
  ],
  picks_model: true,
};

pub static CORNER_CLOSURE: Form = Form {
  title: "CORNER CLOSURE",
  name: "Corner Closure",
  hint: "How the last edge or miter flange closes its corners, and the rule's corner relief",
  icon: Icon::CornerClosure,
  cmd: "sheet.corner_closure",
  fixed: &[],
  inputs: &[
    Input::Choice { key: "corners", label: "Corners", options: CORNERS },
    Input::Choice {
      key: "relief",
      label: "Corner relief",
      options: &[("none", "None"), ("round", "Round"), ("square", "Square"), ("tear", "Tear")],
    },
    Input::Expr { key: "relief_size", label: "Relief size (empty = default)", default: "" },
  ],
  picks_model: false,
};

pub static FLAT_SKETCH: Form = Form {
  title: "FLAT PATTERN SKETCH",
  name: "Flat Pattern Sketch",
  hint: "The flat pattern as an editable sketch: outline and bend lines",
  icon: Icon::FlatPattern,
  cmd: "sheet.flat_sketch",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "body", label: "Body (default the sheet metal body)", required: false },
    Input::Choice {
      key: "on",
      label: "Place on",
      options: &[("face", "Stationary face"), ("xy", "XY plane at the origin")],
    },
  ],
  picks_model: false,
};

const CORNERS: &[(&str, &str)] =
  &[("miter", "Miter"), ("open", "Open"), ("closed", "Closed"), ("overlap", "Overlap"), ("butt", "Butt")];

pub static CAM_SCALLOP: Form = Form {
  title: "SCALLOP",
  name: "Scallop",
  hint: "3D finishing in rings a constant distance apart over the surface",
  icon: Icon::Isocurve,
  cmd: "cam.scallop",
  fixed: &[],
  inputs: &[
    Input::FacesOpt { key: "faces", label: "Faces (default all)" },
    CAM_TOOL,
    Input::Choice { key: "tool_kind", label: "Tool", options: CAM_3D_TOOLS },
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "stepover", label: "Step-over (of the tool)", default: "0.1" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1500" },
  ],
  picks_model: false,
};

pub static SK_TEXT_EXPLODE: Form = Form {
  title: "EXPLODE TEXT",
  name: "Explode Text",
  hint: "Turn a sketch text into plain lines and splines",
  icon: Icon::SkText,
  cmd: "sketch.text_explode",
  fixed: &[],
  inputs: &[Input::Curves { key: "curves", label: "Letter of the text" }],
  picks_model: false,
};

/// Fills a form from what it edits when it opens (Edit Text: the selected text's values).
pub fn prefill(form: &Form, state: &mut State, s: &Session) {
  if !std::ptr::eq(form, &SK_TEXT_EDIT) {
    return;
  }
  let Some(sk) = s.edit_sketch else { return };
  let Ok(sketch) = s.design.sketch(sk) else { return };
  let picked = s.selection.iter().find_map(|x| match x {
    Sel::SketchCurve { curve, .. } => sketch.text_of(*curve),
    _ => None,
  });
  let Some(t) = picked.or(sketch.texts.first().map(|t| t.id)).and_then(|id| sketch.text(id)) else { return };
  for (i, input) in form.inputs.iter().enumerate() {
    let v = match input.key() {
      "text" => json!(t.text),
      "height" => json!(fox_engine::sketch::Dim::new(t.height).expr),
      "angle" => json!(fox_engine::sketch::Dim::new(t.angle.to_degrees()).expr),
      "font" => json!(t.font),
      "bold" => json!(t.style.bold),
      "italic" => json!(t.style.italic),
      "underline" => json!(t.style.underline),
      "halign" => serde_json::to_value(t.style.halign).unwrap_or(json!("left")),
      "valign" => serde_json::to_value(t.style.valign).unwrap_or(json!("baseline")),
      "spacing" => json!(fox_engine::sketch::Dim::new(t.style.spacing).expr),
      _ => continue,
    };
    if let Some(slot) = state.values.get_mut(i) {
      *slot = v;
    }
  }
}

pub static CAM_ADAPTIVE3D: Form = Form {
  title: "ADAPTIVE CLEARING",
  name: "Adaptive Clearing",
  hint: "3D roughing of the stock around the part, level by level, with a light, constant engagement",
  icon: Icon::Shell,
  cmd: "cam.adaptive3d",
  fixed: &[],
  inputs: &[
    CAM_TOOL,
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "8 mm" },
    Input::Expr { key: "stepover", label: "Optimal load (of the tool)", default: "0.15" },
    Input::Expr { key: "stepdown", label: "Maximum step-down", default: "3 mm" },
    Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0.3 mm" },
    Input::Choice { key: "entry", label: "Entry", options: &[("helix", "Helix"), ("plunge", "Plunge")] },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1500" },
    Input::Expr { key: "plunge", label: "Plunge (mm/min)", default: "300" },
  ],
  picks_model: false,
};

pub static CAM_PENCIL: Form = Form {
  title: "PENCIL",
  name: "Pencil",
  hint: "3D finishing along concave corners and fillets tighter than the tool",
  icon: Icon::Line,
  cmd: "cam.pencil",
  fixed: &[],
  inputs: &[
    Input::FacesOpt { key: "faces", label: "Faces (default all)" },
    CAM_TOOL,
    Input::Choice { key: "tool_kind", label: "Tool", options: CAM_3D_TOOLS },
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "4 mm" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1200" },
  ],
  picks_model: false,
};

pub static CAM_STEEP_SHALLOW: Form = Form {
  title: "STEEP AND SHALLOW",
  name: "Steep and Shallow",
  hint: "3D finishing: waterlines on steep areas, parallel rows on shallow ones",
  icon: Icon::Zebra,
  cmd: "cam.steep_shallow",
  fixed: &[],
  inputs: &[
    Input::FacesOpt { key: "faces", label: "Faces (default all)" },
    CAM_TOOL,
    Input::Choice { key: "tool_kind", label: "Tool", options: CAM_3D_TOOLS },
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "threshold", label: "Threshold angle (°)", default: "45" },
    Input::Expr { key: "stepover", label: "Step-over (of the tool)", default: "0.1" },
    Input::Expr { key: "stepdown", label: "Step-down", default: "0.5 mm" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1500" },
  ],
  picks_model: false,
};

pub static CAM_CONTOUR3D: Form = Form {
  title: "CONTOUR",
  name: "Contour",
  hint: "3D finishing in waterline passes at constant heights on steep areas",
  icon: Icon::SkOffset,
  cmd: "cam.contour3d",
  fixed: &[],
  inputs: &[
    Input::FacesOpt { key: "faces", label: "Faces (default all)" },
    CAM_TOOL,
    Input::Choice { key: "tool_kind", label: "Tool", options: CAM_3D_TOOLS },
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "min_slope", label: "Minimum slope (°)", default: "30" },
    Input::Expr { key: "stepdown", label: "Step-down", default: "0.5 mm" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1500" },
  ],
  picks_model: false,
};

pub static SK_INCLUDE: Form = Form {
  title: "INCLUDE 3D GEOMETRY",
  name: "Include 3D Geometry",
  hint: "Bring edges into the sketch at their real 3D positions (sweep paths, rails); they follow the model",
  icon: Icon::Include3D,
  cmd: "sketch.include",
  fixed: &[],
  inputs: &[Input::Edges { key: "edges", label: "Edges" }],
  picks_model: true,
};

pub static FORM_BOX: Form = Form {
  title: "CREATE FORM",
  name: "Create Form",
  hint: "A T-spline form body: the smooth surface inside a box control cage; pull its vertices with form.move_vertex",
  icon: Icon::CreateForm,
  cmd: "form.box",
  fixed: &[],
  inputs: &[
    Input::Plane { key: "plane", label: "Plane" },
    Input::Expr { key: "length", label: "Length", default: "40 mm" },
    Input::Expr { key: "width", label: "Width", default: "40 mm" },
    Input::Expr { key: "height", label: "Height", default: "40 mm" },
    Input::Expr { key: "length_faces", label: "Length faces", default: "2" },
    Input::Expr { key: "width_faces", label: "Width faces", default: "2" },
    Input::Expr { key: "height_faces", label: "Height faces", default: "2" },
    Input::Choice { key: "operation", label: "Operation", options: OPS },
  ],
  picks_model: false,
};

pub static PATH_PATTERN: Form = Form {
  title: "PATTERN ON PATH",
  name: "Pattern on Path",
  hint: "Copies of bodies spaced by arc length along a sketch path",
  icon: Icon::Pattern,
  cmd: "pattern.path",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies", required: true },
    Input::Sketch { key: "path", label: "Path sketch" },
    Input::Curves { key: "path_curves", label: "Path curves (default all)" },
    Input::Expr { key: "count", label: "Quantity", default: "3" },
    Input::Choice {
      key: "distance_type",
      label: "Distance type",
      options: &[("spacing", "Spacing"), ("extent", "Extent")],
    },
    Input::Expr { key: "distance", label: "Distance", default: "10 mm" },
    Input::Expr { key: "start", label: "Start point (0 to 1)", default: "0" },
    Input::Check { key: "flip", label: "Flip" },
    Input::Check { key: "symmetric", label: "Symmetric" },
    Input::Choice {
      key: "orientation",
      label: "Orientation",
      options: &[("identical", "Identical"), ("along_path", "Path direction")],
    },
    Input::Check { key: "join", label: "Join to the original" },
  ],
  picks_model: true,
};

pub static SK_SCALE: Form = Form {
  title: "SKETCH SCALE",
  name: "Sketch Scale",
  hint: "Scale sketch curves about a point (sketch X, Y); dimensions inside them scale too",
  icon: Icon::SkScale,
  cmd: "sketch.scale",
  fixed: &[],
  inputs: &[
    Input::Curves { key: "curves", label: "Entities (default all)" },
    Input::Expr { key: "x", label: "Point X", default: "0" },
    Input::Expr { key: "y", label: "Point Y", default: "0" },
    Input::Expr { key: "scale", label: "Scale factor", default: "1" },
  ],
  picks_model: false,
};

const PROJECTION_TYPES: &[(&str, &str)] = &[("closest_point", "Closest Point"), ("along_vector", "Along Vector")];

pub static SK_PROJECT_SURFACE: Form = Form {
  title: "PROJECT TO SURFACE",
  name: "Project To Surface",
  hint: "Drape sketch curves onto faces: each point to the closest point, or pushed along a direction",
  icon: Icon::ProjectSurface,
  cmd: "sketch.project_to_surface",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Faces" },
    Input::Curves { key: "curves", label: "Curves (default all)" },
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Choice { key: "type", label: "Projection type", options: PROJECTION_TYPES },
    Input::Axis { key: "direction", label: "Direction (Along Vector)" },
  ],
  picks_model: true,
};

pub static SK_INTERSECTION_CURVE: Form = Form {
  title: "INTERSECTION CURVE",
  name: "Intersection Curve",
  hint: "3D curves where sketch curves, pushed along their sketch's normal, cross faces",
  icon: Icon::Intersect,
  cmd: "sketch.intersection_curve",
  fixed: &[],
  inputs: &[
    Input::Curves { key: "curves", label: "Sketch curves (default all)" },
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Faces { key: "faces", label: "Faces" },
  ],
  picks_model: true,
};

const ALIGN_RELATIONS: &[(&str, &str)] =
  &[("auto", "Automatic"), ("flush", "Flush"), ("coincident", "Coincident"), ("concentric", "Concentric")];

pub static ALIGN: Form = Form {
  title: "ALIGN",
  name: "Align",
  hint: "Move a component so its face lines up with another (on another component, or a root body)",
  icon: Icon::Align,
  cmd: "align.form",
  fixed: &[],
  inputs: &[
    Input::Face { key: "moving", label: "Objects to move", near: None },
    Input::Face { key: "fixed", label: "Align to", near: None },
    Input::Choice { key: "relation", label: "Relation", options: ALIGN_RELATIONS },
    Input::Expr { key: "offset", label: "Offset", default: "0 mm" },
    Input::Check { key: "flip", label: "Flip" },
  ],
  picks_model: true,
};
