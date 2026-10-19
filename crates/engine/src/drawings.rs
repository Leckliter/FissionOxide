//! Drawing commands: create drawings, add views, dimensions and notes, and export them.

use std::path::PathBuf;

use fox_model::drawing::{
  Axis, BreakDirection, Characteristic, DEFAULT_GAP, Datums, DimOrientation, Drawing, DrawingDim, FinishProcess,
  MaterialCondition, ProjectionAngle, SheetSize, Standard, ViewBreak, ViewKind, render, scale_text,
};
use fox_model::{BodyId, ComponentId};
use serde_json::{Value, json};

use crate::commands::{expr_of, ids, opt_str, req_str, req_u32, req_vec3};
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
        dr.dimensions.retain(|x| x.view() != Some(view));
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
    "drawing.break" => (|| {
      let id = drawing_id(s, p)?;
      let view = req_u32(p, "view")?;
      let direction = match opt_str(p, "direction") {
        None => BreakDirection::Horizontal,
        Some(d) => BreakDirection::parse(d)
          .ok_or(CmdError::new(format!("unknown break direction '{d}' (horizontal, vertical)")))?,
      };
      let (from, to) = (req_num(s, p, "from")?, req_num(s, p, "to")?);
      if (from - to).abs() < 1e-9 {
        return Err(CmdError::new("a break needs 'from' and 'to' apart"));
      }
      let gap = opt_num(s, p, "gap")?.unwrap_or(DEFAULT_GAP);
      if gap < 0.0 {
        return Err(CmdError::new("the break gap can't be negative"));
      }
      let brk = ViewBreak { direction, from: from.min(to), to: from.max(to), gap };
      s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        let v = dr.views.iter_mut().find(|v| v.id == view).ok_or(CmdError::new("no such view"))?;
        if matches!(v.kind, ViewKind::Iso) {
          return Err(CmdError::new("an isometric view can't be broken"));
        }
        v.breaks.push(brk);
        Ok(())
      })?;
      Ok(json!({"view": view}))
    })(),
    "drawing.remove_breaks" => (|| {
      let id = drawing_id(s, p)?;
      let view = req_u32(p, "view")?;
      s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        dr.views.iter_mut().find(|v| v.id == view).ok_or(CmdError::new("no such view"))?.breaks.clear();
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "drawing.ordinate" => (|| {
      let id = drawing_id(s, p)?;
      let points = points_of(p, "points")?;
      if points.is_empty() {
        return Err(CmdError::new("an ordinate dimension needs 'points'"));
      }
      let dim = DrawingDim::Ordinate {
        view: view_of(s, id, p)?,
        origin: req_vec3(p, "origin")?,
        points,
        orientation: match opt_str(p, "orientation").or(opt_str(p, "direction")).unwrap_or("horizontal") {
          "horizontal" | "x" => DimOrientation::Horizontal,
          "vertical" | "y" => DimOrientation::Vertical,
          o => return Err(CmdError::new(format!("unknown orientation '{o}' (horizontal, vertical)"))),
        },
        offset: opt_num(s, p, "offset")?.unwrap_or(10.0),
      };
      annotate(s, id, dim)
    })(),
    "drawing.angle" => (|| {
      let id = drawing_id(s, p)?;
      let (vertex, a, b) = (req_vec3(p, "vertex")?, req_vec3(p, "a")?, req_vec3(p, "b")?);
      if a == vertex || b == vertex {
        return Err(CmdError::new("'a' and 'b' must differ from the vertex"));
      }
      let radius = opt_num(s, p, "radius")?.unwrap_or(15.0);
      annotate(s, id, DrawingDim::Angle { view: view_of(s, id, p)?, vertex, a, b, radius })
    })(),
    "drawing.feature_control" => (|| {
      let id = drawing_id(s, p)?;
      let std = s.design.drawing(id).map(|d| d.standard).unwrap_or_default();
      let name = req_str(p, "characteristic")?;
      let characteristic = Characteristic::parse(name).ok_or(CmdError::new(format!(
        "unknown characteristic '{name}' ({})",
        Characteristic::ALL.map(|c| c.key()).join(", ")
      )))?;
      if !characteristic.in_standard(std) {
        return Err(CmdError::new(format!(
          "ASME Y14.5-2018 has no {name} tolerance (use position or profile, or switch the drawing to ISO)"
        )));
      }
      let datums = labels(p, "datums")?;
      match characteristic.datums() {
        Datums::None if !datums.is_empty() => {
          return Err(CmdError::new(format!("a {name} tolerance takes no datums")));
        }
        Datums::Required if datums.is_empty() => {
          return Err(CmdError::new(format!("a {name} tolerance needs a datum reference")));
        }
        _ => {}
      }
      if datums.len() > 3 {
        return Err(CmdError::new("at most three datum references (primary, secondary, tertiary)"));
      }
      let tolerance = req_num(s, p, "tolerance")?;
      if tolerance <= 0.0 {
        return Err(CmdError::new("the tolerance must be positive"));
      }
      let modifier = match opt_str(p, "modifier").unwrap_or("none") {
        "none" | "rfs" | "" => None,
        "mmc" | "m" => Some(MaterialCondition::Mmc),
        "lmc" | "l" => Some(MaterialCondition::Lmc),
        o => return Err(CmdError::new(format!("unknown modifier '{o}' (none, mmc, lmc)"))),
      };
      let dim = DrawingDim::FeatureControl {
        view: view_of(s, id, p)?,
        point: req_vec3(p, "point")?,
        offset: offset_of(p)?.unwrap_or([12.0, 12.0]),
        characteristic,
        tolerance,
        diameter: p.get("diameter").and_then(Value::as_bool).unwrap_or(false),
        modifier,
        datums,
      };
      annotate(s, id, dim)
    })(),
    "drawing.datum" => (|| {
      let id = drawing_id(s, p)?;
      let label = match opt_str(p, "label").map(str::trim) {
        Some(l) if !l.is_empty() => l.to_ascii_uppercase(),
        _ => next_datum(s.design.drawing(id).ok_or(CmdError::new("no such drawing"))?),
      };
      // I, O and Q read as numbers (ASME Y14.5 and ISO 5459 both skip them)
      if !label.chars().all(|c| c.is_ascii_alphabetic()) || label.contains(['I', 'O', 'Q']) {
        return Err(CmdError::new(format!("'{label}' isn't a datum letter (A-Z without I, O, Q)")));
      }
      let dim = DrawingDim::Datum {
        view: view_of(s, id, p)?,
        point: req_vec3(p, "point")?,
        offset: offset_of(p)?.unwrap_or([0.0, 12.0]),
        label: label.clone(),
      };
      let mut r = annotate(s, id, dim)?;
      r["label"] = json!(label);
      Ok(r)
    })(),
    "drawing.surface_finish" => (|| {
      let id = drawing_id(s, p)?;
      let roughness = req_num(s, p, "roughness")?;
      if roughness <= 0.0 {
        return Err(CmdError::new("the roughness must be positive"));
      }
      let process = match opt_str(p, "process") {
        None => FinishProcess::Any,
        Some(t) => {
          FinishProcess::parse(t).ok_or(CmdError::new(format!("unknown process '{t}' (any, removal, no_removal)")))?
        }
      };
      let dim = DrawingDim::SurfaceFinish {
        view: view_of(s, id, p)?,
        point: req_vec3(p, "point")?,
        offset: offset_of(p)?.unwrap_or([0.0, 0.0]),
        roughness,
        process,
      };
      annotate(s, id, dim)
    })(),
    "drawing.balloon" => (|| {
      let id = drawing_id(s, p)?;
      let component = component_of(s, p)?;
      let point = match p.get("point") {
        None | Some(Value::Null) => component_center(s, component)?,
        Some(_) => req_vec3(p, "point")?,
      };
      let dim = DrawingDim::Balloon {
        view: view_of(s, id, p)?,
        point,
        offset: offset_of(p)?.unwrap_or([12.0, 12.0]),
        component,
      };
      annotate(s, id, dim)
    })(),
    "drawing.remove_dimension" => (|| {
      let id = drawing_id(s, p)?;
      let index = req_u32(p, "index")? as usize;
      s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        if index >= dr.dimensions.len() {
          return Err(CmdError::new(format!("no dimension {index} (the drawing has {})", dr.dimensions.len())));
        }
        dr.dimensions.remove(index);
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    _ => return None,
  })
}

/// Adds a dimension or annotation; returns its index (for drawing.remove_dimension).
fn annotate(s: &mut Session, id: u32, dim: DrawingDim) -> CmdResult {
  let index = s.edit(|d| {
    let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
    dr.dimensions.push(dim);
    Ok(dr.dimensions.len() - 1)
  })?;
  Ok(json!({"index": index}))
}

/// `view`, which must be in the drawing (default its first view).
fn view_of(s: &Session, id: u32, p: &Value) -> Result<u32, CmdError> {
  let d = s.design.drawing(id).ok_or(CmdError::new("no such drawing"))?;
  match p.get("view") {
    None | Some(Value::Null) => d.views.first().map(|v| v.id).ok_or(CmdError::new("the drawing has no views")),
    Some(_) => {
      let v = req_u32(p, "view")?;
      d.views.iter().any(|x| x.id == v).then_some(v).ok_or(CmdError::new(format!("no view {v} in the drawing")))
    }
  }
}

/// A number: JSON, or an expression string ("0.05", "thick / 2") in the design's parameters.
fn opt_num(s: &Session, p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::String(t)) if t.trim().is_empty() => Ok(None),
    Some(v) => Ok(Some(fox_expr::eval(&expr_of(v)?, fox_expr::Kind::Length, &s.model.params)?)),
  }
}

