//! Robustness fuzzing: seeded random parts (a block with holes, bosses, pockets, fillets,
//! chamfers and a shell), built through the command set like a user would.
//!
//! A step the model refuses (a fillet too big for its edge, say) is dropped. What must never
//! happen: a panic, or a step that is accepted but leaves a body that is not a valid solid or has
//! no volume. At the end the block's height is edited, and the part must either rebuild into valid
//! solids or report which features failed, never break silently.
//!
//! `FOX_FUZZ_SEEDS=n` runs more seeds (default 40); `FOX_FUZZ_SEED=k` runs only seed `k`. A
//! failure prints the accepted script, ready for `fissionoxide-cli run`; `FOX_FUZZ_TRACE=1`
//! prints each step as it runs (for a crash, which can't print afterwards).

use fox_engine::Session;
use fox_kernel::{CurveKind, SurfaceKind};
use serde_json::{Value, json};

/// A small deterministic generator (SplitMix64), so seeds reproduce everywhere.
struct Rng(u64);

impl Rng {
  fn next(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }
  /// Uniform in `lo..hi`, rounded to 0.5 so scripts read well.
  fn range(&mut self, lo: f64, hi: f64) -> f64 {
    let u = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
    ((lo + u * (hi - lo)) * 2.0).round() / 2.0
  }
  fn pick(&mut self, n: usize) -> usize {
    (self.next() % n as u64) as usize
  }
}

type Cmd = (String, Value);

fn cmd(name: &str, p: Value) -> Cmd {
  (name.to_string(), p)
}

fn replay(script: &[Cmd]) -> Session {
  let mut s = Session::default();
  for (c, p) in script {
    s.execute(c, p).unwrap_or_else(|e| panic!("accepted step {c} {p} failed on replay: {}", e.0));
  }
  s
}

fn script_text(script: &[Cmd]) -> String {
  script
    .iter()
    .map(|(c, p)| {
      let mut v = p.clone();
      v["cmd"] = json!(c);
      v.to_string()
    })
    .collect::<Vec<_>>()
    .join("\n")
}

/// Problems with the session's solids: invalid shapes or no volume.
fn broken(s: &Session) -> Vec<String> {
  let mut out = Vec::new();
  for b in s.model.bodies.iter().filter(|b| b.shape.solid_count() > 0) {
    if !b.shape.is_valid() {
      out.push(format!("body {} is not a valid solid", b.name));
    }
    match b.shape.mass() {
      Ok(m) if m.volume > 1e-6 => {}
      Ok(m) => out.push(format!("body {} has volume {}", b.name, m.volume)),
      Err(e) => out.push(format!("body {}: {e}", b.name)),
    }
  }
  out
}

/// The top of the part: the height of its highest upward flat face, and its footprint (min, max).
fn top(s: &Session) -> Option<(f64, [f64; 2], [f64; 2])> {
  let b = s.model.bodies.iter().find(|b| b.shape.solid_count() > 0)?;
  let f = b
    .faces()
    .iter()
    .filter(|f| f.kind == SurfaceKind::Plane && f.normal.z > 0.999)
    .max_by(|a, b| a.origin.z.total_cmp(&b.origin.z))
    .copied()?;
  let m = b.shape.mass().ok()?;
  Some((f.origin.z, [m.bbox_min.x, m.bbox_min.y], [m.bbox_max.x, m.bbox_max.y]))
}

