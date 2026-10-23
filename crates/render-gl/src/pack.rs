//! Scene data packed into the vertex and instance layouts the shaders read.
//!
//! Everything is a flat `f32` array; colours and ids travel as their bit patterns
//! (`f32::from_bits`) and the attribute pointers read them back as bytes and integers.

use std::collections::HashMap;
use std::ops::Range;

use fox_render::{FoldEdge, Marker, Mesh, Polyline};

/// Floats per mesh vertex: position, normal, colour bits, id bits.
pub const MESH_STRIDE: usize = 8;
/// Floats per fold edge: both ends, both triangle normals, both pick ids' bits.
pub const FOLD_STRIDE: usize = 14;
/// Triangles per mesh chunk, the unit of culling and level of detail.
pub const CHUNK_TRIS: usize = 4096;
/// Instances (line segments, markers) per chunk.
pub const CHUNK_INSTANCES: usize = 1024;
/// Fold edges per chunk: small, so that their normals stay close together and most chunks can
/// be skipped for holding no silhouette (see [`Cone`]).
pub const CHUNK_FOLDS: usize = 256;
/// Levels of detail below the full mesh, as the cell sizes of their vertex clustering relative
/// to the chunk's mean edge length (at most half its diagonal).
pub const LOD_CELLS: [f32; 2] = [2.5, 8.0];

/// A box around part of the scene and the elements in it (triangles, as index offsets, or
/// instances).
#[derive(Debug, Clone, PartialEq)]
pub struct Bounds {
  pub lo: [f32; 3],
  pub hi: [f32; 3],
  pub range: Range<usize>,
}

/// The directions a chunk's fold-edge normals lie within: `axis`, give or take `half` radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cone {
  pub axis: [f32; 3],
  pub half: f32,
}

impl Cone {
  /// The cone around unit normals (a full sphere when they spread too far to say).
  pub fn around(normals: impl Iterator<Item = [f32; 3]> + Clone) -> Cone {
    let sum: glam::Vec3 = normals.clone().map(glam::Vec3::from).sum();
    let axis = sum.normalize_or_zero();
    let mut cos = 1.0f32;
    for n in normals {
      cos = cos.min(axis.dot(glam::Vec3::from(n)));
    }
    let half = if axis == glam::Vec3::ZERO { std::f32::consts::PI } else { cos.clamp(-1.0, 1.0).acos() };
    Cone { axis: axis.to_array(), half }
  }
}

/// A run of a mesh's triangles close together, with coarser versions of it.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshChunk {
  /// The full triangles (index offsets into [`MeshData::indices`]).
  pub full: Bounds,
  /// Each level of detail's triangles (index offsets) and its cell size in world units; a level
  /// that wouldn't save enough repeats the one before it.
  pub lods: [(Range<usize>, f32); 2],
}
/// Floats per line segment: start, end, colour bits, id bits, width (px), depth bias, dashed
/// (0 solid, 1 dashed, 2 dash-dot),
/// and how far along its polyline it starts (world units, for the dash pattern).
pub const LINE_STRIDE: usize = 12;
/// Floats per marker: position, colour bits, id bits, radius (px).
pub const MARKER_STRIDE: usize = 7;

/// A mesh as GPU vertices: each vertex carries its triangle's pick id. The indices hold the
/// full triangles in chunk order, then every chunk's levels of detail.
pub struct MeshData {
  pub verts: Vec<f32>,
  pub indices: Vec<u32>,
  pub chunks: Vec<MeshChunk>,
}

fn color_bits(c: [u8; 4]) -> f32 {
  f32::from_bits(u32::from_le_bytes(c))
}

