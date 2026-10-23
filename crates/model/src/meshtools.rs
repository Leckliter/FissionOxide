//! Fusion's mesh tools on welded triangle meshes (see [`crate::meshops::weld`]): Reduce by
//! quadric edge collapse, a volume-preserving Smooth, and Plane Cut with the opening filled.

use std::collections::{BinaryHeap, HashMap, HashSet};

use fox_kernel::glam::{DVec2, DVec3};

/// Where a reduction stops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
  /// As many triangles go as can without the surface moving further than this (mm).
  Tolerance(f64),
  /// Down to this many triangles, or as near as the shape allows.
  Count(usize),
}

/// A sum of squared distances to planes (the upper triangle of a symmetric 4×4 matrix).
#[derive(Debug, Clone, Copy, Default)]
struct Quadric([f64; 10]);

impl Quadric {
  fn plane(n: DVec3, d: f64, w: f64) -> Quadric {
    let (a, b, c) = (n.x, n.y, n.z);
    Quadric([a * a, a * b, a * c, a * d, b * b, b * c, b * d, c * c, c * d, d * d].map(|x| x * w))
  }

  fn add(&self, o: &Quadric) -> Quadric {
    let mut r = *self;
    for (x, y) in r.0.iter_mut().zip(o.0) {
      *x += y;
    }
    r
  }

  fn error(&self, v: DVec3) -> f64 {
    let q = &self.0;
    let (x, y, z) = (v.x, v.y, v.z);
    q[0] * x * x
      + 2.0 * q[1] * x * y
      + 2.0 * q[2] * x * z
      + 2.0 * q[3] * x
      + q[4] * y * y
      + 2.0 * q[5] * y * z
      + 2.0 * q[6] * y
      + q[7] * z * z
      + 2.0 * q[8] * z
      + q[9]
  }
}

/// A candidate collapse of edge `a`–`b` into `at`, valid while both ends are at these versions.
#[derive(Debug, Clone, Copy)]
struct Collapse {
  cost: f64,
  err: f64,
  a: u32,
  b: u32,
  va: u32,
  vb: u32,
  at: DVec3,
}

impl PartialEq for Collapse {
  fn eq(&self, o: &Self) -> bool {
    self.cost.total_cmp(&o.cost).is_eq()
  }
}
impl Eq for Collapse {}
impl PartialOrd for Collapse {
  fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(o))
  }
}
impl Ord for Collapse {
  // the cheapest first out of the max-heap
  fn cmp(&self, o: &Self) -> std::cmp::Ordering {
    o.cost.total_cmp(&self.cost)
  }
}

fn normal(a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
  (b - a).cross(c - a)
}

