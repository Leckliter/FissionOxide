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
  /// Pieces of the trimmed surfaces to remove, clicked in the viewport while picking them is
  /// switched on: points on them (left out when none: the tool's side decides).
  Pieces {
    key: &'static str,
    label: &'static str,
  },
  /// A choice shown as a row of icon toggles (Fusion's Type and Selection Filter rows): key,
  /// icon and name (the tooltip) of each; the first is the default.
  Icons {
    key: &'static str,
    label: &'static str,
    options: &'static [(&'static str, Icon, &'static str)],
  },
  /// A check box that starts ticked (Projection Link).
  CheckOn {
    key: &'static str,
    label: &'static str,
  },
  /// A yes/no drawn as Fusion's Flip icon button, lit while flipped (Rib's Flip Direction).
  Flip {
    key: &'static str,
    label: &'static str,
  },
  /// Selected construction planes (left out when none); counted on the form's `Bodies` chip,
  /// as Boundary Fill's Select Tools takes bodies and planes alike.
  Planes {
    key: &'static str,
  },
  /// A selection row that takes picks only while it is the active row (Fusion's blue chip); a
  /// form with one routes every pick to its active row (see [`State::sync_rows`]). `fits` says
  /// which picks it takes, `multi` whether it holds more than one (a single row hands the active
  /// row on to the next once picked), `prompt` is Fusion's cursor prompt while it is empty.
  Pick {
    key: &'static str,
    label: &'static str,
    fits: Fits,
    multi: bool,
    prompt: &'static str,
  },
}

/// What a [`Input::Pick`] row takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fits {
  /// Faces, as `{body, face}`.
  Faces,
  /// A planar face or a construction plane (`{body, face}` or `{feature}`), or an origin plane.
  Plane,
  /// A plane (as [`Fits::Plane`]), a straight edge (`{body, edge}`), a construction axis
  /// (`{axis}`) or an origin axis: a direction.
  Direction,
  /// Whole bodies, by id (the dialog picks bodies while the row is active).
  Body,
  /// A face, flat or curved, or a plane (as [`Fits::Plane`]): a surface to replace faces with.
  Surface,
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
      | Input::Cells { key, .. }
      | Input::Pieces { key, .. }
      | Input::Icons { key, .. }
      | Input::CheckOn { key, .. }
      | Input::Flip { key, .. }
      | Input::Planes { key }
      | Input::Pick { key, .. } => key,
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
  /// Model points picked on the drawing sheet so far (see [`State::sheet_pick`]).
  picks: usize,
  /// Viewport clicks on the trimmed surfaces pick pieces to remove.
  pub picking_pieces: bool,
  /// Text: the frame's first corner has been clicked (in `x`, `y`); the second is awaited.
  pub text_corner: bool,
  /// Editing a feature from the timeline: its picks stay as they are, so the rows that follow
  /// the picks show.
  pub edit: bool,
  /// The [`Input::Pick`] row the next picks go to (an index into the form's inputs).
  pub row: usize,
}