/// Packs a mesh. The id is per vertex (a flat varying), so a vertex shared by triangles with
/// different ids is duplicated; tessellations are per face, so that is rare. Triangles with an
/// out-of-range index are dropped, as the CPU rasterizer drops them.
pub fn mesh(m: &Mesh) -> MeshData {
  let n = m.positions.len();
  let mut verts = Vec::with_capacity(n * MESH_STRIDE);
  for i in 0..n {
    let p = m.positions[i];
    let nr = m.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
    let c = m.colors.get(i).copied().unwrap_or([255; 4]);
    verts.extend_from_slice(&[p[0], p[1], p[2], nr[0], nr[1], nr[2], color_bits(c), 0.0]);
  }
  let mut owner: Vec<Option<u32>> = vec![None; n];
  let mut dup: HashMap<(u32, u32), u32> = HashMap::new();
  let mut indices = Vec::with_capacity(m.indices.len());
  for (t, tri) in m.indices.chunks_exact(3).enumerate() {
    if tri.iter().any(|&i| i as usize >= n) {
      continue;
    }
    let id = m.tri_ids.get(t).copied().unwrap_or(m.id);
    for &v in tri {
      let slot = match owner[v as usize] {
        None => {
          owner[v as usize] = Some(id);
          verts[v as usize * MESH_STRIDE + 7] = f32::from_bits(id);
          v
        }
        Some(o) if o == id => v,
        Some(_) => *dup.entry((v, id)).or_insert_with(|| {
          let at = v as usize * MESH_STRIDE;
          let mut copy = [0.0f32; MESH_STRIDE];
          copy.copy_from_slice(&verts[at..at + MESH_STRIDE]);
          copy[7] = f32::from_bits(id);
          verts.extend_from_slice(&copy);
          (verts.len() / MESH_STRIDE - 1) as u32
        }),
      };
      indices.push(slot);
    }
  }
  let (indices, chunks) = chunked(&verts, indices);
  MeshData { verts, indices, chunks }
}

/// Bits of a Morton code per axis: 1024 cells a side is plenty to cut chunks of thousands of
/// triangles, and leaves room for the index in one `u64`.
const MORTON_BITS: u32 = 10;

fn spread(v: u32) -> u64 {
  let mut x = v as u64 & 0x1f_ffff;
  x = (x | x << 32) & 0x1f_0000_0000_ffff;
  x = (x | x << 16) & 0x1f_0000_ff00_00ff;
  x = (x | x << 8) & 0x100f_00f0_0f00_f00f;
  x = (x | x << 4) & 0x10c3_0c30_c30c_30c3;
  (x | x << 2) & 0x1249_2492_4924_9249
}

/// The order that walks the points along a Z-order curve through their box, so that runs of it
/// stay close together.
pub fn morton_order(points: &[[f32; 3]]) -> Vec<u32> {
  let (lo, hi) = bounds(points.iter().copied());
  let scale = |k: usize| {
    let ext = hi[k] - lo[k];
    if ext > 0.0 { ((1u32 << MORTON_BITS) - 1) as f32 / ext } else { 0.0 }
  };
  let s = [scale(0), scale(1), scale(2)];
  let code = |p: &[f32; 3]| {
    let q = |k: usize| ((p[k] - lo[k]) * s[k]).clamp(0.0, ((1u32 << MORTON_BITS) - 1) as f32) as u32;
    spread(q(0)) | spread(q(1)) << 1 | spread(q(2)) << 2
  };
  let mut order: Vec<u64> = points.iter().enumerate().map(|(i, p)| code(p) << 32 | i as u64).collect();
  order.sort_unstable();
  order.into_iter().map(|k| k as u32).collect()
}

/// The box around points (finite ones; all zero when there are none).
fn bounds(points: impl Iterator<Item = [f32; 3]>) -> ([f32; 3], [f32; 3]) {
  let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
  for p in points.filter(|p| p.iter().all(|c| c.is_finite())) {
    for k in 0..3 {
      lo[k] = lo[k].min(p[k]);
      hi[k] = hi[k].max(p[k]);
    }
  }
  if lo[0] > hi[0] { ([0.0; 3], [0.0; 3]) } else { (lo, hi) }
}

fn position(verts: &[f32], v: u32) -> [f32; 3] {
  let at = v as usize * MESH_STRIDE;
  [verts[at], verts[at + 1], verts[at + 2]]
}

