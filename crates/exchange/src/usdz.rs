//! USDZ export: an uncompressed zip whose first entry is a USD layer, every entry 64-byte aligned.

use std::io::Write;

use crate::{Mesh, Result};

/// The USD (ASCII) layer for the meshes, millimetres and Z-up as the stage metadata says.
pub fn usda(meshes: &[Mesh]) -> String {
  use std::fmt::Write as _;
  let mut s = String::from(
    "#usda 1.0\n(\n    defaultPrim = \"FissionOxide\"\n    metersPerUnit = 0.001\n    upAxis = \"Z\"\n    doc = \"Exported by FissionOxide\"\n)\n\ndef Xform \"FissionOxide\"\n{\n",
  );
  let mut used = std::collections::HashSet::new();
  for (k, m) in meshes.iter().enumerate() {
    // USD prim names are identifiers
    let mut name: String = m.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
      name = format!("Body_{name}");
    }
    if !used.insert(name.clone()) {
      name = format!("{name}_{k}");
    }
    let _ = writeln!(s, "    def Mesh \"{name}\"\n    {{");
    let counts = vec!["3"; m.triangle_count()].join(", ");
    let _ = writeln!(s, "        int[] faceVertexCounts = [{counts}]");
    let idx: Vec<String> = m.indices.iter().map(u32::to_string).collect();
    let _ = writeln!(s, "        int[] faceVertexIndices = [{}]", idx.join(", "));
    let pts: Vec<String> = m.positions.iter().map(|p| format!("({}, {}, {})", p[0], p[1], p[2])).collect();
    let _ = writeln!(s, "        point3f[] points = [{}]", pts.join(", "));
    if let Some((lo, hi)) = m.bounds() {
      let _ =
        writeln!(s, "        float3[] extent = [({}, {}, {}), ({}, {}, {})]", lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]);
    }
    if m.normals.len() == m.positions.len() {
      let ns: Vec<String> = m.normals.iter().map(|n| format!("({}, {}, {})", n[0], n[1], n[2])).collect();
      let _ = writeln!(
        s,
        "        normal3f[] normals = [{}] (\n            interpolation = \"vertex\"\n        )",
        ns.join(", ")
      );
    }
    let _ = writeln!(
      s,
      "        uniform token subdivisionScheme = \"none\"\n        color3f[] primvars:displayColor = [(0.75, 0.77, 0.8)]\n    }}"
    );
  }
  s.push_str("}\n");
  s
}

pub fn write_usdz(meshes: &[Mesh]) -> Result<Vec<u8>> {
  let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
  let opts =
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).with_alignment(64);
  w.start_file("model.usda", opts)?;
  w.write_all(usda(meshes).as_bytes())?;
  Ok(w.finish()?.into_inner())
}
