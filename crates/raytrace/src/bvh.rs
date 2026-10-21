//! A bounding volume hierarchy over triangles (binned SAH), and ray queries against it.

use glam::Vec3;

#[derive(Debug, Clone, Copy)]
struct Aabb {
  min: Vec3,
  max: Vec3,
}

impl Aabb {
  const EMPTY: Aabb = Aabb { min: Vec3::splat(f32::INFINITY), max: Vec3::splat(f32::NEG_INFINITY) };

  fn grow(&mut self, p: Vec3) {
    self.min = self.min.min(p);
    self.max = self.max.max(p);
  }

  fn union(&mut self, b: &Aabb) {
    self.min = self.min.min(b.min);
    self.max = self.max.max(b.max);
  }

  fn area(&self) -> f32 {
    let d = (self.max - self.min).max(Vec3::ZERO);
    d.x * d.y + d.y * d.z + d.z * d.x
  }

  /// Entry distance of a ray, if it hits within (0, t_max).
  fn hit(&self, o: Vec3, inv: Vec3, t_max: f32) -> Option<f32> {
    let t0 = (self.min - o) * inv;
    let t1 = (self.max - o) * inv;
    let near = t0.min(t1).max_element().max(0.0);
    let far = t0.max(t1).min_element().min(t_max);
    (near <= far).then_some(near)
  }
}

#[derive(Debug, Clone, Copy)]
struct Node {
  bounds: Aabb,
  /// Leaf: first triangle and count; inner: left child index (right = left + 1), count 0.
  start: u32,
  count: u32,
}

/// A ray hit: distance, triangle and barycentrics.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
  pub t: f32,
  pub tri: u32,
  pub u: f32,
  pub v: f32,
}

pub struct Bvh {
  nodes: Vec<Node>,
  /// Triangle order of the leaves.
  pub order: Vec<u32>,
  /// Vertex positions of each triangle (in `order`).
  tris: Vec<[Vec3; 3]>,
}

const LEAF: usize = 4;
const BINS: usize = 12;

impl Bvh {
  pub fn new(tris: &[[Vec3; 3]]) -> Bvh {
    let mut order: Vec<u32> = (0..tris.len() as u32).collect();
    let centroids: Vec<Vec3> = tris.iter().map(|t| (t[0] + t[1] + t[2]) / 3.0).collect();
    let boxes: Vec<Aabb> = tris
      .iter()
      .map(|t| {
        let mut b = Aabb::EMPTY;
        t.iter().for_each(|p| b.grow(*p));
        b
      })
      .collect();
    let mut nodes = vec![Node { bounds: Aabb::EMPTY, start: 0, count: tris.len() as u32 }];
    let mut stack = vec![0usize];
    while let Some(ni) = stack.pop() {
      let (start, count) = (nodes[ni].start as usize, nodes[ni].count as usize);
      let mut bounds = Aabb::EMPTY;
      let mut cb = Aabb::EMPTY;
      for &t in &order[start..start + count] {
        bounds.union(&boxes[t as usize]);
        cb.grow(centroids[t as usize]);
      }
      nodes[ni].bounds = bounds;
      if count <= LEAF {
        continue;
      }
      // best binned SAH split over the three axes
      let mut best: Option<(f32, usize, f32)> = None;
      let extent = cb.max - cb.min;
      for axis in 0..3 {
        if extent[axis] <= 1e-12 {
          continue;
        }
        let mut bins = [(Aabb::EMPTY, 0usize); BINS];
        let k = BINS as f32 / extent[axis];
        for &t in &order[start..start + count] {
          let b = (((centroids[t as usize][axis] - cb.min[axis]) * k) as usize).min(BINS - 1);
          bins[b].0.union(&boxes[t as usize]);
          bins[b].1 += 1;
        }
        let mut right_area = [0.0; BINS];
        let mut right_count = [0usize; BINS];
        let (mut acc, mut n) = (Aabb::EMPTY, 0);
        for i in (1..BINS).rev() {
          acc.union(&bins[i].0);
          n += bins[i].1;
          right_area[i] = acc.area();
          right_count[i] = n;
        }
        let (mut acc, mut n) = (Aabb::EMPTY, 0);
        for i in 0..BINS - 1 {
          acc.union(&bins[i].0);
          n += bins[i].1;
          if n == 0 || right_count[i + 1] == 0 {
            continue;
          }
          let cost = acc.area() * n as f32 + right_area[i + 1] * right_count[i + 1] as f32;
          if best.is_none_or(|b| cost < b.0) {
            best = Some((cost, axis, cb.min[axis] + (i + 1) as f32 / k));
          }
        }
      }
      let Some((cost, axis, split)) = best else { continue };
      if cost >= bounds.area() * count as f32 && count <= 16 {
        continue;
      }
      let slice = &mut order[start..start + count];
      let mut l = 0;
      for i in 0..slice.len() {
        if centroids[slice[i] as usize][axis] < split {
          slice.swap(i, l);
          l += 1;
        }
      }
      if l == 0 || l == count {
        continue;
      }
      let left = nodes.len();
      nodes.push(Node { bounds: Aabb::EMPTY, start: start as u32, count: l as u32 });
      nodes.push(Node { bounds: Aabb::EMPTY, start: (start + l) as u32, count: (count - l) as u32 });
      nodes[ni].start = left as u32;
      nodes[ni].count = 0;
      stack.push(left);
      stack.push(left + 1);
    }
    let tris = order.iter().map(|&t| tris[t as usize]).collect();
    Bvh { nodes, order, tris }
  }

