//! Exploded views (`view.explode`) and batch renders of components (`view.render_components`).
//! Both are display: neither touches the timeline, joints or what bodies are visible.

use std::collections::HashMap;
use std::path::PathBuf;

use fox_kernel::glam::DVec3;
use fox_model::explode::{ExplodeOrder, ExplodedView, subtree_bodies};
use fox_model::{BodyId, ComponentId, Design};
use serde_json::{Value, json};

use crate::commands::{opt_str, req_str};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "view.explode" => explode(s, p),
    "view.render_components" => render_components(s, p),
    _ => return None,
  })
}

/// A body's bounding box from its display mesh.
fn body_bounds(s: &Session, id: BodyId) -> Option<(DVec3, DVec3)> {
  let m = s.model.body(id)?.mesh()?;
  let mut it = m.positions.iter().map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
  let first = it.next()?;
  Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

/// Each moving body's offset in `view`.
pub fn offsets_of(s: &Session, view: &ExplodedView) -> HashMap<BodyId, DVec3> {
  view.offsets(&s.design, |b| body_bounds(s, b))
}

/// Offsets of the design's exploded view (empty when it isn't exploded).
pub fn offsets(s: &Session) -> HashMap<BodyId, DVec3> {
  s.design.exploded_view.as_ref().map(|v| offsets_of(s, v)).unwrap_or_default()
}

/// `"x" | "-z" | [x, y, z] | "auto"` → a direction (`None` = auto).
fn direction(v: &Value) -> Result<Option<[f64; 3]>, CmdError> {
  if let Some(a) = v.as_array() {
    let xs: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
    return match xs[..] {
      [x, y, z] if (x * x + y * y + z * z).sqrt() > 1e-12 && [x, y, z].iter().all(|c| c.is_finite()) => {
        Ok(Some([x, y, z]))
      }
      _ => Err(CmdError::new("'direction' must be three numbers, not all zero")),
    };
  }
  let s = v.as_str().ok_or(CmdError::new("'direction' is x, y, z, -x, -y, -z, auto or [x, y, z]"))?;
  let (neg, axis) = match s.trim().to_ascii_lowercase().as_str() {
    "auto" => return Ok(None),
    a => a.strip_prefix('-').map(|a| (true, a.to_string())).unwrap_or((false, a.trim_start_matches('+').to_string())),
  };
  let d = match axis.as_str() {
    "x" => [1.0, 0.0, 0.0],
    "y" => [0.0, 1.0, 0.0],
    "z" => [0.0, 0.0, 1.0],
    o => return Err(CmdError::new(format!("unknown direction '{o}' (x, y, z, -x, -y, -z, auto or [x, y, z])"))),
  };
  Ok(Some(if neg { d.map(|c| -c) } else { d }))
}

fn component_ids(s: &Session, v: &Value) -> Result<Vec<ComponentId>, CmdError> {
  let list = v.as_array().ok_or(CmdError::new("'components' is a list of component ids"))?;
  list
    .iter()
    .map(|c| {
      let id = c.as_u64().map(|n| ComponentId(n as u32)).or_else(|| {
        let n = c.as_str()?;
        s.design.components.iter().find(|x| x.name.eq_ignore_ascii_case(n)).map(|x| x.id)
      });
      id.filter(|id| s.design.component(*id).is_some()).ok_or_else(|| CmdError::new(format!("no component {c}")))
    })
    .collect()
}

/// Reads `{components?, direction?, factor?, order?, name?}` over `base`.
pub(crate) fn parse_view(s: &Session, p: &Value, base: ExplodedView) -> Result<ExplodedView, CmdError> {
  let mut v = base;
  if let Some(c) = p.get("components").filter(|c| !c.is_null()) {
    v.components = component_ids(s, c)?;
  }
  if let Some(d) = p.get("direction").filter(|d| !d.is_null()) {
    v.direction = direction(d)?;
  }
  if let Some(f) = p.get("factor").filter(|f| !f.is_null()) {
    v.factor = f
      .as_f64()
      .filter(|f| f.is_finite() && *f >= 0.0)
      .ok_or(CmdError::new("'factor' must be a number of at least 0"))?;
  }
  if let Some(o) = opt_str(p, "order") {
    v.order = match o {
      "position" => ExplodeOrder::Position,
      "browser" => ExplodeOrder::Browser,
      o => return Err(CmdError::new(format!("unknown order '{o}' (position, browser)"))),
    };
  }
  if let Some(n) = opt_str(p, "name") {
    let n = n.trim();
    if n.is_empty() {
      return Err(CmdError::new("an exploded view needs a name"));
    }
    v.name = n.to_string();
  }
  Ok(v)
}

/// What an exploded view does: each moving component and its offset.
fn describe(s: &Session, v: &ExplodedView) -> Value {
  let off = offsets_of(s, v);
  let parts: Vec<Value> = v
    .units(&s.design)
    .iter()
    .map(|(c, bodies)| {
      let o = bodies.iter().find_map(|b| off.get(b)).copied().unwrap_or(DVec3::ZERO);
      json!({"component": c.0, "name": s.design.component(*c).map(|c| c.name.clone()), "offset": o.to_array()})
    })
    .collect();
  json!({"exploded": true, "name": v.name, "direction": v.direction, "factor": v.factor, "order": v.order, "components": parts})
}

/// `{off?: true, components?, direction?, factor?, order?, name?}`: shows (or with `off`, clears)
/// the design's exploded view. Fields not given keep the current view's. A view setting, like a
/// named view: saved with the design, no regeneration, no undo step.
fn explode(s: &mut Session, p: &Value) -> CmdResult {
  if p.get("off").and_then(Value::as_bool).unwrap_or(false) {
    let was = s.design.exploded_view.take();
    s.dirty |= was.is_some();
    return Ok(json!({"exploded": false}));
  }
  if s.design.components.is_empty() {
    return Err(CmdError::new("there are no components to explode"));
  }
  let base = s.design.exploded_view.clone().unwrap_or_default();
  let v = parse_view(s, p, base)?;
  let out = describe(s, &v);
  s.design.exploded_view = Some(v);
  s.dirty = true;
  Ok(out)
}

/// `{explode?}` of `view.render` and mesh exports: `true` the design's view (or the default one),
/// `false` none, an object a view of its own; missing = the design's.
pub(crate) fn requested(s: &Session, p: &Value) -> Result<HashMap<BodyId, DVec3>, CmdError> {
  match p.get("explode") {
    None | Some(Value::Null) => Ok(offsets(s)),
    Some(Value::Bool(false)) => Ok(HashMap::new()),
    Some(Value::Bool(true)) => Ok(offsets_of(s, &s.design.exploded_view.clone().unwrap_or_default())),
    Some(o @ Value::Object(_)) => {
      Ok(offsets_of(s, &parse_view(s, o, s.design.exploded_view.clone().unwrap_or_default())?))
    }
    Some(_) => Err(CmdError::new("'explode' is true, false or {direction?, factor?, components?, order?}")),
  }
}

/// The leading part number of a component name: `01 - Letters` → `01`.
pub fn leading_number(name: &str) -> Option<String> {
  let digits: String = name.trim_start().chars().take_while(char::is_ascii_digit).collect();
  (!digits.is_empty()).then_some(digits)
}

/// A name made safe for a file: `<>:"/\|?*` and control characters become spaces.
pub fn sanitize(name: &str) -> String {
  let s: String = name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { ' ' } else { c }).collect();
  let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
  let s = s.trim_end_matches('.').trim().to_string();
  if s.is_empty() { "Untitled".into() } else { s }
}

