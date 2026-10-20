//! ISO metric screw threads (ISO 68-1 profile, ISO 965-1 tolerances): the diameters a thread of a
//! given designation and tolerance class is modeled at.
//!
//! A threaded cylinder is sized to the middle of its class: an external thread's major diameter,
//! an internal thread's minor diameter. The modeled groove has 60° flanks that cross the
//! class-mean pitch diameter half a pitch apart, and a flat root at the middle of the root's
//! limits (external: the maximum flat-root minor diameter d1 and the minimum rounded-root minor
//! diameter d3; internal: the basic major diameter and its maximum, D2 max + 11H/12).

/// Pitches (mm) with their tabulated ISO 965-1 values.
const PITCHES: [f64; 24] = [
  0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.6, 0.7, 0.75, 0.8, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0,
  5.5, 6.0,
];
/// Fundamental deviations e, f, g (µm; 0 where ISO gives none) by pitch.
const DEVIATIONS: [[f64; 3]; 24] = [
  [0.0, 0.0, 17.0],
  [0.0, 0.0, 18.0],
  [0.0, 0.0, 18.0],
  [0.0, 34.0, 19.0],
  [0.0, 34.0, 19.0],
  [0.0, 35.0, 20.0],
  [50.0, 36.0, 20.0],
  [53.0, 36.0, 21.0],
  [56.0, 38.0, 22.0],
  [56.0, 38.0, 22.0],
  [60.0, 38.0, 24.0],
  [60.0, 40.0, 26.0],
  [63.0, 42.0, 28.0],
  [67.0, 45.0, 32.0],
  [71.0, 48.0, 34.0],
  [71.0, 52.0, 38.0],
  [80.0, 58.0, 42.0],
  [85.0, 63.0, 48.0],
  [90.0, 70.0, 53.0],
  [95.0, 75.0, 60.0],
  [100.0, 80.0, 63.0],
  [106.0, 85.0, 71.0],
  [112.0, 90.0, 75.0],
  [118.0, 95.0, 80.0],
];
/// Major diameter tolerance Td, grade 6 (µm), by pitch.
const TD_MAJOR_6: [f64; 24] = [
  56.0, 67.0, 75.0, 85.0, 95.0, 100.0, 106.0, 125.0, 140.0, 140.0, 150.0, 180.0, 212.0, 236.0, 265.0, 280.0, 335.0,
  375.0, 425.0, 450.0, 500.0, 530.0, 560.0, 600.0,
];
/// Diameter range limits (mm) of the pitch diameter tolerances.
const RANGES: [f64; 10] = [0.99, 1.4, 2.8, 5.6, 11.2, 22.4, 45.0, 90.0, 180.0, 355.0];
/// Pitch diameter tolerances (grade 6, µm) where the ISO tables differ from the rounded formula:
/// (range upper limit, pitch, Td2, TD2).
const TD2_TABLE: [(f64, f64, f64, f64); 2] = [(5.6, 0.5, 75.0, 100.0), (11.2, 1.5, 132.0, 180.0)];

/// The diameters a thread is modeled at (mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreadDims {
  pub pitch: f64,
  /// Diameter the cylinder is sized to: the major diameter outside, the minor diameter inside.
  pub crest: f64,
  /// Pitch diameter (where the groove is half a pitch wide).
  pub pitch_diameter: f64,
  /// Diameter of the groove's flat bottom: the minor diameter outside, the major inside.
  pub root: f64,
}

/// ISO coarse pitches by nominal diameter (mm).
const ISO_COARSE: [(f64, f64); 18] = [
  (1.0, 0.25),
  (1.6, 0.35),
  (2.0, 0.4),
  (2.5, 0.45),
  (3.0, 0.5),
  (4.0, 0.7),
  (5.0, 0.8),
  (6.0, 1.0),
  (8.0, 1.25),
  (10.0, 1.5),
  (12.0, 1.75),
  (14.0, 2.0),
  (16.0, 2.0),
  (20.0, 2.5),
  (24.0, 3.0),
  (30.0, 3.5),
  (36.0, 4.0),
  (42.0, 4.5),
];

