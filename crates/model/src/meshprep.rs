//! Mesh preparation (MESH > Tessellate, Repair and the face group commands): turning a solid or
//! surface into triangles to a refinement, finding and fixing what is wrong with a mesh, and
//! dividing a mesh into face groups.

use std::collections::HashMap;

use fox_kernel::glam::DVec3;
use serde::{Deserialize, Serialize};

/// Repair's Repair Type, as Fusion lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairType {
  /// Fill the open boundaries.
  CloseHoles,
  /// Join coincident vertices, drop degenerate and duplicate triangles and small loose pieces.
  #[default]
  StitchAndRemove,
  /// Stitch, close the holes and turn every triangle the same way out.
  Wrap,
  /// Everything at once.
  OneTouchFix,
  /// Everything, then rebuilt from scratch.
  Rebuild,
}

impl RepairType {
  pub const ALL: [RepairType; 5] = [
    RepairType::CloseHoles,
    RepairType::StitchAndRemove,
    RepairType::Wrap,
    RepairType::OneTouchFix,
    RepairType::Rebuild,
  ];

  /// The command's name for it.
  pub fn key(self) -> &'static str {
    match self {
      RepairType::CloseHoles => "close_holes",
      RepairType::StitchAndRemove => "stitch_and_remove",
      RepairType::Wrap => "wrap",
      RepairType::OneTouchFix => "one_touch_fix",
      RepairType::Rebuild => "rebuild",
    }
  }

  pub fn parse(s: &str) -> Option<RepairType> {
    RepairType::ALL.into_iter().find(|t| t.key() == s)
  }
}

/// Tessellate's Refinement Settings, in millimetres and degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Refinement {
  /// Largest distance between a triangle and the surface it stands for.
  pub surface_deviation: f64,
  /// Largest angle between neighbouring triangles' normals.
  pub normal_deviation: f64,
  /// Longest triangle edge.
  pub max_edge: f64,
  /// Longest edge over shortest height a triangle may have.
  pub aspect_ratio: f64,
}

/// Refinement High, Medium or Low for a body whose bounding box has diagonal `diagonal` (mm).
/// Medium is Fusion's default: a normal deviation of 15 degrees, an aspect ratio of 21.5 and a
/// maximum edge of half the diagonal.
pub fn refinement(level: &str, diagonal: f64) -> Refinement {
  let d = diagonal.max(1e-6);
  let (dev, normal, edge, aspect) = match level {
    "high" => (0.0005, 10.0, 0.25, 10.0),
    "low" => (0.005, 30.0, 1.0, 40.0),
    _ => (0.001, 15.0, 0.5, 21.5),
  };
  Refinement { surface_deviation: d * dev, normal_deviation: normal, max_edge: d * edge, aspect_ratio: aspect }
}

/// Splits triangle edges longer than `max_len` at their middles until none is (or the mesh
/// reaches `limit` triangles). Neighbouring triangles split their shared edge alike.
pub fn split_long_edges(pos: &[DVec3], tris: &[u32], max_len: f64, limit: usize) -> (Vec<DVec3>, Vec<u32>) {
  let mut pos = pos.to_vec();
  let mut tris = tris.to_vec();
  if !(max_len > 0.0) {
    return (pos, tris);
  }
  for _ in 0..16 {
    let long = |a: u32, b: u32, pos: &[DVec3]| (pos[a as usize] - pos[b as usize]).length() > max_len;
    if !tris.chunks_exact(3).any(|t| long(t[0], t[1], &pos) || long(t[1], t[2], &pos) || long(t[2], t[0], &pos)) {
      break;
    }
    if tris.len() / 3 * 4 > limit {
      break;
    }
    let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
    let mut split = |a: u32, b: u32, pos: &mut Vec<DVec3>| -> Option<u32> {
      if !long(a, b, pos) {
        return None;
      }
      let key = (a.min(b), a.max(b));
      Some(*mid.entry(key).or_insert_with(|| {
        pos.push((pos[a as usize] + pos[b as usize]) * 0.5);
        (pos.len() - 1) as u32
      }))
    };
    let mut out = Vec::with_capacity(tris.len() * 2);
    for t in tris.chunks_exact(3) {
      let (a, b, c) = (t[0], t[1], t[2]);
      let (ab, bc, ca) = (split(a, b, &mut pos), split(b, c, &mut pos), split(c, a, &mut pos));
      match (ab, bc, ca) {
        (None, None, None) => out.extend([a, b, c]),
        (Some(m), None, None) => out.extend([a, m, c, m, b, c]),
        (None, Some(m), None) => out.extend([a, b, m, a, m, c]),
        (None, None, Some(m)) => out.extend([a, b, m, m, b, c]),
        (Some(p), Some(q), None) => out.extend([a, p, c, p, q, c, p, b, q]),
        (None, Some(q), Some(r)) => out.extend([a, b, r, b, q, r, r, q, c]),
        (Some(p), None, Some(r)) => out.extend([a, p, r, p, b, c, p, c, r]),
        (Some(p), Some(q), Some(r)) => out.extend([a, p, r, p, b, q, r, q, c, p, q, r]),
      }
    }
    tris = out;
  }
  (pos, tris)
}

