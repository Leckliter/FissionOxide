//! The MESH tab's icons that are pictures of their own, as Fusion's are: Insert Mesh's arrow into
//! a mesh box, Tessellate's solid turning to mesh, Remesh's and Reduce's split cubes with a dense
//! and an even or sparse mesh, and the icons of the mesh dialogs' drop-downs and toggles.

use egui::{Color32, Shape, Stroke};

use super::{Icon, Pen, Tone, Tone::*};
use crate::theme;

/// The front face of a box `x0..x1` × `y0..y1` meshed `n` × `n` (each cell split corner to corner).
fn grid(k: &Pen, x0: f32, y0: f32, x1: f32, y1: f32, n: usize, t: Tone) {
  let (dx, dy) = ((x1 - x0) / n as f32, (y1 - y0) / n as f32);
  for i in 1..n {
    let f = i as f32;
    k.line((x0 + dx * f, y0), (x0 + dx * f, y1), t);
    k.line((x0, y0 + dy * f), (x1, y0 + dy * f), t);
  }
  for i in 0..n {
    for j in 0..n {
      let (a, b) = (x0 + dx * i as f32, y0 + dy * j as f32);
      k.line((a, b + dy), (a + dx, b), t);
    }
  }
}

/// A block whose faces are meshed (`n` cells across the front).
fn mesh_block(k: &Pen, x0: f32, y0: f32, x1: f32, y1: f32, d: f32, n: usize) {
  k.block(x0, y0, x1, y1, d, BlueLight, Blue, BlueDark);
  grid(k, x0, y0, x1, y1, n, BlueDark);
  k.line((x0 + d * 0.5, y0 - d * 0.35), (x1 + d * 0.5, y0 - d * 0.35), BlueDark);
}

/// Fusion's blue for the arrow of an insert, kept whatever the family's accent.
fn blue(k: &Pen) -> Color32 {
  k.mono.unwrap_or(theme::ICON_BLUE)
}

