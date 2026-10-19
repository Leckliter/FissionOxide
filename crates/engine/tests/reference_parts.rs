//! Builds every reference part in `tests/reference/` from its `part.jsonl` and compares the result
//! with its `expected.json` (see `tests/reference/README.md`).
//!
//! Fails when a case listed in `tests/reference/baseline.txt` stops passing. Set
//! `FOX_REFERENCE_REPORT=1` to rewrite `docs/reference-parts.md`, and `FOX_REFERENCE=name` to run
//! the cases whose directory name contains `name`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use fox_engine::Session;
use fox_kernel::glam::DVec3;
use serde_json::{Value, json};

fn root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

enum Outcome {
  /// No `part.jsonl` yet.
  NotPorted,
  Pass,
  /// What went wrong: a command error, a feature error or measurements out of tolerance.
  Fail(String),
}

struct Case {
  name: String,
  outcome: Outcome,
  /// Measured values, for the report.
  measured: Option<Measured>,
  seconds: f64,
}

#[derive(Debug, Clone)]
struct Measured {
  solids: usize,
  volume: f64,
  area: f64,
  center: DVec3,
  bbox: (DVec3, DVec3),
  faces: usize,
}

/// `{"cmd": name, ...params}` or `{"command": name, "params": {...}}`, as `fissionoxide-cli run` reads.
fn parse_line(line: &str) -> Result<(String, Value), String> {
  let v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
  let obj = v.as_object().ok_or("expected a JSON object")?;
  if let Some(c) = obj.get("command").and_then(Value::as_str) {
    return Ok((c.to_string(), obj.get("params").cloned().unwrap_or(json!({}))));
  }
  let c = obj.get("cmd").and_then(Value::as_str).ok_or("missing \"cmd\"")?.to_string();
  let mut params = obj.clone();
  params.remove("cmd");
  Ok((c, Value::Object(params)))
}

fn build(script: &str) -> Result<Session, String> {
  let mut s = Session::default();
  for (n, line) in script.lines().enumerate() {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
      continue;
    }
    let (cmd, params) = parse_line(line).map_err(|e| format!("line {}: {e}", n + 1))?;
    s.execute(&cmd, &params).map_err(|e| format!("line {} {cmd}: {}", n + 1, e.0))?;
  }
  let errors = s.model.errors();
  if !errors.is_empty() {
    return Err(format!("feature errors: {errors:?}"));
  }
  Ok(s)
}

/// Totals over the visible bodies' solids.
fn measure(s: &Session) -> Result<Measured, String> {
  let mut m = Measured {
    solids: 0,
    volume: 0.0,
    area: 0.0,
    center: DVec3::ZERO,
    bbox: (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
    faces: 0,
  };
  for b in s.model.bodies.iter().filter(|b| s.body_visible(b.id)) {
    let solids = b.shape.solid_count();
    if solids == 0 {
      continue;
    }
    if !b.shape.is_valid() {
      return Err(format!("body {} is not a valid solid", b.name));
    }
    let p = b.shape.mass().map_err(|e| format!("mass of {}: {e}", b.name))?;
    m.solids += solids;
    m.volume += p.volume;
    m.area += p.area;
    m.center += p.center_of_mass * p.volume;
    m.bbox.0 = m.bbox.0.min(p.bbox_min);
    m.bbox.1 = m.bbox.1.max(p.bbox_max);
    m.faces += b.faces().len();
  }
  if m.volume > 0.0 {
    m.center /= m.volume;
  }
  Ok(m)
}

fn vec3(v: &Value) -> Option<DVec3> {
  let a = v.as_array()?;
  Some(DVec3::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?))
}

