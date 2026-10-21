//! Canvases: reference images placed on a plane (Insert > Canvas), their edits (opacity, size,
//! position) and Calibrate, which rescales one so two picked points lie a typed distance apart.

use fox_model::{FeatureId, FeatureKind};
use serde_json::{Value, json};

use crate::commands::{add_feature, opt_str, plane_ref, req_str, req_u32};
use crate::{CmdError, CmdResult, Session};

/// Width (mm) a canvas gets when none is given.
const DEFAULT_WIDTH: f64 = 100.0;

pub fn run(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  match name {
    "canvas.insert" => insert(s, p),
    "canvas.edit" => edit(s, p),
    "canvas.calibrate" => {
      let id = FeatureId(req_u32(p, "feature")?);
      let (from, to) = (req_pt(p, "from")?, req_pt(p, "to")?);
      let distance = num(p, "distance")?.ok_or(CmdError::new("missing 'distance'"))?;
      let (at, width, height) = canvas(s, id)?;
      let (at, width, height) = calibrate(at, width, height, from, to, distance)?;
      set(s, id, |k| {
        if let FeatureKind::Canvas { at: a, width: w, height: h, .. } = k {
          (*a, *w, *h) = (at, width, height);
        }
      })?;
      Ok(json!({"feature": id.0, "at": at, "width": width, "height": height}))
    }
    _ => Err(CmdError::new(format!("unknown command '{name}' (try 'help')"))),
  }
}

/// Calibrate's maths: scales a canvas (centre `at`, `width` × `height`) about `from` so that
/// `from` and `to` (plane coordinates, as picked on the image) end up `distance` apart. `from`
/// stays where it is. Returns the new centre and size.
pub fn calibrate(
  at: [f64; 2],
  width: f64,
  height: f64,
  from: [f64; 2],
  to: [f64; 2],
  distance: f64,
) -> Result<([f64; 2], f64, f64), CmdError> {
  let picked = (to[0] - from[0]).hypot(to[1] - from[1]);
  if picked < 1e-9 {
    return Err(CmdError::new("pick two different points on the canvas"));
  }
  if !(distance.is_finite() && distance > 0.0) {
    return Err(CmdError::new("the distance must be greater than zero"));
  }
  let k = distance / picked;
  let at = [from[0] + (at[0] - from[0]) * k, from[1] + (at[1] - from[1]) * k];
  Ok((at, width * k, height * k))
}

fn insert(s: &mut Session, p: &Value) -> CmdResult {
  let image = req_str(p, "image")?.to_string();
  let (px_w, px_h) = image::ImageReader::open(&image)
    .and_then(|r| r.with_guessed_format())
    .map_err(image::ImageError::IoError)
    .and_then(|r| r.into_dimensions())
    .map_err(|e| CmdError::new(format!("can't read the image '{image}': {e}")))?;
  if px_w == 0 || px_h == 0 {
    return Err(CmdError::new("the image is empty"));
  }
  let aspect = f64::from(px_h) / f64::from(px_w);
  let plane = plane_ref(s, p.get("plane").unwrap_or(&json!("xy")))?;
  let (width, height) = match (num(p, "width")?, num(p, "height")?) {
    (Some(w), _) => (w, w * aspect),
    (None, Some(h)) => (h / aspect, h),
    (None, None) => (DEFAULT_WIDTH, DEFAULT_WIDTH * aspect),
  };
  if !(width > 0.0 && height > 0.0) {
    return Err(CmdError::new("the canvas size must be greater than zero"));
  }
  let at = opt_pt(p, "at")?.unwrap_or([0.0, 0.0]);
  let angle = num(p, "angle")?.unwrap_or(0.0);
  let opacity = num(p, "opacity")?.unwrap_or(1.0).clamp(0.0, 1.0);
  let r = add_feature(s, FeatureKind::Canvas { plane, image, at, width, height, angle, opacity })?;
  if let (Some(name), Some(id)) = (opt_str(p, "name"), r["feature"].as_u64()) {
    let n = name.to_string();
    s.edit(|d| {
      if let Some(f) = d.feature_mut(FeatureId(id as u32)) {
        f.name = n;
      }
      Ok(())
    })?;
  }
  Ok(r)
}

