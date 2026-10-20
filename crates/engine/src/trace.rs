//! Image tracing: a photo or logo traced into closed sketch curves (`sketch.trace`), or just
//! measured (`trace.info`). The tracer itself is [`fox_trace`]; this maps its pixel contours
//! onto the sketch, centred on `at` and scaled to the requested size.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use fox_sketch::{CurveKind, PointId, Sketch};
use fox_trace::{Contour, Fit, Image, Mode, Options, Seg, Trace};
use serde_json::{Value, json};

use crate::CmdError;
use crate::commands::{expr_of, opt_str, req_str};

/// Default traced width (mm) when no size is given.
const DEFAULT_WIDTH: f64 = 100.0;

/// The last image decoded, so a dialog re-tracing on every slider move doesn't decode it again.
static LAST: Mutex<Option<(PathBuf, Option<SystemTime>, Arc<Image>)>> = Mutex::new(None);

fn load(path: &Path) -> Result<Arc<Image>, CmdError> {
  let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
  if let Ok(last) = LAST.lock()
    && let Some((p, t, img)) = last.as_ref()
    && p == path
    && *t == stamp
  {
    return Ok(img.clone());
  }
  let img = Arc::new(fox_trace::load(path).map_err(|e| CmdError::new(format!("{}: {e}", path.display())))?);
  if let Ok(mut last) = LAST.lock() {
    *last = Some((path.to_path_buf(), stamp, img.clone()));
  }
  Ok(img)
}

fn num(p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => {
      v.as_f64().filter(|f| f.is_finite()).map(Some).ok_or_else(|| CmdError::new(format!("'{k}' must be a number")))
    }
  }
}

/// Tracer options from command parameters; anything left out keeps the tracer's default.
pub(crate) fn options(p: &Value) -> Result<Options, CmdError> {
  let mut o = Options::default();
  let level = |v: f64| -> Result<u8, CmdError> {
    (0.0..=255.0).contains(&v).then_some(v.round() as u8).ok_or(CmdError::new("'threshold' is 0–255"))
  };
  o.mode = match opt_str(p, "mode").unwrap_or("outline") {
    "outline" => Mode::Outline { threshold: num(p, "threshold")?.map(level).transpose()? },
    "colors" | "colours" => {
      let n = num(p, "colors")?.unwrap_or(4.0);
      if !(2.0..=16.0).contains(&n) {
        return Err(CmdError::new("'colors' is 2–16"));
      }
      Mode::Colors { count: n.round() as u8 }
    }
    m => return Err(CmdError::new(format!("unknown mode '{m}' (outline, colors)"))),
  };
  if let Some(b) = p.get("invert").and_then(Value::as_bool) {
    o.invert = b;
  }
  let non_negative = |k: &str| -> Result<Option<f64>, CmdError> {
    match num(p, k)? {
      Some(v) if v < 0.0 => Err(CmdError::new(format!("'{k}' can't be negative"))),
      v => Ok(v),
    }
  };
  if let Some(v) = non_negative("blur")? {
    o.blur = v;
  }
  if let Some(v) = non_negative("specks")? {
    o.specks = v;
  }
  if let Some(v) = non_negative("holes")? {
    o.holes = v;
  }
  if let Some(v) = num(p, "tolerance")? {
    if v <= 0.0 {
      return Err(CmdError::new("'tolerance' must be positive"));
    }
    o.tolerance = v;
  }
  if let Some(v) = num(p, "corners")? {
    o.corner_angle = v.clamp(0.0, 180.0);
  }
  if let Some(f) = opt_str(p, "fit") {
    o.fit = match f {
      "lines" => Fit::Lines,
      "arcs" => Fit::Arcs,
      "curves" => Fit::Curves,
      f => return Err(CmdError::new(format!("unknown fit '{f}' (lines, arcs, curves)"))),
    };
  }
  Ok(o)
}

/// Runs the tracer on the `image` parameter.
pub(crate) fn run(p: &Value) -> Result<Trace, CmdError> {
  let img = load(Path::new(req_str(p, "image")?))?;
  fox_trace::trace(&img, &options(p)?).map_err(|e| CmdError::new(e.to_string()))
}

/// Parameter values, for sizes given as expressions ("120 mm", "plate_w - 10").
pub(crate) type Scope = HashMap<String, f64>;

/// Millimetres per traced pixel: from `width`, `height` (lengths) or `scale` (mm per pixel).
fn mm_per_px(p: &Value, t: &Trace, scope: &Scope) -> Result<f64, CmdError> {
  let eval = |k: &str, kind| -> Result<Option<f64>, CmdError> {
    match p.get(k) {
      None | Some(Value::Null) => Ok(None),
      Some(v) => Ok(Some(fox_expr::eval(&expr_of(v)?, kind, scope)?)),
    }
  };
  let k = if let Some(s) = eval("scale", fox_expr::Kind::Unitless)? {
    s
  } else if let Some(h) = eval("height", fox_expr::Kind::Length)? {
    h / f64::from(t.height.max(1))
  } else {
    eval("width", fox_expr::Kind::Length)?.unwrap_or(DEFAULT_WIDTH) / f64::from(t.width.max(1))
  };
  if k <= 0.0 || !k.is_finite() {
    return Err(CmdError::new("the traced size must be positive"));
  }
  Ok(k)
}

