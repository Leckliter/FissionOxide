//! File formats for FissionOxide that don't need the B-rep kernel: triangle meshes (STL, OBJ, PLY,
//! 3MF, glTF, USDZ), 2D sketch geometry (DXF, SVG), FissionOxide design archives (`.foxz`) and inspection
//! of Autodesk Fusion archives (`.f3d`/`.f3z`).
//!
//! B-rep formats (STEP, IGES, BRep) go through `fox-kernel`; [`FORMATS`] lists every format the
//! app reads or writes so the UI, CLI and engine agree on one table.
//!
//! Units are millimetres throughout; readers convert from the file's units where the format says.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

pub mod archive;
pub mod dxf;
mod glb;
pub mod nurbs;
mod obj;
mod ply;
mod stl;
pub mod svg;
mod threemf;
mod usdz;

pub use glb::write_glb;
pub use obj::{read_obj, write_obj};
pub use ply::{read_ply, write_ply};
pub use stl::{read_stl, write_stl, write_stl_ascii};
pub use threemf::{read_3mf, write_3mf};
pub use usdz::write_usdz;

#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
  #[error("{0}")]
  Format(String),
  #[error("{0}")]
  Io(#[from] std::io::Error),
  #[error("zip: {0}")]
  Zip(#[from] zip::result::ZipError),
  #[error("xml: {0}")]
  Xml(String),
  #[error("json: {0}")]
  Json(#[from] serde_json::Error),
}

impl ExchangeError {
  pub(crate) fn fmt(s: impl Into<String>) -> Self {
    ExchangeError::Format(s.into())
  }
}

pub type Result<T> = std::result::Result<T, ExchangeError>;

/// What a file format carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatKind {
  /// A full parametric FissionOxide design.
  Design,
  /// A packaged design (archive with thumbnail and optional version history).
  Archive,
  /// Exact solid geometry (B-rep).
  Brep,
  /// Triangle meshes.
  Mesh,
  /// 2D sketch geometry.
  Sketch,
}

/// One file format: its extensions and what the app can do with it.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Format {
  pub id: &'static str,
  pub name: &'static str,
  pub extensions: &'static [&'static str],
  pub kind: FormatKind,
  pub import: bool,
  pub export: bool,
  pub note: &'static str,
}

macro_rules! formats {
  ($( $id:literal, $name:literal, [$($ext:literal),*], $kind:ident, $imp:literal, $exp:literal, $note:literal; )*) => {
    /// Every format FissionOxide knows, in the order the file dialogs list them.
    pub const FORMATS: &[Format] = &[ $( Format {
      id: $id, name: $name, extensions: &[$($ext),*], kind: FormatKind::$kind, import: $imp, export: $exp, note: $note,
    } ),* ];
  };
}

formats! {
  "fox", "FissionOxide design", ["fox", "clf"], Design, true, true, "Native parametric design (JSON): timeline, sketches, parameters";
  "foxz", "FissionOxide archive", ["foxz", "clfz"], Archive, true, true, "Design + thumbnail + optional version history in one zip (FissionOxide's .f3d/.f3z)";
  "f3d", "Autodesk Fusion archive", ["f3d", "f3z"], Archive, true, false, "Stored in the data panel with its thumbnail; geometry is Fusion's proprietary ShapeManager B-rep and can't be read - export STEP from Fusion for geometry";
  "step", "STEP", ["step", "stp"], Brep, true, true, "AP214 solids";
  "iges", "IGES", ["iges", "igs"], Brep, true, true, "Trimmed surfaces / solids";
  "brep", "OpenCASCADE BRep", ["brep"], Brep, true, true, "Lossless OCCT text B-rep";
  "stl", "STL mesh", ["stl"], Mesh, true, true, "Binary or ASCII; imported meshes become faceted bodies";
  "obj", "OBJ mesh", ["obj"], Mesh, true, true, "Wavefront OBJ";
  "3mf", "3MF", ["3mf"], Mesh, true, true, "3D Manufacturing Format (3D printing)";
  "ply", "PLY mesh", ["ply"], Mesh, true, true, "ASCII or binary";
  "glb", "glTF binary", ["glb"], Mesh, false, true, "Metres, Y-up, for web and AR viewers";
  "usdz", "USDZ", ["usdz"], Mesh, false, true, "Universal Scene Description package (ASCII layer)";
  "dxf", "DXF", ["dxf"], Sketch, true, true, "2D: imports into a new sketch; exports the active or last sketch";
  "svg", "SVG", ["svg"], Sketch, true, true, "2D: imports into a new sketch (paths, shapes, transforms); exports the active or last sketch with exact curves";
}

/// Looks up a format by file extension (case-insensitive, without the dot).
pub fn format_for_ext(ext: &str) -> Option<&'static Format> {
  let e = ext.trim_start_matches('.').to_ascii_lowercase();
  FORMATS.iter().find(|f| f.extensions.contains(&e.as_str()))
}

