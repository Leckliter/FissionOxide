//! What Fusion's INSPECT analyses draw beyond a shading mode: the bodies an analysis applies to,
//! its opacity, the curvature map's and minimum radius's vertex colours, curvature combs along
//! edges and isocurves across faces (see [`crate::scene::view_options`]).

use fox_kernel::glam::DVec3;
use fox_kernel::{EdgeLine, FaceInfo, SurfaceKind, TriMesh};
use fox_model::BodyId;
use fox_render::{Polyline, Scene};
use serde_json::Value;

use crate::Session;

/// The curvature a curvature map colours by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CurvatureType {
  /// The product of the principal curvatures (as its square root, a curvature): flat along a
  /// cylinder, coloured on a sphere.
  #[default]
  Gaussian,
  /// The principal curvatures' average.
  Mean,
  /// The larger principal curvature.
  Max,
}

/// Vertex colours an analysis paints bodies with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coloring {
  /// Curvature Map: blue where flat through green and yellow to red where tightest, `scale`
  /// times as sensitive as the body's own range; `banded` in eight steps rather than smoothly.
  Curvature { kind: CurvatureType, scale: f64, banded: bool },
  /// Minimum Radius: red where the surface curves tighter than `min` (mm), through yellow to
  /// green at `max` (the largest tool radius); the body's own colour beyond it.
  MinRadius { min: f64, max: f64 },
}

/// The analysis settings that aren't the shading mode itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnalysisView {
  /// The bodies analysed; `None` = every body.
  pub bodies: Option<Vec<BodyId>>,
  /// How far the analysis fades back to the plain look (1 − opacity).
  pub fade: f32,
  pub coloring: Option<Coloring>,
  /// Curvature Comb: the edges, teeth per edge and tooth scale.
  pub combs: Vec<(BodyId, u32)>,
  pub comb_density: usize,
  pub comb_scale: f64,
  /// Isocurve: the faces (body, face; `None` = all the body's faces), which families (U, V)
  /// and how many curves of each.
  pub iso: Vec<(BodyId, Option<u32>)>,
  pub iso_families: [bool; 2],
  pub iso_count: usize,
}

impl AnalysisView {
  /// Is `body` one the analysis applies to?
  pub fn covers(&self, body: BodyId) -> bool {
    self.bodies.as_ref().is_none_or(|b| b.contains(&body))
  }
}

fn f(p: &Value, k: &str, or: f64) -> f64 {
  p.get(k).and_then(Value::as_f64).unwrap_or(or)
}

fn ids(v: Option<&Value>) -> Option<Vec<BodyId>> {
  Some(v?.as_array()?.iter().filter_map(Value::as_u64).map(|b| BodyId(b as u32)).collect())
}

/// `(body, id)` pairs from `[{"body", <key>}]`.
fn refs(v: Option<&Value>, key: &str) -> Vec<(BodyId, u32)> {
  v.and_then(Value::as_array)
    .map(|a| {
      a.iter().filter_map(|e| Some((BodyId(e.get("body")?.as_u64()? as u32), e.get(key)?.as_u64()? as u32))).collect()
    })
    .unwrap_or_default()
}

/// Reads the render parameters' analysis settings (`bodies`, `opacity`, and the kind's own:
/// `type`/`banded`/`scale` for a curvature map, `min`/`max` for minimum radius, `edges`/
/// `density`/`scale` for combs, `faces`/`type`/`count` for isocurves).
pub fn parse(kind: &str, p: &Value) -> AnalysisView {
  let mut v = AnalysisView {
    bodies: ids(p.get("bodies")),
    fade: (1.0 - f(p, "opacity", 100.0) / 100.0).clamp(0.0, 1.0) as f32,
    ..Default::default()
  };
  match kind {
    "curvature" => {
      let kind = match p.get("type").and_then(Value::as_str) {
        Some("mean") => CurvatureType::Mean,
        Some("max") => CurvatureType::Max,
        // the map has always coloured by the average normal turn; a parameter-less request
        // keeps that look
        Some("gaussian") => CurvatureType::Gaussian,
        _ => CurvatureType::Mean,
      };
      let banded = p.get("banded").and_then(Value::as_bool).unwrap_or(false);
      v.coloring = Some(Coloring::Curvature { kind, scale: f(p, "scale", 1.0).max(0.0), banded });
    }
    "min_radius" => {
      let min = f(p, "min", 0.0).max(0.0);
      v.coloring = Some(Coloring::MinRadius { min, max: f(p, "max", 25.0).max(min) });
    }
    "comb" => {
      v.combs = refs(p.get("edges"), "edge");
      v.comb_density = f(p, "density", 50.0).clamp(2.0, 500.0) as usize;
      v.comb_scale = f(p, "scale", 10.0).max(0.0);
    }
    "isocurve" => {
      v.iso = refs(p.get("faces"), "face").into_iter().map(|(b, f)| (b, Some(f))).collect();
      v.iso.extend(v.bodies.iter().flatten().map(|b| (*b, None)));
      v.iso_families = match p.get("type").and_then(Value::as_str) {
        Some("u") => [true, false],
        Some("v") => [false, true],
        _ => [true, true],
      };
      v.iso_count = f(p, "count", 5.0).clamp(1.0, 100.0) as usize;
    }
    _ => {}
  }
  v
}

