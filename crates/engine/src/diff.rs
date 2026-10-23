//! Visual diff between two versions of a design (`hub.diff`, the data panel's Compare): the
//! geometry the newer version adds is drawn green, what it removes red, and what both share as a
//! grey ghost (its edges only, so the changes inside or behind it stay visible).
//!
//! Solids are compared exactly with booleans: added = new − old, removed = old − new, unchanged =
//! old ∩ new. Bodies the booleans can't handle (open surfaces, failing operations) are compared
//! whole: a body identical in both versions is unchanged, others are added or removed.

use fox_kernel::{BoolOp, EdgeLine, Shape, TriMesh};
use fox_render::{Mesh, NO_ID, Polyline, Scene};
use serde_json::{Value, json};

use crate::{CmdError, CmdResult, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
  Added,
  Removed,
  Unchanged,
}

/// A piece of the comparison, tessellated for display.
pub struct DiffPart {
  pub kind: DiffKind,
  pub mesh: TriMesh,
  pub edges: Vec<EdgeLine>,
  pub volume: f64,
}

/// The comparison shown in the viewport instead of the design's own bodies.
pub struct DiffView {
  pub item: String,
  pub a: u32,
  pub b: u32,
  pub parts: Vec<DiffPart>,
}

impl DiffView {
  /// Total volume (mm³) of each kind.
  pub fn volume(&self, kind: DiffKind) -> f64 {
    self.parts.iter().filter(|p| p.kind == kind).map(|p| p.volume).sum()
  }
}

pub const ADDED_COLOR: [u8; 4] = [46, 170, 80, 255];
pub const REMOVED_COLOR: [u8; 4] = [214, 58, 48, 255];
const GHOST_COLOR: [u8; 4] = [120, 126, 136, 150];

/// The pieces of `old` and `new` that were removed, added or kept.
pub fn compare(old: &[&Shape], new: &[&Shape]) -> Vec<(DiffKind, Shape)> {
  let solid = |v: &[&Shape]| -> (Vec<Shape>, Vec<Shape>) {
    let mut solids = Vec::new();
    let mut other = Vec::new();
    for s in v {
      match s.solids() {
        Ok(list) if !list.is_empty() && s.solid_count() > 0 => solids.extend(list),
        _ => other.push((*s).clone()),
      }
    }
    (solids, other)
  };
  let (old_solids, mut old_other) = solid(old);
  let (new_solids, mut new_other) = solid(new);
  let mut out = Vec::new();
  match boolean_diff(&old_solids, &new_solids) {
    Some(parts) => out.extend(parts),
    None => {
      old_other.extend(old_solids);
      new_other.extend(new_solids);
    }
  }
  // whole-body comparison for the rest
  let brep = |s: &Shape| s.to_brep().unwrap_or_default();
  let old_breps: Vec<String> = old_other.iter().map(brep).collect();
  let new_breps: Vec<String> = new_other.iter().map(brep).collect();
  for (s, b) in new_other.into_iter().zip(&new_breps) {
    out.push((if old_breps.contains(b) { DiffKind::Unchanged } else { DiffKind::Added }, s));
  }
  for (s, b) in old_other.into_iter().zip(&old_breps) {
    if !new_breps.contains(b) {
      out.push((DiffKind::Removed, s));
    }
  }
  out
}

/// Exact differences of two sets of solids; `None` if a boolean fails.
fn boolean_diff(old: &[Shape], new: &[Shape]) -> Option<Vec<(DiffKind, Shape)>> {
  let mut out = Vec::new();
  match (old.is_empty(), new.is_empty()) {
    (true, true) => {}
    (true, false) => out.extend(new.iter().map(|s| (DiffKind::Added, s.clone()))),
    (false, true) => out.extend(old.iter().map(|s| (DiffKind::Removed, s.clone()))),
    (false, false) => {
      let a = Shape::compound(old).ok()?;
      let b = Shape::compound(new).ok()?;
      for (kind, r) in [
        (DiffKind::Added, b.boolean(&a, BoolOp::Subtract)),
        (DiffKind::Removed, a.boolean(&b, BoolOp::Subtract)),
        (DiffKind::Unchanged, b.boolean(&a, BoolOp::Intersect)),
      ] {
        out.push((kind, r.ok()?));
      }
    }
  }
  Some(out)
}

