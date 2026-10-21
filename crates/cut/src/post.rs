//! Machine profiles and posts: G-code from line templates, or an R12 DXF.
//!
//! A [`Machine`] is data, so users can add their own: header, pierce, beam-off and footer lines
//! plus one template each for a rapid, a line and the two arcs. Templates fill `{name}`
//! placeholders; a line starting with `?var ` is written only when `var` is not zero (a pierce
//! dwell only when there is a pierce delay). Placeholders:
//!
//! - anywhere: `{name}` (program name), `{units}` (G21/G20), `{feed}`, `{power}` (S value),
//!   `{power_pct}`, `{pierce_delay}` (s), `{pierce_delay_ms}`, `{pierce_height}`,
//!   `{cut_height}`, `{safe_height}`, `{kerf}`, `{material}`
//! - in moves: `{x}`, `{y}`, `{i}`, `{j}` (arc centre from the start), and `{f}`, which is
//!   " F<feed>" when the feed changes and empty otherwise.
//!
//! A machine also has any number of [`Process`]es: named cut settings (material, thickness).

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::plan::{CutParams, Lead, LeadKind, Plan, SegKind};
use crate::{P, Seg};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MachineKind {
  #[default]
  Laser,
  Plasma,
  Waterjet,
  /// Writes the cut paths as an R12 DXF instead of G-code.
  Dxf,
}

/// Named cut settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Process {
  pub name: String,
  #[serde(flatten)]
  pub params: CutParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Machine {
  pub id: String,
  pub name: String,
  #[serde(default)]
  pub kind: MachineKind,
  #[serde(default = "ext")]
  pub extension: String,
  /// G2/G3 arcs; without them arcs are written as short lines.
  #[serde(default = "yes")]
  pub arcs: bool,
  /// Chord tolerance when arcs become lines (mm).
  #[serde(default = "arc_tol")]
  pub arc_tolerance: f64,
  /// S value for full laser power.
  #[serde(default = "max_power")]
  pub max_power: f64,
  /// Rapid rate for time estimates (mm/min).
  #[serde(default = "rapid_rate")]
  pub rapid_rate: f64,
  #[serde(default = "decimals")]
  pub decimals: usize,
  #[serde(default)]
  pub header: Vec<String>,
  /// Turn on at the start of a pass (pierce).
  #[serde(default)]
  pub pierce: Vec<String>,
  /// Turn off at the end of a pass.
  #[serde(default)]
  pub off: Vec<String>,
  #[serde(default)]
  pub footer: Vec<String>,
  #[serde(default = "rapid_t")]
  pub rapid: String,
  #[serde(default = "line_t")]
  pub line: String,
  #[serde(default = "cw_t")]
  pub arc_cw: String,
  #[serde(default = "ccw_t")]
  pub arc_ccw: String,
  /// Cut settings to choose from; the first is the default.
  #[serde(default)]
  pub processes: Vec<Process>,
}

fn ext() -> String {
  "nc".into()
}
fn yes() -> bool {
  true
}
fn arc_tol() -> f64 {
  0.01
}
fn max_power() -> f64 {
  1000.0
}
fn rapid_rate() -> f64 {
  6000.0
}
fn decimals() -> usize {
  3
}
fn rapid_t() -> String {
  "G0 X{x} Y{y}".into()
}
fn line_t() -> String {
  "G1 X{x} Y{y}{f}".into()
}
fn cw_t() -> String {
  "G2 X{x} Y{y} I{i} J{j}{f}".into()
}
fn ccw_t() -> String {
  "G3 X{x} Y{y} I{i} J{j}{f}".into()
}

impl Machine {
  /// Settings of a process by name (case-insensitive) or 0-based index; None = the first.
  pub fn process(&self, name: Option<&str>) -> Result<CutParams, String> {
    let Some(n) = name else {
      return Ok(self.processes.first().map(|p| p.params.clone()).unwrap_or_default());
    };
    if let Ok(i) = n.trim().parse::<usize>()
      && let Some(p) = self.processes.get(i)
    {
      return Ok(p.params.clone());
    }
    self.processes.iter().find(|p| p.name.eq_ignore_ascii_case(n.trim())).map(|p| p.params.clone()).ok_or_else(|| {
      format!(
        "machine '{}' has no process '{n}' ({})",
        self.id,
        self.processes.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
      )
    })
  }

