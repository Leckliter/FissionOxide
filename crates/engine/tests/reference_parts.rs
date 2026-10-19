//! Builds every reference part in `tests/reference/` from its `part.jsonl` and compares the result
//! with its `expected.json` (see `tests/reference/README.md`).
//!
//! Each passing part is also edited: its first extrusion's distance is changed in place and the
//! timeline rebuilt, which must give the same part as building the script with that distance
//! from scratch. A difference means a later feature lost track of the faces or edges it was on.
//!
//! Fails when a case listed in `tests/reference/baseline.txt` stops passing, or one listed in
//! `tests/reference/edit-baseline.txt` stops surviving the edit. Set
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
  /// The edit check: `None` when the part has no extrusion distance to change.
  edit: Option<Result<(), String>>,
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

/// Changes the first extrusion's distance (by +15%) in place and checks the rebuilt part:
/// against one built from scratch with that distance, or, where the script's later picks (points
/// on faces and edges) no longer land on the changed part, by checking the edit keeps every face
/// and that editing it back restores the part exactly. `None` when there is nothing to edit.
fn edit_check(script: &str, before: &Measured) -> Option<Result<(), String>> {
  let lines: Vec<&str> = script.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
  let parsed: Vec<(String, Value)> = lines.iter().map(|l| parse_line(l)).collect::<Result<_, _>>().ok()?;
  // the first extrusion with a plain distance that later commands build on
  let at = parsed.iter().position(|(c, p)| c == "extrude" && p["distance"].is_number())?;
  if at + 1 == parsed.len() {
    return None;
  }
  let d = parsed[at].1["distance"].as_f64()?;
  let new = (d * 1.15 * 1e4).round() / 1e4;

  // from scratch with the new distance, when the script's picks still land
  let mut fresh = parsed.clone();
  fresh[at].1["distance"] = json!(new);
  let script2: String = fresh.iter().map(|(c, p)| format!("{}\n", line_of(c, p))).collect();
  let want = build(&script2).ok().and_then(|s| measure(&s).ok());

  // built as it was, then edited
  let mut s = Session::default();
  let mut feature = None;
  for (i, (c, p)) in parsed.iter().enumerate() {
    let r = s.execute(c, p).ok()?;
    if i == at {
      feature = r["feature"].as_u64();
    }
  }
  let id = feature?;
  let kind = serde_json::to_value(&s.design.features.iter().find(|f| u64::from(f.id.0) == id)?.kind).ok()?;
  let set = |s: &mut Session, dist: f64| -> Result<Measured, String> {
    let mut extent = kind["extent"].clone();
    extent["distance"] = json!(dist.to_string());
    s.execute("feature.edit", &json!({"feature": id, "set": {"extent": extent}}))
      .map_err(|e| format!("the edit was refused: {}", e.0))?;
    let errors = s.model.errors();
    if !errors.is_empty() {
      return Err(format!("after the edit: {errors:?}"));
    }
    measure(s)
  };
  let what = format!("extrude distance {d} -> {new}");
  let got = match set(&mut s, new) {
    Ok(m) => m,
    Err(e) => return Some(Err(format!("{what}: {e}"))),
  };
  let mut diffs = Vec::new();
  match want {
    Some(want) => {
      if (want.volume - before.volume).abs() <= 1e-6 * before.volume {
        return None; // the change doesn't show in the part (a cut through all, say)
      }
      diffs.extend(differences(&got, &want, "built fresh"));
    }
    None => {
      if (got.volume - before.volume).abs() <= 1e-6 * before.volume {
        diffs.push("the edit didn't change the part".into());
      }
      if got.faces != before.faces {
        diffs.push(format!("faces {} (before the edit: {})", got.faces, before.faces));
      }
      match set(&mut s, d) {
        Ok(back) => {
          diffs.extend(differences(&back, before, "before the edits").into_iter().map(|e| format!("edited back: {e}")))
        }
        Err(e) => diffs.push(format!("edited back: {e}")),
      }
    }
  }
  Some(if diffs.is_empty() { Ok(()) } else { Err(format!("{what}: {}", diffs.join(", "))) })
}

