//! Symmetry kept on a form body: edits on one side are copied onto the matching vertices, edges
//! and faces on the others, as Fusion's Mirror - Internal and Circular - Internal.

use glam::{DMat4, DQuat, DVec3};
use serde::{Deserialize, Serialize};

use crate::Selection;
use crate::mesh::Cage;

/// A mirror plane through `origin` with normal `normal`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SymPlane {
  pub origin: [f64; 3],
  pub normal: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Symmetry {
  #[default]
  None,
  /// One to three (perpendicular) mirror planes.
  Mirror { planes: Vec<SymPlane> },
  /// `count` copies round the axis through `origin` along `axis`.
  Circular { origin: [f64; 3], axis: [f64; 3], count: u32 },
}

impl Symmetry {
  pub fn is_none(&self) -> bool {
    *self == Symmetry::None
  }

  /// The symmetry's transforms other than the identity.
  pub fn maps(&self) -> Vec<DMat4> {
    match self {
      Symmetry::None => Vec::new(),
      Symmetry::Mirror { planes } => {
        let mut group = vec![DMat4::IDENTITY];
        for p in planes {
          let r = reflection(DVec3::from(p.origin), DVec3::from(p.normal));
          let more: Vec<DMat4> = group.iter().map(|g| r * *g).collect();
          for m in more {
            if !group.iter().any(|g| g.abs_diff_eq(m, 1e-9)) {
              group.push(m);
            }
          }
        }
        group.into_iter().skip(1).collect()
      }
      Symmetry::Circular { origin, axis, count } => {
        let (o, a) = (DVec3::from(*origin), DVec3::from(*axis).try_normalize().unwrap_or(DVec3::Z));
        (1..*count)
          .map(|i| {
            let q = DQuat::from_axis_angle(a, std::f64::consts::TAU * f64::from(i) / f64::from(*count));
            DMat4::from_translation(o) * DMat4::from_quat(q) * DMat4::from_translation(-o)
          })
          .collect()
      }
    }
  }

  /// For each map, each vertex's partner: the vertex at its image, if any.
  pub fn partners(&self, cage: &Cage) -> Vec<(DMat4, Vec<Option<u32>>)> {
    let tol = 1e-5 * cage.size();
    self
      .maps()
      .into_iter()
      .map(|m| {
        let p = (0..cage.vertices.len())
          .map(|v| {
            let img = m.transform_point3(cage.pos(v as u32));
            let best = cage.nearest_vertex(img.to_array())?;
            (cage.pos(best as u32).distance(img) <= tol).then_some(best as u32)
          })
          .collect();
        (m, p)
      })
      .collect()
  }

  /// A selection with the matching elements on the other sides added.
  pub fn expand(&self, cage: &Cage, sel: &Selection) -> Selection {
    let mut out = sel.clone();
    for (_, p) in self.partners(cage) {
      for &v in &sel.vertices {
        if let Some(Some(w)) = p.get(v as usize) {
          out.vertices.push(*w);
        }
      }
      let edges = cage.edges();
      for e in &sel.edges {
        if let (Some(Some(a)), Some(Some(b))) = (p.get(e[0] as usize), p.get(e[1] as usize)) {
          let k = [(*a).min(*b), (*a).max(*b)];
          if edges.contains(&k) {
            out.edges.push(k);
          }
        }
      }
      for &f in &sel.faces {
        let Some(face) = cage.faces.get(f as usize) else { continue };
        let img: Option<Vec<u32>> = face.iter().map(|v| p.get(*v as usize).copied().flatten()).collect();
        let Some(mut img) = img else { continue };
        img.sort_unstable();
        if let Some(g) = cage.faces.iter().position(|g| {
          let mut g = g.clone();
          g.sort_unstable();
          g == img
        }) {
          out.faces.push(g as u32);
        }
      }
    }
    out.normalize();
    out
  }

  /// Copies the moves of some vertices (new positions with weights) onto their partners and keeps
  /// vertices on a mirror plane or the axis there. Returns the final moves.
  pub fn symmetric_moves(&self, cage: &Cage, moves: &[(u32, DVec3, f64)]) -> Vec<(u32, DVec3)> {
    let mut best: Vec<Option<(DVec3, f64)>> = vec![None; cage.vertices.len()];
    let mut put = |v: u32, p: DVec3, w: f64| {
      let slot = &mut best[v as usize];
      if slot.is_none_or(|(_, bw)| w > bw) {
        *slot = Some((p, w));
      }
    };
    for &(v, p, w) in moves {
      put(v, p, w);
    }
    let partners = self.partners(cage);
    for (m, p) in &partners {
      for &(v, np, w) in moves {
        if let Some(Some(u)) = p.get(v as usize)
          && *u != v
        {
          put(*u, m.transform_point3(np), w);
        }
      }
    }
    let mut out = Vec::new();
    for (v, slot) in best.iter().enumerate() {
      let Some((p, _)) = *slot else { continue };
      let mut p = p;
      // a vertex its own partner stays on the plane or axis
      let fixed: Vec<&DMat4> =
        partners.iter().filter(|(_, q)| q.get(v).copied().flatten() == Some(v as u32)).map(|(m, _)| m).collect();
      if !fixed.is_empty() {
        p = (p + fixed.iter().map(|m| m.transform_point3(p)).sum::<DVec3>()) / (fixed.len() + 1) as f64;
      }
      out.push((v as u32, p));
    }
    out
  }
}

/// The reflection in the plane through `o` with normal `n`.
pub fn reflection(o: DVec3, n: DVec3) -> DMat4 {
  let n = n.try_normalize().unwrap_or(DVec3::X);
  let r = glam::DMat3::from_cols(DVec3::X - 2.0 * n.x * n, DVec3::Y - 2.0 * n.y * n, DVec3::Z - 2.0 * n.z * n);
  DMat4::from_translation(o) * DMat4::from_mat3(r) * DMat4::from_translation(-o)
}
