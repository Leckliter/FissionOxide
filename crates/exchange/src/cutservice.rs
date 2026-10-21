//! Cut-service checks and cut estimates: what an online cutting service (SendCutSend, OSH Cut,
//! Ponoko, your own shop) looks at before it accepts a cut file, and the numbers it quotes by.
//!
//! A service profile ([`CutService`]) is data: its processes (laser, waterjet, router, plasma),
//! each with its minimum feature size, the part sizes it takes and the materials it cuts. The
//! built-in profiles ([`builtin_services`]) carry approximate limits taken from the shops'
//! published guidelines; the shop's own material pages win when they differ.
//!
//! [`check`] runs the flat-pattern checks of [`crate::flatcheck`] against the service's limits
//! and adds what a service rejects files for: text that isn't outlines, profiles that overlap
//! (holes inside a part are fine), parts bigger or smaller than it cuts, and a drawing whose size
//! suggests the wrong units. [`estimate`] measures each part: area, cut length, pierces, holes
//! and size.

use serde::{Deserialize, Serialize};

use crate::dxf::Entity;
use crate::flatcheck::{self, IssueKind, Layered, P2, Role, Rules, role};

const IN: f64 = 25.4;

/// How a service cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Process {
  #[default]
  Laser,
  Waterjet,
  Router,
  Plasma,
}

impl Process {
  pub fn parse(s: &str) -> Option<Process> {
    Some(match s.trim().to_ascii_lowercase().replace([' ', '-', '_'], "").as_str() {
      "laser" => Process::Laser,
      "waterjet" | "water" => Process::Waterjet,
      "router" | "cncrouter" | "cnc" => Process::Router,
      "plasma" => Process::Plasma,
      _ => return None,
    })
  }

  /// The usual smallest hole or web (mm) for material `thickness` thick: laser half the
  /// thickness, waterjet 0.070", router 0.125" (the tool), plasma the thickness.
  pub fn default_min_feature(self, thickness: f64) -> f64 {
    match self {
      Process::Laser => 0.5 * thickness,
      Process::Waterjet => 0.070 * IN,
      Process::Router => 0.125 * IN,
      Process::Plasma => thickness,
    }
  }
}

/// One way a service cuts, with its limits (millimetres).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceProcess {
  /// Shown in lists, e.g. "Laser (metal)"; defaults to the process.
  #[serde(default)]
  pub name: String,
  #[serde(default)]
  pub process: Process,
  /// The smallest hole, slot or web as a multiple of the thickness.
  #[serde(default)]
  pub min_feature_ratio: f64,
  /// The smallest hole, slot or web whatever the thickness. With neither set, the process's
  /// usual minimum ([`Process::default_min_feature`]) applies.
  #[serde(default)]
  pub min_feature: f64,
  /// The largest part (either way round); [0, 0] = no limit.
  #[serde(default)]
  pub max_part: P2,
  /// The smallest part (either way round) the service cuts without losing it.
  #[serde(default)]
  pub min_part: P2,
  /// Words in a material's name this process is for ("steel", "acrylic"); empty = any.
  #[serde(default)]
  pub materials: Vec<String>,
}

impl ServiceProcess {
  pub fn label(&self) -> String {
    if self.name.trim().is_empty() { format!("{:?}", self.process).to_ascii_lowercase() } else { self.name.clone() }
  }

  /// The smallest feature (mm) in material `thickness` thick.
  pub fn min_feature_for(&self, thickness: f64) -> f64 {
    if self.min_feature <= 0.0 && self.min_feature_ratio <= 0.0 {
      return self.process.default_min_feature(thickness);
    }
    self.min_feature.max(self.min_feature_ratio * thickness)
  }
}