/// Fewer triangles by collapsing edges, cheapest first: by how far the surface moves (Adaptive)
/// or by length (`uniform`, which evens out the triangles). No collapse moves the surface more
/// than `max_dev` (mm), folds a triangle over or pinches the mesh, and a closed mesh keeps at
/// least four triangles, so a mesh already at its minimum stays as it is. Open boundaries keep
/// their outline.
pub fn decimate(pos: &[DVec3], tris: &[u32], target: Target, uniform: bool, max_dev: f64) -> (Vec<DVec3>, Vec<u32>) {
  let mut p = pos.to_vec();
  let mut t: Vec<[u32; 3]> = tris.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
  let (stop, limit) = match target {
    Target::Tolerance(tol) => (0, tol.min(max_dev)),
    Target::Count(n) => (n, max_dev),
  };
  if t.len() <= stop.max(4) || p.is_empty() {
    return (pos.to_vec(), tris.to_vec());
  }
  let mut alive = vec![true; t.len()];
  let mut vt: Vec<Vec<usize>> = vec![Vec::new(); p.len()];
  let mut q = vec![Quadric::default(); p.len()];
  let mut edges: HashMap<(u32, u32), (usize, usize)> = HashMap::new();
  for (i, tri) in t.iter().enumerate() {
    let [a, b, c] = tri.map(|v| p[v as usize]);
    let n = normal(a, b, c);
    if let Some(n) = n.try_normalize() {
      let k = Quadric::plane(n, -n.dot(a), 1.0);
      for v in tri {
        q[*v as usize] = q[*v as usize].add(&k);
      }
    }
    for v in tri {
      vt[*v as usize].push(i);
    }
    for (u, w) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
      edges.entry((u.min(w), u.max(w))).or_insert((0, i)).0 += 1;
    }
  }
  // an open boundary's outline stays: a steep plane across each boundary edge
  for (&(u, w), &(n, i)) in &edges {
    if n != 1 {
      continue;
    }
    let [a, b, c] = t[i].map(|v| p[v as usize]);
    let (pu, pw) = (p[u as usize], p[w as usize]);
    if let Some(side) = (pw - pu).cross(normal(a, b, c)).try_normalize() {
      let k = Quadric::plane(side, -side.dot(pu), 1e3);
      q[u as usize] = q[u as usize].add(&k);
      q[w as usize] = q[w as usize].add(&k);
    }
  }
  let mut ver = vec![0u32; p.len()];
  let mut dead = vec![false; p.len()];
  let candidate = |p: &[DVec3], q: &[Quadric], ver: &[u32], a: u32, b: u32| {
    let k = q[a as usize].add(&q[b as usize]);
    let (pa, pb) = (p[a as usize], p[b as usize]);
    let (at, e) = [pa, pb, (pa + pb) / 2.0]
      .into_iter()
      .map(|v| (v, k.error(v)))
      .min_by(|x, y| x.1.total_cmp(&y.1))
      .unwrap_or((pa, 0.0));
    let err = e.max(0.0).sqrt();
    let cost = if uniform { (pa - pb).length() } else { err };
    Collapse { cost, err, a, b, va: ver[a as usize], vb: ver[b as usize], at }
  };
  let mut heap: BinaryHeap<Collapse> = edges.keys().map(|&(u, w)| candidate(&p, &q, &ver, u, w)).collect();
  let mut count = t.len();
  let neighbours = |t: &[[u32; 3]], alive: &[bool], vt: &[Vec<usize>], v: u32| -> HashSet<u32> {
    vt[v as usize].iter().filter(|&&i| alive[i]).flat_map(|&i| t[i]).filter(|&x| x != v).collect()
  };
  while count > stop
    && let Some(c) = heap.pop()
  {
    let (a, b) = (c.a, c.b);
    if dead[a as usize] || dead[b as usize] || ver[a as usize] != c.va || ver[b as usize] != c.vb || c.err > limit {
      continue;
    }
    let shared: Vec<usize> = vt[a as usize]
      .iter()
      .copied()
      .filter(|&i| alive[i] && t[i].contains(&b))
      .collect::<HashSet<_>>()
      .into_iter()
      .collect();
    if shared.is_empty() || count - shared.len() < 4 {
      continue;
    }
    // the link condition: the ends share no neighbour but the tips of the triangles on the edge
    let tips: HashSet<u32> = shared.iter().flat_map(|&i| t[i]).filter(|&x| x != a && x != b).collect();
    let na = neighbours(&t, &alive, &vt, a);
    let nb = neighbours(&t, &alive, &vt, b);
    if na.intersection(&nb).any(|x| !tips.contains(x)) {
      continue;
    }
    // no triangle round the edge folds over or flattens
    let folds = vt[a as usize].iter().chain(&vt[b as usize]).filter(|&&i| alive[i] && !shared.contains(&i)).any(|&i| {
      let before = t[i].map(|v| p[v as usize]);
      let after = t[i].map(|v| if v == a || v == b { c.at } else { p[v as usize] });
      let (n0, n1) = (normal(before[0], before[1], before[2]), normal(after[0], after[1], after[2]));
      match (n0.try_normalize(), n1.try_normalize()) {
        (Some(n0), Some(n1)) => n0.dot(n1) < 0.2,
        (_, None) => true,
        _ => false,
      }
    });
    if folds {
      continue;
    }
    for &i in &shared {
      alive[i] = false;
    }
    count -= shared.len();
    let moved: Vec<usize> = vt[b as usize].iter().copied().filter(|&i| alive[i]).collect();
    for i in moved {
      for v in t[i].iter_mut() {
        if *v == b {
          *v = a;
        }
      }
      vt[a as usize].push(i);
    }
    vt[a as usize].retain(|&i| alive[i]);
    vt[a as usize].dedup();
    p[a as usize] = c.at;
    q[a as usize] = q[a as usize].add(&q[b as usize]);
    dead[b as usize] = true;
    ver[a as usize] += 1;
    for n in neighbours(&t, &alive, &vt, a) {
      heap.push(candidate(&p, &q, &ver, a, n));
    }
  }
  let kept: Vec<[u32; 3]> = t.iter().zip(&alive).filter(|(_, a)| **a).map(|(t, _)| *t).collect();
  compact(&p, &kept.concat())
}

