//! Reading results: scalar fields by name, probes at points, the contour colour map, the surface
//! drawn in a viewport and a study report.

use serde::{Deserialize, Serialize};

use crate::fem::{self, Geo};
use crate::geom::*;
use crate::study::{ModalSolution, Model, StaticSolution, ThermalSolution};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "study", rename_all = "snake_case")]
pub enum Solution {
  Static(StaticSolution),
  Modal(ModalSolution),
  Thermal(ThermalSolution),
}

/// A result field that can be shown as contours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldInfo {
  pub id: &'static str,
  pub label: &'static str,
  pub unit: &'static str,
}

const STATIC_FIELDS: &[FieldInfo] = &[
  FieldInfo { id: "von_mises", label: "Von Mises stress", unit: "MPa" },
  FieldInfo { id: "displacement", label: "Displacement", unit: "mm" },
  FieldInfo { id: "safety_factor", label: "Safety factor", unit: "" },
  FieldInfo { id: "ux", label: "Displacement X", unit: "mm" },
  FieldInfo { id: "uy", label: "Displacement Y", unit: "mm" },
  FieldInfo { id: "uz", label: "Displacement Z", unit: "mm" },
  FieldInfo { id: "p1", label: "1st principal stress", unit: "MPa" },
  FieldInfo { id: "p3", label: "3rd principal stress", unit: "MPa" },
  FieldInfo { id: "sxx", label: "Normal stress XX", unit: "MPa" },
  FieldInfo { id: "syy", label: "Normal stress YY", unit: "MPa" },
  FieldInfo { id: "szz", label: "Normal stress ZZ", unit: "MPa" },
  FieldInfo { id: "sxy", label: "Shear stress XY", unit: "MPa" },
  FieldInfo { id: "syz", label: "Shear stress YZ", unit: "MPa" },
  FieldInfo { id: "szx", label: "Shear stress ZX", unit: "MPa" },
];
const MODAL_FIELDS: &[FieldInfo] = &[FieldInfo { id: "mode", label: "Mode shape (relative)", unit: "" }];
const THERMAL_FIELDS: &[FieldInfo] = &[
  FieldInfo { id: "temperature", label: "Temperature", unit: "°C" },
  FieldInfo { id: "heat_flux", label: "Heat flux", unit: "W/m²" },
  FieldInfo { id: "flux_x", label: "Heat flux X", unit: "W/m²" },
  FieldInfo { id: "flux_y", label: "Heat flux Y", unit: "W/m²" },
  FieldInfo { id: "flux_z", label: "Heat flux Z", unit: "W/m²" },
];

impl Solution {
  pub fn kind(&self) -> &'static str {
    match self {
      Solution::Static(_) => "static",
      Solution::Modal(_) => "modal",
      Solution::Thermal(_) => "thermal",
    }
  }

  /// The fields it has, the default one first.
  pub fn fields(&self) -> &'static [FieldInfo] {
    match self {
      Solution::Static(_) => STATIC_FIELDS,
      Solution::Modal(_) => MODAL_FIELDS,
      Solution::Thermal(_) => THERMAL_FIELDS,
    }
  }

  pub fn field_info(&self, id: &str) -> Option<FieldInfo> {
    self.fields().iter().find(|f| f.id == id).copied()
  }

  /// A scalar field per node (`mode` picks the mode shape, from 1).
  pub fn field(&self, id: &str, mode: usize) -> Option<Vec<f64>> {
    match self {
      Solution::Static(s) => Some(match id {
        "von_mises" => s.von_mises.clone(),
        "displacement" => s.displacement.iter().map(|d| norm(*d)).collect(),
        "safety_factor" => s.safety_factor.clone(),
        "ux" | "uy" | "uz" => {
          let c = (id.as_bytes()[1] - b'x') as usize;
          s.displacement.iter().map(|d| d[c]).collect()
        }
        "p1" => s.stress.iter().map(|t| fem::principal(*t)[0]).collect(),
        "p3" => s.stress.iter().map(|t| fem::principal(*t)[2]).collect(),
        "sxx" | "syy" | "szz" | "sxy" | "syz" | "szx" => {
          let c = ["sxx", "syy", "szz", "sxy", "syz", "szx"].iter().position(|x| *x == id)?;
          s.stress.iter().map(|t| t[c]).collect()
        }
        _ => return None,
      }),
      Solution::Modal(m) => {
        let md = m.modes.get(mode.max(1) - 1)?;
        (id == "mode").then(|| md.shape.iter().map(|d| norm(*d)).collect())
      }
      Solution::Thermal(t) => Some(match id {
        "temperature" => t.temperature.clone(),
        // W/mm² → W/m²
        "heat_flux" => t.flux.iter().map(|q| norm(*q) * 1e6).collect(),
        "flux_x" | "flux_y" | "flux_z" => {
          let c = (id.as_bytes()[5] - b'x') as usize;
          t.flux.iter().map(|q| q[c] * 1e6).collect()
        }
        _ => return None,
      }),
    }
  }

  /// The displacement to deform the drawing by (the mode shape for modal).
  pub fn deformation(&self, mode: usize) -> Option<&[V3]> {
    match self {
      Solution::Static(s) => Some(&s.displacement),
      Solution::Modal(m) => m.modes.get(mode.max(1) - 1).map(|m| m.shape.as_slice()),
      Solution::Thermal(_) => None,
    }
  }
}

