//! 3MF (3D Manufacturing Format): a zip with an XML model part.

use std::collections::HashMap;
use std::io::{Read, Write};

use quick_xml::events::{BytesStart, Event};

use crate::{ExchangeError, Mesh, Result};

const MODEL_PATH: &str = "3D/3dmodel.model";

/// Row-major 3x4 affine transform as 3MF writes it: `m00 m01 m02 m10 m11 m12 m20 m21 m22 m30 m31
/// m32`, applied to row vectors (`p' = p * M`, translation in the last row).
type Xf = [f32; 12];
const IDENTITY: Xf = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

fn apply(m: &Xf, p: [f32; 3]) -> [f32; 3] {
  [
    p[0] * m[0] + p[1] * m[3] + p[2] * m[6] + m[9],
    p[0] * m[1] + p[1] * m[4] + p[2] * m[7] + m[10],
    p[0] * m[2] + p[1] * m[5] + p[2] * m[8] + m[11],
  ]
}

/// `a` then `b`.
fn compose(a: &Xf, b: &Xf) -> Xf {
  let mut r = [0.0; 12];
  for row in 0..4 {
    for col in 0..3 {
      let mut v = if row == 3 { b[9 + col] } else { 0.0 };
      for k in 0..3 {
        v += a[row * 3 + k] * b[k * 3 + col];
      }
      r[row * 3 + col] = v;
    }
  }
  r
}

#[derive(Default)]
struct Object {
  name: String,
  positions: Vec<[f32; 3]>,
  indices: Vec<u32>,
  components: Vec<(String, Xf)>,
}

fn attrs(e: &BytesStart<'_>) -> HashMap<String, String> {
  e.attributes()
    .filter_map(|a| a.ok())
    .map(|a| (a.key.local_name().as_ref().to_string(), a.value.to_string()))
    .collect()
}

fn num(a: &HashMap<String, String>, k: &str) -> Result<f32> {
  a.get(k)
    .and_then(|v| v.trim().parse().ok())
    .ok_or_else(|| ExchangeError::fmt(format!("3MF: missing or bad attribute '{k}'")))
}

fn xform(a: &HashMap<String, String>) -> Xf {
  let Some(t) = a.get("transform") else { return IDENTITY };
  let v: Vec<f32> = t.split_whitespace().filter_map(|s| s.parse().ok()).collect();
  v.try_into().unwrap_or(IDENTITY)
}

/// Reads the build items of a 3MF package as meshes (in millimetres, transforms applied).
pub fn read_3mf(bytes: &[u8]) -> Result<Vec<Mesh>> {
  let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
  let model_name = model_part(&mut zip).unwrap_or_else(|| MODEL_PATH.to_string());
  let mut xml = String::new();
  zip
    .by_name(model_name.trim_start_matches('/'))
    .map_err(|_| ExchangeError::fmt("3MF: the package has no 3D model part"))?
    .read_to_string(&mut xml)?;

  let mut reader = quick_xml::Reader::from_str(&xml);
  reader.config_mut().trim_text(true);
  let mut objects: HashMap<String, Object> = HashMap::new();
  let mut build: Vec<(String, Xf)> = Vec::new();
  let mut current: Option<(String, Object)> = None;
  let mut scale = 1.0f32;
  loop {
    let ev = reader.read_event().map_err(|e| ExchangeError::Xml(e.to_string()))?;
    let (e, is_end) = match &ev {
      Event::Start(e) | Event::Empty(e) => (Some(e.clone()), matches!(ev, Event::Empty(_))),
      Event::End(e) => {
        if e.local_name().as_ref() == "object"
          && let Some((id, o)) = current.take()
        {
          objects.insert(id, o);
        }
        (None, false)
      }
      Event::Eof => break,
      _ => (None, false),
    };
    let Some(e) = e else { continue };
    let a = attrs(&e);
    match e.local_name().as_ref() {
      "model" => {
        scale = match a.get("unit").map(String::as_str) {
          Some("micron") => 0.001,
          Some("centimeter") => 10.0,
          Some("inch") => 25.4,
          Some("foot") => 304.8,
          Some("meter") => 1000.0,
          _ => 1.0,
        }
      }
      "object" => {
        let id = a.get("id").cloned().unwrap_or_default();
        let name = a.get("name").cloned().unwrap_or_else(|| format!("Object {id}"));
        let o = Object { name, ..Object::default() };
        if is_end {
          objects.insert(id, o);
        } else {
          current = Some((id, o));
        }
      }
      "vertex" => {
        if let Some((_, o)) = current.as_mut() {
          o.positions.push([num(&a, "x")?, num(&a, "y")?, num(&a, "z")?]);
        }
      }
      "triangle" => {
        if let Some((_, o)) = current.as_mut() {
          for k in ["v1", "v2", "v3"] {
            o.indices.push(num(&a, k)? as u32);
          }
        }
      }
      "component" => {
        if let Some((_, o)) = current.as_mut() {
          o.components.push((a.get("objectid").cloned().unwrap_or_default(), xform(&a)));
        }
      }
      "item" => build.push((a.get("objectid").cloned().unwrap_or_default(), xform(&a))),
      _ => {}
    }
  }

  let mut out = Vec::new();
  for (id, xf) in &build {
    let Some(o) = objects.get(id) else { continue };
    let mut m = Mesh { name: o.name.clone(), ..Mesh::default() };
    flatten(&objects, id, xf, &mut m, 0)?;
    if !m.indices.is_empty() {
      m.scale(scale);
      out.push(m.validate()?.welded());
    }
  }
  if out.is_empty() {
    return Err(ExchangeError::fmt("the 3MF file has no meshes to build"));
  }
  Ok(out)
}