fn req_num(s: &Session, p: &Value, k: &str) -> Result<f64, CmdError> {
  opt_num(s, p, k)?.ok_or_else(|| CmdError::new(format!("missing '{k}'")))
}

/// `offset`: [x, y] sheet mm.
fn offset_of(p: &Value) -> Result<Option<[f64; 2]>, CmdError> {
  match p.get("offset") {
    None | Some(Value::Null) => Ok(None),
    Some(v) => at(&json!({"at": v})).map_err(|_| CmdError::new("'offset' must be [x, y] (sheet mm)")),
  }
}

/// Model points: [[x,y,z], …] or "x y z; x y z" (as typed in a dialog).
fn points_of(p: &Value, k: &str) -> Result<Vec<[f64; 3]>, CmdError> {
  let bad = || CmdError::new(format!("'{k}' must be [[x, y, z], ...] or \"x y z; x y z\""));
  match p.get(k) {
    None | Some(Value::Null) => Ok(Vec::new()),
    Some(Value::String(t)) => t
      .split(';')
      .filter(|q| !q.trim().is_empty())
      .map(|q| {
        let n: Vec<f64> =
          q.split([',', ' ']).filter(|x| !x.is_empty()).map(str::parse).collect::<Result<_, _>>().map_err(|_| bad())?;
        <[f64; 3]>::try_from(n).map_err(|_| bad())
      })
      .collect(),
    Some(Value::Array(a)) => a.iter().map(|v| req_vec3(&json!({"p": v}), "p").map_err(|_| bad())).collect(),
    Some(_) => Err(bad()),
  }
}

