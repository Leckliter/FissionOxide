//! KiCad library import: symbol libraries (`.kicad_sym`) and footprints (`.kicad_mod`, or a
//! `.pretty` folder of them). Both are S-expression files; KiCad footprints have Y pointing
//! down, so they are flipped into this crate's Y-up coordinates.

use crate::geom::{P, arc3_angles, circle3};
use crate::library::{Device, Footprint, FpGfx, FpLayer, Gfx, Library, Model3d, Pad, PadShape, Pin, PinType, Symbol};

/// An S-expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Sx {
  Atom(String),
  List(Vec<Sx>),
}

impl Sx {
  pub fn head(&self) -> Option<&str> {
    match self {
      Sx::List(v) => v.first().and_then(Sx::atom),
      _ => None,
    }
  }

  pub fn atom(&self) -> Option<&str> {
    match self {
      Sx::Atom(s) => Some(s),
      _ => None,
    }
  }

  pub fn items(&self) -> &[Sx] {
    match self {
      Sx::List(v) => v,
      _ => &[],
    }
  }

  /// The `i`th element (0 = the head) as text.
  pub fn arg(&self, i: usize) -> Option<&str> {
    self.items().get(i).and_then(Sx::atom)
  }

  pub fn num(&self, i: usize) -> Option<f64> {
    self.arg(i).and_then(|s| s.parse().ok())
  }

  /// Child lists with this head.
  pub fn all<'a>(&'a self, head: &'a str) -> impl Iterator<Item = &'a Sx> + 'a {
    self.items().iter().filter(move |s| s.head() == Some(head))
  }

  pub fn child(&self, head: &str) -> Option<&Sx> {
    self.items().iter().find(|s| s.head() == Some(head))
  }

  /// `(head x y)` as a point.
  pub fn xy(&self, head: &str) -> Option<P> {
    let c = self.child(head)?;
    Some([c.num(1)?, c.num(2)?])
  }

  /// Whether a bare atom flag is present, e.g. `(power)` or `hide`.
  pub fn flag(&self, name: &str) -> bool {
    self.items().iter().any(|s| s.atom() == Some(name) || s.head() == Some(name))
  }
}

/// Parses one S-expression (the whole file).
pub fn parse(text: &str) -> Result<Sx, String> {
  let mut stack: Vec<Vec<Sx>> = Vec::new();
  let mut top: Option<Sx> = None;
  let b: Vec<char> = text.chars().collect();
  let mut i = 0;
  while i < b.len() {
    let c = b[i];
    match c {
      '(' => {
        stack.push(Vec::new());
        i += 1;
      }
      ')' => {
        let l = stack.pop().ok_or("unbalanced ')'")?;
        let node = Sx::List(l);
        match stack.last_mut() {
          Some(p) => p.push(node),
          None => {
            top = Some(node);
          }
        }
        i += 1;
      }
      '"' => {
        let mut s = String::new();
        i += 1;
        while i < b.len() && b[i] != '"' {
          if b[i] == '\\' && i + 1 < b.len() {
            i += 1;
            s.push(match b[i] {
              'n' => '\n',
              't' => '\t',
              o => o,
            });
          } else {
            s.push(b[i]);
          }
          i += 1;
        }
        if i >= b.len() {
          return Err("unterminated string".into());
        }
        i += 1;
        stack.last_mut().ok_or("text outside any list")?.push(Sx::Atom(s));
      }
      c if c.is_whitespace() => i += 1,
      _ => {
        let start = i;
        while i < b.len() && !b[i].is_whitespace() && b[i] != '(' && b[i] != ')' {
          i += 1;
        }
        let s: String = b[start..i].iter().collect();
        stack.last_mut().ok_or("text outside any list")?.push(Sx::Atom(s));
      }
    }
    if top.is_some() && stack.is_empty() {
      break;
    }
  }
  if !stack.is_empty() {
    return Err("unbalanced '('".into());
  }
  top.ok_or_else(|| "empty file".into())
}

