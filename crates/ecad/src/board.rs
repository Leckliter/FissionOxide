//! PCB layout: outline, layers, placed footprints, tracks, vias, copper pours, design rules;
//! the copper they make, its ratsnest, and the design rules check.

use serde::{Deserialize, Serialize};

use crate::geom::{P, Shape2, Xf, dist, gap, gap_to_outline, point_in_poly};
use crate::library::{Libs, PadShape};
use crate::schematic::{Netlist, Schematic};
use crate::{Severity, Violation};

/// A copper layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
  Top,
  Inner1,
  Inner2,
  Bottom,
}

impl Layer {
  pub fn parse(s: &str) -> Option<Layer> {
    Some(match s.to_ascii_lowercase().as_str() {
      "top" | "f.cu" | "1" | "l1" => Layer::Top,
      "inner1" | "in1.cu" | "2" | "l2" => Layer::Inner1,
      "inner2" | "in2.cu" | "3" | "l3" => Layer::Inner2,
      "bottom" | "b.cu" | "4" | "l4" => Layer::Bottom,
      _ => return None,
    })
  }

  pub fn name(self) -> &'static str {
    match self {
      Layer::Top => "top",
      Layer::Inner1 => "inner1",
      Layer::Inner2 => "inner2",
      Layer::Bottom => "bottom",
    }
  }

  /// The copper layers of a board with `n` layers (2 or 4), top to bottom.
  pub fn stack(n: u8) -> Vec<Layer> {
    if n >= 4 { vec![Layer::Top, Layer::Inner1, Layer::Inner2, Layer::Bottom] } else { vec![Layer::Top, Layer::Bottom] }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Side {
  #[default]
  Top,
  Bottom,
}

impl Side {
  pub fn layer(self) -> Layer {
    match self {
      Side::Top => Layer::Top,
      Side::Bottom => Layer::Bottom,
    }
  }
}

/// A part's footprint on the board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placement {
  pub refdes: String,
  /// The footprint, when it differs from the device's.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub footprint: String,
  pub at: P,
  #[serde(default)]
  pub rot: f64,
  #[serde(default)]
  pub side: Side,
}

impl Placement {
  pub fn xf(&self) -> Xf {
    Xf { at: self.at, rot: self.rot, mirror: self.side == Side::Bottom }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
  pub layer: Layer,
  pub a: P,
  pub b: P,
  pub width: f64,
  #[serde(default)]
  pub net: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Via {
  pub at: P,
  pub drill: f64,
  pub diameter: f64,
  #[serde(default)]
  pub net: String,
}

/// A copper pour: the polygon filled on a layer, cleared around other nets' copper and the
/// board edge, solidly joined to its own net's pads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pour {
  pub layer: Layer,
  pub net: String,
  pub outline: Vec<P>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub clearance: Option<f64>,
}

/// An unplated mounting hole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hole {
  pub at: P,
  pub drill: f64,
}

/// Design rules (mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
  pub clearance: f64,
  pub track_width: f64,
  pub min_track: f64,
  pub via_drill: f64,
  pub via_diameter: f64,
  pub min_drill: f64,
  pub min_annular: f64,
  pub edge_clearance: f64,
  pub hole_clearance: f64,
  pub mask_expansion: f64,
}