/// Changes a canvas's opacity, visibility, size (width keeps the aspect unless height is given
/// too), centre, angle or plane.
fn edit(s: &mut Session, p: &Value) -> CmdResult {
  let id = FeatureId(req_u32(p, "feature")?);
  let (_, w0, h0) = canvas(s, id)?;
  let aspect = h0 / w0;
  let (width, height) = match (num(p, "width")?, num(p, "height")?) {
    (Some(w), Some(h)) => (Some(w), Some(h)),
    (Some(w), None) => (Some(w), Some(w * aspect)),
    (None, Some(h)) => (Some(h / aspect), Some(h)),
    (None, None) => (None, None),
  };
  if width.is_some_and(|w| w <= 0.0) || height.is_some_and(|h| h <= 0.0) {
    return Err(CmdError::new("the canvas size must be greater than zero"));
  }
  let plane = p.get("plane").map(|v| plane_ref(s, v)).transpose()?;
  let at = opt_pt(p, "at")?;
  let angle = num(p, "angle")?;
  let opacity = num(p, "opacity")?.map(|o| o.clamp(0.0, 1.0));
  if let Some(v) = p.get("visible").and_then(Value::as_bool) {
    s.edit(|d| {
      d.feature_mut(id).ok_or(CmdError::new("no such feature"))?.visible = Some(v);
      Ok(())
    })?;
  }
  set(s, id, |k| {
    if let FeatureKind::Canvas { plane: pl, at: a, width: w, height: h, angle: an, opacity: o, .. } = k {
      if let Some(v) = plane {
        *pl = v;
      }
      *a = at.unwrap_or(*a);
      *w = width.unwrap_or(*w);
      *h = height.unwrap_or(*h);
      *an = angle.unwrap_or(*an);
      *o = opacity.unwrap_or(*o);
    }
  })?;
  Ok(canvas_json(s, id))
}

/// Where canvas `id` lies in sketch `sketch`'s coordinates, for tracing it: the image file, the
/// image centre, its rotation (radians) and its width (mm). The sketch must lie on the canvas's
/// plane or a parallel one facing the same way (the image is projected straight onto it).
pub(crate) fn placement(
  s: &Session,
  sketch: FeatureId,
  id: FeatureId,
) -> Result<(String, [f64; 2], f64, f64), CmdError> {
  let Some(FeatureKind::Canvas { image, at, width, angle, .. }) = s.design.feature(id).map(|f| &f.kind) else {
    return Err(CmdError::new("that feature is not a canvas"));
  };
  let cp = s.model.canvases.get(&id).ok_or(CmdError::new("the canvas has no plane (is it suppressed?)"))?;
  let sp = &s.model.sketches.get(&sketch).ok_or(CmdError::new("the sketch isn't built"))?.plane;
  let (nc, ns) = (cp.x.cross(cp.y), sp.x.cross(sp.y));
  if nc.dot(ns) < 1.0 - 1e-6 {
    return Err(CmdError::new("the sketch isn't on the canvas's plane (or one parallel to it, facing the same way)"));
  }
  let centre = cp.origin + cp.x * at[0] + cp.y * at[1];
  let local = [(centre - sp.origin).dot(sp.x), (centre - sp.origin).dot(sp.y)];
  let a = angle.to_radians();
  let ux = cp.x * a.cos() + cp.y * a.sin();
  Ok((image.clone(), local, ux.dot(sp.y).atan2(ux.dot(sp.x)), *width))
}

/// A canvas's centre, width and height.
fn canvas(s: &Session, id: FeatureId) -> Result<([f64; 2], f64, f64), CmdError> {
  match s.design.feature(id).map(|f| &f.kind) {
    Some(FeatureKind::Canvas { at, width, height, .. }) => Ok((*at, *width, *height)),
    _ => Err(CmdError::new("that feature is not a canvas")),
  }
}

fn set(s: &mut Session, id: FeatureId, f: impl FnOnce(&mut FeatureKind)) -> Result<(), CmdError> {
  s.edit(|d| {
    f(&mut d.feature_mut(id).ok_or(CmdError::new("no such feature"))?.kind);
    Ok(())
  })
}

fn canvas_json(s: &Session, id: FeatureId) -> Value {
  let visible = s.design.feature(id).is_some_and(|f| f.visible != Some(false));
  match s.design.feature(id).map(|f| &f.kind) {
    Some(FeatureKind::Canvas { image, at, width, height, angle, opacity, .. }) => json!({
      "feature": id.0, "image": image, "at": at, "width": width, "height": height, "angle": angle, "opacity": opacity,
      "visible": visible,
    }),
    _ => json!({}),
  }
}

/// A plain number (canvases hold sizes in mm, not expressions).
fn num(p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => {
      v.as_f64().filter(|v| v.is_finite()).map(Some).ok_or_else(|| CmdError::new(format!("'{k}' must be a number")))
    }
  }
}

fn req_pt(p: &Value, k: &str) -> Result<[f64; 2], CmdError> {
  opt_pt(p, k)?.ok_or_else(|| CmdError::new(format!("'{k}' must be [x, y]")))
}

fn opt_pt(p: &Value, k: &str) -> Result<Option<[f64; 2]>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => match v.as_array().map(Vec::as_slice) {
      Some([x, y]) => match (x.as_f64(), y.as_f64()) {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(Some([x, y])),
        _ => Err(CmdError::new(format!("'{k}' must be [x, y]"))),
      },
      _ => Err(CmdError::new(format!("'{k}' must be [x, y]"))),
    },
  }
}
