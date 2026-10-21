//! Fab output: Gerber RS-274X with X2 file attributes for copper, mask, paste, silkscreen and
//! the board outline; Excellon drill files (plated and unplated); BOM and pick-and-place CSV.

use std::fmt::Write;

use crate::board::{BoardPad, Layer, Side};
use crate::geom::{P, add};
use crate::library::{FpLayer, Gfx, PadShape};
use crate::project::Project;
use crate::schematic::natural;

/// One Gerber file being built.
struct Gb {
  apertures: Vec<String>,
  body: String,
  current: Option<usize>,
  dark: bool,
}

fn c(v: f64) -> i64 {
  (v * 1e6).round() as i64
}

fn n(v: f64) -> String {
  let s = format!("{v:.4}");
  let s = s.trim_end_matches('0').trim_end_matches('.');
  if s.is_empty() || s == "-0" { "0".into() } else { s.to_string() }
}

impl Gb {
  fn new() -> Gb {
    Gb { apertures: Vec::new(), body: String::new(), current: None, dark: true }
  }

  fn select(&mut self, def: String) {
    let i = match self.apertures.iter().position(|a| *a == def) {
      Some(i) => i,
      None => {
        self.apertures.push(def);
        self.apertures.len() - 1
      }
    };
    if self.current != Some(i) {
      let _ = writeln!(self.body, "D{}*", i + 10);
      self.current = Some(i);
    }
  }

  fn polarity(&mut self, dark: bool) {
    if self.dark != dark {
      self.body.push_str(if dark { "%LPD*%\n" } else { "%LPC*%\n" });
      self.dark = dark;
    }
  }

  fn flash(&mut self, def: String, at: P) {
    self.select(def);
    let _ = writeln!(self.body, "X{}Y{}D03*", c(at[0]), c(at[1]));
  }

  /// A polyline drawn with a round aperture.
  fn stroke(&mut self, width: f64, pts: &[P]) {
    if pts.len() < 2 {
      return;
    }
    self.select(format!("C,{}", n(width.max(0.01))));
    let _ = writeln!(self.body, "X{}Y{}D02*", c(pts[0][0]), c(pts[0][1]));
    for p in &pts[1..] {
      let _ = writeln!(self.body, "X{}Y{}D01*", c(p[0]), c(p[1]));
    }
  }

  fn region(&mut self, pts: &[P]) {
    if pts.len() < 3 {
      return;
    }
    self.body.push_str("G36*\n");
    let _ = writeln!(self.body, "X{}Y{}D02*", c(pts[0][0]), c(pts[0][1]));
    for p in pts[1..].iter().chain(std::iter::once(&pts[0])) {
      let _ = writeln!(self.body, "X{}Y{}D01*", c(p[0]), c(p[1]));
    }
    self.body.push_str("G37*\n");
  }

  /// A pad, grown by `grow` on every side: a standard aperture when it sits square, else a
  /// region (or a stroke for a rotated obround).
  fn pad(&mut self, p: &BoardPad, grow: f64) {
    let size = [p.size[0] + 2.0 * grow, p.size[1] + 2.0 * grow];
    let r = p.rot.rem_euclid(180.0);
    let square = r.abs() < 1e-6 || (r - 90.0).abs() < 1e-6;
    let (w, h) = if (r - 90.0).abs() < 1e-6 { (size[1], size[0]) } else { (size[0], size[1]) };
    match p.shape {
      PadShape::Circle => self.flash(format!("C,{}", n(size[0])), p.at),
      PadShape::Rect | PadShape::RoundRect if square => self.flash(format!("R,{}X{}", n(w), n(h)), p.at),
      PadShape::Oval if square => self.flash(format!("O,{}X{}", n(w), n(h)), p.at),
      PadShape::Rect | PadShape::RoundRect => {
        if let crate::geom::Shape2::Poly(pts) = p.shape2(grow) {
          self.region(&pts);
        }
      }
      PadShape::Oval => match p.shape2(grow) {
        crate::geom::Shape2::Capsule { a, b, r } => self.stroke(2.0 * r, &[a, b]),
        crate::geom::Shape2::Poly(pts) => self.region(&pts),
      },
    }
  }

