//! Intersect (Fusion's Project / Include > Intersect): the curves where bodies, or chosen faces
//! of them, cut the sketch plane. With the projection link (the default) the curves follow the
//! bodies, kept up to date at every regeneration like a spun profile; without it they are plain
//! fixed curves.

use fox_model::BodyId;
use serde_json::{Value, json};

use super::{req_u32, sketch_id};
use crate::{CmdError, CmdResult, Session};

pub(super) fn intersect(s: &mut Session, p: &Value) -> CmdResult {
  let sk = sketch_id(s, p)?;
  let plane = s.model.sketches.get(&sk).map(|r| r.plane).ok_or(CmdError::new("the sketch failed to build"))?;
  let associative = p.get("associative").and_then(Value::as_bool).unwrap_or(true);
  // whole bodies, by id or name
  let mut bodies: Vec<u32> = Vec::new();
  for v in p.get("bodies").and_then(Value::as_array).into_iter().flatten() {
    let b = match v {
      Value::String(n) => s.model.bodies.iter().find(|b| b.name == *n),
      _ => v.as_u64().and_then(|i| u32::try_from(i).ok()).and_then(|i| s.model.body(BodyId(i))),
    };
    let id = b.ok_or_else(|| CmdError::new(format!("no body {v}")))?.id.0;
    if !bodies.contains(&id) {
      bodies.push(id);
    }
  }
  // faces of bodies, as {body, face}
  let mut faces: Vec<[u32; 2]> = Vec::new();
  for f in p.get("faces").and_then(Value::as_array).into_iter().flatten() {
    let (body, face) = (req_u32(f, "body")?, req_u32(f, "face")?);
    let b = s.model.body(BodyId(body)).ok_or(CmdError::new(format!("no body {body}")))?;
    if !b.faces().iter().any(|x| x.id == face) {
      return Err(CmdError::new(format!("body {body} has no face {face}")));
    }
    if !faces.contains(&[body, face]) {
      faces.push([body, face]);
    }
  }
  if bodies.is_empty() && faces.is_empty() {
    return Err(CmdError::new("give the bodies or faces to intersect with the sketch plane"));
  }
  // a body named whole is sectioned whole; one named only by faces, just those faces
  let mut all: Vec<u32> = bodies.clone();
  for f in &faces {
    if !all.contains(&f[0]) {
      all.push(f[0]);
    }
  }
  let faces: Vec<[u32; 2]> = faces.into_iter().filter(|f| !bodies.contains(&f[0])).collect();
  let picks: Vec<(&fox_kernel::Shape, Vec<u32>)> = all
    .iter()
    .filter_map(|b| s.model.body(BodyId(*b)))
    .map(|b| (&b.shape, faces.iter().filter(|f| f[0] == b.id.0).map(|f| f[1]).collect()))
    .collect();
  // section it now, so bodies the plane misses are an error rather than an empty sketch
  let outline = fox_model::regen::intersect_outline(&picks, &plane).map_err(CmdError::new)?;
  let (id, curves) = s.edit(move |d| {
    let sketch = d.sketch_mut(sk)?;
    if associative {
      let id = sketch.add_section(all, faces);
      sketch.set_spun_outline(id, outline)?;
      let curves = sketch.spun(id).map(|x| x.curves.clone()).unwrap_or_default();
      Ok((Some(id), curves))
    } else {
      Ok((None, sketch.add_outline(&outline)?))
    }
  })?;
  let profiles = s.model.sketches.get(&sk).map(|r| r.profiles.len()).unwrap_or(0);
  let mut out = json!({"curves": curves.iter().map(|c| c.0).collect::<Vec<_>>(), "profiles": profiles});
  if let Some(id) = id {
    out["intersect"] = json!(id.0);
  }
  Ok(out)
}