/// The ISO coarse pitch for a thread of this major diameter (nearest size).
pub fn coarse_pitch(diameter: f64) -> f64 {
  ISO_COARSE.iter().min_by(|a, b| (a.0 - diameter).abs().total_cmp(&(b.0 - diameter).abs())).map(|s| s.1).unwrap_or(1.0)
}

/// Nominal diameter and pitch of a designation such as `M10x1.5`, `M10 x 1.5` or `M10` (coarse).
pub fn parse_designation(s: &str) -> Option<(f64, f64)> {
  let s: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
  let s = s.strip_prefix('m')?;
  let (d, p) = match s.split_once(['x', '×']) {
    Some((d, p)) => (d.parse::<f64>().ok()?, p.parse::<f64>().ok()?),
    None => {
      let d = s.parse::<f64>().ok()?;
      (d, coarse_pitch(d))
    }
  };
  (d > 0.0 && p > 0.0 && p < d).then_some((d, p))
}

/// Fundamental triangle height.
fn h(p: f64) -> f64 {
  3f64.sqrt() / 2.0 * p
}

/// Nearest value of the R40 preferred number series (by ratio).
fn r40(v: f64) -> f64 {
  if v <= 0.0 {
    return 0.0;
  }
  const R40: [f64; 40] = [
    1.0, 1.06, 1.12, 1.18, 1.25, 1.32, 1.4, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.12, 2.24, 2.36, 2.5, 2.65, 2.8, 3.0, 3.15,
    3.35, 3.55, 3.75, 4.0, 4.25, 4.5, 4.75, 5.0, 5.3, 5.6, 6.0, 6.3, 6.7, 7.1, 7.5, 8.0, 8.5, 9.0, 9.5,
  ];
  let scale = 10f64.powf(v.log10().floor());
  R40
    .iter()
    .chain([10.0].iter())
    .map(|r| r * scale)
    .min_by(|a, b| (v / a).ln().abs().total_cmp(&(v / b).ln().abs()))
    .unwrap_or(v)
}

fn table_index(p: f64) -> Option<usize> {
  PITCHES.iter().position(|&q| (q - p).abs() < 1e-9)
}

/// The diameter range (upper limit) a nominal diameter falls in, and its geometric mean.
fn range(d: f64) -> (f64, f64) {
  for w in RANGES.windows(2) {
    if d <= w[1] {
      return (w[1], (w[0] * w[1]).sqrt());
    }
  }
  (d, d)
}

/// Grade-6 pitch diameter tolerances (µm): external Td2 and internal TD2.
fn pitch_tolerances_6(d: f64, p: f64) -> (f64, f64) {
  let (upper, mean) = range(d);
  if let Some(t) = TD2_TABLE.iter().find(|t| (t.0 - upper).abs() < 1e-9 && (t.1 - p).abs() < 1e-9) {
    return (t.2, t.3);
  }
  let td2 = r40(90.0 * p.powf(0.4) * mean.powf(0.1));
  (td2, r40(1.32 * td2))
}

/// Tolerance grade and position of a class like `6g`, `6H` or `5g6g` (pitch grade first).
fn parse_class(class: &str) -> Option<(u32, u32, char)> {
  let c: Vec<char> = class.trim().chars().collect();
  let one = |g: char, pos: char| Some((g.to_digit(10)?, pos));
  match c.as_slice() {
    [g, pos] => {
      let (g, pos) = one(*g, *pos)?;
      Some((g, g, pos))
    }
    [g2, p2, g1, p1] if p2 == p1 => Some((g2.to_digit(10)?, g1.to_digit(10)?, *p1)),
    _ => None,
  }
}

fn grade_factor(grade: u32, factors: &[(u32, f64)]) -> Option<f64> {
  factors.iter().find(|f| f.0 == grade).map(|f| f.1)
}