/// The appearance a set of bodies wears: the library name of the first body's colour, its
/// `#rrggbb`, or `Default`.
fn appearance_name(s: &Session, bodies: &[BodyId]) -> String {
  let Some(c) = bodies.iter().find_map(|b| s.design.body_color(*b)) else { return "Default".into() };
  crate::features::APPEARANCES
    .iter()
    .find(|(_, rgb)| *rgb == c)
    .map(|(n, _)| n.to_string())
    .unwrap_or_else(|| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
}

/// One CSV field, quoted when it needs to be.
fn csv(field: &str) -> String {
  if field.contains([',', '"', '\n', '\r']) { format!("\"{}\"", field.replace('"', "\"\"")) } else { field.to_string() }
}

/// The camera of a batch render: `{yaw, pitch}` (degrees or expressions), a named or standard
/// view's name, or the default 15° around, 20° up. Returns the `view.render` fields.
fn camera(d: &Design, v: Option<&Value>) -> Result<Value, CmdError> {
  let deg = |x: &Value, d: f64| -> Value {
    match x {
      Value::Number(n) => json!(format!("{} deg", n.as_f64().unwrap_or(d))),
      Value::String(s) => json!(s),
      _ => json!(format!("{d} deg")),
    }
  };
  Ok(match v {
    None | Some(Value::Null) => json!({"yaw": "15 deg", "pitch": "20 deg"}),
    Some(Value::Object(o)) => {
      json!({"yaw": deg(o.get("yaw").unwrap_or(&Value::Null), 15.0), "pitch": deg(o.get("pitch").unwrap_or(&Value::Null), 20.0)})
    }
    Some(Value::String(name)) => match d.named_views.iter().find(|v| v.name.eq_ignore_ascii_case(name)) {
      // the named view's direction; each part is still fitted
      Some(nv) => json!({"yaw": format!("{} rad", nv.yaw), "pitch": format!("{} rad", nv.pitch), "fov": format!("{} rad", nv.fov), "ortho": nv.ortho}),
      None if fox_render::StandardView::parse(name).is_some() => json!({"view": name}),
      None => return Err(CmdError::new(format!("no named view '{name}'"))),
    },
    Some(_) => return Err(CmdError::new("'camera' is {yaw, pitch} or a view's name")),
  })
}

/// `{folder, size?, camera?, style?, ortho?, components?, label?, name_pattern?, manifest?,
/// background?}`: one square PNG per component (with the components below it) on its own, and a
/// manifest (`.json` and `.csv`) of part number, name, area and appearance. Bodies' visibility
/// is never touched: each image draws only its component's bodies.
fn render_components(s: &mut Session, p: &Value) -> CmdResult {
  let folder = PathBuf::from(req_str(p, "folder")?);
  std::fs::create_dir_all(&folder)?;
  let size = p.get("size").and_then(Value::as_u64).unwrap_or(1080).clamp(16, 8192);
  let edges = match opt_str(p, "style").unwrap_or("shaded") {
    "shaded" => false,
    "shaded_edges" => true,
    o => return Err(CmdError::new(format!("unknown style '{o}' (shaded, shaded_edges)"))),
  };
  let label = sanitize(opt_str(p, "label").unwrap_or(&s.design.name));
  let pattern = opt_str(p, "name_pattern").unwrap_or("{label} - {component}");
  let comps: Vec<ComponentId> = match p.get("components").filter(|c| !c.is_null()) {
    Some(c) => component_ids(s, c)?,
    None => s.design.components.iter().filter(|c| c.parent.is_none()).map(|c| c.id).collect(),
  };
  let mut base = camera(&s.design, p.get("camera"))?;
  base["ortho"] = json!(p.get("ortho").and_then(Value::as_bool).or(base["ortho"].as_bool()).unwrap_or(true));
  base["width"] = json!(size);
  base["height"] = json!(size);
  base["edges"] = json!(edges);
  base["explode"] = json!(false);
  if let Some(bg) = p.get("background") {
    base["background"] = bg.clone();
  }
  let mut parts = Vec::new();
  let mut rows = vec!["index,component,number,name,area_in2,area_mm2,appearance,image".to_string()];
  let mut used: Vec<String> = Vec::new();
  for (i, c) in comps.iter().enumerate() {
    let Some(comp) = s.design.component(*c) else { continue };
    let cname = comp.name.clone();
    let bodies: Vec<BodyId> = subtree_bodies(&s.design, *c).into_iter().filter(|b| s.model.body(*b).is_some()).collect();
    if bodies.is_empty() {
      continue;
    }
    let number = leading_number(&cname);
    let stem = pattern
      .replace("{label}", &label)
      .replace("{component}", &cname)
      .replace("{number}", number.as_deref().unwrap_or(""))
      .replace("{index}", &format!("{:02}", i + 1));
    let mut stem = sanitize(&stem);
    // two components with one name don't overwrite each other's image
    if used.iter().any(|u| u.eq_ignore_ascii_case(&stem)) {
      stem = format!("{stem} ({})", i + 1);
    }
    used.push(stem.clone());
    let path = folder.join(format!("{stem}.png"));
    let mut rp = base.clone();
    rp["bodies"] = json!(bodies.iter().map(|b| b.0).collect::<Vec<_>>());
    let png = crate::scene::render_png(s, &rp)?;
    std::fs::write(&path, &png)?;
    let area: f64 = bodies.iter().filter_map(|b| s.model.body(*b)?.shape.mass().ok()).map(|m| m.area).sum();
    let in2 = area / 645.16;
    let appearance = appearance_name(s, &bodies);
    rows.push(
      [
        (i + 1).to_string(),
        c.0.to_string(),
        number.clone().unwrap_or_default(),
        cname.clone(),
        format!("{in2:.4}"),
        format!("{area:.2}"),
        appearance.clone(),
        path.display().to_string(),
      ]
      .iter()
      .map(|f| csv(f))
      .collect::<Vec<_>>()
      .join(","),
    );
    parts.push(json!({
      "index": i + 1, "component": c.0, "number": number, "name": cname,
      "area_in2": in2, "area_mm2": area, "appearance": appearance, "image": path,
    }));
  }
  if parts.is_empty() {
    return Err(CmdError::new("no component with bodies to render"));
  }
  let manifest = sanitize(&opt_str(p, "manifest").unwrap_or("{label} - Part Data").replace("{label}", &label));
  let (jpath, cpath) = (folder.join(format!("{manifest}.json")), folder.join(format!("{manifest}.csv")));
  let doc = json!({"label": label, "design": s.design.name, "size": size, "parts": parts});
  std::fs::write(&jpath, serde_json::to_string_pretty(&doc).map_err(|e| CmdError::new(e.to_string()))?)?;
  std::fs::write(&cpath, rows.join("\n") + "\n")?;
  Ok(json!({"folder": folder, "images": parts.len(), "manifest": jpath, "csv": cpath, "parts": parts}))
}

/// Mesh export of an exploded view (`export {explode}`): each body's mesh moved by its offset.
pub(crate) fn exploded_meshes(
  s: &Session,
  p: &Value,
  bodies: &[&fox_model::regen::Body],
  weld: bool,
) -> Result<Vec<fox_exchange::Mesh>, CmdError> {
  let off = requested(s, p)?;
  let mut out = Vec::new();
  for b in bodies {
    let o = off.get(&b.id).copied().unwrap_or(DVec3::ZERO).as_vec3();
    for mut m in crate::files::body_meshes(s, &[*b], weld) {
      for q in m.positions.iter_mut() {
        *q = (fox_kernel::glam::Vec3::from(*q) + o).to_array();
      }
      out.push(m);
    }
  }
  Ok(out)
}
