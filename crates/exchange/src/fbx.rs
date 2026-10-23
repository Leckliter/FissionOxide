//! FBX export: binary FBX 7.4, the version Autodesk's FBX SDK and Fusion write. Each mesh is a
//! Geometry with per-corner normals under a Model, all sharing one grey material. FBX is Y-up:
//! the triangles turn from FissionOxide's Z-up, and the unit is the millimetre (a unit scale of
//! 0.1 cm).

use crate::{ExchangeError, Mesh, Result};

const VERSION: u32 = 7400;
const MAGIC: &[u8] = b"Kaydara FBX Binary  \0\x1a\0";
/// A node list ends with an empty record: end offset, property count and length, name length.
const SENTINEL: [u8; 13] = [0; 13];
/// The file id, creation time and footer id that belong together (the FBX SDK checks the
/// footer against the time, so these are fixed rather than now).
const FILE_ID: [u8; 16] =
  [0x28, 0xb3, 0x2a, 0xeb, 0xb6, 0x24, 0xcc, 0xc2, 0xbf, 0xc8, 0xb0, 0x2a, 0xa9, 0x2b, 0xfc, 0xf1];
const TIME: &str = "1970-01-01 10:00:00:000";
const FOOT_ID: [u8; 16] =
  [0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e];
const FOOT_MAGIC: [u8; 16] =
  [0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b];

/// One property of a node.
enum Prop {
  Bool(bool),
  I32(i32),
  I64(i64),
  F64(f64),
  Str(String),
  Raw(Vec<u8>),
  F64s(Vec<f64>),
  I32s(Vec<i32>),
}

impl Prop {
  fn write(&self, out: &mut Vec<u8>) {
    let array = |out: &mut Vec<u8>, code: u8, n: usize, bytes: Vec<u8>| {
      out.push(code);
      out.extend_from_slice(&(n as u32).to_le_bytes());
      // not compressed
      out.extend_from_slice(&0u32.to_le_bytes());
      out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
      out.extend_from_slice(&bytes);
    };
    match self {
      Prop::Bool(b) => out.extend_from_slice(&[b'C', u8::from(*b)]),
      Prop::I32(v) => {
        out.push(b'I');
        out.extend_from_slice(&v.to_le_bytes());
      }
      Prop::I64(v) => {
        out.push(b'L');
        out.extend_from_slice(&v.to_le_bytes());
      }
      Prop::F64(v) => {
        out.push(b'D');
        out.extend_from_slice(&v.to_le_bytes());
      }
      Prop::Str(s) => {
        out.push(b'S');
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
      }
      Prop::Raw(b) => {
        out.push(b'R');
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
      }
      Prop::F64s(v) => array(out, b'd', v.len(), v.iter().flat_map(|x| x.to_le_bytes()).collect()),
      Prop::I32s(v) => array(out, b'i', v.len(), v.iter().flat_map(|x| x.to_le_bytes()).collect()),
    }
  }
}

/// A node: a name, its properties and the nodes under it.
struct Node {
  name: &'static str,
  props: Vec<Prop>,
  children: Vec<Node>,
}

fn node(name: &'static str, props: Vec<Prop>) -> Node {
  Node { name, props, children: Vec::new() }
}

fn parent(name: &'static str, props: Vec<Prop>, children: Vec<Node>) -> Node {
  Node { name, props, children }
}

fn s(v: &str) -> Prop {
  Prop::Str(v.to_string())
}

/// An object's name and class, as FBX stores them: `name\0\x01Class`.
fn object_name(name: &str, class: &str) -> Prop {
  Prop::Str(format!("{name}\0\x01{class}"))
}

/// A `P` entry of a Properties70 block.
fn p(name: &str, kind: &str, label: &str, values: Vec<Prop>) -> Node {
  let mut props = vec![s(name), s(kind), s(label), s("")];
  props.extend(values);
  node("P", props)
}