/// Quick Analysis: what is wrong with a mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Analysis {
  pub triangles: usize,
  /// Edges with one triangle: the borders of holes.
  pub open_edges: usize,
  /// Open boundaries (each a loop of open edges).
  pub holes: usize,
  /// Edges shared by three or more triangles.
  pub non_manifold_edges: usize,
  /// Triangles with no area.
  pub degenerate: usize,
  /// Triangles turned the other way from a neighbour.
  pub flipped: usize,
  /// Loose pieces.
  pub shells: usize,
}

impl Analysis {
  /// Nothing to repair: Fusion's green check.
  pub fn clean(&self) -> bool {
    self.triangles > 0
      && self.open_edges == 0
      && self.non_manifold_edges == 0
      && self.degenerate == 0
      && self.flipped == 0
  }
}

/// Directed edges of the triangles, counted both ways round.
fn edge_uses(tris: &[u32]) -> HashMap<(u32, u32), (usize, usize)> {
  let mut m: HashMap<(u32, u32), (usize, usize)> = HashMap::new();
  for t in tris.chunks_exact(3) {
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      let e = m.entry((a.min(b), a.max(b))).or_default();
      if a < b {
        e.0 += 1;
      } else {
        e.1 += 1;
      }
    }
  }
  m
}

fn area2(pos: &[DVec3], t: &[u32]) -> f64 {
  let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
  (b - a).cross(c - a).length()
}

/// Analyses a welded mesh (see [`crate::meshops::weld`]).
pub fn analyze(pos: &[DVec3], tris: &[u32]) -> Analysis {
  let size = bounds_size(pos);
  let tiny = (size * 1e-9).powi(2);
  let uses = edge_uses(tris);
  let mut a = Analysis { triangles: tris.len() / 3, ..Analysis::default() };
  for &(f, r) in uses.values() {
    match f + r {
      1 => a.open_edges += 1,
      2 if f == 2 || r == 2 => a.flipped += 1,
      2 => {}
      _ => a.non_manifold_edges += 1,
    }
  }
  a.degenerate = tris.chunks_exact(3).filter(|t| area2(pos, t) <= tiny).count();
  a.holes = boundary_loops(tris).len();
  a.shells = shells(tris).iter().copied().max().map_or(0, |m| m as usize + 1);
  a
}

fn bounds_size(pos: &[DVec3]) -> f64 {
  let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
  for p in pos {
    lo = lo.min(*p);
    hi = hi.max(*p);
  }
  if pos.is_empty() { 1.0 } else { (hi - lo).length().max(1e-9) }
}

