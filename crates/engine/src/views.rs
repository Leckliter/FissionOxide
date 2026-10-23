//! Named views: cameras saved in the design (Fusion's browser Named Views), and appearances on
//! single faces.

use fox_model::{FaceAppearance, NamedView};
use serde_json::{Value, json};

use crate::commands::{face_ref, opt_str, req_str, req_vec3};
use crate::features::APPEARANCES;
use crate::{CmdError, CmdResult, Session};

/// Names Top, Front, Right and Home are taken by the standard views.
const STANDARD: [&str; 4] = ["top", "front", "right", "home"];

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "view.named.save" => save(s, p),
    "view.named.list" => Ok(json!(s.design.named_views)),
    "view.named.get" => get(s, p),
    "view.named.rename" => rename(s, p),
    "view.named.delete" => delete(s, p),
    "face.appearance" => face_appearance(s, p),
    _ => return None,
  })
}

/// The first free "Named View N".
pub(crate) fn next_view_name(views: &[NamedView]) -> String {
  let mut i = 1;
  loop {
    let n = format!("Named View {i}");
    if !views.iter().any(|v| v.name.eq_ignore_ascii_case(&n)) {
      return n;
    }
    i += 1;
  }
}

fn check_name(s: &Session, name: &str, except: Option<usize>) -> Result<(), CmdError> {
  let n = name.trim();
  if n.is_empty() {
    return Err(CmdError::new("a named view needs a name"));
  }
  if STANDARD.iter().any(|x| x.eq_ignore_ascii_case(n)) {
    return Err(CmdError::new(format!("'{n}' is a standard view")));
  }
  if s.design.named_views.iter().enumerate().any(|(i, v)| Some(i) != except && v.name.eq_ignore_ascii_case(n)) {
    return Err(CmdError::new(format!("there is already a named view '{n}'")));
  }
  Ok(())
}

fn index(s: &Session, p: &Value) -> Result<usize, CmdError> {
  let n = req_str(p, "name")?;
  s.design
    .named_views
    .iter()
    .position(|v| v.name.eq_ignore_ascii_case(n))
    .ok_or_else(|| CmdError::new(format!("no named view '{n}'")))
}

fn finite(p: &Value, k: &str) -> Result<Option<f64>, CmdError> {
  match p.get(k) {
    None | Some(Value::Null) => Ok(None),
    Some(v) => match v.as_f64() {
      Some(x) if x.is_finite() => Ok(Some(x)),
      _ => Err(CmdError::new(format!("'{k}' must be a number"))),
    },
  }
}

/// `{name?, target, yaw, pitch, distance, fov?, ortho?}`: saves (or with an existing name,
/// replaces) a view.
fn save(s: &mut Session, p: &Value) -> CmdResult {
  let target = req_vec3(p, "target")?;
  if !target.iter().all(|x| x.is_finite()) {
    return Err(CmdError::new("'target' must be finite"));
  }
  let need = |k: &str| finite(p, k)?.ok_or_else(|| CmdError::new(format!("missing '{k}'")));
  let (yaw, pitch, distance) = (need("yaw")?, need("pitch")?, need("distance")?);
  if distance <= 0.0 {
    return Err(CmdError::new("'distance' must be positive"));
  }
  let fov = finite(p, "fov")?.unwrap_or(30f64.to_radians());
  if !(0.01..3.0).contains(&fov) {
    return Err(CmdError::new("'fov' is in radians, between 0.01 and 3"));
  }
  let ortho = p.get("ortho").and_then(Value::as_bool).unwrap_or(false);
  let name = match opt_str(p, "name") {
    Some(n) => n.trim().to_string(),
    None => next_view_name(&s.design.named_views),
  };
  let existing = s.design.named_views.iter().position(|v| v.name.eq_ignore_ascii_case(&name));
  check_name(s, &name, existing)?;
  let view = NamedView { name: name.clone(), target, yaw, pitch: pitch.clamp(-1.5691, 1.5691), distance, fov, ortho };
  // a view setting, like visibility: no regeneration, no undo step
  match existing {
    Some(i) => s.design.named_views[i] = view,
    None => s.design.named_views.push(view),
  }
  s.dirty = true;
  Ok(json!({"name": name}))
}