/// Reorders the triangles into spatial chunks and appends each chunk's levels of detail.
fn chunked(verts: &[f32], indices: Vec<u32>) -> (Vec<u32>, Vec<MeshChunk>) {
  let tris: Vec<[u32; 3]> = indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
  let centers: Vec<[f32; 3]> = tris
    .iter()
    .map(|t| {
      let p = t.map(|v| position(verts, v));
      [0, 1, 2].map(|k| (p[0][k] + p[1][k] + p[2][k]) / 3.0)
    })
    .collect();
  let order = morton_order(&centers);
  let mut out: Vec<u32> = Vec::with_capacity(indices.len() * 3 / 2);
  let mut chunks = Vec::with_capacity(order.len().div_ceil(CHUNK_TRIS));
  for run in order.chunks(CHUNK_TRIS) {
    let start = out.len();
    for &t in run {
      out.extend_from_slice(&tris[t as usize]);
    }
    let (lo, hi) = bounds(out[start..].iter().map(|&v| position(verts, v)));
    let full = Bounds { lo, hi, range: start..out.len() };
    chunks.push(MeshChunk { full, lods: Default::default() });
  }
  // the coarse levels of every chunk, on all cores; chunks are independent
  let coarse: Vec<[(Vec<u32>, f32); 2]> = {
    let out = &out;
    let lods = |c: &MeshChunk| {
      let full = &out[c.full.range.clone()];
      let diag = (0..3).map(|k| (c.full.hi[k] - c.full.lo[k]).powi(2)).sum::<f32>().sqrt();
      let edge = edge_len(verts, full);
      let mut prev: Option<Vec<u32>> = None;
      LOD_CELLS.map(|scale| {
        let cell = (edge * scale).min(diag / 2.0);
        let from = prev.as_deref().unwrap_or(full);
        let tris = if cell > 0.0 { cluster(verts, from, c.full.lo, cell) } else { Vec::new() };
        // a level must save a good part of the triangles to be worth switching to
        if cell > 0.0 && tris.len() * 10 < from.len() * 6 {
          prev = Some(tris.clone());
          (tris, cell)
        } else {
          (Vec::new(), 0.0)
        }
      })
    };
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(chunks.len().max(1));
    let per = chunks.len().div_ceil(threads.max(1)).max(1);
    std::thread::scope(|scope| {
      let jobs: Vec<_> =
        chunks.chunks(per).map(|part| scope.spawn(move || part.iter().map(lods).collect::<Vec<_>>())).collect();
      jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
    })
  };
  // each level after all the full triangles, chunk by chunk, so neighbours at one level draw together
  for level in 0..LOD_CELLS.len() {
    for (c, lods) in chunks.iter_mut().zip(&coarse) {
      let (tris, cell) = &lods[level];
      c.lods[level] = if *cell > 0.0 {
        let start = out.len();
        out.extend_from_slice(tris);
        (start..out.len(), *cell)
      } else if level == 0 {
        (c.full.range.clone(), 0.0)
      } else {
        c.lods[level - 1].clone()
      };
    }
  }
  (out, chunks)
}

/// The mean length of the triangles' edges.
fn edge_len(verts: &[f32], tris: &[u32]) -> f32 {
  let mut sum = 0.0;
  for t in tris.chunks_exact(3) {
    let p = [t[0], t[1], t[2]].map(|v| glam::Vec3::from(position(verts, v)));
    sum += p[0].distance(p[1]) + p[1].distance(p[2]) + p[2].distance(p[0]);
  }
  if tris.is_empty() { 0.0 } else { sum / tris.len() as f32 }
}

