//! Scene data packed into the vertex and instance layouts the shaders read.
//!
//! Everything is a flat `f32` array; colours and ids travel as their bit patterns
//! (`f32::from_bits`) and the attribute pointers read them back as bytes and integers.

use std::collections::HashMap;
use std::ops::Range;

use clf_render::{Marker, Mesh, Polyline};

/// Floats per mesh vertex: position, normal, colour bits, id bits.
pub const MESH_STRIDE: usize = 8;
/// Floats per line segment: start, end, colour bits, id bits, width (px), depth bias, dashed.
pub const LINE_STRIDE: usize = 11;
/// Floats per marker: position, colour bits, id bits, radius (px).
pub const MARKER_STRIDE: usize = 6;

/// A mesh as GPU vertices: each vertex carries its triangle's pick id.
pub struct MeshData {
  pub verts: Vec<f32>,
  pub indices: Vec<u32>,
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
  MeshData { verts, indices }
}

/// The CPU rasterizer's brush: a square of `2r + 1` pixels.
pub fn line_width_px(width: f32) -> f32 {
  let half = (width.clamp(1.0, 6.0) - 1.0) / 2.0;
  2.0 * half.ceil() + 1.0
}

/// Instances split into draw groups, each a contiguous range: index `on_top * 2 + has_id`.
pub struct Instances {
  pub data: Vec<f32>,
  pub groups: [Range<usize>; 4],
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
  Instances { data, groups }
}

/// One instance per polyline segment.
pub fn lines(lines: &[Polyline]) -> Instances {
  let mut parts: [Vec<f32>; 4] = Default::default();
  for l in lines {
    let part = &mut parts[group_of(l.on_top, l.id)];
    // lines win depth ties against the faces they lie on (the grid loses them)
    let bias = if l.behind { 2e-5 } else { -2e-5 };
    let (width, dashed) = (line_width_px(l.width), if l.dashed { 1.0 } else { 0.0 });
    for s in l.points.windows(2) {
      let (a, b) = (s[0], s[1]);
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
      ]);
    }
  }
  grouped(parts, LINE_STRIDE)
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
    ]);
  }
  grouped(parts, MARKER_STRIDE)
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
    for (t, want) in [(0, 5), (1, 6)] {
      for k in 0..3 {
        assert_eq!(id_at(&d, d.indices[t * 3 + k]), want);
      }
    }
    assert_eq!(&d.verts[4 * MESH_STRIDE..4 * MESH_STRIDE + 3], &[0.0, 0.0, 0.0]);
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
      behind: false,
    };
    let i = lines(&[l(true, 3), l(false, 0), l(false, 8)]);
    assert_eq!(i.groups, [0..2, 2..4, 4..4, 4..6]);
    assert_eq!(i.data[2 * LINE_STRIDE + 7].to_bits(), 8);
    assert_eq!(i.data[6].to_bits(), u32::from_le_bytes([1, 2, 3, 4]));
  }
}
