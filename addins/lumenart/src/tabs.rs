//! The tab system that holds a layered sign together: Quick Tab puts stubs on a layer's edges,
//! Auto Flange stretches them over the standoff gap to the layer below, Slot Projector cuts the
//! receiver slots, Tab Builder adds the locking heads. LumenArt's numbers: 0.25" tabs, 0.063"
//! sheet, 0.200" x 0.085" slots, +0.025" bend allowance.

use serde_json::{Value, json};

use crate::geo::{Face, IN, Info, Topo, Topos, V3, Z, inch, lengths, near};
use crate::{fox, num};

/// How close a measured size must be to LumenArt's.
const TOL: f64 = 0.05; // mm, about 0.002"

/// A flat four-sided face `w` by `t` (a tab stub's end).
fn is_tab_end(t: &Topo, f: &Face, w: f64, th: f64) -> bool {
  let l = lengths(t, f);
  f.kind == "plane"
    && l.len() == 4
    && near(l[0], th, TOL)
    && near(l[1], th, TOL)
    && near(l[2], w, TOL)
    && near(l[3], w, TOL)
}

/// The design's list of points already handled by a tool (kept as a design attribute).
fn done_points(name: &str) -> Vec<V3> {
  let r =
    fox::execute("attribute.get", json!({"target": "design", "group": "LumenArt", "name": name})).unwrap_or_default();
  let text = r.as_array().and_then(|a| a.first()).map(|a| a["value"].clone()).or_else(|| r.get("value").cloned());
  let text = text.and_then(|t| t.as_str().map(str::to_string)).unwrap_or_default();
  serde_json::from_str::<Vec<Value>>(&text).unwrap_or_default().iter().map(V3::of).collect()
}

fn save_points(name: &str, pts: &[V3]) -> Result<(), String> {
  let list: Vec<Value> = pts.iter().map(|p| p.json()).collect();
  fox::execute(
    "attribute.set",
    json!({"target": "design", "group": "LumenArt", "name": name, "value": Value::Array(list).to_string()}),
  )?;
  Ok(())
}

fn tag(feature: &Value, name: &str) {
  if let Some(f) = feature.as_u64() {
    let _ =
      fox::execute("attribute.set", json!({"target": {"feature": f}, "group": "LumenArt", "name": name, "value": "1"}));
  }
}

fn checked(made: Value) -> Result<Value, String> {
  match made["error"].as_str() {
    Some(e) => Err(e.to_string()),
    None => Ok(made),
  }
}