/// The mesh with only the vertices its triangles use.
pub fn compact(pos: &[DVec3], tris: &[u32]) -> (Vec<DVec3>, Vec<u32>) {
  let mut map: HashMap<u32, u32> = HashMap::new();
  let mut out = Vec::new();
  let tris = tris
    .iter()
    .map(|&v| {
      *map.entry(v).or_insert_with(|| {
        out.push(pos[v as usize]);
        (out.len() - 1) as u32
      })
    })
    .collect();
  (out, tris)
}

/// Is every edge shared by exactly two triangles?
pub fn is_closed(tris: &[u32]) -> bool {
  let mut n: HashMap<(u32, u32), u32> = HashMap::new();
  for t in tris.chunks_exact(3) {
    for (u, w) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      *n.entry((u.min(w), u.max(w))).or_default() += 1;
    }
  }
  !n.is_empty() && n.values().all(|&k| k == 2)
}

/// The volume a closed mesh encloses (positive when its triangles face out).
pub fn volume(pos: &[DVec3], tris: &[u32]) -> f64 {
  tris.chunks_exact(3).map(|t| pos[t[0] as usize].dot(pos[t[1] as usize].cross(pos[t[2] as usize]))).sum::<f64>() / 6.0
}

fn neighbour_lists(n: usize, tris: &[u32]) -> Vec<Vec<u32>> {
  let mut nb: Vec<Vec<u32>> = vec![Vec::new(); n];
  for t in tris.chunks_exact(3) {
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      if !nb[a as usize].contains(&b) {
        nb[a as usize].push(b);
      }
      if !nb[b as usize].contains(&a) {
        nb[b as usize].push(a);
      }
    }
  }
  nb
}

/// Splits every triangle in four at its edges' middles.
pub fn subdivide(pos: &[DVec3], tris: &[u32]) -> (Vec<DVec3>, Vec<u32>) {
  let mut p = pos.to_vec();
  let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
  let mut at = |p: &mut Vec<DVec3>, u: u32, w: u32| {
    *mid.entry((u.min(w), u.max(w))).or_insert_with(|| {
      p.push((p[u as usize] + p[w as usize]) / 2.0);
      (p.len() - 1) as u32
    })
  };
  let mut out = Vec::with_capacity(tris.len() * 4);
  for t in tris.chunks_exact(3) {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, bc, ca) = (at(&mut p, a, b), at(&mut p, b, c), at(&mut p, c, a));
    out.extend([a, ab, ca, ab, b, bc, ca, bc, c, ab, bc, ca]);
  }
  (p, out)
}

/// The most triangles Smooth subdivides a coarse mesh into: each becomes a planar face of the
/// body, and building a body of several thousand took seconds (the app froze on Smooth).
pub const SMOOTH_SUBDIVIDE_MAX: usize = 1000;