impl Default for Rules {
  fn default() -> Self {
    // a common low-cost fab's 2-layer capabilities, with some margin
    Rules {
      clearance: 0.2,
      track_width: 0.25,
      min_track: 0.15,
      via_drill: 0.3,
      via_diameter: 0.6,
      min_drill: 0.2,
      min_annular: 0.13,
      edge_clearance: 0.3,
      hole_clearance: 0.25,
      mask_expansion: 0.05,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
  /// The board's outline (counter-clockwise or not), in board coordinates.
  #[serde(default)]
  pub outline: Vec<P>,
  #[serde(default = "thickness")]
  pub thickness: f64,
  #[serde(default = "two")]
  pub layers: u8,
  #[serde(default)]
  pub placements: Vec<Placement>,
  #[serde(default)]
  pub tracks: Vec<Track>,
  #[serde(default)]
  pub vias: Vec<Via>,
  #[serde(default)]
  pub pours: Vec<Pour>,
  #[serde(default)]
  pub holes: Vec<Hole>,
  #[serde(default)]
  pub rules: Rules,
}

fn thickness() -> f64 {
  1.6
}

fn two() -> u8 {
  2
}

impl Default for Board {
  fn default() -> Self {
    Board {
      outline: Vec::new(),
      thickness: 1.6,
      layers: 2,
      placements: Vec::new(),
      tracks: Vec::new(),
      vias: Vec::new(),
      pours: Vec::new(),
      holes: Vec::new(),
      rules: Rules::default(),
    }
  }
}

/// A pad as placed on the board.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardPad {
  pub refdes: String,
  pub number: String,
  pub net: String,
  pub at: P,
  pub size: P,
  pub rot: f64,
  pub shape: PadShape,
  pub drill: Option<f64>,
  pub npth: bool,
  pub side: Side,
  pub layers: Vec<Layer>,
}

impl BoardPad {
  /// The pad's copper, grown by `grow` on every side.
  pub fn shape2(&self, grow: f64) -> Shape2 {
    let size = [self.size[0] + 2.0 * grow, self.size[1] + 2.0 * grow];
    match self.shape {
      PadShape::Rect | PadShape::RoundRect => Shape2::rect(self.at, size, self.rot),
      PadShape::Circle => Shape2::circle(self.at, size[0]),
      PadShape::Oval => Shape2::oval(self.at, size, self.rot),
    }
  }
}

/// What a copper item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
  Pad(usize),
  Track(usize),
  Via(usize),
  Pour(usize),
}

#[derive(Debug, Clone)]
pub struct Copper {
  pub item: Item,
  pub net: String,
  pub layers: Vec<Layer>,
  pub shape: Shape2,
}

/// An unrouted connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Airwire {
  pub net: String,
  pub a: P,
  pub b: P,
}

impl Board {
  pub fn copper_layers(&self) -> Vec<Layer> {
    Layer::stack(self.layers)
  }

  pub fn placement(&self, refdes: &str) -> Option<&Placement> {
    self.placements.iter().find(|p| p.refdes == refdes)
  }

  /// Every pad on the board with its net.
  pub fn pads(&self, sch: &Schematic, libs: &Libs, nets: &Netlist) -> Vec<BoardPad> {
    let all = self.copper_layers();
    let mut out = Vec::new();
    for pl in &self.placements {
      let Some(part) = sch.parts.iter().find(|p| p.refdes == pl.refdes) else { continue };
      let Some(dev) = libs.device(&part.device) else { continue };
      let fp_name = if pl.footprint.is_empty() { &dev.footprint } else { &pl.footprint };
      let Some(fp) = libs.footprint(fp_name) else { continue };
      let sym = libs.symbol(&dev.symbol);
      let xf = pl.xf();
      for pad in &fp.pads {
        let net = sym
          .and_then(|s| {
            s.pins
              .iter()
              .find(|pin| dev.pads_of(&pin.number).contains(&pad.number.as_str()))
              .and_then(|pin| nets.net_of(&pl.refdes, &pin.number))
          })
          .unwrap_or("")
          .to_string();
        out.push(BoardPad {
          refdes: pl.refdes.clone(),
          number: pad.number.clone(),
          net: if pad.npth { String::new() } else { net },
          at: xf.apply(pad.at),
          size: pad.size,
          rot: xf.angle(pad.rot),
          shape: pad.shape,
          drill: pad.drill,
          npth: pad.npth,
          side: pl.side,
          layers: if pad.drill.is_some() { all.clone() } else { vec![pl.side.layer()] },
        });
      }
    }
    out
  }

  /// All copper: pads, tracks, vias and pours (unplated holes carry none).
  pub fn copper(&self, pads: &[BoardPad]) -> Vec<Copper> {
    let all = self.copper_layers();
    let mut out = Vec::new();
    for (i, p) in pads.iter().enumerate() {
      if !p.npth {
        out.push(Copper { item: Item::Pad(i), net: p.net.clone(), layers: p.layers.clone(), shape: p.shape2(0.0) });
      }
    }
    for (i, t) in self.tracks.iter().enumerate() {
      out.push(Copper {
        item: Item::Track(i),
        net: t.net.clone(),
        layers: vec![t.layer],
        shape: Shape2::Capsule { a: t.a, b: t.b, r: t.width / 2.0 },
      });
    }
    for (i, v) in self.vias.iter().enumerate() {
      out.push(Copper {
        item: Item::Via(i),
        net: v.net.clone(),
        layers: all.clone(),
        shape: Shape2::circle(v.at, v.diameter),
      });
    }
    for (i, p) in self.pours.iter().enumerate() {
      if p.outline.len() >= 3 {
        out.push(Copper {
          item: Item::Pour(i),
          net: p.net.clone(),
          layers: vec![p.layer],
          shape: Shape2::Poly(p.outline.clone()),
        });
      }
    }
    out
  }