/// Per vertex: the smallest, largest and average normal turn per unit length along the
/// mesh edges meeting there (about 1/radius), estimates of the principal and mean curvatures.
pub(crate) fn principal(m: &TriMesh) -> Vec<[f32; 3]> {
  let pos: Vec<DVec3> = m.positions.iter().map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)).collect();
  let nor: Vec<DVec3> = m.normals.iter().map(|n| DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64)).collect();
  let mut lo = vec![f64::INFINITY; pos.len()];
  let mut hi = vec![0.0f64; pos.len()];
  let mut sum = vec![0.0f64; pos.len()];
  let mut cnt = vec![0.0f64; pos.len()];
  for t in m.indices.chunks_exact(3) {
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      let (a, b) = (a as usize, b as usize);
      let (Some(pa), Some(pb), Some(na), Some(nb)) = (pos.get(a), pos.get(b), nor.get(a), nor.get(b)) else {
        continue;
      };
      let l = (*pb - *pa).length();
      if l > 1e-9 {
        let k = (*nb - *na).length() / l;
        for v in [a, b] {
          lo[v] = lo[v].min(k);
          hi[v] = hi[v].max(k);
          sum[v] += k;
          cnt[v] += 1.0;
        }
      }
    }
  }
  (0..pos.len())
    .map(|i| if cnt[i] > 0.0 { [lo[i] as f32, hi[i] as f32, (sum[i] / cnt[i]) as f32] } else { [0.0; 3] })
    .collect()
}

fn ramp(t: f32, stops: &[[f32; 3]]) -> [u8; 4] {
  let n = stops.len() - 1;
  let x = t.clamp(0.0, 1.0) * n as f32;
  let i = (x.floor() as usize).min(n - 1);
  let f = x - i as f32;
  let c: [f32; 3] = [0, 1, 2].map(|j| stops[i][j] + (stops[i + 1][j] - stops[i][j]) * f);
  [c[0] as u8, c[1] as u8, c[2] as u8, 255]
}

fn fade_to(c: [u8; 4], base: [u8; 4], fade: f32) -> [u8; 4] {
  let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * fade).round() as u8;
  [m(c[0], base[0]), m(c[1], base[1]), m(c[2], base[2]), 255]
}

/// The vertex colours `coloring` gives a body mesh whose curvatures are `k` (see [`principal`]),
/// faded towards `base` by `fade`.
pub(crate) fn colors(k: &[[f32; 3]], coloring: Coloring, base: [u8; 4], fade: f32) -> Vec<[u8; 4]> {
  match coloring {
    Coloring::Curvature { kind, scale, banded } => {
      let of = |k: &[f32; 3]| match kind {
        CurvatureType::Gaussian => (k[0] * k[1]).max(0.0).sqrt(),
        CurvatureType::Mean => k[2],
        CurvatureType::Max => k[1],
      };
      let vals: Vec<f32> = k.iter().map(of).collect();
      let mut sorted: Vec<f32> = vals.iter().copied().filter(|v| *v > 1e-6).collect();
      sorted.sort_by(f32::total_cmp);
      let top = sorted.get(sorted.len() * 95 / 100).copied().unwrap_or(1.0).max(1e-6);
      let stops = [[40.0, 80.0, 220.0], [40.0, 190.0, 90.0], [240.0, 210.0, 40.0], [225.0, 50.0, 40.0]];
      vals
        .iter()
        .map(|v| {
          let mut t = (v / top * scale as f32).clamp(0.0, 1.0);
          if banded {
            t = (t * 7.0).round() / 7.0;
          }
          fade_to(ramp(t, &stops), base, fade)
        })
        .collect()
    }
    Coloring::MinRadius { min, max } => {
      let stops = [[225.0, 50.0, 40.0], [240.0, 210.0, 40.0], [40.0, 190.0, 90.0]];
      k.iter()
        .map(|k| {
          let r = if k[1] > 1e-9 { 1.0 / k[1] as f64 } else { f64::INFINITY };
          let c = if r >= max {
            base
          } else if r <= min || max <= min {
            ramp(0.0, &stops)
          } else {
            ramp(((r - min) / (max - min)) as f32, &stops)
          };
          fade_to(c, base, fade)
        })
        .collect()
    }
  }
}