impl State {
  pub fn new(form: &Form) -> State {
    let values = form
      .inputs
      .iter()
      .map(|i| match i {
        Input::Expr { default, .. } | Input::Text { default, .. } => json!(default),
        Input::Check { .. } | Input::Flip { .. } => json!(false),
        Input::CheckOn { .. } => json!(true),
        Input::Pick { multi: true, .. } => json!([]),
        Input::Choice { options, .. } => json!(options.first().map(|o| o.0).unwrap_or("")),
        // (Pattern on Path is the pattern dialog's last Type)
        Input::Icons { key: "pattern_type", .. } => json!("path"),
        // (Lofted Flange is the Flange dialog's last Type)
        Input::Icons { key: "flange_type", .. } => json!("lofted"),
        Input::Icons { options, .. } => json!(options.first().map(|o| o.0).unwrap_or("")),
        Input::Vec3 { default, .. } => json!(default),
        Input::Faces { .. }
        | Input::FacesOpt { .. }
        | Input::Edges { .. }
        | Input::Bodies { .. }
        | Input::Curves { .. }
        | Input::Profiles { .. }
        | Input::Cells { .. }
        | Input::Pieces { .. }
        | Input::Planes { .. } => {
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
      picks: 0,
      picking_pieces: false,
      text_corner: false,
      edit: false,
      row: form.inputs.iter().position(|i| matches!(i, Input::Pick { .. })).unwrap_or(0),
    }
  }

  /// A model point picked on the drawing sheet: the view input takes its view, then the point
  /// inputs fill in turn, and a list of points grows after them. A pick in another view starts
  /// over.
  pub fn sheet_pick(&mut self, form: &Form, view: u32, point: [f64; 3]) {
    if let Some(vi) = form.inputs.iter().position(|x| matches!(x, Input::DrawingView { .. }))
      && self.values[vi].as_u64() != Some(view as u64)
    {
      self.values[vi] = json!(view);
      self.picks = 0;
    }
    let xyz = point.map(short);
    let slots: Vec<usize> =
      (0..form.inputs.len()).filter(|&i| matches!(form.inputs[i], Input::Vec3 { .. }) && self.shown(form, i)).collect();
    let list = form.inputs.iter().position(|x| matches!(x, Input::Text { key: "points", .. }));
    let n = slots.len();
    match list {
      Some(li) if self.picks >= n => {
        let text = xyz.join(" ");
        let old = self.values[li].as_str().unwrap_or("").trim().to_string();
        self.values[li] = json!(if self.picks == n || old.is_empty() { text } else { format!("{old}; {text}") });
      }
      _ if n > 0 => self.values[slots[self.picks % n]] = json!(xyz),
      _ => return,
    }
    self.picks += 1;
  }

  /// Refills the pick inputs from the selection. Returns whether anything changed.
  pub fn sync(&mut self, form: &Form, s: &Session, last_sketch: Option<FeatureId>) -> bool {
    if routed(form) {
      return self.sync_rows(form, s);
    }
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
    // construction planes alone (Boundary Fill's tools)
    let construction: Vec<Value> = s
      .selection
      .iter()
      .filter_map(|x| match x {
        Sel::Feature { feature } => match s.design.feature(*feature).map(|f| &f.kind) {
          Some(FeatureKind::ConstructionPlane { .. } | FeatureKind::PlaneBy { .. }) => {
            Some(json!({"feature": feature.0}))
          }
          _ => None,
        },
        _ => None,
      })
      .collect();
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
        Input::Planes { .. } => json!(construction),
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
        // the coil's plane is the one its circle was drawn on, whatever is picked later
        Input::Plane { .. } if form.cmd == "coil.form" => self.chosen[i].clone().unwrap_or(json!("xy")),
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
    match form.cmd {
      "thread" => crate::thread_form::sync(form, self, &before, s),
      // Emboss wraps the profiles around a curved face, as Fusion does: no row for it
      "emboss" => {
        let face = form.inputs.iter().position(|x| x.key() == "face").map(|j| self.values[j].clone());
        let curved =
          face.is_some_and(|f| face_kind(s, &f).is_some_and(|k| k != fox_engine::kernel::SurfaceKind::Plane));
        if let Some(j) = form.inputs.iter().position(|x| x.key() == "wrap") {
          self.values[j] = json!(curved);
        }
      }
      _ if crate::mesh_ui::is_mesh_form(form) => crate::mesh_ui::sync(form, self, &before, s),
      _ => {}
    }
    self.values != before
  }

  /// Pipe with Chain Selection, as Fusion's: the picked path curves and every curve of their
  /// sketch joined end to end to them.
  pub fn chain_path(&mut self, form: &Form, s: &Session) {
    let at = |k: &str| form.inputs.iter().position(|x| x.key() == k);
    let (Some(ci), Some(si), Some(chi)) = (at("path_curves"), at("path"), at("chain")) else { return };
    if form.cmd != "pipe.form" || self.values[chi] != json!(true) {
      return;
    }
    let Some(sk) = self.values[si].as_u64() else { return };
    let Ok(sketch) = s.design.sketch(FeatureId(sk as u32)) else { return };
    let mut chain: Vec<u32> = self.values[ci]
      .as_array()
      .map(|a| a.iter().filter_map(Value::as_u64).map(|c| c as u32).collect())
      .unwrap_or_default();
    if chain.is_empty() {
      return;
    }
    let ends = |c: fox_engine::sketch::CurveId| {
      let segs = sketch.curve_segs(c);
      Some([segs.first()?.start(), segs.last()?.end()])
    };
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6;
    loop {
      let reached: Vec<[f64; 2]> =
        chain.iter().filter_map(|c| ends(fox_engine::sketch::CurveId(*c))).flatten().collect();
      let next = sketch.curves.iter().find(|c| {
        !c.reference()
          && !chain.contains(&c.id.0)
          && ends(c.id).is_some_and(|e| e.iter().any(|p| reached.iter().any(|q| near(*p, *q))))
      });
      match next {
        Some(c) => chain.push(c.id.0),
        None => break,
      }
    }
    self.values[ci] = json!(chain);
  }

  /// Have the form's model or sketch picks been made (or is a feature being edited)? Rows that
  /// Fusion adds after the picks wait for them.
  fn picked(&self, form: &Form, keys: &[&str]) -> bool {
    self.edit
      || keys.iter().all(|k| {
        form
          .inputs
          .iter()
          .position(|x| x.key() == *k)
          .and_then(|j| self.values.get(j))
          .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
      })
  }

  /// A routed form's picks (see [`routed`]): each row keeps the picks still selected, and what
  /// was picked since goes to the active row when it fits there (a face picked for a body row
  /// names its body). A single row takes the newest pick and hands the active row on to the
  /// next row, as Fusion's selection rows do.
  pub fn sync_rows(&mut self, form: &Form, s: &Session) -> bool {
    let before = self.values.clone();
    // (an origin plane or axis is picked outside the model's selection: it stays)
    let selected = |v: &Value| pick_sel(v).is_none_or(|sel| s.selection.contains(&sel));
    for (input, v) in form.inputs.iter().zip(self.values.iter_mut()) {
      let Input::Pick { multi, .. } = input else { continue };
      if *multi {
        if let Some(a) = v.as_array_mut() {
          a.retain(|x| selected(x));
        }
      } else if !v.is_null() && !selected(v) {
        *v = Value::Null;
      }
    }
    let row = self.row;
    let Some(&Input::Pick { fits, multi, .. }) = form.inputs.get(row) else { return self.values != before };
    let held = self.selection(form);
    for sel in s.selection.iter().filter(|x| !held.contains(x)) {
      let Some(v) = fit_value(fits, sel, s) else { continue };
      if !multi {
        self.values[row] = v;
      } else if let Some(a) = self.values[row].as_array_mut()
        && !a.contains(&v)
      {
        a.push(v);
      }
    }
    if !multi && !self.values[row].is_null() && self.values[row] != before[row] {
      self.row = (row + 1..form.inputs.len()).find(|&j| matches!(form.inputs[j], Input::Pick { .. })).unwrap_or(row);
    }
    self.values != before
  }

  /// What a click on the model picks while the form is open: whole bodies for a body row,
  /// faces and edges otherwise (None: the form leaves the priority alone).
  pub fn row_priority(&self, form: &Form) -> Option<crate::select::Priority> {
    match form.inputs.get(self.row) {
      Some(Input::Pick { fits: Fits::Body, .. }) => Some(crate::select::Priority::Body),
      Some(Input::Pick { .. }) => Some(crate::select::Priority::Face),
      _ => None,
    }
  }

  /// Fusion's cursor prompt for the active [`Input::Pick`] row.
  pub fn row_prompt(&self, form: &Form) -> Option<&'static str> {
    match form.inputs.get(self.row) {
      Some(Input::Pick { prompt, .. }) => Some(prompt),
      _ => None,
    }
  }

  /// An origin plane or axis picked in the canvas: the active row takes it if it fits.
  pub fn origin_pick(&mut self, form: &Form, plane: Option<&'static str>, axis: Option<&'static str>) -> bool {
    let row = self.row;
    let v = match form.inputs.get(row) {
      Some(Input::Pick { fits: Fits::Plane | Fits::Surface, .. }) => plane,
      Some(Input::Pick { fits: Fits::Direction, .. }) => plane.or(axis),
      _ => None,
    };
    let Some(v) = v.map(|v| json!(v)).filter(|v| *v != self.values[row]) else { return false };
    self.values[row] = v;
    self.row = (row + 1..form.inputs.len()).find(|&j| matches!(form.inputs[j], Input::Pick { .. })).unwrap_or(row);
    true
  }

  /// Does input `i` count? Some only do when another is set: a composite frame's lower tier,
  /// a weld's finish letter (with its contour), a both-sides weld's other side, a datum target
  /// line's end and an area target's diameter; Text's fields once its frame is placed (or its
  /// path picked), Project To Surface's direction with Along Vector.
  pub fn shown(&self, form: &Form, i: usize) -> bool {
    let get = |k: &str| form.inputs.iter().position(|x| x.key() == k).and_then(|j| self.values.get(j));
    let set = |k: &str| get(k).and_then(Value::as_str).is_some_and(|c| !c.is_empty() && c != "none");
    let both = || get("side").and_then(Value::as_str) == Some("both");
    let key = form.inputs[i].key();
    match form.cmd {
      "sketch.text.form" => {
        let on_path = get("type").and_then(Value::as_str) == Some("path");
        let picked = get("path").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
        match key {
          "type" => true,
          "path" => on_path,
          "below" | "align" => on_path && picked,
          "angle" | "placement" | "x" | "y" | "x2" | "y2" => !on_path && self.text_framed(form),
          _ if on_path => picked,
          _ => self.text_framed(form),
        }
      }
      // Fusion's Align shows Object, From and To until both are picked
      "align.form" if matches!(key, "relation" | "offset" | "flip") => {
        ["moving", "fixed"].iter().all(|k| get(k).is_some_and(|v| !v.is_null()))
      }
      "sketch.project_to_surface" if key == "direction" => get("type").and_then(Value::as_str) == Some("along_vector"),
      // Coil's Type names the two values it asks for
      "coil.form" => {
        let ty = get("type").and_then(Value::as_str).unwrap_or("");
        match key {
          "revolutions" => ty != "height_pitch",
          "height" => matches!(ty, "revolution_height" | "height_pitch"),
          "pitch" => ty != "revolution_height",
          _ => true,
        }
      }
      // Pipe's Distance and Section Size rows appear with the path, Section Thickness with Hollow
      "pipe.form" => match key {
        "distance" | "distance2" | "diameter" => {
          get("path_curves").and_then(Value::as_array).is_some_and(|a| !a.is_empty())
        }
        "wall" => get("hollow").and_then(Value::as_bool).unwrap_or(false),
        _ => true,
      },
      "rib" | "web" => match key {
        "thickness" => self.picked(form, &["curves"]),
        "depth" => self.picked(form, &["curves"]) && get("extent").and_then(Value::as_str) == Some("depth"),
        _ => true,
      },
      "emboss" => match key {
        "depth" | "horizontal" | "vertical" | "rotation" => self.picked(form, &["profiles", "face"]),
        _ => true,
      },
      "thicken" if key == "thickness" => self.picked(form, &["bodies"]),
      // the thread's span: Full Length off
      "thread" if matches!(key, "length" | "offset" | "flip") => {
        get("full_length").and_then(Value::as_bool) == Some(false)
      }
      // Pattern on Path: Type, Object Type, Objects and Path until both are picked
      "pattern.path" => {
        let picked = |k: &str| get(k).and_then(Value::as_array).is_some_and(|a| !a.is_empty());
        matches!(key, "pattern_type" | "object_type" | "bodies" | "path_curves" | "path")
          || picked("bodies") && picked("path_curves")
      }
      // Corner Closure: Edges until they are picked, as in Fusion
      "sheet.corner_closure" => key == "edges" || self.picked(form, &["edges"]),
      // Joint Origin: Origin Mode and Snap until the snap is picked, as in Fusion
      "joint_origin" => matches!(key, "mode" | "face") || get("face").is_some_and(|v| !v.is_null()),
      "drawing.feature_control" if key.starts_with("lower_") => {
        get("composite").and_then(Value::as_bool).unwrap_or(false)
      }
      "drawing.datum_target" => match key {
        "end" => get("target").and_then(Value::as_str) == Some("line"),
        "area" => get("target").and_then(Value::as_str) == Some("area"),
        _ => true,
      },
      "drawing.weld" => match key {
        "finish" => set("contour"),
        "other_contour" => both(),
        "other_finish" => both() && set("other_contour"),
        _ => true,
      },
      _ => true,
    }
  }

  /// Is input `i` passed to the command but not drawn as a row? Text's frame corners (placed by
  /// a drag on the canvas) and the sketch of Project To Surface's and Intersection Curve's
  /// curves (the picked curves' own, or the active sketch).
  pub fn hidden(form: &Form, i: usize) -> bool {
    let key = form.inputs[i].key();
    match form.cmd {
      // (the angle is set by the frame's rotate handle, not a row, as in Fusion)
      "sketch.text.form" => matches!(key, "placement" | "x" | "y" | "x2" | "y2" | "angle"),
      "sketch.project_to_surface" | "sketch.intersection_curve" => key == "sketch",
      "sketch.intersect.form" => key == "faces",
      // the coil's centre is where its circle was drawn; the pipe's sketch is its path's
      "coil.form" => key == "at",
      "pipe.form" => key == "path",
      // the picked lines' and profiles' own sketch, and Emboss's wrap (the face decides it)
      "rib" | "web" => key == "sketch",
      "emboss" => matches!(key, "sketch" | "wrap"),
      "boundary_fill" => key == "planes",
      // (Fusion names joint origins itself; the browser renames them)
      "joint_origin" => key == "name",
      "pattern.path" => matches!(key, "path" | "start" | "join"),
      _ => false,
    }
  }

  /// Text: has its frame been placed (two corners dragged on the canvas)?
  pub fn text_framed(&self, form: &Form) -> bool {
    let get = |k: &str| form.inputs.iter().position(|x| x.key() == k).and_then(|j| self.values.get(j));
    get("x2").and_then(Value::as_str).is_some_and(|t| !t.trim().is_empty())
      && get("y2").and_then(Value::as_str).is_some_and(|t| !t.trim().is_empty())
  }

  /// Text: the frame placed on the canvas, from corner `a` to corner `b` (sketch coordinates),
  /// and the angle (degrees) the text reads at, so it lies in the frame as drawn.
  pub fn text_frame(&mut self, form: &Form, a: [f64; 2], b: [f64; 2], angle: f64) {
    for (input, v) in form.inputs.iter().zip(self.values.iter_mut()) {
      match input.key() {
        "placement" => *v = json!("box"),
        "x" => *v = json!(short(a[0])),
        "y" => *v = json!(short(a[1])),
        "x2" => *v = json!(short(b[0])),
        "y2" => *v = json!(short(b[1])),
        "angle" => *v = json!(short(angle)),
        _ => {}
      }
    }
    self.text_corner = false;
  }

  /// Text: the frame's first corner clicked at `a` (sketch coordinates); the prompt moves on to
  /// the second corner, which a rubber-band frame follows the pointer to.
  pub fn text_first_corner(&mut self, form: &Form, a: [f64; 2]) {
    for (input, v) in form.inputs.iter().zip(self.values.iter_mut()) {
      match input.key() {
        "placement" => *v = json!("box"),
        "x" => *v = json!(short(a[0])),
        "y" => *v = json!(short(a[1])),
        _ => {}
      }
    }
    self.text_corner = true;
  }

  /// Text: the frame's first corner while the second is awaited (sketch coordinates).
  pub fn text_corner_at(&self, form: &Form) -> Option<[f64; 2]> {
    if !self.text_corner || self.text_framed(form) {
      return None;
    }
    let (a, _) = self.text_corners(form)?;
    Some(a)
  }

  /// Text: the frame's two corners once it is placed (sketch coordinates), and the angle
  /// (degrees) the text reads at.
  pub fn text_corners(&self, form: &Form) -> Option<([f64; 2], [f64; 2])> {
    let num = |k: &str| {
      let i = form.inputs.iter().position(|x| x.key() == k)?;
      self.values.get(i)?.as_str()?.trim().parse::<f64>().ok()
    };
    Some(([num("x")?, num("y")?], [num("x2").unwrap_or(0.0), num("y2").unwrap_or(0.0)]))
  }

  /// Text: the angle (degrees) the text reads at, from the hidden angle input.
  pub fn text_angle(&self, form: &Form) -> f64 {
    form
      .inputs
      .iter()
      .position(|x| x.key() == "angle")
      .and_then(|i| self.values.get(i)?.as_str()?.trim().parse::<f64>().ok())
      .unwrap_or(0.0)
  }

  /// Selection Filter set to faces: the form's picks are faces, not whole bodies.
  pub fn filter_faces(&self, form: &Form) -> bool {
    form
      .inputs
      .iter()
      .position(|x| matches!(x, Input::Icons { key: "filter", .. }))
      .and_then(|j| self.values.get(j))
      .and_then(Value::as_str)
      == Some("faces")
  }

  /// The model picks behind the form's values, as a selection that [`State::sync`] reads back
  /// the same: put back when a preview is withdrawn, so a viewport click adds to the picks.
  pub fn selection(&self, form: &Form) -> Vec<Sel> {
    let get = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
    let mut out: Vec<Sel> = Vec::new();
    let mut bodies: Vec<u32> = Vec::new();
    let mut curves: Vec<u32> = Vec::new();
    let mut sketch: Option<FeatureId> = None;
    for (input, v) in form.inputs.iter().zip(&self.values) {
      let picks: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Null => Vec::new(),
        x => vec![x],
      };
      match input {
        Input::Faces { .. }
        | Input::FacesOpt { .. }
        | Input::Face { .. }
        | Input::FaceOther { .. }
        | Input::Edges { .. }
        | Input::Edge { .. }
        | Input::Plane { .. }
        | Input::Axis { .. } => {
          for x in picks {
            let sel = match (get(x, "body"), get(x, "face"), get(x, "edge")) {
              (Some(b), Some(f), _) => Sel::Face { body: BodyId(b), face: f },
              (Some(b), _, Some(e)) => Sel::Edge { body: BodyId(b), edge: e },
              _ => continue,
            };
            if !out.contains(&sel) {
              out.push(sel);
            }
          }
        }
        Input::Pick { .. } => {
          for x in picks {
            if let Some(sel) = pick_sel(x).filter(|sel| !out.contains(sel)) {
              out.push(sel);
            }
          }
        }
        Input::Bodies { .. } => bodies.extend(picks.iter().filter_map(|x| x.as_u64()).map(|b| b as u32)),
        Input::Curves { .. } => curves.extend(picks.iter().filter_map(|x| x.as_u64()).map(|c| c as u32)),
        Input::Sketch { .. } if sketch.is_none() => sketch = v.as_u64().map(|f| FeatureId(f as u32)),
        Input::Planes { .. } => {
          out.extend(picks.iter().filter_map(|x| get(x, "feature")).map(|f| Sel::Feature { feature: FeatureId(f) }))
        }
        _ => {}
      }
    }
    let picked = |b: u32, out: &[Sel]| {
      out.iter().any(|x| matches!(x, Sel::Face { body, .. } | Sel::Edge { body, .. } if body.0 == b))
    };
    let mut lead = Vec::new();
    for b in bodies {
      if !picked(b, &out) && !lead.contains(&Sel::Body { body: BodyId(b) }) {
        lead.push(Sel::Body { body: BodyId(b) });
      }
    }
    if let Some(sk) = sketch {
      lead.extend(curves.into_iter().map(|c| Sel::SketchCurve { sketch: sk, curve: fox_engine::sketch::CurveId(c) }));
    }
    lead.extend(out);
    lead
  }