/// Smallest and largest values of a field and where they are.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Range {
  pub min: f64,
  pub max: f64,
  pub min_at: V3,
  pub max_at: V3,
}

pub fn range(values: &[f64], nodes: &[V3]) -> Range {
  let mut r = Range { min: f64::INFINITY, max: f64::NEG_INFINITY, min_at: [0.0; 3], max_at: [0.0; 3] };
  for (v, p) in values.iter().zip(nodes) {
    if *v < r.min {
      r.min = *v;
      r.min_at = *p;
    }
    if *v > r.max {
      r.max = *v;
      r.max_at = *p;
    }
  }
  if values.is_empty() {
    r.min = 0.0;
    r.max = 0.0;
  }
  r
}

/// Where a point is in the model: (part, element, barycentric coordinates), or the nearest
/// element when it's outside every one (within `tol`).
pub fn locate(model: &Model, p: V3, tol: f64) -> Option<(usize, usize, [f64; 4])> {
  let mut best: Option<(f64, usize, usize, [f64; 4])> = None;
  for (pi, part) in model.parts.iter().enumerate() {
    let m = &part.mesh;
    for (ti, t) in m.tets.iter().enumerate() {
      let x = [0, 1, 2, 3].map(|c| m.nodes[t[c] as usize]);
      let lo = [0, 1, 2].map(|k| x.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min));
      let hi = [0, 1, 2].map(|k| x.iter().map(|v| v[k]).fold(f64::NEG_INFINITY, f64::max));
      if (0..3).any(|k| p[k] < lo[k] - tol || p[k] > hi[k] + tol) {
        continue;
      }
      let geo = Geo::new(x);
      let l = geo.bary(x[0], p);
      let outside = -l.iter().fold(0.0f64, |a, b| a.min(*b));
      if outside <= 1e-9 {
        return Some((pi, ti, l));
      }
      if best.is_none_or(|b| outside < b.0) {
        best = Some((outside, pi, ti, l));
      }
    }
  }
  let (_, pi, ti, l) = best?;
  // clamp onto the element
  let l = l.map(|v| v.max(0.0));
  let s: f64 = l.iter().sum();
  Some((pi, ti, l.map(|v| v / s)))
}

/// A nodal field's value at a point (interpolated with the element's shape functions).
pub fn probe(model: &Model, values: &[f64], p: V3) -> Option<f64> {
  let (lo, hi) = bbox(&model.nodes());
  let tol = 1e-3 * dist(lo, hi) + 1e-9;
  let (pi, ti, l) = locate(model, p, tol)?;
  let off = model.offsets();
  let t = model.parts[pi].mesh.tets[ti];
  let n = fem::shape_values(l);
  Some((0..10).map(|k| n[k] * values[t[k] as usize + off[pi]]).sum())
}

