//! SVG pictures of the schematic and the board, for previews, docs and agents.

use std::fmt::Write;

use crate::board::{Layer, Side};
use crate::geom::{P, Xf, add, rot};
use crate::library::{FpLayer, Gfx};
use crate::project::Project;

struct Svg {
  body: String,
  lo: P,
  hi: P,
}

impl Svg {
  fn new() -> Svg {
    Svg { body: String::new(), lo: [f64::INFINITY; 2], hi: [f64::NEG_INFINITY; 2] }
  }

  fn grow(&mut self, p: P) {
    self.lo = [self.lo[0].min(p[0]), self.lo[1].min(p[1])];
    self.hi = [self.hi[0].max(p[0]), self.hi[1].max(p[1])];
  }

  fn poly(&mut self, pts: &[P], stroke: &str, width: f64, fill: Option<&str>, opacity: f64) {
    if pts.is_empty() {
      return;
    }
    let mut d = String::new();
    for p in pts {
      self.grow(*p);
      let _ = write!(d, "{:.3},{:.3} ", p[0], -p[1]);
    }
    let tag = if fill.is_some() { "polygon" } else { "polyline" };
    let _ = writeln!(
      self.body,
      r#"<{tag} points="{d}" fill="{}" stroke="{stroke}" stroke-width="{width:.3}" stroke-linecap="round" stroke-linejoin="round" opacity="{opacity}"/>"#,
      fill.unwrap_or("none")
    );
  }

  fn circle(&mut self, c: P, r: f64, stroke: &str, width: f64, fill: Option<&str>) {
    self.grow([c[0] - r, c[1] - r]);
    self.grow([c[0] + r, c[1] + r]);
    let _ = writeln!(
      self.body,
      r#"<circle cx="{:.3}" cy="{:.3}" r="{r:.3}" fill="{}" stroke="{stroke}" stroke-width="{width:.3}"/>"#,
      c[0],
      -c[1],
      fill.unwrap_or("none")
    );
  }

  fn text(&mut self, at: P, s: &str, size: f64, color: &str, anchor: &str) {
    self.grow(at);
    let esc = s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let _ = writeln!(
      self.body,
      r#"<text x="{:.3}" y="{:.3}" font-size="{size:.2}" font-family="sans-serif" fill="{color}" text-anchor="{anchor}">{esc}</text>"#,
      at[0], -at[1]
    );
  }

  fn finish(self, background: &str) -> String {
    let (lo, hi) = if self.lo[0].is_finite() { (self.lo, self.hi) } else { ([0.0; 2], [10.0; 2]) };
    let m = 3.0;
    let (w, h) = (hi[0] - lo[0] + 2.0 * m, hi[1] - lo[1] + 2.0 * m);
    format!(
      "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{:.3} {:.3} {w:.3} {h:.3}\" width=\"{:.0}mm\" height=\"{:.0}mm\">\n<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{w:.3}\" height=\"{h:.3}\" fill=\"{background}\"/>\n{}</svg>\n",
      lo[0] - m,
      -hi[1] - m,
      w,
      h,
      lo[0] - m,
      -hi[1] - m,
      self.body
    )
  }
}

fn gfx(svg: &mut Svg, g: &Gfx, xf: &Xf, color: &str, fill: &str) {
  match g {
    Gfx::Text { at, text, size } => svg.text(xf.apply(*at), text, *size, color, "middle"),
    Gfx::Circle { c, r, width, fill: f } => svg.circle(xf.apply(*c), *r, color, width.max(0.15), f.then_some(fill)),
    _ => {
      let filled = matches!(g, Gfx::Rect { fill: true, .. } | Gfx::Poly { fill: true, .. });
      for line in g.polylines() {
        let pts: Vec<P> = line.iter().map(|p| xf.apply(*p)).collect();
        svg.poly(&pts, color, g.width().max(0.15), filled.then_some(fill), 1.0);
      }
    }
  }
}