  /// Is everything the command needs there?
  pub fn missing(&self, form: &Form) -> Option<&'static str> {
    if crate::mesh_ui::is_mesh_form(form) {
      return crate::mesh_ui::missing(form, self);
    }
    let get = |k: &str| form.inputs.iter().position(|x| x.key() == k).and_then(|j| self.values.get(j));
    if form.cmd == "pipe.form" && get("path_curves").and_then(Value::as_array).is_none_or(Vec::is_empty) {
      return Some("Select a path to sweep the profile along");
    }
    if form.cmd == "coil.form" && get("type").and_then(Value::as_str) == Some("spiral") {
      return Some("Spiral coils aren't in FissionOxide yet");
    }
    if form.cmd == "sketch.text.form" {
      let get = |k: &str| form.inputs.iter().position(|x| x.key() == k).and_then(|j| self.values.get(j));
      let on_path = get("type").and_then(Value::as_str) == Some("path");
      let picked = get("path").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
      if on_path && !picked {
        return Some("Select path");
      }
      if !on_path && !self.text_framed(form) {
        return Some(if self.text_corner {
          "Place second corner of text frame"
        } else {
          "Place first corner of text frame"
        });
      }
    }
    self.first_missing(form).map(|(_, label)| label)
  }

  /// The first input still waiting for a pick: its key and label.
  pub fn first_missing(&self, form: &Form) -> Option<(&'static str, &'static str)> {
    // Pattern on Path: the path is picked, not every curve of a sketch
    if form.cmd == "pattern.path" {
      let at = |k: &str| form.inputs.iter().position(|x| x.key() == k).and_then(|j| self.values.get(j));
      let empty = |k: &str| at(k).and_then(Value::as_array).is_none_or(Vec::is_empty);
      if empty("bodies") {
        return Some(("bodies", "Objects"));
      }
      if empty("path_curves") {
        return Some(("path_curves", "Path"));
      }
    }
    for (i, (input, v)) in form.inputs.iter().zip(&self.values).enumerate() {
      let empty = v.is_null() || v.as_array().is_some_and(Vec::is_empty);
      // a hidden sketch is the picks' own: the picks are what's missing
      if matches!(input, Input::Sketch { .. }) && State::hidden(form, i) {
        continue;
      }
      match input {
        Input::Faces { key, label }
        | Input::Edges { key, label }
        | Input::Face { key, label, .. }
        | Input::Edge { key, label }
          if empty =>
        {
          return Some((key, label));
        }
        Input::Bodies { key, label, required: true } | Input::Sketch { key, label } if empty => {
          return Some((key, label));
        }
        // Rib's and Web's Profile, Emboss's Sketch Profiles: OK waits for a pick, as in Fusion
        Input::Curves { label, key } | Input::Profiles { label, key } if empty && required(form, key) => {
          return Some((key, label));
        }
        Input::Pick { key, prompt, .. } if empty => return Some((key, prompt)),
        Input::DrawingView { key, label } | Input::Component { key, label } if empty => return Some((key, label)),
        // picking pieces: nothing to preview until one is clicked
        Input::Pieces { key, label } if self.picking_pieces && empty => return Some((key, label)),
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
      if !self.shown(form, i) {
        continue;
      }
      match input {
        Input::Curves { .. }
        | Input::FacesOpt { .. }
        | Input::Profiles { .. }
        | Input::Cells { .. }
        | Input::Planes { .. }
          if v.as_array().is_some_and(Vec::is_empty) =>
        {
          continue;
        }
        Input::Bodies { required: false, .. } if v.as_array().is_some_and(Vec::is_empty) => continue,
        // picked pieces count only while picking them is on
        Input::Pieces { .. } if !self.picking_pieces || v.as_array().is_none_or(Vec::is_empty) => continue,
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

/// Does the form route its picks by an active row (it has [`Input::Pick`] rows)?
pub fn routed(form: &Form) -> bool {
  form.inputs.iter().any(|i| matches!(i, Input::Pick { .. }))
}

/// The selection a [`Input::Pick`] value stands for (None for an origin plane or axis).
fn pick_sel(v: &Value) -> Option<Sel> {
  let get = |k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
  if let Some(b) = v.as_u64() {
    return Some(Sel::Body { body: BodyId(b as u32) });
  }
  if let Some(f) = get("feature").or(get("axis")) {
    return Some(Sel::Feature { feature: FeatureId(f) });
  }
  let body = BodyId(get("body")?);
  match (get("face"), get("edge")) {
    (Some(face), _) => Some(Sel::Face { body, face }),
    (_, Some(edge)) => Some(Sel::Edge { body, edge }),
    _ => None,
  }
}

/// The value a pick gives a row that takes `fits`, if it fits.
fn fit_value(fits: Fits, sel: &Sel, s: &Session) -> Option<Value> {
  let plane = || match *sel {
    Sel::Face { body, face } if planar(s, body, face) => Some(json!({"body": body.0, "face": face})),
    Sel::Feature { feature } if s.model.planes.contains_key(&feature) => Some(json!({"feature": feature.0})),
    _ => None,
  };
  match fits {
    Fits::Faces => match *sel {
      Sel::Face { body, face } => Some(json!({"body": body.0, "face": face})),
      _ => None,
    },
    Fits::Plane => plane(),
    Fits::Surface => plane().or_else(|| fit_value(Fits::Faces, sel, s)),
    Fits::Direction => plane().or_else(|| match *sel {
      Sel::Edge { body, edge } => {
        let e = s.model.body(body)?.edges().iter().find(|e| e.id == edge)?;
        (e.kind == fox_engine::kernel::CurveKind::Line).then(|| json!({"body": body.0, "edge": edge}))
      }
      Sel::Feature { feature } if s.model.axes.contains_key(&feature) => Some(json!({"axis": feature.0})),
      _ => None,
    }),
    Fits::Body => match *sel {
      Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } => Some(json!(body.0)),
      _ => None,
    },
  }
}

/// Does a drawing form take model points picked on the sheet?
pub fn takes_sheet_points(form: &Form) -> bool {
  form.inputs.iter().any(|x| matches!(x, Input::DrawingView { .. }))
    && form.inputs.iter().any(|x| matches!(x, Input::Vec3 { .. } | Input::Text { key: "points", .. }))
}

/// A model coordinate as a field shows it: up to three decimals, no trailing zeros.
pub fn short(x: f64) -> String {
  let t = format!("{x:.3}");
  let t = t.trim_end_matches('0').trim_end_matches('.');
  if t == "-0" { "0".into() } else { t.into() }
}

fn planar(s: &Session, body: BodyId, face: u32) -> bool {
  s.model
    .body(body)
    .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| f.kind == fox_engine::kernel::SurfaceKind::Plane))
    .unwrap_or(false)
}

fn face_kind(s: &Session, v: &Value) -> Option<fox_engine::kernel::SurfaceKind> {
  let body = BodyId(v.get("body")?.as_u64()? as u32);
  let face = v.get("face")?.as_u64()? as u32;
  s.model.body(body)?.faces().iter().find(|f| f.id == face).map(|f| f.kind)
}

fn face_center(s: &Session, v: &Value) -> Option<[f64; 3]> {
  let body = BodyId(v.get("body")?.as_u64()? as u32);
  let face = v.get("face")?.as_u64()? as u32;
  s.model.body(body)?.faces().iter().find(|f| f.id == face).map(|f| f.center.to_array())
}

const OPS: &[(&str, &str)] = &[("new_body", "New Body"), ("join", "Join"), ("cut", "Cut"), ("intersect", "Intersect")];

pub static THREAD: Form = Form {
  title: "THREAD",
  name: "Thread",
  hint: "Thread cylindrical faces with a standard size: cosmetic, or modeled (cut the helix)",
  icon: Icon::Thread,
  cmd: "thread",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Faces" },
    Input::Check { key: "modeled", label: "Modeled" },
    Input::CheckOn { key: "full_length", label: "Full Length" },
    Input::Flip { key: "flip", label: "Thread Location" },
    Input::Expr { key: "offset", label: "Thread Offset", default: "0 mm" },
    Input::Expr { key: "length", label: "Thread Length", default: "" },
    // Thread Type, Size, Designation and Class are drop-downs fed by the thread tables (drawn
    // by `dialogs::thread`), Threads Library's row above them
    Input::Text { key: "thread_type", label: "Thread Type", default: "" },
    Input::Text { key: "size", label: "Size", default: "" },
    Input::Text { key: "designation", label: "Designation", default: "" },
    Input::Text { key: "class", label: "Class", default: "" },
    Input::Choice { key: "direction", label: "Direction", options: &[("right", "Right hand"), ("left", "Left hand")] },
    Input::CheckOn { key: "remember", label: "Remember Size" },
  ],
  picks_model: true,
};

pub static COIL: Form = Form {
  title: "COIL",
  name: "Coil",
  hint: "A helical coil (spring) around a plane's normal",
  icon: Icon::Coil,
  cmd: "coil.form",
  fixed: &[],
  // Fusion's rows: the plane and centre come from the circle drawn before the dialog opens (the
  // Profile chip; see `dialogs::primitive`), the pitch from the Type's two values
  inputs: &[
    Input::Plane { key: "plane", label: "Profile" },
    Input::Text { key: "at", label: "Center", default: "" },
    Input::Choice {
      key: "type",
      label: "Type",
      options: &[
        ("revolution_height", "Revolution and Height"),
        ("revolution_pitch", "Revolution and Pitch"),
        ("height_pitch", "Height and Pitch"),
        ("spiral", "Spiral"),
      ],
    },
    Input::Icons {
      key: "rotation",
      label: "Rotation",
      options: &[("cw", Icon::RotateCw, "Clockwise"), ("ccw", Icon::RotateCcw, "Counterclockwise")],
    },
    Input::Expr { key: "diameter", label: "Diameter", default: "20 mm" },
    Input::Expr { key: "revolutions", label: "Revolutions", default: "3" },
    Input::Expr { key: "height", label: "Height", default: "20 mm" },
    Input::Expr { key: "pitch", label: "Pitch", default: "5 mm" },
    Input::Expr { key: "angle", label: "Angle", default: "0.0 deg" },
    Input::Choice {
      key: "section",
      label: "Section",
      options: &[
        ("circular", "Circular"),
        ("square", "Square"),
        ("triangle_out", "Triangular (External)"),
        ("triangle_in", "Triangular (Internal)"),
      ],
    },
    Input::Choice {
      key: "section_position",
      label: "Section Position",
      options: &[("center", "On Center"), ("inside", "Inside"), ("outside", "Outside")],
    },
    Input::Expr { key: "section_size", label: "Section Size", default: "6.35 mm" },
    Input::Choice { key: "operation", label: "Operation", options: OPS },
  ],
  picks_model: false,
};

pub static PIPE: Form = Form {
  title: "PIPE",
  name: "Pipe",
  hint: "A round, square or triangular tube along a sketch path of lines and arcs",
  icon: Icon::Pipe,
  cmd: "pipe.form",
  fixed: &[],
  // Fusion's rows: the Path chip (the picked curves, chained to the curves joined to them), the
  // Distance rows and Section Size once a path is picked, Section Thickness under Hollow
  inputs: &[
    Input::Sketch { key: "path", label: "Path sketch" },
    Input::Curves { key: "path_curves", label: "Path" },
    Input::CheckOn { key: "chain", label: "Chain Selection" },
    Input::Expr { key: "distance", label: "Distance", default: "1.00" },
    Input::Expr { key: "distance2", label: "Distance", default: "0.00" },
    Input::Choice {
      key: "section",
      label: "Section",
      options: &[("circular", "Circular"), ("square", "Square"), ("triangular", "Triangular")],
    },
    Input::Expr { key: "diameter", label: "Section Size", default: "5.08 mm" },
    Input::Check { key: "hollow", label: "Hollow" },
    Input::Expr { key: "wall", label: "Section Thickness", default: "0.5 mm" },
    Input::Choice { key: "operation", label: "Operation", options: OPS },
  ],
  picks_model: false,
};