  /// The program for a plan: G-code, or DXF for a DXF machine.
  pub fn output(&self, plan: &Plan, params: &CutParams, name: &str, imperial: bool) -> String {
    match self.kind {
      MachineKind::Dxf => dxf(plan),
      _ => self.gcode(plan, params, name, imperial),
    }
  }

  pub fn gcode(&self, plan: &Plan, params: &CutParams, name: &str, imperial: bool) -> String {
    let k = if imperial { 1.0 / 25.4 } else { 1.0 };
    let d = self.decimals;
    let num = |v: f64| fmt(v, d);
    let power = params.power.clamp(0.0, 100.0) / 100.0 * self.max_power;
    let mut vars: Vec<(&str, String)> = vec![
      ("name", name.to_string()),
      ("units", if imperial { "G20" } else { "G21" }.to_string()),
      ("feed", num(params.feed * k)),
      ("power", num(power)),
      ("power_pct", num(params.power)),
      ("pierce_delay", num(params.pierce_delay)),
      ("pierce_delay_ms", num(params.pierce_delay * 1000.0)),
      ("pierce_height", num(params.pierce_height * k)),
      ("cut_height", num(params.cut_height * k)),
      ("safe_height", num(params.safe_height * k)),
      ("kerf", num(params.kerf * k)),
      ("material", params.material.to_string()),
    ];
    let mut out = String::new();
    let block = |out: &mut String, lines: &[String], vars: &[(&str, String)]| {
      for l in lines {
        if let Some(s) = fill(l, vars) {
          out.push_str(&s);
          out.push('\n');
        }
      }
    };
    block(&mut out, &self.header, &vars);
    let mut last_feed: Option<String> = None;
    for pass in &plan.passes {
      let feed = num(params.feed * pass.feed_scale * k);
      if let Some(v) = vars.iter_mut().find(|v| v.0 == "feed") {
        v.1 = feed.clone();
      }
      let s = pass.start();
      let mv = |t: &str, p: P, extra: &[(&str, String)]| {
        let mut v: Vec<(&str, String)> = vec![("x", num(p[0] * k)), ("y", num(p[1] * k))];
        v.extend(extra.iter().cloned());
        fill(t, &v).unwrap_or_default()
      };
      let _ = writeln!(out, "{}", mv(&self.rapid, s, &[]));
      block(&mut out, &self.pierce, &vars);
      last_feed = last_feed.filter(|_| self.pierce.iter().all(|l| !l.contains('F')));
      for (seg, _) in &pass.segs {
        let mut f = String::new();
        if last_feed.as_deref() != Some(feed.as_str()) {
          f = format!(" F{feed}");
          last_feed = Some(feed.clone());
        }
        if seg.is_line() || dist2(seg) < 1e-8 {
          let _ = writeln!(out, "{}", mv(&self.line, seg.b, &[("f", f)]));
        } else if self.arcs {
          let (c, _) = seg.center_radius();
          let t = if seg.bulge < 0.0 { &self.arc_cw } else { &self.arc_ccw };
          let extra = [("i", num((c[0] - seg.a[0]) * k)), ("j", num((c[1] - seg.a[1]) * k)), ("f", f)];
          let _ = writeln!(out, "{}", mv(t, seg.b, &extra));
        } else {
          let mut pts = Vec::new();
          seg.flatten(self.arc_tolerance, &mut pts);
          for (n, p) in pts.into_iter().enumerate() {
            let f = if n == 0 { f.clone() } else { String::new() };
            let _ = writeln!(out, "{}", mv(&self.line, p, &[("f", f)]));
          }
        }
      }
      block(&mut out, &self.off, &vars);
      if self.off.iter().any(|l| l.contains('F')) {
        last_feed = None;
      }
    }
    block(&mut out, &self.footer, &vars);
    out
  }
}

fn dist2(s: &Seg) -> f64 {
  let d = crate::sub(s.b, s.a);
  d[0] * d[0] + d[1] * d[1]
}