/// The modeled diameters of an ISO metric thread: nominal diameter `d`, pitch `p`, tolerance
/// class (`6g` by default outside, `6H` inside).
pub fn iso_dims(d: f64, p: f64, class: &str, internal: bool) -> Result<ThreadDims, String> {
  let class = if class.trim().is_empty() { if internal { "6H" } else { "6g" } } else { class };
  let (g2, g1, pos) = parse_class(class).ok_or_else(|| format!("unknown thread class '{class}'"))?;
  let row = table_index(p);
  let um = 1e-3;
  let (td2_6, big_td2_6) = pitch_tolerances_6(d, p);
  let h = h(p);
  let d2 = d - 0.75 * h;
  let d1 = d - 1.25 * h;
  if internal {
    let ei = match pos {
      'H' => 0.0,
      'G' => row.map(|i| DEVIATIONS[i][2]).unwrap_or(15.0 + 11.0 * p),
      'F' => row.map(|i| DEVIATIONS[i][1]).filter(|v| *v > 0.0).unwrap_or(30.0 + 11.0 * p),
      'E' => row.map(|i| DEVIATIONS[i][0]).filter(|v| *v > 0.0).unwrap_or(50.0 + 11.0 * p),
      _ => return Err(format!("unknown internal thread class '{class}' (use an upper-case position, as 6H)")),
    } * um;
    let f2 = grade_factor(g2, &[(4, 0.85 / 1.32), (5, 1.06 / 1.32), (6, 1.0), (7, 1.7 / 1.32), (8, 2.12 / 1.32)])
      .ok_or_else(|| format!("no tolerance grade {g2} for internal threads"))?;
    let f1 = grade_factor(g1, &[(4, 0.63), (5, 0.8), (6, 1.0), (7, 1.25), (8, 1.6)])
      .ok_or_else(|| format!("no tolerance grade {g1} for internal threads"))?;
    let td1_6 = if p >= 1.0 { 230.0 * p.powf(0.7) } else { 433.0 * p - 190.0 * p.powf(1.22) };
    let td1 = r40(r40(td1_6) * f1) * um;
    let td2 = if g2 == 6 { big_td2_6 } else { r40(big_td2_6 * f2) } * um;
    let d2_max = d2 + ei + td2;
    Ok(ThreadDims {
      pitch: p,
      crest: d1 + ei + td1 / 2.0,
      pitch_diameter: d2 + ei + td2 / 2.0,
      root: (d + ei + d2_max + 11.0 / 12.0 * h) / 2.0,
    })
  } else {
    let es = -match pos {
      'h' => 0.0,
      'g' => row.map(|i| DEVIATIONS[i][2]).unwrap_or(15.0 + 11.0 * p),
      'f' => row.map(|i| DEVIATIONS[i][1]).filter(|v| *v > 0.0).unwrap_or(30.0 + 11.0 * p),
      'e' => row.map(|i| DEVIATIONS[i][0]).filter(|v| *v > 0.0).unwrap_or(50.0 + 11.0 * p),
      _ => return Err(format!("unknown external thread class '{class}' (use a lower-case position, as 6g)")),
    } * um;
    let f2 = grade_factor(g2, &[(3, 0.5), (4, 0.63), (5, 0.8), (6, 1.0), (7, 1.25), (8, 1.6), (9, 2.0)])
      .ok_or_else(|| format!("no tolerance grade {g2} for external threads"))?;
    let f1 = grade_factor(g1, &[(4, 0.63), (6, 1.0), (8, 1.6)])
      .ok_or_else(|| format!("no major diameter tolerance grade {g1} (4, 6 or 8)"))?;
    let td_6 = row.map(|i| TD_MAJOR_6[i]).unwrap_or_else(|| r40(180.0 * p.powf(2.0 / 3.0) - 3.15 / p.sqrt()));
    let td = if g1 == 6 { td_6 } else { r40(td_6 * f1) } * um;
    let td2 = if g2 == 6 { td2_6 } else { r40(td2_6 * f2) } * um;
    let d2_min = d2 + es - td2;
    Ok(ThreadDims {
      pitch: p,
      crest: d + es - td / 2.0,
      pitch_diameter: d2 + es - td2 / 2.0,
      root: (d1 + es + d2_min - (h - p / 4.0)) / 2.0,
    })
  }
}

