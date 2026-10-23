//! User coordinate systems: named frames (an origin and three axes) saved with the design. Like
//! named views they never enter the timeline; the canvas draws them while the navigation bar's
//! Object Visibility ▸ User Coordinate Systems is on.

use fox_kernel::glam::DVec3;
use serde::{Deserialize, Serialize};

fn yes() -> bool {
  true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserCoordinateSystem {
  pub name: String,
  pub origin: [f64; 3],
  /// Unit, perpendicular X and Y axes (Z is X × Y).
  pub x: [f64; 3],
  pub y: [f64; 3],
  #[serde(default = "yes")]
  pub visible: bool,
}

impl UserCoordinateSystem {
  /// A frame at `origin` with X along `x` and Z along `z` (made perpendicular to X); `None` when
  /// they are zero or parallel.
  pub fn new(name: impl Into<String>, origin: DVec3, x: DVec3, z: DVec3) -> Option<UserCoordinateSystem> {
    let x = x.try_normalize()?;
    let y = z.cross(x).try_normalize()?;
    Some(UserCoordinateSystem {
      name: name.into(),
      origin: origin.to_array(),
      x: x.to_array(),
      y: y.to_array(),
      visible: true,
    })
  }

  /// Its X, Y and Z axes.
  pub fn axes(&self) -> [DVec3; 3] {
    let (x, y) = (DVec3::from(self.x), DVec3::from(self.y));
    [x, y, x.cross(y)]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn axes_are_made_square() {
    let u =
      UserCoordinateSystem::new("UCS1", DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0), DVec3::new(0.0, 0.3, 1.0)).unwrap();
    let [x, y, z] = u.axes();
    assert!(x.abs_diff_eq(DVec3::X, 1e-12) && x.dot(y).abs() < 1e-12 && y.dot(z).abs() < 1e-12);
    assert!(z.dot(DVec3::new(0.0, 0.3, 1.0)) > 0.0, "Z keeps its side");
    assert!(UserCoordinateSystem::new("bad", DVec3::ZERO, DVec3::X, DVec3::X).is_none());
  }
}