/// A cutting service's profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutService {
  pub id: String,
  pub name: String,
  /// Where its design guidelines are.
  #[serde(default)]
  pub url: String,
  #[serde(default)]
  pub notes: String,
  /// The limits are estimates, not the shop's own numbers for each material.
  #[serde(default)]
  pub approximate: bool,
  /// The service wants one part per file.
  #[serde(default)]
  pub one_part_per_file: bool,
  /// The first process is the default.
  pub processes: Vec<ServiceProcess>,
}

impl CutService {
  /// The process to check against: the one named (its name or kind), else the first one for
  /// `material`, else the first that cuts any material, else the first.
  pub fn process_for(&self, material: Option<&str>, process: Option<&str>) -> Result<&ServiceProcess, String> {
    if let Some(name) = process.map(str::trim).filter(|p| !p.is_empty()) {
      let kind = Process::parse(name);
      return self
        .processes
        .iter()
        .find(|p| p.label().eq_ignore_ascii_case(name))
        .or_else(|| self.processes.iter().find(|p| Some(p.process) == kind))
        .ok_or_else(|| {
          let all: Vec<String> = self.processes.iter().map(ServiceProcess::label).collect();
          format!("{} has no process '{name}' ({})", self.name, all.join(", "))
        });
    }
    let m = material.map(str::to_ascii_lowercase).unwrap_or_default();
    if let Some(p) = (!m.trim().is_empty())
      .then(|| self.processes.iter().find(|p| p.materials.iter().any(|w| m.contains(&w.to_ascii_lowercase()))))
      .flatten()
    {
      return Ok(p);
    }
    self
      .processes
      .iter()
      .find(|p| p.materials.is_empty())
      .or_else(|| self.processes.first())
      .ok_or_else(|| format!("{} has no processes", self.name))
  }
}

fn inches(w: f64, h: f64) -> P2 {
  [w * IN, h * IN]
}

fn words(w: &[&str]) -> Vec<String> {
  w.iter().map(|s| s.to_string()).collect()
}

/// The built-in service profiles. Their limits are approximate (taken from each shop's general
/// guidelines in 2026; most shops set them per material and thickness), so each is marked
/// `approximate` and errs on the strict side.
pub fn builtin_services() -> Vec<CutService> {
  let metals = words(&["steel", "alumin", "stainless", "brass", "copper", "titanium", "galvan", "chromoly", "metal"]);
  let plastics_wood = words(&[
    "wood",
    "plywood",
    "mdf",
    "hardboard",
    "acrylic",
    "polycarbonate",
    "hdpe",
    "uhmw",
    "abs",
    "delrin",
    "acetal",
    "g10",
    "carbon fiber",
    "foam",
  ]);
  vec![
    CutService {
      id: "sendcutsend".into(),
      name: "SendCutSend".into(),
      url: "https://sendcutsend.com/guidelines/".into(),
      notes: "Approximate: laser features at least half the thickness, waterjet 0.070\", router 0.125\"; \
        parts about 1\" × 1\" to 44\" × 30\". Each material page lists its own limits."
        .into(),
      approximate: true,
      one_part_per_file: true,
      processes: vec![
        ServiceProcess {
          name: "Laser".into(),
          process: Process::Laser,
          min_feature_ratio: 0.5,
          min_feature: 0.010 * IN,
          max_part: inches(44.0, 30.0),
          min_part: inches(1.0, 1.0),
          materials: metals.clone(),
        },
        ServiceProcess {
          name: "CNC Router".into(),
          process: Process::Router,
          min_feature_ratio: 0.0,
          min_feature: 0.125 * IN,
          max_part: inches(44.0, 30.0),
          min_part: inches(1.0, 1.0),
          materials: plastics_wood.clone(),
        },
        ServiceProcess {
          name: "Waterjet".into(),
          process: Process::Waterjet,
          min_feature_ratio: 0.0,
          min_feature: 0.070 * IN,
          max_part: inches(44.0, 30.0),
          min_part: inches(1.0, 1.0),
          materials: words(&["waterjet"]),
        },
      ],
    },
    CutService {
      id: "oshcut".into(),
      name: "OSH Cut".into(),
      url: "https://www.oshcut.com/design-guide".into(),
      notes: "Approximate (no general limits published): laser features at least half the thickness, \
        parts 0.5\" × 0.5\" to 48\" × 96\". The quote page checks each part."
        .into(),
      approximate: true,
      one_part_per_file: false,
      processes: vec![ServiceProcess {
        name: "Laser".into(),
        process: Process::Laser,
        min_feature_ratio: 0.5,
        min_feature: 0.010 * IN,
        max_part: inches(48.0, 96.0),
        min_part: inches(0.5, 0.5),
        materials: Vec::new(),
      }],
    },
    CutService {
      id: "ponoko".into(),
      name: "Ponoko".into(),
      url: "https://help.ponoko.com/en/articles/9358110-ponoko-manufacturing-standards".into(),
      notes: "Approximate: features at least 1 mm (metal: 1.5 × thickness), parts 6 mm to 790 × 384 mm \
        (the largest sheet; many materials come smaller)."
        .into(),
      approximate: true,
      one_part_per_file: false,
      processes: vec![
        ServiceProcess {
          name: "Laser".into(),
          process: Process::Laser,
          min_feature_ratio: 0.5,
          min_feature: 1.0,
          max_part: [790.0, 384.0],
          min_part: [6.0, 6.0],
          materials: Vec::new(),
        },
        ServiceProcess {
          name: "Laser (metal)".into(),
          process: Process::Laser,
          min_feature_ratio: 1.5,
          min_feature: 1.0,
          max_part: [790.0, 384.0],
          min_part: [6.0, 6.0],
          materials: metals,
        },
      ],
    },
  ]
}