/// Differences between the measured part and `expected.json`; empty when it matches.
fn compare(m: &Measured, e: &Value) -> Vec<String> {
  let only: Option<Vec<&str>> = e["compare"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect());
  let check = |key: &str| only.as_ref().is_none_or(|k| k.contains(&key)) && !e[key].is_null();
  let rel = e["tolerance"]["rel"].as_f64().unwrap_or(1e-3);
  let (emin, emax) = (vec3(&e["bbox"]["min"]), vec3(&e["bbox"]["max"]));
  let diag = match (emin, emax) {
    (Some(a), Some(b)) => (b - a).length(),
    _ => (m.bbox.1 - m.bbox.0).length(),
  };
  let abs = e["tolerance"]["abs_mm"].as_f64().unwrap_or(1e-3 * diag);
  let mut out = Vec::new();
  if check("body_count") && e["body_count"].as_u64() != Some(m.solids as u64) {
    out.push(format!("solids {} (expected {})", m.solids, e["body_count"]));
  }
  for (key, got) in [("volume_mm3", m.volume), ("area_mm2", m.area)] {
    if let Some(want) = e[key].as_f64().filter(|_| check(key))
      && (got - want).abs() > rel * want.abs().max(1e-9)
    {
      out.push(format!("{key} {got:.3} (expected {want:.3}, {:+.3}%)", 100.0 * (got - want) / want));
    }
  }
  if let Some(want) = vec3(&e["center_of_mass"]).filter(|_| check("center_of_mass"))
    && (m.center - want).abs().max_element() > abs
  {
    out.push(format!("centre of mass {:.3?} (expected {:.3?})", m.center.to_array(), want.to_array()));
  }
  if check("bbox")
    && let (Some(a), Some(b)) = (emin, emax)
    && ((m.bbox.0 - a).abs().max_element() > abs || (m.bbox.1 - b).abs().max_element() > abs)
  {
    out.push(format!(
      "bbox {:.3?}..{:.3?} (expected {:.3?}..{:.3?})",
      m.bbox.0.to_array(),
      m.bbox.1.to_array(),
      a.to_array(),
      b.to_array()
    ));
  }
  if check("faces") && e["faces"].as_u64() != Some(m.faces as u64) {
    out.push(format!("faces {} (expected {})", m.faces, e["faces"]));
  }
  out
}

/// Writes the visible solids to STEP, reads them back and checks the volume survives.
fn step_round_trip(s: &Session, m: &Measured, dir: &Path) -> Result<(), String> {
  let shapes: Vec<_> =
    s.model.bodies.iter().filter(|b| s.body_visible(b.id) && b.shape.solid_count() > 0).map(|b| &b.shape).collect();
  let path = dir.join("part.step");
  fox_kernel::write_step(shapes, &path).map_err(|e| format!("STEP export: {e}"))?;
  let back = fox_kernel::read_step(&path).map_err(|e| format!("STEP import: {e}"))?;
  let volume: f64 = back.iter().map(|b| b.mass().map(|p| p.volume).unwrap_or(0.0)).sum();
  if (volume - m.volume).abs() > 1e-4 * m.volume.max(1.0) {
    return Err(format!("STEP round trip changed the volume: {volume:.3} (was {:.3})", m.volume));
  }
  Ok(())
}

fn run_case(dir: &Path, scratch: &Path) -> Case {
  let name = dir.file_name().unwrap().to_string_lossy().into_owned();
  let start = Instant::now();
  let mut case = Case { name, outcome: Outcome::NotPorted, measured: None, seconds: 0.0 };
  let Ok(script) = std::fs::read_to_string(dir.join("part.jsonl")) else {
    return case;
  };
  let expected: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
  let built = std::panic::catch_unwind(|| build(&script)).unwrap_or_else(|_| Err("panicked".into()));
  case.outcome = match built.and_then(|s| measure(&s).map(|m| (s, m))) {
    Err(e) => Outcome::Fail(e),
    Ok((s, m)) => {
      let mut diffs = compare(&m, &expected);
      let out = scratch.join(&case.name);
      std::fs::create_dir_all(&out).unwrap();
      if let Err(e) = step_round_trip(&s, &m, &out) {
        diffs.push(e);
      }
      case.measured = Some(m);
      if diffs.is_empty() { Outcome::Pass } else { Outcome::Fail(diffs.join("; ")) }
    }
  };
  case.seconds = start.elapsed().as_secs_f64();
  case
}