/// Quick Tab: on each picked edge (on the top or bottom face of a layer) a stub `width` long
/// along the edge, `proud` past it and `depth` into the part, joined through the sheet. Its
/// end is a `width` x thickness face, which Auto Flange and Slot Projector look for.
pub fn quick_tab(v: &Value) -> Result<Value, String> {
  let (w, depth, proud) = (num(v, "width", 0.25 * IN), num(v, "depth", 0.125 * IN), num(v, "proud", 0.005 * IN));
  let picks = v["edges"].as_array().cloned().unwrap_or_default();
  // every pick is read before anything changes: a body's edge ids change once a tab joins it
  let mut topos = Topos::default();
  let mut plans = Vec::new();
  for p in &picks {
    let (Some(body), Some(edge)) = (p["body"].as_u64(), p["edge"].as_u64()) else { continue };
    let t = topos.get(body)?;
    let e = t.edge(edge).ok_or_else(|| format!("edge {edge} is not on body {body}"))?;
    // the sheet face is the flat face beside the edge with the larger area
    let mut faces: Vec<&Face> = e.faces.iter().filter_map(|f| t.face(*f)).collect();
    faces.sort_by(|a, b| b.area.total_cmp(&a.area));
    let sheet =
      faces.into_iter().find(|f| f.kind == "plane").ok_or("pick an edge on the flat top or bottom of a layer")?;
    let n = sheet.normal.unit();
    let th = t.thickness_at(sheet).unwrap_or(0.063 * IN);
    plans.push((body, e.clone(), n, th));
  }
  let mut made = 0;
  for (i, (body, e, n, th)) in plans.iter().enumerate() {
    let (body, n, th) = (*body, *n, *th);
    let at = e.mid;
    let tangent = e.tangent().across(n).unit();
    let side = n.cross(tangent).unit();
    // which side of the edge is material: probe just past it, half way through the sheet
    let probe = |d: f64| at.add(side.mul(d)).sub(n.mul(th / 2.0)).json();
    let r = fox::execute("body.classify", json!({"body": body, "points": [probe(0.3), probe(1.0)]}))?;
    let inside = r["inside"].as_array().is_some_and(|a| a.iter().any(|x| x == "in"));
    let out = if inside { -1.0 } else { 1.0 }; // +1: outward is the sketch's +y
    // along the edge: the tab's width, kept on a straight edge
    let (mut lo, mut hi) = (-w / 2.0, w / 2.0);
    if e.kind == "line" {
      let (a, b) = (e.start.sub(at).dot(tangent), e.end.sub(at).dot(tangent));
      lo = lo.max(a.min(b));
      hi = hi.min(a.max(b));
    }
    let sketch =
      fox::execute("sketch.create", json!({"plane": {"origin": at.json(), "normal": n.json(), "x": tangent.json()}}))?
        ["sketch"]
        .clone();
    fox::execute("sketch.rect", json!({"corner1": [lo, out * proud], "corner2": [hi, -out * depth]}))?;
    fox::execute("sketch.finish", json!({}))?;
    let _ = fox::execute("feature.rename", json!({"feature": sketch, "name": "Tab Sketch"}));
    let ext = checked(fox::execute(
      "extrude",
      json!({
        "sketch": sketch, "profiles": [[(lo + hi) / 2.0, out * (proud - depth) / 2.0]], "distance": th,
        "flip": true, "operation": "join", "targets": [body],
      }),
    )?)?;
    tag(&ext["feature"], "QuickTab");
    made += 1;
    if !fox::progress((i + 1) as f32 / plans.len() as f32, "Tabs") {
      return Err("cancelled".into());
    }
  }
  Ok(json!({"message": format!("{made} tab(s) placed"), "tabs": made}))
}

/// A tab stub's end face, as found before changing anything.
struct TabEnd {
  body: u64,
  center: V3,
  normal: V3,
  /// The end's long edge nearest the layer below (where the flange bends).
  bend: (V3, V3),
}

fn tab_ends(info: &Info, w: f64, th: f64, keep: impl Fn(V3) -> bool) -> Result<Vec<TabEnd>, String> {
  let mut out = Vec::new();
  for b in &info.bodies {
    let t = Topo::read(b.id)?;
    for f in t.faces.iter().filter(|f| is_tab_end(&t, f, w, th) && keep(f.normal)) {
      let long =
        t.face_edges(f).into_iter().filter(|e| near(e.length, w, TOL)).min_by(|a, b| a.mid.2.total_cmp(&b.mid.2));
      let Some(long) = long else { continue };
      out.push(TabEnd { body: b.id, center: f.center, normal: f.normal.unit(), bend: (long.start, long.end) });
    }
  }
  Ok(out)
}

/// The face of `body` now at `center` facing `normal` (ids change as a body is rebuilt).
fn face_at(body: u64, center: V3, normal: V3) -> Result<Option<u64>, String> {
  let t = Topo::read(body)?;
  Ok(t.faces.iter().find(|f| f.center.sub(center).len() < 0.01 && f.normal.dot(normal) > 0.99).map(|f| f.id))
}

