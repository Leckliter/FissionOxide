//! Robustness fuzzing: seeded random feature sequences (sketch, extrude, cut, fillet, chamfer,
//! shell, pattern, and edits of earlier features and parameters), checked after every step.
//!
//! After each step that succeeds, every body must be a valid solid with a positive volume, the
//! volume must have changed the way the operation allows (a join never loses volume, a cut never
//! gains it, a pattern of separate copies multiplies it), the incrementally regenerated model must
//! match a from-scratch regeneration of the same design, and undo then redo must give back the
//! part before and after the step. A step whose command or features fail is rolled back, and the
//! part must then be exactly what it was.
//!
//! `cargo test` runs a few short seeds. The long run is ignored by default:
//!
//! ```sh
//! FOX_FUZZ_SEEDS=200 FOX_FUZZ_STEPS=40 cargo test --release -p fox-engine --test fuzz_features -- --ignored
//! ```
//!
//! `FOX_FUZZ_SEED=n` starts at seed `n`. A failure prints the seed and the commands that led to it,
//! as a script `fissionoxide-cli run` replays.

use fox_engine::Session;
use fox_kernel::glam::DVec3;
use serde_json::{Value, json};

/// SplitMix64: small, seedable, and the same on every platform.
struct Rng(u64);

impl Rng {
  fn next(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
  }

  /// Uniform in `[lo, hi)`, rounded to 0.01 so scripts stay readable.
  fn range(&mut self, lo: f64, hi: f64) -> f64 {
    let t = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
    ((lo + t * (hi - lo)) * 100.0).round() / 100.0
  }

  fn below(&mut self, n: usize) -> usize {
    (self.next() % n.max(1) as u64) as usize
  }
}

#[derive(Debug, Clone, PartialEq)]
struct Measured {
  bodies: usize,
  solids: usize,
  faces: usize,
  volume: f64,
  area: f64,
}

/// The visible solid bodies: their ids, volumes and boxes.
struct BodyBox {
  id: u32,
  volume: f64,
  lo: DVec3,
  hi: DVec3,
  /// Total length of the body's edges.
  edge_length: f64,
}

fn bodies(s: &Session) -> Vec<BodyBox> {
  s.model
    .bodies
    .iter()
    .filter(|b| s.body_visible(b.id) && b.shape.solid_count() > 0)
    .filter_map(|b| {
      let m = b.shape.mass().ok()?;
      Some(BodyBox {
        id: b.id.0,
        volume: m.volume,
        lo: m.bbox_min,
        hi: m.bbox_max,
        edge_length: b.edges().iter().map(|e| e.length).sum(),
      })
    })
    .collect()
}

/// Validity of every visible solid body, and the totals over them.
fn measure(s: &Session) -> Result<Measured, String> {
  let mut m = Measured { bodies: 0, solids: 0, faces: 0, volume: 0.0, area: 0.0 };
  for b in s.model.bodies.iter().filter(|b| s.body_visible(b.id)) {
    let solids = b.shape.solid_count();
    if solids == 0 {
      continue;
    }
    if !b.shape.is_valid() {
      return Err(format!("body {} ({}) is not a valid solid", b.id.0, b.name));
    }
    let p = b.shape.mass().map_err(|e| format!("mass of body {}: {e}", b.id.0))?;
    if p.volume.is_nan() || p.volume <= 0.0 {
      return Err(format!("body {} has volume {}", b.id.0, p.volume));
    }
    m.bodies += 1;
    m.solids += solids;
    m.faces += b.faces().len();
    m.volume += p.volume;
    m.area += p.area;
  }
  Ok(m)
}

fn same(a: &Measured, b: &Measured) -> bool {
  let close = |x: f64, y: f64| (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0);
  a.bodies == b.bodies
    && a.solids == b.solids
    && a.faces == b.faces
    && close(a.volume, b.volume)
    && close(a.area, b.area)
}

/// How a step may change the total volume.
enum Expect {
  /// Join: up by at most this much.
  Grow(f64),
  /// Cut: down by at most this much.
  Shrink(f64),
  /// Fillet or chamfer: changes by at most this much either way.
  Near(f64),
  /// Shell: goes down.
  Less,
  /// Pattern of separate copies: exactly this.
  Exactly(f64),
  /// Edits of earlier features: anything, as long as the part is valid.
  Any,
}

struct Step {
  what: String,
  commands: Vec<(String, Value)>,
  expect: Expect,
}

