//! Stanford PLY: ASCII, binary little- and big-endian.

use crate::{ExchangeError, Mesh, Result};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Encoding {
  Ascii,
  Little,
  Big,
}

#[derive(Debug, Clone, Copy)]
enum Ty {
  I8,
  U8,
  I16,
  U16,
  I32,
  U32,
  F32,
  F64,
}

impl Ty {
  fn parse(s: &str) -> Result<Ty> {
    Ok(match s {
      "char" | "int8" => Ty::I8,
      "uchar" | "uint8" => Ty::U8,
      "short" | "int16" => Ty::I16,
      "ushort" | "uint16" => Ty::U16,
      "int" | "int32" => Ty::I32,
      "uint" | "uint32" => Ty::U32,
      "float" | "float32" => Ty::F32,
      "double" | "float64" => Ty::F64,
      _ => return Err(ExchangeError::fmt(format!("PLY: unknown type '{s}'"))),
    })
  }

  fn size(self) -> usize {
    match self {
      Ty::I8 | Ty::U8 => 1,
      Ty::I16 | Ty::U16 => 2,
      Ty::I32 | Ty::U32 | Ty::F32 => 4,
      Ty::F64 => 8,
    }
  }
}

#[derive(Debug)]
struct Prop {
  name: String,
  ty: Ty,
  /// Count type for list properties.
  list: Option<Ty>,
}

#[derive(Debug)]
struct Element {
  name: String,
  count: usize,
  props: Vec<Prop>,
}

/// Reads value-by-value from ASCII tokens or binary bytes.
struct Cursor<'a> {
  enc: Encoding,
  bytes: &'a [u8],
  pos: usize,
  tokens: std::str::SplitAsciiWhitespace<'a>,
}

impl Cursor<'_> {
  fn read(&mut self, ty: Ty) -> Result<f64> {
    if self.enc == Encoding::Ascii {
      let t = self.tokens.next().ok_or_else(|| ExchangeError::fmt("PLY: unexpected end of data"))?;
      return t.parse::<f64>().map_err(|_| ExchangeError::fmt(format!("PLY: bad number '{t}'")));
    }
    let n = ty.size();
    let b = self.bytes.get(self.pos..self.pos + n).ok_or_else(|| ExchangeError::fmt("PLY: data is truncated"))?;
    self.pos += n;
    let mut a = [0u8; 8];
    a[..n].copy_from_slice(b);
    if self.enc == Encoding::Big {
      a[..n].reverse();
    }
    Ok(match ty {
      Ty::I8 => a[0] as i8 as f64,
      Ty::U8 => a[0] as f64,
      Ty::I16 => i16::from_le_bytes([a[0], a[1]]) as f64,
      Ty::U16 => u16::from_le_bytes([a[0], a[1]]) as f64,
      Ty::I32 => i32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
      Ty::U32 => u32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
      Ty::F32 => f32::from_le_bytes([a[0], a[1], a[2], a[3]]) as f64,
      Ty::F64 => f64::from_le_bytes(a),
    })
  }
}

