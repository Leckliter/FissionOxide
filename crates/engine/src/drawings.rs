//! Drawing commands: create drawings, add views, dimensions and notes, and export them.

use std::path::PathBuf;

use clf_model::BodyId;
use clf_model::drawing::{
  Axis, DimOrientation, Drawing, DrawingDim, ProjectionAngle, SheetSize, ViewKind, render, scale_text,
};
use serde_json::{Value, json};

use crate::commands::{ids, opt_str, req_str, req_u32, req_vec3};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "drawing.create" => (|| {
      let kinds = match p.get("views").and_then(Value::as_array) {
        Some(list) => list.iter().map(view_kind).collect::<Result<Vec<_>, _>>()?,
        None => vec![ViewKind::Front, ViewKind::Top, ViewKind::Right, ViewKind::Iso],
      };
      let name = opt_str(p, "name").unwrap_or("").to_string();
      let id = s.edit(|d| {
        let id = d.add_drawing(&name, kinds);
        let dr = d.drawing_mut(id).ok_or(CmdError::new("drawing vanished"))?;
        settings(dr, p)?;
        dr.bodies = ids(p, "bodies").into_iter().map(BodyId).collect();
        Ok(id)
      })?;
      info(s, id)
    })(),
    "drawing.set" => (|| {
      let id = drawing_id(s, p)?;
      s.edit(|d| settings(d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?, p))?;
      info(s, id)
    })(),
    "drawing.add_view" => (|| {
      let id = drawing_id(s, p)?;
      let kind = view_kind(p.get("kind").ok_or(CmdError::new("missing 'kind'"))?)?;
      let at = at(p)?;
      let hidden = p.get("hidden_lines").and_then(Value::as_bool);
      let view = s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        let v = dr.add_view(kind, at);
        if let (Some(h), Some(view)) = (hidden, dr.views.iter_mut().find(|x| x.id == v)) {
          view.hidden_lines = h;
        }
        Ok(v)
      })?;
      Ok(json!({"view": view}))
    })(),
    "drawing.remove_view" => (|| {
      let id = drawing_id(s, p)?;
      let view = req_u32(p, "view")?;
      s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        dr.views.retain(|v| v.id != view);
        dr.dimensions.retain(|x| !matches!(x, DrawingDim::Distance { view: v, .. } if *v == view));
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "drawing.move_view" => (|| {
      let id = drawing_id(s, p)?;
      let view = req_u32(p, "view")?;
      let at = at(p)?;
      s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        dr.views.iter_mut().find(|v| v.id == view).ok_or(CmdError::new("no such view"))?.at = at;
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "drawing.dimension" => (|| {
      let id = drawing_id(s, p)?;
      let dim = DrawingDim::Distance {
        view: req_u32(p, "view")?,
        a: req_vec3(p, "a")?,
        b: req_vec3(p, "b")?,
        orientation: match opt_str(p, "orientation").unwrap_or("aligned") {
          "aligned" => DimOrientation::Aligned,
          "horizontal" => DimOrientation::Horizontal,
          "vertical" => DimOrientation::Vertical,
          o => return Err(CmdError::new(format!("unknown orientation '{o}' (aligned, horizontal, vertical)"))),
        },
        offset: p.get("offset").and_then(Value::as_f64).unwrap_or(10.0),
      };
      s.edit(|d| {
        d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?.dimensions.push(dim);
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "drawing.note" => (|| {
      let id = drawing_id(s, p)?;
      let note =
        DrawingDim::Note { at: at(p)?.ok_or(CmdError::new("missing 'at'"))?, text: req_str(p, "text")?.to_string() };
      s.edit(|d| {
        d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?.dimensions.push(note);
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "drawing.title" => (|| {
      let id = drawing_id(s, p)?;
      let p = p.clone();
      s.edit(move |d| {
        let t = &mut d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?.title;
        for (k, slot) in [
          ("title", &mut t.title),
          ("author", &mut t.author),
          ("part_number", &mut t.part_number),
          ("material", &mut t.material),
          ("revision", &mut t.revision),
          ("company", &mut t.company),
        ] {
          if let Some(v) = opt_str(&p, k) {
            *slot = v.to_string();
          }
        }
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "drawing.delete" => (|| {
      let id = drawing_id(s, p)?;
      s.edit(|d| {
        d.drawings.retain(|x| x.id != id);
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "drawing.list" => {
      Ok(json!(s.design.drawings.iter().map(|d| json!({"id": d.id, "name": d.name})).collect::<Vec<_>>()))
    }
    "drawing.info" => (|| {
      let id = drawing_id(s, p)?;
      info(s, id)
    })(),
    "drawing.export" => (|| {
      let id = drawing_id(s, p)?;
      let path = PathBuf::from(req_str(p, "path")?);
      let d = s.design.drawing(id).ok_or(CmdError::new("no such drawing"))?;
      let plot = render(d, &s.design, &s.model).map_err(CmdError::new)?;
      let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
      match ext.as_str() {
        "svg" => std::fs::write(&path, plot.svg())?,
        "pdf" => std::fs::write(&path, plot.pdf())?,
        "dxf" => std::fs::write(&path, plot.dxf())?,
        o => return Err(CmdError::new(format!("unknown drawing format '.{o}' (svg, pdf, dxf)"))),
      }
      Ok(json!({"path": path, "items": plot.items.len()}))
    })(),
    _ => return None,
  })
}

/// `drawing`, or the only drawing.
fn drawing_id(s: &Session, p: &Value) -> Result<u32, CmdError> {
  if p.get("drawing").is_some() {
    let id = req_u32(p, "drawing")?;
    s.design.drawing(id).ok_or(CmdError::new(format!("no drawing {id}")))?;
    return Ok(id);
  }
  match s.design.drawings.as_slice() {
    [d] => Ok(d.id),
    [] => Err(CmdError::new("there is no drawing (drawing.create)")),
    _ => Err(CmdError::new("there are several drawings; pass 'drawing'")),
  }
}

fn at(p: &Value) -> Result<Option<[f64; 2]>, CmdError> {
  match p.get("at") {
    None | Some(Value::Null) => Ok(None),
    Some(v) => match v.as_array().map(|a| a.as_slice()) {
      Some([x, y]) => match (x.as_f64(), y.as_f64()) {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(Some([x, y])),
        _ => Err(CmdError::new("'at' must be [x, y] (sheet mm)")),
      },
      _ => Err(CmdError::new("'at' must be [x, y] (sheet mm)")),
    },
  }
}

/// "front" | ... | {"section": "x"|"y"|"z", "offset", "flip"?} | {"flat": body}
fn view_kind(v: &Value) -> Result<ViewKind, CmdError> {
  if let Some(n) = v.as_str() {
    return ViewKind::parse(n).ok_or(CmdError::new(format!(
      "unknown view '{n}' (front, back, top, bottom, left, right, iso, {{section}}, {{flat}})"
    )));
  }
  if let Some(a) = v.get("section").and_then(Value::as_str) {
    let axis = match a {
      "x" => Axis::X,
      "y" => Axis::Y,
      "z" => Axis::Z,
      o => return Err(CmdError::new(format!("unknown section axis '{o}' (x, y, z)"))),
    };
    let offset = v.get("offset").and_then(Value::as_f64).unwrap_or(0.0);
    let flip = v.get("flip").and_then(Value::as_bool).unwrap_or(false);
    return Ok(ViewKind::Section { axis, offset, flip });
  }
  if let Some(b) = v.get("flat").and_then(Value::as_u64) {
    return Ok(ViewKind::Flat { body: BodyId(u32::try_from(b).map_err(|_| CmdError::new("bad body"))?) });
  }
  Err(CmdError::new("a view is a name, {\"section\": axis, \"offset\"} or {\"flat\": body}"))
}

fn settings(d: &mut Drawing, p: &Value) -> Result<(), CmdError> {
  if let Some(sh) = opt_str(p, "sheet") {
    d.sheet = SheetSize::parse(sh).ok_or(CmdError::new(format!("unknown sheet '{sh}' (a4..a0, letter, tabloid)")))?;
  }
  match p.get("scale") {
    Some(Value::Null) => d.scale = None,
    Some(Value::String(t)) if t == "auto" => d.scale = None,
    Some(Value::String(t)) => d.scale = Some(parse_scale(t)?),
    Some(v) => d.scale = Some(v.as_f64().filter(|x| *x > 0.0).ok_or(CmdError::new("scale must be positive"))?),
    None => {}
  }
  if let Some(a) = opt_str(p, "projection") {
    d.projection = match a {
      "third" | "third_angle" => ProjectionAngle::ThirdAngle,
      "first" | "first_angle" => ProjectionAngle::FirstAngle,
      o => return Err(CmdError::new(format!("unknown projection '{o}' (third_angle, first_angle)"))),
    };
  }
  if let Some(b) = p.get("auto_dimensions").and_then(Value::as_bool) {
    d.auto_dimensions = b;
  }
  if let Some(n) = opt_str(p, "rename") {
    d.name = n.to_string();
  }
  Ok(())
}

/// "1:2" → 0.5, "2:1" → 2.
fn parse_scale(t: &str) -> Result<f64, CmdError> {
  let bad = || CmdError::new(format!("bad scale '{t}' (like 1:2 or 0.5)"));
  match t.split_once(':') {
    Some((a, b)) => {
      let (a, b): (f64, f64) = (a.trim().parse().map_err(|_| bad())?, b.trim().parse().map_err(|_| bad())?);
      if a > 0.0 && b > 0.0 { Ok(a / b) } else { Err(bad()) }
    }
    None => t.trim().parse::<f64>().ok().filter(|v| *v > 0.0).ok_or_else(bad),
  }
}

fn info(s: &Session, id: u32) -> CmdResult {
  let d = s.design.drawing(id).ok_or(CmdError::new("no such drawing"))?;
  let plot = render(d, &s.design, &s.model);
  let scale = d.scale.map(scale_text);
  Ok(json!({
    "drawing": d.id,
    "name": d.name,
    "sheet": format!("{:?}", d.sheet),
    "scale": scale.unwrap_or_else(|| "auto".into()),
    "views": d.views.iter().map(|v| json!({"id": v.id, "kind": v.kind, "at": v.at})).collect::<Vec<_>>(),
    "dimensions": d.dimensions.len(),
    "error": plot.as_ref().err(),
    "text": plot.as_ref().map(|p| p.texts().iter().map(|t| t.to_string()).collect::<Vec<_>>()).unwrap_or_default(),
  }))
}
