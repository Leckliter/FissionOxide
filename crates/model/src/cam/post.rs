//! Post-processors: G-code for a setup's toolpaths in the dialect of a machine control.
//!
//! A post is a small table of what its control wants (program number, a decimal point on whole
//! numbers, tool changes, tool length offsets, canned drilling cycles, how to send Z home) and
//! one writer for all of them, in millimetres or inches. Circles in the toolpaths go out as G2/G3
//! arcs (see [`super::arcs`]).

use std::fmt::Write;

use fox_kernel::glam::DVec3;

use super::arcs::{self, Seg};
use super::{CLEARANCE, RETRACT, Toolpath};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
  Metric,
  Imperial,
}

impl Units {
  pub fn parse(s: &str) -> Option<Units> {
    match s.trim().to_ascii_lowercase().as_str() {
      "mm" | "metric" | "millimeters" | "millimetres" => Some(Units::Metric),
      "in" | "inch" | "inches" | "imperial" => Some(Units::Imperial),
      _ => None,
    }
  }

  fn scale(self) -> f64 {
    if self == Units::Imperial { 1.0 / 25.4 } else { 1.0 }
  }
}

/// What a control wants to see.
#[derive(Debug, PartialEq, Eq)]
pub struct Post {
  pub id: &'static str,
  pub name: &'static str,
  /// File extension for its programs.
  pub extension: &'static str,
  /// `%` on the first and last lines.
  pub percent: bool,
  /// An `O` program number after the opening `%`.
  pub program: bool,
  /// Whole numbers keep their decimal point ("X10."): Fanuc-style controls read "X10" as ten
  /// thousandths of a millimetre.
  pub decimal_point: bool,
  /// Two-digit M and G codes ("M06", "G00").
  pub two_digit: bool,
  /// Automatic tool changes (M6); without them the program stops (M0) for a manual change.
  pub tool_change: bool,
  /// Tool length offsets (G43 H).
  pub length_offset: bool,
  /// Canned drilling cycles (G81, G83); without them drilling is written out as moves.
  pub cycles: bool,
  /// Safe start line (units are added).
  pub safe_start: &'static str,
  /// Extra setup lines after it.
  pub preamble: &'static [&'static str],
  /// How Z goes home before tool changes and at the end ("" = to the clearance height).
  pub home: &'static [&'static str],
  /// Program end.
  pub end: &'static str,
  /// Circles go out as G2/G3 arcs (I/J centre offsets) instead of runs of short lines.
  pub arcs: bool,
}

pub static POSTS: [Post; 5] = [
  Post {
    id: "fanuc",
    name: "Fanuc",
    extension: "nc",
    percent: true,
    program: true,
    decimal_point: true,
    two_digit: false,
    tool_change: true,
    length_offset: true,
    cycles: true,
    safe_start: "G90 G94 G17 G40 G49 G80",
    preamble: &[],
    home: &["G28 G91 Z0.", "G90"],
    end: "M30",
    arcs: true,
  },
  Post {
    id: "haas",
    name: "Haas",
    extension: "nc",
    percent: true,
    program: true,
    decimal_point: true,
    two_digit: true,
    tool_change: true,
    length_offset: true,
    cycles: true,
    safe_start: "G00 G17 G40 G49 G80 G90",
    preamble: &[],
    home: &["G28 G91 Z0.", "G90"],
    end: "M30",
    arcs: true,
  },
  Post {
    id: "grbl",
    name: "GRBL",
    extension: "gcode",
    percent: false,
    program: false,
    decimal_point: false,
    two_digit: false,
    tool_change: false,
    length_offset: false,
    cycles: false,
    safe_start: "G90 G94 G17",
    preamble: &[],
    home: &[],
    end: "M30",
    arcs: true,
  },
  Post {
    id: "linuxcnc",
    name: "LinuxCNC",
    extension: "ngc",
    percent: false,
    program: false,
    decimal_point: false,
    two_digit: false,
    tool_change: true,
    length_offset: true,
    cycles: true,
    safe_start: "G90 G94 G17 G40 G49 G80",
    preamble: &["G64 P0.01"],
    home: &["G53 G0 Z0"],
    end: "M2",
    arcs: true,
  },
  Post {
    id: "mach3",
    name: "Mach3 / Mach4",
    extension: "tap",
    percent: false,
    program: false,
    decimal_point: false,
    two_digit: false,
    tool_change: true,
    length_offset: true,
    cycles: true,
    safe_start: "G90 G94 G17 G40 G49 G80",
    preamble: &[],
    home: &["G53 G0 Z0"],
    end: "M30",
    arcs: true,
  },
];