/// Rib's and Web's Thickness Direction: half the thickness each side of the lines, or all of
/// it on one side.
const THICKNESS_DIRECTIONS: &[(&str, Icon, &str)] =
  &[("symmetric", Icon::SymmetricExtent, "Symmetric"), ("one_side", Icon::OneSide, "One Side")];

/// Rib's and Web's Extent Type: grow until the body is met, or by a depth.
const RIB_EXTENTS: &[(&str, &str)] = &[("to_next", "To Next"), ("depth", "Depth")];

pub static RIB: Form = Form {
  title: "RIB",
  name: "Rib",
  hint: "A rib grown from open sketch lines until it meets the body",
  icon: Icon::Rib,
  cmd: "rib",
  fixed: &[],
  inputs: &[
    Input::Curves { key: "curves", label: "Profile" },
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Icons { key: "thickness_dir", label: "Thickness Direction", options: THICKNESS_DIRECTIONS },
    Input::Choice { key: "extent", label: "Extent Type", options: RIB_EXTENTS },
    Input::Flip { key: "flip", label: "Flip Direction" },
    Input::Expr { key: "thickness", label: "Thickness", default: "2 mm" },
    Input::Expr { key: "depth", label: "Depth", default: "10 mm" },
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
    Input::Curves { key: "curves", label: "Profile" },
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Icons { key: "thickness_dir", label: "Thickness Direction", options: THICKNESS_DIRECTIONS },
    Input::Choice { key: "extent", label: "Extent Type", options: RIB_EXTENTS },
    Input::CheckOn { key: "extend", label: "Extend Curves" },
    Input::Flip { key: "flip", label: "Flip Direction" },
    Input::Expr { key: "thickness", label: "Thickness", default: "2 mm" },
    Input::Expr { key: "depth", label: "Depth", default: "10 mm" },
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
    Input::Pick {
      key: "faces",
      label: "Faces to Split",
      fits: Fits::Faces,
      multi: true,
      prompt: "Select Face to split",
    },
    Input::Pick {
      key: "tool",
      label: "Splitting Tool(s)",
      fits: Fits::Plane,
      multi: false,
      prompt: "Select splitting tool(s)",
    },
    Input::CheckOn { key: "chain", label: "Chain Selection" },
    Input::Choice { key: "split_type", label: "Split Type", options: SPLIT_TYPES },
    Input::CheckOn { key: "extend", label: "Extend Splitting Tool" },
  ],
  picks_model: true,
};

pub static SILHOUETTE_SPLIT: Form = Form {
  title: "SILHOUETTE SPLIT",
  name: "Silhouette Split",
  hint: "Split a body where its outline lies, seen along a direction",
  icon: Icon::SilhouetteSplit,
  cmd: "silhouette_split",
  fixed: &[],
  inputs: &[
    Input::Pick {
      key: "direction",
      label: "View Direction",
      fits: Fits::Direction,
      multi: false,
      prompt: "Select plane or axis for the pull direction",
    },
    Input::Pick { key: "body", label: "Target Body", fits: Fits::Body, multi: false, prompt: "Select target body" },
    Input::Choice {
      key: "operation",
      label: "Operation",
      options: &[("shelled", "Split Shelled Body"), ("face", "Split Face"), ("solid", "Split Solid Body")],
    },
  ],
  picks_model: true,
};

/// Split Face's Split Type, as Fusion lists it (the engine splits with the surface).
const SPLIT_TYPES: &[(&str, &str)] =
  &[("surface", "Split with Surface"), ("vector", "Along Vector"), ("closest", "Closest Point")];

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

pub static THICKEN: Form = Form {
  title: "THICKEN",
  name: "Thicken",
  hint: "Thicken faces or surface bodies into solids",
  icon: Icon::Thicken,
  cmd: "thicken",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Faces / Surface Bodies", required: true },
    Input::CheckOn { key: "chain", label: "Chain Selection" },
    Input::Expr { key: "thickness", label: "Thickness", default: "1 mm" },
    Input::Choice {
      key: "direction",
      label: "Direction",
      options: &[("one_side", "One Side"), ("symmetric", "Symmetric")],
    },
    Input::Icons {
      key: "corners",
      label: "Thicken Type",
      options: &[("sharp", Icon::ShellSharp, "Sharp"), ("round", Icon::ShellRound, "Rounded")],
    },
    Input::Choice { key: "operation", label: "Operation", options: OPS },
  ],
  picks_model: true,
};

pub static PLANE_ANGLE: Form = Form {
  title: "PLANE AT ANGLE",
  name: "Plane at Angle",
  hint: "A plane through an edge or axis, turned by an angle",
  icon: Icon::PlaneAngle,
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
  icon: Icon::Midplane,
  cmd: "plane",
  fixed: &[("method", "midplane")],
  inputs: &[Input::Plane { key: "a", label: "First plane" }, Input::Plane { key: "b", label: "Second plane" }],
  picks_model: false,
};

pub static TANGENT_PLANE: Form = Form {
  title: "TANGENT PLANE",
  name: "Tangent Plane",
  hint: "A plane tangent to a cylinder, cone or sphere at the picked point",
  icon: Icon::PlaneTangent,
  cmd: "plane",
  fixed: &[("method", "tangent")],
  inputs: &[Input::Face { key: "face", label: "Face", near: Some("near") }],
  picks_model: false,
};

pub static PLANE_TWO_EDGES: Form = Form {
  title: "PLANE THROUGH TWO EDGES",
  name: "Plane Through Two Edges",
  hint: "A plane through two coplanar straight edges",
  icon: Icon::PlaneTwoEdges,
  cmd: "plane",
  fixed: &[("method", "two_edges")],
  inputs: &[Input::Edge { key: "a", label: "First edge" }, Input::Edge { key: "b", label: "Second edge" }],
  picks_model: false,
};

pub static PLANE_THREE_POINTS: Form = Form {
  title: "PLANE THROUGH THREE POINTS",
  name: "Plane Through Three Points",
  hint: "A plane through three points",
  icon: Icon::PlaneThreePoints,
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
  icon: Icon::PlaneAlongPath,
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
  icon: Icon::AxisCylinder,
  cmd: "axis",
  fixed: &[("method", "face")],
  inputs: &[Input::Face { key: "face", label: "Face", near: None }],
  picks_model: false,
};

pub static AXIS_NORMAL: Form = Form {
  title: "AXIS PERPENDICULAR TO FACE",
  name: "Axis Perpendicular to Face",
  hint: "An axis normal to a face through the picked point",
  icon: Icon::AxisNormal,
  cmd: "axis",
  fixed: &[("method", "normal")],
  inputs: &[Input::Face { key: "face", label: "Face", near: Some("near") }],
  picks_model: false,
};

pub static AXIS_TWO_PLANES: Form = Form {
  title: "AXIS THROUGH TWO PLANES",
  name: "Axis Through Two Planes",
  hint: "The line where two planes meet",
  icon: Icon::AxisTwoPlanes,
  cmd: "axis",
  fixed: &[("method", "two_planes")],
  inputs: &[Input::Plane { key: "a", label: "First plane" }, Input::Plane { key: "b", label: "Second plane" }],
  picks_model: false,
};

pub static AXIS_EDGE: Form = Form {
  title: "AXIS THROUGH EDGE",
  name: "Axis Through Edge",
  hint: "An axis along a straight edge (or a circle's axis)",
  icon: Icon::AxisEdge,
  cmd: "axis",
  fixed: &[("method", "edge")],
  inputs: &[Input::Edge { key: "edge", label: "Edge" }],
  picks_model: false,
};

pub static AXIS_TWO_POINTS: Form = Form {
  title: "AXIS THROUGH TWO POINTS",
  name: "Axis Through Two Points",
  hint: "An axis through two points",
  icon: Icon::AxisTwoPoints,
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
  icon: Icon::PointVertex,
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
  icon: Icon::PointTwoEdges,
  cmd: "point",
  fixed: &[("method", "two_edges")],
  inputs: &[Input::Edge { key: "a", label: "First edge" }, Input::Edge { key: "b", label: "Second edge" }],
  picks_model: false,
};

pub static POINT_THREE_PLANES: Form = Form {
  title: "POINT THROUGH THREE PLANES",
  name: "Point Through Three Planes",
  hint: "Where three planes meet",
  icon: Icon::PointThreePlanes,
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
  icon: Icon::PointCenter,
  cmd: "point",
  fixed: &[("method", "center")],
  inputs: &[Input::Face { key: "face", label: "Face", near: None }],
  picks_model: false,
};

pub static POINT_EDGE_PLANE: Form = Form {
  title: "POINT AT EDGE AND PLANE",
  name: "Point at Edge and Plane",
  hint: "Where an edge crosses a plane",
  icon: Icon::PointEdgePlane,
  cmd: "point",
  fixed: &[("method", "edge_plane")],
  inputs: &[Input::Edge { key: "edge", label: "Edge" }, Input::Plane { key: "plane", label: "Plane" }],
  picks_model: false,
};