impl Node {
  /// Writes the node at the end of `out` (whose length is the file offset), the empty record
  /// after its children, or after a node with neither properties nor children unless it is the
  /// last of its list (as the FBX SDK writes them).
  fn write(&self, out: &mut Vec<u8>, last: bool) -> Result<()> {
    let start = out.len();
    out.extend_from_slice(&[0; 12]);
    let name = self.name.as_bytes();
    out.push(u8::try_from(name.len()).map_err(|_| ExchangeError::fmt("FBX node name too long"))?);
    out.extend_from_slice(name);
    let props_at = out.len();
    for p in &self.props {
      p.write(out);
    }
    let props_len = out.len() - props_at;
    if let Some((tail, rest)) = self.children.split_last() {
      for c in rest {
        c.write(out, false)?;
      }
      tail.write(out, true)?;
      out.extend_from_slice(&SENTINEL);
    } else if self.props.is_empty() && !last {
      out.extend_from_slice(&SENTINEL);
    }
    let field = |v: usize| u32::try_from(v).map_err(|_| ExchangeError::fmt("FBX file over 4 GB"));
    let head = [field(out.len())?, field(self.props.len())?, field(props_len)?];
    for (i, v) in head.iter().enumerate() {
      out[start + 4 * i..start + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }
    Ok(())
  }
}

/// FissionOxide's Z-up turned into FBX's Y-up: (x, y, z) -> (x, z, -y).
fn y_up(v: [f32; 3]) -> [f64; 3] {
  [f64::from(v[0]), f64::from(v[2]), -f64::from(v[1])]
}

/// A triangle's normal, for meshes that came without normals.
fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
  let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
  let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
  let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
  if len > 0.0 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0, 0.0, 1.0] }
}

fn geometry(id: i64, m: &Mesh) -> Node {
  let vertices: Vec<f64> = m.positions.iter().flat_map(|p| y_up(*p)).collect();
  // a polygon's last corner is stored as -(index + 1)
  let corners: Vec<i32> =
    m.indices.chunks_exact(3).flat_map(|t| [t[0] as i32, t[1] as i32, -(t[2] as i32) - 1]).collect();
  let has_normals = m.normals.len() == m.positions.len();
  let normals: Vec<f64> = m
    .indices
    .chunks_exact(3)
    .flat_map(|t| {
      let at = |i: u32| m.positions[i as usize];
      let flat = face_normal(at(t[0]), at(t[1]), at(t[2]));
      t.iter().map(move |&i| if has_normals { m.normals[i as usize] } else { flat }).collect::<Vec<_>>()
    })
    .flat_map(y_up)
    .collect();
  let layer_element = |kind: &str| {
    parent("LayerElement", vec![], vec![node("Type", vec![s(kind)]), node("TypedIndex", vec![Prop::I32(0)])])
  };
  parent(
    "Geometry",
    vec![Prop::I64(id), object_name(&m.name, "Geometry"), s("Mesh")],
    vec![
      node("Properties70", vec![]),
      node("GeometryVersion", vec![Prop::I32(124)]),
      node("Vertices", vec![Prop::F64s(vertices)]),
      node("PolygonVertexIndex", vec![Prop::I32s(corners)]),
      parent(
        "LayerElementNormal",
        vec![Prop::I32(0)],
        vec![
          node("Version", vec![Prop::I32(102)]),
          node("Name", vec![s("")]),
          node("MappingInformationType", vec![s("ByPolygonVertex")]),
          node("ReferenceInformationType", vec![s("Direct")]),
          node("Normals", vec![Prop::F64s(normals)]),
        ],
      ),
      parent(
        "LayerElementMaterial",
        vec![Prop::I32(0)],
        vec![
          node("Version", vec![Prop::I32(101)]),
          node("Name", vec![s("")]),
          node("MappingInformationType", vec![s("AllSame")]),
          node("ReferenceInformationType", vec![s("IndexToDirect")]),
          node("Materials", vec![Prop::I32s(vec![0])]),
        ],
      ),
      parent(
        "Layer",
        vec![Prop::I32(0)],
        vec![
          node("Version", vec![Prop::I32(100)]),
          layer_element("LayerElementNormal"),
          layer_element("LayerElementMaterial"),
        ],
      ),
    ],
  )
}

fn model(id: i64, name: &str) -> Node {
  parent(
    "Model",
    vec![Prop::I64(id), object_name(name, "Model"), s("Mesh")],
    vec![
      node("Version", vec![Prop::I32(232)]),
      parent("Properties70", vec![], vec![p("DefaultAttributeIndex", "int", "Integer", vec![Prop::I32(0)])]),
      node("Shading", vec![Prop::Bool(true)]),
      node("Culling", vec![s("CullingOff")]),
    ],
  )
}

fn material(id: i64) -> Node {
  let rgb = |v: [f64; 3]| v.iter().map(|c| Prop::F64(*c)).collect::<Vec<_>>();
  parent(
    "Material",
    vec![Prop::I64(id), object_name("Default", "Material"), s("")],
    vec![
      node("Version", vec![Prop::I32(102)]),
      node("ShadingModel", vec![s("phong")]),
      node("MultiLayer", vec![Prop::I32(0)]),
      parent(
        "Properties70",
        vec![],
        vec![
          p("DiffuseColor", "Color", "", rgb([0.75, 0.77, 0.8])),
          p("SpecularColor", "Color", "", rgb([0.2, 0.2, 0.2])),
          p("Shininess", "double", "Number", vec![Prop::F64(20.0)]),
        ],
      ),
    ],
  )
}