fn width(s: &Sx) -> f64 {
  s.child("stroke")
    .and_then(|st| st.child("width"))
    .and_then(|w| w.num(1))
    .or_else(|| s.child("width").and_then(|w| w.num(1)))
    .unwrap_or(0.0)
}

fn filled(s: &Sx) -> bool {
  s.child("fill").and_then(|f| f.child("type")).and_then(|t| t.arg(1)).is_some_and(|t| t != "none")
}

/// Symbol graphics (KiCad symbols are Y-up already).
fn sym_gfx(s: &Sx, off: P) -> Option<Gfx> {
  let o = |p: P| [p[0] + off[0], p[1] + off[1]];
  Some(match s.head()? {
    "rectangle" => Gfx::Rect { a: o(s.xy("start")?), b: o(s.xy("end")?), width: width(s), fill: filled(s) },
    "circle" => Gfx::Circle { c: o(s.xy("center")?), r: s.child("radius")?.num(1)?, width: width(s), fill: filled(s) },
    "polyline" => {
      let pts: Vec<P> = s.child("pts")?.all("xy").filter_map(|p| Some(o([p.num(1)?, p.num(2)?]))).collect();
      if pts.len() == 2 {
        Gfx::Line { a: pts[0], b: pts[1], width: width(s) }
      } else {
        Gfx::Poly { pts, width: width(s), fill: filled(s) }
      }
    }
    "arc" => {
      let (a, b) = (o(s.xy("start")?), o(s.xy("end")?));
      match s.xy("mid") {
        Some(m) => {
          let m = o(m);
          let (c, r) = circle3(a, m, b)?;
          let (st, en) = arc3_angles(c, a, m, b);
          let (st, en) = if en < st { (en, st) } else { (st, en) };
          Gfx::Arc { c, r, start: st, end: en, width: width(s) }
        }
        None => Gfx::Line { a, b, width: width(s) },
      }
    }
    "text" => Gfx::Text { at: o(s.xy("at")?), text: s.arg(1)?.to_string(), size: 1.27 },
    _ => return None,
  })
}

fn property(s: &Sx, name: &str) -> Option<String> {
  s.all("property").find(|p| p.arg(1) == Some(name)).and_then(|p| p.arg(2)).map(str::to_string)
}