/// Checks that a cylinder drawn at diameter `drawn` suits nominal size `d` and pitch `p`: an
/// external thread's rod near the nominal (major) diameter, an internal thread's bore near the
/// basic minor diameter. Allows half a pitch or 3% of the nominal size, whichever is larger, so
/// a designation never silently resizes the face to a different size of thread.
pub fn check_drawn_size(d: f64, p: f64, drawn: f64, internal: bool) -> Result<(), String> {
  let (want, what) = if internal { (d - 1.25 * h(p), "minor") } else { (d, "major") };
  let tol = (0.5 * p).max(0.03 * d);
  if (drawn - want).abs() > tol {
    let fmt = |v: f64| format!("{:.3}", v).trim_end_matches('0').trim_end_matches('.').to_string();
    return Err(format!(
      "an M{}x{} thread needs a {} near Ø{} ({what} diameter), not Ø{}",
      fmt(d),
      fmt(p),
      if internal { "hole" } else { "cylinder" },
      fmt(want),
      fmt(drawn)
    ));
  }
  Ok(())
}

/// A thread on a cylinder of diameter `dia` with no designation: the basic profile (no
/// allowances or tolerances), the cylinder being the major diameter outside, the minor inside.
pub fn basic_dims(dia: f64, p: f64, internal: bool) -> ThreadDims {
  let h = h(p);
  if internal {
    let d = dia + 1.25 * h;
    let d2 = d - 0.75 * h;
    ThreadDims { pitch: p, crest: dia, pitch_diameter: d2, root: (d + d2 + 11.0 / 12.0 * h) / 2.0 }
  } else {
    let d2 = dia - 0.75 * h;
    ThreadDims { pitch: p, crest: dia, pitch_diameter: d2, root: (dia - 1.25 * h + d2 - (h - p / 4.0)) / 2.0 }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 5e-4
  }

  #[test]
  fn iso_class_means() {
    let m10 = iso_dims(10.0, 1.5, "6g", false).unwrap();
    assert!(close(m10.crest, 9.85) && close(m10.pitch_diameter, 8.928) && close(m10.root, 8.141), "{m10:?}");
    let m8 = iso_dims(8.0, 1.25, "", false).unwrap();
    assert!(close(m8.crest, 7.866) && close(m8.pitch_diameter, 7.101), "{m8:?}");
    let m12 = iso_dims(12.0, 1.75, "6g", false).unwrap();
    assert!(close(m12.crest, 11.8335) && close(m12.pitch_diameter, 10.754), "{m12:?}");
    let m10i = iso_dims(10.0, 1.5, "6H", true).unwrap();
    assert!(close(m10i.crest, 8.526) && close(m10i.pitch_diameter, 9.116) && close(m10i.root, 10.198), "{m10i:?}");
    assert!(iso_dims(10.0, 1.5, "6x", false).is_err());
    assert_eq!(parse_designation("M10x1.5"), Some((10.0, 1.5)));
    assert_eq!(parse_designation("m12"), Some((12.0, 1.75)));
    assert_eq!(parse_designation("10x1.5"), None);
    assert!((r40(174.0) - 170.0).abs() < 1e-9 && (r40(233.3) - 236.0).abs() < 1e-9);
  }

  #[test]
  fn drawn_size_must_match_the_designation() {
    // a Ø10 rod, or a Ø8.5 tap-drill bore, takes M10x1.5
    assert!(check_drawn_size(10.0, 1.5, 10.0, false).is_ok());
    assert!(check_drawn_size(10.0, 1.5, 9.85, false).is_ok());
    assert!(check_drawn_size(10.0, 1.5, 8.5, true).is_ok());
    // a Ø20 rod or a Ø10 bore does not
    let e = check_drawn_size(10.0, 1.5, 20.0, false).unwrap_err();
    assert!(e.contains("Ø10") && e.contains("Ø20"), "{e}");
    assert!(check_drawn_size(10.0, 1.5, 10.0, true).is_err());
    assert!(check_drawn_size(10.0, 1.5, 8.5, false).is_err());
  }
}