  /// The nearest hit along a ray before `t_max` (`tri` is the index in the input).
  pub fn intersect(&self, o: Vec3, d: Vec3, t_max: f32) -> Option<Hit> {
    if self.tris.is_empty() {
      return None;
    }
    let inv = d.recip();
    let mut best: Option<Hit> = None;
    let mut t_best = t_max;
    let mut stack = [0u32; 128];
    let mut sp = 1;
    while sp > 0 {
      sp -= 1;
      let node = &self.nodes[stack[sp] as usize];
      if node.bounds.hit(o, inv, t_best).is_none() {
        continue;
      }
      if node.count > 0 {
        for i in node.start..node.start + node.count {
          if let Some((t, u, v)) = tri_hit(&self.tris[i as usize], o, d, t_best) {
            t_best = t;
            best = Some(Hit { t, tri: self.order[i as usize], u, v });
          }
        }
      } else {
        // nearer child last, so it's popped first
        let (a, b) = (node.start, node.start + 1);
        let ta = self.nodes[a as usize].bounds.hit(o, inv, t_best);
        let tb = self.nodes[b as usize].bounds.hit(o, inv, t_best);
        let (first, second) = match (ta, tb) {
          (Some(x), Some(y)) if y < x => (Some(b), Some(a)),
          (Some(_), Some(_)) => (Some(a), Some(b)),
          (Some(_), None) => (Some(a), None),
          (None, Some(_)) => (Some(b), None),
          (None, None) => (None, None),
        };
        if sp + 2 > stack.len() {
          continue;
        }
        if let Some(s) = second {
          stack[sp] = s;
          sp += 1;
        }
        if let Some(f) = first {
          stack[sp] = f;
          sp += 1;
        }
      }
    }
    best
  }

  /// Whether anything blocks the ray before `t_max`.
  pub fn occluded(&self, o: Vec3, d: Vec3, t_max: f32) -> bool {
    self.intersect(o, d, t_max).is_some()
  }
}

/// Möller–Trumbore.
fn tri_hit(t: &[Vec3; 3], o: Vec3, d: Vec3, t_max: f32) -> Option<(f32, f32, f32)> {
  let e1 = t[1] - t[0];
  let e2 = t[2] - t[0];
  let p = d.cross(e2);
  let det = e1.dot(p);
  if det.abs() < 1e-12 {
    return None;
  }
  let inv = 1.0 / det;
  let s = o - t[0];
  let u = s.dot(p) * inv;
  if !(0.0..=1.0).contains(&u) {
    return None;
  }
  let q = s.cross(e1);
  let v = d.dot(q) * inv;
  if v < 0.0 || u + v > 1.0 {
    return None;
  }
  let dist = e2.dot(q) * inv;
  (dist > 0.0 && dist < t_max).then_some((dist, u, v))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_the_nearest_of_many_triangles() {
    let mut tris = Vec::new();
    for i in 0..200 {
      let z = i as f32;
      tris.push([Vec3::new(-1.0, -1.0, z), Vec3::new(1.0, -1.0, z), Vec3::new(0.0, 1.0, z)]);
    }
    let bvh = Bvh::new(&tris);
    let h = bvh.intersect(Vec3::new(0.0, 0.0, 50.5), Vec3::Z, f32::INFINITY).unwrap();
    assert_eq!(h.tri, 51);
    assert!((h.t - 0.5).abs() < 1e-5);
    let h = bvh.intersect(Vec3::new(0.0, 0.0, 50.5), -Vec3::Z, f32::INFINITY).unwrap();
    assert_eq!(h.tri, 50);
    assert!(bvh.intersect(Vec3::new(5.0, 0.0, 0.0), Vec3::Z, f32::INFINITY).is_none());
  }
}
