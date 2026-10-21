//! Small vector maths and what the add-in reads from the engine: bodies, their faces and edges.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::fox;

/// Millimetres per inch: the engine works in mm, LumenArt's numbers are inches.
pub const IN: f64 = 25.4;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct V3(pub f64, pub f64, pub f64);

impl V3 {
  pub fn of(v: &Value) -> V3 {
    V3(v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0), v[2].as_f64().unwrap_or(0.0))
  }
  pub fn add(self, o: V3) -> V3 {
    V3(self.0 + o.0, self.1 + o.1, self.2 + o.2)
  }
  pub fn sub(self, o: V3) -> V3 {
    V3(self.0 - o.0, self.1 - o.1, self.2 - o.2)
  }
  pub fn mul(self, k: f64) -> V3 {
    V3(self.0 * k, self.1 * k, self.2 * k)
  }
  pub fn dot(self, o: V3) -> f64 {
    self.0 * o.0 + self.1 * o.1 + self.2 * o.2
  }
  pub fn cross(self, o: V3) -> V3 {
    V3(self.1 * o.2 - self.2 * o.1, self.2 * o.0 - self.0 * o.2, self.0 * o.1 - self.1 * o.0)
  }
  pub fn len(self) -> f64 {
    self.dot(self).sqrt()
  }
  pub fn unit(self) -> V3 {
    let l = self.len();
    if l > 1e-12 { self.mul(1.0 / l) } else { self }
  }
  /// The part of `self` across `n` (n a unit vector).
  pub fn across(self, n: V3) -> V3 {
    self.sub(n.mul(self.dot(n)))
  }
  pub fn json(self) -> Value {
    json!([self.0, self.1, self.2])
  }
}

pub const Z: V3 = V3(0.0, 0.0, 1.0);

#[derive(Debug, Clone)]
pub struct Face {
  pub id: u64,
  pub kind: String,
  pub area: f64,
  pub normal: V3,
  pub center: V3,
  pub edges: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct Edge {
  pub id: u64,
  pub kind: String,
  pub start: V3,
  pub end: V3,
  pub mid: V3,
  pub center: V3,
  pub axis: V3,
  pub length: f64,
  pub faces: Vec<u64>,
}

impl Edge {
  /// Its direction at the middle.
  pub fn tangent(&self) -> V3 {
    if self.kind == "line" || self.axis.len() < 1e-9 {
      self.end.sub(self.start).unit()
    } else {
      self.axis.cross(self.mid.sub(self.center)).unit()
    }
  }
}

/// A body's faces and edges (`body.topology`).
#[derive(Debug, Clone)]
pub struct Topo {
  pub faces: Vec<Face>,
  pub edges: Vec<Edge>,
}

impl Topo {
  pub fn read(body: u64) -> Result<Topo, String> {
    let t = fox::execute("body.topology", json!({"body": body}))?;
    let ids = |v: &Value| v.as_array().into_iter().flatten().filter_map(Value::as_u64).collect::<Vec<_>>();
    let faces = t["faces"]
      .as_array()
      .into_iter()
      .flatten()
      .map(|f| Face {
        id: f["id"].as_u64().unwrap_or(0),
        kind: f["kind"].as_str().unwrap_or("").to_string(),
        area: f["area"].as_f64().unwrap_or(0.0),
        normal: V3::of(&f["normal"]),
        center: V3::of(&f["center"]),
        edges: ids(&f["edges"]),
      })
      .collect();
    let edges = t["edges"]
      .as_array()
      .into_iter()
      .flatten()
      .map(|e| Edge {
        id: e["id"].as_u64().unwrap_or(0),
        kind: e["kind"].as_str().unwrap_or("").to_string(),
        start: V3::of(&e["start"]),
        end: V3::of(&e["end"]),
        mid: V3::of(&e["mid"]),
        center: V3::of(&e["center"]),
        axis: V3::of(&e["axis"]),
        length: e["length"].as_f64().unwrap_or(0.0),
        faces: ids(&e["faces"]),
      })
      .collect();
    Ok(Topo { faces, edges })
  }

  pub fn face(&self, id: u64) -> Option<&Face> {
    self.faces.iter().find(|f| f.id == id)
  }

  pub fn edge(&self, id: u64) -> Option<&Edge> {
    self.edges.iter().find(|e| e.id == id)
  }

  pub fn face_edges(&self, f: &Face) -> Vec<&Edge> {
    f.edges.iter().filter_map(|e| self.edge(*e)).collect()
  }

  /// The other face of an edge.
  pub fn across_edge(&self, e: &Edge, from: u64) -> Option<&Face> {
    e.faces.iter().find(|f| **f != from).and_then(|f| self.face(*f))
  }