/// Vertex clustering: every vertex moves to the first vertex in its grid cell, and the triangles
/// that collapse are dropped. The coarse triangles reuse the mesh's vertices (and their ids).
fn cluster(verts: &[f32], tris: &[u32], origin: [f32; 3], cell: f32) -> Vec<u32> {
  let fast = std::hash::BuildHasherDefault::<FastHasher>::default();
  let mut rep: HashMap<[i32; 3], u32, _> = HashMap::with_capacity_and_hasher(tris.len(), fast.clone());
  let mut seen: std::collections::HashSet<[u32; 3], _> =
    std::collections::HashSet::with_capacity_and_hasher(tris.len() / 3, fast);
  let mut out = Vec::new();
  let mut snap = |v: u32| {
    let p = position(verts, v);
    let key = [0, 1, 2].map(|k| ((p[k] - origin[k]) / cell).floor() as i32);
    *rep.entry(key).or_insert(v)
  };
  for t in tris.chunks_exact(3) {
    let r = [snap(t[0]), snap(t[1]), snap(t[2])];
    if r[0] == r[1] || r[1] == r[2] || r[0] == r[2] {
      continue;
    }
    let mut key = r;
    key.sort_unstable();
    if seen.insert(key) {
      out.extend_from_slice(&r);
    }
  }
  out
}

/// A multiply-rotate hash for small integer keys (std's SipHash is several times slower and its
/// protection against crafted keys isn't needed for vertex positions).
#[derive(Default, Clone)]
struct FastHasher(u64);

impl std::hash::Hasher for FastHasher {
  fn write(&mut self, bytes: &[u8]) {
    for b in bytes {
      self.write_u64(*b as u64);
    }
  }
  fn write_u32(&mut self, v: u32) {
    self.write_u64(v as u64);
  }
  fn write_i32(&mut self, v: i32) {
    self.write_u64(v as u32 as u64);
  }
  fn write_u64(&mut self, v: u64) {
    self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
  }
  fn finish(&self) -> u64 {
    self.0
  }
}

/// Reorders each group's instances into spatial chunks (`boxes` gives an instance's box from its
/// floats). Returns the new data and every group's chunks.
fn chunk_instances(
  data: &[f32],
  groups: &[Range<usize>; 4],
  stride: usize,
  size: usize,
  boxes: impl Fn(&[f32]) -> ([f32; 3], [f32; 3]),
) -> (Vec<f32>, [Vec<Bounds>; 4]) {
  let mut out = Vec::with_capacity(data.len());
  let mut chunks: [Vec<Bounds>; 4] = Default::default();
  for (g, range) in groups.iter().enumerate() {
    let items: Vec<&[f32]> = data[range.start * stride..range.end * stride].chunks_exact(stride).collect();
    let boxes: Vec<([f32; 3], [f32; 3])> = items.iter().map(|i| boxes(i)).collect();
    let centers: Vec<[f32; 3]> = boxes.iter().map(|(lo, hi)| [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0)).collect();
    let order = morton_order(&centers);
    for run in order.chunks(size) {
      let start = out.len() / stride;
      for &i in run {
        out.extend_from_slice(items[i as usize]);
      }
      let (lo, hi) = bounds(run.iter().flat_map(|&i| {
        let (lo, hi) = boxes[i as usize];
        [lo, hi]
      }));
      chunks[g].push(Bounds { lo, hi, range: start..out.len() / stride });
    }
  }
  (out, chunks)
}

fn segment_box(i: &[f32]) -> ([f32; 3], [f32; 3]) {
  bounds([[i[0], i[1], i[2]], [i[3], i[4], i[5]]].into_iter())
}

/// The CPU rasterizer's brush: a square of `2r + 1` pixels.
pub fn line_width_px(width: f32) -> f32 {
  let half = (width.clamp(1.0, 6.0) - 1.0) / 2.0;
  2.0 * half.ceil() + 1.0
}

/// Instances split into draw groups, each a contiguous range: index `on_top * 2 + has_id`. Each
/// group is cut into spatial chunks, for culling.
pub struct Instances {
  pub data: Vec<f32>,
  pub groups: [Range<usize>; 4],
  pub chunks: [Vec<Bounds>; 4],
  /// For fold edges: each chunk's normal cone (the chunks of group 1, in order).
  pub cones: Vec<Cone>,
}

fn group_of(on_top: bool, id: u32) -> usize {
  on_top as usize * 2 + (id != 0) as usize
}