/// Auto Flange: every tab stub sticking out sideways grows by the gap down to the layer under it
/// plus the bend allowance, and its bend line is drawn (as construction) in a "Bend Lines"
/// sketch for each layer height.
pub fn auto_flange(v: &Value) -> Result<Value, String> {
  let (allow, w, th) =
    (num(v, "allowance", 0.025 * IN), num(v, "tab_width", 0.25 * IN), num(v, "thickness", 0.063 * IN));
  let info = Info::read()?;
  let done = done_points("FlangeEnds");
  let ends = tab_ends(&info, w, th, |n| n.dot(Z).abs() < 0.3)?;
  let mut flanged = done.clone();
  let mut lines: Vec<(f64, V3, V3)> = Vec::new();
  let mut report = Vec::new();
  let mut skipped = 0;
  for (i, end) in ends.iter().enumerate() {
    if done.iter().any(|d| d.sub(end.center).len() < 0.05) {
      continue; // flanged on an earlier run
    }
    let Some(below) = info.below(end.center, end.body, 100.0 * IN) else {
      skipped += 1;
      report.push(format!("no layer under the tab at {}", fmt_pt(end.center)));
      continue;
    };
    let gap = end.center.2 - below.hi.2;
    let len = gap + allow;
    let Some(face) = face_at(end.body, end.center, end.normal)? else { continue };
    checked(fox::execute("press_pull", json!({"faces": [{"body": end.body, "face": face}], "distance": len}))?)?;
    flanged.push(end.center.add(end.normal.mul(len)));
    lines.push((end.bend.0.2, end.bend.0, end.bend.1));
    report.push(format!("{} long over a {} gap to {}", inch(len), inch(gap), below.name));
    if !fox::progress((i + 1) as f32 / ends.len().max(1) as f32, "Flanges") {
      return Err("cancelled".into());
    }
  }
  // the bend lines, one sketch per height
  lines.sort_by(|a, b| a.0.total_cmp(&b.0));
  let mut k = 0;
  while k < lines.len() {
    let z = lines[k].0;
    let sketch = fox::execute(
      "sketch.create",
      json!({"plane": {"origin": [0, 0, z], "normal": [0, 0, 1], "x": [1, 0, 0]}}),
    )?["sketch"]
      .clone();
    let mut curves = Vec::new();
    while k < lines.len() && (lines[k].0 - z).abs() < 0.01 {
      let (a, b) = (lines[k].1, lines[k].2);
      let r = fox::execute("sketch.line", json!({"points": [[a.0, a.1], [b.0, b.1]]}))?;
      curves.extend(r["curves"].as_array().cloned().unwrap_or_default());
      k += 1;
    }
    if !curves.is_empty() {
      fox::execute("sketch.construction", json!({"curves": curves, "on": true}))?;
    }
    fox::execute("sketch.finish", json!({}))?;
    let _ = fox::execute("feature.rename", json!({"feature": sketch, "name": "Bend Lines"}));
  }
  save_points("FlangeEnds", &flanged)?;
  let n = flanged.len() - done.len();
  let mut msg = format!("{n} flange(s)");
  if skipped > 0 {
    msg.push_str(&format!(", {skipped} tab(s) with no layer under them"));
  }
  Ok(json!({"message": msg, "flanges": n, "details": report}))
}

fn fmt_pt(p: V3) -> String {
  format!("({:.3}, {:.3}, {:.3}) in", p.0 / IN, p.1 / IN, p.2 / IN)
}

/// Slot Projector: under every tab end that points down, a slot through the layer below,
/// `slot_length` along the tab and `slot_width` across it.
pub fn slot_projector(v: &Value) -> Result<Value, String> {
  let (sl, sw) = (num(v, "slot_length", 0.2 * IN), num(v, "slot_width", 0.085 * IN));
  let (w, th) = (num(v, "tab_width", 0.25 * IN), num(v, "thickness", 0.063 * IN));
  let dry = v["dry_run"].as_bool().unwrap_or(false);
  let info = Info::read()?;
  let done = done_points("SlotsAt");
  let ends = tab_ends(&info, w, th, |n| n.dot(Z) < -0.7)?;
  let mut cut = done.clone();
  let mut missing = 0;
  for (i, end) in ends.iter().enumerate() {
    if done.iter().any(|d| d.sub(end.center).len() < 0.05) {
      continue;
    }
    let Some(receiver) = info.below(end.center, end.body, 100.0 * IN) else {
      missing += 1;
      continue;
    };
    if dry {
      cut.push(end.center);
      continue;
    }
    let along = end.bend.1.sub(end.bend.0).across(end.normal).unit();
    let sketch = fox::execute(
      "sketch.create",
      json!({"plane": {"origin": end.center.json(), "normal": end.normal.json(), "x": along.json()}}),
    )?["sketch"]
      .clone();
    fox::execute("sketch.rect", json!({"center": [0, 0], "corner": [sl / 2.0, sw / 2.0]}))?;
    fox::execute("sketch.finish", json!({}))?;
    let _ = fox::execute("feature.rename", json!({"feature": sketch, "name": "Slot Sketch"}));
    let depth = end.center.2 - receiver.lo.2 + 0.2;
    let ext = checked(fox::execute(
      "extrude",
      json!({"sketch": sketch, "profiles": [[0, 0]], "distance": depth, "operation": "cut", "targets": [receiver.id]}),
    )?)?;
    tag(&ext["feature"], "Slot");
    cut.push(end.center);
    if !fox::progress((i + 1) as f32 / ends.len().max(1) as f32, "Slots") {
      return Err("cancelled".into());
    }
  }
  let n = cut.len() - done.len();
  if dry {
    return Ok(json!({"message": format!("{n} tab(s) point at a layer below, {missing} at nothing"), "tabs": n}));
  }
  save_points("SlotsAt", &cut)?;
  let mut msg = format!("{n} slot(s) cut");
  if missing > 0 {
    msg.push_str(&format!(", {missing} tab(s) with no layer under them"));
  }
  Ok(json!({"message": msg, "slots": n}))
}