  /// How thick the body is through a flat face: the nearest flat face facing the other way.
  pub fn thickness_at(&self, f: &Face) -> Option<f64> {
    self
      .faces
      .iter()
      .filter(|g| g.id != f.id && g.kind == "plane" && g.normal.dot(f.normal) < -0.999)
      .map(|g| f.center.sub(g.center).dot(f.normal))
      .filter(|d| *d > 1e-6)
      .min_by(f64::total_cmp)
  }
}

/// A body as `info` lists it.
#[derive(Debug, Clone)]
pub struct Body {
  pub id: u64,
  pub name: String,
  pub lo: V3,
  pub hi: V3,
}

impl Body {
  pub fn contains_xy(&self, p: V3, margin: f64) -> bool {
    p.0 >= self.lo.0 - margin && p.0 <= self.hi.0 + margin && p.1 >= self.lo.1 - margin && p.1 <= self.hi.1 + margin
  }
}

/// The design summary (`info`).
pub struct Info {
  pub raw: Value,
  pub bodies: Vec<Body>,
}

impl Info {
  pub fn read() -> Result<Info, String> {
    let raw = fox::execute("info", json!({}))?;
    let bodies = raw["bodies"]
      .as_array()
      .into_iter()
      .flatten()
      .filter(|b| b["bbox"].is_array())
      .map(|b| Body {
        id: b["id"].as_u64().unwrap_or(0),
        name: b["name"].as_str().unwrap_or("").to_string(),
        lo: V3::of(&b["bbox"][0]),
        hi: V3::of(&b["bbox"][1]),
      })
      .collect();
    Ok(Info { raw, bodies })
  }

  /// The component a body is in (by id), if any.
  pub fn component_of(&self, body: u64) -> Option<(u64, String)> {
    self.raw["components"].as_array().into_iter().flatten().find_map(|c| {
      let has = c["bodies"].as_array().is_some_and(|b| b.iter().any(|x| x.as_u64() == Some(body)));
      has.then(|| (c["id"].as_u64().unwrap_or(0), c["name"].as_str().unwrap_or("").to_string()))
    })
  }

  /// The id of the last timeline feature with this name.
  pub fn feature_named(&self, name: &str) -> Option<u64> {
    self.raw["timeline"]
      .as_array()
      .into_iter()
      .flatten()
      .rev()
      .find(|f| f["name"] == name)
      .and_then(|f| f["id"].as_u64())
  }

  /// The highest body under `p` whose top is below it (within `reach` mm), other than `not`:
  /// a ray straight down, checked against the body itself.
  pub fn below(&self, p: V3, not: u64, reach: f64) -> Option<&Body> {
    let mut under: Vec<&Body> = self
      .bodies
      .iter()
      .filter(|b| b.id != not && b.contains_xy(p, 0.01) && b.hi.2 < p.2 - 1e-4 && p.2 - b.hi.2 < reach)
      .collect();
    under.sort_by(|a, b| b.hi.2.total_cmp(&a.hi.2));
    let hit = |b: &&Body| {
      // just under its top, or half way down where the top isn't flat there
      let probe = [b.hi.2 - 0.02, (b.hi.2 + b.lo.2) / 2.0];
      let pts: Vec<Value> = probe.iter().map(|z| json!([p.0, p.1, z])).collect();
      fox::execute("body.classify", json!({"body": b.id, "points": pts}))
        .map(|r| r["inside"].as_array().is_some_and(|a| a.iter().any(|w| w != "out")))
        .unwrap_or(true)
    };
    under.into_iter().find(hit)
  }
}

/// The sketch or feature id of a pick (`{"type": "feature", "feature": 3}`).
pub fn pick_feature(p: &Value) -> Option<u64> {
  p["feature"].as_u64().or_else(|| p["sketch"].as_u64())
}

/// Approximately equal, within `tol`.
pub fn near(a: f64, b: f64, tol: f64) -> bool {
  (a - b).abs() <= tol
}

/// Sorted edge lengths of a face, keyed by face, for size tests.
pub fn lengths(t: &Topo, f: &Face) -> Vec<f64> {
  let mut l: Vec<f64> = t.face_edges(f).iter().map(|e| e.length).collect();
  l.sort_by(f64::total_cmp);
  l
}

/// A cache of topologies by body.
#[derive(Default)]
pub struct Topos(HashMap<u64, Topo>);

impl Topos {
  pub fn get(&mut self, body: u64) -> Result<&Topo, String> {
    Ok(match self.0.entry(body) {
      std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
      std::collections::hash_map::Entry::Vacant(e) => e.insert(Topo::read(body)?),
    })
  }
}

/// Formats a length in inches for messages.
pub fn inch(mm: f64) -> String {
  format!("{:.3}\"", mm / IN)
}