/// Looks up a format by the extension of a path.
pub fn format_for_path(path: &std::path::Path) -> Option<&'static Format> {
  format_for_ext(&path.extension()?.to_string_lossy())
}

/// An indexed triangle mesh in millimetres.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
  pub name: String,
  pub positions: Vec<[f32; 3]>,
  /// Per-vertex normals; empty when the source had none.
  pub normals: Vec<[f32; 3]>,
  /// Three vertex indices per triangle.
  pub indices: Vec<u32>,
}

impl Mesh {
  pub fn triangle_count(&self) -> usize {
    self.indices.len() / 3
  }

  /// Merges vertices with identical coordinates (STL and per-face-vertex OBJ/PLY data repeat
  /// them), drops degenerate triangles and normals. A welded mesh has shared edges, which is
  /// what turning it into a closed body needs.
  pub fn welded(&self) -> Mesh {
    let mut map: std::collections::HashMap<[u32; 3], u32> = std::collections::HashMap::new();
    let mut positions = Vec::new();
    let mut remap = Vec::with_capacity(self.positions.len());
    for p in &self.positions {
      // +0.0 and -0.0 are the same point
      let key = [(p[0] + 0.0).to_bits(), (p[1] + 0.0).to_bits(), (p[2] + 0.0).to_bits()];
      let id = *map.entry(key).or_insert_with(|| {
        positions.push(*p);
        (positions.len() - 1) as u32
      });
      remap.push(id);
    }
    let mut indices = Vec::with_capacity(self.indices.len());
    for t in self.indices.chunks_exact(3) {
      let v: Vec<u32> = t.iter().filter_map(|&i| remap.get(i as usize).copied()).collect();
      if v.len() == 3 && v[0] != v[1] && v[1] != v[2] && v[0] != v[2] {
        indices.extend_from_slice(&v);
      }
    }
    Mesh { name: self.name.clone(), positions, normals: Vec::new(), indices }
  }

  /// Scales every position (unit conversion).
  pub fn scale(&mut self, k: f32) {
    for p in &mut self.positions {
      for c in p.iter_mut() {
        *c *= k;
      }
    }
  }

  /// Axis-aligned bounds, or `None` for an empty mesh.
  pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
    let mut it = self.positions.iter();
    let first = *it.next()?;
    let (mut lo, mut hi) = (first, first);
    for p in it {
      for k in 0..3 {
        lo[k] = lo[k].min(p[k]);
        hi[k] = hi[k].max(p[k]);
      }
    }
    Some((lo, hi))
  }

  /// Face normal of triangle `t` (unit, or zero for a degenerate triangle).
  pub fn face_normal(&self, t: usize) -> [f32; 3] {
    let get =
      |k: usize| self.indices.get(t * 3 + k).and_then(|&i| self.positions.get(i as usize)).copied().unwrap_or([0.0; 3]);
    let (a, b, c) = (get(0), get(1), get(2));
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 0.0 { [n[0] / l, n[1] / l, n[2] / l] } else { [0.0; 3] }
  }

  /// Checks indices are in range.
  pub(crate) fn validate(self) -> Result<Mesh> {
    let n = self.positions.len() as u32;
    if self.indices.iter().any(|&i| i >= n) {
      return Err(ExchangeError::fmt(format!("mesh '{}' has a vertex index out of range", self.name)));
    }
    if self.positions.iter().flatten().any(|v| !v.is_finite()) {
      return Err(ExchangeError::fmt(format!("mesh '{}' has non-finite coordinates", self.name)));
    }
    Ok(self)
  }
}

/// Reads any supported mesh format by extension.
pub fn read_mesh(ext: &str, bytes: &[u8]) -> Result<Vec<Mesh>> {
  match ext.to_ascii_lowercase().as_str() {
    "stl" => read_stl(bytes).map(|m| vec![m]),
    "obj" => read_obj(bytes),
    "ply" => read_ply(bytes).map(|m| vec![m]),
    "3mf" => read_3mf(bytes),
    e => Err(ExchangeError::fmt(format!("'.{e}' is not a mesh format FissionOxide reads (stl, obj, ply, 3mf)"))),
  }
}

/// Writes meshes in a format chosen by extension.
pub fn write_mesh(ext: &str, meshes: &[Mesh]) -> Result<Vec<u8>> {
  match ext.to_ascii_lowercase().as_str() {
    "stl" => Ok(write_stl(meshes)),
    "obj" => Ok(write_obj(meshes).into_bytes()),
    "ply" => Ok(write_ply(meshes)),
    "3mf" => write_3mf(meshes, None),
    "glb" => write_glb(meshes),
    "usdz" => write_usdz(meshes),
    e => Err(ExchangeError::fmt(format!("'.{e}' is not a mesh format FissionOxide writes"))),
  }
}

fn parse_f32(s: &str) -> Result<f32> {
  s.trim().parse::<f32>().map_err(|_| ExchangeError::fmt(format!("bad number '{s}'")))
}

#[cfg(test)]
mod tests;
