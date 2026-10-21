//! Post-processors: G-code for a setup's toolpaths in the dialect of a machine control.
//!
//! A post is a small table of what its control wants (program number, a decimal point on whole
//! numbers, tool changes, tool length offsets, canned drilling cycles, how to send Z home) and
//! one writer for all of them, in millimetres or inches. With a 4/5-axis machine the moves go
//! through its kinematics: tilted (3+2) operations rotate the part once and cut in the machine's
//! frame; simultaneous ones move every axis together with inverse-time feeds (G93).

use std::fmt::Write;

use fox_kernel::glam::{DVec2, DVec3};

use super::axes::{Angles, Machine};
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
  /// Rotary axes (A, B, C) for 4/5-axis machines.
  pub rotary: bool,
  /// Safe start line (units are added).
  pub safe_start: &'static str,
  /// Extra setup lines after it.
  pub preamble: &'static [&'static str],
  /// How Z goes home before tool changes and at the end ("" = to the clearance height).
  pub home: &'static [&'static str],
  /// Program end.
  pub end: &'static str,
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
    rotary: true,
    safe_start: "G90 G94 G17 G40 G49 G80",
    preamble: &[],
    home: &["G28 G91 Z0.", "G90"],
    end: "M30",
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
    rotary: true,
    safe_start: "G00 G17 G40 G49 G80 G90",
    preamble: &[],
    home: &["G28 G91 Z0.", "G90"],
    end: "M30",
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
    rotary: false,
    safe_start: "G90 G94 G17",
    preamble: &[],
    home: &[],
    end: "M30",
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
    rotary: true,
    safe_start: "G90 G94 G17 G40 G49 G80",
    preamble: &["G64 P0.01"],
    home: &["G53 G0 Z0"],
    end: "M2",
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
    rotary: true,
    safe_start: "G90 G94 G17 G40 G49 G80",
    preamble: &[],
    home: &["G53 G0 Z0"],
    end: "M30",
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
  write_with(post, units, title, paths, origin, None)
}

/// [`write`], with cutting moves on circles written as G2/G3 arcs (in the XY plane, G17) when
/// `arcs` gives a tolerance (mm; see [`super::arcs`]).
pub fn write_with(
  post: &Post,
  units: Units,
  title: &str,
  paths: &[Toolpath],
  origin: DVec3,
  arcs: Option<f64>,
) -> String {
  let jobs: Vec<Job> =
    paths.iter().map(|tp| Job { tp: tp.clone(), angles: Vec::new(), together: false, travel: Vec::new() }).collect();
  write_jobs(post, units, title, &jobs, origin, arcs, None)
}

/// A toolpath ready to write: moved into the machine's frame, with the rotary positions of each
/// move (none on a 3-axis machine), whether they change as it cuts and how far the tip moves
/// over the part each move.
struct Job {
  tp: Toolpath,
  angles: Vec<Angles>,
  together: bool,
  travel: Vec<f64>,
}