/// A number with up to `d` decimals, no trailing zeros, never "-0".
pub fn fmt(v: f64, d: usize) -> String {
  let mut s = format!("{v:.d$}");
  if s.contains('.') {
    while s.ends_with('0') {
      s.pop();
    }
    if s.ends_with('.') {
      s.pop();
    }
  }
  if s == "-0" { "0".into() } else { s }
}

/// Fills `{var}` placeholders; None for a `?var ` line whose variable is zero or empty.
fn fill(t: &str, vars: &[(&str, String)]) -> Option<String> {
  let mut t = t;
  if let Some(rest) = t.strip_prefix('?') {
    let (var, line) = rest.split_once(' ').unwrap_or((rest, ""));
    let v = vars.iter().find(|v| v.0 == var).map(|v| v.1.as_str()).unwrap_or("");
    if v.is_empty() || v.parse::<f64>().is_ok_and(|x| x == 0.0) {
      return None;
    }
    t = line;
  }
  let mut s = t.to_string();
  for (k, v) in vars {
    s = s.replace(&format!("{{{k}}}"), v);
  }
  Some(s)
}

/// The plan's paths as an R12 DXF (LINE and ARC entities, millimetres): layer OUTER for part
/// outlines, INNER for holes, LEAD for lead-ins and lead-outs, SHEET for the sheet of a nest.
pub fn dxf(plan: &Plan) -> String {
  let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n");
  let all = [("OUTER", 7), ("INNER", 1), ("LEAD", 3), ("SHEET", 8)];
  let layers = if plan.sheet.is_some() { &all[..] } else { &all[..3] };
  let _ = write!(s, "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n{}\n", layers.len());
  for (name, color) in layers {
    let _ = write!(s, "0\nLAYER\n2\n{name}\n70\n0\n62\n{color}\n6\nCONTINUOUS\n");
  }
  s.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
  let f = |v: f64| fmt(v, 6);
  if let Some((lo, hi)) = plan.sheet {
    let c = [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]];
    for i in 0..4 {
      let (a, b) = (c[i], c[(i + 1) % 4]);
      let _ = write!(
        s,
        "0\nLINE\n8\nSHEET\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n",
        f(a[0]),
        f(a[1]),
        f(b[0]),
        f(b[1])
      );
    }
  }
  for pass in &plan.passes {
    for (seg, kind) in &pass.segs {
      let layer = match (kind, pass.hole) {
        (SegKind::Cut, false) => "OUTER",
        (SegKind::Cut, true) => "INNER",
        _ => "LEAD",
      };
      if seg.is_line() {
        let _ = write!(
          s,
          "0\nLINE\n8\n{layer}\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n",
          f(seg.a[0]),
          f(seg.a[1]),
          f(seg.b[0]),
          f(seg.b[1])
        );
      } else {
        let (c, r) = seg.center_radius();
        let ang = |p: P| (p[1] - c[1]).atan2(p[0] - c[0]).to_degrees();
        let (a0, a1) = if seg.bulge > 0.0 { (ang(seg.a), ang(seg.b)) } else { (ang(seg.b), ang(seg.a)) };
        let _ = write!(
          s,
          "0\nARC\n8\n{layer}\n10\n{}\n20\n{}\n30\n0\n40\n{}\n50\n{}\n51\n{}\n",
          f(c[0]),
          f(c[1]),
          f(r),
          f(a0),
          f(a1)
        );
      }
    }
  }
  s.push_str("0\nENDSEC\n0\nEOF\n");
  s
}

fn lines(v: &[&str]) -> Vec<String> {
  v.iter().map(|s| s.to_string()).collect()
}

fn process(name: &str, p: CutParams) -> Process {
  Process { name: name.into(), params: p }
}

