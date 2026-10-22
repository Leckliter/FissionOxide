//! All flat patterns on one sheet: each sheet metal body's flat pattern outline (bend lines as
//! construction) copied into one new sketch on XY, or straight into one DXF, laid out in rows
//! that wrap before a part would run past the sheet's width.

use fox_model::{FeatureKind, OriginPlane, PlaneRef};
use fox_sketch::{CurveKind, PointId, Sketch};
use serde_json::{Value, json};

use crate::commands::{ids, opt_str};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "sheet.collect_flat" => collect(s, p),
    _ => return None,
  })
}

/// A length in `p`, or the default expression.
fn length(s: &Session, p: &Value, k: &str, default: &str) -> Result<f64, CmdError> {
  let e = match p.get(k) {
    Some(Value::Number(n)) => n.to_string(),
    Some(Value::String(e)) if !e.trim().is_empty() => e.clone(),
    _ => default.to_string(),
  };
  Ok(fox_expr::eval(&e, fox_expr::Kind::Length, &s.model.params)?)
}

/// Copies `src`'s lines, circles and arcs into `dst` with fixed points.
fn append(dst: &mut Sketch, src: &Sketch) -> Result<(), CmdError> {
  let mut map: Vec<(PointId, PointId)> = Vec::new();
  let mut pt = |dst: &mut Sketch, id: PointId| -> PointId {
    if let Some((_, to)) = map.iter().find(|(from, _)| *from == id) {
      return *to;
    }
    let [x, y] = src.pos(id).unwrap_or([0.0, 0.0]);
    let to = dst.add_point(x, y);
    if let Some(q) = dst.points.iter_mut().find(|q| q.id == to) {
      q.fixed = true;
    }
    map.push((id, to));
    to
  };
  for c in &src.curves {
    let kind = match c.kind {
      CurveKind::Line { a, b } => CurveKind::Line { a: pt(dst, a), b: pt(dst, b) },
      CurveKind::Circle { center, radius } => CurveKind::Circle { center: pt(dst, center), radius },
      CurveKind::Arc { center, start, end } => {
        CurveKind::Arc { center: pt(dst, center), start: pt(dst, start), end: pt(dst, end) }
      }
      _ => continue,
    };
    let id = dst.add_curve(kind).map_err(|e| CmdError::new(e.to_string()))?;
    if c.construction {
      dst.set_construction(id, true).map_err(|e| CmdError::new(e.to_string()))?;
    }
    if c.centerline {
      dst.set_centerline(id, true).map_err(|e| CmdError::new(e.to_string()))?;
    }
  }
  Ok(())
}

/// `name`, else `name 02`, `name 03`, … when a feature already has it.
fn free_name(s: &Session, name: &str) -> String {
  let taken = |n: &str| s.design.features.iter().any(|f| f.name.eq_ignore_ascii_case(n));
  if !taken(name) {
    return name.to_string();
  }
  (2..).map(|i| format!("{name} {i:02}")).find(|n| !taken(n)).unwrap_or_else(|| name.to_string())
}

fn collect(s: &mut Session, p: &Value) -> CmdResult {
  let max_width = length(s, p, "max_width", "48 in")?;
  let gap = length(s, p, "gap", "0.25 in")?;
  if max_width <= 0.0 || gap < 0.0 {
    return Err(CmdError::new("max_width must be positive and gap not negative"));
  }
  let picked = ids(p, "bodies");
  let bodies: Vec<fox_model::BodyId> = if picked.is_empty() {
    s.model.bodies.iter().filter(|b| b.sheet.is_some()).map(|b| b.id).collect()
  } else {
    picked.into_iter().map(fox_model::BodyId).collect()
  };
  if bodies.is_empty() {
    return Err(CmdError::new("there are no sheet metal bodies"));
  }
  let mut sheet = Sketch::new();
  let mut parts = Vec::new();
  let mut skipped = Vec::new();
  let mut warnings = Vec::new();
  // the row's left edge runs along x = 0, rows grow downward from y = 0
  let (mut x, mut top, mut row_h, mut row) = (0.0_f64, 0.0_f64, 0.0_f64, 0usize);
  let mut width = 0.0_f64;
  for id in bodies {
    let body = s.model.body(id).ok_or(CmdError::new(format!("no body {}", id.0)))?;
    if body.sheet.is_none() {
      skipped.push(json!({"body": id.0, "name": body.name, "reason": "not sheet metal"}));
      continue;
    }
    let flat = match body.flat_pattern(None) {
      Ok(f) => f,
      Err(e) => {
        skipped.push(json!({"body": id.0, "name": body.name, "reason": e}));
        continue;
      }
    };
    let at_origin = fox_model::sheet2::flat_sketch(&flat, &flat.plane, [0.0, 0.0]).map_err(CmdError::new)?;
    let Some((lo, hi)) = at_origin.bounds() else {
      skipped.push(json!({"body": id.0, "name": body.name, "reason": "the flat pattern is empty"}));
      continue;
    };
    let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    // wrap before the part would run past the width
    if x > 0.0 && x + w > max_width + 1e-9 {
      x = 0.0;
      top -= row_h + gap;
      row_h = 0.0;
      row += 1;
    }
    if w > max_width + 1e-9 {
      warnings.push(format!("{} is {w:.2} mm wide, wider than the sheet ({max_width:.2} mm)", body.name));
    }
    let shift = [x - lo[0], top - h - lo[1]];
    let placed = fox_model::sheet2::flat_sketch(&flat, &flat.plane, shift).map_err(CmdError::new)?;
    append(&mut sheet, &placed)?;
    parts.push(json!({
      "body": id.0, "name": body.name, "row": row + 1,
      "min": [x, top - h], "max": [x + w, top], "size": [w, h], "bends": flat.bends.len(),
    }));
    width = width.max(x + w);
    x += w + gap;
    row_h = row_h.max(h);
  }
  if parts.is_empty() {
    return Err(CmdError::new("no flat pattern could be made"));
  }
  let height = -(top - row_h);
  let mut out = json!({
    "parts": parts, "skipped": skipped, "warnings": warnings, "rows": row + 1,
    "width": width, "height": height, "max_width": max_width, "gap": gap,
  });
  match opt_str(p, "path").filter(|f| !f.trim().is_empty()) {
    Some(path) => {
      let ents = crate::files::sketch_entities(&sheet, false);
      std::fs::write(path, fox_exchange::dxf::write_dxf(&ents))?;
      out["path"] = json!(path);
      out["entities"] = json!(ents.len());
    }
    None => {
      let name = free_name(s, opt_str(p, "sketch_name").filter(|n| !n.trim().is_empty()).unwrap_or("Cut File"));
      let curves = sheet.curves.len();
      let kind =
        FeatureKind::Sketch { plane: PlaneRef::Origin { plane: OriginPlane::Xy }, sketch: sheet, links: Vec::new() };
      // one undo step: the sketch arrives with its name
      let id = s.edit(|d| {
        let id = d.add_feature(kind)?;
        d.feature_mut(id).ok_or(CmdError::new("no such feature"))?.name = name.clone();
        Ok(id)
      })?;
      if let Some(e) = s.model.status(id).and_then(|x| x.error.clone()) {
        out["error"] = json!(e);
      }
      out["sketch"] = json!(id.0);
      out["name"] = json!(name);
      out["curves"] = json!(curves);
    }
  }
  Ok(out)
}
