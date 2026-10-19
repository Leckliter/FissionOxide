//! Manufacture commands: setups, 2.5-axis milling operations, toolpaths and G-code.

use clf_model::BodyId;
use clf_model::cam::{self, OpKind, Operation, Setup, Tool};
use serde_json::{Value, json};

use crate::commands::{face_ref, opt_str, req_str};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "cam.setup" => setup(s, p),
    "cam.face" | "cam.pocket" | "cam.contour" | "cam.drill" => operation(s, name, p),
    "cam.generate" => generate(s, p),
    "cam.post" => post(s, p),
    "cam.list" => Ok(json!(s.design.cam.iter().map(|st| json!({
      "setup": st.id, "name": st.name, "body": st.body.map(|b| b.0),
      "operations": st.operations.iter().map(|o| json!({"name": o.name, "tool": o.tool.diameter})).collect::<Vec<_>>(),
    })).collect::<Vec<_>>())),
    "cam.delete" => delete(s, p),
    _ => return None,
  })
}

/// A number, or an expression string ("6 mm", "0.4").
fn num(p: &Value, k: &str, default: f64) -> f64 {
  match p.get(k) {
    Some(Value::String(e)) => clf_expr::eval(e, clf_expr::Kind::Length, &std::collections::HashMap::new())
      .or_else(|_| clf_expr::eval(e, clf_expr::Kind::Unitless, &std::collections::HashMap::new()))
      .unwrap_or(default),
    Some(v) => v.as_f64().unwrap_or(default),
    None => default,
  }
}

/// The setup a command names, or the only/last one.
fn setup_index(s: &Session, p: &Value) -> Result<usize, CmdError> {
  match p.get("setup").and_then(Value::as_u64) {
    Some(id) => s.design.cam.iter().position(|st| st.id as u64 == id).ok_or(CmdError::new(format!("no setup {id}"))),
    None if s.design.cam.is_empty() => Err(CmdError::new("there is no setup: cam.setup first")),
    None => Ok(s.design.cam.len() - 1),
  }
}

fn setup(s: &mut Session, p: &Value) -> CmdResult {
  let body = p.get("body").and_then(Value::as_u64).map(|b| BodyId(b as u32));
  if let Some(b) = body
    && s.model.body(b).is_none()
  {
    return Err(CmdError::new(format!("no body {}", b.0)));
  }
  if s.model.bodies.is_empty() {
    return Err(CmdError::new("there is nothing to machine"));
  }
  let id = s.design.cam.iter().map(|st| st.id).max().unwrap_or(0) + 1;
  let st = Setup {
    id,
    name: opt_str(p, "name").map(str::to_string).unwrap_or_else(|| format!("Setup{id}")),
    body,
    stock_side: num(p, "stock_side", 2.0),
    stock_top: num(p, "stock_top", 1.0),
    operations: Vec::new(),
  };
  let (lo, hi) = cam::stock(&st, &s.model).map_err(CmdError::new)?;
  s.edit(|d| {
    d.cam.push(st);
    Ok(())
  })?;
  Ok(json!({"setup": id, "stock": [lo.to_array(), hi.to_array()]}))
}

fn faces_of(s: &Session, p: &Value, k: &str) -> Result<Vec<clf_model::FaceRef>, CmdError> {
  let mut out = Vec::new();
  for f in p.get(k).and_then(Value::as_array).into_iter().flatten() {
    out.push(face_ref(s, f)?);
  }
  Ok(out)
}