/// A post by id ("" = Fanuc); "mach4" is Mach3's.
pub fn find(id: &str) -> Option<&'static Post> {
  let id = id.trim().to_ascii_lowercase();
  let id = match id.as_str() {
    "" => "fanuc",
    "mach4" | "mach" => "mach3",
    "emc" | "emc2" => "linuxcnc",
    other => other,
  };
  POSTS.iter().find(|p| p.id == id)
}

pub fn ids() -> Vec<&'static str> {
  POSTS.iter().map(|p| p.id).collect()
}

/// Text for a comment: upper case, without the parentheses that would end it.
fn comment(s: &str) -> String {
  s.to_uppercase().replace(['(', ')'], "").replace(['\n', '\r'], " ")
}

struct Out<'a> {
  post: &'a Post,
  units: Units,
  g: String,
}

impl Out<'_> {
  fn line(&mut self, s: &str) {
    let _ = writeln!(self.g, "{s}");
  }

  /// A coordinate in the program's units.
  fn num(&self, v: f64) -> String {
    let places = if self.units == Units::Imperial { 4 } else { 3 };
    self.fmt(v * self.units.scale(), places)
  }

  fn feed(&self, v: f64) -> String {
    let places = if self.units == Units::Imperial { 2 } else { 1 };
    self.fmt(v * self.units.scale(), places)
  }

  fn fmt(&self, v: f64, places: usize) -> String {
    let s = format!("{:.*}", places, v);
    let mut s = s.trim_end_matches('0').to_string();
    if s == "-." || s == "-0." {
      s = "0.".into();
    }
    if !self.post.decimal_point {
      s = s.trim_end_matches('.').to_string();
    }
    s
  }

  fn code(&self, letter: char, n: u32) -> String {
    if self.post.two_digit { format!("{letter}{n:02}") } else { format!("{letter}{n}") }
  }

  fn home(&mut self, safe: f64) {
    if self.post.home.is_empty() {
      let l = format!("{} Z{}", self.code('G', 0), self.num(safe));
      self.line(&l);
    } else {
      for l in self.post.home {
        self.line(l);
      }
    }
  }
}