pub static POINT_ALONG_PATH: Form = Form {
  title: "POINT ALONG PATH",
  name: "Point Along Path",
  hint: "A point a fraction of the way along an edge",
  icon: Icon::PointAlongPath,
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

// not on the SURFACE menus, as in Fusion: to fold into Patch's boundary pick
// (batch ui-surface-create-dialogs)
#[allow(dead_code)]
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

/// Fusion's Text: the dialog opens with the Type row alone (a text box, or text on a path) and the
/// prompt to place the frame's first corner; two clicks on the canvas place the frame (or the
/// path is picked), and only then the Text, Font, Typeface, Height, Character Spacing, Flip and
/// Alignment rows appear with "Sample Text" in the frame. The frame's corners and the angle its
/// rotate handle sets are hidden inputs (see [`State::text_frame`]). The Typeface, Flip and
/// Alignment rows are drawn as icon toggles (see `Dialog::form_fields`).
pub static SK_TEXT: Form = Form {
  title: "TEXT",
  name: "Text",
  hint: "Editable text whose outlines you can extrude, in a frame placed on the sketch or along a picked curve",
  icon: Icon::SkText,
  cmd: "sketch.text.form",
  fixed: &[],
  inputs: &[
    Input::Icons { key: "type", label: "Type", options: TEXT_TYPES },
    Input::Curves { key: "path", label: "Path" },
    Input::Text { key: "text", label: "Text", default: "Sample Text" },
    Input::Choice { key: "font", label: "Font", options: TEXT_FONTS },
    Input::Check { key: "bold", label: "Typeface" },
    Input::Check { key: "italic", label: "Italic" },
    Input::Expr { key: "height", label: "Height", default: "10 mm" },
    Input::Expr { key: "spacing", label: "Character Spacing", default: "0.00" },
    Input::Check { key: "flip_h", label: "Flip" },
    Input::Check { key: "flip_v", label: "Flip vertical" },
    Input::Choice { key: "halign", label: "Alignment", options: TEXT_HALIGN },
    Input::Choice { key: "valign", label: "Vertical Alignment", options: TEXT_VALIGN },
    Input::Expr { key: "angle", label: "Angle (deg)", default: "0" },
    Input::Check { key: "below", label: "Below the path" },
    Input::Choice {
      key: "align",
      label: "Path alignment",
      options: &[("left", "Left"), ("center", "Center"), ("right", "Right")],
    },
    Input::Choice { key: "placement", label: "Placement", options: TEXT_PLACEMENTS },
    Input::Expr { key: "x", label: "X", default: "0" },
    Input::Expr { key: "y", label: "Y", default: "0" },
    Input::Expr { key: "x2", label: "Corner 2 X", default: "" },
    Input::Expr { key: "y2", label: "Corner 2 Y", default: "" },
  ],
  picks_model: false,
};

/// Fusion's Type row: a text box dragged on the sketch, or text along a picked curve.
const TEXT_TYPES: &[(&str, Icon, &str)] =
  &[("box", Icon::TextBox, "Text Box"), ("path", Icon::TextOnPath, "Text On Path")];
/// Sketch Text: in a box between two corners (the alignment places it in the box), or at a point.
const TEXT_PLACEMENTS: &[(&str, &str)] = &[("box", "Two-corner box"), ("point", "Point")];
const TEXT_HALIGN: &[(&str, &str)] = &[("left", "Left"), ("center", "Center"), ("right", "Right")];
const TEXT_VALIGN: &[(&str, &str)] =
  &[("baseline", "Baseline"), ("top", "Top"), ("middle", "Middle"), ("bottom", "Bottom")];

/// The `sketch.text` parameters of the Text form's values: a box's corners place the text by its
/// alignment (left, centre or right; top, middle or bottom of the box).
fn text_params(p: &Value) -> Value {
  let num = |k: &str| p.get(k).and_then(Value::as_str).and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
  let get = |k: &str, d: &str| p.get(k).and_then(Value::as_str).unwrap_or(d).to_string();
  let (halign, mut valign) = (get("halign", "left"), get("valign", "baseline"));
  let framed = ["x2", "y2"].iter().all(|k| p.get(k).and_then(Value::as_str).is_some_and(|t| !t.trim().is_empty()));
  let at = if get("placement", "point") == "box" && framed {
    // the frame's corners in the text's own frame (turned back by its angle), so a text that
    // reads at an angle still sits in the frame as dragged
    let (sa, ca) = num("angle").to_radians().sin_cos();
    let turn = |q: [f64; 2], s: f64| [q[0] * ca + q[1] * s, -q[0] * s + q[1] * ca];
    let (c1, c2) = (turn([num("x"), num("y")], sa), turn([num("x2"), num("y2")], sa));
    let (x0, x1) = (c1[0].min(c2[0]), c1[0].max(c2[0]));
    let (y0, y1) = (c1[1].min(c2[1]), c1[1].max(c2[1]));
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
    turn([x, y], -sa).map(|v| if v.abs() < 1e-9 { 0.0 } else { v })
  } else {
    [num("x"), num("y")]
  };
  let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
  // the height is an expression ("10 mm", "0.394 in", a parameter), which the engine evaluates
  let height = match p.get("height").and_then(Value::as_str).map(str::trim) {
    Some(t) if !t.is_empty() => json!(t),
    _ => json!(5.0),
  };
  json!({
    "at": at,
    "text": p.get("text").cloned().unwrap_or(json!("")),
    "height": height,
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
    "coil.form" => ("coil", coil_params(&p)),
    "pipe.form" => ("pipe", pipe_params(&p)),
    "plane.points" => ("plane", json!({"method": "three_points", "a": pt("a"), "b": pt("b"), "c": pt("c")})),
    "axis.points" => ("axis", json!({"method": "two_points", "a": pt("a"), "b": pt("b")})),
    "point.vertex" => (
      "point",
      json!({"method": "vertex", "point": {"vertex": p.get("edge").cloned().unwrap_or(Value::Null), "end": p.get("end") == Some(&json!("end"))}}),
    ),
    "sketch.intersect.form" | "sketch.spun_profile.form" => {
      // the Selection Filter decides what Geometry holds: whole bodies, or just the picked faces
      // (Spun Profile spins their bodies: the engine spins bodies only)
      let mut q = p.clone();
      if let Some(o) = q.as_object_mut() {
        let faces = o.get("filter").and_then(Value::as_str) == Some("faces");
        o.remove("filter");
        if faces && form.cmd == "sketch.intersect.form" {
          o.remove("bodies");
        } else {
          o.remove("faces");
        }
      }
      (if form.cmd == "sketch.intersect.form" { "sketch.intersect" } else { "sketch.spun_profile" }, q)
    }
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
    // drawing annotations: the frame's or symbol's offset from its point, in sheet mm; a
    // composite frame's lower tier
    c if c.starts_with("drawing.") && p.get("offset_x").is_some() => {
      let num = |k: &str| p.get(k).and_then(Value::as_str).and_then(|t| t.trim().parse::<f64>().ok()).unwrap_or(0.0);
      let mut q = p.clone();
      if let Some(m) = q.as_object_mut() {
        m.remove("offset_x");
        m.remove("offset_y");
        m.insert("offset".into(), json!([num("offset_x"), num("offset_y")]));
        let mut lower = Map::new();
        // the lower tier's zone is the upper's shape (Ø or not)
        for k in ["tolerance", "modifier", "datums"] {
          if let Some(v) = m.remove(&format!("lower_{k}")) {
            lower.insert(k.into(), v);
          }
        }
        if m.remove("composite") == Some(json!(true)) {
          m.insert("lower".into(), Value::Object(lower));
        }
      }
      (form.cmd, q)
    }
    "rib" | "web" | "emboss" | "thicken" | "sheet.corner_closure" => {
      let mut q = p.clone();
      if let Some(m) = q.as_object_mut() {
        engine_keys(form.cmd, m);
      }
      (form.cmd, q)
    }
    "thread" => ("thread", crate::thread_form::engine_params(&p)),
    "surface.trim.body" => {
      // the tool is the body of the picked face; it doesn't trim itself
      let tool = p.pointer("/tool/body").cloned().unwrap_or(Value::Null);
      let bodies: Vec<Value> =
        p.get("bodies").and_then(Value::as_array).into_iter().flatten().filter(|b| **b != tool).cloned().collect();
      let mut q =
        json!({"bodies": bodies, "tool": {"body": tool}, "flip": p.get("flip").cloned().unwrap_or(json!(false))});
      if let Some(r) = p.get("remove") {
        q["remove"] = r.clone();
      }
      ("surface.trim", q)
    }
    "surface.trim.sketch" => {
      let tool = json!({"sketch": p.get("sketch").cloned().unwrap_or(Value::Null), "curves": p.get("curves").cloned().unwrap_or(json!([]))});
      let mut q = json!({"bodies": p.get("bodies").cloned().unwrap_or(json!([])), "tool": tool, "flip": p.get("flip").cloned().unwrap_or(json!(false))});
      if let Some(r) = p.get("remove") {
        q["remove"] = r.clone();
      }
      ("surface.trim", q)
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
      for k in ["objects", "relation", "offset", "flip"] {
        if let Some(v) = p.get(k) {
          q[k] = v.clone();
        }
      }
      ("align", q)
    }
    // the ticked types of the Features group
    "remove_features.form" => {
      let types: Vec<&str> =
        REMOVE_FEATURE_TYPES.iter().copied().filter(|t| p.get(*t).and_then(Value::as_bool).unwrap_or(false)).collect();
      ("remove_features", json!({"bodies": p.get("bodies").cloned().unwrap_or(json!([])), "types": types}))
    }
    "sketch.scale_to.form" => crate::scale_to_ui::params(&p),
    "sketch.remove_duplicates.form" => {
      // a preset tolerance, or the custom one
      let mut q = p.clone();
      if let Some(m) = q.as_object_mut() {
        let custom = m.remove("custom");
        if m.get("tolerance").and_then(Value::as_str) == Some("custom") {
          match custom {
            Some(c) => m.insert("tolerance".into(), c),
            None => m.remove("tolerance"),
          };
        }
      }
      ("sketch.remove_duplicates", q)
    }
    "sketch.cleanup.form" => {
      // the form's ticks keep links and points; the command's flags remove them
      let mut q = p.clone();
      if let Some(m) = q.as_object_mut() {
        let keep_links = m.remove("keep_links").and_then(|v| v.as_bool()).unwrap_or(false);
        let keep_points = m.remove("keep_points").and_then(|v| v.as_bool()).unwrap_or(false);
        m.insert("break_links".into(), json!(!keep_links));
        m.insert("purge_points".into(), json!(!keep_points));
      }
      ("sketch.cleanup", q)
    }
    "mesh.tessellate" | "mesh.section_sketch" => {
      let mut q = p;
      crate::mesh_ui::params(form, &mut q);
      (form.cmd, q)
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
  hint: "Trim surface bodies with a plane, or click the pieces to remove",
  icon: Icon::Trim,
  cmd: "surface.trim",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Surfaces", required: true },
    Input::Plane { key: "tool", label: "Trimming tool" },
    Input::Check { key: "flip", label: "Keep the other side" },
    Input::Pieces { key: "remove", label: "Pieces to remove" },
  ],
  picks_model: true,
};

const CAM_POSTS: &[(&str, &str)] =
  &[("fanuc", "Fanuc"), ("haas", "Haas"), ("grbl", "GRBL"), ("linuxcnc", "LinuxCNC"), ("mach3", "Mach3 / Mach4")];

const CAM_TOOL: Input = Input::Expr { key: "tool_number", label: "Library tool (0 = by diameter)", default: "0" };

/// 3+2 milling: the tool axis (straight down Z unless the setup has a 4/5-axis machine).
const CAM_TOOL_AXIS: Input = Input::Vec3 { key: "tool_axis", label: "Tool axis (3+2)", default: ["0", "0", "1"] };

const CAM_MACHINES: &[(&str, &str)] = &[
  ("3axis", "3-axis mill"),
  ("table_table", "5-axis, table/table (A tilt, C table)"),
  ("head_table", "5-axis, head/table (B head, C table)"),
  ("rotary", "4th axis (A about X)"),
];

const LATHE_POSTS: &[(&str, &str)] =
  &[("fanuc_lathe", "Fanuc lathe"), ("haas_lathe", "Haas lathe"), ("linuxcnc_lathe", "LinuxCNC lathe")];

/// A lathe tool from the library (0 = the library's first of the kind the operation needs).
const LATHE_TOOL: Input = Input::Expr { key: "tool_number", label: "Library tool (0 = default)", default: "0" };

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
    Input::Bodies { key: "fixtures", label: "Fixtures (vise, clamps)", required: false },
    Input::Choice { key: "machine", label: "Machine", options: CAM_MACHINES },
    Input::Expr { key: "tilt_min", label: "Tilt travel from (°)", default: "-120" },
    Input::Expr { key: "tilt_max", label: "Tilt travel to (°)", default: "120" },
    Input::Expr { key: "pivot", label: "Head pivot to tool tip (0 = TCP)", default: "0" },
  ],
  picks_model: false,
};

pub static CAM_TURN_SETUP: Form = Form {
  title: "SETUP",
  name: "New Turning Setup",
  hint: "A lathe setup: the bar in the chuck about the part's axis, Z0 on a face, X0 on the axis",
  icon: Icon::Revolve,
  cmd: "cam.setup",
  fixed: &[("type", "turning")],
  inputs: &[
    Input::Text { key: "name", label: "Name", default: "" },
    Input::Choice {
      key: "axis",
      label: "Spindle axis",
      options: &[("", "Automatic"), ("x", "X"), ("y", "Y"), ("z", "Z"), ("-x", "-X"), ("-y", "-Y"), ("-z", "-Z")],
    },
    Input::Expr { key: "stock_side", label: "Bar over the part (radial)", default: "2" },
    Input::Expr { key: "stock_front", label: "Stock to face off", default: "1" },
    Input::Expr { key: "chuck_gap", label: "Gap before the chuck", default: "5" },
    Input::Expr { key: "grip", label: "Bar in the jaws", default: "15" },
    Input::Choice {
      key: "origin",
      label: "Z0",
      options: &[("front", "Part front face"), ("stock_front", "Stock front"), ("back", "Part back face")],
    },
    Input::Choice { key: "post", label: "Post-processor", options: LATHE_POSTS },
    Input::Choice { key: "units", label: "G-code units", options: &[("mm", "Millimetres"), ("inch", "Inches")] },
  ],
  picks_model: false,
};

pub static CAM_TURN_FACE: Form = Form {
  title: "FACE",
  name: "Turning Face",
  hint: "Face the front of the bar down to the part",
  icon: Icon::Extrude,
  cmd: "cam.turn_face",
  fixed: &[],
  inputs: &[LATHE_TOOL, Input::Expr { key: "stepdown", label: "Depth per pass", default: "1 mm" }],
  picks_model: false,
};

pub static CAM_TURN_ROUGH: Form = Form {
  title: "PROFILE ROUGHING",
  name: "Profile Roughing",
  hint: "Rough the outside down to the part in passes along the axis",
  icon: Icon::Shell,
  cmd: "cam.turn_rough",
  fixed: &[],
  inputs: &[
    LATHE_TOOL,
    Input::Expr { key: "stepdown", label: "Depth of cut (radial)", default: "1.5 mm" },
    Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0.2 mm" },
  ],
  picks_model: false,
};

pub static CAM_TURN_FINISH: Form = Form {
  title: "PROFILE FINISHING",
  name: "Profile Finishing",
  hint: "One pass down the front face and along the part's outside",
  icon: Icon::SkOffset,
  cmd: "cam.turn_finish",
  fixed: &[],
  inputs: &[LATHE_TOOL, Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0" }],
  picks_model: false,
};

pub static CAM_GROOVE: Form = Form {
  title: "GROOVE",
  name: "Groove",
  hint: "Plunge the grooves and undercuts the profile passes can't reach",
  icon: Icon::Slot,
  cmd: "cam.groove",
  fixed: &[],
  inputs: &[
    LATHE_TOOL,
    Input::Expr { key: "stepover", label: "Step-over (of the insert)", default: "0.8" },
    Input::Expr { key: "stepdown", label: "Peck depth", default: "2 mm" },
  ],
  picks_model: false,
};

pub static CAM_PART_OFF: Form = Form {
  title: "PART",
  name: "Part Off",
  hint: "Part the finished part off the bar at its back face",
  icon: Icon::SplitBody,
  cmd: "cam.part_off",
  fixed: &[],
  inputs: &[LATHE_TOOL, Input::Expr { key: "stepdown", label: "Peck depth", default: "3 mm" }],
  picks_model: false,
};

pub static CAM_THREAD: Form = Form {
  title: "THREAD",
  name: "Turning Thread",
  hint: "Cut an outside thread along a cylindrical face",
  icon: Icon::Thread,
  cmd: "cam.thread",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Thread face" },
    LATHE_TOOL,
    Input::Expr { key: "pitch", label: "Pitch", default: "1.5 mm" },
    Input::Expr { key: "depth", label: "Thread depth (0 = ISO)", default: "0" },
    Input::Expr { key: "stepdown", label: "First pass depth", default: "0.15 mm" },
  ],
  picks_model: false,
};

pub static CAM_ROTARY: Form = Form {
  title: "ROTARY",
  name: "Rotary",
  hint: "4th axis: passes wrapped around the part as it turns, the tool square to the axis",
  icon: Icon::Revolve,
  cmd: "cam.rotary",
  fixed: &[],
  inputs: &[
    CAM_TOOL,
    Input::Choice { key: "tool_kind", label: "Tool", options: &[("ball", "Ball end mill"), ("flat", "Flat end mill")] },
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Check { key: "around", label: "Rings around the axis (else passes along it)" },
    Input::Expr { key: "stepover", label: "Step-over (of the tool)", default: "0.15" },
    Input::Expr { key: "stepdown", label: "Maximum step-down", default: "3 mm" },
    Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1200" },
  ],
  picks_model: false,
};

pub static CAM_SWARF: Form = Form {
  title: "SWARF",
  name: "Swarf",
  hint: "Simultaneous 5-axis: the side of the tool along drafted or conical walls",
  icon: Icon::Draft,
  cmd: "cam.swarf",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Walls" },
    CAM_TOOL,
    Input::Expr { key: "tool_diameter", label: "Tool diameter", default: "6 mm" },
    Input::Expr { key: "stepdown", label: "Maximum step-down", default: "5 mm" },
    Input::Expr { key: "stock_to_leave", label: "Stock to leave", default: "0" },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1000" },
  ],
  picks_model: false,
};