/// The colour layers `sketch.trace` draws: `layers` if given, else every non-background one.
fn chosen_layers(p: &Value, t: &Trace) -> Result<Vec<usize>, CmdError> {
  match p.get("layers").and_then(Value::as_array) {
    Some(list) => list
      .iter()
      .map(|v| {
        v.as_u64()
          .and_then(|i| usize::try_from(i).ok())
          .filter(|i| *i < t.layers.len())
          .ok_or_else(|| CmdError::new(format!("no colour layer {v} (there are {})", t.layers.len())))
      })
      .collect(),
    None => Ok((0..t.layers.len()).filter(|i| !t.layers[*i].background).collect()),
  }
}

fn hex(c: [u8; 3]) -> String {
  format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn summary(t: &Trace, k: f64) -> Value {
  let layers: Vec<Value> = t
    .layers
    .iter()
    .enumerate()
    .map(|(i, l)| {
      let holes = l.contours.iter().filter(|c| c.hole).count();
      json!({
        "index": i,
        "color": hex(l.color),
        "background": l.background,
        "shapes": l.contours.len() - holes,
        "holes": holes,
        "segments": l.contours.iter().map(|c| c.segs.len()).sum::<usize>(),
      })
    })
    .collect();
  json!({
    "pixels": [t.width, t.height],
    "size": [f64::from(t.width) * k, f64::from(t.height) * k],
    "layers": layers,
  })
}

/// `trace.info`: what a trace would give, without changing the design.
pub(crate) fn info(p: &Value, scope: &Scope) -> Result<Value, CmdError> {
  let t = run(p)?;
  let k = mm_per_px(p, &t, scope)?;
  Ok(summary(&t, k))
}

/// Pixel → sketch mapping: the image centre lands on `at`.
struct Place {
  k: f64,
  cx: f64,
  cy: f64,
  at: [f64; 2],
}

impl Place {
  fn map(&self, q: [f64; 2]) -> [f64; 2] {
    [self.at[0] + (q[0] - self.cx) * self.k, self.at[1] + (q[1] - self.cy) * self.k]
  }
}

/// Adds one closed contour; consecutive segments share their end points.
fn add_contour(sk: &mut Sketch, c: &Contour, place: &Place) -> Result<(), CmdError> {
  let err = |e: fox_sketch::SketchError| CmdError::new(e.to_string());
  if let [Seg::Circle { center, radius }] = c.segs.as_slice() {
    let q = place.map(*center);
    let center = sk.add_point(q[0], q[1]);
    sk.add_curve(CurveKind::Circle { center, radius: radius * place.k }).map_err(err)?;
    return Ok(());
  }
  let start = |s: &Seg| match s {
    Seg::Line { a, .. } | Seg::Arc { a, .. } | Seg::Cubic { a, .. } => Some(*a),
    Seg::Circle { .. } => None,
  };
  let Some(first) = c.segs.first().and_then(start) else { return Ok(()) };
  let q = place.map(first);
  let first_id = sk.add_point(q[0], q[1]);
  let mut prev = first_id;
  let n = c.segs.len();
  for (i, seg) in c.segs.iter().enumerate() {
    let end = |sk: &mut Sketch, b: [f64; 2]| -> PointId {
      if i + 1 == n {
        return first_id;
      }
      let q = place.map(b);
      sk.add_point(q[0], q[1])
    };
    match seg {
      Seg::Line { b, .. } => {
        let b = end(sk, *b);
        sk.add_curve(CurveKind::Line { a: prev, b }).map_err(err)?;
        prev = b;
      }
      Seg::Arc { a, b, center, ccw } => {
        let (pa, pb) = (place.map(*a), place.map(*b));
        let c = on_bisector(place.map(*center), pa, pb);
        let bid = end(sk, *b);
        let center = sk.add_point(c[0], c[1]);
        let (start, end) = if *ccw { (prev, bid) } else { (bid, prev) };
        sk.add_curve(CurveKind::Arc { center, start, end }).map_err(err)?;
        prev = bid;
      }
      Seg::Cubic { c1, c2, b, .. } => {
        let (q1, q2) = (place.map(*c1), place.map(*c2));
        let (p1, p2) = (sk.add_point(q1[0], q1[1]), sk.add_point(q2[0], q2[1]));
        let b = end(sk, *b);
        sk.add_curve(CurveKind::control_spline(vec![prev, p1, p2, b])).map_err(err)?;
        prev = b;
      }
      Seg::Circle { .. } => return Err(CmdError::new("the tracer returned a circle inside a contour")),
    }
  }
  Ok(())
}

/// `c` moved onto the perpendicular bisector of `a`–`b`, so both ends lie exactly on the arc.
fn on_bisector(c: [f64; 2], a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
  let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
  let d = [b[0] - a[0], b[1] - a[1]];
  let len2 = d[0] * d[0] + d[1] * d[1];
  if len2 < 1e-24 {
    return c;
  }
  // remove the component of (c - m) along a–b
  let t = ((c[0] - m[0]) * d[0] + (c[1] - m[1]) * d[1]) / len2;
  [c[0] - t * d[0], c[1] - t * d[1]]
}

/// A contour as a closed polygon (pixels), curves sampled finely enough to classify regions.
fn polygon(c: &Contour) -> Vec<[f64; 2]> {
  const N: usize = 16;
  let mut out = Vec::new();
  for seg in &c.segs {
    match *seg {
      Seg::Line { a, .. } => out.push(a),
      Seg::Arc { a, b, center, ccw } => {
        let (a0, mut a1) = ((a[1] - center[1]).atan2(a[0] - center[0]), (b[1] - center[1]).atan2(b[0] - center[0]));
        let tau = std::f64::consts::TAU;
        if ccw && a1 <= a0 {
          a1 += tau;
        } else if !ccw && a1 >= a0 {
          a1 -= tau;
        }
        let r = (a[0] - center[0]).hypot(a[1] - center[1]);
        for i in 0..N {
          let t = a0 + (a1 - a0) * i as f64 / N as f64;
          out.push([center[0] + r * t.cos(), center[1] + r * t.sin()]);
        }
      }
      Seg::Cubic { a, c1, c2, b } => {
        for i in 0..N {
          let t = i as f64 / N as f64;
          let u = 1.0 - t;
          let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
          out.push([
            w[0] * a[0] + w[1] * c1[0] + w[2] * c2[0] + w[3] * b[0],
            w[0] * a[1] + w[1] * c1[1] + w[2] * c2[1] + w[3] * b[1],
          ]);
        }
      }
      Seg::Circle { center, radius } => {
        for i in 0..4 * N {
          let t = std::f64::consts::TAU * i as f64 / (4 * N) as f64;
          out.push([center[0] + radius * t.cos(), center[1] + radius * t.sin()]);
        }
      }
    }
  }
  out
}

fn inside(poly: &[[f64; 2]], q: [f64; 2]) -> bool {
  let mut odd = false;
  let mut j = poly.len().wrapping_sub(1);
  for (i, a) in poly.iter().enumerate() {
    let b = poly[j];
    if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < a[0] + (b[0] - a[0]) * (q[1] - a[1]) / (b[1] - a[1]) {
      odd = !odd;
    }
    j = i;
  }
  odd
}

/// Which of the sketch's profiles are traced ink (inside a shape and not in one of its holes),
/// as opposed to the holes the outlines also enclose: what an extrude of the trace should pick.
pub(crate) fn ink_profiles(placed: &Placed, profiles: &[fox_sketch::Profile]) -> Vec<usize> {
  let (s, c) = (-placed.angle).sin_cos();
  profiles
    .iter()
    .enumerate()
    .filter(|(_, pr)| {
      // the region's inside point back in traced pixels
      let q = pr.interior_point();
      let (dx, dy) = (q[0] - placed.place.at[0], q[1] - placed.place.at[1]);
      let (x, y) = (dx * c - dy * s, dx * s + dy * c);
      let px = [placed.place.cx + x / placed.place.k, placed.place.cy + y / placed.place.k];
      placed.polygons.iter().any(|layer| layer.iter().filter(|poly| inside(poly, px)).count() % 2 == 1)
    })
    .map(|(i, _)| i)
    .collect()
}

/// Where a trace went, to tell its ink from its holes afterwards.
pub(crate) struct Placed {
  place: Place,
  pub angle: f64,
  /// The drawn layers' contours as polygons (pixels), one list per layer.
  polygons: Vec<Vec<Vec<[f64; 2]>>>,
}

/// The traced curves of the chosen layers as a standalone sketch (merged into the target sketch
/// by `insert_from`), with the summary the command returns.
pub(crate) fn sketch(p: &Value, at: [f64; 2], angle: f64, scope: &Scope) -> Result<(Sketch, Value, Placed), CmdError> {
  let t = run(p)?;
  let k = mm_per_px(p, &t, scope)?;
  let place = Place { k, cx: f64::from(t.width) / 2.0, cy: f64::from(t.height) / 2.0, at };
  let layers = chosen_layers(p, &t)?;
  let mut sk = Sketch::new();
  let (mut shapes, mut holes) = (0, 0);
  for &i in &layers {
    for c in &t.layers[i].contours {
      add_contour(&mut sk, c, &place)?;
      if c.hole {
        holes += 1;
      } else {
        shapes += 1;
      }
    }
  }
  let mut out = summary(&t, k);
  out["shapes"] = json!(shapes);
  out["holes"] = json!(holes);
  out["drawn_layers"] = json!(layers);
  if shapes == 0 {
    out["warning"] = json!("nothing was traced: try another threshold, Invert, or fewer ignored specks");
  }
  let polygons = layers.iter().map(|&i| t.layers[i].contours.iter().map(polygon).collect()).collect();
  Ok((sk, out, Placed { place, angle, polygons }))
}