/// The G-code of toolpaths, relative to the work origin; `safe` is the clearance height in
/// work coordinates.
pub fn write(post: &Post, units: Units, title: &str, paths: &[Toolpath], origin: DVec3) -> String {
  let mut o = Out { post, units, g: String::new() };
  let safe = paths.iter().flat_map(|t| t.moves.iter()).map(|m| m.to.z - origin.z).fold(CLEARANCE, f64::max);
  if post.percent {
    o.line("%");
  }
  if post.program {
    o.line(&format!("O1000 ({})", comment(title)));
  } else {
    o.line(&format!("({})", comment(title)));
  }
  o.line(&format!(
    "(FISSIONOXIDE - {} POST, {})",
    comment(post.name),
    if units == Units::Metric { "MM" } else { "INCH" }
  ));
  o.line("(WORK ORIGIN: STOCK TOP, MIN X/Y CORNER - G54)");
  o.line(&format!("{} {}", post.safe_start, if units == Units::Metric { "G21" } else { "G20" }));
  for l in post.preamble {
    o.line(l);
  }
  o.line("G54");
  let mut tool = None;
  for tp in paths {
    o.line(&format!("({} - {})", comment(&tp.name), comment(&tp.tool.describe())));
    if tool != Some(tp.tool.number) {
      o.line(&o.code('M', 5));
      o.home(safe);
      if post.tool_change {
        o.line(&format!("T{} {}", tp.tool.number, o.code('M', 6)));
      } else {
        o.line(&format!("(CHANGE TO T{} - {})", tp.tool.number, comment(&tp.tool.describe())));
        o.line(&o.code('M', 0));
      }
      o.line(&format!("S{} {}", tp.tool.rpm.round() as i64, o.code('M', 3)));
      if post.length_offset {
        let h = if post.two_digit { format!("H{:02}", tp.tool.number) } else { format!("H{}", tp.tool.number) };
        o.line(&format!("{} {h} Z{}", o.code('G', 43), o.num(safe)));
      }
      tool = Some(tp.tool.number);
    }
    let local = |p: DVec3| p - origin;
    if post.cycles
      && let Some((holes, peck)) = &tp.drill
    {
      for (k, (top, depth)) in holes.iter().enumerate() {
        let t = local(*top);
        let (x, y, z, r) = (o.num(t.x), o.num(t.y), o.num(t.z - depth), o.num(t.z + RETRACT));
        if k == 0 {
          o.line(&format!("{} X{x} Y{y} Z{}", o.code('G', 0), o.num(safe)));
          let cycle = if *peck > 0.0 { format!("G83 Q{}", o.num(*peck)) } else { "G81".to_string() };
          o.line(&format!("G98 {cycle} X{x} Y{y} Z{z} R{r} F{}", o.feed(tp.tool.plunge)));
        } else {
          o.line(&format!("X{x} Y{y} Z{z} R{r}"));
        }
      }
      o.line(&o.code('G', 80));
      continue;
    }
    let mut last_feed = -1.0;
    let mut at: Option<DVec3> = None;
    let moves = &tp.moves;
    let mut k = 0;
    while k < moves.len() {
      if moves[k].rapid {
        let p = local(moves[k].to);
        o.line(&format!("{} X{} Y{} Z{}", o.code('G', 0), o.num(p.x), o.num(p.y), o.num(p.z)));
        at = Some(p);
        k += 1;
        continue;
      }
      // a run of cutting moves at one feed: circles in it go out as arcs
      let feed = moves[k].feed;
      let end =
        (k..moves.len()).find(|&e| moves[e].rapid || (moves[e].feed - feed).abs() > 1e-9).unwrap_or(moves.len());
      let points: Vec<DVec3> = moves[k..end].iter().map(|m| local(m.to)).collect();
      let segs = match at {
        Some(from) if post.arcs => arcs::fit(from, &points, arcs::FIT_TOL),
        _ => points.iter().map(|p| Seg::Line(*p)).collect(),
      };
      let mut from = at.unwrap_or(points[0]);
      for seg in segs {
        let f = if (feed - last_feed).abs() > 1e-9 { format!(" F{}", o.feed(feed)) } else { String::new() };
        last_feed = feed;
        match seg {
          Seg::Line(p) => {
            o.line(&format!("{} X{} Y{} Z{}{f}", o.code('G', 1), o.num(p.x), o.num(p.y), o.num(p.z)));
            from = p;
          }
          Seg::Arc { to, center, cw } => {
            let (i, j) = (center.x - from.x, center.y - from.y);
            let g = o.code('G', if cw { 2 } else { 3 });
            o.line(&format!("{g} X{} Y{} I{} J{}{f}", o.num(to.x), o.num(to.y), o.num(i), o.num(j)));
            from = to;
          }
        }
      }
      at = points.last().copied();
      k = end;
    }
  }
  o.line(&o.code('M', 5));
  o.home(safe);
  o.line(post.end);
  if post.percent {
    o.line("%");
  }
  o.g
}