pub static CAM_FIXTURES: Form = Form {
  title: "FIXTURES",
  name: "Fixtures",
  hint: "The bodies holding the part (vise jaws, clamps): never machined, checked for collisions with tools and holders",
  icon: Icon::Component,
  cmd: "cam.fixtures",
  fixed: &[],
  inputs: &[Input::Bodies { key: "fixtures", label: "Fixtures (none = clear)", required: false }],
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
    CAM_TOOL_AXIS,
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
    CAM_TOOL_AXIS,
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
    CAM_TOOL_AXIS,
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
    CAM_TOOL_AXIS,
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

/// The fonts the Font drop-down always offers: the two built in; the system's fonts (Fusion
/// lists them, "Arial" first) are added when the row is drawn (see [`font_choices`]).
const TEXT_FONTS: &[(&str, &str)] = &[("sans", "Sans (Ubuntu Light)"), ("mono", "Mono (Hack)")];

/// Fusion's default font.
pub const DEFAULT_FONT: &str = "Arial";

/// The Font drop-down's choices: Arial first when the system has it (Fusion's default), the
/// other system fonts by name, then the built-in two. Found once per run.
pub fn font_choices() -> &'static [(String, String)] {
  static FONTS: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
  FONTS.get_or_init(|| {
    let mut names: Vec<String> = fox_engine::sketch::system_fonts().into_iter().map(|(n, _)| n).collect();
    names.sort_by_key(|n| (!n.eq_ignore_ascii_case(DEFAULT_FONT), n.to_lowercase()));
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let mut out: Vec<(String, String)> = names.into_iter().map(|n| (n.clone(), n)).collect();
    out.extend(TEXT_FONTS.iter().map(|(k, l)| (k.to_string(), l.to_string())));
    out
  })
}

/// The font a new Text starts with: Arial when the system has it, else the built-in sans.
pub fn default_font() -> &'static str {
  if font_choices().iter().any(|(k, _)| k == DEFAULT_FONT) { DEFAULT_FONT } else { "sans" }
}

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
    CAM_TOOL_AXIS,
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
    // Fusion: the Flange dialog with Type Lofted flange (see `crate::dialogs::FLANGE_TYPES`)
    Input::Icons { key: "flange_type", label: "Type", options: crate::dialogs::FLANGE_TYPES },
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
    CAM_TOOL_AXIS,
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
    Input::Check { key: "composite", label: "Composite (lower tier)" },
    Input::Expr { key: "lower_tolerance", label: "  Lower tolerance", default: "0.05" },
    Input::Choice {
      key: "lower_modifier",
      label: "  Lower material condition",
      options: &[("none", "None"), ("mmc", "MMC Ⓜ"), ("lmc", "LMC Ⓛ")],
    },
    Input::Text { key: "lower_datums", label: "  Lower datums", default: "A" },
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
    Input::Profiles { key: "profiles", label: "Sketch Profiles" },
    Input::Face { key: "face", label: "Faces", near: None },
    Input::CheckOn { key: "tangent_chain", label: "Tangent Chain" },
    Input::Icons {
      key: "effect",
      label: "Effect",
      options: &[("emboss", Icon::EmbossRaise, "Emboss"), ("deboss", Icon::EmbossSink, "Deboss")],
    },
    Input::Expr { key: "depth", label: "Depth", default: "1 mm" },
    Input::Expr { key: "horizontal", label: "Horizontal Distance", default: "0 mm" },
    Input::Expr { key: "vertical", label: "Vertical Distance", default: "0 mm" },
    Input::Expr { key: "rotation", label: "Rotation Angle", default: "0 deg" },
    Input::Check { key: "wrap", label: "Wrap" },
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
    Input::Check { key: "close_corners", label: "Close corners" },
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
    Input::Expr { key: "extension", label: "Extension", default: "0 mm" },
  ],
  picks_model: true,
};

// not on the SURFACE menus, as in Fusion: to fold into Trim's Trim Tool pick
// (batch ui-surface-modify-dialogs)
#[cfg_attr(not(test), allow(dead_code))]
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
    Input::Pieces { key: "remove", label: "Pieces to remove" },
  ],
  picks_model: true,
};

// not on the SURFACE menus, as in Fusion: to fold into Trim's Trim Tool pick
// (batch ui-surface-modify-dialogs)
#[cfg_attr(not(test), allow(dead_code))]
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
    Input::Pieces { key: "remove", label: "Pieces to remove" },
  ],
  picks_model: true,
};

pub static BOUNDARY_FILL: Form = Form {
  title: "BOUNDARY FILL",
  name: "Boundary Fill",
  hint: "Solids from the closed cells that bodies and planes bound",
  icon: Icon::BoundaryFill,
  cmd: "boundary_fill",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "tools", label: "Select Tools", required: true },
    Input::Planes { key: "planes" },
    Input::Cells { key: "cells", label: "Select Cells", tools: "tools" },
    Input::Choice { key: "operation", label: "Operation", options: OPS },
    Input::Check { key: "remove_tools", label: "Remove Tools" },
  ],
  picks_model: true,
};

pub static REPLACE_FACE: Form = Form {
  title: "REPLACE FACE",
  name: "Replace Face",
  hint: "Replace faces (flat or curved) with a face of another body (or a plane); neighbours extend or trim",
  icon: Icon::ReplaceFace,
  cmd: "replace_face",
  fixed: &[],
  inputs: &[
    Input::Pick {
      key: "faces",
      label: "Source Faces",
      fits: Fits::Faces,
      multi: true,
      prompt: "Select faces to replace",
    },
    Input::Check { key: "tangent_chain", label: "Tangent Chain" },
    // (a face, or a plane: the engine takes one target)
    Input::Pick {
      key: "target",
      label: "Target Faces",
      fits: Fits::Surface,
      multi: false,
      prompt: "Select target faces",
    },
  ],
  picks_model: true,
};

// not on the SURFACE menus, as in Fusion: to fold into Offset's Offset Type
// (batch ui-surface-create-dialogs)
#[cfg_attr(not(test), allow(dead_code))]
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
    // Fusion opens with Edges alone (lit) and OK greyed; the corner's rows come with the pick
    Input::Edges { key: "edges", label: "Edges" },
    Input::Choice { key: "corners", label: "Corners", options: CORNERS },
    Input::Choice {
      key: "relief",
      label: "Corner relief",
      options: &[("none", "None"), ("round", "Round"), ("square", "Square"), ("tear", "Tear")],
    },
    Input::Expr { key: "relief_size", label: "Relief size (empty = default)", default: "" },
  ],
  picks_model: true,
};

pub static JOIN_BY_BEND: Form = Form {
  title: "JOIN BY BEND",
  name: "Join by bend",
  hint: "Join two sheet metal bodies with a bend: pick an edge on each flange",
  icon: Icon::Fold,
  cmd: "sheet.join_by_bend",
  fixed: &[],
  inputs: &[
    Input::Edge { key: "edge1", label: "Edge one" },
    Input::Edge { key: "edge2", label: "Edge two" },
    Input::Choice {
      key: "position",
      label: "Bend position",
      options: &[("tangent", "Tangent to both"), ("first", "From edge one"), ("second", "From edge two")],
    },
    Input::Expr { key: "bend_radius", label: "Bend radius (empty = rule)", default: "" },
    Input::Choice {
      key: "relief",
      label: "Bend relief",
      options: &[("rule", "Rule"), ("rectangular", "Rectangular"), ("round", "Round"), ("none", "None")],
    },
  ],
  picks_model: true,
};

pub static SHEET_FILLET: Form = Form {
  title: "SHEET METAL FILLET",
  name: "Sheet Metal Fillet",
  hint: "Round edges that run through the sheet's thickness; the rounding stays in the flat pattern",
  icon: Icon::Fillet,
  cmd: "sheet.fillet",
  fixed: &[],
  inputs: &[
    Input::Edges { key: "edges", label: "Edges" },
    Input::Expr { key: "radius", label: "Radius", default: "2 mm" },
  ],
  picks_model: true,
};