  fn finish(self, function: &str, polarity: &str) -> String {
    let mut s = String::new();
    s.push_str("%TF.GenerationSoftware,FissionOxide,fox-ecad,0.1*%\n");
    let _ = writeln!(s, "%TF.FileFunction,{function}*%");
    let _ = writeln!(s, "%TF.FilePolarity,{polarity}*%");
    s.push_str("%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\nG01*\n");
    for (i, a) in self.apertures.iter().enumerate() {
      let _ = writeln!(s, "%ADD{}{a}*%", i + 10);
    }
    s.push_str(&self.body);
    if !self.dark {
      s.push_str("%LPD*%\n");
    }
    s.push_str("M02*\n");
    s
  }
}

fn layer_function(l: Layer, n: u8) -> String {
  match l {
    Layer::Top => "Copper,L1,Top".into(),
    Layer::Inner1 => "Copper,L2,Inr".into(),
    Layer::Inner2 => "Copper,L3,Inr".into(),
    Layer::Bottom => format!("Copper,L{},Bot", if n >= 4 { 4 } else { 2 }),
  }
}

fn layer_file(l: Layer) -> &'static str {
  match l {
    Layer::Top => "F_Cu",
    Layer::Inner1 => "In1_Cu",
    Layer::Inner2 => "In2_Cu",
    Layer::Bottom => "B_Cu",
  }
}

/// A copper layer: pours with their clearances cut, then tracks, pads and vias.
fn copper(pr: &Project, pads: &[BoardPad], layer: Layer) -> String {
  let b = &pr.board;
  let r = &b.rules;
  let mut g = Gb::new();
  for pour in b.pours.iter().filter(|p| p.layer == layer && p.outline.len() >= 3) {
    let cl = pour.clearance.unwrap_or(r.clearance);
    g.polarity(true);
    g.region(&pour.outline);
    g.polarity(false);
    for t in b.tracks.iter().filter(|t| t.layer == layer && t.net != pour.net) {
      g.stroke(t.width + 2.0 * cl, &[t.a, t.b]);
    }
    for p in pads.iter().filter(|p| p.layers.contains(&layer) && (p.net != pour.net || p.npth || p.net.is_empty())) {
      if p.npth {
        g.flash(format!("C,{}", n(p.drill.unwrap_or(p.size[0]) + 2.0 * cl)), p.at);
      } else {
        g.pad(p, cl);
      }
    }
    for v in b.vias.iter().filter(|v| v.net != pour.net) {
      g.flash(format!("C,{}", n(v.diameter + 2.0 * cl)), v.at);
    }
    for h in &b.holes {
      g.flash(format!("C,{}", n(h.drill + 2.0 * cl)), h.at);
    }
    if b.outline.len() >= 3 {
      let mut o = b.outline.clone();
      o.push(o[0]);
      g.stroke(2.0 * r.edge_clearance, &o);
    }
  }
  g.polarity(true);
  for t in b.tracks.iter().filter(|t| t.layer == layer) {
    g.stroke(t.width, &[t.a, t.b]);
  }
  for p in pads.iter().filter(|p| p.layers.contains(&layer) && !p.npth) {
    g.pad(p, 0.0);
  }
  for v in &b.vias {
    g.flash(format!("C,{}", n(v.diameter)), v.at);
  }
  g.finish(&layer_function(layer, b.layers), "Positive")
}

fn mask(pr: &Project, pads: &[BoardPad], side: Side) -> String {
  let mut g = Gb::new();
  let e = pr.board.rules.mask_expansion;
  for p in pads.iter().filter(|p| p.layers.contains(&side.layer()) || p.npth) {
    if p.npth {
      g.flash(format!("C,{}", n(p.drill.unwrap_or(p.size[0]) + 2.0 * e)), p.at);
    } else {
      g.pad(p, e);
    }
  }
  for h in &pr.board.holes {
    g.flash(format!("C,{}", n(h.drill + 2.0 * e)), h.at);
  }
  g.finish(if side == Side::Top { "Soldermask,Top" } else { "Soldermask,Bot" }, "Negative")
}

fn paste(pads: &[BoardPad], side: Side) -> String {
  let mut g = Gb::new();
  for p in pads.iter().filter(|p| p.drill.is_none() && p.side == side) {
    g.pad(p, 0.0);
  }
  g.finish(if side == Side::Top { "Paste,Top" } else { "Paste,Bot" }, "Positive")
}