/// One random step as commands, for the part as it stands.
fn step(rng: &mut Rng, s: &Session, shelled: &mut bool) -> Option<Vec<Cmd>> {
  let (z, lo, hi) = top(s)?;
  let (w, d) = (hi[0] - lo[0], hi[1] - lo[1]);
  let at =
    |rng: &mut Rng, margin: f64| [rng.range(lo[0] + margin, hi[0] - margin), rng.range(lo[1] + margin, hi[1] - margin)];
  let on_top = json!({"origin": [0.0, 0.0, z], "normal": [0.0, 0.0, 1.0], "x": [1.0, 0.0, 0.0]});
  let body = s.model.bodies.iter().find(|b| b.shape.solid_count() > 0)?;
  let edges: Vec<_> = body
    .edges()
    .iter()
    .filter(|e| e.faces[0] != 0 && e.faces[1] != 0 && e.kind != CurveKind::Degenerate)
    .cloned()
    .collect();
  Some(match rng.pick(7) {
    // a hole through everything
    0 => {
      let r = rng.range(1.0, (w.min(d) / 8.0).max(1.5));
      vec![
        cmd("sketch.create", json!({"plane": on_top})),
        cmd("sketch.circle", json!({"center": at(rng, r + 1.0), "radius": r})),
        cmd("sketch.finish", json!({})),
        cmd("extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"})),
      ]
    }
    // a boss on top
    1 => {
      let c = at(rng, 4.0);
      let (a, b) = (rng.range(2.0, w / 4.0 + 2.0), rng.range(2.0, d / 4.0 + 2.0));
      vec![
        cmd("sketch.create", json!({"plane": on_top})),
        cmd("sketch.rect", json!({"center": c, "corner": [c[0] + a, c[1] + b]})),
        cmd("sketch.finish", json!({})),
        cmd("extrude", json!({"distance": rng.range(1.0, 12.0), "operation": "join"})),
      ]
    }
    // a blind pocket
    2 => {
      let c = at(rng, 5.0);
      let r = rng.range(1.5, (w.min(d) / 6.0).max(2.0));
      vec![
        cmd("sketch.create", json!({"plane": on_top})),
        cmd("sketch.circle", json!({"center": c, "radius": r})),
        cmd("sketch.finish", json!({})),
        cmd("extrude", json!({"distance": rng.range(0.5, 4.0), "flip": true, "operation": "cut"})),
      ]
    }
    // fillets or a chamfer on a few edges
    3..=5 if !edges.is_empty() => {
      let n = 1 + rng.pick(3.min(edges.len()));
      let picked: Vec<Value> = (0..n).map(|_| json!({"point": edges[rng.pick(edges.len())].mid.to_array()})).collect();
      let size = rng.range(0.5, 3.0);
      if rng.pick(3) == 0 {
        vec![cmd("chamfer", json!({"edges": picked, "distance": size}))]
      } else {
        vec![cmd("fillet", json!({"edges": picked, "radius": size}))]
      }
    }
    // hollow it once, opening the top
    6 if !*shelled => {
      *shelled = true;
      let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, z];
      vec![cmd("shell", json!({"faces": [{"point": c, "normal": [0, 0, 1]}], "thickness": rng.range(0.5, 2.5)}))]
    }
    _ => return None,
  })
}

/// Builds seed `seed`'s part; `Err` describes a broken invariant with the script that shows it.
fn fuzz(seed: u64) -> Result<(usize, usize), String> {
  let mut rng = Rng(seed);
  let (w, d, h) = (rng.range(20.0, 80.0), rng.range(20.0, 80.0), rng.range(5.0, 30.0));
  let mut script = vec![
    cmd("sketch.create", json!({"plane": "xy"})),
    cmd("sketch.rect", json!({"corner1": [0, 0], "corner2": [w, d]})),
    cmd("sketch.finish", json!({})),
    cmd("extrude", json!({"distance": h})),
  ];
  let mut s = replay(&script);
  let mut shelled = false;
  let (mut accepted, mut refused) = (0, 0);
  for _ in 0..(4 + rng.pick(6)) {
    let Some(cmds) = step(&mut rng, &s, &mut shelled) else { continue };
    let mut ok = true;
    for (c, p) in &cmds {
      if std::env::var_os("FOX_FUZZ_TRACE").is_some() {
        eprintln!("{}", script_text(&[(c.clone(), p.clone())]));
      }
      if s.execute(c, p).is_err() {
        ok = false;
        break;
      }
    }
    if ok && s.model.errors().is_empty() {
      let bad = broken(&s);
      if !bad.is_empty() {
        script.extend(cmds);
        return Err(format!("{}\n--- script ---\n{}", bad.join("; "), script_text(&script)));
      }
      script.extend(cmds);
      accepted += 1;
    } else {
      refused += 1;
      s = replay(&script);
    }
  }
  // edit the block's height: the part must rebuild valid, or say what failed
  let block = s.design.features.iter().find(|f| f.kind.base_name() == "Extrude").map(|f| f.id.0);
  if let Some(id) = block {
    let mut extent =
      serde_json::to_value(&s.design.feature(fox_engine::model::FeatureId(id)).unwrap().kind).unwrap()["extent"]
        .clone();
    extent["distance"] = json!((h * 1.3).to_string());
    s.execute("feature.edit", &json!({"feature": id, "set": {"extent": extent}})).map_err(|e| e.0)?;
    let bad = broken(&s);
    if !bad.is_empty() {
      return Err(format!(
        "after editing the block's height to {}: {}\n--- script ---\n{}",
        h * 1.3,
        bad.join("; "),
        script_text(&script)
      ));
    }
  }
  Ok((accepted, refused))
}

#[test]
fn random_parts_stay_valid() {
  let seeds: Vec<u64> = match std::env::var("FOX_FUZZ_SEED") {
    Ok(k) => vec![k.parse().unwrap()],
    Err(_) => (0..std::env::var("FOX_FUZZ_SEEDS").ok().and_then(|n| n.parse().ok()).unwrap_or(40)).collect(),
  };
  let mut failures = Vec::new();
  let (mut accepted, mut refused) = (0, 0);
  for seed in seeds {
    match std::panic::catch_unwind(|| fuzz(seed)) {
      Ok(Ok((a, r))) => {
        accepted += a;
        refused += r;
      }
      Ok(Err(e)) => failures.push(format!("seed {seed}: {e}")),
      Err(_) => failures.push(format!("seed {seed}: panicked")),
    }
  }
  println!("{accepted} steps accepted, {refused} refused");
  assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