/// The colour of combs' teeth and outline.
const COMB_COLOR: [u8; 4] = [40, 90, 200, 255];
/// The colour of isocurves.
const ISO_COLOR: [u8; 4] = [70, 70, 210, 255];

fn line(points: Vec<DVec3>, color: [u8; 4]) -> Polyline {
  Polyline {
    points: points.iter().map(|p| p.as_vec3().to_array()).collect(),
    color,
    width: 1.0,
    id: 0,
    on_top: false,
    dashed: false,
    dash_dot: false,
    behind: false,
  }
}

/// `n` points spaced evenly along a polyline.
fn resample(pts: &[DVec3], n: usize) -> Vec<DVec3> {
  let total: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
  if pts.len() < 2 || total <= 1e-12 || n < 2 {
    return pts.to_vec();
  }
  let mut out = Vec::with_capacity(n);
  let (mut seg, mut at) = (0usize, 0.0f64);
  for i in 0..n {
    let want = total * i as f64 / (n - 1) as f64;
    while seg + 2 < pts.len() && at + (pts[seg + 1] - pts[seg]).length() < want {
      at += (pts[seg + 1] - pts[seg]).length();
      seg += 1;
    }
    let l = (pts[seg + 1] - pts[seg]).length();
    let t = if l > 1e-12 { ((want - at) / l).clamp(0.0, 1.0) } else { 0.0 };
    out.push(pts[seg].lerp(pts[seg + 1], t));
  }
  out
}

/// A curvature comb along an edge: `teeth` points along it, each with a tooth pointing away from
/// its centre of curvature, as long as the curvature times `scale` times `size` (a length that
/// makes teeth readable on the model), and the outline joining the teeth's tips.
pub(crate) fn comb(edge: &EdgeLine, teeth: usize, scale: f64, size: f64) -> Vec<Polyline> {
  let p = resample(&edge.points, teeth.max(3));
  let n = p.len();
  if n < 3 {
    return Vec::new();
  }
  let closed = (p[0] - p[n - 1]).length() < 1e-6;
  // the curvature vector by the second difference of evenly spaced points: towards the centre,
  // as long as 1/radius
  let h = (p[1] - p[0]).length().max(1e-12);
  let kappa = |i: usize| -> DVec3 {
    let (a, b, c) = match i {
      0 if closed => (p[n - 2], p[0], p[1]),
      0 => (p[0], p[1], p[2]),
      i if i == n - 1 && closed => (p[n - 2], p[n - 1], p[1]),
      i if i == n - 1 => (p[n - 3], p[n - 2], p[n - 1]),
      i => (p[i - 1], p[i], p[i + 1]),
    };
    (a + c - b * 2.0) / (h * h)
  };
  let tips: Vec<DVec3> = (0..n).map(|i| p[i] - kappa(i) * scale * size).collect();
  let mut out: Vec<Polyline> =
    (0..n).filter(|i| (tips[*i] - p[*i]).length() > 1e-9).map(|i| line(vec![p[i], tips[i]], COMB_COLOR)).collect();
  out.push(line(tips, COMB_COLOR));
  out
}

/// Where the plane `n·x = d` crosses the triangles `tris` of `m`: one segment per triangle it
/// cuts, lifted `lift` off the surface so the depth test doesn't hide it, kept when `keep`
/// accepts its midpoint.
fn slice(m: &TriMesh, tris: &[usize], n: DVec3, d: f64, lift: f64, keep: &dyn Fn(DVec3) -> bool) -> Vec<Polyline> {
  let v = |i: u32| -> (DVec3, DVec3) {
    let p = m.positions[i as usize];
    let q = m.normals.get(i as usize).copied().unwrap_or([0.0; 3]);
    (DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64), DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64))
  };
  let mut out = Vec::new();
  for &t in tris {
    let Some(tri) = m.indices.get(t * 3..t * 3 + 3) else { continue };
    let c = [v(tri[0]), v(tri[1]), v(tri[2])];
    let s = c.map(|(p, _)| n.dot(p) - d);
    let mut hits = Vec::new();
    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
      if (s[a] < 0.0) != (s[b] < 0.0) {
        let t = s[a] / (s[a] - s[b]);
        let p = c[a].0.lerp(c[b].0, t);
        let nn = c[a].1.lerp(c[b].1, t).normalize_or_zero();
        hits.push(p + nn * lift);
      }
    }
    if let [a, b] = hits[..]
      && keep((a + b) / 2.0)
    {
      out.push(line(vec![a, b], ISO_COLOR));
    }
  }
  out
}