/// Imports a `.kicad_sym` library: a symbol and a device per KiCad symbol. A device's footprint
/// is the symbol's Footprint property without its library prefix. Multi-unit symbols get their
/// units side by side.
pub fn import_symbols(text: &str, lib_name: &str) -> Result<Library, String> {
  let root = parse(text)?;
  if root.head() != Some("kicad_symbol_lib") {
    return Err("not a KiCad symbol library (no kicad_symbol_lib)".into());
  }
  let mut lib = Library { name: lib_name.into(), ..Default::default() };
  let all: Vec<&Sx> = root.all("symbol").collect();
  for s in &all {
    let Some(name) = s.arg(1) else { continue };
    // `extends`: take the parent's drawing
    let body = match s.child("extends").and_then(|e| e.arg(1)) {
      Some(parent) => all.iter().find(|p| p.arg(1) == Some(parent)).copied().unwrap_or(s),
      None => s,
    };
    let mut units: Vec<(u32, &Sx)> = Vec::new();
    for u in body.all("symbol") {
      let un = u.arg(1).unwrap_or("");
      let parts: Vec<&str> = un.rsplitn(3, '_').collect();
      let unit: u32 = parts.get(1).and_then(|x| x.parse().ok()).unwrap_or(0);
      units.push((unit, u));
    }
    let max_unit = units.iter().map(|u| u.0).max().unwrap_or(0);
    let mut pins = Vec::new();
    let mut gfx = Vec::new();
    let spacing = 25.4;
    for (unit, u) in &units {
      let off = [if *unit > 1 { (*unit - 1) as f64 * spacing } else { 0.0 }, 0.0];
      let unit_list: Vec<u32> = if *unit == 0 { (1..=max_unit.max(1)).collect() } else { vec![*unit] };
      for item in u.items() {
        if item.head() == Some("pin") {
          let Some(at) = item.child("at") else { continue };
          let (x, y, a) = (at.num(1).unwrap_or(0.0), at.num(2).unwrap_or(0.0), at.num(3).unwrap_or(0.0));
          let number = item.child("number").and_then(|n| n.arg(1)).unwrap_or("").to_string();
          let pname = item.child("name").and_then(|n| n.arg(1)).unwrap_or("").to_string();
          for un in &unit_list {
            let uoff = if *un > 1 { (*un - 1) as f64 * spacing } else { 0.0 };
            if pins.iter().any(|p: &Pin| p.number == number && !number.is_empty()) {
              continue;
            }
            pins.push(Pin {
              number: number.clone(),
              name: if pname == "~" { String::new() } else { pname.clone() },
              at: [x + uoff, y],
              dir: a,
              length: item.child("length").and_then(|l| l.num(1)).unwrap_or(2.54),
              kind: PinType::parse(item.arg(1).unwrap_or("passive")),
              hidden: item.flag("hide") && matches!(PinType::parse(item.arg(1).unwrap_or("")), PinType::PowerIn),
            });
          }
        } else if let Some(g) = sym_gfx(item, off) {
          gfx.push(g);
        }
      }
    }
    let power = s.flag("power") || body.flag("power");
    lib.symbols.push(Symbol { name: name.into(), pins, gfx, power });
    let reference = property(s, "Reference").or_else(|| property(body, "Reference")).unwrap_or("U".into());
    let footprint = property(s, "Footprint").or_else(|| property(body, "Footprint")).unwrap_or_default();
    let footprint = footprint.rsplit(':').next().unwrap_or("").to_string();
    lib.devices.push(Device {
      name: name.into(),
      description: property(s, "Description").or_else(|| property(s, "ki_description")).unwrap_or_default(),
      prefix: reference,
      value: property(s, "Value").unwrap_or(name.into()),
      symbol: name.into(),
      footprint: if power { String::new() } else { footprint },
      model: None,
      pinmap: Vec::new(),
      manufacturer: property(s, "Manufacturer").unwrap_or_default(),
      mpn: property(s, "MPN").unwrap_or_default(),
    });
  }
  Ok(lib)
}

fn flip(p: P) -> P {
  [p[0], -p[1]]
}

fn fp_layer(s: &Sx) -> Option<FpLayer> {
  match s.child("layer").and_then(|l| l.arg(1))? {
    "F.SilkS" | "F.Silkscreen" | "B.SilkS" | "B.Silkscreen" => Some(FpLayer::Silk),
    "F.CrtYd" | "F.Courtyard" | "B.CrtYd" | "B.Courtyard" => Some(FpLayer::Courtyard),
    "F.Fab" | "B.Fab" => Some(FpLayer::Fab),
    _ => None,
  }
}