/// A flange leg's broad face: `w` wide, 0.02" to 2" long, not a stub's end.
fn is_leg(t: &Topo, f: &Face, w: f64, th: f64) -> bool {
  let l = lengths(t, f);
  if f.kind != "plane" || l.len() != 4 || is_tab_end(t, f, w, th) {
    return false;
  }
  let wide: Vec<f64> = l.iter().copied().filter(|x| near(*x, w, 0.25)).collect();
  let long: Vec<f64> = l.iter().copied().filter(|x| !near(*x, w, 0.25) && (0.02 * IN..=2.0 * IN).contains(x)).collect();
  wide.len() == 2 && long.len() == 2
}

/// Tab Builder: on the free end of each flange leg a tab head (resources/tab_head.dxf: a 0.110"
/// neck, a 0.170" head) in the leg's plane, joined through its thickness.
pub fn tab_builder(v: &Value) -> Result<Value, String> {
  let (w, th_default) = (num(v, "tab_width", 0.25 * IN), num(v, "thickness", 0.063 * IN));
  let info = Info::read()?;
  let mut legs: Vec<(u64, V3, V3)> = Vec::new(); // body, face centre, normal
  let picked = v["faces"].as_array().cloned().unwrap_or_default();
  if picked.is_empty() {
    for b in &info.bodies {
      let t = Topo::read(b.id)?;
      for f in t.faces.iter().filter(|f| is_leg(&t, f, w, th_default)) {
        // the leg's two broad faces are one leg
        let twin =
          legs.iter().any(|(bb, c, n)| *bb == b.id && n.dot(f.normal).abs() > 0.95 && c.sub(f.center).len() < w);
        if !twin {
          legs.push((b.id, f.center, f.normal.unit()));
        }
      }
    }
  } else {
    for p in &picked {
      let (Some(body), Some(face)) = (p["body"].as_u64(), p["face"].as_u64()) else { continue };
      let t = Topo::read(body)?;
      let f = t.face(face).ok_or("the picked face is gone")?;
      legs.push((body, f.center, f.normal.unit()));
    }
  }
  let mut made = 0;
  let mut failed = Vec::new();
  for (i, (body, center, normal)) in legs.iter().enumerate() {
    match head_on(*body, *center, *normal, w, th_default, &info) {
      Ok(()) => made += 1,
      Err(e) => failed.push(e),
    }
    if !fox::progress((i + 1) as f32 / legs.len().max(1) as f32, "Tab heads") {
      return Err("cancelled".into());
    }
  }
  let mut msg = format!("{made} tab head(s)");
  if !failed.is_empty() {
    msg.push_str(&format!(", {} skipped: {}", failed.len(), failed[0]));
  }
  Ok(json!({"message": msg, "heads": made, "skipped": failed}))
}