struct Fuzz {
  s: Session,
  rng: Rng,
  seed: u64,
  /// Commands of the steps kept so far.
  log: Vec<String>,
  problems: Vec<String>,
  steps_kept: usize,
  steps_rejected: usize,
}

fn line(cmd: &str, p: &Value) -> String {
  let mut v = serde_json::Map::new();
  v.insert("cmd".into(), json!(cmd));
  v.extend(p.as_object().cloned().unwrap_or_default());
  Value::Object(v).to_string()
}

fn top_plane(z: f64) -> Value {
  json!({"origin": [0.0, 0.0, z], "normal": [0.0, 0.0, 1.0], "x": [1.0, 0.0, 0.0]})
}

impl Fuzz {
  fn new(seed: u64) -> Fuzz {
    Fuzz {
      s: Session::default(),
      rng: Rng(seed),
      seed,
      log: Vec::new(),
      problems: Vec::new(),
      steps_kept: 0,
      steps_rejected: 0,
    }
  }

  fn problem(&mut self, step: &Step, what: String) {
    let mut script = self.log.clone();
    script.extend(step.commands.iter().map(|(c, p)| line(c, p)));
    self.problems.push(format!(
      "seed {} step {} ({}): {what}\nscript:\n{}",
      self.seed,
      self.steps_kept + self.steps_rejected,
      step.what,
      script.join("\n")
    ));
  }

  /// The base block: parameters, a rectangle sketch and an extrude by `height`.
  fn base(&mut self) -> Step {
    let (w, d, h) = (self.rng.range(20.0, 80.0), self.rng.range(20.0, 80.0), self.rng.range(5.0, 30.0));
    Step {
      what: "base block".into(),
      commands: vec![
        ("param.add".into(), json!({"name": "height", "expr": format!("{h} mm")})),
        ("param.add".into(), json!({"name": "width", "expr": format!("{w} mm")})),
        ("sketch.create".into(), json!({"plane": "xy"})),
        // four lines from the origin: points 0 (the origin), 1 = (w, 0), 4 and 7
        ("sketch.line".into(), json!({"points": [[0, 0], [w, 0]]})),
        ("sketch.line".into(), json!({"points": [[w, 0], [w, d]]})),
        ("sketch.line".into(), json!({"points": [[w, d], [0, d]]})),
        ("sketch.line".into(), json!({"points": [[0, d], [0, 0]]})),
        ("sketch.dimension".into(), json!({"type": "horizontal", "points": [0, 1], "value": "width"})),
        ("sketch.finish".into(), json!({})),
        ("extrude".into(), json!({"sketch": 1, "distance": "height", "operation": "new_body"})),
      ],
      expect: Expect::Any,
    }
  }

  /// A random point over the part's footprint and its top height.
  fn spot(&mut self, bb: &[BodyBox]) -> (f64, f64, f64, f64) {
    let b = &bb[self.rng.below(bb.len())];
    let size = (b.hi - b.lo).truncate().min_element();
    let x = self.rng.range(b.lo.x + 0.1 * size, b.hi.x - 0.1 * size);
    let y = self.rng.range(b.lo.y + 0.1 * size, b.hi.y - 0.1 * size);
    (x, y, b.hi.z, size)
  }

  fn next_feature(&self) -> u32 {
    self.s.design.features.iter().map(|f| f.id.0).max().unwrap_or(0) + 1
  }

