//! STL: binary and ASCII.

use crate::{ExchangeError, Mesh, Result, parse_f32};

/// Reads binary or ASCII STL into one welded mesh.
pub fn read_stl(bytes: &[u8]) -> Result<Mesh> {
  let raw = if is_binary(bytes) { read_binary(bytes)? } else { read_ascii(bytes)? };
  if raw.indices.is_empty() {
    return Err(ExchangeError::fmt("the STL file has no triangles"));
  }
  raw.validate().map(|m| m.welded())
}

/// A file is binary when its size matches the triangle count in the header. ASCII files start
/// with "solid", but so do many binary ones, so the size check decides.
fn is_binary(b: &[u8]) -> bool {
  if b.len() < 84 {
    return false;
  }
  let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize;
  n.checked_mul(50).and_then(|x| x.checked_add(84)) == Some(b.len()) || !b.starts_with(b"solid")
}

fn read_binary(b: &[u8]) -> Result<Mesh> {
  let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize;
  let body = b.get(84..).unwrap_or_default();
  if body.len() / 50 < n {
    return Err(ExchangeError::fmt("binary STL is truncated"));
  }
  let mut m = Mesh { name: "STL".into(), ..Mesh::default() };
  m.positions.reserve(n * 3);
  m.indices.reserve(n * 3);
  let f = |c: &[u8], k: usize| f32::from_le_bytes([c[k], c[k + 1], c[k + 2], c[k + 3]]);
  for chunk in body.chunks_exact(50).take(n) {
    for v in 0..3 {
      let o = 12 + v * 12;
      m.positions.push([f(chunk, o), f(chunk, o + 4), f(chunk, o + 8)]);
      m.indices.push(m.indices.len() as u32);
    }
  }
  Ok(m)
}

fn read_ascii(b: &[u8]) -> Result<Mesh> {
  let text = String::from_utf8_lossy(b);
  let mut m = Mesh { name: "STL".into(), ..Mesh::default() };
  let mut facet: Vec<[f32; 3]> = Vec::new();
  for line in text.lines() {
    let mut it = line.split_whitespace();
    match it.next() {
      Some("solid") => {
        if let Some(n) = it.next() {
          m.name = n.to_string();
        }
      }
      Some("vertex") => {
        let x = parse_f32(it.next().unwrap_or_default())?;
        let y = parse_f32(it.next().unwrap_or_default())?;
        let z = parse_f32(it.next().unwrap_or_default())?;
        facet.push([x, y, z]);
      }
      Some("endloop") => {
        // fan-triangulate (some writers emit quads)
        for k in 1..facet.len().saturating_sub(1) {
          for p in [facet[0], facet[k], facet[k + 1]] {
            m.positions.push(p);
            m.indices.push(m.indices.len() as u32);
          }
        }
        facet.clear();
      }
      _ => {}
    }
  }
  Ok(m)
}

/// Binary STL of all meshes.
pub fn write_stl(meshes: &[Mesh]) -> Vec<u8> {
  let tris: usize = meshes.iter().map(Mesh::triangle_count).sum();
  let mut out = Vec::with_capacity(84 + tris * 50);
  let mut header = [0u8; 80];
  let tag = b"FissionOxide binary STL (mm)";
  header[..tag.len()].copy_from_slice(tag);
  out.extend_from_slice(&header);
  out.extend_from_slice(&(u32::try_from(tris).unwrap_or(u32::MAX)).to_le_bytes());
  for m in meshes {
    for t in 0..m.triangle_count() {
      let n = m.face_normal(t);
      let mut put = |v: [f32; 3]| {
        for k in v {
          out.extend_from_slice(&k.to_le_bytes());
        }
      };
      put(n);
      for k in 0..3 {
        put(m.indices.get(t * 3 + k).and_then(|&i| m.positions.get(i as usize)).copied().unwrap_or([0.0; 3]));
      }
      out.extend_from_slice(&[0, 0]);
    }
  }
  out
}

/// ASCII STL of all meshes (one `solid` per mesh).
pub fn write_stl_ascii(meshes: &[Mesh]) -> String {
  use std::fmt::Write as _;
  let mut s = String::new();
  for m in meshes {
    let name = m.name.replace(char::is_whitespace, "_");
    let _ = writeln!(s, "solid {name}");
    for t in 0..m.triangle_count() {
      let n = m.face_normal(t);
      let _ = writeln!(s, "  facet normal {:e} {:e} {:e}\n    outer loop", n[0], n[1], n[2]);
      for k in 0..3 {
        let p = m.indices.get(t * 3 + k).and_then(|&i| m.positions.get(i as usize)).copied().unwrap_or([0.0; 3]);
        let _ = writeln!(s, "      vertex {:e} {:e} {:e}", p[0], p[1], p[2]);
      }
      let _ = writeln!(s, "    endloop\n  endfacet");
    }
    let _ = writeln!(s, "endsolid {name}");
  }
  s
}
