//! Sketch constraints as Fusion's pick commands. A CONSTRAINTS entry starts [`Tool::Constrain`]:
//! its icon lights up, the cursor prompt asks for sketch objects, each click adds a point or
//! curve to the selection (one the constraint can't use is refused silently), the constraint
//! applies on the last pick it needs and the command stays active for the next one until Escape.
//! A selection made before the entry serves as its first picks, and picking another entry
//! changes the constraint type without leaving the command. Fix/UnFix applies on every pick:
//! picking fixed geometry again unfixes it.

use fox_engine::Sel;
use fox_engine::model::FeatureId;
use fox_engine::scene::Pick;
use fox_engine::sketch::{ConstraintKind, CurveId, CurveKind, PointId, Sketch};
use serde_json::json;

use crate::App;
use crate::viewport::Tool;

/// Fusion's cursor prompts.
pub const PROMPT_START: &str = "Select sketch objects or change constraint type";
pub const PROMPT_START_HV: &str = "Select sketch objects or constraint type";
pub const PROMPT_MORE: &str = "Select other geometries";
pub const PROMPT_CONSTRAIN: &str = "Select sketch objects to constrain";
pub const PROMPT_SYMMETRY_SECOND: &str = "Select second sketch element";
pub const PROMPT_SYMMETRY_LINE: &str = "Select a symmetry line";
pub const PROMPT_POLYGON: &str = "Select a closed profile with equal sides and angles to constrain";

/// The cursor prompt of constraint `kind` with `picks` picked so far. `any`: something was
/// picked since the command started (Fix/UnFix and Curvature keep "to constrain" after that).
pub fn prompt(kind: &str, picks: usize, any: bool) -> &'static str {
  match (kind, picks) {
    ("polygon", _) => PROMPT_POLYGON,
    ("fix", _) if any => PROMPT_CONSTRAIN,
    ("hv", 0) => PROMPT_START_HV,
    (_, 0) => PROMPT_START,
    ("curvature", _) => PROMPT_CONSTRAIN,
    ("symmetric", 1) => PROMPT_SYMMETRY_SECOND,
    ("symmetric", _) => PROMPT_SYMMETRY_LINE,
    _ => PROMPT_MORE,
  }
}

/// A pick: a point or a curve of the sketch being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
  Point(PointId),
  Curve(CurveId),
}

impl Item {
  fn sel(self, sk: FeatureId) -> Sel {
    match self {
      Item::Point(point) => Sel::SketchPoint { sketch: sk, point },
      Item::Curve(curve) => Sel::SketchCurve { sketch: sk, curve },
    }
  }
}

/// The command's picks: the sketch points and curves in the selection, in pick order.
fn picks(app: &App, sk: FeatureId) -> Vec<Item> {
  app
    .s
    .selection
    .iter()
    .filter_map(|s| match s {
      Sel::SketchPoint { sketch, point } if *sketch == sk => Some(Item::Point(*point)),
      Sel::SketchCurve { sketch, curve } if *sketch == sk => Some(Item::Curve(*curve)),
      _ => None,
    })
    .collect()
}

fn set_picks(app: &mut App, sk: FeatureId, items: &[Item]) {
  app.s.selection = items.iter().map(|i| i.sel(sk)).collect();
  app.tstate.con_picks = items.len();
}

/// Starts constraint `kind` (or switches the running command to it). The sketch geometry already
/// selected is the first picks, so a selection followed by the entry applies at once, as in
/// Fusion.
pub fn start(app: &mut App, kind: &'static str) {
  let Some(sk) = app.s.edit_sketch else { return };
  app.set_tool(Tool::Constrain(kind));
  let pre = picks(app, sk);
  // Polygon: lines selected beforehand are taken as the profile
  if kind == "polygon" {
    let curves: Vec<u32> = pre.iter().filter_map(|i| if let Item::Curve(c) = i { Some(c.0) } else { None }).collect();
    set_picks(app, sk, &[]);
    if curves.len() >= 3 {
      let before = app.status.clone();
      if app.run("sketch.constrain", json!({"sketch": sk.0, "type": "polygon", "curves": curves})).is_none() {
        app.status = before;
        app.forget_run_error();
      }
    }
    return;
  }
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  // Fix/UnFix takes the whole selection; the others keep what they can use, in order
  let mut kept: Vec<Item> = Vec::new();
  for item in pre {
    if kind == "fix" || accepts(sketch, kind, &kept, item) {
      kept.push(item);
    }
  }
  set_picks(app, sk, &kept);
  try_apply(app, sk, kind);
}

