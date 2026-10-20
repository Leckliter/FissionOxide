//! Sketch commands for elliptical arcs and the fonts sketch text can use.

use fox_sketch::{CurveKind, Ellipse2, PtSpec, Sketch};
use serde_json::{Value, json};

use super::{req_pt, sketch_id};
use crate::{CmdError, CmdResult, Session};

/// Snap radius (mm) for joining an arc's ends to existing points.
const SNAP: f64 = 1e-6;

pub(super) fn run(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  match name {
    "sketch.elliptical_arc" => elliptical_arc(s, p),
    "sketch.fonts" => Ok(fonts()),
    _ => Err(CmdError::new(format!("unknown command '{name}'"))),
  }
}

fn at_or_snap(sk: &mut Sketch, at: [f64; 2]) -> Result<fox_sketch::PointId, CmdError> {
  let spec = match sk.nearest_point(at, SNAP, &[]) {
    Some(id) => PtSpec::Point(id),
    None => PtSpec::At(at[0], at[1]),
  };
  Ok(sk.resolve(spec)?)
}

/// The built-in fonts and the font files found in the system's font folders.
fn fonts() -> Value {
  let system: Vec<Value> = fox_sketch::system_fonts()
    .into_iter()
    .map(|(name, path)| json!({"name": name, "path": path.display().to_string()}))
    .collect();
  json!({"builtin": fox_sketch::text_fonts(), "system": system})
}

/// An elliptical arc: the ellipse from its centre, the end of the major axis and the minor
/// radius, counter-clockwise from the point of it nearest `start` round to the one nearest `end`.
fn elliptical_arc(s: &mut Session, p: &Value) -> CmdResult {
  let (c, m, a, b) = (req_pt(p, "center")?, req_pt(p, "major")?, req_pt(p, "start")?, req_pt(p, "end")?);
  let minor = p.get("minor").and_then(Value::as_f64).ok_or(CmdError::new("missing 'minor'"))?;
  let e = Ellipse2::new(c, m, minor).ok_or(CmdError::new("the ellipse needs two positive radii"))?;
  let (ps, pe) = (e.at(e.param(a)), e.at(e.param(b)));
  if (ps[0] - pe[0]).hypot(ps[1] - pe[1]) < 1e-9 {
    return Err(CmdError::new("the arc's ends are the same point: draw an ellipse"));
  }
  let sk = sketch_id(s, p)?;
  let id = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    let center = at_or_snap(sketch, c)?;
    let major = sketch.add_point(m[0], m[1]);
    let start = at_or_snap(sketch, ps)?;
    let end = at_or_snap(sketch, pe)?;
    let id = sketch.add_curve(CurveKind::EllipticalArc { center, major, minor, start, end })?;
    sketch.solve();
    Ok(id)
  })?;
  let profiles = s.model.sketches.get(&sk).map(|r| r.profiles.len()).unwrap_or(0);
  Ok(json!({"curves": [id.0], "profiles": profiles}))
}