/// Fusion's Smooth: `smoothness` (0–1) of Laplacian smoothing, each pass followed (on a closed
/// mesh) by scaling about the centre back to the volume it had, so the mesh rounds off without
/// shrinking. A coarse mesh is subdivided first (a box's eight corners have nothing to round
/// between them), up to [`SMOOTH_SUBDIVIDE_MAX`] triangles. An open mesh's boundary stays.
pub fn smooth_volume(pos: &[DVec3], tris: &[u32], smoothness: f64) -> (Vec<DVec3>, Vec<u32>) {
  let passes = (smoothness.clamp(0.0, 1.0) * 10.0).round() as usize;
  if passes == 0 || pos.is_empty() {
    return (pos.to_vec(), tris.to_vec());
  }
  let (lo, hi) = pos.iter().fold((pos[0], pos[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
  let fine = (hi - lo).length() / 16.0;
  let longest = |p: &[DVec3], t: &[u32]| {
    t.chunks_exact(3)
      .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
      .map(|(u, w)| (p[u as usize] - p[w as usize]).length())
      .fold(0.0, f64::max)
  };
  let (mut p, mut tris) = (pos.to_vec(), tris.to_vec());
  while tris.len() / 3 * 4 <= SMOOTH_SUBDIVIDE_MAX && longest(&p, &tris) > fine {
    (p, tris) = subdivide(&p, &tris);
  }
  let nb = neighbour_lists(p.len(), &tris);
  let mut edge_uses: HashMap<(u32, u32), u32> = HashMap::new();
  for t in tris.chunks_exact(3) {
    for (u, w) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      *edge_uses.entry((u.min(w), u.max(w))).or_default() += 1;
    }
  }
  let mut fixed = vec![false; p.len()];
  for (&(u, w), &k) in &edge_uses {
    if k == 1 {
      fixed[u as usize] = true;
      fixed[w as usize] = true;
    }
  }
  let closed = edge_uses.values().all(|&k| k == 2);
  let v0 = volume(&p, &tris);
  for _ in 0..passes {
    let prev = p.clone();
    for (i, n) in nb.iter().enumerate() {
      if n.is_empty() || fixed[i] {
        continue;
      }
      let avg = n.iter().map(|&j| prev[j as usize]).sum::<DVec3>() / n.len() as f64;
      p[i] = prev[i] * 0.5 + avg * 0.5;
    }
    let v1 = volume(&p, &tris);
    if closed && v0.abs() > 1e-12 && v1.abs() > 1e-12 && v0.signum() == v1.signum() {
      let k = (v0 / v1).cbrt();
      let c = p.iter().sum::<DVec3>() / p.len() as f64;
      for v in &mut p {
        *v = c + (*v - c) * k;
      }
    }
  }
  (p, tris)
}

/// A mesh cut by the plane through `origin` with unit `normal`: the shared positions (the cut
/// adds points where it crosses edges), the triangles behind the plane and those in front.
pub fn plane_cut(pos: &[DVec3], tris: &[u32], origin: DVec3, normal: DVec3) -> (Vec<DVec3>, Vec<u32>, Vec<u32>) {
  let scale = pos.iter().fold(0.0f64, |m, p| m.max(p.abs().max_element())).max(1.0);
  let eps = 1e-9 * scale;
  let mut out = pos.to_vec();
  let mut d: Vec<f64> = pos.iter().map(|p| (*p - origin).dot(normal)).collect();
  for (x, p) in d.iter_mut().zip(out.iter_mut()) {
    if x.abs() < eps {
      // a vertex on the plane is on it exactly, so both sides share it
      *p -= normal * *x;
      *x = 0.0;
    }
  }
  let side = |x: f64| {
    if x > 0.0 {
      1
    } else if x < 0.0 {
      -1
    } else {
      0
    }
  };
  let mut cross: HashMap<(u32, u32), u32> = HashMap::new();
  let (mut below, mut above) = (Vec::new(), Vec::new());
  for t in tris.chunks_exact(3) {
    let s = [0, 1, 2].map(|k| side(d[t[k] as usize]));
    if s.iter().all(|&x| x <= 0) {
      below.extend_from_slice(t);
      continue;
    }
    if s.iter().all(|&x| x >= 0) {
      above.extend_from_slice(t);
      continue;
    }
    let (mut lo, mut hi) = (Vec::new(), Vec::new());
    for k in 0..3 {
      let (i, j) = (t[k], t[(k + 1) % 3]);
      let (si, sj) = (s[k], s[(k + 1) % 3]);
      if si <= 0 {
        lo.push(i);
      }
      if si >= 0 {
        hi.push(i);
      }
      if si * sj < 0 {
        let m = *cross.entry((i.min(j), i.max(j))).or_insert_with(|| {
          let (di, dj) = (d[i as usize], d[j as usize]);
          let f = di / (di - dj);
          out.push(out[i as usize] + (out[j as usize] - out[i as usize]) * f);
          d.push(0.0);
          (out.len() - 1) as u32
        });
        lo.push(m);
        hi.push(m);
      }
    }
    for (poly, into) in [(lo, &mut below), (hi, &mut above)] {
      for k in 1..poly.len().saturating_sub(1) {
        into.extend([poly[0], poly[k], poly[k + 1]]);
      }
    }
  }
  (out, below, above)
}

/// Triangles closing the openings a cut left in `tris` (the side of the mesh behind the plane
/// with unit `normal` when `behind`, else in front), facing out of it; the points `Uniform` fill
/// adds go on the end of `pos`. Openings are the loops of boundary edges lying in the plane;
/// one inside another is a hole in it.
pub fn fill_cut(
  pos: &mut Vec<DVec3>,
  tris: &[u32],
  origin: DVec3,
  normal: DVec3,
  behind: bool,
  uniform: bool,
) -> Vec<u32> {
  let scale = pos.iter().fold(0.0f64, |m, p| m.max(p.abs().max_element())).max(1.0);
  let on = |v: u32| ((pos[v as usize] - origin).dot(normal)).abs() < 1e-7 * scale;
  let mut half: HashSet<(u32, u32)> = HashSet::new();
  for t in tris.chunks_exact(3) {
    for (u, w) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      half.insert((u, w));
    }
  }
  // the cap runs against the open boundary
  let mut next: HashMap<u32, u32> = HashMap::new();
  for &(u, w) in &half {
    if !half.contains(&(w, u)) && on(u) && on(w) {
      next.insert(w, u);
    }
  }
  let mut loops: Vec<Vec<u32>> = Vec::new();
  let mut seen: HashSet<u32> = HashSet::new();
  let mut starts: Vec<u32> = next.keys().copied().collect();
  starts.sort_unstable();
  for s in starts {
    if seen.contains(&s) {
      continue;
    }
    let mut l = vec![s];
    seen.insert(s);
    let mut v = s;
    while let Some(&w) = next.get(&v) {
      if w == s || !seen.insert(w) {
        break;
      }
      l.push(w);
      v = w;
    }
    if l.len() >= 3 && next.get(&v) == Some(&s) {
      loops.push(l);
    }
  }
  // 2D on the plane, turning about the cap's outward normal
  let out_n = if behind { normal } else { -normal };
  let e1 = out_n.any_orthonormal_vector();
  let e2 = out_n.cross(e1);
  let flat = |v: u32| {
    let p = pos[v as usize] - origin;
    DVec2::new(p.dot(e1), p.dot(e2))
  };
  let area = |l: &[u32]| {
    let pts: Vec<DVec2> = l.iter().map(|&v| flat(v)).collect();
    (0..pts.len()).map(|i| pts[i].perp_dot(pts[(i + 1) % pts.len()])).sum::<f64>() / 2.0
  };
  let (outer, holes): (Vec<Vec<u32>>, Vec<Vec<u32>>) = loops.into_iter().partition(|l| area(l.as_slice()) > 0.0);
  let mut polys: Vec<Vec<u32>> = outer;
  for h in holes {
    let probe = flat(h[0]);
    let host = polys
      .iter()
      .enumerate()
      .filter(|(_, o)| inside(&o.iter().map(|&v| flat(v)).collect::<Vec<_>>(), probe))
      .min_by(|x, y| area(x.1.as_slice()).total_cmp(&area(y.1.as_slice())))
      .map(|(i, _)| i);
    if let Some(i) = host {
      polys[i] = bridge(&polys[i], &h, &flat);
    }
  }
  let mut cap = Vec::new();
  for poly in &polys {
    let pts: Vec<DVec2> = poly.iter().map(|&v| flat(v)).collect();
    for [a, b, c] in ear_clip(&pts) {
      cap.extend([poly[a], poly[b], poly[c]]);
    }
  }
  if uniform && !cap.is_empty() {
    let rim: HashSet<(u32, u32)> = polys
      .iter()
      .flat_map(|p| (0..p.len()).map(move |i| (p[i].min(p[(i + 1) % p.len()]), p[i].max(p[(i + 1) % p.len()]))))
      .collect();
    let (sum, n) =
      rim.iter().fold((0.0, 0usize), |(s, n), (u, w)| (s + (pos[*u as usize] - pos[*w as usize]).length(), n + 1));
    refine(pos, &mut cap, &rim, sum / n.max(1) as f64);
  }
  cap
}

/// Splits the longest inner edge of `cap` (its rim's edges stay whole, so the cap still meets
/// the mesh) at its middle until none is much longer than `len`.
fn refine(pos: &mut Vec<DVec3>, cap: &mut Vec<u32>, rim: &HashSet<(u32, u32)>, len: f64) {
  if len <= 0.0 {
    return;
  }
  for _ in 0..4000 {
    let key = |u: u32, w: u32| (u.min(w), u.max(w));
    let longest = cap
      .chunks_exact(3)
      .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
      .filter(|&(u, w)| !rim.contains(&key(u, w)))
      .map(|(u, w)| (key(u, w), (pos[u as usize] - pos[w as usize]).length()))
      .max_by(|x, y| x.1.total_cmp(&y.1));
    let Some(((u, w), l)) = longest else { return };
    if l < 1.5 * len {
      return;
    }
    pos.push((pos[u as usize] + pos[w as usize]) / 2.0);
    let m = (pos.len() - 1) as u32;
    let mut out = Vec::with_capacity(cap.len() + 6);
    for t in cap.chunks_exact(3) {
      let k = (0..3).find(|&k| key(t[k], t[(k + 1) % 3]) == (u, w));
      match k {
        Some(k) => {
          let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
          out.extend([a, m, c, m, b, c]);
        }
        None => out.extend_from_slice(t),
      }
    }
    *cap = out;
  }
}

/// Is `p` inside the polygon `poly` (even-odd)?
fn inside(poly: &[DVec2], p: DVec2) -> bool {
  let mut c = false;
  for i in 0..poly.len() {
    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
    if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
      c = !c;
    }
  }
  c
}