/// [`write_with`] for a 4/5-axis machine: each move's tool axis through the machine's
/// kinematics, the rotary axes written beside X, Y and Z. Fails where the machine can't point
/// the tool.
pub fn write_machine(
  post: &Post,
  units: Units,
  title: &str,
  paths: &[Toolpath],
  origin: DVec3,
  arcs: Option<f64>,
  machine: Option<&Machine>,
) -> Result<String, String> {
  let Some(m) = machine else {
    return Ok(write_with(post, units, title, paths, origin, arcs));
  };
  let mut prev = Angles::default();
  let mut jobs = Vec::new();
  let mut tip: Option<DVec3> = None;
  for tp in paths {
    let mut angles = Vec::with_capacity(tp.moves.len());
    let mut travel = Vec::with_capacity(tp.moves.len());
    let mut out = tp.clone();
    for (i, mv) in tp.moves.iter().enumerate() {
      travel.push(tip.map_or(0.0, |t| (mv.to - t).length()));
      tip = Some(mv.to);
      let t = tp.axes.get(i).copied().unwrap_or(DVec3::Z);
      let a = m.solve(t, prev).ok_or_else(|| {
        format!("{}: the machine can't point the tool along ({:.3}, {:.3}, {:.3})", tp.name, t.x, t.y, t.z)
      })?;
      prev = a;
      out.moves[i].to = origin + m.to_machine(mv.to - origin, a);
      angles.push(a);
    }
    let first = angles.first().copied().unwrap_or(prev);
    let together = angles.iter().any(|a| (a.tilt - first.tilt).abs() > 1e-9 || (a.turn - first.turn).abs() > 1e-9);
    // drilling cycles run along the machine's Z only
    match &mut out.drill {
      Some((holes, _)) if !together && m.tool_along_z(first) => {
        for (top, _) in holes.iter_mut() {
          *top = origin + m.to_machine(*top - origin, first);
        }
      }
      _ => out.drill = None,
    }
    jobs.push(Job { tp: out, angles, together, travel });
  }
  Ok(write_jobs(post, units, title, &jobs, origin, arcs, Some(m)))
}

