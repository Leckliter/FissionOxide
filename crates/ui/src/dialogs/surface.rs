//! The SURFACE tab's Create dialogs in Fusion's layout. Extrude, Revolve, Sweep and Loft have
//! dialogs of their own (none has an Operation row): their pick rows (Profiles, Axis, Path) take
//! sketch profiles, curves, straight edges or axes, the next pick going to the active row, which
//! a click on a row's chip makes active. Patch, Ruled and Offset are forms ([`crate::forms`])
//! whose rows this module draws where Fusion's differ from a form's (Patch's continuity table and
//! Flip button, the greyed rows), with their value rows waiting for the first pick. None of these
//! dialogs has a Preview checkbox; Sweep, Loft, Patch and Ruled have the Feature and Analysis tabs
//! of the solid Sweep and Loft ([`super::rsl`]); a drag arrow (or Revolve's dial) with its floating
//! value box sets the distance (or angle).

use fox_engine::kernel::glam::DVec3;
use fox_engine::model::FeatureId;
use fox_engine::sketch::{CurveId, CurveKind};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::manip::{deg, mm};
use super::{Dialog, Kind, choice, icon_row, sel_chip, sel_chip_lit, value_edit};
use crate::App;
use crate::handles::Handle;
use crate::icons::{self, Icon};
use crate::theme;

/// A pick a surface dialog's rows can hold.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Pick {
  /// A sketch profile (its index in the sketch and a point inside it).
  Profile(FeatureId, usize, [f64; 2]),
  Curve(FeatureId, u32),
  /// A body edge.
  Edge(u32, u32),
  /// A construction axis.
  Axis(FeatureId),
}

/// The surface dialogs' values (and the Analysis tab of the forms with one).
#[derive(Debug, Clone)]
pub(crate) struct State {
  /// The row the next picks go to.
  pub(super) row: usize,
  /// Picks per row: Profiles, then Axis (Revolve) or Path (Sweep), then Loft's Rails or
  /// Centerline.
  pub(super) rows: [Vec<Pick>; 3],
  /// Picks already routed to a row (or replaced there), so they aren't routed again.
  seen: Vec<Pick>,
  /// Chaining / Chain Selection: a picked curve brings the curves joined end to end to it.
  pub(super) chain: bool,
  /// Extrude: Start ("" profile plane, "offset"), its offset, Direction and the distances.
  pub(super) start: &'static str,
  pub(super) offset: String,
  pub(super) direction: &'static str,
  pub(super) distance: String,
  pub(super) distance2: String,
  /// Revolve: Extent Type ("partial" or "full"), the angles and Project Axis.
  pub(super) extent: &'static str,
  pub(super) angle: String,
  pub(super) angle2: String,
  project_axis: bool,
  /// Sweep: Orientation ("perpendicular" or "parallel").
  pub(super) orientation: &'static str,
  /// Loft: Guide Type ("rails" or "centerline"), Closed and Tangent Edges ("merge" or "keep").
  pub(super) guide: &'static str,
  pub(super) closed: bool,
  tangent_edges: &'static str,
  /// Patch: the picked boundaries as drawn on the canvas (each pick's chained loop as polylines),
  /// for the picks and Enable Chaining they were worked out for.
  outline: Option<(Value, Vec<Vec<Vec<DVec3>>>)>,
  /// What the picks come to, worked out from the session at each sync: the profile sketch with
  /// its picked profiles and (chained) curves, the path sketch with its curves, the loft sections.
  src: Option<(FeatureId, Vec<[f64; 2]>, Vec<u32>)>,
  path: Option<(FeatureId, Vec<u32>)>,
  sections: Vec<Value>,
  /// Loft's rails (or its one centerline) as the engine takes them: `{sketch, curves}` each.
  guides: Vec<Value>,
}

impl State {
  /// A fresh state for the dialog `kind`: Loft's Chain Selection starts unchecked, as Fusion's.
  pub(super) fn new(kind: Kind) -> State {
    State { chain: kind != Kind::SurfLoft, ..State::default() }
  }
}

impl Default for State {
  fn default() -> State {
    State {
      row: 0,
      rows: [Vec::new(), Vec::new(), Vec::new()],
      seen: Vec::new(),
      chain: true,
      start: "",
      offset: "0 mm".into(),
      direction: "one_side",
      distance: "0 mm".into(),
      distance2: "0 mm".into(),
      extent: "partial",
      angle: "360 deg".into(),
      angle2: "0 deg".into(),
      project_axis: true,
      orientation: "perpendicular",
      guide: "rails",
      closed: false,
      tangent_edges: "merge",
      outline: None,
      src: None,
      path: None,
      sections: Vec::new(),
      guides: Vec::new(),
    }
  }
}

const STARTS: [(&str, &str); 2] = [("", "Profile Plane"), ("offset", "Offset")];
const DIRECTIONS: [(&str, &str); 3] =
  [("one_side", "One Side"), ("two_sides", "Two Sides"), ("symmetric", "Symmetric")];
const EXTENT_TYPES: [(&str, &str); 1] = [("distance", "Distance")];
const REVOLVE_EXTENTS: [(&str, &str); 2] = [("partial", "Partial"), ("full", "Full")];
const ORIENTATIONS: [(&str, &str); 2] = [("perpendicular", "Perpendicular"), ("parallel", "Parallel")];
/// Loft Guide Type: rails or a centerline (the solid Loft's icons).
const GUIDE_TYPES: [icon_row::Choice; 2] =
  [icon_row::choice("rails", Icon::SplineFit, "Rails"), icon_row::choice("centerline", Icon::Centerline, "Centerline")];
const SWEEP_TYPES: [(&str, &str); 1] = [("single", "Single Path")];
const CONTINUITY: [(&str, &str); 3] = [("connected", "Connected"), ("tangent", "Tangent"), ("curvature", "Curvature")];

/// The surface dialogs (their own kinds; the forms are told apart by their commands).
pub(super) fn is_surface(kind: Kind) -> bool {
  matches!(kind, Kind::SurfExtrude | Kind::SurfRevolve | Kind::SurfSweep | Kind::SurfLoft)
}

/// Commands of the surface forms this module draws rows for.
fn form_cmd(kind: Kind) -> Option<&'static str> {
  match kind {
    Kind::Form(f) if matches!(f.cmd, "surface.patch.form" | "surface.ruled.form" | "surface.offset.form") => {
      Some(f.cmd)
    }
    _ => None,
  }
}

/// Is the form row `key` of a surface form shown? Its value rows wait for the first pick.
pub(crate) fn form_row_shown<'a>(cmd: &str, key: &str, get: &dyn Fn(&str) -> Option<&'a Value>) -> bool {
  let picked = |k: &str| get(k).and_then(Value::as_array).is_some_and(|a| !a.is_empty());
  match (cmd, key) {
    ("surface.ruled.form", "distance" | "angle" | "mitered") => picked("edges"),
    ("surface.offset.form", "distance") => picked("faces") || picked("bodies"),
    ("surface.patch.form", "continuity") => picked("edges"),
    _ => true,
  }
}

/// What a surface form still needs, when its own rule decides (None: the form's usual check).
pub(crate) fn form_missing<'a>(cmd: &str, get: &dyn Fn(&str) -> Option<&'a Value>) -> Option<Option<&'static str>> {
  let picked = |k: &str| get(k).and_then(Value::as_array).is_some_and(|a| !a.is_empty());
  match cmd {
    "surface.offset.form" if !picked("faces") && !picked("bodies") => Some(Some("Faces / Surface Bodies")),
    "surface.offset.form" => Some(None),
    // OK waits for a distance, as Fusion's does
    "surface.ruled.form" if picked("edges") && !nonzero(get("distance").and_then(Value::as_str).unwrap_or("")) => {
      Some(Some("Distance"))
    }
    _ => None,
  }
}