/// The schematic: symbols with their pins, names and values; wires, junctions, labels, buses.
pub fn schematic(pr: &Project) -> String {
  let libs = pr.libs();
  let mut svg = Svg::new();
  let (ink, pin_c, wire_c) = ("#8b1a1a", "#8b1a1a", "#1a7a1a");
  for part in &pr.schematic.parts {
    let Some(dev) = libs.device(&part.device) else { continue };
    let Some(sym) = libs.symbol(&dev.symbol) else { continue };
    let xf = part.xf();
    for g in &sym.gfx {
      gfx(&mut svg, g, &xf, ink, "#f5e6c8");
    }
    for p in sym.pins.iter().filter(|p| !p.hidden) {
      let a = xf.apply(p.at);
      let b = xf.apply(add(p.at, rot([p.length, 0.0], p.dir)));
      svg.poly(&[a, b], pin_c, 0.2, None, 1.0);
      if !p.name.is_empty() && !sym.power && p.name != "~" && sym.pins.len() > 2 {
        let inward = xf.angle(p.dir);
        let at = add(b, rot([0.6, 0.0], inward));
        let anchor = if inward.cos() < -0.5 {
          "end"
        } else if inward.cos() > 0.5 {
          "start"
        } else {
          "middle"
        };
        svg.text([at[0], at[1] - 0.4], &p.name, 1.1, "#333", anchor);
      }
    }
    let (lo, hi) =
      crate::geom::bounds(sym.gfx.iter().flat_map(|g| g.polylines().into_iter().flatten()).map(|p| xf.apply(p)))
        .unwrap_or((part.at, part.at));
    if sym.power {
      let ground = sym.gfx.iter().flat_map(|g| g.polylines().into_iter().flatten()).any(|p| p[1] < -0.5);
      let at = xf.apply(if ground { [0.0, -3.6] } else { [0.0, 2.2] });
      svg.text([at[0], at[1] - 0.4], &part.value, 1.3, "#333", "middle");
    } else {
      svg.text([hi[0] + 0.8, (lo[1] + hi[1]) / 2.0 + 0.8], &part.refdes, 1.5, "#333", "start");
      svg.text([hi[0] + 0.8, (lo[1] + hi[1]) / 2.0 - 1.2], &part.value, 1.3, "#555", "start");
    }
  }
  for w in &pr.schematic.wires {
    svg.poly(&[w.a, w.b], wire_c, 0.25, None, 1.0);
  }
  // junctions: wire ends on three or more wire ends or on another wire's middle
  for w in &pr.schematic.wires {
    for e in [w.a, w.b] {
      let touching = pr.schematic.wires.iter().filter(|o| crate::geom::point_seg_dist(e, o.a, o.b) < 1e-4).count();
      if touching >= 3
        || pr.schematic.wires.iter().any(|o| {
          crate::geom::point_seg_dist(e, o.a, o.b) < 1e-4 && !crate::geom::near(e, o.a) && !crate::geom::near(e, o.b)
        })
      {
        svg.circle(e, 0.5, wire_c, 0.0, Some(wire_c));
      }
    }
  }
  for l in &pr.schematic.labels {
    // the text runs away from the wire it ends
    let from = pr.schematic.wires.iter().find_map(|w| {
      if crate::geom::near(w.a, l.at) {
        Some(w.b)
      } else if crate::geom::near(w.b, l.at) {
        Some(w.a)
      } else {
        None
      }
    });
    let left = from.is_some_and(|f| f[0] > l.at[0] + 1e-6);
    let x = if left { l.at[0] - 0.3 } else { l.at[0] + 0.3 };
    svg.text([x, l.at[1] + 0.4], &l.name, 1.4, "#1a4a8b", if left { "end" } else { "start" });
  }
  for b in &pr.schematic.buses {
    svg.poly(&b.pts, "#1a3a9b", 0.6, None, 1.0);
    if let Some(p) = b.pts.first() {
      svg.text([p[0], p[1] + 0.8], &b.name, 1.4, "#1a3a9b", "start");
    }
  }
  for nc in &pr.schematic.no_connects {
    let d = 0.7;
    svg.poly(&[[nc[0] - d, nc[1] - d], [nc[0] + d, nc[1] + d]], "#1a1a9b", 0.2, None, 1.0);
    svg.poly(&[[nc[0] - d, nc[1] + d], [nc[0] + d, nc[1] - d]], "#1a1a9b", 0.2, None, 1.0);
  }
  svg.finish("#fffdf5")
}