  /// A random step on the current part.
  fn random_step(&mut self) -> Option<Step> {
    let bb = bodies(&self.s);
    if bb.is_empty() {
      return None;
    }
    let total: f64 = bb.iter().map(|b| b.volume).sum();
    let sketch = self.next_feature();
    let step = match self.rng.below(9) {
      // boss: a circle on the top plane, extruded up and joined
      0 | 1 => {
        let (x, y, z, size) = self.spot(&bb);
        let r = self.rng.range(0.05 * size, 0.25 * size);
        let h = self.rng.range(1.0, 15.0);
        Step {
          what: format!("boss r {r} h {h}"),
          commands: vec![
            ("sketch.create".into(), json!({"plane": top_plane(z)})),
            ("sketch.circle".into(), json!({"center": [x, y], "radius": r})),
            ("sketch.finish".into(), json!({})),
            ("extrude".into(), json!({"sketch": sketch, "distance": h, "operation": "join"})),
          ],
          expect: Expect::Grow(std::f64::consts::PI * r * r * h),
        }
      }
      // pocket or through hole: a circle or rectangle on the top plane, cut down
      2 | 3 => {
        let (x, y, z, size) = self.spot(&bb);
        let r = self.rng.range(0.05 * size, 0.25 * size);
        let shape = if self.rng.below(2) == 0 {
          ("sketch.circle", json!({"center": [x, y], "radius": r}))
        } else {
          ("sketch.rect", json!({"center": [x, y], "corner": [x + r, y + 0.7 * r]}))
        };
        let through = self.rng.below(3) == 0;
        let depth = self.rng.range(0.5, 20.0);
        let mut ex = json!({"sketch": sketch, "operation": "cut", "flip": true});
        if through {
          ex["extent"] = json!("through_all");
        } else {
          ex["distance"] = json!(depth);
        }
        Step {
          what: format!("{} cut{}", shape.0, if through { " through all" } else { "" }),
          commands: vec![
            ("sketch.create".into(), json!({"plane": top_plane(z)})),
            (shape.0.into(), shape.1),
            ("sketch.finish".into(), json!({})),
            ("extrude".into(), ex),
          ],
          // generous: the whole part could be cut away down to nothing
          expect: Expect::Shrink(total),
        }
      }
      // fillet or chamfer one or two random edges
      4 | 5 => {
        let b = &bb[self.rng.below(bb.len())];
        let edges: Vec<u32> =
          self.s.model.body(fox_model::BodyId(b.id)).map(|x| x.edges().iter().map(|e| e.id).collect())?;
        let n = 1 + self.rng.below(2);
        let picked: Vec<Value> =
          (0..n).map(|_| json!({"body": b.id, "edge": edges[self.rng.below(edges.len())]})).collect();
        let r = self.rng.range(0.2, 2.0);
        // generous bound: tangent chains propagate, and sharp edges lose more than r² per length
        let bound = 10.0 * r * r * b.edge_length;
        if self.rng.below(2) == 0 {
          Step {
            what: format!("fillet r {r}"),
            commands: vec![("fillet".into(), json!({"edges": picked, "radius": r}))],
            expect: Expect::Near(bound),
          }
        } else {
          Step {
            what: format!("chamfer {r}"),
            commands: vec![("chamfer".into(), json!({"edges": picked, "distance": r}))],
            expect: Expect::Near(bound),
          }
        }
      }
      // shell, opening one random planar face
      6 => {
        let b = &bb[self.rng.below(bb.len())];
        let faces: Vec<u32> = self
          .s
          .model
          .body(fox_model::BodyId(b.id))
          .map(|x| x.faces().iter().filter(|f| f.kind == fox_kernel::SurfaceKind::Plane).map(|f| f.id).collect())?;
        if faces.is_empty() {
          return None;
        }
        let t = self.rng.range(0.5, 2.5);
        Step {
          what: format!("shell {t}"),
          commands: vec![(
            "shell".into(),
            json!({"faces": [{"body": b.id, "face": faces[self.rng.below(faces.len())]}], "thickness": t}),
          )],
          expect: Expect::Less,
        }
      }
      // rectangular pattern of one body, copies far enough apart not to touch
      7 => {
        if bb.len() >= 3 {
          return None;
        }
        let b = &bb[self.rng.below(bb.len())];
        let lo = bb.iter().map(|b| b.lo).fold(DVec3::splat(f64::INFINITY), DVec3::min);
        let hi = bb.iter().map(|b| b.hi).fold(DVec3::splat(f64::NEG_INFINITY), DVec3::max);
        let count = 2 + self.rng.below(2);
        let spacing = ((hi.y - lo.y) + self.rng.range(2.0, 10.0)).ceil();
        Step {
          what: format!("pattern x{count}"),
          commands: vec![(
            "pattern.rectangular".into(),
            json!({"direction": [0, 1, 0], "count": count, "spacing": spacing, "bodies": [b.id]}),
          )],
          expect: Expect::Exactly(total + (count - 1) as f64 * b.volume),
        }
      }
      // edit something earlier: the base height or width, or an earlier extrude, fillet, chamfer or shell
      _ => self.edit_step()?,
    };
    Some(step)
  }