fn differences(got: &Measured, want: &Measured, what: &str) -> Vec<String> {
  let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * b.abs().max(1.0);
  let mut diffs = Vec::new();
  if got.solids != want.solids {
    diffs.push(format!("solids {} ({what}: {})", got.solids, want.solids));
  }
  if !close(got.volume, want.volume) {
    diffs.push(format!("volume {:.3} ({what}: {:.3})", got.volume, want.volume));
  }
  if !close(got.area, want.area) {
    diffs.push(format!("area {:.3} ({what}: {:.3})", got.area, want.area));
  }
  if got.faces != want.faces {
    diffs.push(format!("faces {} ({what}: {})", got.faces, want.faces));
  }
  diffs
}

fn line_of(cmd: &str, params: &Value) -> String {
  let mut v = params.clone();
  v["cmd"] = json!(cmd);
  v.to_string()
}

fn run_case(dir: &Path, scratch: &Path) -> Case {
  let name = dir.file_name().unwrap().to_string_lossy().into_owned();
  let start = Instant::now();
  let mut case = Case { name, outcome: Outcome::NotPorted, measured: None, seconds: 0.0, edit: None };
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
      if diffs.is_empty() {
        case.edit = std::panic::catch_unwind(|| edit_check(&script, case.measured.as_ref().unwrap()))
          .unwrap_or_else(|_| Some(Err("panicked".into())));
        Outcome::Pass
      } else {
        Outcome::Fail(diffs.join("; "))
      }
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
  let edits = cases.iter().filter(|c| c.edit.is_some()).count();
  let edits_ok = cases.iter().filter(|c| matches!(c.edit, Some(Ok(())))).count();
  writeln!(
    s,
    "**{edits_ok} of {edits} survive an edit**: their first extrusion's distance, changed in place,\n\
     rebuilds correctly (see the README for how that is checked).\n"
  )
  .unwrap();
  writeln!(s, "| Case | Result | Edit | Volume mm³ | Area mm² | Time | Notes |\n|---|---|---|---|---|---|---|")
    .unwrap();
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
    let (edit, notes) = match &c.edit {
      None => ("", notes),
      Some(Ok(())) => ("ok", notes),
      Some(Err(e)) => {
        ("**fail**", format!("{notes}{}edit: {}", if notes.is_empty() { "" } else { "; " }, e.replace('|', "\\|")))
      }
    };
    writeln!(s, "| {} | {result} | {edit} | {v} | {a} | {t} | {notes} |", c.name).unwrap();
  }
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
    if let Some(Err(e)) = &c.edit {
      println!("EDIT  {}: {e}", c.name);
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
  let edit_baseline = std::fs::read_to_string(root.join("tests/reference/edit-baseline.txt")).unwrap_or_default();
  let edit_baseline: Vec<&str> =
    edit_baseline.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
  let edit_ok = |name: &str| cases.iter().any(|c| c.name == name && matches!(c.edit, Some(Ok(()))));
  let edit_regressed: Vec<&str> = edit_baseline.iter().copied().filter(|n| ran(n) && !edit_ok(n)).collect();
  let edit_new: Vec<&str> = cases
    .iter()
    .filter(|c| matches!(c.edit, Some(Ok(()))) && !edit_baseline.contains(&c.name.as_str()))
    .map(|c| c.name.as_str())
    .collect();
  if !edit_new.is_empty() {
    println!("\nNow surviving the edit; add to tests/reference/edit-baseline.txt:\n{}", edit_new.join("\n"));
  }
  assert!(regressed.is_empty(), "reference parts that used to pass now fail: {regressed:?}");
  assert!(edit_regressed.is_empty(), "reference parts that used to survive an edit now don't: {edit_regressed:?}");
}