// ---------------------------------------------------------------- estimate

/// One part (an outer contour with the holes in it).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartEstimate {
  /// 1-based, in drawing order.
  pub part: usize,
  /// Material left: the outline's area less its holes'.
  pub area_mm2: f64,
  pub area_in2: f64,
  /// The outline's and the holes' lengths.
  pub cut_length_mm: f64,
  pub cut_length_in: f64,
  /// One per closed contour.
  pub pierces: usize,
  pub holes: usize,
  pub min: P2,
  pub max: P2,
  pub size_mm: P2,
  pub size_in: P2,
}

/// What a cut file measures, for quoting.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Estimate {
  pub parts: Vec<PartEstimate>,
  /// Separate pieces that come off the sheet (outer contours).
  pub pieces: usize,
  pub holes: usize,
  /// Closed contours, and one per open chain.
  pub pierces: usize,
  pub closed_contours: usize,
  pub open_chains: usize,
  pub area_mm2: f64,
  pub area_in2: f64,
  /// Everything on cut layers, open chains included.
  pub cut_length_mm: f64,
  pub cut_length_in: f64,
  pub min: P2,
  pub max: P2,
  pub size_mm: P2,
  pub size_in: P2,
}

fn poly_length(pts: &[P2]) -> f64 {
  pts.windows(2).map(|w| (w[0][0] - w[1][0]).hypot(w[0][1] - w[1][1])).sum()
}

fn bounds(pts: &[P2]) -> (P2, P2) {
  pts.iter().fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), p| {
    ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
  })
}

fn size(lo: P2, hi: P2) -> P2 {
  if lo[0] > hi[0] { [0.0, 0.0] } else { [hi[0] - lo[0], hi[1] - lo[1]] }
}

/// The cut-layer entities of a layered drawing.
fn cut_entities(l: &Layered) -> Vec<&Entity> {
  l.entities.iter().filter(|(layer, _)| role(layer) == Role::Cut).map(|(_, e)| e).collect()
}

/// Closed loops of the cut entities with their nesting: (loops, depth, parent of each).
struct Nest {
  loops: Vec<Vec<P2>>,
  depth: Vec<usize>,
  parent: Vec<Option<usize>>,
  open_ends: usize,
  open_length: f64,
}