fn header() -> Node {
  let stamp = [("Version", 1000), ("Year", 1970), ("Month", 1), ("Day", 1), ("Hour", 10), ("Minute", 0)]
    .into_iter()
    .chain([("Second", 0), ("Millisecond", 0)])
    .map(|(k, v)| node(k, vec![Prop::I32(v)]))
    .collect();
  parent(
    "FBXHeaderExtension",
    vec![],
    vec![
      node("FBXHeaderVersion", vec![Prop::I32(1003)]),
      node("FBXVersion", vec![Prop::I32(VERSION as i32)]),
      node("EncryptionType", vec![Prop::I32(0)]),
      parent("CreationTimeStamp", vec![], stamp),
      node("Creator", vec![s("FissionOxide")]),
    ],
  )
}

/// Y-up, X-right, Z-front (right-handed), in millimetres.
fn global_settings() -> Node {
  let int = |name: &str, v: i32| p(name, "int", "Integer", vec![Prop::I32(v)]);
  let double = |name: &str, v: f64| p(name, "double", "Number", vec![Prop::F64(v)]);
  parent(
    "GlobalSettings",
    vec![],
    vec![
      node("Version", vec![Prop::I32(1000)]),
      parent(
        "Properties70",
        vec![],
        vec![
          int("UpAxis", 1),
          int("UpAxisSign", 1),
          int("FrontAxis", 2),
          int("FrontAxisSign", 1),
          int("CoordAxis", 0),
          int("CoordAxisSign", 1),
          int("OriginalUpAxis", 1),
          int("OriginalUpAxisSign", 1),
          double("UnitScaleFactor", 0.1),
          double("OriginalUnitScaleFactor", 0.1),
        ],
      ),
    ],
  )
}

fn definitions(meshes: usize) -> Node {
  let kind = |name: &str, n: usize| parent("ObjectType", vec![s(name)], vec![node("Count", vec![Prop::I32(n as i32)])]);
  parent(
    "Definitions",
    vec![],
    vec![
      node("Version", vec![Prop::I32(100)]),
      node("Count", vec![Prop::I32((2 * meshes + 2) as i32)]),
      kind("GlobalSettings", 1),
      kind("Model", meshes),
      kind("Geometry", meshes),
      kind("Material", 1),
    ],
  )
}

/// Writes meshes as a binary FBX.
pub fn write_fbx(meshes: &[Mesh]) -> Result<Vec<u8>> {
  // object ids: the material, then a model and its geometry per mesh (0 is the scene's root)
  const MATERIAL: i64 = 1_000_000;
  let mut objects = vec![material(MATERIAL)];
  let mut links = Vec::new();
  let link = |child: i64, parent: i64| node("C", vec![s("OO"), Prop::I64(child), Prop::I64(parent)]);
  for (i, m) in meshes.iter().enumerate() {
    if m.positions.is_empty() || m.indices.is_empty() {
      return Err(ExchangeError::fmt(format!("mesh '{}' is empty", m.name)));
    }
    let (model_id, geometry_id) = (MATERIAL + 1 + 2 * i as i64, MATERIAL + 2 + 2 * i as i64);
    objects.push(geometry(geometry_id, m));
    objects.push(model(model_id, &m.name));
    links.push(link(model_id, 0));
    links.push(link(geometry_id, model_id));
    links.push(link(MATERIAL, model_id));
  }
  let top = [
    header(),
    node("FileId", vec![Prop::Raw(FILE_ID.to_vec())]),
    node("CreationTime", vec![s(TIME)]),
    node("Creator", vec![s("FissionOxide")]),
    global_settings(),
    definitions(meshes.len()),
    parent("Objects", vec![], objects),
    parent("Connections", vec![], links),
  ];
  let mut out = MAGIC.to_vec();
  out.extend_from_slice(&VERSION.to_le_bytes());
  for (i, n) in top.iter().enumerate() {
    n.write(&mut out, i + 1 == top.len())?;
  }
  out.extend_from_slice(&SENTINEL);
  // the footer: its id, padding to 16 bytes, the version, then a fixed block
  out.extend_from_slice(&FOOT_ID);
  out.extend_from_slice(&[0; 4]);
  let pad = match out.len() % 16 {
    0 => 16,
    r => 16 - r,
  };
  out.extend(std::iter::repeat_n(0u8, pad));
  out.extend_from_slice(&VERSION.to_le_bytes());
  out.extend_from_slice(&[0; 120]);
  out.extend_from_slice(&FOOT_MAGIC);
  Ok(out)
}