fn grouped(mut parts: [Vec<f32>; 4], stride: usize) -> Instances {
  let mut data = Vec::with_capacity(parts.iter().map(Vec::len).sum());
  let mut groups: [Range<usize>; 4] = Default::default();
  for (g, part) in parts.iter_mut().enumerate() {
    let start = data.len() / stride;
    data.append(part);
    groups[g] = start..data.len() / stride;
  }
  Instances { data, groups, chunks: Default::default(), cones: Vec::new() }
}

/// One instance per polyline segment.
pub fn lines(lines: &[Polyline]) -> Instances {
  let mut parts: [Vec<f32>; 4] = Default::default();
  for l in lines {
    let part = &mut parts[group_of(l.on_top, l.id)];
    // lines win depth ties against the faces they lie on (the grid loses them)
    let bias = if l.behind { 2e-5 } else { -2e-5 };
    // 0 solid, 1 dashed, 2 dash-dot (see `fox_render::dash_gap`)
    let dashed = if !l.dashed {
      0.0
    } else if l.dash_dot {
      2.0
    } else {
      1.0
    };
    let width = line_width_px(l.width);
    let mut along = 0.0f32;
    for s in l.points.windows(2) {
      let (a, b) = (s[0], s[1]);
      let start = along;
      along += glam::Vec3::from(a).distance(glam::Vec3::from(b));
      part.extend_from_slice(&[
        a[0],
        a[1],
        a[2],
        b[0],
        b[1],
        b[2],
        color_bits(l.color),
        f32::from_bits(l.id),
        width,
        bias,
        dashed,
        start,
      ]);
    }
  }
  let mut i = grouped(parts, LINE_STRIDE);
  (i.data, i.chunks) = chunk_instances(&i.data, &i.groups, LINE_STRIDE, CHUNK_INSTANCES, segment_box);
  i
}

/// The fold edges of the body surfaces (the meshes marked `analyze`), for the silhouette pass:
/// all in the group of depth-tested lines with ids.
pub fn folds(meshes: &[Mesh]) -> Instances {
  folds_of(meshes.iter().filter(|m| m.analyze))
}

/// The fold edges of the outlined meshes ([`Mesh::outline`]) and the outline's colour (the
/// first one's), for the outline pass; `None` when nothing is outlined.
pub fn outline_folds(meshes: &[Mesh]) -> Option<(Instances, [u8; 4])> {
  let color = meshes.iter().find_map(|m| m.outline)?;
  Some((folds_of(meshes.iter().filter(|m| m.outline.is_some())), color))
}

fn folds_of<'a>(meshes: impl Iterator<Item = &'a Mesh>) -> Instances {
  let mut data = Vec::new();
  for m in meshes {
    for FoldEdge { a, b, normals: [n0, n1], ids } in fox_render::fold_edges(m) {
      data.extend_from_slice(&[a[0], a[1], a[2], b[0], b[1], b[2]]);
      data.extend_from_slice(&[n0[0], n0[1], n0[2], n1[0], n1[1], n1[2]]);
      data.extend_from_slice(&[f32::from_bits(ids[0]), f32::from_bits(ids[1])]);
    }
  }
  let n = data.len() / FOLD_STRIDE;
  let groups = [0..0, 0..n, n..n, n..n];
  let (data, chunks) = chunk_instances(&data, &groups, FOLD_STRIDE, CHUNK_FOLDS, segment_box);
  let cones = chunks[1]
    .iter()
    .map(|c| {
      let edges = &data[c.range.start * FOLD_STRIDE..c.range.end * FOLD_STRIDE];
      Cone::around(edges.chunks_exact(FOLD_STRIDE).flat_map(|e| [[e[6], e[7], e[8]], [e[9], e[10], e[11]]]))
    })
    .collect();
  Instances { data, groups, chunks, cones }
}