/// Silkscreen strokes for one side: footprint artwork and reference designators.
pub fn silk_strokes(pr: &Project, side: Side) -> Vec<(f64, Vec<P>)> {
  let libs = pr.libs();
  let mut out = Vec::new();
  for pl in pr.board.placements.iter().filter(|p| p.side == side) {
    let Some(part) = pr.schematic.parts.iter().find(|p| p.refdes == pl.refdes) else { continue };
    let Some(dev) = libs.device(&part.device) else { continue };
    let Some(fp) = libs.footprint(if pl.footprint.is_empty() { &dev.footprint } else { &pl.footprint }) else {
      continue;
    };
    let xf = pl.xf();
    for g in fp.gfx.iter().filter(|g| g.layer == FpLayer::Silk) {
      let w = g.gfx.width().max(0.12);
      if let Gfx::Circle { c, r, fill: true, .. } = g.gfx {
        out.push((2.0 * r, vec![xf.apply(c), xf.apply(c)]));
        continue;
      }
      for line in g.gfx.polylines() {
        out.push((w, line.iter().map(|p| xf.apply(*p)).collect()));
      }
    }
    // the reference designator above the courtyard
    let (lo, hi) = fp.courtyard();
    let size = 1.0;
    let x0 = (lo[0] + hi[0]) / 2.0 - crate::font::width(&pl.refdes, size) / 2.0;
    let base = [x0, hi[1] + 0.3];
    // upside-down text is turned round about its middle so it reads from the board's bottom edge
    let flip = {
      let r = pl.rot.rem_euclid(360.0);
      r > 90.0 + 1e-9 && r <= 270.0 + 1e-9
    };
    let mid = [(lo[0] + hi[0]) / 2.0, hi[1] + 0.3 + size / 2.0];
    let turn = |p: P| if flip { [2.0 * mid[0] - p[0], 2.0 * mid[1] - p[1]] } else { p };
    for (a, b) in crate::font::strokes(&pl.refdes, size) {
      out.push((0.15, vec![xf.apply(turn(add(a, base))), xf.apply(turn(add(b, base)))]));
    }
  }
  out
}

fn silk(pr: &Project, side: Side) -> String {
  let mut g = Gb::new();
  for (w, pts) in silk_strokes(pr, side) {
    if pts.len() == 2 && crate::geom::near(pts[0], pts[1]) {
      g.flash(format!("C,{}", n(w)), pts[0]);
    } else {
      g.stroke(w, &pts);
    }
  }
  g.finish(if side == Side::Top { "Legend,Top" } else { "Legend,Bot" }, "Positive")
}

fn edge(pr: &Project) -> String {
  let mut g = Gb::new();
  let mut o = pr.board.outline.clone();
  if let Some(f) = o.first().copied() {
    o.push(f);
  }
  g.stroke(0.1, &o);
  g.finish("Profile,NP", "Positive")
}

/// An Excellon drill file for plated or unplated holes.
pub fn drill(pr: &Project, pads: &[BoardPad], plated: bool) -> String {
  let holes: Vec<(P, f64)> = pr.board.drills(pads).into_iter().filter(|h| h.2 == plated).map(|h| (h.0, h.1)).collect();
  let mut sizes: Vec<f64> = holes.iter().map(|h| (h.1 * 1000.0).round() / 1000.0).collect();
  sizes.sort_by(f64::total_cmp);
  sizes.dedup();
  let mut s = String::from("M48\n");
  let _ = writeln!(s, "; DRILL file {{FissionOxide fox-ecad}}");
  let _ = writeln!(s, "; FORMAT={{-:-/ absolute / metric / decimal}}");
  let _ = writeln!(
    s,
    "; #@! TF.FileFunction,{},1,{},{}",
    if plated { "Plated" } else { "NonPlated" },
    pr.board.layers.max(2),
    if plated { "PTH" } else { "NPTH" }
  );
  s.push_str("FMAT,2\nMETRIC\n");
  for (i, d) in sizes.iter().enumerate() {
    let _ = writeln!(s, "T{}C{d:.3}", i + 1);
  }
  s.push_str("%\nG90\nG05\n");
  for (i, d) in sizes.iter().enumerate() {
    let _ = writeln!(s, "T{}", i + 1);
    for h in holes.iter().filter(|h| ((h.1 * 1000.0).round() / 1000.0 - d).abs() < 1e-9) {
      let _ = writeln!(s, "X{:.3}Y{:.3}", h.0[0], h.0[1]);
    }
  }
  s.push_str("M30\n");
  s
}

