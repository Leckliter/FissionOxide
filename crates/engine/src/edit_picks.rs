//! Edit Feature's picks made again on the rolled-back model, for the features whose edit
//! dialogs are their creation dialogs with live selection chips (Revolve's profiles and axis,
//! Shell's faces and bodies, Offset Plane's and Mirror's plane, the patterns' objects and
//! axis). `feature.edit`'s `picks` are parsed as the creation command parses them and become
//! fields of the stored feature; Fillet's and Chamfer's are in `commands::edit_picks`.

use fox_model::{BodyId, FeatureId, FeatureKind};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::commands::{axis_ref, body_sel, face_ref, ids, plane_ref, profile_points};
use crate::{CmdError, Session};

type Fields = Map<String, Value>;

fn put(out: &mut Fields, k: &str, v: impl Serialize) -> Result<(), CmdError> {
  out.insert(k.into(), serde_json::to_value(v)?);
  Ok(())
}

/// The picked `bodies`, each checked to exist.
fn bodies(s: &Session, picks: &Value) -> Result<Vec<BodyId>, CmdError> {
  let list: Vec<BodyId> = ids(picks, "bodies").into_iter().map(BodyId).collect();
  if let Some(b) = list.iter().find(|b| s.model.body(**b).is_none()) {
    return Err(CmdError::new(format!("no body {}", b.0)));
  }
  Ok(list)
}

/// The fields `picks` sets on `kind`, or None when the feature takes no picks this way.
pub(crate) fn fields(s: &Session, kind: &FeatureKind, picks: &Value) -> Option<Result<Fields, CmdError>> {
  let mut out = Fields::new();
  let r = match kind {
    FeatureKind::Revolve { sketch, .. } => revolve(s, *sketch, picks, &mut out),
    FeatureKind::Shell { .. } => shell(s, picks, &mut out),
    FeatureKind::ConstructionPlane { .. } => {
      picks.get("plane").map_or(Ok(()), |p| put(&mut out, "base", plane_ref(s, p)?))
    }
    FeatureKind::Mirror { .. } => mirror(s, picks, &mut out),
    FeatureKind::RectangularPattern { .. } => objects(s, picks, &mut out),
    FeatureKind::CircularPattern { .. } => objects(s, picks, &mut out).and_then(|()| match picks.get("axis") {
      Some(a) => put(&mut out, "axis", axis_ref(s, a)?),
      None => Ok(()),
    }),
    _ => return None,
  };
  Some(r.map(|()| out))
}

fn revolve(s: &Session, stored: FeatureId, picks: &Value, out: &mut Fields) -> Result<(), CmdError> {
  let sketch = match picks.get("sketch").and_then(Value::as_u64) {
    Some(id) => FeatureId(u32::try_from(id).map_err(|_| CmdError::new("bad sketch id"))?),
    None => stored,
  };
  s.design.sketch(sketch)?;
  if picks.get("profiles").is_some() {
    let profiles = profile_points(s, sketch, picks)?;
    if profiles.is_empty() {
      return Err(CmdError::new("pick a profile to revolve"));
    }
    put(out, "sketch", sketch)?;
    put(out, "profiles", profiles)?;
  }
  if let Some(a) = picks.get("axis") {
    put(out, "axis", axis_ref(s, a)?)?;
  }
  Ok(())
}

fn shell(s: &Session, picks: &Value, out: &mut Fields) -> Result<(), CmdError> {
  if let Some(list) = picks.get("faces").and_then(Value::as_array) {
    let mut picked = list.clone();
    if picks.get("tangent_chain").and_then(Value::as_bool).unwrap_or(false) {
      picked = crate::solid::tangent_faces(s, picked);
    }
    let faces = picked.iter().map(|f| face_ref(s, f)).collect::<Result<Vec<_>, _>>()?;
    put(out, "faces", faces)?;
  }
  if picks.get("bodies").is_some() {
    put(out, "bodies", bodies(s, picks)?)?;
  }
  if out.get("faces").and_then(Value::as_array).is_some_and(Vec::is_empty)
    && out.get("bodies").and_then(Value::as_array).is_some_and(Vec::is_empty)
  {
    return Err(CmdError::new("pick the faces to remove or the bodies to shell"));
  }
  Ok(())
}

fn mirror(s: &Session, picks: &Value, out: &mut Fields) -> Result<(), CmdError> {
  if let Some(p) = picks.get("plane") {
    put(out, "plane", plane_ref(s, p)?)?;
  }
  if picks.get("features").is_some() {
    let features: Vec<FeatureId> = ids(picks, "features").into_iter().map(FeatureId).collect();
    if let Some(f) = features.iter().find(|f| s.design.feature(**f).is_none()) {
      return Err(CmdError::new(format!("no feature {}", f.0)));
    }
    put(out, "features", features)?;
  }
  objects(s, picks, out)
}

/// The patterns' and Mirror's objects: the picked bodies.
fn objects(s: &Session, picks: &Value, out: &mut Fields) -> Result<(), CmdError> {
  if picks.get("bodies").is_some() {
    bodies(s, picks)?;
    put(out, "bodies", body_sel(picks))?;
  }
  Ok(())
}
