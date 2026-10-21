//! The component library format: symbols (schematic), footprints (board), 3D model references,
//! and devices tying the three together with a pin-to-pad map.

use serde::{Deserialize, Serialize};

use crate::geom::P;

/// A pin's electrical type, for ERC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PinType {
  Input,
  Output,
  Bidirectional,
  TriState,
  #[default]
  Passive,
  PowerIn,
  PowerOut,
  OpenCollector,
  NoConnect,
  Unspecified,
}

impl PinType {
  pub fn parse(s: &str) -> PinType {
    match s {
      "input" | "in" => PinType::Input,
      "output" | "out" => PinType::Output,
      "bidirectional" | "io" => PinType::Bidirectional,
      "tri_state" | "hiz" => PinType::TriState,
      "power_in" | "pwr" => PinType::PowerIn,
      "power_out" | "sup" => PinType::PowerOut,
      "open_collector" | "open_emitter" | "oc" => PinType::OpenCollector,
      "no_connect" | "nc" => PinType::NoConnect,
      "unspecified" | "free" => PinType::Unspecified,
      _ => PinType::Passive,
    }
  }

  /// Whether the pin can set a net's level.
  pub fn drives(self) -> bool {
    matches!(
      self,
      PinType::Output
        | PinType::Bidirectional
        | PinType::TriState
        | PinType::PowerOut
        | PinType::OpenCollector
        | PinType::Passive
        | PinType::Unspecified
    )
  }
}

/// A symbol pin. `at` is the connection point at the pin's outer end; `dir` points from there
/// into the symbol body (degrees), `length` long.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pin {
  pub number: String,
  #[serde(default)]
  pub name: String,
  pub at: P,
  #[serde(default)]
  pub dir: f64,
  #[serde(default = "pin_len")]
  pub length: f64,
  #[serde(default)]
  pub kind: PinType,
  /// A hidden power pin connects to the net named after it (VCC, GND) without a wire.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub hidden: bool,
}

fn pin_len() -> f64 {
  2.54
}

/// Drawing primitives for symbols and footprints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Gfx {
  Line {
    a: P,
    b: P,
    #[serde(default)]
    width: f64,
  },
  Rect {
    a: P,
    b: P,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    fill: bool,
  },
  Circle {
    c: P,
    r: f64,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    fill: bool,
  },
  /// Counter-clockwise from `start` to `end` (degrees).
  Arc {
    c: P,
    r: f64,
    start: f64,
    end: f64,
    #[serde(default)]
    width: f64,
  },
  Poly {
    pts: Vec<P>,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    fill: bool,
  },
  Text {
    at: P,
    text: String,
    #[serde(default = "text_size")]
    size: f64,
  },
}

fn text_size() -> f64 {
  1.27
}

impl Gfx {
  /// The shape as polylines (arcs and circles sampled).
  pub fn polylines(&self) -> Vec<Vec<P>> {
    use crate::geom::arc_points;
    match self {
      Gfx::Line { a, b, .. } => vec![vec![*a, *b]],
      Gfx::Rect { a, b, .. } => vec![vec![*a, [b[0], a[1]], *b, [a[0], b[1]], *a]],
      Gfx::Circle { c, r, .. } => vec![arc_points(*c, *r, 0.0, 360.0, 15.0)],
      Gfx::Arc { c, r, start, end, .. } => vec![arc_points(*c, *r, *start, *end, 10.0)],
      Gfx::Poly { pts, fill, .. } => {
        let mut p = pts.clone();
        if *fill && !p.is_empty() {
          p.push(p[0]);
        }
        vec![p]
      }
      Gfx::Text { .. } => Vec::new(),
    }
  }

  pub fn width(&self) -> f64 {
    match self {
      Gfx::Line { width, .. }
      | Gfx::Rect { width, .. }
      | Gfx::Circle { width, .. }
      | Gfx::Arc { width, .. }
      | Gfx::Poly { width, .. } => *width,
      Gfx::Text { .. } => 0.0,
    }
  }
}

/// A schematic symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
  pub name: String,
  pub pins: Vec<Pin>,
  #[serde(default)]
  pub gfx: Vec<Gfx>,
  /// A power symbol (GND, VCC): it names the net its pin touches after the part's value.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub power: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
  #[default]
  Rect,
  Circle,
  Oval,
  RoundRect,
}

/// A footprint pad. Without a drill it's an SMD pad on the footprint's side; with one, a plated
/// through-hole pad on every copper layer (or a bare hole when `npth`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pad {
  pub number: String,
  #[serde(default)]
  pub shape: PadShape,
  pub at: P,
  pub size: P,
  #[serde(default)]
  pub rot: f64,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub drill: Option<f64>,
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub npth: bool,
}