/// Isocurves across a face whose triangles are `tris`: `count` curves of each family asked for,
/// by the face's own parameter directions where its surface has them (lines across a plane,
/// lines along and circles around a cylinder, cone or torus, meridians and parallels on a
/// sphere), else straight across two directions in it.
pub(crate) fn isocurves(
  m: &TriMesh,
  tris: &[usize],
  face: &FaceInfo,
  families: [bool; 2],
  count: usize,
  lift: f64,
) -> Vec<Polyline> {
  let pts: Vec<DVec3> = tris
    .iter()
    .flat_map(|t| m.indices.get(t * 3..t * 3 + 3).unwrap_or(&[]).to_vec())
    .map(|i| {
      let p = m.positions[i as usize];
      DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)
    })
    .collect();
  if pts.is_empty() {
    return Vec::new();
  }
  let span = |n: DVec3| {
    let (lo, hi) =
      pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(n.dot(*p)), hi.max(n.dot(*p))));
    (1..=count).map(move |i| lo + (hi - lo) * i as f64 / (count + 1) as f64)
  };
  let any = |_: DVec3| true;
  let mut out = Vec::new();
  let axis = face.normal.normalize_or_zero();
  let around =
    matches!(face.kind, SurfaceKind::Cylinder | SurfaceKind::Cone | SurfaceKind::Torus | SurfaceKind::Sphere)
      && axis.length() > 0.5;
  if around {
    let centre = if face.kind == SurfaceKind::Sphere { face.center } else { face.origin };
    let x = {
      let x = face.xdir - axis * face.xdir.dot(axis);
      if x.length() > 1e-9 { x.normalize() } else { axis.any_orthonormal_vector() }
    };
    let y = axis.cross(x);
    if families[0] {
      // along: half-planes through the axis
      for i in 0..count {
        let a = std::f64::consts::TAU * i as f64 / count as f64;
        let radial = x * a.cos() + y * a.sin();
        let n = axis.cross(radial);
        let keep = move |p: DVec3| (p - centre).dot(radial) > 0.0;
        out.extend(slice(m, tris, n, n.dot(centre), lift, &keep));
      }
    }
    if families[1] {
      // around: planes across the axis
      for d in span(axis) {
        out.extend(slice(m, tris, axis, d, lift, &any));
      }
    }
  } else {
    // across a plane (or any other surface): two directions in it
    let normal = if face.kind == SurfaceKind::Plane { axis } else { average_normal(m, tris) };
    let u = {
      let x = face.xdir - normal * face.xdir.dot(normal);
      if x.length() > 1e-9 { x.normalize() } else { normal.any_orthonormal_vector() }
    };
    let v = normal.cross(u);
    for (on, dir) in [(families[0], u), (families[1], v)] {
      if on && dir.length() > 0.5 {
        for d in span(dir) {
          out.extend(slice(m, tris, dir, d, lift, &any));
        }
      }
    }
  }
  out
}

fn average_normal(m: &TriMesh, tris: &[usize]) -> DVec3 {
  let mut n = DVec3::ZERO;
  for &t in tris {
    if let Some(tri) = m.indices.get(t * 3..t * 3 + 3) {
      let p = |i: u32| {
        let q = m.positions[i as usize];
        DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64)
      };
      n += (p(tri[1]) - p(tri[0])).cross(p(tri[2]) - p(tri[0]));
    }
  }
  if n.length() > 1e-12 { n.normalize() } else { DVec3::Z }
}

