//! Construction picks: the objects Fusion's CONSTRUCT dialogs take in their "Select" chips
//! ([`super::Input::Construct`]), as references the plane, axis and point commands understand, and
//! where each one is (for the point a normal or tangent goes through).

use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{BodyId, FeatureId, FeatureKind, OriginPlane};
use fox_engine::sketch::{CurveId, CurveKind, PointId};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::Pick;
use crate::origin::OriginItem;

/// One picked object: its reference and the chips it can fill.
pub(super) struct Item {
  pub value: Value,
  plane: bool,
  line: bool,
  point: bool,
  path: bool,
  round: bool,
}

impl Item {
  fn new(value: Value) -> Item {
    Item { value, plane: false, line: false, point: false, path: false, round: false }
  }

  /// Can it fill a chip of this kind?
  pub fn fits(&self, what: Pick) -> bool {
    match what {
      Pick::Plane => self.plane,
      Pick::Line => self.line,
      Pick::Point => self.point,
      Pick::Path => self.path,
      Pick::Round => self.round,
      Pick::Any => true,
    }
  }
}

/// An origin item as a reference: "xy", "x", or the origin point.
pub(crate) fn origin_value(item: OriginItem) -> Value {
  match item {
    OriginItem::Point => json!({"at": [0.0, 0.0, 0.0], "origin": true}),
    OriginItem::X => json!("x"),
    OriginItem::Y => json!("y"),
    OriginItem::Z => json!("z"),
    o => o.sketch_plane().unwrap_or(json!("xy")),
  }
}

/// The session's picks, then the origin items picked while the form is open, in pick order.
pub(super) fn items(s: &Session, origin: &[Value]) -> Vec<Item> {
  let mut out = Vec::new();
  for x in &s.selection {
    let it = match *x {
      Sel::Face { body, face } => {
        let mut it = Item::new(json!({"body": body.0, "face": face}));
        it.plane = super::planar(s, body, face);
        it.round = !it.plane;
        it
      }
      Sel::Edge { body, edge } => {
        let mut it = Item::new(json!({"body": body.0, "edge": edge}));
        it.line = true;
        it.path = true;
        it.round = true;
        it
      }
      Sel::Vertex { body, edge, end } => {
        let mut it = Item::new(json!({"vertex": {"body": body.0, "edge": edge}, "end": end}));
        it.point = true;
        it
      }
      Sel::SketchPoint { sketch, point } => {
        let Some(at) = s.model.sketches.get(&sketch).and_then(|r| Some(r.plane.to_world(r.sketch.pos(point)?))) else {
          continue;
        };
        let mut it = Item::new(json!({"at": at.to_array(), "sketch": sketch.0, "sketch_point": point.0}));
        it.point = true;
        it
      }
      Sel::SketchCurve { sketch, curve } => {
        let mut it = Item::new(json!({"sketch": sketch.0, "curve": curve.0}));
        let kind = s.model.sketches.get(&sketch).and_then(|r| r.sketch.curve(curve)).map(|c| &c.kind);
        it.line = matches!(kind, Some(CurveKind::Line { .. }));
        it.path = true;
        it
      }
      Sel::Feature { feature } => match s.design.feature(feature).map(|f| &f.kind) {
        Some(FeatureKind::ConstructionPlane { .. } | FeatureKind::PlaneBy { .. }) => {
          let mut it = Item::new(json!({"feature": feature.0}));
          it.plane = true;
          it
        }
        Some(FeatureKind::ConstructionAxis { .. }) => {
          let mut it = Item::new(json!({"axis": feature.0}));
          it.line = true;
          it
        }
        Some(FeatureKind::ConstructionPoint { .. }) => {
          let mut it = Item::new(json!({"point": feature.0}));
          it.point = true;
          it
        }
        _ => continue,
      },
      _ => continue,
    };
    out.push(it);
  }
  for v in origin {
    let mut it = Item::new(v.clone());
    match v.as_str() {
      Some("x" | "y" | "z") => it.line = true,
      Some(_) => it.plane = true,
      None => it.point = true,
    }
    out.push(it);
  }
  out
}

/// The selection a pick came from (none for an origin item, which the form keeps itself).
pub(super) fn to_sel(v: &Value) -> Option<Sel> {
  let id = |k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
  if let Some(e) = v.get("vertex") {
    let body = BodyId(e.get("body")?.as_u64()? as u32);
    let edge = e.get("edge")?.as_u64()? as u32;
    return Some(Sel::Vertex { body, edge, end: v.get("end").and_then(Value::as_bool).unwrap_or(false) });
  }
  if let (Some(sketch), Some(point)) = (id("sketch"), id("sketch_point")) {
    return Some(Sel::SketchPoint { sketch: FeatureId(sketch), point: PointId(point) });
  }
  if let (Some(sketch), Some(curve)) = (id("sketch"), id("curve")) {
    return Some(Sel::SketchCurve { sketch: FeatureId(sketch), curve: CurveId(curve) });
  }
  let feature = id("feature").or_else(|| id("axis")).or_else(|| id("point"))?;
  Some(Sel::Feature { feature: FeatureId(feature) })
}