/// A click in the command: the hovered point or curve becomes a pick (a click on nothing, or on
/// what the constraint can't use, picks nothing and says nothing); picking a pick again drops
/// it. Polygon takes a closed profile.
pub fn click(app: &mut App, kind: &'static str) {
  let Some(sk) = app.s.edit_sketch else { return };
  let item = match app.hover {
    Some(Pick::SketchPoint { sketch, point }) if sketch == sk => Item::Point(point),
    Some(Pick::SketchCurve { sketch, curve }) if sketch == sk => Item::Curve(curve),
    Some(Pick::Profile { sketch, index }) if sketch == sk && kind == "polygon" => {
      profile_pick(app, sk, index);
      return;
    }
    _ => return,
  };
  let mut cur = picks(app, sk);
  if let Some(i) = cur.iter().position(|p| *p == item) {
    cur.remove(i);
    set_picks(app, sk, &cur);
    return;
  }
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  if !accepts(sketch, kind, &cur, item) {
    return;
  }
  app.tstate.con_any = true;
  cur.push(item);
  set_picks(app, sk, &cur);
  try_apply(app, sk, kind);
}

/// Whether `item` can join `picks` for constraint `kind`: the kinds of geometry it takes, in
/// the order Fusion takes them, and no more than it needs.
fn accepts(sketch: &Sketch, kind: &str, picks: &[Item], item: Item) -> bool {
  if picks.contains(&item) {
    return false;
  }
  let n_pts = picks.iter().filter(|i| matches!(i, Item::Point(_))).count();
  let n_cur = picks.len() - n_pts;
  let is_line = |c: CurveId| sketch.curve(c).is_some_and(|c| matches!(c.kind, CurveKind::Line { .. }));
  match (kind, item) {
    ("fix", _) => true,
    // a line, or two points
    ("hv", Item::Point(_)) => n_cur == 0 && n_pts < 2,
    ("hv", Item::Curve(c)) => picks.is_empty() && is_line(c),
    // two points, or a point and a curve
    ("coincident", Item::Point(_)) => n_pts < 2 && picks.len() < 2,
    ("coincident", Item::Curve(_)) => n_cur == 0 && picks.len() < 2,
    ("midpoint", Item::Point(_)) => n_pts == 0,
    ("midpoint", Item::Curve(c)) => n_cur == 0 && is_line(c),
    // two points or two curves, then the symmetry line
    ("symmetric", Item::Point(_)) => n_cur == 0 && n_pts < 2,
    ("symmetric", Item::Curve(c)) => match picks.len() {
      0 => true,
      1 => n_cur == 1,
      2 => is_line(c),
      _ => false,
    },
    // a closed profile, never single curves
    ("polygon", _) => false,
    // tangent, equal, parallel, perpendicular, collinear, concentric, curvature: two curves
    (_, Item::Curve(_)) => n_pts == 0 && n_cur < 2,
    (_, Item::Point(_)) => false,
  }
}

/// Whether `picks` are all constraint `kind` needs.
fn complete(kind: &str, picks: &[Item]) -> bool {
  let n_pts = picks.iter().filter(|i| matches!(i, Item::Point(_))).count();
  match kind {
    "fix" => !picks.is_empty(),
    "hv" => n_pts == 2 || (n_pts == 0 && picks.len() == 1),
    "symmetric" => picks.len() == 3,
    "polygon" => false,
    _ => picks.len() == 2,
  }
}

/// Applies the constraint once its picks are complete. A pick the solver refuses is dropped
/// without a message (Fusion leaves none); the status keeps its text.
fn try_apply(app: &mut App, sk: FeatureId, kind: &str) {
  let cur = picks(app, sk);
  if !complete(kind, &cur) {
    return;
  }
  let before = app.status.clone();
  if apply(app, sk, kind, &cur) {
    app.tstate.con_any = true;
    set_picks(app, sk, &[]);
  } else {
    app.status = before;
    app.forget_run_error();
    let mut cur = cur;
    cur.pop();
    set_picks(app, sk, &cur);
  }
}

