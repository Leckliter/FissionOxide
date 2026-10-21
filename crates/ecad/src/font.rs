//! A small stroke font for silkscreen text (reference designators): each glyph is a set of
//! strokes between the points of a 2×4 grid, like a segment display.

use crate::geom::P;

/// The grid points a segment letter joins.
fn segment(c: u8) -> Option<(P, P)> {
  const TL: P = [0.0, 4.0];
  const TM: P = [1.0, 4.0];
  const TR: P = [2.0, 4.0];
  const ML: P = [0.0, 2.0];
  const MM: P = [1.0, 2.0];
  const MR: P = [2.0, 2.0];
  const BL: P = [0.0, 0.0];
  const BM: P = [1.0, 0.0];
  const BR: P = [2.0, 0.0];
  Some(match c {
    b'a' => (TL, TM),
    b'b' => (TM, TR),
    b'c' => (TR, MR),
    b'd' => (MR, BR),
    b'e' => (BR, BM),
    b'f' => (BM, BL),
    b'g' => (BL, ML),
    b'h' => (ML, TL),
    b'i' => (ML, MM),
    b'j' => (MM, MR),
    b'k' => (TL, MM),
    b'l' => (TM, MM),
    b'm' => (TR, MM),
    b'n' => (MM, BL),
    b'o' => (MM, BM),
    b'p' => (MM, BR),
    b'q' => (ML, BM),
    b'r' => (MR, BM),
    b's' => (TM, MR),
    _ => return None,
  })
}

fn glyph(ch: char) -> &'static str {
  match ch.to_ascii_uppercase() {
    '0' => "abcdefghmn",
    '1' => "lofek",
    '2' => "abcijgfe",
    '3' => "abcdefj",
    '4' => "hijcd",
    '5' | 'S' => "abhijdef",
    '6' => "abhgijdef",
    '7' => "abcd",
    '8' => "abcdefghij",
    '9' => "abhcijdef",
    'A' => "abhgcdij",
    'B' => "abcdefloj",
    'C' => "abhgfe",
    'D' => "ahgfsr",
    'E' => "abhgfei",
    'F' => "abhgi",
    'G' => "abhgfedj",
    'H' => "hgcdij",
    'I' => "ablofe",
    'J' => "cdefg",
    'K' => "hgimp",
    'L' => "hgfe",
    'M' => "hgcdkm",
    'N' => "hgcdkp",
    'O' => "abcdefgh",
    'P' => "abhgcij",
    'Q' => "abcdefghp",
    'R' => "abhgcijp",
    'T' => "ablo",
    'U' => "hgcdfe",
    'V' => "hqcr",
    'W' => "hgcdnp",
    'X' => "kmnp",
    'Y' => "kmo",
    'Z' => "abmnfe",
    '-' => "ij",
    '+' => "ijlo",
    '_' => "fe",
    '/' => "mn",
    '$' => "abhijdeflo",
    '.' => "e",
    _ => "",
  }
}

/// Strokes for `text`, `size` tall, starting at the origin (left baseline), as line segments.
pub fn strokes(text: &str, size: f64) -> Vec<(P, P)> {
  let u = size / 4.0;
  let mut out = Vec::new();
  for (i, ch) in text.chars().enumerate() {
    let x0 = i as f64 * 3.0;
    for s in glyph(ch).bytes() {
      if let Some((a, b)) = segment(s) {
        out.push(([(a[0] + x0) * u, a[1] * u], [(b[0] + x0) * u, b[1] * u]));
      }
    }
  }
  out
}

/// The width of `text` set `size` tall.
pub fn width(text: &str, size: f64) -> f64 {
  let n = text.chars().count() as f64;
  if n == 0.0 { 0.0 } else { (n * 3.0 - 1.0) * size / 4.0 }
}