fn report(cases: &[Case]) -> String {
  let pass = cases.iter().filter(|c| matches!(c.outcome, Outcome::Pass)).count();
  let ported = cases.iter().filter(|c| !matches!(c.outcome, Outcome::NotPorted)).count();
  let mut s = String::new();
  writeln!(s, "# Reference parts\n").unwrap();
  writeln!(
    s,
    "Generated by `FOX_REFERENCE_REPORT=1 cargo test -p fox-engine --test reference_parts`; see\n\
     [tests/reference/README.md](../tests/reference/README.md).\n\n\
     **{pass} of {} pass** ({ported} have a FissionOxide script).\n",
    cases.len()
  )
  .unwrap();
  writeln!(s, "| Case | Result | Volume mm³ | Area mm² | Time | Notes |\n|---|---|---|---|---|---|").unwrap();
  for c in cases {
    let (result, notes) = match &c.outcome {
      Outcome::NotPorted => ("not ported", String::new()),
      Outcome::Pass => ("pass", String::new()),
      Outcome::Fail(e) => ("**fail**", e.replace('|', "\\|")),
    };
    let (v, a) = c
      .measured
      .as_ref()
      .map_or((String::new(), String::new()), |m| (format!("{:.1}", m.volume), format!("{:.1}", m.area)));
    let t = if matches!(c.outcome, Outcome::NotPorted) { String::new() } else { format!("{:.1} s", c.seconds) };
    writeln!(s, "| {} | {result} | {v} | {a} | {t} | {notes} |", c.name).unwrap();
  }
  writeln!(s, "\nEdit checks on these parts: [reference-variants.md](reference-variants.md).").unwrap();
  s
}

#[test]
fn reference_parts() {
  let root = root();
  let filter = std::env::var("FOX_REFERENCE").ok();
  let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.join("tests/reference"))
    .unwrap()
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.join("expected.json").is_file())
    .filter(|p| filter.as_ref().is_none_or(|f| p.file_name().unwrap().to_string_lossy().contains(f.as_str())))
    .collect();
  dirs.sort();
  let scratch = std::env::temp_dir().join(format!("fox-reference-{}", std::process::id()));
  let cases: Vec<Case> = dirs.iter().map(|d| run_case(d, &scratch)).collect();
  let _ = std::fs::remove_dir_all(&scratch);

  for c in &cases {
    match &c.outcome {
      Outcome::NotPorted => {}
      Outcome::Pass => println!("pass  {} ({:.1} s)", c.name, c.seconds),
      Outcome::Fail(e) => println!("FAIL  {}: {e}", c.name),
    }
  }
  if std::env::var_os("FOX_REFERENCE_REPORT").is_some() && filter.is_none() {
    std::fs::write(root.join("docs/reference-parts.md"), report(&cases)).unwrap();
  }

  let baseline = std::fs::read_to_string(root.join("tests/reference/baseline.txt")).unwrap_or_default();
  let baseline: Vec<&str> = baseline.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
  let passing = |name: &str| cases.iter().any(|c| c.name == name && matches!(c.outcome, Outcome::Pass));
  let ran = |name: &str| cases.iter().any(|c| c.name == name);
  let regressed: Vec<&str> = baseline.iter().copied().filter(|n| ran(n) && !passing(n)).collect();
  let new: Vec<&str> = cases
    .iter()
    .filter(|c| matches!(c.outcome, Outcome::Pass) && !baseline.contains(&c.name.as_str()))
    .map(|c| c.name.as_str())
    .collect();
  if !new.is_empty() {
    println!("\nNow passing; add to tests/reference/baseline.txt:\n{}", new.join("\n"));
  }
  assert!(regressed.is_empty(), "reference parts that used to pass now fail: {regressed:?}");
}