fn segments_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
  let o = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
  let (d1, d2, d3, d4) = (o(a, b, c), o(a, b, d), o(c, d, a), o(c, d, b));
  d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// One polygon of `outer` and the hole `hole` inside it, joined by a cut from the hole's
/// rightmost point to the nearest outer point it sees.
fn bridge(outer: &[u32], hole: &[u32], flat: &dyn Fn(u32) -> DVec2) -> Vec<u32> {
  let k = (0..hole.len()).max_by(|&x, &y| flat(hole[x]).x.total_cmp(&flat(hole[y]).x)).unwrap_or(0);
  let h = flat(hole[k]);
  let edges: Vec<(DVec2, DVec2)> = [outer, hole]
    .iter()
    .flat_map(|l| (0..l.len()).map(move |i| (l[i], l[(i + 1) % l.len()])))
    .map(|(u, w)| (flat(u), flat(w)))
    .collect();
  let i = (0..outer.len())
    .filter(|&i| {
      let o = flat(outer[i]);
      !edges.iter().any(|&(a, b)| segments_cross(h, o, a, b))
    })
    .min_by(|&x, &y| (flat(outer[x]) - h).length().total_cmp(&(flat(outer[y]) - h).length()))
    .unwrap_or(0);
  let mut out: Vec<u32> = outer[..=i].to_vec();
  out.extend(hole[k..].iter().chain(&hole[..=k]));
  out.extend_from_slice(&outer[i..]);
  out
}