fn csv(field: &str) -> String {
  if field.contains([',', '"', '\n']) { format!("\"{}\"", field.replace('"', "\"\"")) } else { field.to_string() }
}

/// Bill of materials: one row per device, value and footprint.
pub fn bom(pr: &Project) -> String {
  let libs = pr.libs();
  let mut rows: Vec<(String, String, String, Vec<String>, String, String, String)> = Vec::new();
  for part in pr.parts_sorted() {
    let Some(dev) = libs.device(&part.device) else { continue };
    if dev.footprint.is_empty() {
      continue;
    }
    let fp = pr
      .board
      .placement(&part.refdes)
      .map(|p| p.footprint.clone())
      .filter(|f| !f.is_empty())
      .unwrap_or(dev.footprint.clone());
    match rows.iter_mut().find(|r| r.0 == dev.name && r.1 == part.value && r.2 == fp) {
      Some(r) => r.3.push(part.refdes.clone()),
      None => rows.push((
        dev.name.clone(),
        part.value.clone(),
        fp,
        vec![part.refdes.clone()],
        dev.manufacturer.clone(),
        dev.mpn.clone(),
        dev.description.clone(),
      )),
    }
  }
  let mut s = String::from("Qty,References,Value,Device,Footprint,Manufacturer,MPN,Description\n");
  for r in rows {
    let _ = writeln!(
      s,
      "{},{},{},{},{},{},{},{}",
      r.3.len(),
      csv(&r.3.join(" ")),
      csv(&r.1),
      csv(&r.0),
      csv(&r.2),
      csv(&r.4),
      csv(&r.5),
      csv(&r.6)
    );
  }
  s
}

/// Pick-and-place (centroid) file: every placed part's centre, rotation and side.
pub fn pick_and_place(pr: &Project) -> String {
  let mut s = String::from("Designator,Value,Package,Mid X,Mid Y,Rotation,Layer\n");
  let libs = pr.libs();
  let mut pls: Vec<_> = pr.board.placements.iter().collect();
  pls.sort_by(|a, b| natural(&a.refdes, &b.refdes));
  for pl in pls {
    let Some(part) = pr.schematic.parts.iter().find(|p| p.refdes == pl.refdes) else { continue };
    let Some(dev) = libs.device(&part.device) else { continue };
    let fp = if pl.footprint.is_empty() { dev.footprint.clone() } else { pl.footprint.clone() };
    let _ = writeln!(
      s,
      "{},{},{},{:.4},{:.4},{},{}",
      csv(&pl.refdes),
      csv(&part.value),
      csv(&fp),
      pl.at[0],
      pl.at[1],
      n(pl.rot.rem_euclid(360.0)),
      if pl.side == Side::Top { "Top" } else { "Bottom" }
    );
  }
  s
}

/// Every fab file, named after the project: (file name, contents).
pub fn fab_files(pr: &Project) -> Vec<(String, String)> {
  let pads = pr.pads();
  let base: String =
    pr.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
  let mut out = Vec::new();
  for l in pr.board.copper_layers() {
    out.push((format!("{base}-{}.gbr", layer_file(l)), copper(pr, &pads, l)));
  }
  out.push((format!("{base}-F_Mask.gbr"), mask(pr, &pads, Side::Top)));
  out.push((format!("{base}-B_Mask.gbr"), mask(pr, &pads, Side::Bottom)));
  out.push((format!("{base}-F_Paste.gbr"), paste(&pads, Side::Top)));
  out.push((format!("{base}-B_Paste.gbr"), paste(&pads, Side::Bottom)));
  out.push((format!("{base}-F_Silkscreen.gbr"), silk(pr, Side::Top)));
  out.push((format!("{base}-B_Silkscreen.gbr"), silk(pr, Side::Bottom)));
  out.push((format!("{base}-Edge_Cuts.gbr"), edge(pr)));
  out.push((format!("{base}-PTH.drl"), drill(pr, &pads, true)));
  out.push((format!("{base}-NPTH.drl"), drill(pr, &pads, false)));
  out.push((format!("{base}-BOM.csv"), bom(pr)));
  out.push((format!("{base}-PnP.csv"), pick_and_place(pr)));
  out
}