/// A minimal G-code reader for checking programs: the tool tip's moves in millimetres, in work
/// coordinates, with canned cycles expanded. Returns (rapid, position, feed in mm/min).
pub fn read(g: &str) -> Result<Vec<(bool, DVec3, f64)>, String> {
  let mut out = Vec::new();
  let mut pos = DVec3::ZERO;
  let mut scale = 1.0;
  let mut motion = 0;
  let mut feed = 0.0;
  let mut cycle: Option<(u32, f64, f64, f64)> = None; // (kind, z, r, q)
  let mut absolute = true;
  for (n, raw) in g.lines().enumerate() {
    // strip comments
    let mut line = String::new();
    let mut depth = 0;
    for c in raw.chars() {
      match c {
        '(' => depth += 1,
        ')' => depth -= 1,
        ';' if depth == 0 => break,
        _ if depth == 0 => line.push(c),
        _ => {}
      }
    }
    let line = line.trim().to_ascii_uppercase();
    if line.is_empty() || line == "%" {
      continue;
    }
    let mut words: Vec<(char, f64)> = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
      if c.is_whitespace() {
        continue;
      }
      if !c.is_ascii_alphabetic() {
        return Err(format!("line {}: unexpected '{c}'", n + 1));
      }
      let mut v = String::new();
      while let Some(&d) = chars.peek() {
        if d.is_ascii_digit() || d == '.' || d == '-' || d == '+' {
          v.push(d);
          chars.next();
        } else {
          break;
        }
      }
      let v: f64 = v.parse().map_err(|_| format!("line {}: bad number after '{c}'", n + 1))?;
      words.push((c, v));
    }
    let get = |l: char| words.iter().find(|w| w.0 == l).map(|w| w.1);
    let gs: Vec<u32> = words.iter().filter(|w| w.0 == 'G').map(|w| (w.1 * 10.0).round() as u32).collect();
    let mut machine = false;
    let mut home = false;
    for g in &gs {
      match g {
        0 => motion = 0,
        10 => motion = 1,
        20 => motion = 2,
        30 => motion = 3,
        200 => scale = 25.4,
        210 => scale = 1.0,
        900 => absolute = true,
        910 => absolute = false,
        280 => home = true,
        530 => machine = true,
        800 => cycle = None,
        810 | 830 => {
          motion = *g;
          cycle = Some((*g, 0.0, 0.0, 0.0));
        }
        _ => {}
      }
    }
    if let Some(f) = get('F') {
      feed = f * scale;
    }
    if home || machine {
      // machine coordinate and homing moves go up out of the work: skip them
      continue;
    }
    if !absolute {
      return Err(format!("line {}: incremental moves aren't read", n + 1));
    }
    let (x, y, z) = (get('X'), get('Y'), get('Z'));
    if x.is_none() && y.is_none() && z.is_none() {
      continue;
    }
    if let Some(c) = cycle.as_mut().filter(|_| motion == 810 || motion == 830) {
      if let Some(z) = z {
        c.1 = z * scale;
      }
      if let Some(r) = get('R') {
        c.2 = r * scale;
      }
      if let Some(q) = get('Q') {
        c.3 = q * scale;
      }
      let (kind, zb, r, q) = *c;
      let xy = DVec3::new(x.map_or(pos.x, |v| v * scale), y.map_or(pos.y, |v| v * scale), pos.z);
      // G98: back to the starting height after each hole
      let start = pos.z;
      out.push((true, DVec3::new(xy.x, xy.y, start), 0.0));
      out.push((true, DVec3::new(xy.x, xy.y, r), 0.0));
      if kind == 830 && q > 0.0 {
        let mut at = r;
        while at > zb + 1e-9 {
          at = (at - q).max(zb);
          out.push((false, DVec3::new(xy.x, xy.y, at), feed));
          out.push((true, DVec3::new(xy.x, xy.y, r), 0.0));
        }
      } else {
        out.push((false, DVec3::new(xy.x, xy.y, zb), feed));
        out.push((true, DVec3::new(xy.x, xy.y, r), 0.0));
      }
      out.push((true, DVec3::new(xy.x, xy.y, start), 0.0));
      pos = DVec3::new(xy.x, xy.y, start);
      continue;
    }
    let to = DVec3::new(x.map_or(pos.x, |v| v * scale), y.map_or(pos.y, |v| v * scale), z.map_or(pos.z, |v| v * scale));
    if motion == 2 || motion == 3 {
      // an arc in XY around the start plus (I, J), as points no more than 2 degrees apart
      let c = DVec3::new(pos.x + get('I').unwrap_or(0.0) * scale, pos.y + get('J').unwrap_or(0.0) * scale, pos.z);
      let (a0, a1) = ((pos.y - c.y).atan2(pos.x - c.x), (to.y - c.y).atan2(to.x - c.x));
      let tau = std::f64::consts::TAU;
      let mut sweep = if motion == 3 { (a1 - a0).rem_euclid(tau) } else { -(a0 - a1).rem_euclid(tau) };
      if sweep.abs() < 1e-9 {
        sweep = if motion == 3 { tau } else { -tau };
      }
      let r = (pos - c).truncate().length();
      let n = (sweep.abs() / 2f64.to_radians()).ceil().max(1.0) as usize;
      for s in 1..n {
        let t = s as f64 / n as f64;
        let a = a0 + sweep * t;
        out.push((false, DVec3::new(c.x + r * a.cos(), c.y + r * a.sin(), pos.z + (to.z - pos.z) * t), feed));
      }
    }
    pos = to;
    out.push((motion == 0, pos, feed));
  }
  Ok(out)
}