fn laser(id: &str, name: &str, control: &str) -> Machine {
  let base = CutParams {
    kerf: 0.15,
    feed: 600.0,
    power: 80.0,
    pierce_delay: 0.0,
    lead_in: Lead { kind: LeadKind::Arc, length: 1.0 },
    lead_out: Lead { kind: LeadKind::None, length: 1.0 },
    ..CutParams::default()
  };
  Machine {
    id: id.into(),
    name: name.into(),
    kind: MachineKind::Laser,
    extension: "gcode".into(),
    arcs: true,
    arc_tolerance: arc_tol(),
    max_power: 1000.0,
    rapid_rate: 6000.0,
    decimals: 3,
    header: vec![
      "; {name}".into(),
      format!("; {control}: laser mode on ($32=1), S{{power}} = {{power_pct}} % of $30=1000"),
      "G90 G17 {units}".into(),
      "M5".into(),
    ],
    // M4 power follows the speed (no burning at corners); a dwell needs constant power (M3)
    pierce: lines(&["?pierce_delay M3 S{power}", "?pierce_delay G4 P{pierce_delay}", "M4 S{power}"]),
    off: lines(&["M5"]),
    footer: lines(&["M5 S0", "G0 X0 Y0", "M2"]),
    rapid: rapid_t(),
    line: line_t(),
    arc_cw: cw_t(),
    arc_ccw: ccw_t(),
    processes: vec![
      process("3 mm plywood", base.clone()),
      process("3 mm acrylic", CutParams { kerf: 0.12, feed: 300.0, power: 90.0, ..base.clone() }),
      process("Card, 1 pass", CutParams { kerf: 0.1, feed: 1500.0, power: 40.0, ..base }),
    ],
  }
}