/// A plane reference's normal and a point on it.
fn plane_of(s: &Session, v: &Value) -> Option<(DVec3, DVec3)> {
  if let Some(name) = v.as_str() {
    let o = match name {
      "xz" => OriginPlane::Xz,
      "yz" => OriginPlane::Yz,
      _ => OriginPlane::Xy,
    };
    let pl = o.plane(s.design.frames());
    return Some((pl.normal(), pl.origin));
  }
  if let Some(f) = v.get("feature").and_then(Value::as_u64) {
    let pl = s.model.planes.get(&FeatureId(f as u32))?;
    return Some((pl.normal(), pl.origin));
  }
  let body = s.model.body(BodyId(v.get("body")?.as_u64()? as u32))?;
  let face = v.get("face")?.as_u64()? as u32;
  body.faces().iter().find(|f| f.id == face).map(|f| (f.normal, f.center))
}

/// Where a picked object is: a point's position, a face's centre, an edge's midpoint, an axis's
/// or plane's origin.
pub(super) fn position(s: &Session, v: &Value) -> Option<[f64; 3]> {
  if let Some(at) = v.get("at").and_then(Value::as_array) {
    let a: Vec<f64> = at.iter().filter_map(Value::as_f64).collect();
    return (a.len() == 3).then(|| [a[0], a[1], a[2]]);
  }
  if v.is_string() {
    return Some([0.0; 3]);
  }
  let id = |k: &str| v.get(k).and_then(Value::as_u64).map(|x| FeatureId(x as u32));
  if let Some(f) = id("point") {
    return s.model.points.get(&f).map(|p| p.to_array());
  }
  if let Some(f) = id("axis") {
    return s.model.axes.get(&f).map(|(o, _)| o.to_array());
  }
  if let Some(e) = v.get("vertex") {
    let body = s.model.body(BodyId(e.get("body")?.as_u64()? as u32))?;
    let edge = e.get("edge")?.as_u64()? as u32;
    let info = body.edges().iter().find(|x| x.id == edge)?;
    let end = v.get("end").and_then(Value::as_bool).unwrap_or(false);
    return Some(if end { info.end } else { info.start }.to_array());
  }
  if let Some(edge) = v.get("edge").and_then(Value::as_u64) {
    let body = s.model.body(BodyId(v.get("body")?.as_u64()? as u32))?;
    return body.edges().iter().find(|x| x.id == edge as u32).map(|e| e.mid.to_array());
  }
  plane_of(s, v).map(|(_, o)| o.to_array())
}

/// The point a tangent plane touches its face nearest, from the Reference pick: a point's
/// position, or for a plane far out along its normal from the face (the tangent plane then lies
/// parallel to the reference, as Fusion's does at 0°).
pub(super) fn reference_near(s: &Session, face: Option<[f64; 3]>, reference: &Value) -> Option<[f64; 3]> {
  let point_like = reference.get("at").is_some()
    || reference.get("point").is_some()
    || reference.get("vertex").is_some()
    || reference.get("edge").is_some()
    || reference.get("axis").is_some()
    || matches!(reference.as_str(), Some("x" | "y" | "z"));
  if !point_like && let Some((n, _)) = plane_of(s, reference) {
    let c = DVec3::from(face?);
    return Some((c + n * 1e4).to_array());
  }
  position(s, reference)
}

/// Plane at Angle's 0° plane for a picked line: a planar face along the edge, or the sketch's
/// plane for a sketch line (none: the command's default, XY).
pub(super) fn angle_base(s: &Session, line: &Value) -> Option<Value> {
  if let Some(sk) = line.get("sketch").and_then(Value::as_u64) {
    let pl = s.model.sketches.get(&FeatureId(sk as u32))?.plane;
    let (o, n, x) = (pl.origin, pl.normal(), pl.x);
    return Some(json!({"origin": o.to_array(), "normal": n.to_array(), "x": x.to_array()}));
  }
  let b = BodyId(line.get("body")?.as_u64()? as u32);
  let edge = line.get("edge")?.as_u64()? as u32;
  let info = s.model.body(b)?.edges().iter().find(|x| x.id == edge)?.faces;
  let face = info.into_iter().find(|f| *f != 0 && super::planar(s, b, *f))?;
  Some(json!({"body": b.0, "face": face}))
}