/// Draws `icon` if it is one of these; false otherwise.
pub(super) fn paint(k: &Pen, icon: Icon) -> bool {
  match icon {
    Icon::InsertMesh => {
      mesh_block(k, 0.34, 0.42, 0.86, 0.94, 0.12, 2);
      let c = blue(k);
      let s = Stroke::new(k.w * 1.6, c);
      k.p.add(Shape::line_segment([k.u(0.02, 0.1), k.u(0.34, 0.38)], s));
      let (tip, d, n) = (k.u(0.46, 0.49), egui::vec2(0.75, 0.66).normalized(), egui::vec2(-0.66, 0.75));
      let size = 0.18 * k.r.width();
      k.p.add(Shape::convex_polygon(
        vec![tip, tip - d * size + n * size * 0.6, tip - d * size - n * size * 0.6],
        c,
        Stroke::NONE,
      ));
    }
    Icon::Tessellate => {
      // a grey cylinder, its right half turned to mesh
      k.fill(&[(0.14, 0.3), (0.5, 0.3), (0.5, 0.82), (0.14, 0.82)], Fill);
      k.fill(&[(0.5, 0.3), (0.86, 0.3), (0.86, 0.82), (0.5, 0.82)], BlueLight);
      k.ellipse((0.5, 0.82), 0.36, 0.1, None, Ink);
      for x in [0.62, 0.74] {
        k.line((x, 0.3), (x, 0.9), BlueDark);
      }
      k.line((0.5, 0.56), (0.86, 0.56), BlueDark);
      k.line((0.5, 0.56), (0.62, 0.3), BlueDark);
      k.line((0.62, 0.56), (0.74, 0.3), BlueDark);
      k.line((0.5, 0.88), (0.62, 0.56), BlueDark);
      k.line((0.62, 0.9), (0.74, 0.56), BlueDark);
      k.line((0.74, 0.88), (0.86, 0.56), BlueDark);
      k.line((0.14, 0.3), (0.14, 0.82), Ink);
      k.line((0.86, 0.3), (0.86, 0.82), Ink);
      k.ellipse((0.5, 0.3), 0.36, 0.1, Some(Light), Ink);
    }
    Icon::Remesh | Icon::Reduce => {
      // a cube split down the middle: dense mesh on the left, even (Remesh) or sparse (Reduce)
      // on the right
      k.block(0.06, 0.36, 0.46, 0.94, 0.14, BlueLight, Blue, BlueDark);
      grid(k, 0.06, 0.36, 0.46, 0.94, 3, BlueDark);
      k.block(0.54, 0.36, 0.86, 0.94, 0.12, BlueLight, Blue, BlueDark);
      if icon == Icon::Remesh {
        grid(k, 0.54, 0.36, 0.86, 0.94, 2, BlueDark);
      } else {
        k.line((0.54, 0.94), (0.86, 0.36), BlueDark);
      }
    }
    Icon::RemeshAdaptive => {
      k.solid(&[(0.08, 0.14), (0.92, 0.14), (0.92, 0.86), (0.08, 0.86)], BlueLight);
      // small triangles at the left, large at the right
      for (x, n) in [(0.2, 4), (0.36, 3), (0.6, 2)] {
        k.line((x, 0.14), (x, 0.86), BlueDark);
        for j in 0..n {
          let dy = 0.72 / n as f32;
          k.line((x - 0.14, 0.14 + dy * (j + 1) as f32), (x, 0.14 + dy * j as f32), BlueDark);
        }
      }
      k.line((0.6, 0.86), (0.92, 0.14), BlueDark);
    }
    Icon::RemeshUniform => {
      k.solid(&[(0.08, 0.14), (0.92, 0.14), (0.92, 0.86), (0.08, 0.86)], BlueLight);
      grid(k, 0.08, 0.14, 0.92, 0.86, 3, BlueDark);
    }
    Icon::FillNone => {
      // an open rim: the cut's outline with nothing across it
      k.closed(&[(0.12, 0.3), (0.62, 0.3), (0.88, 0.7), (0.38, 0.7)], Ink);
      k.dashed((0.2, 0.5), (0.8, 0.5), Ink);
    }
    Icon::FillUniform | Icon::FillMinimal => {
      k.solid(&[(0.12, 0.3), (0.62, 0.3), (0.88, 0.7), (0.38, 0.7)], BlueLight);
      if icon == Icon::FillUniform {
        for (a, b) in [((0.37, 0.3), (0.63, 0.7)), ((0.25, 0.5), (0.75, 0.5)), ((0.37, 0.3), (0.25, 0.5))] {
          k.line(a, b, BlueDark);
        }
        k.line((0.62, 0.3), (0.75, 0.5), BlueDark);
        k.line((0.25, 0.5), (0.38, 0.7), BlueDark);
        k.line((0.5, 0.5), (0.63, 0.7), BlueDark);
        k.line((0.5, 0.5), (0.37, 0.3), BlueDark);
      } else {
        k.line((0.12, 0.3), (0.88, 0.7), BlueDark);
      }
    }
    Icon::ConvertFaceted => {
      k.block(0.12, 0.36, 0.68, 0.92, 0.22, Light, Fill, Dark);
      k.line((0.12, 0.92), (0.68, 0.36), GREY);
      k.line((0.68, 0.92), (0.9, 0.2), GREY);
      k.line((0.12, 0.36), (0.9, 0.2), GREY);
    }
    Icon::ConvertPrismatic => k.block(0.12, 0.36, 0.68, 0.92, 0.22, BlueLight, Blue, BlueDark),
    Icon::ConvertOrganic => {
      k.ellipse((0.5, 0.56), 0.38, 0.32, Some(BlueLight), Blue);
      k.ellipse((0.4, 0.44), 0.12, 0.07, Some(Light), BlueLight);
    }
    _ => return false,
  }
  true
}

/// The facet lines of a grey solid.
const GREY: Tone = Dark;
