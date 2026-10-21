//! 3D adaptive roughing: the stock minus the part, cleared in levels a step-down apart (plus a
//! level at each flat area of the part). At each level the tool's centre may go where the
//! tool, grown by the stock to leave, clears the part ("drop cutter" on the part's
//! heightfield); the clearing grows in from the open air around the stock, or from a helix in
//! closed areas, by the step-over each pass (see [`super::adaptive`]), so the tool's
//! engagement stays about constant.
//!
//! Rest roughing runs the setup's earlier toolpaths through the stock first (see
//! [`super::rest`]): at each level, wherever the tool would find no material left it counts
//! as open air, so the clearing only grows into what a larger tool left.

use fox_kernel::SurfaceKind;
use fox_kernel::glam::DVec2;

use super::adaptive::{Walls, helix_radius, plan_on, replay};
use super::region::Grid;
use super::rest::LEFT;
use super::sim::Heightfield;
use super::{Operation, Path, Setup, Tool, ToolKind};
use crate::regen::Model;

/// Signed distances on a grid (positive where the tool may go), sampled bilinearly.
struct Field(Grid);

impl Walls for Field {
  fn sdf(&self, p: DVec2, max: f64) -> f64 {
    self.0.sample(p).clamp(-max, max)
  }
}

/// The tool grown by `extra` all round: its tip `extra` lower, its sides `extra` further out.
fn grown(tool: &Tool, extra: f64) -> Tool {
  let mut t = tool.clone();
  t.diameter += 2.0 * extra;
  if t.kind == ToolKind::BullNose {
    t.corner_radius += extra;
  }
  t
}

/// `left`: the stock the earlier operations left, for rest roughing.
pub(super) fn rough(
  p: &mut Path,
  setup: &Setup,
  model: &Model,
  op: &Operation,
  left: Option<&Heightfield>,
) -> Result<(), String> {
  let (slo, shi) = super::stock(setup, model)?;
  let (plo, _) = super::part_box(setup, model)?;
  let r = op.tool.radius();
  let s = op.tool.diameter * op.stepover;
  let stl = op.stock_to_leave.max(0.0);
  let extent = (shi - slo).truncate().max_element().max(1e-3);
  let h = (s / 3.0).min(r / 4.0).max(extent / 400.0).max(0.02);
  // tool centres from the open air around the stock in
  let margin = r + s + 4.0 * h;
  let (lo, hi) = (slo.truncate() - DVec2::splat(margin), shi.truncate() + DVec2::splat(margin));
  let mut grid = Grid::new(lo, hi, h, 1, 0.0);
  let n = grid.v.len();
  // the lowest the (grown) tool's tip may go at each node: −∞ off the part
  let hp = (h / 2.0).min(0.25).max(extent / 1000.0);
  let fat = grown(&op.tool, stl);
  let pad = DVec2::splat(fat.radius() + hp);
  let mut part = Heightfield::new(lo - pad, hi + pad, hp, 0.0);
  let meshes: Vec<&fox_kernel::TriMesh> = super::bodies(setup, model).into_iter().filter_map(|b| b.mesh()).collect();
  if meshes.is_empty() {
    return Err("the part has no surface to machine".into());
  }
  part.rasterize_over(&meshes);
  let low: Vec<f64> = (0..n).map(|i| part.drop_tool_over(&fat, grid.node(i)) + stl).collect();
  // open air: the tool doesn't reach the stock
  let air: Vec<bool> = (0..n)
    .map(|i| {
      let q = grid.node(i);
      (q - q.clamp(slo.truncate(), shi.truncate())).length() >= r - 1e-9
    })
    .collect();
  // levels a step-down apart, and one at each flat area facing up
  let mut zs = super::levels(shi.z, plo.z, op.stepdown);
  for b in super::bodies(setup, model) {
    for f in b.faces() {
      let z = f.origin.z + stl;
      if f.kind == SurfaceKind::Plane
        && f.normal.z > 0.999
        && z < shi.z - 1e-6
        && z > plo.z + 1e-6
        && zs.iter().all(|l| (l - z).abs() > 0.05)
      {
        zs.push(z);
      }
    }
  }
  zs.sort_by(|a, b| b.total_cmp(a));
  let wall = h;
  let cap = wall + helix_radius(op).max(s) + 4.0 * h;
  let mut above = shi.z;
  let mut any = false;
  // columns well inside the tool only, so the wall beside a tangent pass doesn't count
  let reach = left.map_or(0.0, |l| (r - l.h * 0.5).max(l.h * 0.5));
  for z in zs {
    let blocked: Vec<bool> = low.iter().map(|d| *d > z + 1e-9).collect();
    // rest roughing: nothing left under the tool counts as air
    let air: Vec<bool> = match left {
      Some(l) => (0..n).map(|i| air[i] || (!blocked[i] && !l.material_above(grid.node(i), reach, z, LEFT))).collect(),
      None => air.clone(),
    };
    // anything to cut at this level?
    if !(0..n).any(|i| !air[i] && !blocked[i]) {
      continue;
    }
    let to_blocked = grid.distance_to(&blocked);
    let open: Vec<bool> = blocked.iter().map(|b| !b).collect();
    let to_open = grid.distance_to(&open);
    for i in 0..n {
      let d = if blocked[i] { -(to_open[i] - h / 2.0).max(0.0) } else { to_blocked[i] - h / 2.0 };
      grid.v[i] = d.clamp(-cap, cap);
    }
    let walls = Field(grid.clone());
    let plan = match plan_on(grid.clone(), &walls, wall, cap, op, air) {
      Ok(plan) => plan,
      // nowhere the tool fits at this level
      Err(e) if e.contains("doesn't fit") => continue,
      Err(e) => return Err(e),
    };
    if plan.is_empty() {
      continue;
    }
    replay(p, &plan, op, z, above);
    any = true;
    above = z;
  }
  if !any {
    return Err(if left.is_some() {
      "the earlier operations left nothing this tool can reach".into()
    } else {
      "there is no stock around the part the tool can reach".into()
    });
  }
  Ok(())
}
