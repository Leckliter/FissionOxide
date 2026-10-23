//! Spun profiles (Fusion's Spun Profile): the outline of the region bodies sweep through as they
//! turn about an axis, drawn in the half of the sketch plane on one side of the axis. The sketch
//! keeps what to trace (bodies, axis, side) and owns the fixed curves of the outline; fox-model
//! works the outline out at every regeneration and hands it over with
//! [`Sketch::set_spun_outline`], so the curves follow the bodies.

use serde::{Deserialize, Serialize};

use crate::{CurveId, CurveKind, PointId, PtSpec, Sketch, SketchError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SpunId(pub u32);

/// The axis a spun profile turns the bodies about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SpunAxis {
  /// A line in model space: a point on it and its direction.
  World { origin: [f64; 3], dir: [f64; 3] },
  /// A line of this sketch.
  Line { curve: CurveId },
}

/// One piece of a spun outline, in sketch coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SpunSeg {
  Line {
    a: [f64; 2],
    b: [f64; 2],
  },
  Arc {
    a: [f64; 2],
    mid: [f64; 2],
    b: [f64; 2],
  },
  Circle {
    center: [f64; 2],
    radius: f64,
  },
  /// A spline through these points.
  Spline {
    points: Vec<[f64; 2]>,
  },
  /// A chain of cubic Bézier pieces (points 0, 3, 6, … on the curve, handles between).
  Bezier {
    points: Vec<[f64; 2]>,
  },
  /// The axis, as a construction centre line.
  Centerline {
    a: [f64; 2],
    b: [f64; 2],
  },
  /// The axis, as a plain line.
  Axis {
    a: [f64; 2],
    b: [f64; 2],
  },
}

/// How a spun profile draws its axis: Fusion's Project Axis and Add Centerline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpunAxisLine {
  /// Project Axis off: no axis.
  None,
  /// Project Axis on, Add Centerline off: a plain line.
  Line,
  /// Both on: a construction centre line.
  Centerline,
}

impl SketchSpun {
  /// How this profile draws its axis.
  pub fn axis_drawn(&self) -> SpunAxisLine {
    match (self.centerline, self.axis_line) {
      (true, _) => SpunAxisLine::Centerline,
      (false, true) => SpunAxisLine::Line,
      (false, false) => SpunAxisLine::None,
    }
  }
}

