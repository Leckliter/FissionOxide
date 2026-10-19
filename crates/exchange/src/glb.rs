//! glTF 2.0 binary (.glb) export.

use serde_json::json;

use crate::{ExchangeError, Mesh, Result};

/// Writes meshes as a GLB. glTF is metres and Y-up, so a root node scales by 0.001 and turns
/// CLFusion's Z-up into Y-up.
pub fn write_glb(meshes: &[Mesh]) -> Result<Vec<u8>> {
  let mut bin: Vec<u8> = Vec::new();
  let mut views = Vec::new();
  let mut accessors = Vec::new();
  let mut gl_meshes = Vec::new();
  let mut nodes = vec![json!({
    "name": "CLFusion",
    "scale": [0.001, 0.001, 0.001],
    // -90 degrees about X: Z-up -> Y-up
    "rotation": [-std::f64::consts::FRAC_1_SQRT_2, 0.0, 0.0, std::f64::consts::FRAC_1_SQRT_2],
    "children": (1..=meshes.len()).collect::<Vec<_>>(),
  })];
  for m in meshes {
    if m.positions.is_empty() || m.indices.is_empty() {
      return Err(ExchangeError::fmt(format!("mesh '{}' is empty", m.name)));
    }
    let (lo, hi) = m.bounds().unwrap_or(([0.0; 3], [0.0; 3]));
    let mut attributes = serde_json::Map::new();
    // positions
    let off = bin.len();
    for p in &m.positions {
      for c in p {
        bin.extend_from_slice(&c.to_le_bytes());
      }
    }
    views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": bin.len() - off, "target": 34962}));
    accessors.push(json!({
      "bufferView": views.len() - 1, "componentType": 5126, "count": m.positions.len(), "type": "VEC3",
      "min": lo, "max": hi,
    }));
    attributes.insert("POSITION".into(), json!(accessors.len() - 1));
    if m.normals.len() == m.positions.len() {
      let off = bin.len();
      for n in &m.normals {
        for c in n {
          bin.extend_from_slice(&c.to_le_bytes());
        }
      }
      views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": bin.len() - off, "target": 34962}));
      accessors
        .push(json!({"bufferView": views.len() - 1, "componentType": 5126, "count": m.normals.len(), "type": "VEC3"}));
      attributes.insert("NORMAL".into(), json!(accessors.len() - 1));
    }
    let off = bin.len();
    for i in &m.indices {
      bin.extend_from_slice(&i.to_le_bytes());
    }
    views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": bin.len() - off, "target": 34963}));
    accessors
      .push(json!({"bufferView": views.len() - 1, "componentType": 5125, "count": m.indices.len(), "type": "SCALAR"}));
    gl_meshes.push(json!({
      "name": m.name,
      "primitives": [{"attributes": attributes, "indices": accessors.len() - 1, "material": 0}],
    }));
    nodes.push(json!({"name": m.name, "mesh": gl_meshes.len() - 1}));
  }
  let doc = json!({
    "asset": {"version": "2.0", "generator": "CLFusion"},
    "scene": 0,
    "scenes": [{"nodes": [0]}],
    "nodes": nodes,
    "meshes": gl_meshes,
    "materials": [{"name": "Default", "pbrMetallicRoughness": {"baseColorFactor": [0.75, 0.77, 0.8, 1.0], "metallicFactor": 0.1, "roughnessFactor": 0.6}}],
    "accessors": accessors,
    "bufferViews": views,
    "buffers": [{"byteLength": bin.len()}],
  });
  let mut js = serde_json::to_vec(&doc)?;
  while !js.len().is_multiple_of(4) {
    js.push(b' ');
  }
  while !bin.len().is_multiple_of(4) {
    bin.push(0);
  }
  let total = 12 + 8 + js.len() + 8 + bin.len();
  let total = u32::try_from(total).map_err(|_| ExchangeError::fmt("model too large for GLB"))?;
  let mut out = Vec::with_capacity(total as usize);
  out.extend_from_slice(b"glTF");
  out.extend_from_slice(&2u32.to_le_bytes());
  out.extend_from_slice(&total.to_le_bytes());
  out.extend_from_slice(&(js.len() as u32).to_le_bytes());
  out.extend_from_slice(b"JSON");
  out.extend_from_slice(&js);
  out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
  out.extend_from_slice(b"BIN\0");
  out.extend_from_slice(&bin);
  Ok(out)
}