fn nest(l: &Layered, tol: f64) -> Nest {
  let cut = cut_entities(l);
  let found = flatcheck::cut_loops(&cut, tol);
  let loops = found.loops;
  let depth = flatcheck::depths(&loops);
  let parent = (0..loops.len())
    .map(|i| {
      (depth[i] > 0)
        .then(|| {
          (0..loops.len())
            .find(|&j| j != i && depth[j] + 1 == depth[i] && loops[i].iter().all(|p| flatcheck::inside(&loops[j], *p)))
        })
        .flatten()
    })
    .collect();
  Nest { loops, depth, parent, open_ends: found.open.len(), open_length: found.open_length }
}

/// Measures the parts of a cut drawing: area, cut length, pierces, holes and size.
pub fn estimate(l: &Layered) -> Estimate {
  estimate_nest(&nest(l, 1e-3))
}

fn estimate_nest(n: &Nest) -> Estimate {
  let mut parts = Vec::new();
  let mut holes_total = 0;
  for i in (0..n.loops.len()).filter(|&i| n.depth[i].is_multiple_of(2)) {
    let holes: Vec<usize> = (0..n.loops.len()).filter(|&j| n.parent[j] == Some(i)).collect();
    holes_total += holes.len();
    let area = flatcheck::area(&n.loops[i]) - holes.iter().map(|&j| flatcheck::area(&n.loops[j])).sum::<f64>();
    let length = poly_length(&n.loops[i]) + holes.iter().map(|&j| poly_length(&n.loops[j])).sum::<f64>();
    let (lo, hi) = bounds(&n.loops[i]);
    let sz = size(lo, hi);
    parts.push(PartEstimate {
      part: parts.len() + 1,
      area_mm2: area,
      area_in2: area / (IN * IN),
      cut_length_mm: length,
      cut_length_in: length / IN,
      pierces: 1 + holes.len(),
      holes: holes.len(),
      min: lo,
      max: hi,
      size_mm: sz,
      size_in: [sz[0] / IN, sz[1] / IN],
    });
  }
  let (lo, hi) = bounds(&n.loops.iter().flatten().copied().collect::<Vec<_>>());
  let sz = size(lo, hi);
  let open_chains = n.open_ends.div_ceil(2);
  let cut = n.loops.iter().map(|l| poly_length(l)).sum::<f64>() + n.open_length;
  let area: f64 = parts.iter().map(|p| p.area_mm2).sum();
  Estimate {
    pieces: parts.len(),
    holes: holes_total,
    pierces: n.loops.len() + open_chains,
    closed_contours: n.loops.len(),
    open_chains,
    area_mm2: area,
    area_in2: area / (IN * IN),
    cut_length_mm: cut,
    cut_length_in: cut / IN,
    min: if sz == [0.0, 0.0] { [0.0, 0.0] } else { lo },
    max: if sz == [0.0, 0.0] { [0.0, 0.0] } else { hi },
    size_mm: sz,
    size_in: [sz[0] / IN, sz[1] / IN],
    parts,
  }
}

/// The estimate as CSV: one row per part, then the totals.
pub fn estimate_csv(e: &Estimate) -> String {
  let mut s = String::from(
    "part,width_in,height_in,width_mm,height_mm,area_in2,area_mm2,cut_length_in,cut_length_mm,pierces,holes\n",
  );
  let row = |s: &mut String, name: &str, sz: P2, a: f64, l: f64, pierces: usize, holes: usize| {
    s.push_str(&format!(
      "{name},{:.4},{:.4},{:.3},{:.3},{:.4},{:.2},{:.4},{:.3},{pierces},{holes}\n",
      sz[0] / IN,
      sz[1] / IN,
      sz[0],
      sz[1],
      a / (IN * IN),
      a,
      l / IN,
      l
    ));
  };
  for p in &e.parts {
    row(&mut s, &p.part.to_string(), p.size_mm, p.area_mm2, p.cut_length_mm, p.pierces, p.holes);
  }
  row(&mut s, "total", e.size_mm, e.area_mm2, e.cut_length_mm, e.pierces, e.holes);
  s
}