pub fn markers(markers: &[Marker]) -> Instances {
  let mut parts: [Vec<f32>; 4] = Default::default();
  for m in markers {
    let r = m.size.clamp(1.0, 12.0).trunc();
    parts[group_of(m.on_top, m.id)].extend_from_slice(&[
      m.at[0],
      m.at[1],
      m.at[2],
      color_bits(m.color),
      f32::from_bits(m.id),
      r,
      if m.hollow { 1.0 } else { 0.0 },
    ]);
  }
  let mut i = grouped(parts, MARKER_STRIDE);
  (i.data, i.chunks) = chunk_instances(&i.data, &i.groups, MARKER_STRIDE, CHUNK_INSTANCES, |m| {
    let p = [m[0], m[1], m[2]];
    (p, p)
  });
  i
}

#[cfg(test)]
mod tests {
  use super::*;

  fn id_at(d: &MeshData, v: u32) -> u32 {
    d.verts[v as usize * MESH_STRIDE + 7].to_bits()
  }

  #[test]
  fn shared_vertices_with_different_ids_are_split() {
    let m = Mesh {
      positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
      indices: vec![0, 1, 2, 0, 2, 3, 0, 9, 1],
      tri_ids: vec![5, 6, 7],
      ..Default::default()
    };
    let d = mesh(&m);
    // the bad triangle is dropped; vertices 0 and 2 are copied for the second face
    assert_eq!(d.indices.len(), 6);
    assert_eq!(d.verts.len() / MESH_STRIDE, 6);
    // (in whatever order the chunking left them)
    let mut ids: Vec<u32> = d.indices.chunks_exact(3).map(|t| id_at(&d, t[0])).collect();
    for t in d.indices.chunks_exact(3) {
      assert!(t.iter().all(|&v| id_at(&d, v) == id_at(&d, t[0])));
    }
    ids.sort();
    assert_eq!(ids, [5, 6]);
    assert_eq!(&d.verts[4 * MESH_STRIDE..4 * MESH_STRIDE + 3], &[0.0, 0.0, 0.0]);
    assert_eq!(d.chunks.len(), 1);
    assert_eq!(d.chunks[0].full.range, 0..6);
  }

  /// A finely tessellated plate of `n` × `n` squares on the XY plane.
  fn plate(n: u32, size: f32) -> Mesh {
    let mut m = Mesh { id: 1, ..Default::default() };
    for j in 0..=n {
      for i in 0..=n {
        m.positions.push([size * i as f32 / n as f32, size * j as f32 / n as f32, 0.0]);
        m.normals.push([0.0, 0.0, 1.0]);
      }
    }
    for j in 0..n {
      for i in 0..n {
        let a = j * (n + 1) + i;
        m.indices.extend_from_slice(&[a, a + 1, a + n + 1, a + 1, a + n + 2, a + n + 1]);
      }
    }
    m
  }

