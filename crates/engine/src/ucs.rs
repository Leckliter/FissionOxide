//! User coordinate systems (`ucs.*`): named frames saved with the design, drawn by the canvas
//! while Object Visibility ▸ User Coordinate Systems is on. An undo step each, no regeneration.

use fox_kernel::glam::DVec3;
use fox_model::ucs::UserCoordinateSystem;
use serde_json::{Value, json};

use crate::commands::{opt_str, req_str};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "ucs.create" => create(s, p),
    "ucs.list" => Ok(list(s)),
    "ucs.delete" => delete(s, p),
    "ucs.rename" => rename(s, p),
    "ucs.visible" => visible(s, p),
    _ => return None,
  })
}

fn list(s: &Session) -> Value {
  let all: Vec<Value> = s
    .design
    .user_coordinate_systems
    .iter()
    .map(|u| {
      let [x, y, z] = u.axes();
      json!({"name": u.name, "origin": u.origin, "x": x.to_array(), "y": y.to_array(), "z": z.to_array(), "visible": u.visible})
    })
    .collect();
  json!({"coordinate_systems": all})
}

fn vec3(p: &Value, k: &str, default: DVec3) -> Result<DVec3, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(default),
    Some(_) => Ok(DVec3::from(crate::commands::req_vec3(p, k)?)),
  }
}

/// The next free name: UCS1, UCS2, …
fn next_name(s: &Session) -> String {
  let n = s
    .design
    .user_coordinate_systems
    .iter()
    .filter_map(|u| u.name.strip_prefix("UCS")?.parse::<usize>().ok())
    .max()
    .unwrap_or(0);
  format!("UCS{}", n + 1)
}

fn find(s: &Session, name: &str) -> Result<usize, CmdError> {
  s.design
    .user_coordinate_systems
    .iter()
    .position(|u| u.name.eq_ignore_ascii_case(name))
    .ok_or_else(|| CmdError::new(format!("no user coordinate system '{name}'")))
}

fn create(s: &mut Session, p: &Value) -> CmdResult {
  let name =
    opt_str(p, "name").map(str::trim).filter(|n| !n.is_empty()).map(String::from).unwrap_or_else(|| next_name(s));
  if find(s, &name).is_ok() {
    return Err(CmdError::new(format!("there is already a user coordinate system '{name}'")));
  }
  let origin = vec3(p, "origin", DVec3::ZERO)?;
  let x = vec3(p, "x", DVec3::X)?;
  let z = vec3(p, "z", DVec3::Z)?;
  let u = UserCoordinateSystem::new(name.clone(), origin, x, z)
    .ok_or(CmdError::new("x and z must be non-zero and not parallel"))?;
  s.edit_deferred(|d| {
    d.user_coordinate_systems.push(u);
    Ok(())
  })?;
  Ok(json!({"name": name}))
}

fn delete(s: &mut Session, p: &Value) -> CmdResult {
  let i = find(s, req_str(p, "name")?)?;
  s.edit_deferred(|d| {
    d.user_coordinate_systems.remove(i);
    Ok(())
  })?;
  Ok(json!({}))
}

fn rename(s: &mut Session, p: &Value) -> CmdResult {
  let i = find(s, req_str(p, "name")?)?;
  let to = req_str(p, "to")?.trim().to_string();
  if to.is_empty() || find(s, &to).is_ok_and(|j| j != i) {
    return Err(CmdError::new(format!("'{to}' can't be used as the name")));
  }
  s.edit_deferred(|d| {
    d.user_coordinate_systems[i].name = to;
    Ok(())
  })?;
  Ok(json!({}))
}

fn visible(s: &mut Session, p: &Value) -> CmdResult {
  let i = find(s, req_str(p, "name")?)?;
  let on = p.get("visible").and_then(Value::as_bool).ok_or(CmdError::new("'visible' is true or false"))?;
  s.edit_deferred(|d| {
    d.user_coordinate_systems[i].visible = on;
    Ok(())
  })?;
  Ok(json!({}))
}