/// The machines that ship with FissionOxide.
pub fn builtin_machines() -> Vec<Machine> {
  let plasma = CutParams {
    kerf: 1.5,
    feed: 2500.0,
    power: 100.0,
    pierce_delay: 0.5,
    pierce_height: 3.8,
    cut_height: 1.5,
    safe_height: 10.0,
    lead_in: Lead { kind: LeadKind::Arc, length: 4.0 },
    lead_out: Lead { kind: LeadKind::Line, length: 2.0 },
    small_hole_diameter: 32.0,
    small_hole_feed: 0.6,
    ..CutParams::default()
  };
  let plasma_processes = vec![
    process("3 mm mild steel, 45 A", plasma.clone()),
    process(
      "6 mm mild steel, 45 A",
      CutParams { kerf: 1.7, feed: 1300.0, pierce_delay: 0.8, pierce_height: 4.5, ..plasma.clone() },
    ),
    process("1.5 mm mild steel, 30 A", CutParams { kerf: 1.2, feed: 3800.0, pierce_delay: 0.2, ..plasma.clone() }),
  ];
  let water = CutParams {
    kerf: 1.0,
    feed: 120.0,
    pierce_delay: 8.0,
    lead_in: Lead { kind: LeadKind::Arc, length: 2.0 },
    lead_out: Lead { kind: LeadKind::None, length: 1.0 },
    ..CutParams::default()
  };
  vec![
    laser("grbl_laser", "GRBL laser (M4 dynamic power)", "GRBL"),
    laser("fluidnc_laser", "FluidNC laser (M4 dynamic power)", "FluidNC"),
    Machine {
      id: "qtplasmac".into(),
      name: "LinuxCNC QtPlasmaC plasma (torch height control)".into(),
      kind: MachineKind::Plasma,
      extension: "ngc".into(),
      arcs: true,
      arc_tolerance: arc_tol(),
      max_power: 1.0,
      rapid_rate: 10000.0,
      decimals: 3,
      // pierce height, pierce delay and cut height come from the QtPlasmaC material table
      header: lines(&[
        "; {name}",
        "; QtPlasmaC: heights and pierce delay from material {material} of the material table",
        "{units}",
        "G40 G49 G80 G90 G92.1 G94 G97",
        "G64 P0.1",
        "M52 P1",
        "?material M190 P{material}",
        "?material M66 P3 L3 Q1",
      ]),
      pierce: lines(&["M3 $0 S1"]),
      off: lines(&["M5 $0"]),
      footer: lines(&["G0 X0 Y0", "M2"]),
      rapid: rapid_t(),
      line: line_t(),
      arc_cw: cw_t(),
      arc_ccw: ccw_t(),
      processes: plasma_processes.clone(),
    },
    Machine {
      id: "plasma_z".into(),
      name: "Plasma with torch heights in the G-code (Mach-style THC)".into(),
      kind: MachineKind::Plasma,
      extension: "tap".into(),
      arcs: true,
      arc_tolerance: arc_tol(),
      max_power: 1.0,
      rapid_rate: 10000.0,
      decimals: 3,
      header: lines(&["({name})", "{units} G90 G17 G40", "G0 Z{safe_height}"]),
      pierce: lines(&["G0 Z{pierce_height}", "M3", "?pierce_delay G4 P{pierce_delay}", "G1 Z{cut_height} F{feed}"]),
      off: lines(&["M5", "G0 Z{safe_height}"]),
      footer: lines(&["G0 X0 Y0", "M30"]),
      rapid: rapid_t(),
      line: line_t(),
      arc_cw: cw_t(),
      arc_ccw: ccw_t(),
      processes: plasma_processes.clone(),
    },
    // written from Langmuir's published post output, not run on a Crossfire table yet
    Machine {
      id: "firecontrol".into(),
      name: "Langmuir FireControl plasma (Crossfire; untested)".into(),
      kind: MachineKind::Plasma,
      extension: "nc".into(),
      arcs: true,
      arc_tolerance: arc_tol(),
      max_power: 1.0,
      rapid_rate: 10000.0,
      decimals: 4,
      // pierce height, cut height and torch height control are set in FireControl itself
      header: lines(&[
        "({name})",
        "(FireControl: heights and THC from the FireControl settings; untested)",
        "G90 G94",
        "G17",
        "{units}",
      ]),
      pierce: lines(&["M3", "?pierce_delay G4 P{pierce_delay}"]),
      off: lines(&["M5"]),
      footer: lines(&["G0 X0 Y0", "M30"]),
      rapid: rapid_t(),
      line: line_t(),
      arc_cw: cw_t(),
      arc_ccw: ccw_t(),
      processes: plasma_processes,
    },
    Machine {
      id: "wazer".into(),
      name: "Waterjet, Wazer-style G-code (jet M3/M5, lines only)".into(),
      kind: MachineKind::Waterjet,
      extension: "gcode".into(),
      arcs: false,
      arc_tolerance: 0.02,
      max_power: 1.0,
      rapid_rate: 3000.0,
      decimals: 3,
      header: lines(&["({name})", "G90", "{units}"]),
      pierce: lines(&["M3", "?pierce_delay G4 P{pierce_delay}"]),
      off: lines(&["M5"]),
      footer: lines(&["G0 X0 Y0", "M2"]),
      rapid: rapid_t(),
      line: line_t(),
      arc_cw: cw_t(),
      arc_ccw: ccw_t(),
      processes: vec![
        process("3 mm aluminium", water.clone()),
        process("1.5 mm stainless steel", CutParams { feed: 70.0, pierce_delay: 10.0, ..water.clone() }),
        process("6 mm acrylic", CutParams { feed: 250.0, pierce_delay: 3.0, ..water }),
      ],
    },
    Machine {
      id: "dxf".into(),
      name: "R12 DXF (OMAX and other waterjet or laser software)".into(),
      kind: MachineKind::Dxf,
      extension: "dxf".into(),
      arcs: true,
      arc_tolerance: arc_tol(),
      max_power: 1.0,
      rapid_rate: 6000.0,
      decimals: 6,
      header: Vec::new(),
      pierce: Vec::new(),
      off: Vec::new(),
      footer: Vec::new(),
      rapid: rapid_t(),
      line: line_t(),
      arc_cw: cw_t(),
      arc_ccw: ccw_t(),
      // the receiving software compensates the kerf and adds its own leads
      processes: vec![process(
        "Part geometry",
        CutParams {
          kerf: 0.0,
          compensate: false,
          lead_in: Lead { kind: LeadKind::None, length: 1.0 },
          lead_out: Lead { kind: LeadKind::None, length: 1.0 },
          ..CutParams::default()
        },
      )],
    },
  ]
}

/// A built-in machine by id.
pub fn find(id: &str) -> Option<Machine> {
  builtin_machines().into_iter().find(|m| m.id.eq_ignore_ascii_case(id.trim()))
}