/// Triangulates a simple polygon (either way round) by ear clipping; the triangles index `pts`
/// and run the way the polygon does.
pub fn ear_clip(pts: &[DVec2]) -> Vec<[usize; 3]> {
  let n = pts.len();
  if n < 3 {
    return Vec::new();
  }
  let ccw = (0..n).map(|i| pts[i].perp_dot(pts[(i + 1) % n])).sum::<f64>() >= 0.0;
  let turn = |a: DVec2, b: DVec2, c: DVec2| {
    let x = (b - a).perp_dot(c - b);
    if ccw { x } else { -x }
  };
  let mut idx: Vec<usize> = (0..n).collect();
  let mut out = Vec::new();
  while idx.len() > 3 {
    let m = idx.len();
    let ear = (0..m).find(|&i| {
      let (a, b, c) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
      let (pa, pb, pc) = (pts[a], pts[b], pts[c]);
      if turn(pa, pb, pc) <= 0.0 {
        return false;
      }
      // no other corner inside (corners at the same place, as a bridge's ends, don't count)
      !idx.iter().any(|&j| {
        let pj = pts[j];
        j != a
          && j != b
          && j != c
          && pj != pa
          && pj != pb
          && pj != pc
          && turn(pa, pb, pj) >= 0.0
          && turn(pb, pc, pj) >= 0.0
          && turn(pc, pa, pj) >= 0.0
      })
    });
    // a degenerate polygon: clip the flattest corner rather than stop
    let i = ear.unwrap_or_else(|| {
      (0..m)
        .min_by(|&x, &y| {
          let t = |i: usize| turn(pts[idx[(i + m - 1) % m]], pts[idx[i]], pts[idx[(i + 1) % m]]).abs();
          t(x).total_cmp(&t(y))
        })
        .unwrap_or(0)
    });
    let (a, b, c) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
    if ear.is_some() {
      out.push([a, b, c]);
    }
    idx.remove(i);
  }
  out.push([idx[0], idx[1], idx[2]]);
  out
}