  fn edit_step(&mut self) -> Option<Step> {
    if self.rng.below(2) == 0 {
      let (name, v) = if self.rng.below(2) == 0 {
        ("height", self.rng.range(5.0, 30.0))
      } else {
        ("width", self.rng.range(20.0, 80.0))
      };
      return Some(Step {
        what: format!("edit parameter {name} = {v}"),
        commands: vec![("param.set".into(), json!({"name": name, "expr": format!("{v} mm")}))],
        expect: Expect::Any,
      });
    }
    let candidates: Vec<(u32, Value)> = self
      .s
      .design
      .features
      .iter()
      .filter_map(|f| {
        let k = serde_json::to_value(&f.kind).ok()?;
        matches!(k["type"].as_str(), Some("extrude" | "fillet" | "chamfer" | "shell")).then_some((f.id.0, k))
      })
      .collect();
    if candidates.is_empty() {
      return None;
    }
    let (id, kind) = candidates[self.rng.below(candidates.len())].clone();
    let set = match kind["type"].as_str()? {
      "extrude" => {
        let mut extent = kind["extent"].clone();
        if extent["distance"] == json!("height") || extent["kind"] == json!("through_all") {
          return None;
        }
        extent["distance"] = json!(self.rng.range(0.5, 20.0).to_string());
        json!({"extent": extent})
      }
      "fillet" => json!({"radius": self.rng.range(0.2, 2.0).to_string()}),
      "chamfer" => json!({"distance": self.rng.range(0.2, 2.0).to_string()}),
      _ => json!({"thickness": self.rng.range(0.5, 2.5).to_string()}),
    };
    Some(Step {
      what: format!("edit feature {id} ({})", kind["type"].as_str().unwrap_or("?")),
      commands: vec![("feature.edit".into(), json!({"feature": id, "set": set}))],
      expect: Expect::Any,
    })
  }

  /// Runs one step in a transaction and checks the result; rolls it back if it fails.
  fn run(&mut self, step: Step) {
    let before = match measure(&self.s) {
      Ok(m) => m,
      Err(e) => {
        self.problem(&step, format!("part invalid before the step: {e}"));
        return;
      }
    };
    if std::env::var_os("FOX_FUZZ_VERBOSE").is_some() {
      println!("seed {} step {}", self.seed, step.what);
    }
    self.s.begin_transaction().unwrap();
    let mut failed = None;
    for (cmd, p) in &step.commands {
      if std::env::var_os("FOX_FUZZ_VERBOSE").is_some() {
        println!("seed {} run {}", self.seed, line(cmd, p));
      }
      if let Err(e) = self.s.execute(cmd, p) {
        failed = Some(format!("{cmd}: {}", e.0));
        break;
      }
    }
    let errors = self.s.model.errors();
    if failed.is_none() && !errors.is_empty() {
      failed = Some(format!("feature errors: {errors:?}"));
    }
    if let Some(reason) = failed {
      if reason.contains("internal error") {
        self.problem(&step, format!("panicked: {reason}"));
      }
      if std::env::var_os("FOX_FUZZ_VERBOSE").is_some() {
        println!("seed {} rejected {}: {reason}", self.seed, step.what);
      }
      self.s.rollback_transaction().unwrap();
      self.s.edit_sketch = None;
      self.steps_rejected += 1;
      match measure(&self.s) {
        Ok(m) if same(&before, &m) => {}
        Ok(m) => self.problem(&step, format!("rolling back a failed step changed the part: {before:?} -> {m:?}")),
        Err(e) => self.problem(&step, format!("after rolling back a failed step: {e}")),
      }
      return;
    }
    self.s.commit_transaction().unwrap();
    self.s.edit_sketch = None;
    let after = match measure(&self.s) {
      Ok(m) => m,
      Err(e) => {
        self.problem(&step, e);
        self.s.undo();
        return;
      }
    };
    let tol = 1e-6 * before.volume.max(after.volume) + 1e-6;
    let dv = after.volume - before.volume;
    let ok = match step.expect {
      Expect::Grow(max) => dv >= -tol && dv <= max + tol,
      Expect::Shrink(max) => dv <= tol && -dv <= max + tol,
      Expect::Near(max) => dv.abs() <= max + tol,
      Expect::Less => dv < tol,
      Expect::Exactly(v) => (after.volume - v).abs() <= 1e-6 * v,
      Expect::Any => true,
    };
    if !ok {
      self.problem(&step, format!("volume {:.4} -> {:.4} not allowed for this step", before.volume, after.volume));
    }
    match measure(&Session::new(self.s.design.clone())) {
      Ok(f) if same(&after, &f) => {}
      Ok(f) => self.problem(&step, format!("from-scratch regeneration differs: {after:?} vs {f:?}")),
      Err(e) => self.problem(&step, format!("from-scratch regeneration: {e}")),
    }
    self.s.undo();
    match measure(&self.s) {
      Ok(m) if same(&before, &m) => {}
      Ok(m) => self.problem(&step, format!("undo gave {m:?}, expected {before:?}")),
      Err(e) => self.problem(&step, format!("after undo: {e}")),
    }
    self.s.redo();
    match measure(&self.s) {
      Ok(m) if same(&after, &m) => {}
      Ok(m) => self.problem(&step, format!("redo gave {m:?}, expected {after:?}")),
      Err(e) => self.problem(&step, format!("after redo: {e}")),
    }
    self.log.extend(step.commands.iter().map(|(c, p)| line(c, p)));
    self.steps_kept += 1;
  }
}