pub static SHEET_CHAMFER: Form = Form {
  title: "SHEET METAL CHAMFER",
  name: "Sheet Metal Chamfer",
  hint: "Bevel edges that run through the sheet's thickness; the bevel stays in the flat pattern",
  icon: Icon::Chamfer,
  cmd: "sheet.chamfer",
  fixed: &[],
  inputs: &[
    Input::Edges { key: "edges", label: "Edges" },
    Input::Choice {
      key: "chamfer_type",
      label: "Chamfer type",
      options: &[("equal", "Equal distance"), ("two_distances", "Two distances")],
    },
    Input::Expr { key: "distance", label: "Distance", default: "2 mm" },
    Input::Expr { key: "distance2", label: "Distance 2 (two distances)", default: "2 mm" },
    Input::Check { key: "flipped", label: "Flip" },
  ],
  picks_model: true,
};

pub static FLAT_SKETCH: Form = Form {
  title: "FLAT PATTERN SKETCH",
  name: "Flat Pattern Sketch",
  hint: "The flat pattern as a sketch: outline and bend lines that follow the body as it changes",
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
    Input::Choice {
      key: "curves",
      label: "Curves",
      options: &[("follow", "Follow the body"), ("once", "Free, made once")],
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
  if crate::mesh_ui::is_mesh_form(form) {
    crate::mesh_ui::prefill(form, state, s);
    return;
  }
  if std::ptr::eq(form, &SK_TEXT) {
    // Fusion's new text is in Arial (when the system has it)
    if let Some(i) = form.inputs.iter().position(|x| x.key() == "font")
      && let Some(slot) = state.values.get_mut(i)
    {
      *slot = json!(default_font());
    }
    return;
  }
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
    Input::Check { key: "rest", label: "Rest machining (only what earlier, larger tools left)" },
    Input::Choice { key: "entry", label: "Entry", options: &[("helix", "Helix"), ("plunge", "Plunge")] },
    Input::Expr { key: "feed", label: "Feed (mm/min)", default: "1500" },
    Input::Expr { key: "plunge", label: "Plunge (mm/min)", default: "300" },
    CAM_TOOL_AXIS,
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
    Input::Expr { key: "passes", label: "Passes either side", default: "0" },
    Input::Expr { key: "stepover", label: "Step-over (of the tool)", default: "0.1" },
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
    CAM_TOOL_AXIS,
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
    CAM_TOOL_AXIS,
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
  inputs: &[
    Input::Edges { key: "edges", label: "Geometry" },
    Input::CheckOn { key: "associative", label: "Projection Link" },
  ],
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

/// The solid patterns' Type row (the pattern dialog switches kind on it; see `dialogs::copies`).
const PATTERN_TYPES: &[(&str, Icon, &str)] = &[
  ("rectangular", Icon::Pattern, "Rectangular Pattern"),
  ("circular", Icon::CircPattern, "Circular Pattern"),
  ("path", Icon::PathPattern, "Pattern on Path"),
];

pub static PATH_PATTERN: Form = Form {
  title: "PATTERN ON PATH",
  name: "Pattern on Path",
  hint: "Copies of bodies spaced by arc length along a sketch path",
  icon: Icon::Pattern,
  cmd: "pattern.path",
  fixed: &[],
  // Fusion: the pattern dialog with Type on path: Type, Object Type, Objects and Path, then
  // Distribution, Quantity, Distance and Orientation once both are picked (the path's sketch,
  // start point and join are passed but not rows; see `State::hidden`)
  inputs: &[
    Input::Icons { key: "pattern_type", label: "Type", options: PATTERN_TYPES },
    Input::Choice { key: "object_type", label: "Object Type", options: &[("bodies", "Bodies")] },
    Input::Bodies { key: "bodies", label: "Objects", required: true },
    Input::Curves { key: "path_curves", label: "Path" },
    Input::Sketch { key: "path", label: "Path sketch" },
    Input::Choice {
      key: "distance_type",
      label: "Distribution",
      options: &[("extent", "Extent"), ("spacing", "Spacing")],
    },
    Input::Expr { key: "count", label: "Quantity", default: "3" },
    Input::Expr { key: "distance", label: "Distance", default: "10 mm" },
    Input::Choice {
      key: "orientation",
      label: "Orientation",
      options: &[("identical", "Identical"), ("along_path", "Path Direction")],
    },
    Input::Check { key: "flip", label: "Flip" },
    Input::Check { key: "symmetric", label: "Symmetric" },
    Input::Expr { key: "start", label: "Start point (0 to 1)", default: "0" },
    Input::Check { key: "join", label: "Join to the original" },
  ],
  picks_model: true,
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
    Input::Curves { key: "curves", label: "Curves" },
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Choice { key: "type", label: "Project Type", options: PROJECTION_TYPES },
    Input::Axis { key: "direction", label: "Direction" },
    Input::CheckOn { key: "associative", label: "Projection Link" },
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
    Input::Curves { key: "curves", label: "Curve" },
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Faces { key: "faces", label: "Curves or faces" },
    Input::CheckOn { key: "associative", label: "Projection Link" },
  ],
  picks_model: true,
};

/// Fusion's Selection Filter row: whole bodies, or faces and edges.
const GEOMETRY_FILTERS: &[(&str, Icon, &str)] =
  &[("bodies", Icon::FilterBodies, "Bodies"), ("faces", Icon::FilterFaces, "Faces and edges")];

/// Project / Include > Intersect: the curves where picked bodies (or faces) cut the sketch plane.
pub static SK_INTERSECT: Form = Form {
  title: "INTERSECT",
  name: "Intersect",
  hint: "Curves where bodies or faces cut the sketch plane; they follow the bodies",
  icon: Icon::Intersect,
  cmd: "sketch.intersect.form",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Geometry", required: true },
    Input::FacesOpt { key: "faces", label: "Faces" },
    Input::Icons { key: "filter", label: "Selection Filter", options: GEOMETRY_FILTERS },
    Input::CheckOn { key: "associative", label: "Projection Link" },
  ],
  picks_model: true,
};

/// Project / Include > Spun Profile, with Fusion's rows (Project Axis and Tolerance are greyed:
/// the engine draws the outline exactly and adds the axis as a centre line).
pub static SK_SPUN_PROFILE: Form = Form {
  title: "SPUN PROFILE",
  name: "Spun Profile",
  hint: "The outline bodies sweep through as they turn about an axis, on the sketch plane; it follows the bodies",
  icon: Icon::Revolve,
  cmd: "sketch.spun_profile.form",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Geometry", required: true },
    Input::Icons { key: "filter", label: "Selection Filter", options: GEOMETRY_FILTERS },
    Input::CheckOn { key: "associative", label: "Projection Link" },
    Input::Axis { key: "axis", label: "Axis" },
    Input::CheckOn { key: "project_axis", label: "Project Axis" },
    Input::CheckOn { key: "centerline", label: "Add Centerline" },
    Input::Expr { key: "tolerance", label: "Tolerance", default: "0 mm" },
    Input::Check { key: "flip", label: "Flip Result" },
  ],
  picks_model: true,
};

/// Project / Include > Isoparametric Curve: Fusion's dialog, greyed until the kernel can
/// evaluate a face's iso-curves (the engine has no command for it yet).
pub static SK_ISOPARAMETRIC: Form = Form {
  title: "ISOPARAMETRIC CURVE",
  name: "Isoparametric Curve",
  hint: "A curve of constant U or V on a face, as a 3D sketch curve",
  icon: Icon::Intersect,
  cmd: "sketch.isoparametric_curve",
  fixed: &[],
  inputs: &[
    Input::Choice { key: "curve", label: "Curve", options: &[("single", "Single"), ("multiple", "Multiple")] },
    Input::Face { key: "face", label: "Face", near: None },
    Input::CheckOn { key: "associative", label: "Projection Link" },
  ],
  picks_model: true,
};

/// Why one option of a form's icon row is drawn greyed (not pickable): what FissionOxide can't
/// do yet.
pub fn greyed_option(form: &Form, key: &str, option: &str) -> Option<&'static str> {
  match (form.cmd, key, option) {
    ("joint_origin", "mode", "between" | "two_edge") => {
      Some("Only the Simple mode is in FissionOxide yet: a joint origin sits on one picked face")
    }
    _ => None,
  }
}

/// Why a form's input is drawn greyed: what FissionOxide can't act on yet, or always does.
/// The command's parameters from the values of Fusion's rows: Rib's Thickness Direction and
/// Extent Type, Emboss's Effect, Thicken's Direction (a thickness typed negative thickens the
/// other way, as Fusion's has no Flip row). Rows FissionOxide can't act on (greyed) go.
pub fn engine_keys(cmd: &str, m: &mut Map<String, Value>) {
  match cmd {
    "rib" | "web" => {
      if m.remove("thickness_dir") == Some(json!("one_side")) {
        m.insert("one_side".into(), json!(true));
      }
      if m.remove("extent").is_some_and(|e| e != "depth") {
        m.remove("depth");
      }
      m.remove("extend");
    }
    // the picked edges open the dialog's rows; the closure goes on the last edge or miter flange
    "sheet.corner_closure" => {
      m.remove("edges");
    }
    "emboss" => {
      if let Some(e) = m.remove("effect") {
        m.insert("deboss".into(), json!(e == "deboss"));
      }
      m.remove("tangent_chain");
    }
    "thicken" => {
      if let Some(d) = m.remove("direction") {
        m.insert("symmetric".into(), json!(d == "symmetric"));
      }
      let t = m.get("thickness").and_then(Value::as_str).unwrap_or("").trim().to_string();
      if let Some(rest) = t.strip_prefix('-') {
        m.insert("thickness".into(), json!(rest.trim_start()));
        m.insert("flip".into(), json!(true));
      }
      m.remove("chain");
    }
    _ => {}
  }
}

/// Does the form wait for a pick of this (otherwise optional) input before OK? Rib's and Web's
/// Profile and Emboss's Sketch Profiles, as in Fusion.
fn required(form: &Form, key: &str) -> bool {
  matches!((form.cmd, key), ("rib" | "web", "curves") | ("emboss", "profiles"))
}

/// Fusion's cursor prompt while a form waits for its picks, where it isn't the missing row's
/// name.
pub fn pick_prompt(form: &Form) -> Option<&'static str> {
  Some(match form.cmd {
    "rib" => "Select open sketch profile to create rib",
    "web" => "Select open sketch profile to create web",
    "thicken" => "Select Surfaces to Thicken",
    "boundary_fill" => "Select bodies & planes",
    "sheet.corner_closure" => "Select edges",
    "mesh.combine_face_groups" => crate::mesh_ui::COMBINE_PROMPT,
    _ => return None,
  })
}

/// Fusion's cursor prompt once the form has its picks.
pub fn specify_prompt(form: &Form) -> Option<&'static str> {
  match form.cmd {
    "thread" => Some("Specify size, or hold Ctrl/CMD to modify selections"),
    _ => None,
  }
}

/// Forms whose Fusion dialog has no Preview check box: they always preview.
pub fn always_previews(form: &Form) -> bool {
  matches!(form.cmd, "thread" | "emboss" | "thicken" | "boundary_fill" | "rib" | "web")
}

/// Forms whose drop-downs show an icon before each value, as Fusion's Extent Type, Direction
/// and Operation do.
pub fn icon_dropdowns(form: &Form) -> bool {
  matches!(form.cmd, "rib" | "web" | "thicken" | "boundary_fill")
}

