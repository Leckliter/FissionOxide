//! Nesting parts on sheets: copies of each part placed on as few rectangular sheets as it takes,
//! `spacing` apart and `margin` in from the sheet's edges, turned through the allowed rotations
//! (and mirrored if allowed).
//!
//! The geometry is [`jagua_rs`]'s collision detection engine: each part's outline becomes an
//! item inflated by half the spacing, each sheet a layout. The placement is a bottom-left fill:
//! the largest parts first, each at the lowest (then leftmost) spot where it fits on the first
//! sheet with room, found on a grid and then slid down and left until it touches.

use std::sync::Arc;

use jagua_rs::collision_detection::CDEConfig;
use jagua_rs::collision_detection::hazards::filter::NoFilter;
use jagua_rs::entities::{Container, Item, Layout};
use jagua_rs::geometry::fail_fast::SPSurrogateConfig;
use jagua_rs::geometry::geo_enums::RotationRange;
use jagua_rs::geometry::geo_traits::Transformable;
use jagua_rs::geometry::primitives::{Point, Rect, SPolygon};
use jagua_rs::geometry::shape_modification::{ShapeModifyConfig, ShapeModifyMode};
use jagua_rs::geometry::{AllowedOrientations, DTransformation, OriginalShape};
use serde::{Deserialize, Serialize};

use crate::{Contour, P, dist, nesting};

/// Sheet and placement settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NestParams {
  /// Sheet size (mm).
  pub sheet_width: f64,
  pub sheet_height: f64,
  /// Gap between parts (mm); at least the kerf.
  pub spacing: f64,
  /// Uncut border along the sheet's edges (mm).
  pub margin: f64,
  /// Orientations a part may take: 1 = as drawn, 2 = also turned 180°, 4 = every 90°, n = every
  /// 360/n degrees.
  pub rotations: u32,
  /// Parts may also be flipped over (for material with no good side).
  pub mirror: bool,
  /// Sheets to use at most; parts that don't fit on them are reported as not placed.
  pub max_sheets: u32,
}

impl Default for NestParams {
  fn default() -> Self {
    NestParams {
      sheet_width: 1250.0,
      sheet_height: 2500.0,
      spacing: 3.0,
      margin: 5.0,
      rotations: 4,
      mirror: false,
      max_sheets: 100,
    }
  }
}

/// A part to nest: an outline and its holes, as drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NestPart {
  pub name: String,
  /// The outline first, then its holes.
  pub contours: Vec<Contour>,
  pub quantity: u32,
  /// Lead-in positions picked on this part (see [`crate::CutParams::lead_points`]); they move
  /// with every copy.
  #[serde(default)]
  pub lead_points: Vec<P>,
}

impl NestPart {
  /// Area of material in the part (outline less holes).
  pub fn area(&self) -> f64 {
    let mut a = self.contours.first().map(|c| c.area().abs()).unwrap_or(0.0);
    for h in self.contours.iter().skip(1) {
      a -= h.area().abs();
    }
    a.max(0.0)
  }
}

/// Splits contours into parts: each outline with the holes directly in it. A part cut out of
/// another's hole is a part of its own.
pub fn parts_of(contours: &[Contour]) -> Vec<Vec<Contour>> {
  let (depth, parent) = nesting(contours);
  let mut out: Vec<(usize, Vec<Contour>)> = Vec::new();
  for (i, c) in contours.iter().enumerate() {
    if depth[i] % 2 == 0 {
      out.push((i, vec![c.clone()]));
    }
  }
  for (i, c) in contours.iter().enumerate() {
    if depth[i] % 2 == 1
      && let Some(p) = parent[i]
      && let Some(part) = out.iter_mut().find(|o| o.0 == p)
    {
      part.1.push(c.clone());
    }
  }
  out.into_iter().map(|o| o.1).collect()
}

/// Where a copy of a part goes: mirrored (y → −y) if asked, turned `rotation` degrees about the
/// part's origin, then moved by `offset`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
  pub part: usize,
  pub rotation: f64,
  #[serde(default)]
  pub mirrored: bool,
  pub offset: P,
}

impl Placement {
  pub fn apply(&self, p: P) -> P {
    let y = if self.mirrored { -p[1] } else { p[1] };
    let (s, c) = self.rotation.to_radians().sin_cos();
    [c * p[0] - s * y + self.offset[0], s * p[0] + c * y + self.offset[1]]
  }