fn operation(s: &mut Session, name: &str, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let t = p.get("tool").cloned().unwrap_or(json!({}));
  let diameter = match (t.get("diameter"), p.get("tool_diameter")) {
    (Some(_), _) => num(&t, "diameter", 0.0),
    (None, Some(_)) => num(p, "tool_diameter", 0.0),
    _ => return Err(CmdError::new("missing the tool diameter ('tool': {diameter} or 'tool_diameter')")),
  };
  let kind = match name {
    "cam.face" => OpKind::Face,
    "cam.pocket" => {
      let floors = faces_of(s, p, "floors")?;
      if floors.is_empty() {
        return Err(CmdError::new("missing 'floors' (the pocket floor faces)"));
      }
      OpKind::Pocket { floors }
    }
    "cam.contour" => OpKind::Contour {
      faces: faces_of(s, p, "faces")?,
      inside: p.get("inside").and_then(Value::as_bool).unwrap_or(false),
    },
    _ => OpKind::Drill { holes: faces_of(s, p, "holes")?, peck: num(p, "peck", 0.0) },
  };
  // tools are numbered by diameter unless given
  let used: Vec<&Tool> = s.design.cam.iter().flat_map(|st| st.operations.iter().map(|o| &o.tool)).collect();
  let number = t.get("number").and_then(Value::as_u64).map(|n| n as u32).unwrap_or_else(|| {
    used
      .iter()
      .find(|x| (x.diameter - diameter).abs() < 1e-9)
      .map(|x| x.number)
      .unwrap_or_else(|| used.iter().map(|x| x.number).max().unwrap_or(0) + 1)
  });
  let tool = Tool {
    number,
    diameter,
    rpm: num(&t, "rpm", 10000.0),
    feed: num(&t, "feed", num(p, "feed", 1000.0)),
    plunge: num(&t, "plunge", num(p, "plunge", 300.0)),
  };
  let label = match name {
    "cam.face" => "Face",
    "cam.pocket" => "2D Pocket",
    "cam.contour" => "2D Contour",
    _ => "Drill",
  };
  let n = s.design.cam[i].operations.iter().filter(|o| o.name.starts_with(label)).count() + 1;
  let op = Operation {
    name: opt_str(p, "name").map(str::to_string).unwrap_or_else(|| format!("{label}{n}")),
    kind,
    tool,
    stepover: num(p, "stepover", 0.4),
    stepdown: num(p, "stepdown", 2.0),
    stock_to_leave: num(p, "stock_to_leave", 0.0),
  };
  // check it computes before keeping it
  let probe = Setup { operations: vec![op.clone()], ..s.design.cam[i].clone() };
  let paths = cam::toolpaths(&probe, &s.model).map_err(CmdError::new)?;
  let moves = paths.first().map(|t| t.moves.len()).unwrap_or(0);
  let opname = op.name.clone();
  s.edit(|d| {
    d.cam[i].operations.push(op);
    Ok(())
  })?;
  Ok(json!({"operation": opname, "moves": moves}))
}

/// Computes the toolpaths of a setup (shown in the viewport) and reports their length and time.
fn generate(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let st = &s.design.cam[i];
  let paths = cam::toolpaths(st, &s.model).map_err(CmdError::new)?;
  let mut at = cam::origin(st, &s.model).map_err(CmdError::new)?;
  let mut report = Vec::new();
  let mut total = 0.0;
  for t in &paths {
    let (cut, rapid, time) = t.stats(at);
    total += time;
    at = t.moves.last().map(|m| m.to).unwrap_or(at);
    report.push(json!({
      "operation": t.name, "tool": t.tool.number, "diameter": t.tool.diameter, "moves": t.moves.len(),
      "cutting_mm": cut, "rapid_mm": rapid, "minutes": time,
    }));
  }
  let id = st.id;
  s.toolpaths = paths;
  s.toolpaths_of = Some((id, s.revision));
  Ok(json!({"setup": id, "operations": report, "minutes": total}))
}

/// Writes a setup's G-code (`.nc`, `.tap`, `.gcode`); returns it when no path is given.
fn post(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let st = &s.design.cam[i];
  let paths = cam::toolpaths(st, &s.model).map_err(CmdError::new)?;
  if paths.is_empty() {
    return Err(CmdError::new("the setup has no operations"));
  }
  let origin = cam::origin(st, &s.model).map_err(CmdError::new)?;
  let code = cam::gcode(st, &paths, origin);
  match opt_str(p, "path") {
    Some(path) => {
      std::fs::write(path, &code)?;
      Ok(json!({"path": path, "lines": code.lines().count()}))
    }
    None => Ok(json!({"gcode": code})),
  }
}

fn delete(s: &mut Session, p: &Value) -> CmdResult {
  let i = setup_index(s, p)?;
  let op = p.get("operation").map(|_| req_str(p, "operation").map(str::to_string)).transpose()?;
  s.edit(|d| {
    match &op {
      Some(name) => {
        let ops = &mut d.cam[i].operations;
        let before = ops.len();
        ops.retain(|o| o.name != *name);
        if ops.len() == before {
          return Err(CmdError::new(format!("no operation '{name}'")));
        }
      }
      None => {
        d.cam.remove(i);
      }
    }
    Ok(())
  })?;
  s.toolpaths.clear();
  s.toolpaths_of = None;
  Ok(json!({}))
}