// ---------------------------------------------------------------- check

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
  Error,
  Warning,
  Info,
}

/// Something the check found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
  pub severity: Severity,
  /// text, open_contour, branch, duplicate, overlap, no_outline, small_feature, thin_web,
  /// hole_near_bend, too_big, too_small, scale, several_parts, loose_piece, notes, thickness
  pub kind: &'static str,
  pub message: String,
  /// Where, in the drawing's coordinates (mm).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub at: Option<P2>,
}

/// PASS, WARNINGS or ERRORS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
  Pass,
  Warnings,
  Errors,
}

/// What [`check`] needs besides the drawing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CheckOptions {
  /// Material thickness (mm).
  pub thickness: f64,
  /// Inside bend radius (mm); the thickness when not known.
  pub bend_radius: Option<f64>,
  /// Text on notes layers is FissionOxide's own (bend and sheet notes): leave it out.
  pub own_notes: bool,
  pub material: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServiceReport {
  pub status: Status,
  pub service: String,
  pub service_name: String,
  pub process: String,
  pub approximate: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub material: Option<String>,
  pub thickness: f64,
  /// The smallest feature the service cuts in this thickness (mm).
  pub min_feature: f64,
  /// The narrowest contour in the drawing (mm).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub smallest_feature: Option<f64>,
  pub errors: usize,
  pub warnings: usize,
  /// Most severe first.
  pub issues: Vec<Finding>,
  pub estimate: Estimate,
}

/// Whether a part `sz` fits within `limit` either way round.
fn fits(sz: P2, limit: P2) -> bool {
  (sz[0] <= limit[0] + 1e-6 && sz[1] <= limit[1] + 1e-6) || (sz[0] <= limit[1] + 1e-6 && sz[1] <= limit[0] + 1e-6)
}

fn fmt_size(sz: P2) -> String {
  format!("{:.3}\" × {:.3}\" ({:.1} × {:.1} mm)", sz[0] / IN, sz[1] / IN, sz[0], sz[1])
}

