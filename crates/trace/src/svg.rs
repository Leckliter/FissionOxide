//! SVG output, for previews and for other tools.
//!
//! The viewBox is the traced image in pixels (y flipped back to point down). Each non-background
//! layer is a `<g>` filled with its colour; each shape (an outer boundary and the holes that
//! follow it) is one `<path>` of closed subpaths with `fill-rule="evenodd"`. Background layers
//! are left out.

use crate::fit::arc_sweep;
use crate::{Contour, Seg, Trace};
use std::f64::consts::PI;

/// A coordinate with at most three decimals and no trailing zeros.
fn num(v: f64) -> String {
  let s = format!("{v:.3}");
  let s = s.trim_end_matches('0').trim_end_matches('.');
  if s == "-0" { "0".into() } else { s.into() }
}

impl Trace {
  /// The trace as an SVG document.
  pub fn to_svg(&self) -> String {
    let (w, h) = (self.width, self.height);
    let mut out =
      format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n");
    for layer in self.layers.iter().filter(|l| !l.background) {
      let [r, g, b] = layer.color;
      out.push_str(&format!("  <g fill=\"#{r:02x}{g:02x}{b:02x}\" fill-rule=\"evenodd\">\n"));
      let mut shape = String::new();
      for c in &layer.contours {
        if !c.hole && !shape.is_empty() {
          out.push_str(&format!("    <path d=\"{shape}\"/>\n"));
          shape.clear();
        }
        if !shape.is_empty() {
          shape.push(' ');
        }
        path(c, f64::from(h), &mut shape);
      }
      if !shape.is_empty() {
        out.push_str(&format!("    <path d=\"{shape}\"/>\n"));
      }
      out.push_str("  </g>\n");
    }
    out.push_str("</svg>\n");
    out
  }
}

/// One closed subpath.
fn path(c: &Contour, h: f64, out: &mut String) {
  let Some(first) = c.segs.first() else { return };
  let pt = |p: [f64; 2]| format!("{} {}", num(p[0]), num(h - p[1]));
  out.push_str(&format!("M {}", pt(first.start())));
  for seg in &c.segs {
    match *seg {
      Seg::Line { b, .. } => out.push_str(&format!(" L {}", pt(b))),
      Seg::Arc { a, b, center, ccw } => {
        let r = num(((a[0] - center[0]).hypot(a[1] - center[1])).max(1e-9));
        let large = u8::from(arc_sweep(a, b, center, ccw) > PI);
        // flipping y turns counter-clockwise into SVG's negative sweep
        let sweep = u8::from(!ccw);
        out.push_str(&format!(" A {r} {r} 0 {large} {sweep} {}", pt(b)));
      }
      Seg::Cubic { c1, c2, b, .. } => out.push_str(&format!(" C {} {} {}", pt(c1), pt(c2), pt(b))),
      Seg::Circle { center, radius } => {
        let r = num(radius);
        let sweep = u8::from(c.hole);
        let far = [center[0] - radius, center[1]];
        out.push_str(&format!(" A {r} {r} 0 0 {sweep} {} A {r} {r} 0 0 {sweep} {}", pt(far), pt(seg.end())));
      }
    }
  }
  out.push_str(" Z");
}