/// Runs `steps` random steps from `seed`; returns the problems found and (kept, rejected) counts.
fn fuzz(seed: u64, steps: usize) -> (Vec<String>, usize, usize) {
  let mut f = Fuzz::new(seed);
  let base = f.base();
  f.run(base);
  if f.steps_kept == 0 {
    f.problems.push(format!("seed {seed}: the base block failed"));
  }
  let mut tries = 0;
  while f.steps_kept + f.steps_rejected < steps && tries < steps * 4 {
    tries += 1;
    if let Some(step) = f.random_step() {
      f.run(step);
    }
    if f.problems.len() > 3 {
      break;
    }
  }
  (f.problems, f.steps_kept, f.steps_rejected)
}

/// The tests here take turns: OpenCASCADE gives different results when two threads use it at
/// once, even on unrelated shapes (seen as incremental and from-scratch regeneration disagreeing).
static KERNEL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run_seeds(first: u64, seeds: u64, steps: usize) {
  let _turn = KERNEL.lock().unwrap_or_else(|e| e.into_inner());
  let mut problems = Vec::new();
  for seed in first..first + seeds {
    let (p, kept, rejected) = fuzz(seed, steps);
    println!("seed {seed}: {kept} steps kept, {rejected} rejected, {} problems", p.len());
    problems.extend(p);
  }
  assert!(problems.is_empty(), "{} problems:\n\n{}", problems.len(), problems.join("\n\n"));
}

fn env(name: &str, default: u64) -> u64 {
  std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
fn fuzz_short() {
  run_seeds(env("FOX_FUZZ_SEED", 1), 4, 12);
}

#[test]
#[ignore = "long run: cargo test --release -p fox-engine --test fuzz_features -- --ignored"]
fn fuzz_long() {
  run_seeds(env("FOX_FUZZ_SEED", 1000), env("FOX_FUZZ_SEEDS", 100), env("FOX_FUZZ_STEPS", 40) as usize);
}

/// Seeds that found bugs: 1007 rolled back a failed chamfer onto a corrupted (invalid) body, then
/// produced an inside-out (negative volume) solid that passed the validity check; 1022 cut a body
/// and undo gave back a corrupted copy of it.
#[test]
fn fuzz_found_bugs() {
  run_seeds(1007, 1, 30);
  run_seeds(1022, 1, 30);
}

/// The scripts in `tests/fuzz/` crashed the process inside OpenCASCADE at their last command. Every
/// command before it must still work, and the last must fail as an error or leave valid solids.
#[test]
fn crash_scripts_fail_cleanly() {
  let _turn = KERNEL.lock().unwrap_or_else(|e| e.into_inner());
  let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fuzz");
  let mut scripts: Vec<_> = std::fs::read_dir(&dir)
    .unwrap()
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
    .collect();
  scripts.sort();
  assert!(!scripts.is_empty());
  for path in scripts {
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let mut s = Session::default();
    for (i, l) in lines.iter().enumerate() {
      let mut p: Value = serde_json::from_str(l).unwrap();
      let cmd = p["cmd"].as_str().unwrap().to_string();
      p.as_object_mut().unwrap().remove("cmd");
      let r = s.execute(&cmd, &p);
      if i + 1 < lines.len() {
        assert!(r.is_ok(), "{}: line {} {cmd}: {:?}", path.display(), i + 1, r.err());
      }
    }
    s.edit_sketch = None;
    if s.model.errors().is_empty() {
      measure(&s).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
  }
}