pub fn greyed(form: &Form, key: &str) -> Option<&'static str> {
  match (form.cmd, key) {
    ("web", "extend") => Some("A web's lines aren't extended to the body in FissionOxide yet"),
    ("emboss", "tangent_chain") => Some("Emboss goes on one face in FissionOxide: a tangent chain isn't picked yet"),
    ("thicken", "chain") => Some("Thicken picks whole surface bodies in FissionOxide: chains of faces aren't yet"),
    ("thread", "remember") => Some("Each Thread starts from the picked face's size in FissionOxide"),
    ("sketch.project_to_surface" | "sketch.intersection_curve", "associative") => {
      Some("These curves always follow the model in FissionOxide")
    }
    ("sketch.spun_profile.form", "associative") => Some("A spun profile always follows its bodies in FissionOxide"),
    ("sketch.spun_profile.form", "project_axis") => Some("Projecting the axis isn't in FissionOxide yet"),
    ("sketch.spun_profile.form", "tolerance") => {
      Some("The outline is drawn exactly; a tolerance isn't in FissionOxide yet")
    }
    ("coil.form", "angle") => Some("Tapered coils aren't in FissionOxide yet"),
    ("pipe.form", "distance2") => Some("A pipe runs from its path's start in FissionOxide"),
    ("split_face", "split_type") => Some("FissionOxide splits faces with the surface only"),
    ("split_face", "extend") => Some("A splitting plane always extends across the faces in FissionOxide"),
    ("remove_features.form", "chamfer" | "extrude" | "revolve" | "other") => {
      Some("Recognising these features isn't in FissionOxide yet: it finds fillets and holes")
    }
    ("sketch.isoparametric_curve", "curve" | "associative") => {
      Some("Isoparametric curves aren't in FissionOxide yet: the kernel can't evaluate a face's iso-curves")
    }
    _ => None,
  }
}

const ALIGN_RELATIONS: &[(&str, &str)] =
  &[("auto", "Automatic"), ("flush", "Flush"), ("coincident", "Coincident"), ("concentric", "Concentric")];

/// What Align moves: the From geometry's body or its component (Fusion's Object drop-down).
const ALIGN_OBJECTS: &[(&str, &str)] = &[("bodies", "Bodies"), ("components", "Components")];

/// Fusion's ALIGN: Object, From and To; the relation rows follow once both are picked.
pub static ALIGN: Form = Form {
  title: "ALIGN",
  name: "Align",
  hint: "Move a body or component so its geometry lines up with another",
  icon: Icon::Align,
  cmd: "align.form",
  fixed: &[],
  inputs: &[
    Input::Choice { key: "objects", label: "Object", options: ALIGN_OBJECTS },
    Input::Face { key: "moving", label: "From", near: None },
    Input::Face { key: "fixed", label: "To", near: None },
    Input::Choice { key: "relation", label: "Relation", options: ALIGN_RELATIONS },
    Input::Expr { key: "offset", label: "Offset", default: "0 mm" },
    Input::Check { key: "flip", label: "Flip" },
  ],
  picks_model: true,
};

/// Fusion's REMOVE with nothing selected: one Objects row; OK removes them (a timeline feature).
pub static REMOVE: Form = Form {
  title: "REMOVE",
  name: "Remove",
  hint: "Remove bodies or components (as a timeline feature)",
  icon: Icon::RemoveBody,
  cmd: "body.remove",
  fixed: &[],
  inputs: &[Input::Bodies { key: "bodies", label: "Objects", required: true }],
  picks_model: true,
};

/// Fusion's REMOVE FEATURES (Simplify): Bodies, then a Features group of types to look for; the
/// found features are removed from the dialog, which has only a Close button.
pub static REMOVE_FEATURES: Form = Form {
  title: "REMOVE FEATURES",
  name: "Remove Features",
  hint: "Find fillets, holes and other features on bodies and remove them",
  icon: Icon::RemoveFeatures,
  cmd: "remove_features.form",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies", required: true },
    Input::CheckOn { key: "all", label: "Select All" },
    Input::CheckOn { key: "fillet", label: "Fillet" },
    Input::CheckOn { key: "hole", label: "Hole" },
    Input::CheckOn { key: "chamfer", label: "Chamfer" },
    Input::CheckOn { key: "extrude", label: "Extrude" },
    Input::CheckOn { key: "revolve", label: "Revolve" },
    Input::CheckOn { key: "other", label: "Other" },
  ],
  picks_model: false,
};

/// The feature types of [`REMOVE_FEATURES`]'s Features group, in its order.
pub const REMOVE_FEATURE_TYPES: [&str; 6] = ["fillet", "hole", "chamfer", "extrude", "revolve", "other"];

const PRIMITIVES: &[(&str, &str)] = &[("box", "Box"), ("cylinder", "Cylinder")];

/// Replace with Primitives (Simplify): each body swapped for a box or cylinder around it.
pub static REPLACE_PRIMITIVES: Form = Form {
  title: "REPLACE WITH PRIMITIVES",
  name: "Replace with Primitives",
  hint: "Replace bodies with a box or cylinder that encloses each",
  icon: Icon::ReplacePrimitives,
  cmd: "replace_with_primitives",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies", required: true },
    Input::Choice { key: "shape", label: "Primitive Type", options: PRIMITIVES },
  ],
  picks_model: true,
};

/// Fusion's cursor prompt while a form's input `key` is the one missing, where it isn't the
/// row's label ("Select ...").
pub fn prompt(form: &Form, key: &str) -> Option<&'static str> {
  match (form.cmd, key) {
    ("align.form", "moving" | "fixed") => Some("Select geometry or snap point on alignable object"),
    ("body.remove", "bodies") => Some("Select bodies or components to remove"),
    ("remove_features.form" | "replace_with_primitives", "bodies") => Some("Select solid bodies"),
    _ => None,
  }
}

/// Does a form's dialog show the Preview box? Fusion's Align and Remove show their result
/// without one.
pub fn preview_box(form: &Form) -> bool {
  !matches!(form.cmd, "align.form" | "body.remove")
}

/// Forms whose dialog has only a Close button: what they do is applied from the dialog itself.
pub fn close_only(form: &Form) -> bool {
  form.cmd == "remove_features.form"
}

pub static SK_SCALE_TO: Form = Form {
  title: "SCALE TO SIZE",
  name: "Scale to Size",
  hint: "Scale sketch curves to a width or height, or to fit in a box (turned to fit best)",
  icon: Icon::SkScaleTo,
  cmd: "sketch.scale_to.form",
  fixed: &[],
  inputs: &[
    Input::Curves { key: "curves", label: "Entities (default all)" },
    Input::Choice { key: "mode", label: "Scale to", options: crate::scale_to_ui::MODES },
    Input::Expr { key: "width", label: "Width", default: "36 in" },
    Input::Expr { key: "height", label: "Height", default: "18 in" },
    Input::Choice { key: "preset", label: "Box", options: crate::scale_to_ui::PRESETS },
    Input::Expr { key: "box_width", label: "Custom box width", default: "20 in" },
    Input::Expr { key: "box_height", label: "Custom box height", default: "20 in" },
    Input::Check { key: "margin_mode", label: "Margin mode (the box is the outer size)" },
    Input::Expr { key: "padding", label: "Padding", default: "0.25 in" },
    Input::Choice { key: "anchor", label: "Anchor", options: crate::scale_to_ui::ANCHORS },
    Input::Check { key: "rotate", label: "Rotate to fit (box)" },
  ],
  picks_model: false,
};

const DUPLICATE_TOLERANCES: &[(&str, &str)] = &[
  ("0.001 in", "Standard 0.001\""),
  ("0.00001 in", "Exact"),
  ("0.0001 in", "Tight 0.0001\""),
  ("0.01 in", "Loose 0.01\""),
  ("custom", "Custom"),
];

pub static SK_REMOVE_DUPLICATES: Form = Form {
  title: "REMOVE DUPLICATES",
  name: "Remove Duplicates",
  hint: "Delete curves lying on another of the same kind (the first stays); overlapping lines are cut back",
  icon: Icon::SkRemoveDuplicates,
  cmd: "sketch.remove_duplicates.form",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Choice { key: "tolerance", label: "Tolerance", options: DUPLICATE_TOLERANCES },
    Input::Expr { key: "custom", label: "Custom tolerance", default: "0.001 in" },
    Input::Check { key: "include_construction", label: "Include construction" },
  ],
  picks_model: false,
};

const CLEANUP_CONSTRAINTS: &[(&str, &str)] =
  &[("projected", "On projected geometry"), ("all", "All"), ("none", "None")];
const CLEANUP_SPLINES: &[(&str, &str)] = &[("none", "Keep"), ("arcs", "Arcs and lines"), ("lines", "Lines")];

pub static SK_CLEANUP: Form = Form {
  title: "CLEAN UP",
  name: "Clean Up",
  hint: "Make an imported or projected sketch DXF-clean: links broken, constraints and stray points gone, gaps closed, splines to arcs or lines",
  icon: Icon::SkCleanup,
  cmd: "sketch.cleanup.form",
  fixed: &[],
  inputs: &[
    Input::Sketch { key: "sketch", label: "Sketch" },
    Input::Check { key: "keep_links", label: "Keep projection links" },
    Input::Choice { key: "delete_constraints", label: "Delete constraints", options: CLEANUP_CONSTRAINTS },
    Input::Check { key: "delete_dimensions", label: "Delete dimensions" },
    Input::Check { key: "keep_points", label: "Keep unattached points" },
    Input::Check { key: "construction_to_normal", label: "Construction to normal" },
    Input::Check { key: "merge_gaps", label: "Merge gaps" },
    Input::Expr { key: "gap_tolerance", label: "Gap tolerance", default: "0.001 in" },
    Input::Choice { key: "splines_to", label: "Splines to", options: CLEANUP_SPLINES },
    Input::Expr { key: "segments", label: "Segments (lines)", default: "64" },
    Input::Expr { key: "spline_tolerance", label: "Tolerance (arcs)", default: "0.001 in" },
  ],
  picks_model: false,
};

pub static COLLECT_FLAT: Form = Form {
  title: "ALL FLAT PATTERNS ON ONE SHEET",
  name: "All Flat Patterns on One Sheet",
  hint: "Every sheet metal body's flat pattern in one Cut File sketch, in rows that wrap at the sheet width",
  icon: Icon::FlatPattern,
  cmd: "sheet.collect_flat",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies (default every sheet metal body)", required: false },
    Input::Expr { key: "max_width", label: "Sheet width", default: "48 in" },
    Input::Expr { key: "gap", label: "Gap", default: "0.25 in" },
    Input::Text { key: "sketch_name", label: "Sketch name", default: "Cut File" },
  ],
  picks_model: false,
};

/// The `coil` command for Coil's rows: the Type's two values make the height and pitch, the
/// Rotation the hand (counterclockwise is left-handed) and the Section Position moves the
/// section's centre inside or outside the drawn diameter.
fn coil_params(p: &Value) -> Value {
  let text = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
  let mut q = Map::new();
  for k in ["plane", "at", "section", "section_size", "operation"] {
    if let Some(v) = p.get(k) {
      q.insert(k.into(), v.clone());
    }
  }
  let (d, size) = (text("diameter"), text("section_size"));
  let diameter = match p.get("section_position").and_then(Value::as_str) {
    Some("inside") => format!("({d}) - ({size})"),
    Some("outside") => format!("({d}) + ({size})"),
    _ => d,
  };
  q.insert("diameter".into(), json!(diameter));
  let (h, n, pitch) = (text("height"), text("revolutions"), text("pitch"));
  match p.get("type").and_then(Value::as_str).unwrap_or("revolution_height") {
    "revolution_height" => {
      q.insert("height".into(), json!(h));
      q.insert("pitch".into(), json!(format!("({h}) / ({n})")));
    }
    "height_pitch" => {
      q.insert("height".into(), json!(h));
      q.insert("pitch".into(), json!(pitch));
    }
    _ => {
      q.insert("revolutions".into(), json!(n));
      q.insert("pitch".into(), json!(pitch));
    }
  }
  q.insert("left".into(), json!(p.get("rotation").and_then(Value::as_str) == Some("ccw")));
  Value::Object(q)
}

/// The `pipe` command for Pipe's rows: the wall only when Hollow is ticked.
fn pipe_params(p: &Value) -> Value {
  let mut q = Map::new();
  for k in ["path", "path_curves", "section", "diameter", "distance", "operation"] {
    if let Some(v) = p.get(k) {
      q.insert(k.into(), v.clone());
    }
  }
  if p.get("hollow").and_then(Value::as_bool) == Some(true)
    && let Some(w) = p.get("wall")
  {
    q.insert("wall".into(), w.clone());
  }
  Value::Object(q)
}