/// The contour colour of `t` in 0–1: blue, cyan, green, yellow, red (Fusion's rainbow).
pub fn colormap(t: f64) -> [u8; 4] {
  let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
  const STOPS: [[f64; 3]; 5] = [[0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
  let x = t * 4.0;
  let i = (x.floor() as usize).min(3);
  let f = x - i as f64;
  let c = [0, 1, 2].map(|k| STOPS[i][k] + (STOPS[i + 1][k] - STOPS[i][k]) * f);
  [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8, 255]
}

/// The model's outer surface for drawing: each 6-node boundary triangle split into 4. Returns
/// (global node of each vertex, triangles, the CAD face (part, face) of each triangle).
pub fn display_surface(model: &Model) -> (Vec<u32>, Vec<[u32; 3]>, Vec<(usize, u32)>) {
  let off = model.offsets();
  let mut verts: Vec<u32> = Vec::new();
  let mut index = Map::default();
  let mut tris = Vec::new();
  let mut faces = Vec::new();
  for (pi, p) in model.parts.iter().enumerate() {
    for (b, f) in p.mesh.boundary.iter().zip(&p.mesh.boundary_face) {
      let g = b.map(|n| {
        let gn = n + off[pi] as u32;
        *index.entry(gn).or_insert_with(|| {
          verts.push(gn);
          verts.len() as u32 - 1
        })
      });
      // corners 0 1 2, mids 3 (01) 4 (12) 5 (20)
      for t in [[g[0], g[3], g[5]], [g[3], g[1], g[4]], [g[5], g[4], g[2]], [g[3], g[4], g[5]]] {
        tris.push(t);
        faces.push((pi, *f));
      }
    }
  }
  (verts, tris, faces)
}

/// What a report says about the study's setup.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportInput {
  pub title: String,
  pub study: String,
  /// (body, material, nodes, elements).
  pub bodies: Vec<(String, String, usize, usize)>,
  pub mesh: String,
  pub constraints: Vec<String>,
  pub loads: Vec<String>,
  pub contacts: Vec<String>,
}

fn esc(s: &str) -> String {
  s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn g(v: f64) -> String {
  if v == 0.0 || !v.is_finite() {
    return format!("{v}");
  }
  let a = v.abs();
  if !(1e-3..1e6).contains(&a) {
    format!("{v:.4e}")
  } else {
    format!("{:.5}", v).trim_end_matches('0').trim_end_matches('.').to_string()
  }
}

fn p3(p: V3) -> String {
  format!("({}, {}, {})", g(p[0]), g(p[1]), g(p[2]))
}

/// The results as rows: (quantity, value).
pub fn summary(model: &Model, sol: &Solution) -> Vec<(String, String)> {
  let nodes = model.nodes();
  let mut rows = Vec::new();
  match sol {
    Solution::Static(s) => {
      for id in ["von_mises", "displacement", "safety_factor"] {
        let info = sol.field_info(id).unwrap_or(STATIC_FIELDS[0]);
        let r = range(&sol.field(id, 1).unwrap_or_default(), &nodes);
        if id == "safety_factor" {
          rows.push((format!("Minimum {}", info.label.to_lowercase()), format!("{} at {}", g(r.min), p3(r.min_at))));
        } else {
          rows.push((
            format!("Maximum {}", info.label.to_lowercase()),
            format!("{} {} at {}", g(r.max), info.unit, p3(r.max_at)),
          ));
        }
      }
      for (i, r) in s.reactions.iter().enumerate() {
        rows.push((format!("Reaction force, constraint {}", i + 1), format!("{} N (|R| = {} N)", p3(*r), g(norm(*r)))));
      }
      rows.push(("Applied load".into(), format!("{} N", p3(s.applied))));
    }
    Solution::Modal(m) => {
      rows.push(("Mass".into(), format!("{} kg", g(m.mass))));
      for (i, md) in m.modes.iter().enumerate() {
        rows.push((
          format!("Mode {}", i + 1),
          format!(
            "{} Hz (effective mass X {:.1} %, Y {:.1} %, Z {:.1} %)",
            g(md.frequency),
            md.participation[0] * 100.0,
            md.participation[1] * 100.0,
            md.participation[2] * 100.0
          ),
        ));
      }
    }
    Solution::Thermal(t) => {
      let r = range(&t.temperature, &nodes);
      rows.push(("Maximum temperature".into(), format!("{} °C at {}", g(r.max), p3(r.max_at))));
      rows.push(("Minimum temperature".into(), format!("{} °C at {}", g(r.min), p3(r.min_at))));
      let q = range(&sol.field("heat_flux", 1).unwrap_or_default(), &nodes);
      rows.push(("Maximum heat flux".into(), format!("{} W/m² at {}", g(q.max), p3(q.max_at))));
      for (i, h) in t.heat_flow.iter().enumerate() {
        if *h != 0.0 {
          rows.push((format!("Heat flow out, thermal load {}", i + 1), format!("{} W", g(*h))));
        }
      }
    }
  }
  rows
}

/// A report: HTML, or Markdown when `markdown`.
pub fn report(input: &ReportInput, model: &Model, sol: &Solution, markdown: bool) -> String {
  let rows = summary(model, sol);
  let bodies: Vec<[String; 4]> =
    input.bodies.iter().map(|(b, m, n, e)| [b.clone(), m.clone(), n.to_string(), e.to_string()]).collect();
  if markdown {
    let mut s = format!(
      "# {}\n\n**Study:** {}\n\n## Bodies\n\n| Body | Material | Nodes | Elements |\n|---|---|---|---|\n",
      input.title, input.study
    );
    for b in &bodies {
      s += &format!("| {} | {} | {} | {} |\n", b[0], b[1], b[2], b[3]);
    }
    s += &format!("\n**Mesh:** {}\n", input.mesh);
    for (head, list) in [("Constraints", &input.constraints), ("Loads", &input.loads), ("Contacts", &input.contacts)] {
      if !list.is_empty() {
        s += &format!("\n## {head}\n\n");
        for l in list {
          s += &format!("- {l}\n");
        }
      }
    }
    s += "\n## Results\n\n| Quantity | Value |\n|---|---|\n";
    for (k, v) in rows {
      s += &format!("| {k} | {v} |\n");
    }
    return s;
  }
  let mut s = format!(
    "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{t}</title><style>body{{font-family:sans-serif;margin:2em;color:#222}}\
     table{{border-collapse:collapse;margin:1em 0}}td,th{{border:1px solid #bbb;padding:4px 10px;text-align:left}}\
     th{{background:#eee}}h1{{color:#1f5f9f}}</style></head><body>\n<h1>{t}</h1>\n<p><b>Study:</b> {st}</p>\n<h2>Bodies</h2>\n\
     <table><tr><th>Body</th><th>Material</th><th>Nodes</th><th>Elements</th></tr>\n",
    t = esc(&input.title),
    st = esc(&input.study)
  );
  for b in &bodies {
    s += &format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n", esc(&b[0]), esc(&b[1]), b[2], b[3]);
  }
  s += &format!("</table>\n<p><b>Mesh:</b> {}</p>\n", esc(&input.mesh));
  for (head, list) in [("Constraints", &input.constraints), ("Loads", &input.loads), ("Contacts", &input.contacts)] {
    if !list.is_empty() {
      s += &format!("<h2>{head}</h2>\n<ul>\n");
      for l in list {
        s += &format!("<li>{}</li>\n", esc(l));
      }
      s += "</ul>\n";
    }
  }
  s += "<h2>Results</h2>\n<table><tr><th>Quantity</th><th>Value</th></tr>\n";
  for (k, v) in rows {
    s += &format!("<tr><td>{}</td><td>{}</td></tr>\n", esc(&k), esc(&v));
  }
  s += "</table>\n</body></html>\n";
  s
}