// Edit checks: `variants.json` next to a case lists edits to make to the built part and what the
// edited part must measure (see `tests/reference/README.md`). Each variant edits the existing
// model, which regenerates incrementally; the result must also match a from-scratch regeneration
// of the edited design, and undoing the edits must bring the original part back.

struct VariantCase {
  /// `case/variant`.
  name: String,
  outcome: Outcome,
  measured: Option<Measured>,
  seconds: f64,
}

/// One edit command (`{"cmd": name, ...params}`, as in `part.jsonl`).
fn run_edit(s: &mut Session, edit: &Value) -> Result<(), String> {
  let (cmd, params) = parse_line(&edit.to_string())?;
  s.execute(&cmd, &params).map(|_| ()).map_err(|e| format!("{cmd}: {}", e.0))
}

/// Differences in volume, area, solid and face counts between two measurements of the same part.
fn same_part(a: &Measured, b: &Measured, what: &str) -> Option<String> {
  let close = |x: f64, y: f64| (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0);
  if a.solids == b.solids && a.faces == b.faces && close(a.volume, b.volume) && close(a.area, b.area) {
    return None;
  }
  Some(format!(
    "{what}: volume {:.3}, area {:.3}, {} solids, {} faces (expected {:.3}, {:.3}, {}, {})",
    b.volume, b.area, b.solids, b.faces, a.volume, a.area, a.solids, a.faces
  ))
}

/// Applies one variant's edits to the built part `s`, checks the result, then rolls the edits back.
fn run_variant(s: &mut Session, base: &Measured, v: &Value) -> (Outcome, Option<Measured>) {
  let mut diffs = Vec::new();
  let mut measured = None;
  s.begin_transaction().unwrap();
  let edited = v["edits"].as_array().into_iter().flatten().try_for_each(|e| run_edit(s, e));
  match edited.and_then(|_| {
    let errors = s.model.errors();
    if errors.is_empty() { measure(s) } else { Err(format!("feature errors: {errors:?}")) }
  }) {
    Err(e) => diffs.push(e),
    Ok(m) => {
      diffs.extend(compare(&m, &v["expected"]));
      // The incremental regeneration must agree with regenerating the edited design from scratch.
      let fresh = Session::new(s.design.clone());
      match measure(&fresh) {
        Ok(f) => diffs.extend(same_part(&m, &f, "regenerated from scratch")),
        Err(e) => diffs.push(format!("regenerated from scratch: {e}")),
      }
      measured = Some(m);
    }
  }
  s.rollback_transaction().unwrap();
  s.edit_sketch = None;
  match measure(s) {
    Ok(m) => diffs.extend(same_part(base, &m, "after undoing the edits")),
    Err(e) => diffs.push(format!("after undoing the edits: {e}")),
  }
  (if diffs.is_empty() { Outcome::Pass } else { Outcome::Fail(diffs.join("; ")) }, measured)
}

/// Builds the case once and runs each of its variants on it.
fn run_variants(dir: &Path) -> Vec<VariantCase> {
  let case = dir.file_name().unwrap().to_string_lossy().into_owned();
  let (Ok(doc), Ok(script)) =
    (std::fs::read_to_string(dir.join("variants.json")), std::fs::read_to_string(dir.join("part.jsonl")))
  else {
    return Vec::new();
  };
  let doc: Value = serde_json::from_str(&doc).unwrap_or_else(|e| panic!("{case}/variants.json: {e}"));
  let variants = doc["variants"].as_array().cloned().unwrap_or_default();
  let built = std::panic::catch_unwind(|| build(&script).and_then(|s| measure(&s).map(|m| (s, m))))
    .unwrap_or_else(|_| Err("panicked".into()));
  let mut built = built.map_err(|e| format!("the part itself fails: {e}"));
  variants
    .iter()
    .map(|v| {
      let name = format!("{case}/{}", v["name"].as_str().unwrap_or("?"));
      let start = Instant::now();
      let (outcome, measured) = match &mut built {
        Err(e) => (Outcome::Fail(e.clone()), None),
        Ok((s, base)) => {
          let base = base.clone();
          std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_variant(s, &base, v)))
            .unwrap_or_else(|_| (Outcome::Fail("panicked".into()), None))
        }
      };
      VariantCase { name, outcome, measured, seconds: start.elapsed().as_secs_f64() }
    })
    .collect()
}