/// Checks a cut drawing against a service's process.
pub fn check(l: &Layered, service: &CutService, process: &ServiceProcess, o: &CheckOptions) -> ServiceReport {
  let tol = 1e-3;
  let t = o.thickness;
  let min_feature = process.min_feature_for(t);
  let mut issues: Vec<Finding> = Vec::new();
  let mut add = |severity, kind, at: Option<P2>, message: String| issues.push(Finding { severity, kind, message, at });

  // text: a service cuts outlines, never TEXT entities
  let mut notes = 0;
  for (layer, text) in &l.texts {
    if role(layer) == Role::Note {
      notes += 1;
    } else {
      add(Severity::Error, "text", None, format!("text '{text}' is a text entity: convert it to outlines"));
    }
  }
  if notes > 0 && !o.own_notes {
    add(Severity::Warning, "notes", None, format!("{notes} note(s) on a notes layer won't be cut: remove them"));
  }
  if t <= 0.0 {
    add(Severity::Warning, "thickness", None, "no material thickness: the minimum feature size is a guess".into());
  }

  // the flat-pattern checks against the service's minimum
  let rules = Rules { min_feature, hole_to_bend: 2.0 * t + o.bend_radius.unwrap_or(t), tol };
  let base = flatcheck::check(&Layered { entities: l.entities.clone(), texts: Vec::new() }, rules);
  let n = nest(l, tol);

  // profiles that cross or touch without one being inside the other
  let mut crossings: Vec<P2> = Vec::new();
  let boxes: Vec<(P2, P2)> = n.loops.iter().map(|l| bounds(l)).collect();
  let contains = |a: usize, b: usize| n.loops[b].iter().all(|p| flatcheck::inside(&n.loops[a], *p));
  for i in 0..n.loops.len() {
    for j in i + 1..n.loops.len() {
      let ((alo, ahi), (blo, bhi)) = (boxes[i], boxes[j]);
      if alo[0] > bhi[0] + tol || blo[0] > ahi[0] + tol || alo[1] > bhi[1] + tol || blo[1] > ahi[1] + tol {
        continue;
      }
      let (d, at) = flatcheck::poly_poly(&n.loops[i], &n.loops[j]);
      if d <= tol && !contains(i, j) && !contains(j, i) {
        crossings.push(at);
        add(Severity::Error, "overlap", Some(at), format!("profiles {} and {} overlap or touch", i + 1, j + 1));
      }
    }
  }
  for is in &base.issues {
    let (severity, kind) = match is.kind {
      IssueKind::NoOutline => (Severity::Error, "no_outline"),
      IssueKind::OpenContour => (Severity::Error, "open_contour"),
      IssueKind::Branch => (Severity::Error, "branch"),
      IssueKind::Duplicate => (Severity::Error, "duplicate"),
      IssueKind::SmallFeature => (Severity::Warning, "small_feature"),
      IssueKind::ThinWeb => {
        // an overlap is already an error
        if crossings.iter().any(|c| (c[0] - is.at[0]).hypot(c[1] - is.at[1]) < 1e-9) {
          continue;
        }
        (Severity::Warning, "thin_web")
      }
      IssueKind::HoleNearBend => (Severity::Warning, "hole_near_bend"),
      // counted below, by the service's rule
      IssueKind::SeveralParts => continue,
    };
    let message = match is.kind {
      IssueKind::OpenContour => "open contour: the curves here don't close into a profile".to_string(),
      IssueKind::Duplicate => "duplicate cut: two curves overlap here (the head would cut twice)".to_string(),
      _ => is.message.clone(),
    };
    add(severity, kind, Some(is.at), message);
  }

  let est = estimate_nest(&n);
  // part sizes
  let (lo_lim, hi_lim) = (process.min_part, process.max_part);
  for (p, i) in est.parts.iter().zip((0..n.loops.len()).filter(|&i| n.depth[i].is_multiple_of(2))) {
    let at = Some([(p.min[0] + p.max[0]) / 2.0, (p.min[1] + p.max[1]) / 2.0]);
    if hi_lim[0] > 0.0 && hi_lim[1] > 0.0 && !fits(p.size_mm, hi_lim) {
      add(
        Severity::Error,
        "too_big",
        at,
        format!(
          "part {} is {}: bigger than {}'s largest part {}",
          p.part,
          fmt_size(p.size_mm),
          service.name,
          fmt_size(hi_lim)
        ),
      );
    }
    if lo_lim[0] > 0.0
      && lo_lim[1] > 0.0
      && p.size_mm[0].max(p.size_mm[1]) < lo_lim[0].max(lo_lim[1]) - 1e-6
      && p.size_mm[0].min(p.size_mm[1]) < lo_lim[0].min(lo_lim[1]) - 1e-6
    {
      add(
        Severity::Warning,
        "too_small",
        at,
        format!(
          "part {} is {}: smaller than {}'s smallest part {}",
          p.part,
          fmt_size(p.size_mm),
          service.name,
          fmt_size(lo_lim)
        ),
      );
    }
    if n.depth[i] > 0 {
      add(
        Severity::Info,
        "loose_piece",
        at,
        format!("part {} sits inside a hole of another part and comes out loose", p.part),
      );
    }
  }
  if est.pieces > 1 {
    let severity = if service.one_part_per_file { Severity::Warning } else { Severity::Info };
    let rule =
      if service.one_part_per_file { format!(" ({} wants one part per file)", service.name) } else { String::new() };
    add(severity, "several_parts", None, format!("{} parts in the file{rule}", est.pieces));
  }
  // scale: under 1 mm or over 10 m across is almost always the wrong units
  let span = est.size_mm[0].max(est.size_mm[1]);
  if !n.loops.is_empty() && span < 1.0 {
    add(Severity::Warning, "scale", Some(est.min), format!("the drawing is only {span:.4} mm across: wrong units?"));
  } else if span > 10_000.0 {
    add(
      Severity::Warning,
      "scale",
      Some(est.min),
      format!("the drawing is {:.1} m across: wrong units?", span / 1000.0),
    );
  }

  issues.sort_by_key(|f| f.severity);
  let errors = issues.iter().filter(|f| f.severity == Severity::Error).count();
  let warnings = issues.iter().filter(|f| f.severity == Severity::Warning).count();
  let status = if errors > 0 {
    Status::Errors
  } else if warnings > 0 {
    Status::Warnings
  } else {
    Status::Pass
  };
  let smallest =
    n.loops.iter().map(|l| flatcheck::min_width(l)).fold(None, |m: Option<f64>, w| Some(m.map_or(w, |m| m.min(w))));
  ServiceReport {
    status,
    service: service.id.clone(),
    service_name: service.name.clone(),
    process: process.label(),
    approximate: service.approximate,
    material: o.material.clone(),
    thickness: t,
    min_feature,
    smallest_feature: smallest,
    errors,
    warnings,
    issues,
    estimate: est,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn line(a: P2, b: P2) -> (String, Entity) {
    ("0".into(), Entity::Line { a, b })
  }

  fn rect(lo: P2, hi: P2) -> Vec<(String, Entity)> {
    let (a, b, c, d) = (lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]);
    vec![line(a, b), line(b, c), line(c, d), line(d, a)]
  }

  fn circle(c: P2, r: f64) -> (String, Entity) {
    ("0".into(), Entity::Circle { center: c, radius: r })
  }

  fn scs() -> CutService {
    builtin_services().into_iter().find(|s| s.id == "sendcutsend").unwrap()
  }

  fn opts(t: f64) -> CheckOptions {
    CheckOptions { thickness: t, ..Default::default() }
  }

  fn kinds(r: &ServiceReport) -> Vec<&'static str> {
    r.issues.iter().map(|f| f.kind).collect()
  }

  #[test]
  fn a_plate_with_holes_passes_and_is_measured() {
    // 4" × 2" plate with two Ø0.25" holes: holes inside a part are not overlaps
    let mut entities = rect([0.0, 0.0], [4.0 * IN, 2.0 * IN]);
    entities.push(circle([IN, IN], 0.125 * IN));
    entities.push(circle([3.0 * IN, IN], 0.125 * IN));
    let l = Layered { entities, texts: Vec::new() };
    let s = scs();
    let r = check(&l, &s, s.process_for(Some("5052 Aluminum"), None).unwrap(), &opts(0.125 * IN));
    assert_eq!(r.status, Status::Pass, "{:?}", r.issues);
    assert_eq!(r.process, "Laser");
    let e = &r.estimate;
    assert_eq!((e.pieces, e.holes, e.pierces), (1, 2, 3));
    let hole = std::f64::consts::PI * 0.125 * 0.125;
    assert!((e.area_in2 - (8.0 - 2.0 * hole)).abs() < 2e-3, "{}", e.area_in2);
    let cut = 12.0 + 2.0 * std::f64::consts::PI * 0.25;
    assert!((e.cut_length_in - cut).abs() < 2e-3, "{}", e.cut_length_in);
    assert!((e.size_in[0] - 4.0).abs() < 1e-9 && (e.size_in[1] - 2.0).abs() < 1e-9);
    let csv = estimate_csv(e);
    assert!(csv.lines().count() == 3 && csv.contains("total,4.0000,2.0000"), "{csv}");
  }

  #[test]
  fn finds_what_a_service_rejects() {
    let s = scs();
    let laser = s.process_for(None, Some("laser")).unwrap();
    let base = || {
      let mut e = rect([0.0, 0.0], [100.0, 50.0]);
      e.push(circle([20.0, 25.0], 5.0));
      e
    };
    // a line drawn twice
    let mut e = base();
    e.push(line([100.0, 0.0], [0.0, 0.0]));
    let r = check(&Layered { entities: e, texts: Vec::new() }, &s, laser, &opts(3.0));
    assert!(kinds(&r).contains(&"duplicate"), "{:?}", kinds(&r));
    // text, an open line, overlapping profiles, a hole under the minimum
    let mut e = base();
    e.push(line([200.0, 0.0], [210.0, 5.0]));
    e.extend(rect([90.0, 40.0], [130.0, 60.0]));
    e.push(circle([60.0, 25.0], 0.5));
    let l = Layered { entities: e, texts: vec![("0".into(), "HELLO".into())] };
    let r = check(&l, &s, laser, &opts(3.0));
    let k = kinds(&r);
    assert_eq!(r.status, Status::Errors);
    for want in ["text", "open_contour", "overlap", "small_feature"] {
      assert!(k.contains(&want), "{want} missing from {k:?}");
    }
    assert!(!k.contains(&"thin_web"), "the overlap is reported once: {k:?}");
    assert!(r.issues.windows(2).all(|w| w[0].severity <= w[1].severity));
    // too big for the laser bed, and several parts in one file
    let mut e = rect([0.0, 0.0], [50.0 * IN, 10.0 * IN]);
    e.extend(rect([0.0, -100.0], [40.0, -60.0]));
    let r = check(&Layered { entities: e, texts: Vec::new() }, &s, laser, &opts(3.0));
    let k = kinds(&r);
    assert!(k.contains(&"too_big") && k.contains(&"several_parts"), "{k:?}");
    // a tiny drawing: wrong units, and too small
    let r = check(&Layered { entities: rect([0.0, 0.0], [0.5, 0.25]), texts: Vec::new() }, &s, laser, &opts(0.01));
    let k = kinds(&r);
    assert!(k.contains(&"scale") && k.contains(&"too_small"), "{k:?}");
    // a router minimum is the tool: 0.125"
    let router = s.process_for(Some("Birch plywood"), None).unwrap();
    assert_eq!(router.process, Process::Router);
    assert!((router.min_feature_for(6.0) - 3.175).abs() < 1e-9);
    let mut e = base();
    e.push(circle([60.0, 25.0], 1.0));
    let r = check(&Layered { entities: e, texts: Vec::new() }, &s, router, &opts(6.0));
    assert_eq!(kinds(&r), vec!["small_feature"]);
    assert_eq!(r.status, Status::Warnings);
  }

  #[test]
  fn process_defaults_and_choice() {
    assert!((Process::Laser.default_min_feature(3.0) - 1.5).abs() < 1e-12);
    assert!((Process::Waterjet.default_min_feature(3.0) - 1.778).abs() < 1e-9);
    let p = ServiceProcess {
      name: String::new(),
      process: Process::Waterjet,
      min_feature_ratio: 0.0,
      min_feature: 0.0,
      max_part: [0.0, 0.0],
      min_part: [0.0, 0.0],
      materials: Vec::new(),
    };
    assert_eq!(p.label(), "waterjet");
    assert!((p.min_feature_for(10.0) - 1.778).abs() < 1e-9);
    let ponoko = builtin_services().into_iter().find(|s| s.id == "ponoko").unwrap();
    assert_eq!(ponoko.process_for(Some("Stainless steel 304"), None).unwrap().label(), "Laser (metal)");
    assert_eq!(ponoko.process_for(Some("Acrylic"), None).unwrap().label(), "Laser");
    assert!(ponoko.process_for(None, Some("router")).is_err());
    let json = serde_json::to_string(&ponoko).unwrap();
    let back: CutService = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ponoko);
  }
}