impl SpunSeg {
  pub(crate) fn close(&self, o: &SpunSeg, tol: f64) -> bool {
    let p = |a: &[f64; 2], b: &[f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) <= tol;
    match (self, o) {
      (SpunSeg::Line { a, b }, SpunSeg::Line { a: c, b: d })
      | (SpunSeg::Centerline { a, b }, SpunSeg::Centerline { a: c, b: d })
      | (SpunSeg::Axis { a, b }, SpunSeg::Axis { a: c, b: d }) => p(a, c) && p(b, d),
      (SpunSeg::Arc { a, mid, b }, SpunSeg::Arc { a: c, mid: m, b: d }) => p(a, c) && p(mid, m) && p(b, d),
      (SpunSeg::Circle { center, radius }, SpunSeg::Circle { center: c, radius: r }) => {
        p(center, c) && (radius - r).abs() <= tol
      }
      (SpunSeg::Spline { points }, SpunSeg::Spline { points: q })
      | (SpunSeg::Bezier { points }, SpunSeg::Bezier { points: q }) => {
        points.len() == q.len() && points.iter().zip(q).all(|(a, b)| p(a, b))
      }
      _ => false,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SketchSpun {
  pub id: SpunId,
  /// The model's ids of the bodies traced.
  pub bodies: Vec<u32>,
  pub axis: SpunAxis,
  /// Draw on the other side of the axis.
  #[serde(default)]
  pub flip: bool,
  /// Add the axis as a construction centre line.
  #[serde(default)]
  pub centerline: bool,
  /// Without `centerline`: add the axis as a plain line (Fusion's Project Axis with Add
  /// Centerline off).
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub axis_line: bool,
  /// How far the outline's lines and arcs may stray from the bodies' true outline (mm); 0 fits
  /// to the bodies' size.
  #[serde(default, skip_serializing_if = "is_zero")]
  pub tolerance: f64,
  /// The outline last generated.
  #[serde(default)]
  pub outline: Vec<SpunSeg>,
  /// The curves and points the outline made (all fixed).
  #[serde(default)]
  pub curves: Vec<CurveId>,
  #[serde(default)]
  pub points: Vec<PointId>,
  /// A plane section (Fusion's Intersect) rather than a spun outline: the curves where the bodies
  /// (or just `faces` of them) cut the sketch plane. `axis`, `flip` and `centerline` are unused.
  #[serde(default)]
  pub section: bool,
  /// Intersect: the faces sectioned, as `[body, face]` (empty = whole bodies).
  #[serde(default)]
  pub faces: Vec<[u32; 2]>,
}

fn is_zero(v: &f64) -> bool {
  *v == 0.0
}

impl Sketch {
  pub fn spun(&self, id: SpunId) -> Option<&SketchSpun> {
    self.spuns.iter().find(|s| s.id == id)
  }

  /// The spun profile a curve belongs to.
  pub fn spun_of(&self, curve: CurveId) -> Option<SpunId> {
    self.spuns.iter().find(|s| s.curves.contains(&curve)).map(|s| s.id)
  }

  /// Adds a spun profile of `bodies` about `axis`, drawing the axis as `line` says and fitting
  /// its outline to `tolerance` (mm, 0: to the bodies' size); it has no curves until the model
  /// gives it an outline.
  pub fn add_spun(
    &mut self,
    bodies: Vec<u32>,
    axis: SpunAxis,
    flip: bool,
    line: SpunAxisLine,
    tolerance: f64,
  ) -> SpunId {
    let id = SpunId(self.alloc());
    self.spuns.push(SketchSpun {
      id,
      bodies,
      axis,
      flip,
      centerline: line == SpunAxisLine::Centerline,
      axis_line: line == SpunAxisLine::Line,
      tolerance,
      outline: Vec::new(),
      curves: Vec::new(),
      points: Vec::new(),
      section: false,
      faces: Vec::new(),
    });
    id
  }

  /// Adds a plane section (Fusion's Intersect) of `bodies`, or of `faces` (`[body, face]`) of
  /// them; it has no curves until the model gives it an outline.
  pub fn add_section(&mut self, bodies: Vec<u32>, faces: Vec<[u32; 2]>) -> SpunId {
    let id = SpunId(self.alloc());
    self.spuns.push(SketchSpun {
      id,
      bodies,
      axis: SpunAxis::World { origin: [0.0; 3], dir: [0.0, 0.0, 1.0] },
      flip: false,
      centerline: false,
      axis_line: false,
      tolerance: 0.0,
      outline: Vec::new(),
      curves: Vec::new(),
      points: Vec::new(),
      section: true,
      faces,
    });
    id
  }

  /// Adds `outline` as plain fixed curves that belong to no spun profile or section (an Intersect
  /// without its projection link). Returns the curves made.
  pub fn add_outline(&mut self, outline: &[SpunSeg]) -> Result<Vec<CurveId>, SketchError> {
    Ok(self.place_outline(outline)?.0)
  }

  /// Replaces a spun profile's curves with `outline`. Returns whether anything changed (an outline
  /// within 1e-9 of the current one keeps the current curves and their ids).
  pub fn set_spun_outline(&mut self, id: SpunId, outline: Vec<SpunSeg>) -> Result<bool, SketchError> {
    let s = self.spun(id).cloned().ok_or(SketchError::InvalidValue(format!("no spun profile {}", id.0)))?;
    if s.outline.len() == outline.len() && s.outline.iter().zip(&outline).all(|(a, b)| a.close(b, 1e-9)) {
      return Ok(false);
    }
    self.clear_spun(&s);
    let (curves, points) = self.place_outline(&outline)?;
    if let Some(s) = self.spuns.iter_mut().find(|s| s.id == id) {
      s.outline = outline;
      s.curves = curves;
      s.points = points;
    }
    Ok(true)
  }

  /// Adds the fixed curves of an outline (ends within 1e-7 shared); returns them and the points
  /// they made.
  pub(crate) fn place_outline(&mut self, outline: &[SpunSeg]) -> Result<(Vec<CurveId>, Vec<PointId>), SketchError> {
    let before: Vec<PointId> = self.points.iter().map(|p| p.id).collect();
    let mut made: Vec<(PointId, [f64; 2])> = Vec::new();
    let mut at = |sk: &mut Sketch, q: [f64; 2]| -> PointId {
      if let Some((id, _)) = made.iter().find(|(_, p)| (p[0] - q[0]).hypot(p[1] - q[1]) <= 1e-7) {
        return *id;
      }
      let id = sk.add_point(q[0], q[1]);
      made.push((id, q));
      id
    };
    let mut curves = Vec::new();
    for seg in outline {
      let c = match seg {
        SpunSeg::Line { a, b } | SpunSeg::Centerline { a, b } | SpunSeg::Axis { a, b } => {
          let (pa, pb) = (at(self, *a), at(self, *b));
          if pa == pb {
            continue;
          }
          let c = self.add_curve(CurveKind::Line { a: pa, b: pb })?;
          if matches!(seg, SpunSeg::Centerline { .. }) {
            self.set_construction(c, true)?;
          }
          c
        }
        SpunSeg::Arc { a, mid, b } => {
          let (pa, pb) = (at(self, *a), at(self, *b));
          self.add_arc_3pt(PtSpec::Point(pa), *mid, PtSpec::Point(pb))?
        }
        SpunSeg::Circle { center, radius } => {
          let c = at(self, *center);
          self.add_circle(PtSpec::Point(c), *radius)?
        }
        SpunSeg::Spline { points } => {
          let ids: Vec<PointId> = points.iter().map(|q| at(self, *q)).collect();
          self.add_curve(CurveKind::spline(ids))?
        }
        SpunSeg::Bezier { points } => {
          // handles get points of their own, even where they coincide with an end
          let ids: Vec<PointId> = points
            .iter()
            .enumerate()
            .map(|(k, q)| if k % 3 == 0 { at(self, *q) } else { self.add_point(q[0], q[1]) })
            .collect();
          self.add_curve(CurveKind::Bezier { points: ids })?
        }
      };
      curves.push(c);
    }
    let points: Vec<PointId> = self.points.iter().map(|p| p.id).filter(|p| !before.contains(p)).collect();
    for p in self.points.iter_mut().filter(|p| points.contains(&p.id)) {
      p.fixed = true;
    }
    Ok((curves, points))
  }

  /// Deletes a spun profile and its curves.
  pub fn remove_spun(&mut self, id: SpunId) -> Result<(), SketchError> {
    let i = self
      .spuns
      .iter()
      .position(|s| s.id == id)
      .ok_or(SketchError::InvalidValue(format!("no spun profile {}", id.0)))?;
    let s = self.spuns.remove(i);
    self.clear_spun(&s);
    Ok(())
  }

  fn clear_spun(&mut self, s: &SketchSpun) {
    self.curves.retain(|c| !s.curves.contains(&c.id));
    self.points.retain(|p| !s.points.contains(&p.id));
    self.constraints.retain(|c| {
      !c.kind.curves().iter().any(|x| s.curves.contains(x)) && !c.kind.points().iter().any(|x| s.points.contains(x))
    });
  }
}