  /// Where a point of the sheet was on the part as drawn.
  pub fn invert(&self, q: P) -> P {
    let (s, c) = self.rotation.to_radians().sin_cos();
    let (x, y) = (q[0] - self.offset[0], q[1] - self.offset[1]);
    let p = [c * x + s * y, -s * x + c * y];
    if self.mirrored { [p[0], -p[1]] } else { p }
  }

  pub fn contour(&self, c: &Contour) -> Contour {
    let segs = c
      .segs
      .iter()
      .map(|s| crate::Seg {
        a: self.apply(s.a),
        b: self.apply(s.b),
        bulge: if self.mirrored { -s.bulge } else { s.bulge },
      })
      .collect();
    Contour { segs }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NestSheet {
  pub placements: Vec<Placement>,
  /// Material in the parts over the sheet's area.
  pub utilisation: f64,
  /// Top-right corner of the parts on the sheet (what's left above or beside is offcut).
  pub extent: P,
}

/// A finished nest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nest {
  pub width: f64,
  pub height: f64,
  pub sheets: Vec<NestSheet>,
  /// Copies that didn't fit, per part index: (part, how many).
  pub unplaced: Vec<(usize, u32)>,
  pub warnings: Vec<String>,
}

impl Nest {
  /// Material in the placed parts over the area of the sheets used.
  pub fn utilisation(&self, parts: &[NestPart]) -> f64 {
    let area: f64 = self.sheets.iter().flat_map(|s| &s.placements).map(|p| parts[p.part].area()).sum();
    let sheets = self.sheets.len() as f64 * self.width * self.height;
    if sheets > 0.0 { area / sheets } else { 0.0 }
  }

  pub fn placed(&self) -> usize {
    self.sheets.iter().map(|s| s.placements.len()).sum()
  }

