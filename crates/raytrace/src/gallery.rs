//! The render gallery: finished renders kept in the design (as PNGs inside its JSON).

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One render in the gallery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GalleryImage {
  pub id: u32,
  pub name: String,
  pub width: u32,
  pub height: u32,
  pub samples: u32,
  /// Render time (seconds).
  #[serde(default)]
  pub seconds: f64,
  /// When it was made (seconds since 1970).
  #[serde(default)]
  pub created: u64,
  pub png: Png,
}

/// PNG bytes, shared between undo steps (the design is copied on every edit) and written into
/// the design JSON as base64.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Png(pub Arc<Vec<u8>>);

impl Serialize for Png {
  fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&base64_encode(&self.0))
  }
}

impl<'de> Deserialize<'de> for Png {
  fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Png, D::Error> {
    let s = String::deserialize(d)?;
    base64_decode(&s).map(|v| Png(Arc::new(v))).ok_or_else(|| serde::de::Error::custom("bad base64 PNG"))
  }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
  let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
  for c in data.chunks(3) {
    let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
    let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
    for k in 0..4 {
      if k <= c.len() {
        out.push(ALPHABET[(n >> (18 - 6 * k) & 63) as usize] as char);
      } else {
        out.push('=');
      }
    }
  }
  out
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
  let mut out = Vec::with_capacity(s.len() / 4 * 3);
  let (mut acc, mut bits) = (0u32, 0);
  for ch in s.bytes() {
    let v = match ch {
      b'A'..=b'Z' => ch - b'A',
      b'a'..=b'z' => ch - b'a' + 26,
      b'0'..=b'9' => ch - b'0' + 52,
      b'+' | b'-' => 62,
      b'/' | b'_' => 63,
      b'=' => break,
      b'\n' | b'\r' | b' ' => continue,
      _ => return None,
    };
    acc = acc << 6 | v as u32;
    bits += 6;
    if bits >= 8 {
      bits -= 8;
      out.push((acc >> bits) as u8);
    }
  }
  Some(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn base64_round_trips() {
    for n in 0..20 {
      let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
      assert_eq!(base64_decode(&base64_encode(&data)).unwrap(), data);
    }
    assert_eq!(base64_encode(b"Man"), "TWFu");
    assert_eq!(base64_encode(b"Ma"), "TWE=");
  }
}