fn write_jobs(
  post: &Post,
  units: Units,
  title: &str,
  jobs: &[Job],
  origin: DVec3,
  arcs: Option<f64>,
  machine: Option<&Machine>,
) -> String {
  let paths: Vec<&Toolpath> = jobs.iter().map(|j| &j.tp).collect();
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
  match machine.map(|m| m.kinematics) {
    None => o.line("(WORK ORIGIN: STOCK TOP, MIN X/Y CORNER - G54)"),
    Some(super::axes::Kinematics::Rotary) => o.line("(WORK ORIGIN: ON THE A AXIS AT THE STOCK'S MIN X - G54)"),
    Some(_) => o.line("(WORK ORIGIN: STOCK BOTTOM CENTRE, WHERE THE ROTARY AXES CROSS - G54)"),
  }
  o.line(&format!("{} {}", post.safe_start, if units == Units::Metric { "G21" } else { "G20" }));
  for l in post.preamble {
    o.line(l);
  }
  o.line("G54");
  let mut tool = None;
  // where the tool is, for the first arc of a toolpath
  let mut at: Option<DVec3> = None;
  let mut rotary: Option<Angles> = None;
  let axis_words = |o: &Out, a: Angles| -> String {
    let Some(m) = machine else { return String::new() };
    let (tilt, turn) = m.kinematics.letters();
    let mut w = format!(" {tilt}{}", o.fmt(a.tilt, 3));
    if let Some(c) = turn {
      w += &format!(" {c}{}", o.fmt(a.turn, 3));
    }
    w
  };
  for job in jobs {
    let tp = &job.tp;
    o.line(&format!("({} - {})", comment(&tp.name), comment(&tp.tool.describe())));
    if tool != Some(tp.tool.number) {
      o.line(&o.code('M', 5));
      o.home(safe);
      at = None;
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
    // turn the part (or the head) to the toolpath's first orientation, clear of it
    if let Some(first) = job.angles.first().copied()
      && rotary.is_none_or(|r| (r.tilt - first.tilt).abs() > 1e-9 || (r.turn - first.turn).abs() > 1e-9)
    {
      if rotary.is_some() {
        o.home(safe);
      }
      let w = axis_words(&o, first);
      o.line(&format!("{}{w}", o.code('G', 0)));
      rotary = Some(first);
      at = None;
    }
    let local = |p: DVec3| p - origin;
    if job.together {
      // every axis together, at inverse-time feeds
      let mut from = at.or(tp.moves.first().map(|m| m.to)).unwrap_or(origin);
      let mut inverse = false;
      for (i, m) in tp.moves.iter().enumerate() {
        let p = local(m.to);
        let w = axis_words(&o, job.angles[i]);
        let (x, y, z) = (o.num(p.x), o.num(p.y), o.num(p.z));
        if m.rapid {
          if inverse {
            o.line(&o.code('G', 94));
            inverse = false;
          }
          o.line(&format!("{} X{x} Y{y} Z{z}{w}", o.code('G', 0)));
        } else {
          if !inverse {
            o.line(&o.code('G', 93));
            inverse = true;
          }
          // the tip's travel on the part, or the machine's where the tip stands still
          let len = job.travel.get(i).copied().unwrap_or(0.0).max((m.to - from).length()).max(0.01);
          o.line(&format!("{} X{x} Y{y} Z{z}{w} F{}", o.code('G', 1), o.fmt(m.feed / len, 3)));
        }
        from = m.to;
      }
      if inverse {
        o.line(&o.code('G', 94));
      }
      rotary = job.angles.last().copied();
      at = None;
      continue;
    }
    let arcs = arcs.filter(|_| machine.is_none_or(|m| job.angles.first().is_none_or(|a| m.tool_along_z(*a))));
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
      at = None;
      continue;
    }
    let mut last_feed = -1.0;
    let segments = match arcs {
      Some(tol) => super::arcs::fit(&tp.moves, at, tol),
      None => tp.moves.iter().map(|m| super::arcs::Segment::Move(*m)).collect(),
    };
    at = tp.moves.last().map(|m| m.to).or(at);
    for seg in segments {
      let m = match seg {
        super::arcs::Segment::Move(m) => m,
        super::arcs::Segment::Arc(a) => {
          let (p, c) = (local(a.to), a.centre - a.from.truncate());
          let mut l = format!(
            "{} X{} Y{} Z{} I{} J{}",
            o.code('G', if a.ccw { 3 } else { 2 }),
            o.num(p.x),
            o.num(p.y),
            o.num(p.z),
            o.num(c.x),
            o.num(c.y)
          );
          if (a.feed - last_feed).abs() > 1e-9 {
            l += &format!(" F{}", o.feed(a.feed));
            last_feed = a.feed;
          }
          o.line(&l);
          continue;
        }
      };
      let p = local(m.to);
      let (x, y, z) = (o.num(p.x), o.num(p.y), o.num(p.z));
      if m.rapid {
        o.line(&format!("{} X{x} Y{y} Z{z}", o.code('G', 0)));
      } else if (m.feed - last_feed).abs() > 1e-9 {
        o.line(&format!("{} X{x} Y{y} Z{z} F{}", o.code('G', 1), o.feed(m.feed)));
        last_feed = m.feed;
      } else {
        o.line(&format!("{} X{x} Y{y} Z{z}", o.code('G', 1)));
      }
    }
  }
  o.line(&o.code('M', 5));
  o.home(safe);
  if rotary.is_some() {
    // the rotary axes back to zero
    let w = axis_words(&o, Angles::default());
    o.line(&format!("{}{w}", o.code('G', 0)));
  }
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
        20 | 30 => motion = *g,
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
    if motion == 20 || motion == 30 {
      // arcs in the XY plane, centre relative to the start (I, J); read as short moves
      if get('R').is_some() {
        return Err(format!("line {}: R arcs aren't read", n + 1));
      }
      let centre = pos.truncate() + DVec2::new(get('I').unwrap_or(0.0), get('J').unwrap_or(0.0)) * scale;
      let r0 = (pos.truncate() - centre).length();
      let r1 = (to.truncate() - centre).length();
      if (r0 - r1).abs() > 0.006 + 1e-3 * r0 {
        return Err(format!("line {}: the arc's end isn't on its circle ({r0} vs {r1})", n + 1));
      }
      let arc = super::arcs::Arc { from: pos, to, centre, ccw: motion == 30, feed, moves: 0 };
      for q in arc.points(2f64.to_radians()) {
        out.push((false, q, feed));
      }
      pos = to;
      continue;
    }
    pos = to;
    out.push((motion == 0, pos, feed));
  }
  Ok(out)
}