/// Tessellates comparison pieces for display (`tol` in mm), dropping empty ones.
pub fn parts(pieces: Vec<(DiffKind, Shape)>, tol: f64) -> Vec<DiffPart> {
  let mut out = Vec::new();
  for (kind, s) in pieces {
    let Ok(mesh) = s.tessellate(tol) else { continue };
    if mesh.indices.is_empty() {
      continue;
    }
    let volume = s.mass().map(|m| m.volume.abs()).unwrap_or(0.0);
    // slivers left by tolerances in the booleans aren't changes
    if kind != DiffKind::Unchanged && volume < 1e-6 && s.solid_count() > 0 {
      continue;
    }
    let edges = s.edge_lines(tol).unwrap_or_default();
    out.push(DiffPart { kind, mesh, edges, volume });
  }
  out
}

/// Draws the comparison: added green, removed red, unchanged as ghosted edges.
pub fn draw(d: &DiffView, scene: &mut Scene) {
  for p in &d.parts {
    let (face, edge) = match p.kind {
      DiffKind::Added => (Some(ADDED_COLOR), darker(ADDED_COLOR)),
      DiffKind::Removed => (Some(REMOVED_COLOR), darker(REMOVED_COLOR)),
      DiffKind::Unchanged => (None, GHOST_COLOR),
    };
    if let Some(color) = face {
      scene.meshes.push(Mesh {
        positions: p.mesh.positions.clone(),
        normals: p.mesh.normals.clone(),
        indices: p.mesh.indices.clone(),
        tri_ids: Vec::new(),
        id: NO_ID,
        color,
        highlight: None,
        highlight_color: color,
        colors: Vec::new(),
        analyze: false,
        unanalyzed: false,
        unlit: false,
        texture: None,
        outline: None,
      });
    }
    for e in &p.edges {
      scene.lines.push(Polyline {
        points: e.points.iter().map(|q| q.as_vec3().to_array()).collect(),
        color: edge,
        width: if p.kind == DiffKind::Unchanged { 1.0 } else { 1.5 },
        id: NO_ID,
        on_top: false,
        dashed: false,
        dash_dot: false,
        behind: false,
      });
    }
  }
}

fn darker(c: [u8; 4]) -> [u8; 4] {
  [c[0] / 2, c[1] / 2, c[2] / 2, 255]
}

/// `hub.diff`: compare version `a` of a design with `b` (default the latest) and show it in the
/// viewport; `{clear: true}` goes back to the design.
pub(crate) fn run(s: &mut Session, st: &fox_store::Store, p: &Value) -> CmdResult {
  if p.get("clear").and_then(Value::as_bool).unwrap_or(false) {
    s.diff = None;
    return Ok(json!({}));
  }
  let item = crate::hub::item_of(s, p)?;
  let it = st.item(&item)?;
  let a = crate::hub::req_version(p, "a")?;
  let b = crate::hub::version_of(p, "b")?.or(it.latest().map(|v| v.number)).unwrap_or(a);
  let old = crate::hub::session_for(st, &item, Some(a))?;
  let new = crate::hub::session_for(st, &item, Some(b))?;
  let shapes = |s: &Session| -> Vec<Shape> { s.model.bodies.iter().map(|b| b.shape.clone()).collect() };
  let (old_shapes, new_shapes) = (shapes(&old), shapes(&new));
  if old_shapes.is_empty() && new_shapes.is_empty() {
    return Err(CmdError::new(format!("neither v{a} nor v{b} has bodies to compare")));
  }
  let size = old.model.bodies.iter().chain(&new.model.bodies).map(|b| b.diagonal()).fold(0.0, f64::max);
  let pieces = compare(&old_shapes.iter().collect::<Vec<_>>(), &new_shapes.iter().collect::<Vec<_>>());
  let view = DiffView { item: item.clone(), a, b, parts: parts(pieces, (size * 0.001).clamp(0.002, 0.5)) };
  let out = json!({
    "item": item,
    "a": a,
    "b": b,
    "added_mm3": view.volume(DiffKind::Added),
    "removed_mm3": view.volume(DiffKind::Removed),
    "unchanged_mm3": view.volume(DiffKind::Unchanged),
    "parts": view.parts.iter().map(|p| json!({"kind": p.kind, "volume_mm3": p.volume})).collect::<Vec<_>>(),
  });
  s.diff = Some(view);
  Ok(out)
}