  /// The contours on sheet `k` (sheet coordinates, origin at its lower-left corner) and the
  /// lead points moved with their parts.
  pub fn sheet(&self, parts: &[NestPart], k: usize) -> (Vec<Contour>, Vec<P>) {
    let mut cs = Vec::new();
    let mut leads = Vec::new();
    for pl in self.sheets.get(k).map(|s| &s.placements[..]).unwrap_or(&[]) {
      let part = &parts[pl.part];
      cs.extend(part.contours.iter().map(|c| pl.contour(c)));
      leads.extend(part.lead_points.iter().map(|p| pl.apply(*p)));
    }
    (cs, leads)
  }
}

/// Chord tolerance of the polygons that stand in for the parts (mm). The spacing covers it.
const FLAT_TOL: f64 = 0.05;

fn polygon(c: &Contour) -> Vec<P> {
  let mut pts: Vec<P> = Vec::new();
  for p in c.polygon(FLAT_TOL) {
    if pts.last().is_none_or(|q| dist(*q, p) > 1e-3) {
      pts.push(p);
    }
  }
  while pts.len() > 1 && dist(pts[0], pts[pts.len() - 1]) <= 1e-3 {
    pts.pop();
  }
  pts
}

fn centroid(pts: &[P]) -> P {
  let n = pts.len().max(1) as f64;
  let s = pts.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
  [s[0] / n, s[1] / n]
}

fn cde_config() -> CDEConfig {
  CDEConfig {
    quadtree_depth: 5,
    cd_threshold: 16,
    item_surrogate_config: SPSurrogateConfig {
      n_pole_limits: [(64, 0.0), (16, 0.8), (8, 0.9)],
      ff_pole_area_ratio: 0.0,
      n_ff_piers: 0,
    },
  }
}

/// A part as jagua-rs sees it, with the shift that centred it.
struct Shape {
  item: Arc<Item>,
  centre: P,
  angles: Vec<f64>,
}

fn item(idx: usize, part: &NestPart, params: &NestParams) -> Result<Shape, String> {
  let outline = part.contours.first().ok_or("a part has no outline")?;
  let pts = polygon(outline);
  let centre = centroid(&pts);
  let poly = SPolygon::new(pts.iter().map(|p| Point((p[0] - centre[0]) as f32, (p[1] - centre[1]) as f32)).collect())
    .map_err(|e| format!("part '{}': {e}", part.name))?;
  let n = params.rotations.clamp(1, 360);
  let angles: Vec<f64> = (0..n).map(|i| 360.0 * i as f64 / n as f64).collect();
  let rotations = RotationRange::Discrete(angles.iter().map(|a| a.to_radians() as f32).collect());
  let orient = AllowedOrientations::new(rotations, if params.mirror { vec![0.0] } else { Vec::new() })
    .map_err(|e| e.to_string())?;
  let shape = OriginalShape {
    shape: poly,
    pre_transform: DTransformation::empty(),
    modify_mode: ShapeModifyMode::Inflate,
    modify_config: ShapeModifyConfig {
      simplify_tolerance: Some(0.001),
      offset: (params.spacing > 0.0).then_some(params.spacing as f32 / 2.0),
      narrow_concavity_cutoff: None,
    },
  };
  let item = Item::new(idx, idx as u64, shape, orient, None, cde_config().item_surrogate_config)
    .map_err(|e| format!("part '{}': {e}", part.name))?;
  Ok(Shape { item: Arc::new(item), centre, angles })
}

fn container(params: &NestParams) -> Result<Container, String> {
  // the parts carry half the spacing around them: the usable rectangle is the sheet less the
  // margin, grown by that half
  let inset = (params.margin - params.spacing / 2.0) as f32;
  let (w, h) = (params.sheet_width as f32, params.sheet_height as f32);
  let rect = Rect::try_new(inset, inset, w - inset, h - inset).map_err(|_| "the sheet is smaller than its margins")?;
  let shape = OriginalShape {
    shape: SPolygon::from(rect),
    pre_transform: DTransformation::empty(),
    modify_mode: ShapeModifyMode::Deflate,
    modify_config: ShapeModifyConfig::default(),
  };
  Container::new(shape, Vec::new(), cde_config()).map_err(|e| e.to_string())
}

/// Where a part fits on a layout: (transformation, top of the part, left of the part).
fn find_spot(layout: &Layout, item: &Item, angles: &[f64], mirror: bool) -> Option<(DTransformation, f32, f32)> {
  let bounds = layout.container().outer_cd().bbox();
  let mut best: Option<(DTransformation, f32, f32)> = None;
  for &mirrored in if mirror { &[false, true][..] } else { &[false][..] } {
    for &a in angles {
      let r = a.to_radians() as f32;
      let turned =
        item.shape_cd().transform_clone(&DTransformation::new(r, (0.0, 0.0)).with_reflection(mirrored).compose());
      let b = turned.bbox();
      // a hair in from the container's edges, where touching counts as a collision
      const IN: f32 = 0.01;
      let (x0, x1) = (bounds.x_min - b.x_min + IN, bounds.x_max - b.x_max - IN);
      let (y0, y1) = (bounds.y_min - b.y_min + IN, bounds.y_max - b.y_max - IN);
      if x1 < x0 || y1 < y0 {
        continue;
      }
      let fits = |tx: f32, ty: f32| {
        let t = DTransformation::new(r, (tx, ty)).with_reflection(mirrored);
        let s = item.shape_cd().transform_clone(&t.compose());
        !layout.cde().detect_poly_collision(&s, &NoFilter)
      };
      let step = (b.width().min(b.height()) / 4.0).max(0.25);
      let nx = (((x1 - x0) / step).ceil() as usize).clamp(2, 120);
      let ny = (((y1 - y0) / step).ceil() as usize).clamp(2, 240);
      let at = |i: usize, n: usize, lo: f32, hi: f32| if n == 0 { lo } else { lo + (hi - lo) * i as f32 / n as f32 };
      'rows: for j in 0..=ny {
        let ty = at(j, ny, y0, y1);
        if best.as_ref().is_some_and(|bst| ty + b.y_max > bst.1 + 1e-3) {
          break;
        }
        for i in 0..=nx {
          let tx = at(i, nx, x0, x1);
          if !fits(tx, ty) {
            continue;
          }
          // slide down and left until it touches
          let (mut x, mut y) = (tx, ty);
          let mut d = step;
          while d > 0.02 {
            let mut moved = false;
            if y - d >= y0 && fits(x, y - d) {
              y -= d;
              moved = true;
            }
            if x - d >= x0 && fits(x - d, y) {
              x -= d;
              moved = true;
            }
            if !moved {
              d /= 2.0;
            }
          }
          let (top, left) = (y + b.y_max, x + b.x_min);
          if best.as_ref().is_none_or(|bst| top < bst.1 - 1e-3 || (top < bst.1 + 1e-3 && left < bst.2)) {
            best = Some((DTransformation::new(r, (x, y)).with_reflection(mirrored), top, left));
          }
          break 'rows;
        }
      }
    }
  }
  best
}