pub fn read_ply(bytes: &[u8]) -> Result<Mesh> {
  let end = find(bytes, b"end_header").ok_or_else(|| ExchangeError::fmt("not a PLY file (no end_header)"))?;
  let header = String::from_utf8_lossy(&bytes[..end]);
  let mut data_start = end + b"end_header".len();
  // the header ends with "\n" or "\r\n"
  if bytes.get(data_start) == Some(&b'\r') {
    data_start += 1;
  }
  if bytes.get(data_start) == Some(&b'\n') {
    data_start += 1;
  }
  let mut lines = header.lines();
  if lines.next().map(str::trim) != Some("ply") {
    return Err(ExchangeError::fmt("not a PLY file"));
  }
  let mut enc = None;
  let mut elements: Vec<Element> = Vec::new();
  for line in lines {
    let t: Vec<&str> = line.split_whitespace().collect();
    match t.as_slice() {
      ["format", f, ..] => {
        enc = Some(match *f {
          "ascii" => Encoding::Ascii,
          "binary_little_endian" => Encoding::Little,
          "binary_big_endian" => Encoding::Big,
          _ => return Err(ExchangeError::fmt(format!("PLY: unknown format '{f}'"))),
        })
      }
      ["element", name, count] => elements.push(Element {
        name: name.to_string(),
        count: count.parse().map_err(|_| ExchangeError::fmt("PLY: bad element count"))?,
        props: Vec::new(),
      }),
      ["property", "list", ct, ty, name] => {
        if let Some(e) = elements.last_mut() {
          e.props.push(Prop { name: name.to_string(), ty: Ty::parse(ty)?, list: Some(Ty::parse(ct)?) });
        }
      }
      ["property", ty, name] => {
        if let Some(e) = elements.last_mut() {
          e.props.push(Prop { name: name.to_string(), ty: Ty::parse(ty)?, list: None });
        }
      }
      _ => {}
    }
  }
  let enc = enc.ok_or_else(|| ExchangeError::fmt("PLY: missing format line"))?;
  let data = bytes.get(data_start..).unwrap_or_default();
  let ascii = if enc == Encoding::Ascii { String::from_utf8_lossy(data).into_owned() } else { String::new() };
  let mut cur = Cursor { enc, bytes: data, pos: 0, tokens: ascii.split_ascii_whitespace() };
  let mut m = Mesh { name: "PLY".into(), ..Mesh::default() };
  for e in &elements {
    // guard against absurd counts before allocating
    if e.count > 100_000_000 {
      return Err(ExchangeError::fmt("PLY: element count too large"));
    }
    for _ in 0..e.count {
      let mut xyz = [0f32; 3];
      for p in &e.props {
        match p.list {
          Some(ct) => {
            let n = cur.read(ct)? as usize;
            let mut poly = Vec::with_capacity(n);
            for _ in 0..n {
              poly.push(cur.read(p.ty)? as u32);
            }
            if e.name == "face" && (p.name == "vertex_indices" || p.name == "vertex_index") {
              for k in 1..poly.len().saturating_sub(1) {
                m.indices.extend_from_slice(&[poly[0], poly[k], poly[k + 1]]);
              }
            }
          }
          None => {
            let v = cur.read(p.ty)?;
            if e.name == "vertex" {
              match p.name.as_str() {
                "x" => xyz[0] = v as f32,
                "y" => xyz[1] = v as f32,
                "z" => xyz[2] = v as f32,
                _ => {}
              }
            }
          }
        }
      }
      if e.name == "vertex" {
        m.positions.push(xyz);
      }
    }
  }
  if m.indices.is_empty() {
    return Err(ExchangeError::fmt("the PLY file has no faces"));
  }
  m.validate().map(|m| m.welded())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
  hay.windows(needle.len()).position(|w| w == needle)
}

/// Binary little-endian PLY with all meshes merged.
pub fn write_ply(meshes: &[Mesh]) -> Vec<u8> {
  let nv: usize = meshes.iter().map(|m| m.positions.len()).sum();
  let nf: usize = meshes.iter().map(Mesh::triangle_count).sum();
  let header = format!(
    "ply\nformat binary_little_endian 1.0\ncomment CLFusion export (mm)\nelement vertex {nv}\nproperty float x\nproperty float y\nproperty float z\nelement face {nf}\nproperty list uchar uint vertex_indices\nend_header\n"
  );
  let mut out = header.into_bytes();
  for m in meshes {
    for p in &m.positions {
      for c in p {
        out.extend_from_slice(&c.to_le_bytes());
      }
    }
  }
  let mut base = 0u32;
  for m in meshes {
    for t in m.indices.chunks_exact(3) {
      out.push(3);
      for i in t {
        out.extend_from_slice(&(i + base).to_le_bytes());
      }
    }
    base += m.positions.len() as u32;
  }
  out
}