/// Datum letters: ["A", "B"] or "A B" / "A, B".
fn labels(p: &Value, k: &str) -> Result<Vec<String>, CmdError> {
  let list: Vec<String> = match p.get(k) {
    None | Some(Value::Null) => Vec::new(),
    Some(Value::String(t)) => t.split([',', ' ', '|']).filter(|x| !x.is_empty()).map(str::to_string).collect(),
    Some(Value::Array(a)) => a
      .iter()
      .map(|v| v.as_str().map(str::to_string).ok_or(CmdError::new(format!("'{k}' must be letters"))))
      .collect::<Result<_, _>>()?,
    Some(_) => return Err(CmdError::new(format!("'{k}' must be letters"))),
  };
  Ok(list.into_iter().map(|l| l.trim().to_ascii_uppercase()).filter(|l| !l.is_empty()).collect())
}

/// The first datum letter the drawing doesn't use yet.
fn next_datum(d: &Drawing) -> String {
  let used: Vec<&str> = d
    .dimensions
    .iter()
    .filter_map(|x| if let DrawingDim::Datum { label, .. } = x { Some(label.as_str()) } else { None })
    .collect();
  ('A'..='Z')
    .filter(|c| !"IOQ".contains(*c))
    .map(String::from)
    .find(|l| !used.contains(&l.as_str()))
    .unwrap_or("A".into())
}