/// A length that isn't zero (a parameter's name counts; a half-typed value doesn't).
fn nonzero(t: &str) -> bool {
  match fox_engine::expr::eval(t, fox_engine::expr::Kind::Length, &fox_engine::expr::Empty) {
    Ok(v) => v.abs() >= 1e-9,
    Err(_) => !t.trim().trim_matches(['.', '-', '+', ' ']).is_empty(),
  }
}

/// The engine command of a surface form.
pub(crate) fn form_command(cmd: &str, p: &Value) -> (&'static str, Value) {
  let get = |k: &str| p.get(k).cloned();
  let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
  match cmd {
    "surface.patch.form" => {
      let mut q = json!({"edges": get("edges").unwrap_or(json!([])), "chain": flag("chain"), "flip": flag("flip")});
      let cont: Vec<Value> = p.get("continuity").and_then(Value::as_array).cloned().unwrap_or_default();
      if flag("group") {
        q["continuity"] = cont.first().cloned().unwrap_or(json!("connected"));
      } else if !cont.is_empty() {
        q["continuity"] = json!(cont);
      }
      ("surface.patch", q)
    }
    "surface.ruled.form" => (
      "surface.ruled",
      json!({
        "edges": get("edges").unwrap_or(json!([])), "chain": flag("chain"),
        "kind": get("kind").unwrap_or(json!("normal")), "distance": get("distance").unwrap_or(json!("0 mm")),
        "angle": get("angle").unwrap_or(json!("0 deg")),
      }),
    ),
    _ => {
      // a whole body counts only when none of its faces is picked
      let faces: Vec<Value> = p.get("faces").and_then(Value::as_array).cloned().unwrap_or_default();
      let bodies: Vec<Value> = p
        .get("bodies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|b| !faces.iter().any(|f| f.get("body") == Some(b)))
        .cloned()
        .collect();
      let mut q = json!({
        "distance": get("distance").unwrap_or(json!("0 mm")), "tangent_chain": flag("tangent_chain"),
        "corners": get("corners").unwrap_or(json!("sharp")),
      });
      if !faces.is_empty() {
        q["faces"] = json!(faces);
      }
      if !bodies.is_empty() {
        q["bodies"] = json!(bodies);
      }
      ("surface.offset", q)
    }
  }
}

/// A selection as a surface dialog's pick.
fn pick_of(s: &Session, x: &Sel) -> Option<Pick> {
  match *x {
    Sel::Profile { sketch, index } => {
      Some(Pick::Profile(sketch, index, s.model.sketches.get(&sketch)?.profiles.get(index)?.interior_point()))
    }
    Sel::SketchCurve { sketch, curve } => Some(Pick::Curve(sketch, curve.0)),
    Sel::Edge { body, edge } => Some(Pick::Edge(body.0, edge)),
    Sel::Feature { feature } if s.model.axes.contains_key(&feature) => Some(Pick::Axis(feature)),
    _ => None,
  }
}

fn is_line(s: &Session, sketch: FeatureId, curve: u32) -> bool {
  s.design
    .sketch(sketch)
    .is_ok_and(|sk| sk.curve(CurveId(curve)).is_some_and(|c| matches!(c.kind, CurveKind::Line { .. })))
}

/// The sketch curves joined end to end to `seeds` (and the seeds), in the order reached.
fn chain_curves(s: &Session, sketch: FeatureId, seeds: &[u32]) -> Vec<u32> {
  let Ok(sk) = s.design.sketch(sketch) else { return seeds.to_vec() };
  let ends = |c: CurveId| {
    let segs = sk.curve_segs(c);
    Some([segs.first()?.start(), segs.last()?.end()])
  };
  let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6;
  let mut chain = seeds.to_vec();
  loop {
    let reached: Vec<[f64; 2]> = chain.iter().filter_map(|c| ends(CurveId(*c))).flatten().collect();
    let next = sk.curves.iter().find(|c| {
      !c.reference()
        && !chain.contains(&c.id.0)
        && ends(c.id).is_some_and(|e| e.iter().any(|p| reached.iter().any(|q| near(*p, *q))))
    });
    match next {
      Some(c) => chain.push(c.id.0),
      None => return chain,
    }
  }
}

impl Dialog {
  fn single_row(&self, row: usize) -> bool {
    row == 1 && self.kind == Kind::SurfRevolve
  }

  /// Can pick `p` go to row `row`?
  fn accepts(&self, s: &Session, row: usize, p: &Pick) -> bool {
    match (self.kind, row, p) {
      (_, 0, Pick::Profile(..) | Pick::Curve(..)) => true,
      (Kind::SurfRevolve, 1, Pick::Axis(_)) => true,
      (Kind::SurfRevolve, 1, Pick::Curve(sk, c)) => is_line(s, *sk, *c),
      (Kind::SurfRevolve, 1, Pick::Edge(b, e)) => s
        .model
        .body(fox_engine::model::BodyId(*b))
        .and_then(|b| b.edges().iter().find(|x| x.id == *e).map(|x| x.kind == fox_engine::kernel::CurveKind::Line))
        .unwrap_or(false),
      (Kind::SurfSweep, 1, Pick::Curve(..)) => true,
      // Loft's Rails (or Centerline): sketch curves
      (Kind::SurfLoft, 2, Pick::Curve(..)) => true,
      _ => false,
    }
  }

  /// Opens a surface dialog: the sketch's only profile picked, as Fusion's Extrude and Revolve
  /// open, and the next row (Revolve's Axis) active.
  pub(super) fn surf_open(&mut self, app: &mut App) {
    let s = &mut app.s;
    if !s.selection.iter().any(|x| pick_of(s, x).is_some())
      && matches!(self.kind, Kind::SurfExtrude | Kind::SurfRevolve)
      && let Some(sk) = s.edit_sketch.or(app.last_sketch).or_else(|| super::last_unconsumed_sketch(s))
      && s.model.sketches.get(&sk).is_some_and(|r| r.profiles.len() == 1)
    {
      s.selection.push(Sel::Profile { sketch: sk, index: 0 });
    }
    self.surf_sync(s);
  }

  /// Routes new picks to the active row (or the first row that takes them) and drops picks
  /// that were deselected.
  pub(super) fn surf_sync(&mut self, s: &Session) {
    let picks: Vec<Pick> = s.selection.iter().filter_map(|x| pick_of(s, x)).collect();
    let before = self.surf.rows.clone();
    for row in self.surf.rows.iter_mut() {
      row.retain(|p| picks.contains(p));
    }
    // a pick deselected and picked again is routed afresh
    self.surf.seen.retain(|p| picks.contains(p));
    for p in picks {
      if self.surf.seen.contains(&p) {
        continue;
      }
      self.surf.seen.push(p.clone());
      let active = self.surf.row;
      let Some(row) =
        (if self.accepts(s, active, &p) { Some(active) } else { (0..3).find(|r| self.accepts(s, *r, &p)) })
      else {
        continue;
      };
      if self.single_row(row) {
        self.surf.rows[row].clear();
      }
      self.surf.rows[row].push(p);
    }
    // the first profile moves the active row on to the axis or path, as in Fusion
    if matches!(self.kind, Kind::SurfRevolve | Kind::SurfSweep)
      && before[0].is_empty()
      && !self.surf.rows[0].is_empty()
      && self.surf.rows[1].is_empty()
    {
      self.surf.row = 1;
    }
    if self.surf.rows != before {
      self.dirty = true;
    }
    self.surf_resolve(s);
  }

  /// Works out what the picks come to (chains need the sketches); a change re-previews.
  pub(super) fn surf_resolve(&mut self, s: &Session) {
    let src = self.surf_source(s);
    let path =
      self.surf.rows[1].iter().find_map(|p| if let Pick::Curve(sk, _) = p { Some(*sk) } else { None }).map(|sk| {
        let seeds: Vec<u32> = self.surf.rows[1]
          .iter()
          .filter_map(|p| match p {
            Pick::Curve(k, c) if *k == sk => Some(*c),
            _ => None,
          })
          .collect();
        (sk, if self.surf.chain { chain_curves(s, sk, &seeds) } else { seeds })
      });
    let mut sections = self.loft_sections();
    if self.surf.chain {
      for x in sections.iter_mut() {
        let (Some(sk), Some(cs)) = (x["sketch"].as_u64(), x.get("curves").and_then(Value::as_array)) else { continue };
        let seeds: Vec<u32> = cs.iter().filter_map(Value::as_u64).map(|c| c as u32).collect();
        x["curves"] = json!(chain_curves(s, FeatureId(sk as u32), &seeds));
      }
    }
    let guides = self.loft_guides(s);
    if src != self.surf.src || path != self.surf.path || sections != self.surf.sections || guides != self.surf.guides {
      self.surf.src = src;
      self.surf.path = path;
      self.surf.sections = sections;
      self.surf.guides = guides;
      self.dirty = true;
    }
  }

  /// Loft's Rails as the engine takes them: one rail per picked curve (with the curves chained
  /// to it when Chain Selection is on; a curve another rail already reached adds none), or with
  /// Guide Type Centerline the picked curves as one path per sketch.
  fn loft_guides(&self, s: &Session) -> Vec<Value> {
    let mut out: Vec<(FeatureId, Vec<u32>)> = Vec::new();
    for p in &self.surf.rows[2] {
      let Pick::Curve(sk, c) = p else { continue };
      if self.surf.guide == "centerline" {
        match out.iter_mut().find(|(k, _)| k == sk) {
          Some((_, cs)) => cs.push(*c),
          None => out.push((*sk, vec![*c])),
        }
      } else if !out.iter().any(|(k, cs)| k == sk && cs.contains(c)) {
        out.push((*sk, if self.surf.chain { chain_curves(s, *sk, &[*c]) } else { vec![*c] }));
      }
    }
    if self.surf.guide == "centerline" && self.surf.chain {
      for (sk, cs) in out.iter_mut() {
        *cs = chain_curves(s, *sk, cs);
      }
    }
    out.into_iter().map(|(sk, cs)| json!({"sketch": sk.0, "curves": cs})).collect()
  }

  /// The sketch of the first profile pick, with the picked profiles' points and curves of it
  /// (chained when Chaining is on).
  fn surf_source(&self, s: &Session) -> Option<(FeatureId, Vec<[f64; 2]>, Vec<u32>)> {
    let sketch = self.surf.rows[0].iter().find_map(|p| match p {
      Pick::Profile(sk, ..) | Pick::Curve(sk, _) => Some(*sk),
      _ => None,
    })?;
    let profiles = self.surf.rows[0]
      .iter()
      .filter_map(|p| match p {
        Pick::Profile(sk, _, at) if *sk == sketch => Some(*at),
        _ => None,
      })
      .collect();
    let curves: Vec<u32> = self.surf.rows[0]
      .iter()
      .filter_map(|p| match p {
        Pick::Curve(sk, c) if *sk == sketch => Some(*c),
        _ => None,
      })
      .collect();
    let curves = if self.surf.chain && !curves.is_empty() { chain_curves(s, sketch, &curves) } else { curves };
    Some((sketch, profiles, curves))
  }

  /// Enough picked to try (and, for Extrude, a distance that isn't zero).
  pub(super) fn surf_ready(&self) -> bool {
    let [a, b] = &self.surf.rows;
    match self.kind {
      Kind::SurfExtrude => !a.is_empty() && nonzero(&self.surf.distance),
      Kind::SurfRevolve | Kind::SurfSweep => !a.is_empty() && !b.is_empty(),
      _ => self.loft_sections().len() >= 2,
    }
  }

  pub(super) fn surf_missing(&self) -> &'static str {
    let [a, b] = &self.surf.rows;
    match self.kind {
      Kind::SurfExtrude if a.is_empty() => "select profiles or curves",
      Kind::SurfExtrude => "specify a distance",
      Kind::SurfRevolve if a.is_empty() => "select a profile",
      Kind::SurfRevolve if b.is_empty() => "select an axis",
      Kind::SurfSweep if a.is_empty() => "select a profile",
      Kind::SurfSweep if b.is_empty() => "select a path",
      _ => "select profiles on two or more sketches",
    }
  }

  /// Fusion's cursor prompt for a surface dialog or form; None leaves it to the usual prompt.
  pub(super) fn surface_prompt(&self, hovering: bool) -> Option<String> {
    let [a, b] = &self.surf.rows;
    let text = match (self.kind, form_cmd(self.kind)) {
      (_, Some("surface.patch.form")) if !self.form_picked("edges") => "Select edge",
      (_, Some("surface.offset.form")) if !self.form_picked("faces") && !self.form_picked("bodies") => {
        "Select faces or surface bodies to offset"
      }
      (_, Some("surface.ruled.form")) if !self.form_picked("edges") => "Select edges",
      (_, Some("surface.ruled.form")) if !hovering => "Adjust distance and angle or modify selection",
      (Kind::SurfExtrude, _) if a.is_empty() => "Select sketch profiles or curves to extrude",
      (Kind::SurfRevolve, _) if a.is_empty() => "Select sketch profiles or curves to revolve",
      (Kind::SurfRevolve, _) if b.is_empty() => "Select axis",
      (Kind::SurfRevolve, _) if !hovering => "Adjust angle value or modify the selection",
      (Kind::SurfSweep, _) if a.is_empty() => "Select profile",
      (Kind::SurfSweep, _) if b.is_empty() => "Select path",
      (Kind::SurfLoft, _) if self.loft_sections().len() < 2 => "Select profiles",
      (Kind::SurfExtrude, _) if !hovering => "Specify distance and taper angle",
      _ => return None,
    };
    Some(text.into())
  }

  fn form_picked(&self, key: &str) -> bool {
    let Kind::Form(f) = self.kind else { return false };
    f.inputs
      .iter()
      .position(|x| x.key() == key)
      .and_then(|i| self.form.values.get(i))
      .and_then(Value::as_array)
      .is_some_and(|a| !a.is_empty())
  }

  /// Loft sections in pick order: a profile each, or the picked curves of a sketch.
  fn loft_sections(&self) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for p in &self.surf.rows[0] {
      match p {
        Pick::Profile(sk, _, at) => out.push(json!({"sketch": sk.0, "profile": at})),
        Pick::Curve(sk, c) => match out.iter_mut().find(|x| x["sketch"] == json!(sk.0) && x.get("curves").is_some()) {
          Some(x) => x["curves"].as_array_mut().into_iter().for_each(|a| a.push(json!(c))),
          None => out.push(json!({"sketch": sk.0, "curves": [c]})),
        },
        _ => {}
      }
    }
    out
  }

  /// The engine command of a surface dialog.
  pub(super) fn surf_command(&self) -> Option<(&'static str, Value)> {
    match self.kind {
      Kind::SurfExtrude => {
        let (sketch, profiles, curves) = self.surf.src.clone()?;
        let (distance, flip) = super::extrude::split_sign(&self.surf.distance);
        let mut p =
          json!({"sketch": sketch.0, "profiles": profiles, "curves": curves, "distance": distance, "flip": flip});
        match self.surf.direction {
          "symmetric" => p["symmetric"] = json!(true),
          "two_sides" => p["distance2"] = json!(self.surf.distance2),
          _ => {}
        }
        if self.surf.start == "offset" {
          p["offset"] = json!(self.surf.offset);
        }
        Some(("surface.extrude", p))
      }
      Kind::SurfRevolve => {
        let (sketch, profiles, curves) = self.surf.src.clone()?;
        let angle = if self.surf.extent == "full" { "360 deg".to_string() } else { self.surf.angle.clone() };
        let mut p = json!({"sketch": sketch.0, "profiles": profiles, "curves": curves, "axis": self.surf_axis()?, "angle": angle});
        match self.surf.direction {
          "symmetric" if self.surf.extent != "full" => p["symmetric"] = json!(true),
          "two_sides" if self.surf.extent != "full" => p["angle2"] = json!(self.surf.angle2),
          _ => {}
        }
        Some(("surface.revolve", p))
      }
      Kind::SurfSweep => {
        let (sketch, profiles, curves) = self.surf.src.clone()?;
        let (path_sketch, path) = self.surf.path.clone()?;
        Some((
          "surface.sweep",
          json!({
            "sketch": sketch.0, "profiles": profiles, "curves": curves, "path": path_sketch.0, "path_curves": path,
            "orientation": self.surf.orientation,
          }),
        ))
      }
      _ => {
        let mut p = json!({"sections": self.surf.sections, "closed": self.surf.closed});
        match (self.surf.guide, self.surf.guides.first()) {
          (_, None) => {}
          ("centerline", Some(c)) => p["centerline"] = c.clone(),
          _ => p["rails"] = json!(self.surf.guides),
        }
        Some(("surface.loft", p))
      }
    }
  }

  /// Revolve's axis as the engine takes it.
  fn surf_axis(&self) -> Option<Value> {
    Some(match self.surf.rows[1].first()? {
      Pick::Edge(b, e) => json!({"body": b, "edge": e}),
      Pick::Curve(sk, c) => json!({"sketch": sk.0, "curve": c}),
      Pick::Axis(f) => json!({"axis": f.0}),
      Pick::Profile(..) => return None,
    })
  }

  /// The sketches whose profiles stay pickable while a surface dialog is open.
  pub(super) fn surf_sketches(&self) -> Vec<FeatureId> {
    let mut out: Vec<FeatureId> = Vec::new();
    for p in self.surf.rows.iter().flatten() {
      if let Pick::Profile(sk, ..) | Pick::Curve(sk, _) = p
        && !out.contains(sk)
      {
        out.push(*sk);
      }
    }
    out
  }

  /// A pick row's chip: lit while it is the row the next picks go to; a click makes it active.
  fn surf_chip(&mut self, ui: &mut egui::Ui, label: &str, row: usize) {
    ui.label(label);
    let n = self.surf.rows[row].len();
    let active = self.surf.row == row;
    let text = if n == 0 { "Select".to_string() } else { format!("{n} selected") };
    let chip = sel_chip_lit(ui, self.accent(), &text, active || n > 0, n > 0);
    if !active && chip.interact(egui::Sense::click()).on_hover_text("Pick for this row").clicked() {
      self.surf.row = row;
    }
    ui.end_row();
  }

  /// The rows of a surface dialog: the Feature tab's, or the Analysis tab (Sweep and Loft).
  pub(super) fn surf_rows(&mut self, ui: &mut egui::Ui) {
    if self.has_tabs() && self.solid.rsl.analysis_tab {
      self.analysis_rows(ui);
      return;
    }
    let mut dirty = false;
    match self.kind {
      Kind::SurfExtrude => {
        self.surf_chip(ui, "Profiles", 0);
        let st = &mut self.surf;
        super::Dialog::check(ui, "Chaining", &mut st.chain, &mut dirty);
        dirty |= combo_row(ui, "Start", "surf-start", &STARTS, &mut st.start);
        if st.start == "offset" {
          dirty |= value_row(ui, "Offset", &mut st.offset);
        }
        dirty |= combo_row(ui, "Direction", "surf-direction", &DIRECTIONS, &mut st.direction);
        let mut kind: &'static str = "distance";
        dirty |= combo_row(ui, "Extent Type", "surf-extent", &EXTENT_TYPES, &mut kind);
        if !self.surf.rows[0].is_empty() {
          let st = &mut self.surf;
          let two = st.direction == "two_sides";
          dirty |= value_row(ui, if two { "Distance 1" } else { "Distance" }, &mut st.distance);
          if two {
            dirty |= value_row(ui, "Distance 2", &mut st.distance2);
          }
          ui.label("Taper Angle");
          let mut t = "0.0 deg".to_string();
          ui.add_enabled_ui(false, |ui| value_edit(ui, &mut t))
            .inner
            .on_disabled_hover_text("FissionOxide can't taper a surface extrude yet");
          ui.end_row();
        }
      }
      Kind::SurfRevolve => {
        self.surf_chip(ui, "Profile", 0);
        super::Dialog::check(ui, "Chaining", &mut self.surf.chain, &mut dirty);
        self.surf_chip(ui, "Axis", 1);
        super::Dialog::check(ui, "Project Axis", &mut self.surf.project_axis, &mut dirty);
        if !self.surf.rows[1].is_empty() {
          let st = &mut self.surf;
          dirty |= combo_row(ui, "Extent Type", "surf-rev-extent", &REVOLVE_EXTENTS, &mut st.extent);
          if st.extent == "partial" {
            dirty |= value_row(ui, "Angle", &mut st.angle);
            dirty |= combo_row(ui, "Direction", "surf-rev-direction", &DIRECTIONS, &mut st.direction);
            if st.direction == "two_sides" {
              dirty |= value_row(ui, "Angle 2", &mut st.angle2);
            }
          }
        }
      }
      Kind::SurfSweep => {
        let mut ty: &'static str = "single";
        dirty |= combo_row(ui, "Type", "surf-sweep-type", &SWEEP_TYPES, &mut ty);
        self.surf_chip(ui, "Profile", 0);
        self.surf_chip(ui, "Path", 1);
        super::Dialog::check(ui, "Chain Selection", &mut self.surf.chain, &mut dirty);
        dirty |= combo_row(ui, "Orientation", "surf-sweep-orient", &ORIENTATIONS, &mut self.surf.orientation);
      }
      _ => {
        self.loft_list(ui, &mut dirty);
        // Fusion's two Guide Type toggles, as the solid Loft's; the guide row under them follows
        // (a switch starts its picks afresh)
        ui.label("Guide Type");
        if icon_row::icon_row(ui, &mut self.surf.guide, &GUIDE_TYPES, "") {
          self.surf.rows[2].clear();
          if self.surf.row == 2 {
            self.surf.row = 0;
          }
          dirty = true;
        }
        ui.end_row();
        self.surf_chip(ui, if self.surf.guide == "centerline" { "Centerline" } else { "Rails" }, 2);
        let st = &mut self.surf;
        super::Dialog::check(ui, "Chain Selection", &mut st.chain, &mut dirty);
        super::Dialog::check(ui, "Closed", &mut st.closed, &mut dirty);
        ui.label("Tangent Edges");
        let edges = [
          icon_row::choice("merge", Icon::Tangent, "Merge Tangent Edges"),
          icon_row::choice("keep", Icon::Line, "Keep Tangent Edges"),
        ];
        dirty |= icon_row::icon_row(ui, &mut st.tangent_edges, &edges, "");
        ui.end_row();
      }
    }
    if dirty {
      self.dirty = true;
    }
  }

  /// Loft's Profiles: one row per section with an x to remove it, and + to pick more.
  fn loft_list(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
    ui.label("Profiles");
    let accent = self.accent();
    let mut remove = None;
    ui.vertical(|ui| {
      ui.horizontal(|ui| {
        if icons::icon_button(ui, Icon::Plus, 20.0, "Add profiles", self.surf.row == 0, None).clicked() {
          self.surf.row = 0;
        }
        if icons::icon_button(ui, Icon::Close, 20.0, "Remove the last profile", false, None).clicked() {
          remove = self.surf.rows[0].len().checked_sub(1);
        }
      });
      if self.surf.rows[0].is_empty() {
        sel_chip_lit(ui, accent, "Select", true, false);
      }
      for (i, _) in self.surf.rows[0].iter().enumerate() {
        ui.horizontal(|ui| {
          ui.label(egui::RichText::new(format!("Profile{}", i + 1)).color(theme::TEXT));
          if icons::icon_button(ui, Icon::Close, 16.0, "Remove this profile", false, None).clicked() {
            remove = Some(i);
          }
        });
      }
    });
    ui.end_row();
    if let Some(i) = remove.filter(|i| *i < self.surf.rows[0].len()) {
      self.surf.rows[0].remove(i);
      *dirty = true;
    }
  }

  /// Draws a surface form's row where Fusion's differs from the form's own; true when drawn.
  pub(super) fn surface_form_field(
    &mut self,
    f: &'static crate::forms::Form,
    i: usize,
    ui: &mut egui::Ui,
    dirty: &mut bool,
  ) -> bool {
    let Some(cmd) = form_cmd(self.kind) else { return false };
    let accent = self.accent();
    let key = f.inputs[i].key();
    let at = |k: &str| f.inputs.iter().position(|x| x.key() == k);
    let count = |d: &Dialog, k: &str| at(k).and_then(|j| d.form.values[j].as_array().map(Vec::len)).unwrap_or(0);
    match (cmd, key) {
      // the active pick row shows its arrow alone, as Fusion's
      ("surface.patch.form", "edges") => {
        ui.label("Boundary Edges");
        let n = count(self, "edges");
        if n == 0 {
          sel_chip_lit(ui, accent, "", true, false);
        } else {
          sel_chip(ui, accent, &format!("{n} selected"), true);
        }
        ui.end_row();
      }
      ("surface.patch.form", "continuity") => {
        let group = at("group").and_then(|j| self.form.values[j].as_bool()).unwrap_or(false);
        let n = if group { 1 } else { count(self, "edges") };
        let mut list: Vec<Value> = self.form.values[i].as_array().cloned().unwrap_or_default();
        list.resize(n, json!("connected"));
        for (k, v) in list.iter_mut().enumerate() {
          ui.label(
            egui::RichText::new(if group { "Edges".to_string() } else { format!("Edge{}", k + 1) }).color(theme::TEXT),
          );
          let mut cur: &'static str =
            CONTINUITY.iter().find(|(c, _)| v.as_str() == Some(c)).map_or("connected", |(c, _)| *c);
          if choice::combo(ui, &format!("patch-cont-{k}"), &CONTINUITY, &mut cur, continuity_icon) {
            *dirty = true;
          }
          *v = json!(cur);
          ui.end_row();
        }
        self.form.values[i] = json!(list);
      }
      ("surface.patch.form", "interior") => {
        ui.label("Interior Rails/Points");
        ui.add_enabled_ui(false, |ui| sel_chip(ui, accent, "Select", false))
          .inner
          .on_disabled_hover_text("Interior rails and points aren't in FissionOxide's Patch dialog yet");
        ui.end_row();
      }
      ("surface.patch.form", "flip") => {
        ui.label("Flip");
        let mut on = self.form.values[i].as_bool().unwrap_or(false);
        if icons::icon_button(ui, Icon::Flip, 24.0, "Flip", on, None).clicked() {
          on = !on;
          *dirty = true;
        }
        self.form.values[i] = json!(on);
        ui.end_row();
      }
      ("surface.ruled.form", "mitered") => {
        super::Dialog::check_greyed(
          ui,
          "Mitered Corners",
          "FissionOxide's ruled surfaces don't miter their corners yet",
        );
      }
      ("surface.offset.form", "faces") => {
        ui.label("Faces / Surface Bodies");
        let picked = self.form.values[i].as_array().map(Vec::len).unwrap_or(0);
        let faces: Vec<Value> = self.form.values[i].as_array().cloned().unwrap_or_default();
        // whole bodies picked on their own (not the bodies of the picked faces)
        let bodies = at("bodies")
          .and_then(|j| self.form.values[j].as_array())
          .map(|b| b.iter().filter(|b| !faces.iter().any(|f| f.get("body") == Some(b))).count())
          .unwrap_or(0);
        let n = picked + bodies;
        sel_chip(ui, accent, &if n == 0 { "Select".to_string() } else { format!("{n} selected") }, n > 0);
        ui.end_row();
      }
      _ => return false,
    }
    true
  }

  /// Fusion's surface dialogs preview without a Preview checkbox.
  pub(super) fn no_preview_box(&self) -> bool {
    form_cmd(self.kind).is_some()
  }

  /// The drag handles: Extrude's distance arrow on the first pick, Revolve's angle dial about
  /// its axis, the Ruled and Offset forms' distance arrow on the first edge or face.
  pub(super) fn surface_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    match self.kind {
      Kind::SurfExtrude => {
        let (o, n) = self.surf_anchor(s)?;
        let (d, flip) = super::extrude::split_sign(&self.surf.distance);
        let v = self.len(s, &d)? * if flip { -1.0 } else { 1.0 };
        Some(vec![Handle::arrow(o, n, v, mm(self.unit, v))])
      }
      Kind::SurfRevolve => {
        let (p, _) = self.surf_anchor(s)?;
        let (o, axis) = self.surf_axis_line(s)?;
        let foot = o + axis * (p - o).dot(axis);
        let zero = (p - foot).try_normalize()?;
        let a = if self.surf.extent == "full" { std::f64::consts::TAU } else { self.ang(s, &self.surf.angle)? };
        Some(vec![Handle::dial(foot, axis, zero, Some((p - foot).length()), a, deg(a))])
      }
      Kind::Form(f) if matches!(f.cmd, "surface.ruled.form" | "surface.offset.form") => {
        let key = if f.cmd == "surface.ruled.form" { "edges" } else { "faces" };
        let first =
          f.inputs.iter().position(|x| x.key() == key).and_then(|j| self.form.values[j].as_array()?.first())?;
        let (o, n) = super::manip::anchor(s, first)?;
        let v = self.len(s, self.form_text("distance")?)?;
        Some(vec![Handle::arrow(o, n, v, mm(self.unit, v))])
      }
      _ => None,
    }
  }

  fn form_text(&self, key: &str) -> Option<&str> {
    let Kind::Form(f) = self.kind else { return None };
    let j = f.inputs.iter().position(|x| x.key() == key)?;
    self.form.values.get(j)?.as_str()
  }

  /// A dragged handle's value.
  pub(super) fn set_surface_handle(&mut self, v: f64, step: f64) -> bool {
    let snapped = if step > 0.0 { (v / step).round() * step } else { v };
    match self.kind {
      Kind::SurfExtrude => self.surf.distance = mm(self.unit, snapped),
      Kind::SurfRevolve => {
        self.surf.extent = "partial";
        self.surf.angle = deg(snapped);
      }
      Kind::Form(_) => {
        let unit = self.unit;
        let Some(t) = self.surface_handle_field() else { return false };
        *t = mm(unit, snapped);
      }
      _ => return false,
    }
    true
  }

  /// The field the handle's floating value box edits.
  pub(super) fn surface_handle_field(&mut self) -> Option<&mut String> {
    match self.kind {
      Kind::SurfExtrude => Some(&mut self.surf.distance),
      Kind::SurfRevolve => Some(&mut self.surf.angle),
      Kind::Form(f) if matches!(f.cmd, "surface.ruled.form" | "surface.offset.form") => {
        let j = f.inputs.iter().position(|x| x.key() == "distance")?;
        match self.form.values.get_mut(j)? {
          Value::String(t) => Some(t),
          _ => None,
        }
      }
      _ => None,
    }
  }

  /// Where the arrow starts: the first picked profile's inside point (or curve's middle) and the
  /// sketch normal.
  fn surf_anchor(&self, s: &Session) -> Option<(DVec3, DVec3)> {
    let r = |sk: &FeatureId| s.model.sketches.get(sk);
    match self.surf.rows[0].first()? {
      Pick::Profile(sk, _, at) => {
        let r = r(sk)?;
        Some((r.plane.to_world(*at), r.plane.normal()))
      }
      Pick::Curve(sk, c) => {
        let res = r(sk)?;
        let segs = res.sketch.curve_segs(CurveId(*c));
        let (a, b) = (segs.first()?.start(), segs.last()?.end());
        Some((res.plane.to_world([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]), res.plane.normal()))
      }
      _ => None,
    }
  }

  /// Revolve's axis as a point and a unit direction.
  fn surf_axis_line(&self, s: &Session) -> Option<(DVec3, DVec3)> {
    match self.surf.rows[1].first()? {
      Pick::Edge(b, e) => {
        let e = s.model.body(fox_engine::model::BodyId(*b))?.edges().iter().find(|x| x.id == *e).copied()?;
        Some((e.start, (e.end - e.start).try_normalize()?))
      }
      Pick::Curve(sk, c) => {
        let r = s.model.sketches.get(sk)?;
        let segs = r.sketch.curve_segs(CurveId(*c));
        let (a, b) = (r.plane.to_world(segs.first()?.start()), r.plane.to_world(segs.last()?.end()));
        Some((a, (b - a).try_normalize()?))
      }
      Pick::Axis(f) => s.model.axes.get(f).copied(),
      Pick::Profile(..) => None,
    }
  }
}