  /// The net of the copper under a point on a layer (pads and vias first, then tracks, pours).
  pub fn net_at(&self, pads: &[BoardPad], at: P, layer: Option<Layer>) -> Option<String> {
    let cu = self.copper(pads);
    let hit = |c: &&Copper| layer.is_none_or(|l| c.layers.contains(&l)) && c.shape.contains(at) && !c.net.is_empty();
    cu.iter()
      .filter(|c| matches!(c.item, Item::Pad(_) | Item::Via(_)))
      .find(hit)
      .or_else(|| cu.iter().filter(|c| matches!(c.item, Item::Track(_))).find(hit))
      .or_else(|| cu.iter().find(hit))
      .map(|c| c.net.clone())
  }

  /// Connections still to route: per net, a minimum spanning tree between the groups of copper
  /// that already touch.
  pub fn ratsnest(&self, pads: &[BoardPad]) -> Vec<Airwire> {
    let cu = self.copper(pads);
    let mut nets: Vec<&str> = pads.iter().map(|p| p.net.as_str()).filter(|n| !n.is_empty()).collect();
    nets.sort_by(|a, b| crate::schematic::natural(a, b));
    nets.dedup();
    let mut out = Vec::new();
    for net in nets {
      let items: Vec<&Copper> = cu.iter().filter(|c| c.net == net).collect();
      let n = items.len();
      let mut parent: Vec<usize> = (0..n).collect();
      fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
          r = p[r];
        }
        p[i] = r;
        r
      }
      for i in 0..n {
        for j in i + 1..n {
          let shared = items[i].layers.iter().any(|l| items[j].layers.contains(l));
          if shared && touches(&items[i].shape, &items[j].shape) {
            let (a, b) = (find(&mut parent, i), find(&mut parent, j));
            if a != b {
              parent[a.max(b)] = a.min(b);
            }
          }
        }
      }
      // anchor points per group that holds a pad
      let mut groups: Vec<(usize, Vec<P>)> = Vec::new();
      for (i, it) in items.iter().enumerate() {
        let r = find(&mut parent, i);
        let pts: Vec<P> = match (&it.item, &it.shape) {
          (Item::Pour(_), _) => Vec::new(),
          (Item::Pad(k), _) => vec![pads[*k].at],
          (_, Shape2::Capsule { a, b, .. }) => vec![*a, *b],
          (_, s) => vec![s.anchor()],
        };
        match groups.iter_mut().find(|g| g.0 == r) {
          Some(g) => g.1.extend(pts),
          None => groups.push((r, pts)),
        }
      }
      let has_pad: Vec<bool> = groups
        .iter()
        .map(|g| (0..n).any(|i| matches!(items[i].item, Item::Pad(_)) && find(&mut parent, i) == g.0))
        .collect();
      let groups: Vec<Vec<P>> =
        groups.into_iter().zip(has_pad).filter(|(g, p)| *p && !g.1.is_empty()).map(|(g, _)| g.1).collect();
      // Kruskal over the groups
      let mut edges = Vec::new();
      for i in 0..groups.len() {
        for j in i + 1..groups.len() {
          let mut best = (f64::INFINITY, [0.0; 2], [0.0; 2]);
          for a in &groups[i] {
            for b in &groups[j] {
              let d = dist(*a, *b);
              if d < best.0 {
                best = (d, *a, *b);
              }
            }
          }
          edges.push((best.0, i, j, best.1, best.2));
        }
      }
      edges.sort_by(|a, b| a.0.total_cmp(&b.0));
      let mut gp: Vec<usize> = (0..groups.len()).collect();
      for (_, i, j, a, b) in edges {
        let (ri, rj) = (find(&mut gp, i), find(&mut gp, j));
        if ri != rj {
          gp[ri.max(rj)] = ri.min(rj);
          out.push(Airwire { net: net.to_string(), a, b });
        }
      }
    }
    out
  }

  /// Every drilled hole: (centre, diameter, plated).
  pub fn drills(&self, pads: &[BoardPad]) -> Vec<(P, f64, bool)> {
    let mut out: Vec<(P, f64, bool)> = Vec::new();
    for p in pads {
      if let Some(d) = p.drill {
        out.push((p.at, d, !p.npth));
      }
    }
    for v in &self.vias {
      out.push((v.at, v.drill, true));
    }
    for h in &self.holes {
      out.push((h.at, h.drill, false));
    }
    out
  }

  /// The design rules check.
  pub fn drc(&self, sch: &Schematic, libs: &Libs, nets: &Netlist) -> Vec<Violation> {
    let r = &self.rules;
    let mut out = Vec::new();
    let viol = |code: &str, sev: Severity, msg: String, at: P, layer: Option<Layer>| Violation {
      code: code.into(),
      severity: sev,
      message: msg,
      at: Some(at),
      layer: layer.map(|l| l.name().to_string()),
    };
    if self.outline.len() < 3 {
      out.push(Violation {
        code: "no_outline".into(),
        severity: Severity::Error,
        message: "the board has no outline".into(),
        at: None,
        layer: None,
      });
    }
    for pl in &self.placements {
      let fp = sch
        .parts
        .iter()
        .find(|p| p.refdes == pl.refdes)
        .and_then(|p| libs.device(&p.device))
        .map(|d| if pl.footprint.is_empty() { d.footprint.clone() } else { pl.footprint.clone() });
      if fp.as_ref().is_none_or(|f| libs.footprint(f).is_none()) {
        out.push(viol("missing_footprint", Severity::Error, format!("{}: no footprint", pl.refdes), pl.at, None));
      }
    }
    let pads = self.pads(sch, libs, nets);
    let cu = self.copper(&pads);
    let describe = |c: &Copper| match c.item {
      Item::Pad(i) => format!("pad {}.{}", pads[i].refdes, pads[i].number),
      Item::Track(_) => "track".to_string(),
      Item::Via(_) => "via".to_string(),
      Item::Pour(_) => "pour".to_string(),
    };
    let solid: Vec<&Copper> = cu.iter().filter(|c| !matches!(c.item, Item::Pour(_))).collect();
    let boxes: Vec<(P, P)> = solid.iter().map(|c| c.shape.bbox()).collect();
    for i in 0..solid.len() {
      for j in i + 1..solid.len() {
        let (a, b) = (solid[i], solid[j]);
        if !a.net.is_empty() && a.net == b.net {
          continue;
        }
        let Some(layer) = a.layers.iter().find(|l| b.layers.contains(l)) else { continue };
        let (ba, bb) = (boxes[i], boxes[j]);
        if ba.0[0] - r.clearance > bb.1[0]
          || bb.0[0] - r.clearance > ba.1[0]
          || ba.0[1] - r.clearance > bb.1[1]
          || bb.0[1] - r.clearance > ba.1[1]
        {
          continue;
        }
        let g = gap(&a.shape, &b.shape);
        if g < r.clearance - 1e-6 {
          let at = a.shape.anchor();
          let (na, nb) =
            (if a.net.is_empty() { "no net" } else { &a.net }, if b.net.is_empty() { "no net" } else { &b.net });
          if g <= 0.0 {
            out.push(viol(
              "short",
              Severity::Error,
              format!("{} ({na}) touches {} ({nb})", describe(a), describe(b)),
              at,
              Some(*layer),
            ));
          } else {
            out.push(viol(
              "clearance",
              Severity::Error,
              format!("{} ({na}) is {g:.3} mm from {} ({nb}), less than {:.3}", describe(a), describe(b), r.clearance),
              at,
              Some(*layer),
            ));
          }
        }
      }
    }
    for t in &self.tracks {
      if t.width < r.min_track - 1e-9 {
        out.push(viol(
          "track_width",
          Severity::Error,
          format!("track {:.3} mm wide, narrower than {:.3}", t.width, r.min_track),
          t.a,
          Some(t.layer),
        ));
      }
    }
    for v in &self.vias {
      if v.drill < r.min_drill - 1e-9 {
        out.push(viol(
          "drill",
          Severity::Error,
          format!("via drill {:.3} mm, smaller than {:.3}", v.drill, r.min_drill),
          v.at,
          None,
        ));
      }
      if (v.diameter - v.drill) / 2.0 < r.min_annular - 1e-9 {
        out.push(viol(
          "annular_ring",
          Severity::Error,
          format!("via annular ring {:.3} mm, less than {:.3}", (v.diameter - v.drill) / 2.0, r.min_annular),
          v.at,
          None,
        ));
      }
    }
    for p in pads.iter().filter(|p| !p.npth) {
      if let Some(d) = p.drill {
        if d < r.min_drill - 1e-9 {
          out.push(viol(
            "drill",
            Severity::Error,
            format!("pad {}.{} drill {d:.3} mm, smaller than {:.3}", p.refdes, p.number, r.min_drill),
            p.at,
            None,
          ));
        }
        let ring = (p.size[0].min(p.size[1]) - d) / 2.0;
        if ring < r.min_annular - 1e-9 {
          out.push(viol(
            "annular_ring",
            Severity::Error,
            format!("pad {}.{} annular ring {ring:.3} mm, less than {:.3}", p.refdes, p.number, r.min_annular),
            p.at,
            None,
          ));
        }
      }
    }
    if self.outline.len() >= 3 {
      for c in &cu {
        if matches!(c.item, Item::Pour(_)) {
          continue;
        }
        let (g, inside) = gap_to_outline(&c.shape, &self.outline);
        let at = c.shape.anchor();
        if !inside {
          out.push(viol(
            "outside_board",
            Severity::Error,
            format!("{} is outside the board", describe(c)),
            at,
            c.layers.first().copied(),
          ));
        } else if g < r.edge_clearance - 1e-6 {
          out.push(viol(
            "edge_clearance",
            Severity::Error,
            format!("{} is {g:.3} mm from the board edge, less than {:.3}", describe(c), r.edge_clearance),
            at,
            c.layers.first().copied(),
          ));
        }
      }
      for (at, d, _) in self.drills(&pads) {
        if !point_in_poly(at, &self.outline) {
          out.push(viol("outside_board", Severity::Error, format!("{d:.2} mm hole is outside the board"), at, None));
        }
      }
    }
    let drills = self.drills(&pads);
    for i in 0..drills.len() {
      for j in i + 1..drills.len() {
        let (a, b) = (drills[i], drills[j]);
        let g = dist(a.0, b.0) - (a.1 + b.1) / 2.0;
        if g < r.hole_clearance - 1e-6 {
          out.push(viol(
            "hole_clearance",
            Severity::Error,
            format!("holes {g:.3} mm apart, less than {:.3}", r.hole_clearance),
            a.0,
            None,
          ));
        }
      }
    }
    // courtyards on the same side
    let yards: Vec<(String, Side, Shape2)> = self
      .placements
      .iter()
      .filter_map(|pl| {
        let part = sch.parts.iter().find(|p| p.refdes == pl.refdes)?;
        let dev = libs.device(&part.device)?;
        let fp = libs.footprint(if pl.footprint.is_empty() { &dev.footprint } else { &pl.footprint })?;
        let (lo, hi) = fp.courtyard();
        let xf = pl.xf();
        Some((
          pl.refdes.clone(),
          pl.side,
          Shape2::Poly([lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]].iter().map(|p| xf.apply(*p)).collect()),
        ))
      })
      .collect();
    for i in 0..yards.len() {
      for j in i + 1..yards.len() {
        if yards[i].1 == yards[j].1 && overlap(&yards[i].2, &yards[j].2) {
          out.push(viol(
            "courtyard_overlap",
            Severity::Warning,
            format!("{} and {} overlap", yards[i].0, yards[j].0),
            yards[i].2.anchor(),
            None,
          ));
        }
      }
    }
    for a in self.ratsnest(&pads) {
      out.push(viol(
        "unrouted",
        Severity::Warning,
        format!("net {} is not routed ({:.1} mm)", a.net, dist(a.a, a.b)),
        a.a,
        None,
      ));
    }
    out
  }
}

fn touches(a: &Shape2, b: &Shape2) -> bool {
  gap(a, b) <= 1e-6
}

/// Whether two polygons overlap by more than touching.
fn overlap(a: &Shape2, b: &Shape2) -> bool {
  let (Shape2::Poly(p), Shape2::Poly(q)) = (a, b) else { return touches(a, b) };
  let shrink = |s: &[P]| -> Vec<P> {
    let c = Shape2::Poly(s.to_vec()).anchor();
    s.iter().map(|v| [c[0] + (v[0] - c[0]) * 0.999, c[1] + (v[1] - c[1]) * 0.999]).collect()
  };
  touches(&Shape2::Poly(shrink(p)), &Shape2::Poly(shrink(q)))
}