/// `component`: an id or a name.
fn component_of(s: &Session, p: &Value) -> Result<ComponentId, CmdError> {
  let c = match p.get("component") {
    Some(Value::String(n)) => s.design.components.iter().find(|c| c.name == *n).map(|c| c.id),
    Some(_) => s.design.component(ComponentId(req_u32(p, "component")?)).map(|c| c.id),
    None => return Err(CmdError::new("missing 'component'")),
  };
  c.ok_or(CmdError::new("no such component"))
}

/// Centre of mass of a component's first body.
fn component_center(s: &Session, c: ComponentId) -> Result<[f64; 3], CmdError> {
  let comp = s.design.component(c).ok_or(CmdError::new("no such component"))?;
  let b = comp.bodies.iter().find_map(|b| s.model.body(*b)).ok_or(CmdError::new("the component has no bodies"))?;
  Ok(b.shape.mass().map_err(|e| CmdError::new(e.to_string()))?.center_of_mass.to_array())
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
  if let Some(parent) = v.get("detail").and_then(Value::as_u64) {
    return Ok(ViewKind::Detail {
      parent: u32::try_from(parent).map_err(|_| CmdError::new("bad view id"))?,
      center: req_vec3(v, "center")?,
      radius: v.get("radius").and_then(Value::as_f64).ok_or(CmdError::new("a detail view needs 'radius'"))?,
      factor: v.get("factor").and_then(Value::as_f64).unwrap_or(2.0),
    });
  }
  if let Some(b) = v.get("flat").and_then(Value::as_u64) {
    return Ok(ViewKind::Flat { body: BodyId(u32::try_from(b).map_err(|_| CmdError::new("bad body"))?) });
  }
  Err(CmdError::new(
    "a view is a name, {\"section\": axis, \"offset\"}, {\"flat\": body} or {\"detail\": view, \"center\": [x,y,z], \"radius\", \"factor\"?}",
  ))
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
  if let Some(b) = p.get("parts_list").and_then(Value::as_bool) {
    d.parts_list = b;
  }
  if let Some(n) = opt_str(p, "rename") {
    d.name = n.to_string();
  }
  if let Some(t) = opt_str(p, "standard") {
    d.standard = Standard::parse(t).ok_or(CmdError::new(format!("unknown standard '{t}' (asme, iso)")))?;
  }
  if let Some(b) = p.get("auto_balloons").and_then(Value::as_bool) {
    d.auto_balloons = b;
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
    "views": d.views.iter().map(|v| json!({"id": v.id, "kind": v.kind, "at": v.at, "breaks": v.breaks})).collect::<Vec<_>>(),
    "dimensions": d.dimensions.len(),
    "standard": d.standard,
    "auto_balloons": d.auto_balloons,
    "error": plot.as_ref().err(),
    "text": plot.as_ref().map(|p| p.texts().iter().map(|t| t.to_string()).collect::<Vec<_>>()).unwrap_or_default(),
  }))
}

#[cfg(test)]
mod tests;