fn get(s: &Session, p: &Value) -> CmdResult {
  Ok(json!(s.design.named_views[index(s, p)?]))
}

fn rename(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let to = req_str(p, "to")?.trim().to_string();
  check_name(s, &to, Some(i))?;
  s.design.named_views[i].name = to.clone();
  s.dirty = true;
  Ok(json!({"name": to}))
}

fn delete(s: &mut Session, p: &Value) -> CmdResult {
  let i = index(s, p)?;
  let v = s.design.named_views.remove(i);
  s.dirty = true;
  Ok(json!({"deleted": v.name}))
}

/// `{faces: [face], appearance? | color?}`: sets (or with neither, clears) faces' appearance.
fn face_appearance(s: &mut Session, p: &Value) -> CmdResult {
  let list = p.get("faces").and_then(Value::as_array).ok_or(CmdError::new("missing 'faces'"))?;
  if list.is_empty() {
    return Err(CmdError::new("no faces given"));
  }
  let mut refs = Vec::new();
  let mut ids = Vec::new();
  for f in list {
    let r = face_ref(s, f)?;
    let b = s.model.body(r.body).ok_or(CmdError::new("no such body"))?;
    let id = r.resolve(b.faces(), b.mesh(), b.diagonal()).ok_or(CmdError::new("the face can't be found"))?;
    ids.push((r.body, id));
    refs.push(r);
  }
  let color = match (opt_str(p, "appearance"), p.get("color")) {
    (Some(name), _) => Some(
      APPEARANCES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, c)| *c)
        .ok_or_else(|| CmdError::new(format!("unknown appearance '{name}' (see 'appearances')")))?,
    ),
    (None, None | Some(Value::Null)) => None,
    (None, Some(c)) => Some(crate::commands::parse_color(c)?),
  };
  // what the faces had before goes
  let old: Vec<usize> =
    face_colors(s).into_iter().filter(|(b, f, ..)| ids.contains(&(*b, *f))).map(|(.., i)| i).collect();
  // and so does a render material on them: a face has one appearance, the one applied last
  let old_render: Vec<usize> = crate::rendering::face_materials(s)
    .into_iter()
    .filter(|(b, f, ..)| ids.contains(&(*b, *f)))
    .map(|(.., i)| i)
    .collect();
  s.edit(|d| {
    let mut i = 0;
    d.face_appearances.retain(|_| {
      i += 1;
      !old.contains(&(i - 1))
    });
    let mut i = 0;
    d.render.faces.retain(|_| {
      i += 1;
      !old_render.contains(&(i - 1))
    });
    if let Some(c) = color {
      d.face_appearances.extend(refs.into_iter().map(|face| FaceAppearance { face, color: c }));
    }
    Ok(())
  })?;
  Ok(json!({"faces": ids.iter().map(|(b, f)| json!({"body": b.0, "face": f})).collect::<Vec<_>>(), "color": color}))
}

/// Each face appearance as it resolves now: (body, face id, colour, index into the design's list).
/// Later entries win where two land on one face.
pub(crate) fn face_colors(s: &Session) -> Vec<(fox_model::BodyId, u32, [u8; 3], usize)> {
  let mut out = Vec::new();
  for (i, fa) in s.design.face_appearances.iter().enumerate() {
    let Some(b) = s.model.body(fa.face.body) else { continue };
    for f in fa.face.resolve_all(b.faces(), b.mesh(), b.diagonal()) {
      out.push((fa.face.body, f, fa.color, i));
    }
  }
  out
}

/// The face colours of one body, by face id (later appearances over earlier ones).
pub(crate) fn body_face_colors(s: &Session, body: fox_model::BodyId) -> std::collections::HashMap<u32, [u8; 3]> {
  let mut out = std::collections::HashMap::new();
  if s.design.face_appearances.iter().all(|fa| fa.face.body != body) {
    return out;
  }
  let Some(b) = s.model.body(body) else { return out };
  for fa in s.design.face_appearances.iter().filter(|fa| fa.face.body == body) {
    for f in fa.face.resolve_all(b.faces(), b.mesh(), b.diagonal()) {
      out.insert(f, fa.color);
    }
  }
  out
}