/// Runs the constraint's command(s) for `picks`. Returns whether every one succeeded.
fn apply(app: &mut App, sk: FeatureId, kind: &str, picks: &[Item]) -> bool {
  let points: Vec<u32> = picks.iter().filter_map(|i| if let Item::Point(p) = i { Some(p.0) } else { None }).collect();
  let curves: Vec<u32> = picks.iter().filter_map(|i| if let Item::Curve(c) = i { Some(c.0) } else { None }).collect();
  let Ok(sketch) = app.s.design.sketch(sk) else { return false };
  let jobs: Vec<serde_json::Value> = match kind {
    // Horizontal/Vertical: whichever is closer, per line or for the point pair
    "hv" => {
      let closer = |a: [f64; 2], b: [f64; 2]| {
        if (b[0] - a[0]).abs() >= (b[1] - a[1]).abs() { "horizontal" } else { "vertical" }
      };
      if points.len() == 2 {
        let (Some(a), Some(b)) = (sketch.pos(PointId(points[0])), sketch.pos(PointId(points[1]))) else { return false };
        vec![json!({"sketch": sk.0, "type": closer(a, b), "points": points})]
      } else {
        curves
          .iter()
          .filter_map(|c| {
            let ends = sketch.curve_points(CurveId(*c));
            let (a, b) = (sketch.pos(*ends.first()?)?, sketch.pos(*ends.get(1)?)?);
            Some(json!({"sketch": sk.0, "type": closer(a, b), "curves": [c]}))
          })
          .collect()
      }
    }
    // Fix/UnFix: geometry that's already fixed is unfixed
    "fix" => {
      if unfix(app, sk, &points, &curves) {
        return true;
      }
      let mut jobs: Vec<serde_json::Value> =
        points.iter().map(|p| json!({"sketch": sk.0, "type": "fix", "points": [p]})).collect();
      if !curves.is_empty() {
        jobs.push(json!({"sketch": sk.0, "type": "fix", "curves": curves}));
      }
      jobs
    }
    // a point on a curve is Fusion's Coincident too
    "coincident" if points.len() == 1 => {
      vec![json!({"sketch": sk.0, "type": "point_on_curve", "points": points, "curves": curves})]
    }
    _ => vec![json!({"sketch": sk.0, "type": kind, "points": points, "curves": curves})],
  };
  if jobs.is_empty() {
    return false;
  }
  let mut all = true;
  for params in jobs {
    all &= app.run("sketch.constrain", params).is_some();
  }
  all
}

/// Polygon: a click on a closed profile constrains its curves as a regular polygon (a profile
/// that isn't one is refused without a message).
fn profile_pick(app: &mut App, sk: FeatureId, index: usize) {
  let mut curves: Vec<u32> = app
    .s
    .model
    .sketches
    .get(&sk)
    .and_then(|r| r.profiles.get(index))
    .map(|p| p.outer.src.iter().map(|s| s.curve.0).collect())
    .unwrap_or_default();
  curves.dedup();
  app.tstate.con_any = true;
  if curves.len() < 3 {
    return;
  }
  let before = app.status.clone();
  if app.run("sketch.constrain", json!({"sketch": sk.0, "type": "polygon", "curves": curves})).is_none() {
    app.status = before;
    app.forget_run_error();
  }
  set_picks(app, sk, &[]);
}