/// Patch's picked boundaries on the canvas, as Fusion draws them: each pick's loop bold black
/// with an "Edge N" label (N the pick's row in the continuity table).
pub(crate) fn patch_overlay(app: &mut App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<egui::Pos2>) {
  let Some(d) = app.dialog.as_mut() else { return };
  let Kind::Form(f) = d.kind else { return };
  if f.cmd != "surface.patch.form" {
    return;
  }
  let get = |k: &str| f.inputs.iter().position(|x| x.key() == k).map(|i| d.form.values[i].clone());
  let key = json!([get("edges"), get("chain")]);
  if d.surf.outline.as_ref().is_none_or(|o| o.0 != key) {
    let chain = get("chain").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut loops = Vec::new();
    for e in get("edges").and_then(|v| v.as_array().cloned()).unwrap_or_default() {
      let (Some(b), Some(id)) = (e["body"].as_u64(), e["edge"].as_u64()) else { continue };
      let Some(body) = app.s.model.body(fox_engine::model::BodyId(b as u32)) else { continue };
      let ids = if chain { fox_engine::chain_boundary(body.edges(), id as u32) } else { vec![id as u32] };
      let lines = body.shape.edge_lines(0.05).unwrap_or_default();
      loops.push(lines.into_iter().filter(|l| ids.contains(&l.id)).map(|l| l.points).collect());
    }
    d.surf.outline = Some((key, loops));
  }
  let Some((_, loops)) = d.surf.outline.as_ref() else { return };
  let ink = egui::Stroke::new(3.0, egui::Color32::BLACK);
  for (k, lines) in loops.iter().enumerate() {
    for l in lines {
      let pts: Vec<egui::Pos2> = l.iter().filter_map(|p| project(*p)).collect();
      if pts.len() == l.len() && pts.len() > 1 {
        painter.add(egui::Shape::line(pts, ink));
      }
    }
    let Some(at) = lines.first().and_then(|l| l.get(l.len() / 2)).and_then(|p| project(*p)) else { continue };
    let font = egui::FontId::proportional(12.0);
    let text = format!("Edge {}", k + 1);
    let galley = painter.layout_no_wrap(text, font, theme::TEXT);
    let r = egui::Rect::from_min_size(at + egui::vec2(8.0, -22.0), galley.size() + egui::vec2(8.0, 4.0));
    painter.rect(r, 2.0, egui::Color32::WHITE, egui::Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
    painter.galley(r.min + egui::vec2(4.0, 2.0), galley, theme::TEXT);
  }
}

fn continuity_icon(key: &str) -> Option<Icon> {
  match key {
    "tangent" => Some(Icon::Tangent),
    _ => None,
  }
}

fn combo_row(
  ui: &mut egui::Ui,
  label: &str,
  id: &str,
  items: &[(&'static str, &'static str)],
  cur: &mut &'static str,
) -> bool {
  ui.label(label);
  let changed = choice::combo(ui, id, items, cur, choice::extrude_icon);
  ui.end_row();
  changed
}

fn value_row(ui: &mut egui::Ui, label: &str, v: &mut String) -> bool {
  ui.label(label);
  let changed = value_edit(ui, v).changed();
  ui.end_row();
  changed
}

#[cfg(test)]
mod tests {
  use fox_engine::model::FeatureKind;
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::super::{Dialog, Kind};
  use super::GUIDE_TYPES;
  use crate::App;

  fn sketch(app: &mut App, cmd: &str, p: Value) -> fox_engine::model::FeatureId {
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let sk = app.s.edit_sketch.unwrap();
    app.run(cmd, p).unwrap();
    app.run("sketch.finish", json!({})).unwrap();
    app.s.selection.clear();
    sk
  }

  fn last_kind(app: &App) -> FeatureKind {
    app.s.design.features.last().unwrap().kind.clone()
  }

  #[test]
  fn extrude_opens_on_the_profile_and_waits_for_a_distance() {
    let mut app = App::with_session(Session::default());
    let sk = sketch(&mut app, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let mut d = Dialog::open(&mut app, Kind::SurfExtrude);
    assert_eq!(d.heading(), "EXTRUDE");
    assert_eq!(d.surf.rows[0].len(), 1, "the only profile is picked");
    assert!(!d.ready() && !d.ok_enabled(), "OK waits for a distance");
    assert!(d.handles(&app.s).len() == 1, "the arrow shows on the profile");
    assert_eq!(d.surface_handle_field().cloned().as_deref(), Some("0 mm"));
    *d.surface_handle_field().unwrap() = "5 mm".into();
    d.value_box_edited();
    assert!(d.ok_enabled());
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "surface.extrude");
    assert_eq!(p["sketch"], json!(sk.0));
    assert_eq!(p["profiles"].as_array().unwrap().len(), 1);
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let b = app.s.model.bodies.last().unwrap();
    assert_eq!(b.shape.solid_count(), 0, "a surface");
    // two sides, from an offset, dragged the other way
    d.surf.direction = "two_sides";
    d.surf.distance2 = "2 mm".into();
    d.surf.start = "offset";
    d.surf.offset = "1 mm".into();
    d.set_surface_handle(-3.0, 0.0);
    let (_, p) = d.command().unwrap();
    assert_eq!(
      (p["flip"].clone(), p["distance2"].clone(), p["offset"].clone()),
      (json!(true), json!("2 mm"), json!("1 mm"))
    );
    d.cancel(&mut app.s);
  }

  #[test]
  fn revolve_opens_with_the_axis_row_active() {
    let mut app = App::with_session(Session::default());
    sketch(&mut app, "sketch.rect", json!({"corner1": [5, 0], "corner2": [10, 4]}));
    let mut d = Dialog::open(&mut app, Kind::SurfRevolve);
    assert_eq!(d.surf.rows[0].len(), 1);
    assert_eq!(d.surf.row, 1, "Axis is the active row");
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select axis"));
    // a construction axis along Y
    app.run("axis", json!({"method": "two_points", "a": {"at": [0, 0, 0]}, "b": {"at": [0, 10, 0]}})).unwrap();
    let axis = *app.s.model.axes.keys().next().unwrap();
    app.s.selection.push(Sel::Feature { feature: axis });
    d.sync_selection(&app.s);
    assert_eq!(d.surf.rows[1].len(), 1);
    assert!(d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Adjust angle value or modify the selection"));
    assert_eq!(d.handles(&app.s).len(), 1, "the angle dial");
    let (cmd, p) = d.command().unwrap();
    assert_eq!(
      (cmd, p["axis"].clone(), p["angle"].clone()),
      ("surface.revolve", json!({"axis": axis.0}), json!("360 deg"))
    );
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    d.cancel(&mut app.s);
  }

  #[test]
  fn sweep_routes_the_profile_then_the_path() {
    let mut app = App::with_session(Session::default());
    let path = sketch(&mut app, "sketch.line", json!({"points": [[0, 0], [10, 0], [10, 10]]}));
    app.run("sketch.create", json!({"plane": {"origin": [0, 0, 0], "normal": [1, 0, 0], "x": [0, 0, 1]}})).unwrap();
    let prof = app.s.edit_sketch.unwrap();
    app.run("sketch.line", json!({"points": [[-2, 0], [2, 0]]})).unwrap();
    app.run("sketch.finish", json!({})).unwrap();
    app.s.selection.clear();
    let mut d = Dialog::open(&mut app, Kind::SurfSweep);
    assert!(d.has_tabs());
    assert!(d.surf.rows[0].is_empty() && d.surf.row == 0);
    let curve = |sk: fox_engine::model::FeatureId, app: &App, i: usize| {
      let c = app.s.design.sketch(sk).unwrap().curves.iter().filter(|c| !c.reference()).nth(i).unwrap().id;
      Sel::SketchCurve { sketch: sk, curve: c }
    };
    app.s.selection.push(curve(prof, &app, 0));
    d.sync_selection(&app.s);
    assert_eq!((d.surf.rows[0].len(), d.surf.row), (1, 1), "the path row is next");
    // one path curve picked: Chain Selection brings the other
    app.s.selection.push(curve(path, &app, 0));
    d.sync_selection(&app.s);
    assert!(d.ok_enabled());
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "surface.sweep");
    assert_eq!(p["path_curves"].as_array().unwrap().len(), 2, "{p}");
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    d.cancel(&mut app.s);
  }

  /// `ui:surface:sweep-loft-missing`: LOFT opens with Chain Selection unchecked (SWEEP's is
  /// checked) and Guide Type as two icon toggles, rails and centerline, the guide row following.
  #[test]
  fn loft_opens_unchained_with_two_guide_toggles() {
    let mut app = App::with_session(Session::default());
    assert!(Dialog::open(&mut app, Kind::SurfSweep).surf.chain, "Sweep chains");
    let mut d = Dialog::open(&mut app, Kind::SurfLoft);
    assert!(!d.surf.chain, "Loft's Chain Selection starts unchecked");
    assert!(GUIDE_TYPES.iter().all(|c| c.enabled), "both Guide Type icons are toggles");
    assert_eq!(GUIDE_TYPES.map(|c| c.key), ["rails", "centerline"]);
    assert_eq!(d.surf.guide, "rails");
    let ctx = egui::Context::default();
    let texts = |d: &mut Dialog| {
      let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))),
        ..Default::default()
      };
      let out = ctx.run_ui(raw, |ui| {
        egui::Grid::new("t").num_columns(2).show(ui, |ui| d.surf_rows(ui));
      });
      let mut t = Vec::new();
      fn walk(s: &egui::Shape, out: &mut Vec<String>) {
        match s {
          egui::Shape::Text(x) => out.push(x.galley.text().to_string()),
          egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
          _ => {}
        }
      }
      out.shapes.iter().for_each(|c| walk(&c.shape, &mut t));
      out.drop_without_applying_deltas();
      t
    };
    let t = texts(&mut d);
    assert!(t.iter().any(|x| x == "Rails") && !t.iter().any(|x| x == "Centerline"), "{t:?}");
    d.surf.guide = "centerline";
    let t = texts(&mut d);
    assert!(t.iter().any(|x| x == "Centerline") && !t.iter().any(|x| x == "Rails"), "{t:?}");
    d.cancel(&mut app.s);
  }

  #[test]
  fn loft_takes_a_profile_from_each_sketch() {
    let mut app = App::with_session(Session::default());
    let a = sketch(&mut app, "sketch.circle", json!({"center": [0, 0], "radius": 5}));
    app.run("plane.offset", json!({"plane": "xy", "offset": 10})).unwrap();
    let pl = app.s.design.features.last().unwrap().id.0;
    app.run("sketch.create", json!({"plane": {"feature": pl}})).unwrap();
    let b = app.s.edit_sketch.unwrap();
    app.run("sketch.circle", json!({"center": [0, 0], "radius": 3})).unwrap();
    app.run("sketch.finish", json!({})).unwrap();
    app.s.selection.clear();
    let mut d = Dialog::open(&mut app, Kind::SurfLoft);
    assert!(!d.ok_enabled());
    app.s.selection = vec![Sel::Profile { sketch: a, index: 0 }, Sel::Profile { sketch: b, index: 0 }];
    d.sync_selection(&app.s);
    assert!(d.ok_enabled());
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "surface.loft");
    assert_eq!(p["sections"].as_array().unwrap().len(), 2);
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!(matches!(last_kind(&app), FeatureKind::SurfaceLoft { .. }));
    d.cancel(&mut app.s);
  }

  /// `ui:surface:sweep-loft-missing`: LOFT's Rails row takes sketch curves (its chip active once
  /// clicked) and the surface follows them; Guide Type Centerline takes a centre line instead.
  #[test]
  fn loft_follows_rails_or_a_centerline() {
    let mut app = App::with_session(Session::default());
    // two 20 long lines, 40 apart, and on XZ two bowed rails from end to end
    let a = sketch(&mut app, "sketch.line", json!({"points": [[0, 0], [20, 0]]}));
    app.run("plane.offset", json!({"plane": "xy", "offset": 40})).unwrap();
    let pl = app.s.design.features.last().unwrap().id.0;
    app.run("sketch.create", json!({"plane": {"feature": pl}})).unwrap();
    let b = app.s.edit_sketch.unwrap();
    app.run("sketch.line", json!({"points": [[0, 0], [20, 0]]})).unwrap();
    app.run("sketch.finish", json!({})).unwrap();
    app.run("sketch.create", json!({"plane": {"origin": [0, 0, 0], "normal": [0, -1, 0], "x": [1, 0, 0]}})).unwrap();
    let r = app.s.edit_sketch.unwrap();
    app.run("sketch.arc", json!({"start": [0, 0], "mid": [-5, 20], "end": [0, 40]})).unwrap();
    app.run("sketch.arc", json!({"start": [20, 0], "mid": [25, 20], "end": [20, 40]})).unwrap();
    app.run("sketch.line", json!({"points": [[10, 0], [10, 40]]})).unwrap();
    app.run("sketch.finish", json!({})).unwrap();
    app.s.selection.clear();
    let curve = |sk: fox_engine::model::FeatureId, app: &App, i: usize| {
      let c = app.s.design.sketch(sk).unwrap().curves.iter().filter(|c| !c.reference()).nth(i).unwrap().id;
      Sel::SketchCurve { sketch: sk, curve: c }
    };
    let mut d = Dialog::open(&mut app, Kind::SurfLoft);
    app.s.selection = vec![curve(a, &app, 0), curve(b, &app, 0)];
    d.sync_selection(&app.s);
    assert_eq!(d.surf.rows[0].len(), 2);
    // the Rails chip clicked: the next curves are rails
    d.surf.row = 2;
    app.s.selection.extend([curve(r, &app, 0), curve(r, &app, 1)]);
    d.sync_selection(&app.s);
    assert_eq!((d.surf.rows[0].len(), d.surf.rows[2].len()), (2, 2));
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "surface.loft");
    assert_eq!(p["rails"].as_array().map(Vec::len), Some(2), "{p}");
    assert!(p.get("centerline").is_none());
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let FeatureKind::SurfaceLoft { rails, .. } = last_kind(&app) else { panic!() };
    assert_eq!(rails.len(), 2);
    let m = app.s.model.bodies.last().unwrap().shape.mass().unwrap();
    assert!(m.bbox_min.x < -4.0 && m.bbox_max.x > 24.0, "the surface bows out to the rails: {m:?}");
    // Guide Type Centerline: the rails go and the centre line is the path
    d.surf.guide = "centerline";
    d.surf.rows[2].clear();
    app.s.selection.retain(|x| !matches!(x, Sel::SketchCurve { sketch, .. } if *sketch == r));
    d.sync_selection(&app.s);
    d.surf.row = 2;
    app.s.selection.push(curve(r, &app, 2));
    d.sync_selection(&app.s);
    let (_, p) = d.command().unwrap();
    assert!(p.get("rails").is_none(), "{p}");
    assert_eq!(p["centerline"]["sketch"], json!(r.0), "{p}");
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!(matches!(last_kind(&app), FeatureKind::SurfaceLoft { centerline: Some(_), .. }));
    d.cancel(&mut app.s);
  }

  /// A cone narrowing from radius 10 at z = 0 to 5 at z = 10, open at both ends, selecting
  /// nothing.
  fn cone(app: &mut App) {
    app.run("sketch.create", json!({"plane": {"origin": [0, 0, 0], "normal": [0, -1, 0], "x": [1, 0, 0]}})).unwrap();
    app.run("sketch.line", json!({"points": [[10, 0], [5, 10]]})).unwrap();
    app.run("surface.revolve", json!({"axis": "z"})).unwrap();
    app.s.selection.clear();
  }

  fn rim(app: &App) -> Sel {
    let b = app.s.model.bodies.last().unwrap();
    let e = b.edges().iter().find(|e| (e.center.z - 10.0).abs() < 1e-6 && e.radius > 4.0).unwrap();
    Sel::Edge { body: b.id, edge: e.id }
  }

  fn row_shown(d: &Dialog, key: &str) -> bool {
    let Kind::Form(f) = d.kind else { panic!() };
    let i = f.inputs.iter().position(|x| x.key() == key).unwrap();
    d.form.shown(f, i)
  }

  fn set(d: &mut Dialog, key: &str, v: Value) {
    let Kind::Form(f) = d.kind else { panic!() };
    let i = f.inputs.iter().position(|x| x.key() == key).unwrap();
    d.form.values[i] = v;
  }

  #[test]
  fn patch_has_tabs_no_preview_box_and_a_continuity_per_boundary() {
    let mut app = App::with_session(Session::default());
    cone(&mut app);
    let mut d = Dialog::open(&mut app, Kind::Form(&crate::forms::PATCH));
    assert!(d.has_tabs() && d.no_preview_box());
    assert_eq!(d.prompt(false).as_deref(), Some("Select edge"));
    assert!(!row_shown(&d, "continuity"), "the table waits for a boundary");
    assert!(!d.ok_enabled());
    app.s.selection = vec![rim(&app)];
    d.sync_selection(&app.s);
    assert!(row_shown(&d, "continuity") && d.ok_enabled());
    set(&mut d, "continuity", json!(["tangent"]));
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "surface.patch");
    assert_eq!((p["chain"].clone(), p["continuity"].clone()), (json!(true), json!(["tangent"])));
    // Group Edges: one continuity for every edge
    set(&mut d, "group", json!(true));
    assert_eq!(d.command().unwrap().1["continuity"], json!("tangent"));
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let disc = std::f64::consts::PI * 25.0;
    assert!(app.s.model.bodies.last().unwrap().shape.mass().unwrap().area > disc + 5.0, "a dome");
    d.cancel(&mut app.s);
  }

  /// Patch draws each picked boundary on the canvas, the whole loop with Enable Chaining on.
  #[test]
  fn patch_outlines_the_picked_boundary_loop() {
    let mut app = App::with_session(Session::default());
    sketch(&mut app, "sketch.line", json!({"points": [[0, 0], [20, 0]]}));
    app.run("surface.extrude", json!({"distance": 5})).unwrap();
    app.s.selection.clear();
    app.dialog = Some(Dialog::open(&mut app, Kind::Form(&crate::forms::PATCH)));
    let b = app.s.model.bodies[0].id;
    let top = app.s.model.bodies[0].edges().iter().find(|e| (e.mid.z - 5.0).abs() < 1e-6).unwrap().id;
    app.s.selection = vec![Sel::Edge { body: b, edge: top }];
    let (d, s) = (app.dialog.as_mut().unwrap(), &app.s);
    d.sync_selection(s);
    let ctx = egui::Context::default();
    // (fonts load on the first frame)
    ctx.run_ui(egui::RawInput::default(), |_| {}).drop_without_applying_deltas();
    let painter = egui::Painter::new(ctx, egui::LayerId::background(), egui::Rect::EVERYTHING);
    let project = |p: fox_engine::kernel::glam::DVec3| Some(egui::pos2(p.x as f32, p.y as f32));
    super::patch_overlay(&mut app, &painter, &project);
    let loops = &app.dialog.as_ref().unwrap().surf.outline.as_ref().unwrap().1;
    assert_eq!(loops.len(), 1, "one boundary picked");
    assert_eq!(loops[0].len(), 4, "the strip's whole boundary");
  }

  #[test]
  fn ruled_rows_wait_for_an_edge_and_ok_for_a_distance() {
    let mut app = App::with_session(Session::default());
    sketch(&mut app, "sketch.line", json!({"points": [[0, 0], [20, 0]]}));
    app.run("surface.extrude", json!({"distance": 5})).unwrap();
    app.s.selection.clear();
    let mut d = Dialog::open(&mut app, Kind::Form(&crate::forms::RULED));
    assert!(d.has_tabs() && d.no_preview_box());
    assert!(!row_shown(&d, "distance") && !row_shown(&d, "angle"));
    let b = app.s.model.bodies[0].id;
    let top = app.s.model.bodies[0].edges().iter().find(|e| (e.mid.z - 5.0).abs() < 1e-6).unwrap().id;
    app.s.selection = vec![Sel::Edge { body: b, edge: top }];
    d.sync_selection(&app.s);
    assert!(row_shown(&d, "distance") && row_shown(&d, "angle"));
    assert!(!d.ok_enabled(), "OK waits for a distance");
    assert_eq!(d.prompt(false).as_deref(), Some("Adjust distance and angle or modify selection"));
    assert_eq!(d.handles(&app.s).len(), 1, "the distance arrow");
    d.set_handle(&app.s, 0, 4.0, 0.0);
    assert!(d.ok_enabled());
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, p["kind"].clone(), p["distance"].clone()), ("surface.ruled", json!("normal"), json!("4 mm")));
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!(matches!(last_kind(&app), FeatureKind::RuledSurface { .. }));
    d.cancel(&mut app.s);
  }

  #[test]
  fn offset_takes_faces_or_bodies_and_its_offset_type() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10, "at": [5, 5]})).unwrap();
    app.s.selection.clear();
    let mut d = Dialog::open(&mut app, Kind::Form(&crate::forms::SURFACE_OFFSET));
    assert!(!d.has_tabs() && d.no_preview_box());
    assert_eq!(d.prompt(false).as_deref(), Some("Select faces or surface bodies to offset"));
    assert!(!row_shown(&d, "distance") && !d.ok_enabled());
    let b = &app.s.model.bodies[0];
    let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    app.s.selection = vec![Sel::Face { body: b.id, face: top }];
    d.sync_selection(&app.s);
    assert!(row_shown(&d, "distance") && d.ok_enabled(), "OK once something is picked");
    assert_eq!(d.handles(&app.s).len(), 1);
    *d.surface_handle_field().unwrap() = "2 mm".into();
    let (cmd, p) = d.command().unwrap();
    assert_eq!(cmd, "surface.offset");
    assert!(p.get("bodies").is_none(), "the face's body isn't offset whole: {p}");
    assert_eq!((p["corners"].clone(), p["tangent_chain"].clone()), (json!("sharp"), json!(true)));
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    d.cancel(&mut app.s);
  }
}
