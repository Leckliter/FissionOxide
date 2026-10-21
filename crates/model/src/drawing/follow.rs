//! Drawing dimensions that follow the model: a measured point picked on a model edge keeps the
//! edge (an [`EdgeRef`], found by its faces' names first, see [`crate::naming`]) and which point
//! of it, and is measured where that point is now.

use fox_kernel::glam::DVec3;
use fox_kernel::{CurveKind, EdgeInfo};
use serde::{Deserialize, Serialize};

use crate::EdgeRef;
use crate::regen::Model;

/// Which point of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeAt {
  Start,
  End,
  Mid,
  /// A circle's or arc's centre.
  Center,
}

/// A point of a model edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgePoint {
  pub edge: Box<EdgeRef>,
  pub at: EdgeAt,
}

pub(super) fn is_free(f: &[Option<EdgePoint>; 2]) -> bool {
  f.iter().all(Option::is_none)
}

impl EdgePoint {
  /// Where the point is on the model now; `None` when its edge is gone.
  pub fn locate(&self, model: &Model) -> Option<DVec3> {
    let body = model.body(self.edge.body)?;
    let id = body.find_edge(&self.edge)?;
    let e = body.edges().iter().find(|e| e.id == id)?;
    // the edge may run the other way now: the end nearer the one picked
    let (s0, e0) = (DVec3::from(self.edge.start), DVec3::from(self.edge.end));
    let flipped = (e.start - s0).length() + (e.end - e0).length() > (e.end - s0).length() + (e.start - e0).length();
    Some(match self.at {
      EdgeAt::Start if flipped => e.end,
      EdgeAt::Start => e.start,
      EdgeAt::End if flipped => e.start,
      EdgeAt::End => e.end,
      EdgeAt::Mid => e.mid,
      EdgeAt::Center => e.center,
    })
  }

  /// The edge point of a visible body at `p` (within 1e-6 of an edge's end, middle or centre).
  pub fn at_point(model: &Model, p: DVec3) -> Option<EdgePoint> {
    let near = |q: DVec3| (q - p).length() <= 1e-6 * p.length().max(1.0);
    let which = |e: &EdgeInfo| {
      if near(e.start) {
        Some(EdgeAt::Start)
      } else if near(e.end) {
        Some(EdgeAt::End)
      } else if e.kind == CurveKind::Circle && near(e.center) {
        Some(EdgeAt::Center)
      } else if near(e.mid) {
        Some(EdgeAt::Mid)
      } else {
        None
      }
    };
    model.bodies.iter().find_map(|b| {
      b.edges()
        .iter()
        .filter(|e| e.kind != CurveKind::Degenerate)
        .find_map(|e| which(e).map(|at| EdgePoint { edge: Box::new(b.edge_ref(e, [0; 2])), at }))
    })
  }
}