/// The open boundaries, each as its vertices in order.
fn boundary_loops(tris: &[u32]) -> Vec<Vec<u32>> {
  let uses = edge_uses(tris);
  // an open edge runs the way its one triangle doesn't, so the fill faces the same way
  let mut next: HashMap<u32, u32> = HashMap::new();
  for t in tris.chunks_exact(3) {
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      if uses.get(&(a.min(b), a.max(b))).is_some_and(|(f, r)| f + r == 1) {
        next.insert(b, a);
      }
    }
  }
  let mut loops = Vec::new();
  let mut starts: Vec<u32> = next.keys().copied().collect();
  starts.sort_unstable();
  for s in starts {
    if !next.contains_key(&s) {
      continue;
    }
    let mut l = vec![s];
    let mut v = s;
    while let Some(n) = next.remove(&v) {
      if n == s {
        break;
      }
      l.push(n);
      v = n;
    }
    if l.len() >= 3 {
      loops.push(l);
    }
  }
  loops
}

/// Which loose piece each triangle belongs to.
fn shells(tris: &[u32]) -> Vec<u32> {
  let n = tris.len() / 3;
  let adj = adjacency(tris);
  let mut piece = vec![u32::MAX; n];
  let mut k = 0;
  for s in 0..n {
    if piece[s] != u32::MAX {
      continue;
    }
    let mut stack = vec![s];
    piece[s] = k;
    while let Some(t) = stack.pop() {
      for &u in &adj[t] {
        if piece[u] == u32::MAX {
          piece[u] = k;
          stack.push(u);
        }
      }
    }
    k += 1;
  }
  piece
}

/// Triangles sharing an edge with each triangle.
fn adjacency(tris: &[u32]) -> Vec<Vec<usize>> {
  let n = tris.len() / 3;
  let mut by_edge: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
  for (i, t) in tris.chunks_exact(3).enumerate() {
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
      by_edge.entry((a.min(b), a.max(b))).or_default().push(i);
    }
  }
  let mut adj = vec![Vec::new(); n];
  for ts in by_edge.values() {
    for &a in ts {
      for &b in ts {
        if a != b && !adj[a].contains(&b) {
          adj[a].push(b);
        }
      }
    }
  }
  adj
}

/// Drops triangles with no area, repeats of a triangle, and loose pieces with under a tenth of
/// the largest piece's triangles.
fn stitch_and_remove(pos: &[DVec3], tris: &[u32]) -> Vec<u32> {
  let tiny = (bounds_size(pos) * 1e-9).powi(2);
  let mut seen = std::collections::HashSet::new();
  let kept: Vec<u32> = tris
    .chunks_exact(3)
    .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2] && area2(pos, t) > tiny)
    .filter(|t| {
      let mut k = [t[0], t[1], t[2]];
      k.sort_unstable();
      seen.insert(k)
    })
    .flatten()
    .copied()
    .collect();
  let piece = shells(&kept);
  let mut count: HashMap<u32, usize> = HashMap::new();
  for p in &piece {
    *count.entry(*p).or_default() += 1;
  }
  let largest = count.values().copied().max().unwrap_or(0);
  kept.chunks_exact(3).zip(&piece).filter(|(_, p)| count[p] * 10 >= largest).flat_map(|(t, _)| t).copied().collect()
}

/// Fills each open boundary with a fan of triangles around its centre.
fn close_holes(pos: &mut Vec<DVec3>, tris: &[u32]) -> Vec<u32> {
  let mut out = tris.to_vec();
  for l in boundary_loops(tris) {
    if l.len() == 3 {
      out.extend([l[0], l[1], l[2]]);
      continue;
    }
    let c = l.iter().map(|&v| pos[v as usize]).sum::<DVec3>() / l.len() as f64;
    pos.push(c);
    let ci = (pos.len() - 1) as u32;
    for k in 0..l.len() {
      out.extend([l[k], l[(k + 1) % l.len()], ci]);
    }
  }
  out
}