/// Imports one `.kicad_mod` footprint.
pub fn import_footprint(text: &str) -> Result<Footprint, String> {
  let root = parse(text)?;
  if !matches!(root.head(), Some("footprint") | Some("module")) {
    return Err("not a KiCad footprint (no footprint or module)".into());
  }
  let name = root.arg(1).ok_or("footprint without a name")?.to_string();
  let mut pads = Vec::new();
  let mut gfx = Vec::new();
  let mut model = None;
  for it in root.items() {
    match it.head() {
      Some("pad") => {
        let number = it.arg(1).unwrap_or("").to_string();
        let kind = it.arg(2).unwrap_or("smd");
        let shape = match it.arg(3).unwrap_or("rect") {
          "circle" => PadShape::Circle,
          "oval" => PadShape::Oval,
          "roundrect" => PadShape::RoundRect,
          _ => PadShape::Rect,
        };
        let at = it.child("at").ok_or(format!("pad {number} has no position"))?;
        let size = it.child("size").ok_or(format!("pad {number} has no size"))?;
        let drill = it.child("drill").and_then(|d| {
          // (drill 1.0) or (drill oval 1.0 2.0)
          if d.arg(1) == Some("oval") { d.num(2) } else { d.num(1) }
        });
        if kind == "connect" {
          continue;
        }
        pads.push(Pad {
          number,
          shape,
          at: flip([at.num(1).unwrap_or(0.0), at.num(2).unwrap_or(0.0)]),
          size: [size.num(1).unwrap_or(1.0), size.num(2).unwrap_or(1.0)],
          rot: -at.num(3).unwrap_or(0.0),
          drill: if kind == "smd" { None } else { drill.or(Some(size.num(1).unwrap_or(1.0))) },
          npth: kind == "np_thru_hole",
        });
      }
      Some(h @ ("fp_line" | "fp_rect" | "fp_circle" | "fp_arc" | "fp_poly")) => {
        let Some(layer) = fp_layer(it) else { continue };
        let w = width(it);
        let g = match h {
          "fp_line" => (|| Some(Gfx::Line { a: flip(it.xy("start")?), b: flip(it.xy("end")?), width: w }))(),
          "fp_rect" => {
            (|| Some(Gfx::Rect { a: flip(it.xy("start")?), b: flip(it.xy("end")?), width: w, fill: filled(it) }))()
          }
          "fp_circle" => (|| {
            let c = flip(it.xy("center")?);
            let e = flip(it.xy("end")?);
            Some(Gfx::Circle { c, r: crate::geom::dist(c, e), width: w, fill: filled(it) })
          })(),
          "fp_arc" => (|| {
            let (a, b) = (flip(it.xy("start")?), flip(it.xy("end")?));
            let m = flip(it.xy("mid")?);
            let (c, r) = circle3(a, m, b)?;
            let (st, en) = arc3_angles(c, a, m, b);
            let (st, en) = if en < st { (en, st) } else { (st, en) };
            Some(Gfx::Arc { c, r, start: st, end: en, width: w })
          })(),
          _ => (|| {
            let pts: Vec<P> = it.child("pts")?.all("xy").filter_map(|p| Some(flip([p.num(1)?, p.num(2)?]))).collect();
            Some(Gfx::Poly { pts, width: w, fill: filled(it) })
          })(),
        };
        if let Some(gfx_item) = g {
          gfx.push(FpGfx { layer, gfx: gfx_item });
        }
      }
      Some("model") => {
        let xyz = |head: &str, def: f64| -> [f64; 3] {
          it.child(head)
            .and_then(|c| c.child("xyz"))
            .map(|x| [x.num(1).unwrap_or(def), x.num(2).unwrap_or(def), x.num(3).unwrap_or(def)])
            .unwrap_or([def; 3])
        };
        if let Some(path) = it.arg(1) {
          let off = xyz("offset", 0.0);
          model = Some(Model3d::File {
            path: path.to_string(),
            offset: [off[0], off[1], off[2]],
            rotate: xyz("rotate", 0.0),
            scale: xyz("scale", 1.0),
          });
        }
      }
      _ => {}
    }
  }
  Ok(Footprint { name, pads, gfx, model })
}

/// A `.pretty` folder (or any folder) of `.kicad_mod` files as one library.
pub fn import_footprint_dir(dir: &std::path::Path, lib_name: &str) -> Result<Library, String> {
  let mut lib = Library { name: lib_name.into(), ..Default::default() };
  let mut entries: Vec<_> = std::fs::read_dir(dir)
    .map_err(|e| format!("{}: {e}", dir.display()))?
    .filter_map(Result::ok)
    .map(|e| e.path())
    .collect();
  entries.sort();
  for p in entries.into_iter().filter(|p| p.extension().is_some_and(|e| e == "kicad_mod")) {
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    lib.footprints.push(import_footprint(&text).map_err(|e| format!("{}: {e}", p.display()))?);
  }
  if lib.footprints.is_empty() {
    return Err(format!("no .kicad_mod files in {}", dir.display()));
  }
  Ok(lib)
}