/// Plane Cut's plane: the plane through `origin` with in-plane directions `x` and `y` moved
/// `distance` along its normal, then turned `y_angle` about `x` and `z_angle` about `y`
/// (radians). Returns its origin and unit normal, which points at the side Trim removes (the
/// other side with `flip`).
pub fn cut_plane(
  origin: DVec3,
  x: DVec3,
  y: DVec3,
  distance: f64,
  y_angle: f64,
  z_angle: f64,
  flip: bool,
) -> (DVec3, DVec3) {
  use fox_kernel::glam::DQuat;
  let unit = |v: DVec3, or: DVec3| v.try_normalize().unwrap_or(or);
  let n = unit(x.cross(y), DVec3::Z);
  let turn = DQuat::from_axis_angle(unit(y, DVec3::Y), z_angle) * DQuat::from_axis_angle(unit(x, DVec3::X), y_angle);
  let n2 = turn * n;
  (origin + n * distance, if flip { -n2 } else { n2 })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A closed box, 8 corners and 12 triangles facing out.
  pub(crate) fn cube(s: f64) -> (Vec<DVec3>, Vec<u32>) {
    let pos =
      (0..8).map(|i| DVec3::new((i & 1) as f64 * s, ((i >> 1) & 1) as f64 * s, ((i >> 2) & 1) as f64 * s)).collect();
    let tris = vec![
      0, 2, 1, 1, 2, 3, // bottom
      4, 5, 6, 5, 7, 6, // top
      0, 1, 4, 1, 5, 4, // front
      2, 6, 3, 3, 6, 7, // back
      0, 4, 2, 2, 4, 6, // left
      1, 3, 5, 3, 7, 5, // right
    ];
    (pos, tris)
  }

  /// A UV sphere of radius `r`, closed.
  fn ball(r: f64, n: usize) -> (Vec<DVec3>, Vec<u32>) {
    let mut pos = vec![DVec3::new(0.0, 0.0, r), DVec3::new(0.0, 0.0, -r)];
    for i in 1..n {
      let th = std::f64::consts::PI * i as f64 / n as f64;
      for j in 0..2 * n {
        let ph = std::f64::consts::PI * j as f64 / n as f64;
        pos.push(DVec3::new(th.sin() * ph.cos(), th.sin() * ph.sin(), th.cos()) * r);
      }
    }
    let ring = |i: usize, j: usize| (2 + (i - 1) * 2 * n + j % (2 * n)) as u32;
    let mut tris = Vec::new();
    for j in 0..2 * n {
      tris.extend([0, ring(1, j), ring(1, j + 1)]);
      tris.extend([1, ring(n - 1, j + 1), ring(n - 1, j)]);
      for i in 1..n - 1 {
        tris.extend([ring(i, j), ring(i + 1, j), ring(i + 1, j + 1), ring(i, j), ring(i + 1, j + 1), ring(i, j + 1)]);
      }
    }
    (pos, tris)
  }

  #[test]
  fn reduce_leaves_a_mesh_at_its_minimum_and_halves_a_ball() {
    let (pos, tris) = cube(10.0);
    let (p, t) = decimate(&pos, &tris, Target::Count(3), false, 0.01 * 17.3);
    assert_eq!(t.len(), tris.len(), "a box can't lose a triangle without changing shape");
    assert!((volume(&p, &t) - 1000.0).abs() < 1e-9);
    let (pos, tris) = ball(10.0, 16);
    assert!(is_closed(&tris) && volume(&pos, &tris) > 0.0);
    let n = tris.len() / 3;
    let (p, t) = decimate(&pos, &tris, Target::Count(n / 2), false, 1.0);
    assert!(t.len() / 3 <= n / 2 + 2 && t.len() / 3 > n / 4, "{} of {n}", t.len() / 3);
    assert!(is_closed(&t), "the ball stays closed");
    let v = 4.0 / 3.0 * std::f64::consts::PI * 1000.0;
    assert!((volume(&p, &t) - v).abs() < 0.1 * v, "{} vs {v}", volume(&p, &t));
    // within a tolerance: a tight one keeps more than a loose one
    let tight = decimate(&pos, &tris, Target::Tolerance(0.01), false, 1.0).1.len();
    let loose = decimate(&pos, &tris, Target::Tolerance(0.5), true, 1.0).1.len();
    assert!(loose < tight && tight <= tris.len(), "{loose} {tight}");
  }

  #[test]
  fn smoothing_keeps_the_volume() {
    let (pos, tris) = cube(10.0);
    let (p, t) = smooth_volume(&pos, &tris, 1.0);
    assert!(is_closed(&t));
    assert!((volume(&p, &t) - 1000.0).abs() < 1e-6, "{}", volume(&p, &t));
    // rounded: the corners come in, the faces' middles stay out
    let r: Vec<f64> = p.iter().map(|v| (*v - DVec3::splat(5.0)).length()).collect();
    let (near, far) = (r.iter().copied().fold(f64::MAX, f64::min), r.iter().copied().fold(0.0, f64::max));
    assert!(far < 75f64.sqrt() - 0.5 && near > 4.0, "{near} {far}");
    assert_eq!(smooth_volume(&pos, &tris, 0.0), (pos, tris));
    // a coarse mesh is subdivided only so far: a body of a few hundred faces builds at once
    assert!(t.len() / 3 <= SMOOTH_SUBDIVIDE_MAX && t.len() / 3 > 100, "{}", t.len() / 3);
  }

  #[test]
  fn a_plane_cut_closes_both_halves() {
    let (pos, tris) = cube(10.0);
    let (o, n) = (DVec3::new(0.0, 0.0, 4.0), DVec3::Z);
    let (mut p, below, above) = plane_cut(&pos, &tris, o, n);
    assert!(!is_closed(&below) && !is_closed(&above));
    for uniform in [false, true] {
      let mut q = p.clone();
      let cap = fill_cut(&mut q, &below, o, n, true, uniform);
      let all: Vec<u32> = below.iter().chain(&cap).copied().collect();
      assert!(is_closed(&all), "uniform {uniform}");
      assert!((volume(&q, &all) - 400.0).abs() < 1e-6, "{}", volume(&q, &all));
    }
    let cap = fill_cut(&mut p, &above, o, n, false, false);
    let all: Vec<u32> = above.iter().chain(&cap).copied().collect();
    assert!(is_closed(&all) && (volume(&p, &all) - 600.0).abs() < 1e-6);
  }

  #[test]
  fn a_cut_through_a_ring_fills_around_its_hole() {
    // a square tube along Z (outer 10, inner 4) with closed ends; cut across the middle
    let mut pos = Vec::new();
    for z in [0.0, 10.0] {
      for (x, y) in [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0), (3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)]
      {
        pos.push(DVec3::new(x, y, z));
      }
    }
    let mut tris: Vec<u32> = Vec::new();
    let quad = |t: &mut Vec<u32>, a: u32, b: u32, c: u32, d: u32| t.extend([a, b, c, a, c, d]);
    for i in 0..4u32 {
      let (j, k) = ((i + 1) % 4, i);
      // outer wall facing out, inner wall facing in
      quad(&mut tris, k, j, j + 8, k + 8);
      quad(&mut tris, j + 4, k + 4, k + 12, j + 12);
      // the ends: bottom facing down, top facing up
      quad(&mut tris, j, k, k + 4, j + 4);
      quad(&mut tris, k + 8, j + 8, j + 12, k + 12);
    }
    assert!(is_closed(&tris));
    let v0 = volume(&pos, &tris);
    assert!((v0 - 840.0).abs() < 1e-9, "{v0}");
    let (o, n) = (DVec3::new(0.0, 0.0, 5.0), DVec3::Z);
    let (mut p, below, _) = plane_cut(&pos, &tris, o, n);
    let cap = fill_cut(&mut p, &below, o, n, true, false);
    let all: Vec<u32> = below.iter().chain(&cap).copied().collect();
    assert!(is_closed(&all));
    assert!((volume(&p, &all) - 420.0).abs() < 1e-6, "{}", volume(&p, &all));
  }
}