fn layer_color(l: Layer) -> &'static str {
  match l {
    Layer::Top => "#c83232",
    Layer::Inner1 => "#c8c832",
    Layer::Inner2 => "#c832c8",
    Layer::Bottom => "#3264c8",
  }
}

/// The board seen from above: outline, pours, tracks, pads, vias, silkscreen and ratsnest.
pub fn board(pr: &Project) -> String {
  let b = &pr.board;
  let pads = pr.pads();
  let libs = pr.libs();
  let mut svg = Svg::new();
  let mut o = b.outline.clone();
  if let Some(f) = o.first().copied() {
    o.push(f);
  }
  svg.poly(&b.outline, "#d0d0a0", 0.15, Some("#103a10"), 1.0);
  let mut layers = b.copper_layers();
  layers.reverse();
  for l in &layers {
    for p in b.pours.iter().filter(|p| p.layer == *l) {
      svg.poly(&p.outline, layer_color(*l), 0.1, Some(layer_color(*l)), 0.25);
    }
    for t in b.tracks.iter().filter(|t| t.layer == *l) {
      svg.poly(&[t.a, t.b], layer_color(*l), t.width, None, 0.85);
    }
  }
  for p in &pads {
    let color = if p.drill.is_some() {
      "#c8a832"
    } else if p.side == Side::Top {
      "#c83232"
    } else {
      "#3264c8"
    };
    match p.shape2(0.0) {
      crate::geom::Shape2::Poly(pts) => svg.poly(&pts, color, 0.01, Some(color), 1.0),
      crate::geom::Shape2::Capsule { a, b, r } => {
        if crate::geom::near(a, b) {
          svg.circle(a, r, color, 0.0, Some(color));
        } else {
          svg.poly(&[a, b], color, 2.0 * r, None, 1.0);
        }
      }
    }
    if let Some(d) = p.drill {
      svg.circle(p.at, d / 2.0, "#000", 0.0, Some(if p.npth { "#000" } else { "#222" }));
    }
  }
  for v in &b.vias {
    svg.circle(v.at, v.diameter / 2.0, "#c8a832", 0.0, Some("#c8a832"));
    svg.circle(v.at, v.drill / 2.0, "#000", 0.0, Some("#222"));
  }
  for h in &b.holes {
    svg.circle(h.at, h.drill / 2.0, "#888", 0.1, Some("#000"));
  }
  for (w, pts) in crate::gerber::silk_strokes(pr, Side::Top) {
    if pts.len() == 2 && crate::geom::near(pts[0], pts[1]) {
      svg.circle(pts[0], w / 2.0, "#eee", 0.0, Some("#eee"));
    } else {
      svg.poly(&pts, "#eeeeee", w, None, 0.9);
    }
  }
  // parts off the board still show where they are
  for pl in &b.placements {
    let Some(part) = pr.schematic.parts.iter().find(|p| p.refdes == pl.refdes) else { continue };
    let Some(fp) = libs
      .device(&part.device)
      .and_then(|d| libs.footprint(if pl.footprint.is_empty() { &d.footprint } else { &pl.footprint }))
    else {
      continue;
    };
    let xf = pl.xf();
    for g in fp.gfx.iter().filter(|g| g.layer == FpLayer::Courtyard) {
      gfx(&mut svg, &g.gfx, &xf, "#808080", "none");
    }
  }
  for a in b.ratsnest(&pads) {
    svg.poly(&[a.a, a.b], "#f0f0f0", 0.08, None, 0.9);
  }
  svg.finish("#202020")
}