/// Nests the parts on as few sheets as it takes.
pub fn nest(parts: &[NestPart], params: &NestParams) -> Result<Nest, String> {
  if !(params.sheet_width > 0.0 && params.sheet_height > 0.0) {
    return Err("the sheet needs a width and a height".into());
  }
  if params.spacing < 0.0 || params.margin < 0.0 {
    return Err("spacing and margin can't be negative".into());
  }
  let shapes: Vec<Shape> = parts.iter().enumerate().map(|(i, p)| item(i, p, params)).collect::<Result<_, _>>()?;
  let cont = container(params)?;
  let mut order: Vec<usize> = (0..parts.len()).filter(|&i| parts[i].quantity > 0).collect();
  order.sort_by(|&a, &b| parts[b].area().total_cmp(&parts[a].area()).then(a.cmp(&b)));

  let mut layouts: Vec<Layout> = Vec::new();
  let mut placements: Vec<Vec<Placement>> = Vec::new();
  let mut unplaced: Vec<(usize, u32)> = Vec::new();
  let mut warnings = Vec::new();
  for &i in &order {
    let sh = &shapes[i];
    // sheets only fill up: a copy that didn't fit on a sheet won't fit there later
    let mut first = 0;
    let mut placed = 0;
    let mut too_big = false;
    for _ in 0..parts[i].quantity {
      let mut done = false;
      let mut k = first;
      while !done {
        if k == layouts.len() {
          if layouts.len() >= params.max_sheets as usize {
            break;
          }
          layouts.push(Layout::new(cont.clone()));
          placements.push(Vec::new());
        }
        match find_spot(&layouts[k], &sh.item, &sh.angles, params.mirror) {
          Some((t, _, _)) => {
            layouts[k].place_item(&sh.item, t);
            let (tx, ty) = t.translation();
            let pl = Placement {
              part: i,
              rotation: f64::from(t.rotation()).to_degrees(),
              mirrored: t.reflected,
              offset: [0.0, 0.0],
            };
            // the item was centred on the outline's centroid: fold that shift into the offset
            let c = pl.apply(sh.centre);
            placements[k].push(Placement { offset: [f64::from(tx) - c[0], f64::from(ty) - c[1]], ..pl });
            first = k;
            done = true;
          }
          None if layouts[k].is_empty() => {
            // it doesn't fit on an empty sheet: drop that sheet and give up on the part
            layouts.pop();
            placements.pop();
            too_big = true;
            break;
          }
          None => k += 1,
        }
      }
      if !done {
        break;
      }
      placed += 1;
    }
    let missing = parts[i].quantity - placed;
    if too_big {
      let (lo, hi) = parts[i].contours[0].bounds();
      warnings.push(format!(
        "'{}' ({:.1} x {:.1} mm) doesn't fit on a {:.0} x {:.0} mm sheet with {:.1} mm margins",
        parts[i].name,
        hi[0] - lo[0],
        hi[1] - lo[1],
        params.sheet_width,
        params.sheet_height,
        params.margin
      ));
    } else if missing > 0 && !warnings.iter().any(|w| w.contains("sheet(s) are not enough")) {
      warnings.push(format!("{} sheet(s) are not enough for every part", params.max_sheets));
    }
    if missing > 0 {
      unplaced.push((i, missing));
    }
  }
  let sheet_area = params.sheet_width * params.sheet_height;
  let sheets = placements
    .into_iter()
    .map(|pls| {
      let area: f64 = pls.iter().map(|p| parts[p.part].area()).sum();
      let mut extent = [0.0f64, 0.0f64];
      for p in &pls {
        let (_, hi) = p.contour(&parts[p.part].contours[0]).bounds();
        extent = [extent[0].max(hi[0]), extent[1].max(hi[1])];
      }
      NestSheet { placements: pls, utilisation: area / sheet_area, extent }
    })
    .collect();
  Ok(Nest { width: params.sheet_width, height: params.sheet_height, sheets, unplaced, warnings })
}