/// UnFix: when every selected point, and every point of each selected curve, is fixed, deletes
/// those Fix constraints (and a selected circle's fixed radius). Returns whether it did.
pub(crate) fn unfix(app: &mut App, sk: FeatureId, points: &[u32], curves: &[u32]) -> bool {
  let Ok(sketch) = app.s.design.sketch(sk) else { return false };
  let mut pts: Vec<PointId> = points.iter().map(|p| PointId(*p)).collect();
  for c in curves {
    pts.extend(sketch.curve_points(CurveId(*c)));
  }
  if pts.is_empty() {
    return false;
  }
  let fix_of = |p: PointId| {
    sketch.constraints.iter().find(|c| matches!(c.kind, ConstraintKind::Fix { p: q, .. } if q == p)).map(|c| c.id.0)
  };
  // points pinned by the sketch itself (its origin) count as fixed but have nothing to delete
  let pinned = |p: PointId| sketch.point(p).is_some_and(|pt| pt.fixed);
  if !pts.iter().all(|p| fix_of(*p).is_some() || pinned(*p)) {
    return false;
  }
  let mut cons: Vec<u32> = pts.iter().filter_map(|p| fix_of(*p)).collect();
  if cons.is_empty() {
    return false;
  }
  cons.extend(sketch.constraints.iter().filter_map(|c| match c.kind {
    ConstraintKind::FixRadius { curve, .. } if curves.contains(&curve.0) => Some(c.id.0),
    _ => None,
  }));
  cons.sort_unstable();
  cons.dedup();
  app.run("sketch.delete", json!({"sketch": sk.0, "constraints": cons})).is_some()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::panels::Cmd;
  use fox_engine::Session;

  fn sketching() -> (App, FeatureId) {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    let sk = app.s.edit_sketch.unwrap();
    (app, sk)
  }

  fn line(app: &mut App, sk: FeatureId, a: [f64; 2], b: [f64; 2]) -> CurveId {
    let r = app.run("sketch.line", json!({"sketch": sk.0, "points": [a, b]})).unwrap();
    CurveId(r["curves"][0].as_u64().unwrap() as u32)
  }

  fn ends(app: &App, sk: FeatureId, c: CurveId) -> (PointId, PointId) {
    let pts = app.s.design.sketch(sk).unwrap().curve_points(c);
    (pts[0], pts[1])
  }

  fn hover_curve(app: &mut App, sk: FeatureId, c: CurveId) {
    app.hover = Some(Pick::SketchCurve { sketch: sk, curve: c });
  }

  fn hover_point(app: &mut App, sk: FeatureId, p: PointId) {
    app.hover = Some(Pick::SketchPoint { sketch: sk, point: p });
  }

  fn kinds(app: &App, sk: FeatureId) -> Vec<String> {
    app.s.design.sketch(sk).unwrap().constraints.iter().map(|c| c.kind.name().to_string()).collect()
  }

  fn no_error(app: &App) {
    assert!(app.status.as_ref().is_none_or(|s| !s.1), "{:?}", app.status);
  }

  /// `ui:sketch:constraint-pick-command`: the entry starts a command with a prompt, the picks
  /// accumulate in the selection, the constraint applies on the last pick, the picks clear and
  /// the command stays active.
  #[test]
  fn a_constraint_entry_is_a_pick_command() {
    let (mut app, sk) = sketching();
    let a = line(&mut app, sk, [0.0, 0.0], [10.0, 1.0]);
    let b = line(&mut app, sk, [0.0, 5.0], [10.0, 7.0]);
    Cmd::Constraint("parallel").run(&mut app);
    assert_eq!(app.tool, Tool::Constrain("parallel"));
    assert!(Cmd::Constraint("parallel").active(&app), "the panel icon lights up");
    assert!(!Cmd::Constraint("equal").active(&app));
    no_error(&app);
    assert_eq!(crate::viewport::sketch_prompt(app.tool, &app.tstate), Some(PROMPT_START));
    hover_curve(&mut app, sk, a);
    click(&mut app, "parallel");
    assert_eq!(app.s.selection, vec![Sel::SketchCurve { sketch: sk, curve: a }], "the first pick is selected");
    assert_eq!(crate::viewport::sketch_prompt(app.tool, &app.tstate), Some(PROMPT_MORE));
    hover_curve(&mut app, sk, b);
    click(&mut app, "parallel");
    assert!(kinds(&app, sk).iter().any(|k| k == "Parallel"), "{:?}", kinds(&app, sk));
    assert!(app.s.selection.is_empty(), "the picks clear once the constraint is on");
    assert_eq!(app.tool, Tool::Constrain("parallel"), "the command stays active");
    assert_eq!(crate::viewport::sketch_prompt(app.tool, &app.tstate), Some(PROMPT_START));
    no_error(&app);
  }

  #[test]
  fn a_pre_selection_is_the_first_picks_and_another_entry_changes_the_type() {
    let (mut app, sk) = sketching();
    let a = line(&mut app, sk, [0.0, 0.0], [10.0, 1.0]);
    let b = line(&mut app, sk, [0.0, 5.0], [10.0, 7.0]);
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve: a }];
    Cmd::Constraint("perpendicular").run(&mut app);
    assert_eq!(app.s.selection.len(), 1, "one line waits for the second");
    assert_eq!(app.tstate.con_picks, 1);
    // the ribbon's Parallel switches the type, keeping the pick
    Cmd::Constraint("parallel").run(&mut app);
    assert_eq!(app.tool, Tool::Constrain("parallel"));
    assert_eq!(app.s.selection.len(), 1);
    hover_curve(&mut app, sk, b);
    click(&mut app, "parallel");
    assert_eq!(kinds(&app, sk), ["Parallel"]);
    // two lines selected, then Equal: it applies at once
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve: a }, Sel::SketchCurve { sketch: sk, curve: b }];
    Cmd::Constraint("equal").run(&mut app);
    assert!(kinds(&app, sk).iter().any(|k| k == "Equal"), "{:?}", kinds(&app, sk));
    assert!(app.s.selection.is_empty());
  }

  #[test]
  fn picks_the_constraint_cannot_use_are_refused_silently() {
    let (mut app, sk) = sketching();
    let a = line(&mut app, sk, [0.0, 0.0], [10.0, 1.0]);
    let (p, _) = ends(&app, sk, a);
    Cmd::Constraint("parallel").run(&mut app);
    // a point is nothing to Parallel
    hover_point(&mut app, sk, p);
    click(&mut app, "parallel");
    assert!(app.s.selection.is_empty());
    // a click on nothing picks nothing
    app.hover = None;
    click(&mut app, "parallel");
    assert!(app.s.selection.is_empty());
    assert!(app.status.is_none(), "{:?}", app.status);
    // Coincident takes two points
    Cmd::Constraint("coincident").run(&mut app);
    hover_point(&mut app, sk, p);
    click(&mut app, "coincident");
    assert_eq!(app.s.selection.len(), 1);
    // picking it again drops it
    click(&mut app, "coincident");
    assert!(app.s.selection.is_empty());
  }

  #[test]
  fn horizontal_vertical_applies_to_a_line_or_two_points() {
    let (mut app, sk) = sketching();
    let a = line(&mut app, sk, [0.0, 0.0], [10.0, 1.0]);
    let b = line(&mut app, sk, [20.0, 0.0], [21.0, 10.0]);
    Cmd::Constraint("hv").run(&mut app);
    assert_eq!(crate::viewport::sketch_prompt(app.tool, &app.tstate), Some(PROMPT_START_HV));
    hover_curve(&mut app, sk, a);
    click(&mut app, "hv");
    assert_eq!(kinds(&app, sk), ["Horizontal"]);
    assert!(app.s.selection.is_empty());
    let (p, _) = ends(&app, sk, a);
    let (q, _) = ends(&app, sk, b);
    hover_point(&mut app, sk, p);
    click(&mut app, "hv");
    assert_eq!(crate::viewport::sketch_prompt(app.tool, &app.tstate), Some(PROMPT_MORE));
    hover_point(&mut app, sk, q);
    click(&mut app, "hv");
    assert_eq!(kinds(&app, sk), ["Horizontal", "Horizontal"]);
  }

  /// `ui:sketch:fix-unfix-flow`: Fix applies on each pick; picking fixed geometry again in the
  /// same command unfixes it; the prompt after a pick reads "Select sketch objects to constrain".
  #[test]
  fn fix_applies_per_pick_and_picking_again_unfixes() {
    let (mut app, sk) = sketching();
    // off the origin: a line from the origin point has only one end to fix
    let a = line(&mut app, sk, [2.0, 3.0], [12.0, 4.0]);

    Cmd::Constraint("fix").run(&mut app);
    assert_eq!(crate::viewport::sketch_prompt(app.tool, &app.tstate), Some(PROMPT_START));
    let fixes = |app: &App| kinds(app, sk).iter().filter(|k| *k == "Fix").count();
    hover_curve(&mut app, sk, a);
    click(&mut app, "fix");
    assert_eq!(fixes(&app), 2, "both ends fixed");
    assert!(app.s.selection.is_empty());
    assert_eq!(crate::viewport::sketch_prompt(app.tool, &app.tstate), Some(PROMPT_CONSTRAIN));
    assert!(fox_engine::scene::fixed_curves(app.s.design.sketch(sk).unwrap()).contains(&a), "drawn green");
    click(&mut app, "fix");
    assert_eq!(fixes(&app), 0, "picking the fixed line again unfixes it");
    assert!(!fox_engine::scene::fixed_curves(app.s.design.sketch(sk).unwrap()).contains(&a));
    assert_eq!(app.tool, Tool::Constrain("fix"));
  }

  /// `ui:sketch:curvature-polygon-refusal`: Curvature takes a first line and silently refuses a
  /// second; Polygon ignores single lines and has its own prompt.
  #[test]
  fn curvature_refuses_a_second_line_and_polygon_ignores_lines() {
    let (mut app, sk) = sketching();
    let a = line(&mut app, sk, [0.0, 0.0], [10.0, 1.0]);
    let b = line(&mut app, sk, [10.0, 1.0], [20.0, 5.0]);
    Cmd::Constraint("curvature").run(&mut app);
    hover_curve(&mut app, sk, a);
    click(&mut app, "curvature");
    assert_eq!(app.s.selection.len(), 1);
    assert_eq!(crate::viewport::sketch_prompt(app.tool, &app.tstate), Some(PROMPT_CONSTRAIN));
    hover_curve(&mut app, sk, b);
    click(&mut app, "curvature");
    assert_eq!(app.s.selection, vec![Sel::SketchCurve { sketch: sk, curve: a }], "the line stays the one pick");
    assert!(app.status.is_none(), "no message: {:?}", app.status);
    assert!(kinds(&app, sk).is_empty());
    Cmd::Constraint("polygon").run(&mut app);
    assert_eq!(crate::viewport::sketch_prompt(app.tool, &app.tstate), Some(PROMPT_POLYGON));
    hover_curve(&mut app, sk, a);
    click(&mut app, "polygon");
    assert!(app.s.selection.is_empty(), "a single line pick is ignored");
    assert!(app.status.is_none(), "{:?}", app.status);
  }

  #[test]
  fn polygon_takes_a_closed_profile() {
    let (mut app, sk) = sketching();
    let n = 5;
    let pts: Vec<[f64; 2]> = (0..n)
      .map(|i| {
        let t = std::f64::consts::TAU * i as f64 / n as f64;
        [10.0 * t.cos(), 10.0 * t.sin()]
      })
      .collect();
    for i in 0..n {
      app.run("sketch.line", json!({"sketch": sk.0, "points": [pts[i], pts[(i + 1) % n]]})).unwrap();
    }
    assert_eq!(app.s.model.sketches[&sk].profiles.len(), 1);
    Cmd::Constraint("polygon").run(&mut app);
    app.hover = Some(Pick::Profile { sketch: sk, index: 0 });
    click(&mut app, "polygon");
    assert!(kinds(&app, sk).iter().any(|k| k == "Polygon"), "{:?}", kinds(&app, sk));
    assert!(app.s.selection.is_empty());
  }

  /// `ui:sketch:constraint-stale-error`: a refused command's red message goes when the next
  /// command succeeds.
  #[test]
  fn a_refused_commands_error_clears_on_the_next_success() {
    let (mut app, sk) = sketching();
    let a = line(&mut app, sk, [0.0, 0.0], [10.0, 1.0]);
    assert!(app.run("sketch.constrain", json!({"sketch": sk.0, "type": "coincident"})).is_none());
    assert!(app.status.as_ref().is_some_and(|s| s.1 && s.0.contains("needs 2 point(s)")), "{:?}", app.status);
    app.run("sketch.constrain", json!({"sketch": sk.0, "type": "horizontal", "curves": [a.0]})).unwrap();
    assert!(app.status.is_none(), "{:?}", app.status);
    // a message that isn't a command's refusal stays
    app.status = Some(("Computed in 0.1 s".into(), false));
    app.run("sketch.constrain", json!({"sketch": sk.0, "type": "fix", "curves": [a.0]})).unwrap();
    assert_eq!(app.status, Some(("Computed in 0.1 s".into(), false)));
  }

  #[test]
  fn escape_ends_the_command_and_drops_its_picks() {
    let (mut app, sk) = sketching();
    let a = line(&mut app, sk, [0.0, 0.0], [10.0, 1.0]);
    Cmd::Constraint("parallel").run(&mut app);
    hover_curve(&mut app, sk, a);
    click(&mut app, "parallel");
    assert_eq!(app.s.selection.len(), 1);
    app.escape();
    assert_eq!(app.tool, Tool::Select);
    assert!(app.s.selection.is_empty());
  }

  #[test]
  fn prompts_per_constraint() {
    assert_eq!(prompt("coincident", 0, false), PROMPT_START);
    assert_eq!(prompt("coincident", 1, true), PROMPT_MORE);
    assert_eq!(prompt("hv", 0, false), PROMPT_START_HV);
    assert_eq!(prompt("symmetric", 1, true), PROMPT_SYMMETRY_SECOND);
    assert_eq!(prompt("symmetric", 2, true), PROMPT_SYMMETRY_LINE);
    assert_eq!(prompt("fix", 0, false), PROMPT_START);
    assert_eq!(prompt("fix", 0, true), PROMPT_CONSTRAIN);
    assert_eq!(prompt("curvature", 1, true), PROMPT_CONSTRAIN);
    assert_eq!(prompt("polygon", 0, false), PROMPT_POLYGON);
  }
}