/// Turns every triangle of a piece the way its first triangle faces, then each closed piece
/// outward (positive volume).
fn orient(pos: &[DVec3], tris: &[u32]) -> Vec<u32> {
  let mut t: Vec<[u32; 3]> = tris.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
  let adj = adjacency(tris);
  let mut done = vec![false; t.len()];
  let has = |x: &[u32; 3], a: u32, b: u32| (0..3).any(|k| x[k] == a && x[(k + 1) % 3] == b);
  for s in 0..t.len() {
    if done[s] {
      continue;
    }
    done[s] = true;
    let mut piece = vec![s];
    let mut stack = vec![s];
    while let Some(i) = stack.pop() {
      for &j in &adj[i] {
        if done[j] {
          continue;
        }
        let ti = t[i];
        // a shared edge runs opposite ways in two triangles that face the same way
        let same = (0..3).any(|k| has(&t[j], ti[k], ti[(k + 1) % 3]));
        if same {
          t[j].swap(1, 2);
        }
        done[j] = true;
        piece.push(j);
        stack.push(j);
      }
    }
    let vol: f64 = piece
      .iter()
      .map(|&i| {
        let [a, b, c] = t[i].map(|v| pos[v as usize]);
        a.dot(b.cross(c))
      })
      .sum();
    if vol < 0.0 {
      for &i in &piece {
        t[i].swap(1, 2);
      }
    }
  }
  t.into_iter().flatten().collect()
}

/// Repairs a welded mesh.
pub fn repair(pos: &[DVec3], tris: &[u32], ty: RepairType) -> (Vec<DVec3>, Vec<u32>) {
  let mut pos = pos.to_vec();
  let tris = match ty {
    RepairType::StitchAndRemove => stitch_and_remove(&pos, tris),
    RepairType::CloseHoles => close_holes(&mut pos, tris),
    RepairType::Wrap | RepairType::OneTouchFix | RepairType::Rebuild => {
      let t = stitch_and_remove(&pos, tris);
      let t = close_holes(&mut pos, &t);
      orient(&pos, &t)
    }
  };
  (pos, tris)
}

fn normal(pos: &[DVec3], t: &[u32]) -> DVec3 {
  let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
  (b - a).cross(c - a).normalize_or_zero()
}

/// Generate Face Groups: grows groups of triangles across edges while a triangle's normal stays
/// within `angle` (degrees) of its group's first triangle, then merges groups smaller than
/// `min_size` of an average group's area into the neighbour they share most edges with. The
/// result is a group number per triangle, from 0.
pub fn face_groups(pos: &[DVec3], tris: &[u32], angle: f64, min_size: f64) -> Vec<u32> {
  let n = tris.len() / 3;
  let adj = adjacency(tris);
  let normals: Vec<DVec3> = tris.chunks_exact(3).map(|t| normal(pos, t)).collect();
  let areas: Vec<f64> = tris.chunks_exact(3).map(|t| area2(pos, t) * 0.5).collect();
  let cos = angle.to_radians().cos();
  let mut group = vec![u32::MAX; n];
  let mut k = 0u32;
  for s in 0..n {
    if group[s] != u32::MAX {
      continue;
    }
    group[s] = k;
    let mut stack = vec![s];
    while let Some(t) = stack.pop() {
      for &u in &adj[t] {
        if group[u] == u32::MAX && normals[u].dot(normals[s]) >= cos {
          group[u] = k;
          stack.push(u);
        }
      }
    }
    k += 1;
  }
  if k <= 1 || min_size <= 0.0 {
    return group;
  }
  let mut area = vec![0.0; k as usize];
  for (t, g) in group.iter().enumerate() {
    area[*g as usize] += areas[t];
  }
  let mean = area.iter().sum::<f64>() / k as f64;
  // smallest first, each into its best-connected neighbour
  let mut order: Vec<u32> = (0..k).collect();
  order.sort_by(|a, b| area[*a as usize].total_cmp(&area[*b as usize]));
  for g in order {
    if area[g as usize] >= min_size * mean || area[g as usize] == 0.0 {
      continue;
    }
    let mut shared: HashMap<u32, usize> = HashMap::new();
    for t in (0..n).filter(|t| group[*t] == g) {
      for &u in &adj[t] {
        if group[u] != g {
          *shared.entry(group[u]).or_default() += 1;
        }
      }
    }
    let Some((&into, _)) = shared.iter().max_by_key(|(h, c)| (**c, std::cmp::Reverse(**h))) else { continue };
    for x in group.iter_mut().filter(|x| **x == g) {
      *x = into;
    }
    area[into as usize] += area[g as usize];
    area[g as usize] = 0.0;
  }
  renumber(&mut group);
  group
}