  #[test]
  fn meshes_are_cut_into_compact_chunks_with_coarser_levels() {
    let m = plate(200, 100.0);
    let d = mesh(&m);
    let tris = m.indices.len() / 3;
    assert_eq!(d.chunks.len(), tris.div_ceil(CHUNK_TRIS));
    // every triangle is in exactly one chunk's full range, and the full ranges come first
    let full: usize = d.chunks.iter().map(|c| c.full.range.len()).sum();
    assert_eq!(full, m.indices.len());
    assert!(d.chunks.windows(2).all(|w| w[0].full.range.end == w[1].full.range.start));
    let mut seen: Vec<[u32; 3]> = d.indices[..full]
      .chunks_exact(3)
      .map(|t| {
        let mut t = [t[0], t[1], t[2]];
        t.sort();
        t
      })
      .collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), tris);
    for c in &d.chunks {
      // compact: a chunk covers a small part of the plate, and its box holds its triangles
      let area = (c.full.hi[0] - c.full.lo[0]) * (c.full.hi[1] - c.full.lo[1]);
      assert!(area < 100.0 * 100.0 / 2.0, "{area}");
      for &v in &d.indices[c.full.range.clone()] {
        let p = position(&d.verts, v);
        assert!((0..3).all(|k| p[k] >= c.full.lo[k] && p[k] <= c.full.hi[k]));
      }
      // coarser levels: fewer triangles each, made of the mesh's own vertices
      let [(l1, cell1), (l2, cell2)] = c.lods.clone();
      assert!(
        l1.len() < c.full.range.len() && l2.len() <= l1.len(),
        "{} {} {}",
        c.full.range.len(),
        l1.len(),
        l2.len()
      );
      assert!(cell1 > 0.0 && cell2 >= cell1);
      assert!(l1.start >= full && l2.start >= full);
    }
  }

  #[test]
  fn instances_are_chunked_within_their_groups() {
    let seg = |x: f32, id| Polyline {
      points: (0..=2000).map(|k| [x, k as f32, 0.0]).collect(),
      color: [0; 4],
      width: 1.0,
      id,
      on_top: false,
      dashed: false,
      dash_dot: false,
      behind: false,
    };
    let i = lines(&[seg(0.0, 1), seg(50.0, 0), seg(100.0, 2)]);
    assert_eq!(i.groups, [0..2000, 2000..6000, 6000..6000, 6000..6000]);
    for (g, chunks) in i.chunks.iter().enumerate() {
      let n: usize = chunks.iter().map(|c| c.range.len()).sum();
      assert_eq!(n, i.groups[g].len());
      assert!(chunks.iter().all(|c| c.range.len() <= CHUNK_INSTANCES && i.groups[g].contains(&c.range.start)));
    }
    // each chunk's box holds its segments
    for c in i.chunks.iter().flatten() {
      for s in i.data[c.range.start * LINE_STRIDE..c.range.end * LINE_STRIDE].chunks_exact(LINE_STRIDE) {
        assert!(s[1] >= c.lo[1] && s[4] <= c.hi[1] && s[0] >= c.lo[0] && s[0] <= c.hi[0]);
      }
    }
  }

  #[test]
  fn folds_pack_both_normals_and_ids() {
    let mut m = plate(4, 10.0);
    m.analyze = true;
    // bend the last row up, so the plate folds along y = 7.5
    for p in &mut m.positions {
      if p[1] > 7.6 {
        p[2] = 3.0;
      }
    }
    let f = folds(std::slice::from_ref(&m));
    assert_eq!(f.groups[1].len(), f.data.len() / FOLD_STRIDE);
    assert!(f.groups[1].len() >= 4);
    assert!(f.data.chunks_exact(FOLD_STRIDE).all(|e| e[12].to_bits() == 1 && e[13].to_bits() == 1));
    assert!(folds(&[Mesh { analyze: false, ..m }]).data.is_empty());
    assert_eq!(f.cones.len(), f.chunks[1].len());
  }

  #[test]
  fn cones_hold_their_normals() {
    let c = Cone::around([[0.0, 0.0, 1.0], [0.0, 1.0, 0.0]].into_iter());
    assert!((c.half - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    assert!((glam::Vec3::from(c.axis) - glam::Vec3::new(0.0, 1.0, 1.0).normalize()).length() < 1e-5);
    let opposite = Cone::around([[0.0, 0.0, 1.0], [0.0, 0.0, -1.0]].into_iter());
    assert_eq!(opposite.half, std::f32::consts::PI);
  }

  #[test]
  fn widths_and_groups_match_the_cpu_rules() {
    assert_eq!([1.0, 1.5, 2.0, 3.0, 4.0, 9.0].map(line_width_px), [1.0, 3.0, 3.0, 3.0, 5.0, 7.0]);
    let l = |on_top, id| Polyline {
      points: vec![[0.0; 3], [1.0; 3], [2.0; 3]],
      color: [1, 2, 3, 4],
      width: 1.0,
      id,
      on_top,
      dashed: false,
      dash_dot: false,
      behind: false,
    };
    let i = lines(&[l(true, 3), l(false, 0), l(false, 8)]);
    assert_eq!(i.groups, [0..2, 2..4, 4..4, 4..6]);
    assert_eq!(i.data[2 * LINE_STRIDE + 7].to_bits(), 8);
    assert_eq!(i.data[6].to_bits(), u32::from_le_bytes([1, 2, 3, 4]));
  }
}