fn head_on(body: u64, center: V3, normal: V3, w: f64, th_default: f64, info: &Info) -> Result<(), String> {
  let t = Topo::read(body)?;
  let f = t
    .faces
    .iter()
    .find(|f| f.center.sub(center).len() < 0.01 && f.normal.dot(normal) > 0.99)
    .ok_or("the leg's face is gone")?;
  let th = t.thickness_at(f).unwrap_or(th_default);
  let mut ends: Vec<_> = t.face_edges(f).into_iter().filter(|e| near(e.length, w, 0.25)).collect();
  if ends.len() != 2 {
    return Err("a leg needs two tab-wide ends".into());
  }
  // the free end has a small flat end face beyond it; the other one meets the bend or the layer
  let end_face = |e: &crate::geo::Edge| {
    t.across_edge(e, f.id).is_some_and(|g| g.kind == "plane" && near(g.area, w * th, w * th * 0.2 + 0.05))
  };
  let b = info.bodies.iter().find(|b| b.id == body);
  let middle = b.map(|b| b.lo.add(b.hi).mul(0.5)).unwrap_or_default();
  // the free end: the one with an end face; then the lower one (tabs point down into the
  // receiver); then the one further from the middle of the part
  let score = |e: &crate::geo::Edge| (end_face(e) as u8 as f64) * 1e9 - e.mid.2.round() * 1e3 + e.mid.sub(middle).len();
  ends.sort_by(|a, b| score(b).total_cmp(&score(a)));
  let (free, mount) = (ends[0], ends[1]);
  let out = free.mid.sub(mount.mid).across(normal).unit();
  let mut x = free.end.sub(free.start).across(normal).unit();
  if normal.cross(x).dot(out) < 0.0 {
    x = x.mul(-1.0);
  }
  let sketch = fox::execute(
    "sketch.create",
    json!({"plane": {"origin": free.mid.json(), "normal": normal.json(), "x": x.json()}}),
  )?["sketch"]
    .clone();
  let ins = fox::execute("sketch.insert", json!({"sketch": sketch, "from": {"path": "resources/tab_head.dxf"}}))?;
  let curves = ins["curves"].clone();
  // the DXF's shoulder (its top edge, middle) goes on the free edge's middle, the head pointing out
  let (lo, hi) = sketch_box(&sketch, &curves)?;
  let anchor = [(lo[0] + hi[0]) / 2.0, hi[1]];
  fox::execute(
    "sketch.transform",
    json!({"sketch": sketch, "curves": curves, "angle": 180, "center": anchor, "translate": [-anchor[0], -anchor[1]]}),
  )?;
  // the shoulder now runs along the bottom of the box: centre it on the origin
  let (lo, hi) = sketch_box(&sketch, &curves)?;
  let (dx, dy) = (-(lo[0] + hi[0]) / 2.0, -lo[1]);
  if dx.abs() > 0.001 || dy.abs() > 0.001 {
    fox::execute("sketch.transform", json!({"sketch": sketch, "curves": curves, "translate": [dx, dy]}))?;
  }
  let _ = fox::execute("sketch.remove_duplicates", json!({"sketch": sketch}));
  fox::execute("sketch.finish", json!({}))?;
  let _ = fox::execute("feature.rename", json!({"feature": sketch, "name": "Tab Head"}));
  // a point in the head: below the 0.0675" neck, in the 0.075" head
  let ext = checked(fox::execute(
    "extrude",
    json!({"sketch": sketch, "profiles": [[0, 0.105 * IN]], "distance": th, "flip": true, "operation": "join", "targets": [body]}),
  )?)?;
  tag(&ext["feature"], "TabHead");
  Ok(())
}

/// The box of some curves of a sketch, in sketch coordinates.
fn sketch_box(sketch: &Value, curves: &Value) -> Result<([f64; 2], [f64; 2]), String> {
  let ids: Vec<u64> = curves.as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
  let info = fox::execute("sketch.info", json!({"sketch": sketch}))?;
  let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
  for g in info["geometry"].as_array().into_iter().flatten() {
    if !g["curve"].as_u64().is_some_and(|c| ids.contains(&c)) {
      continue;
    }
    for k in 0..2 {
      lo[k] = lo[k].min(g["bbox"][0][k].as_f64().unwrap_or(f64::MAX));
      hi[k] = hi[k].max(g["bbox"][1][k].as_f64().unwrap_or(f64::MIN));
    }
  }
  if lo[0] > hi[0] {
    return Err("the tab head DXF has no curves".into());
  }
  Ok((lo, hi))
}

/// Auto Tab: Slot Projector, then Tab Builder, over the whole sign.
pub fn auto_tab(_: &Value) -> Result<Value, String> {
  let slots = slot_projector(&json!({}))?;
  let heads = tab_builder(&json!({}))?;
  Ok(json!({
    "message": format!("{}; {}", slots["message"].as_str().unwrap_or(""), heads["message"].as_str().unwrap_or("")),
    "slots": slots, "heads": heads,
  }))
}