/// Numbers groups 0, 1, 2… in order of first appearance.
pub fn renumber(group: &mut [u32]) {
  let mut map: HashMap<u32, u32> = HashMap::new();
  for g in group.iter_mut() {
    let next = map.len() as u32;
    *g = *map.entry(*g).or_insert(next);
  }
}

/// Combine Face Groups: the groups in `picked` become one.
pub fn combine_groups(group: &mut [u32], picked: &[u32]) {
  let Some(&into) = picked.iter().min() else { return };
  for g in group.iter_mut() {
    if picked.contains(g) {
      *g = into;
    }
  }
  renumber(group);
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A unit cube's eight corners and twelve outward triangles.
  fn cube() -> (Vec<DVec3>, Vec<u32>) {
    let pos = (0..8).map(|i| DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64)).collect();
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let tris = quads.iter().flat_map(|q| [q[0], q[1], q[2], q[0], q[2], q[3]]).collect();
    (pos, tris)
  }

  #[test]
  fn a_closed_cube_is_clean_and_a_holed_one_is_closed_again() {
    let (pos, tris) = cube();
    let a = analyze(&pos, &tris);
    assert!(a.clean(), "{a:?}");
    assert_eq!((a.triangles, a.shells), (12, 1));
    // drop the bottom's two triangles: a square hole
    let holed: Vec<u32> = tris[6..].to_vec();
    let h = analyze(&pos, &holed);
    assert!(!h.clean());
    assert_eq!((h.open_edges, h.holes), (4, 1));
    let (p, t) = repair(&pos, &holed, RepairType::CloseHoles);
    assert!(analyze(&p, &t).clean(), "{:?}", analyze(&p, &t));
  }

  #[test]
  fn stitch_and_remove_drops_repeats_slivers_and_crumbs() {
    let (mut pos, mut tris) = cube();
    tris.extend_from_within(..3);
    pos.extend([DVec3::new(5.0, 5.0, 5.0), DVec3::new(6.0, 5.0, 5.0), DVec3::new(5.0, 6.0, 5.0)]);
    tris.extend([8, 9, 10]);
    let (p, t) = repair(&pos, &tris, RepairType::StitchAndRemove);
    assert_eq!(t.len() / 3, 12);
    assert!(analyze(&p, &t).clean());
  }

  #[test]
  fn one_touch_fix_turns_flipped_triangles_back() {
    let (pos, mut tris) = cube();
    tris.swap(1, 2);
    assert!(analyze(&pos, &tris).flipped > 0);
    let (p, t) = repair(&pos, &tris, RepairType::OneTouchFix);
    let a = analyze(&p, &t);
    assert!(a.clean(), "{a:?}");
  }

  #[test]
  fn a_cube_has_six_face_groups_and_combining_two_leaves_five() {
    let (pos, tris) = cube();
    let mut g = face_groups(&pos, &tris, 25.0, 0.2);
    assert_eq!(g.iter().max(), Some(&5));
    assert_eq!(g[0], g[1], "each side's two triangles are one group");
    let picked = [g[0], g[2]];
    combine_groups(&mut g, &picked);
    assert_eq!(g.iter().max(), Some(&4));
  }

  #[test]
  fn long_edges_split_until_short_enough() {
    let (pos, tris) = cube();
    let (p, t) = split_long_edges(&pos, &tris, 0.5, 100_000);
    assert!(
      t.chunks_exact(3).all(|t| (0..3).all(|k| (p[t[k] as usize] - p[t[(k + 1) % 3] as usize]).length() <= 0.5 + 1e-9))
    );
    let a = analyze(&p, &t);
    assert!(a.clean(), "the split mesh stays closed: {a:?}");
  }
}