/// Appends object `id` (and its components, recursively) to `m` under transform `xf`.
fn flatten(objects: &HashMap<String, Object>, id: &str, xf: &Xf, m: &mut Mesh, depth: u32) -> Result<()> {
  if depth > 32 {
    return Err(ExchangeError::fmt("3MF: components nest too deeply (cycle?)"));
  }
  let Some(o) = objects.get(id) else { return Ok(()) };
  let base = m.positions.len() as u32;
  if o.indices.iter().any(|&i| i as usize >= o.positions.len()) {
    return Err(ExchangeError::fmt(format!("3MF: object {id} has a vertex index out of range")));
  }
  m.positions.extend(o.positions.iter().map(|p| apply(xf, *p)));
  m.indices.extend(o.indices.iter().map(|i| i + base));
  for (cid, cxf) in &o.components {
    flatten(objects, cid, &compose(cxf, xf), m, depth + 1)?;
  }
  Ok(())
}

/// The model part named by the package relationships, if any.
fn model_part<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>) -> Option<String> {
  let mut rels = String::new();
  zip.by_name("_rels/.rels").ok()?.read_to_string(&mut rels).ok()?;
  let mut reader = quick_xml::Reader::from_str(&rels);
  loop {
    match reader.read_event().ok()? {
      Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "Relationship" => {
        let a = attrs(&e);
        if a.get("Type").is_some_and(|t| t.ends_with("/3dmodel")) {
          return a.get("Target").cloned();
        }
      }
      Event::Eof => return None,
      _ => {}
    }
  }
}

fn xml_escape(s: &str) -> String {
  s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Writes a 3MF package (millimetres), one object and build item per mesh, with an optional PNG
/// thumbnail.
pub fn write_3mf(meshes: &[Mesh], thumbnail_png: Option<&[u8]>) -> Result<Vec<u8>> {
  use std::fmt::Write as _;
  let mut model = String::from(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n <metadata name=\"Application\">CLFusion</metadata>\n <resources>\n",
  );
  for (k, m) in meshes.iter().enumerate() {
    let _ = writeln!(
      model,
      "  <object id=\"{}\" name=\"{}\" type=\"model\">\n   <mesh>\n    <vertices>",
      k + 1,
      xml_escape(&m.name)
    );
    for p in &m.positions {
      let _ = writeln!(model, "     <vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", p[0], p[1], p[2]);
    }
    model.push_str("    </vertices>\n    <triangles>\n");
    for t in m.indices.chunks_exact(3) {
      let _ = writeln!(model, "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>", t[0], t[1], t[2]);
    }
    model.push_str("    </triangles>\n   </mesh>\n  </object>\n");
  }
  model.push_str(" </resources>\n <build>\n");
  for k in 0..meshes.len() {
    let _ = writeln!(model, "  <item objectid=\"{}\"/>", k + 1);
  }
  model.push_str(" </build>\n</model>\n");

  let thumb_rel = if thumbnail_png.is_some() {
    "\n <Relationship Target=\"/Metadata/thumbnail.png\" Id=\"rel1\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail\"/>"
  } else {
    ""
  };
  let rels = format!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n <Relationship Target=\"/{MODEL_PATH}\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/>{thumb_rel}\n</Relationships>\n"
  );
  let types = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\n <Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/>\n <Default Extension=\"png\" ContentType=\"image/png\"/>\n</Types>\n";

  let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
  let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
  w.start_file("[Content_Types].xml", opts)?;
  w.write_all(types.as_bytes())?;
  w.start_file("_rels/.rels", opts)?;
  w.write_all(rels.as_bytes())?;
  w.start_file(MODEL_PATH, opts)?;
  w.write_all(model.as_bytes())?;
  if let Some(png) = thumbnail_png {
    w.start_file("Metadata/thumbnail.png", opts)?;
    w.write_all(png)?;
  }
  Ok(w.finish()?.into_inner())
}
