//! Wavefront OBJ.

use crate::{ExchangeError, Mesh, Result, parse_f32};

/// Reads an OBJ: one mesh per `o` (or `g` when there are no objects), polygons fan-triangulated,
/// negative (relative) indices supported. Texture coordinates and materials are ignored.
pub fn read_obj(bytes: &[u8]) -> Result<Vec<Mesh>> {
  let text = String::from_utf8_lossy(bytes);
  let has_objects = text.lines().any(|l| l.trim_start().starts_with("o "));
  let mut verts: Vec<[f32; 3]> = Vec::new();
  let mut meshes: Vec<(String, Vec<u32>)> = vec![("OBJ".into(), Vec::new())];
  for (n, line) in text.lines().enumerate() {
    let line = line.split('#').next().unwrap_or_default();
    let mut it = line.split_whitespace();
    match it.next() {
      Some("v") => {
        let c: Vec<f32> = it.take(3).map(parse_f32).collect::<Result<_>>()?;
        if c.len() != 3 {
          return Err(ExchangeError::fmt(format!("OBJ line {}: a vertex needs x y z", n + 1)));
        }
        verts.push([c[0], c[1], c[2]]);
      }
      Some(k @ ("o" | "g")) if (k == "o") == has_objects => {
        let name = it.collect::<Vec<_>>().join(" ");
        let name = if name.is_empty() { format!("Object{}", meshes.len()) } else { name };
        match meshes.last_mut() {
          Some(last) if last.1.is_empty() => last.0 = name,
          _ => meshes.push((name, Vec::new())),
        }
      }
      Some("f") => {
        let mut poly = Vec::new();
        for tok in it {
          let i: i64 = tok
            .split('/')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| ExchangeError::fmt(format!("OBJ line {}: bad face index '{tok}'", n + 1)))?;
          let idx = if i < 0 { verts.len() as i64 + i } else { i - 1 };
          if idx < 0 || idx >= verts.len() as i64 {
            return Err(ExchangeError::fmt(format!("OBJ line {}: face index {i} out of range", n + 1)));
          }
          poly.push(idx as u32);
        }
        if let Some(last) = meshes.last_mut() {
          for k in 1..poly.len().saturating_sub(1) {
            last.1.extend_from_slice(&[poly[0], poly[k], poly[k + 1]]);
          }
        }
      }
      _ => {}
    }
  }
  // each object gets its own compact vertex list
  let mut out = Vec::new();
  for (name, idx) in meshes.into_iter().filter(|m| !m.1.is_empty()) {
    let mut map = std::collections::HashMap::new();
    let mut m = Mesh { name, ..Mesh::default() };
    for i in idx {
      let j = *map.entry(i).or_insert_with(|| {
        m.positions.push(verts[i as usize]);
        (m.positions.len() - 1) as u32
      });
      m.indices.push(j);
    }
    out.push(m.validate()?.welded());
  }
  if out.is_empty() {
    return Err(ExchangeError::fmt("the OBJ file has no faces"));
  }
  Ok(out)
}

/// Writes meshes as one OBJ, an `o` per mesh, with normals when the meshes have them.
pub fn write_obj(meshes: &[Mesh]) -> String {
  use std::fmt::Write as _;
  let mut s = String::from("# CLFusion OBJ export (mm)\n");
  let mut base = 1usize;
  for m in meshes {
    let _ = writeln!(s, "o {}", m.name.replace(char::is_whitespace, "_"));
    for p in &m.positions {
      let _ = writeln!(s, "v {} {} {}", p[0], p[1], p[2]);
    }
    let normals = m.normals.len() == m.positions.len();
    if normals {
      for n in &m.normals {
        let _ = writeln!(s, "vn {} {} {}", n[0], n[1], n[2]);
      }
    }
    for t in m.indices.chunks_exact(3) {
      let (a, b, c) = (t[0] as usize + base, t[1] as usize + base, t[2] as usize + base);
      if normals {
        let _ = writeln!(s, "f {a}//{a} {b}//{b} {c}//{c}");
      } else {
        let _ = writeln!(s, "f {a} {b} {c}");
      }
    }
    base += m.positions.len();
  }
  s
}