fn variant_report(cases: &[VariantCase]) -> String {
  let pass = cases.iter().filter(|c| matches!(c.outcome, Outcome::Pass)).count();
  let mut s = String::new();
  writeln!(s, "# Reference part edit checks\n").unwrap();
  writeln!(
    s,
    "Generated by `FOX_REFERENCE_REPORT=1 cargo test -p fox-engine --test reference_parts`; see\n\
     [tests/reference/README.md](../tests/reference/README.md). Each variant edits a built reference\n\
     part (its `variants.json`), and the regenerated part must match independently computed\n\
     measurements, a from-scratch regeneration of the edited design, and, once the edits are undone,\n\
     the original part.\n\n\
     **{pass} of {} pass.**\n",
    cases.len()
  )
  .unwrap();
  writeln!(s, "| Variant | Result | Volume mm³ | Area mm² | Time | Notes |\n|---|---|---|---|---|---|").unwrap();
  for c in cases {
    let (result, notes) = match &c.outcome {
      Outcome::NotPorted => ("not ported", String::new()),
      Outcome::Pass => ("pass", String::new()),
      Outcome::Fail(e) => ("**fail**", e.replace('|', "\\|")),
    };
    let (v, a) = c
      .measured
      .as_ref()
      .map_or((String::new(), String::new()), |m| (format!("{:.1}", m.volume), format!("{:.1}", m.area)));
    writeln!(s, "| {} | {result} | {v} | {a} | {:.1} s | {notes} |", c.name, c.seconds).unwrap();
  }
  s
}

/// Runs every case's `variants.json`. Fails when a variant listed in
/// `tests/reference/variant-baseline.txt` stops passing.
#[test]
fn reference_variants() {
  let root = root();
  let filter = std::env::var("FOX_REFERENCE").ok();
  let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.join("tests/reference"))
    .unwrap()
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.join("variants.json").is_file())
    .filter(|p| filter.as_ref().is_none_or(|f| p.file_name().unwrap().to_string_lossy().contains(f.as_str())))
    .collect();
  dirs.sort();
  let cases: Vec<VariantCase> = dirs.iter().flat_map(|d| run_variants(d)).collect();

  for c in &cases {
    match &c.outcome {
      Outcome::NotPorted => {}
      Outcome::Pass => println!("pass  {} ({:.1} s)", c.name, c.seconds),
      Outcome::Fail(e) => println!("FAIL  {}: {e}", c.name),
    }
  }
  if std::env::var_os("FOX_REFERENCE_REPORT").is_some() && filter.is_none() {
    std::fs::write(root.join("docs/reference-variants.md"), variant_report(&cases)).unwrap();
  }

  let baseline = std::fs::read_to_string(root.join("tests/reference/variant-baseline.txt")).unwrap_or_default();
  let baseline: Vec<&str> = baseline.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
  let passing = |name: &str| cases.iter().any(|c| c.name == name && matches!(c.outcome, Outcome::Pass));
  let ran = |name: &str| cases.iter().any(|c| c.name == name);
  let regressed: Vec<&str> = baseline.iter().copied().filter(|n| ran(n) && !passing(n)).collect();
  let new: Vec<&str> = cases
    .iter()
    .filter(|c| matches!(c.outcome, Outcome::Pass) && !baseline.contains(&c.name.as_str()))
    .map(|c| c.name.as_str())
    .collect();
  if !new.is_empty() {
    println!("\nNow passing; add to tests/reference/variant-baseline.txt:\n{}", new.join("\n"));
  }
  assert!(regressed.is_empty(), "reference part variants that used to pass now fail: {regressed:?}");
}