impl Pad {
  pub fn smd(&self) -> bool {
    self.drill.is_none()
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FpLayer {
  Silk,
  Courtyard,
  Fab,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FpGfx {
  pub layer: FpLayer,
  #[serde(flatten)]
  pub gfx: Gfx,
}

/// A 3D model for a footprint: a simple body, or a model file (STEP) positioned on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Model3d {
  /// A box `size` (x, y, z) standing on the board, centred at `offset` (x, y).
  Box {
    size: [f64; 3],
    #[serde(default)]
    offset: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<[f32; 3]>,
  },
  /// An upright cylinder.
  Cylinder {
    diameter: f64,
    height: f64,
    #[serde(default)]
    offset: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<[f32; 3]>,
  },
  /// A model file, moved by `offset` (x, y, z), rotated by `rotate` (degrees about x, y, z)
  /// and scaled.
  File {
    path: String,
    #[serde(default)]
    offset: [f64; 3],
    #[serde(default)]
    rotate: [f64; 3],
    #[serde(default = "one3")]
    scale: [f64; 3],
  },
}

fn one3() -> [f64; 3] {
  [1.0; 3]
}

/// A board footprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Footprint {
  pub name: String,
  pub pads: Vec<Pad>,
  #[serde(default)]
  pub gfx: Vec<FpGfx>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<Model3d>,
}

impl Footprint {
  /// The courtyard rectangle: the drawn courtyard's bounds, else the pads' and silk's grown by
  /// 0.25 mm.
  pub fn courtyard(&self) -> (P, P) {
    let pts = |l: FpLayer| -> Vec<P> {
      self.gfx.iter().filter(|g| g.layer == l).flat_map(|g| g.gfx.polylines().into_iter().flatten()).collect()
    };
    if let Some(b) = crate::geom::bounds(pts(FpLayer::Courtyard)) {
      return b;
    }
    let mut all: Vec<P> = pts(FpLayer::Silk);
    for p in &self.pads {
      let r = p.size[0].max(p.size[1]) / 2.0;
      all.push([p.at[0] - r, p.at[1] - r]);
      all.push([p.at[0] + r, p.at[1] + r]);
    }
    let (lo, hi) = crate::geom::bounds(all).unwrap_or(([-0.5, -0.5], [0.5, 0.5]));
    ([lo[0] - 0.25, lo[1] - 0.25], [hi[0] + 0.25, hi[1] + 0.25])
  }
}

/// A device: what goes on the schematic and the board as one part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
  pub name: String,
  #[serde(default)]
  pub description: String,
  /// Reference designator prefix: R, C, U, ...
  #[serde(default = "prefix")]
  pub prefix: String,
  /// The value a new part gets.
  #[serde(default)]
  pub value: String,
  pub symbol: String,
  /// Footprint name; empty for symbols with no board part (power symbols).
  #[serde(default)]
  pub footprint: String,
  /// Overrides the footprint's 3D model.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub model: Option<Model3d>,
  /// Symbol pin number → pad number; pins not listed go to the pad with the same number.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub pinmap: Vec<(String, String)>,
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub manufacturer: String,
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub mpn: String,
}

fn prefix() -> String {
  "U".into()
}

impl Device {
  /// The pads a symbol pin connects to.
  pub fn pads_of<'a>(&'a self, pin: &'a str) -> Vec<&'a str> {
    let mapped: Vec<&str> = self.pinmap.iter().filter(|(p, _)| p == pin).map(|(_, pad)| pad.as_str()).collect();
    if mapped.is_empty() { vec![pin] } else { mapped }
  }
}

/// A library: symbols, footprints and devices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Library {
  pub name: String,
  #[serde(default)]
  pub symbols: Vec<Symbol>,
  #[serde(default)]
  pub footprints: Vec<Footprint>,
  #[serde(default)]
  pub devices: Vec<Device>,
}

impl Library {
  pub fn symbol(&self, name: &str) -> Option<&Symbol> {
    self.symbols.iter().find(|s| s.name.eq_ignore_ascii_case(name))
  }

  pub fn footprint(&self, name: &str) -> Option<&Footprint> {
    self.footprints.iter().find(|s| s.name.eq_ignore_ascii_case(name))
  }

  pub fn device(&self, name: &str) -> Option<&Device> {
    self.devices.iter().find(|s| s.name.eq_ignore_ascii_case(name))
  }

  /// Adds or replaces by name.
  pub fn merge(&mut self, other: Library) {
    for s in other.symbols {
      self.symbols.retain(|x| !x.name.eq_ignore_ascii_case(&s.name));
      self.symbols.push(s);
    }
    for f in other.footprints {
      self.footprints.retain(|x| !x.name.eq_ignore_ascii_case(&f.name));
      self.footprints.push(f);
    }
    for d in other.devices {
      self.devices.retain(|x| !x.name.eq_ignore_ascii_case(&d.name));
      self.devices.push(d);
    }
  }
}

/// Libraries searched in order: a name may be qualified as "lib:name".
pub struct Libs<'a>(pub Vec<&'a Library>);

impl<'a> Libs<'a> {
  fn split(name: &str) -> (Option<&str>, &str) {
    match name.split_once(':') {
      Some((l, n)) => (Some(l), n),
      None => (None, name),
    }
  }

  fn find<T>(&self, name: &str, f: impl Fn(&'a Library, &str) -> Option<&'a T>) -> Option<&'a T> {
    let (lib, n) = Self::split(name);
    self.0.iter().filter(|l| lib.is_none_or(|x| l.name.eq_ignore_ascii_case(x))).find_map(|l| f(l, n))
  }

  pub fn device(&self, name: &str) -> Option<&'a Device> {
    self.find(name, |l, n| l.device(n))
  }

  pub fn symbol(&self, name: &str) -> Option<&'a Symbol> {
    self.find(name, |l, n| l.symbol(n))
  }

  pub fn footprint(&self, name: &str) -> Option<&'a Footprint> {
    self.find(name, |l, n| l.footprint(n))
  }
}