/// Adds the combs and isocurves `v` asks for to the scene. `edges` gives a body's edge lines.
pub(crate) fn draw(scene: &mut Scene, s: &Session, v: &AnalysisView, edges: &mut dyn FnMut(BodyId) -> Vec<EdgeLine>) {
  for (body, edge) in &v.combs {
    let Some(b) = s.model.body(*body) else { continue };
    if !s.body_visible(*body) {
      continue;
    }
    let size = b.diagonal() * 0.01;
    if let Some(e) = edges(*body).iter().find(|e| e.id == *edge) {
      scene.lines.extend(comb(e, v.comb_density, v.comb_scale, size));
    }
  }
  for (body, face) in &v.iso {
    let Some(b) = s.model.body(*body) else { continue };
    let Some(m) = b.mesh() else { continue };
    if !s.body_visible(*body) {
      continue;
    }
    let lift = b.diagonal() * 5e-4;
    for info in b.faces().iter().filter(|f| face.is_none_or(|x| x == f.id)) {
      let tris: Vec<usize> = (0..m.tri_face.len()).filter(|t| m.tri_face[*t] == info.id).collect();
      scene.lines.extend(isocurves(m, &tris, info, v.iso_families, v.iso_count, lift));
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_comb_on_a_circle_has_outward_teeth_of_one_length() {
    let pts: Vec<DVec3> = (0..=64)
      .map(|i| {
        let a = std::f64::consts::TAU * i as f64 / 64.0;
        DVec3::new(10.0 * a.cos(), 10.0 * a.sin(), 0.0)
      })
      .collect();
    let lines = comb(&EdgeLine { id: 1, points: pts }, 20, 10.0, 1.0);
    // 20 teeth and the outline
    assert_eq!(lines.len(), 21);
    for l in &lines[..20] {
      let (a, b) = (DVec3::from(l.points[0].map(f64::from)), DVec3::from(l.points[1].map(f64::from)));
      assert!(b.length() > a.length(), "teeth point away from the centre");
      // curvature 1/10, scale 10, size 1: a tooth 1 long
      assert!(((b - a).length() - 1.0).abs() < 0.05, "{}", (b - a).length());
    }
    // a straight edge has no teeth, only its outline
    let straight = EdgeLine { id: 2, points: vec![DVec3::ZERO, DVec3::new(5.0, 0.0, 0.0)] };
    assert_eq!(comb(&straight, 10, 10.0, 1.0).len(), 1);
  }

  #[test]
  fn curvature_types_tell_a_cylinder_from_a_sphere() {
    // a vertex where the turn is 0 one way and 0.1 the other (a cylinder of radius 10)
    let k = [[0.0, 0.1, 0.05], [0.1, 0.1, 0.1]];
    let base = [128, 128, 128, 255];
    let c = |kind| colors(&k, Coloring::Curvature { kind, scale: 1.0, banded: false }, base, 0.0);
    let gauss = c(CurvatureType::Gaussian);
    assert_eq!(gauss[0], [40, 80, 220, 255], "no Gaussian curvature on a cylinder: blue");
    assert_ne!(gauss[1], gauss[0]);
    assert_eq!(c(CurvatureType::Max)[0], c(CurvatureType::Max)[1]);
    // opacity 0 is the body's own colour
    assert!(
      colors(&k, Coloring::Curvature { kind: CurvatureType::Max, scale: 1.0, banded: false }, base, 1.0)
        .iter()
        .all(|c| *c == base)
    );
  }

  #[test]
  fn minimum_radius_colours_only_what_a_tool_cannot_reach() {
    // radii 2, 10 and 50 (flat counts as endless)
    let k = [[0.0, 0.5, 0.0], [0.0, 0.1, 0.0], [0.0, 0.02, 0.0], [0.0; 3]];
    let base = [128, 128, 128, 255];
    let c = colors(&k, Coloring::MinRadius { min: 5.0, max: 25.0 }, base, 0.0);
    assert_eq!(c[0], [225, 50, 40, 255], "tighter than the minimum: red");
    assert!(c[1] != base && c[1] != c[0]);
    assert_eq!(c[2], base);
    assert_eq!(c[3], base);
  }

  #[test]
  fn settings_parse_with_fusions_defaults() {
    let v = parse("comb", &serde_json::json!({"edges": [{"body": 1, "edge": 4}]}));
    assert_eq!((v.combs, v.comb_density, v.comb_scale), (vec![(BodyId(1), 4)], 50, 10.0));
    let v = parse("isocurve", &serde_json::json!({"faces": [{"body": 2, "face": 3}], "type": "u", "count": 7}));
    assert_eq!((v.iso, v.iso_families, v.iso_count), (vec![(BodyId(2), Some(3))], [true, false], 7));
    let v = parse("zebra", &serde_json::json!({"bodies": [5], "opacity": 25}));
    assert!(v.covers(BodyId(5)) && !v.covers(BodyId(6)));
    assert!((v.fade - 0.75).abs() < 1e-6);
    assert!(parse("zebra", &serde_json::json!({})).covers(BodyId(9)));
  }
}
