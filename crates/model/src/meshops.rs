//! Mesh tools for faceted bodies: welding, reduction (vertex clustering) and smoothing.

use std::collections::HashMap;

use fox_kernel::TriMesh;
use fox_kernel::glam::DVec3;

/// Shared vertices (a tessellation repeats them per face) and triangles indexing them.
pub fn weld(m: &TriMesh) -> (Vec<DVec3>, Vec<u32>) {
  let mut index: HashMap<[i64; 3], u32> = HashMap::new();
  let mut pos = Vec::new();
  let q = |v: f32| (v as f64 * 1e5).round() as i64;
  let mut map = Vec::with_capacity(m.positions.len());
  for p in &m.positions {
    let key = [q(p[0]), q(p[1]), q(p[2])];
    let i = *index.entry(key).or_insert_with(|| {
      pos.push(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
      (pos.len() - 1) as u32
    });
    map.push(i);
  }
  let tris = m.indices.iter().map(|&i| map.get(i as usize).copied().unwrap_or(0)).collect();
  (pos, drop_degenerate(tris))
}

fn drop_degenerate(tris: Vec<u32>) -> Vec<u32> {
  tris.chunks_exact(3).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).flatten().copied().collect()
}

/// Keeps about `keep` (0–1) of the triangles by clustering vertices on a grid.
pub fn reduce(pos: &[DVec3], tris: &[u32], keep: f64) -> (Vec<DVec3>, Vec<u32>) {
  if keep >= 0.999 || pos.is_empty() {
    return (pos.to_vec(), tris.to_vec());
  }
  let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
  for p in pos {
    lo = lo.min(*p);
    hi = hi.max(*p);
  }
  let size = (hi - lo).max_element().max(1e-9);
  // vertices scale roughly with the square of the grid resolution on a surface
  let target = ((pos.len() as f64) * keep).max(8.0);
  let mut cells = (target.sqrt() * 2.0).max(2.0);
  let mut out = (pos.to_vec(), tris.to_vec());
  for _ in 0..12 {
    let cell = size / cells;
    let mut ids: HashMap<[i64; 3], usize> = HashMap::new();
    let mut sums: Vec<(DVec3, f64)> = Vec::new();
    let map: Vec<usize> = pos
      .iter()
      .map(|p| {
        let k = ((*p - lo) / cell).floor();
        let key = [k.x as i64, k.y as i64, k.z as i64];
        let i = *ids.entry(key).or_insert_with(|| {
          sums.push((DVec3::ZERO, 0.0));
          sums.len() - 1
        });
        sums[i].0 += *p;
        sums[i].1 += 1.0;
        i
      })
      .collect();
    let new_pos: Vec<DVec3> = sums.iter().map(|(s, n)| *s / *n).collect();
    let new_tris = drop_degenerate(tris.iter().map(|&i| map[i as usize] as u32).collect());
    out = (new_pos, new_tris);
    if (out.1.len() as f64) <= tris.len() as f64 * keep * 1.15 {
      break;
    }
    cells *= 0.8;
  }
  out
}

/// Laplacian smoothing (each pass moves vertices halfway to their neighbours' average).
pub fn smooth(pos: &[DVec3], tris: &[u32], passes: usize) -> Vec<DVec3> {
  let mut nb: Vec<Vec<u32>> = vec![Vec::new(); pos.len()];
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
  let mut p = pos.to_vec();
  for _ in 0..passes {
    let prev = p.clone();
    for (i, n) in nb.iter().enumerate() {
      if n.is_empty() {
        continue;
      }
      let avg = n.iter().map(|&j| prev[j as usize]).sum::<DVec3>() / n.len() as f64;
      p[i] = prev[i] * 0.5 + avg * 0.5;
    }
  }
  p
}

#[cfg(test)]
mod tests {
  use super::*;

  fn grid(n: usize) -> (Vec<DVec3>, Vec<u32>) {
    let mut pos = Vec::new();
    for j in 0..=n {
      for i in 0..=n {
        pos.push(DVec3::new(i as f64, j as f64, 0.0));
      }
    }
    let mut tris = Vec::new();
    let w = (n + 1) as u32;
    for j in 0..n as u32 {
      for i in 0..n as u32 {
        let a = j * w + i;
        tris.extend([a, a + 1, a + w + 1, a, a + w + 1, a + w]);
      }
    }
    (pos, tris)
  }

  #[test]
  fn reduce_keeps_about_the_fraction() {
    let (pos, tris) = grid(40);
    let (_, t) = reduce(&pos, &tris, 0.25);
    assert!(t.len() < tris.len() / 2, "{} of {}", t.len() / 3, tris.len() / 3);
    assert!(!t.is_empty());
  }

  #[test]
  fn smoothing_keeps_a_flat_grid_flat() {
    let (pos, tris) = grid(5);
    let p = smooth(&pos, &tris, 3);
    assert!(p.iter().all(|v| v.z.abs() < 1e-12));
  }
}
