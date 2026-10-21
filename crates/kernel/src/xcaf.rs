//! STEP with assembly structure, names and colours (see `cpp/xcaf.cpp`).
//!
//! Reading gives the file's assembly tree as occurrences ([`StepNode`]): every placement of a
//! sub-assembly or part, with the prototype it places, so a caller can tell an instanced part
//! from distinct ones. Writing ([`StepWriter`]) builds the tree from assemblies and parts, and
//! places a prototype as often as needed (STEP instances).

use std::path::Path;

use glam::{DMat4, DVec4};

use crate::{KernelError, Result, Shape, sys};

/// A body to write: its name, the component (sub-assembly) it belongs to (empty = the top
/// assembly) and its colour.
#[derive(Debug, Clone, Copy)]
pub struct StepPart<'a> {
  pub name: &'a str,
  pub component: &'a str,
  pub color: Option<[u8; 3]>,
  pub shape: &'a Shape,
}

/// A body read from a STEP file, placed where the assembly puts it.
#[derive(Debug, Clone)]
pub struct StepBody {
  pub name: String,
  /// The top-level sub-assembly it came from (empty when it sits in the top assembly).
  pub component: String,
  pub color: Option<[u8; 3]>,
  /// Faces coloured differently from the body: 1-based face id (as in [`Shape::faces`]) and colour.
  pub face_colors: Vec<(u32, [u8; 3])>,
  pub shape: Shape,
}

/// One occurrence in a STEP file's assembly tree: a sub-assembly or a part, placed.
#[derive(Debug, Clone)]
pub struct StepNode {
  /// The sub-assembly occurrence it sits in (an index into the list); `None` = the top level.
  pub parent: Option<usize>,
  pub assembly: bool,
  /// The prototype's name.
  pub name: String,
  /// The occurrence's own name (often empty, or the prototype's name with a number).
  pub instance: String,
  /// The prototype it places: every occurrence of one part or sub-assembly has the same number.
  pub proto: usize,
  /// How many times the file places the prototype (in all the assemblies using it).
  pub uses: usize,
  /// Where the occurrence sits in the file's world.
  pub placement: DMat4,
  /// The part's bodies (none for a sub-assembly), already placed.
  pub bodies: Vec<StepBody>,
}

fn path_str(path: &Path) -> Result<&str> {
  path.to_str().ok_or(KernelError::Input("path is not UTF-8".into()))
}

fn rgb_u8(c: &[f64]) -> [u8; 3] {
  [c[0], c[1], c[2]].map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn rgb_f64(c: [u8; 3]) -> Vec<f64> {
  c.iter().map(|v| *v as f64 / 255.0).collect()
}

/// A rigid placement as the 3×4 rows the bridge passes.
fn rows(m: DMat4) -> Vec<f64> {
  (0..3).flat_map(|r| (0..4).map(move |c| m.col(c)[r])).collect()
}

fn from_rows(v: &[f64]) -> DMat4 {
  if v.len() != 12 {
    return DMat4::IDENTITY;
  }
  let col = |c: usize| DVec4::new(v[c], v[4 + c], v[8 + c], if c == 3 { 1.0 } else { 0.0 });
  DMat4::from_cols(col(0), col(1), col(2), col(3))
}

/// Reads a STEP file's assembly tree: sub-assemblies and parts in the order they appear, parents
/// first; each part's solids as bodies (a part of surfaces is sewn into shells), with names,
/// colours and the colours of single faces.
pub fn read_step_tree(path: &Path) -> Result<Vec<StepNode>> {
  let doc = sys::step_doc_read(path_str(path)?)?;
  let d = doc.as_ref().ok_or(KernelError::Empty)?;
  let mut out: Vec<StepNode> = Vec::new();
  // file occurrence -> index in `out` (parts without solids are left out)
  let mut index: Vec<Option<usize>> = Vec::new();
  for i in 0..sys::step_doc_len(d) {
    let parent = usize::try_from(sys::step_doc_parent(d, i)).ok().and_then(|p| index.get(p).copied().flatten());
    let assembly = sys::step_doc_is_assembly(d, i);
    let name = sys::step_doc_name(d, i);
    let mut node = StepNode {
      parent,
      assembly,
      name: name.clone(),
      instance: sys::step_doc_instance(d, i),
      proto: sys::step_doc_proto(d, i),
      uses: sys::step_doc_uses(d, i),
      placement: from_rows(&sys::step_doc_placement(d, i)),
      bodies: Vec::new(),
    };
    if !assembly {
      let color = match sys::step_doc_color(d, i).as_slice() {
        c @ [_, _, _] => Some(rgb_u8(c)),
        _ => None,
      };
      let bodies = sys::step_doc_shape(d, i)
        .ok()
        .and_then(|s| Shape::wrap(s).ok())
        .and_then(|s| crate::imported_bodies(s).ok())
        .unwrap_or_default();
      let several = bodies.len() > 1;
      for (k, shape) in bodies.into_iter().enumerate() {
        let faces: Vec<(u32, [u8; 3])> = sys::step_doc_face_colors(d, i, shape.get()?)?
          .chunks_exact(4)
          .map(|c| (c[0] as u32, rgb_u8(&c[1..])))
          .collect();
        // a part without a colour of its own takes its faces' most common one
        let color = color.or_else(|| {
          let mut count: Vec<([u8; 3], usize)> = Vec::new();
          for (_, c) in &faces {
            match count.iter_mut().find(|e| e.0 == *c) {
              Some(e) => e.1 += 1,
              None => count.push((*c, 1)),
            }
          }
          count.iter().max_by_key(|e| e.1).map(|e| e.0)
        });
        let face_colors = faces.into_iter().filter(|(_, c)| Some(*c) != color).collect();
        let name = if several { format!("{name} ({})", k + 1) } else { name.clone() };
        node.bodies.push(StepBody { name, component: String::new(), color, face_colors, shape });
      }
      if node.bodies.is_empty() {
        index.push(None);
        continue;
      }
    }
    index.push(Some(out.len()));
    out.push(node);
  }
  // each body's top-level sub-assembly
  for i in 0..out.len() {
    let mut top = None;
    let mut at = out[i].parent;
    while let Some(p) = at {
      top = Some(p);
      at = out[p].parent;
    }
    let component = top.map(|t| component_name(&out[t], t)).unwrap_or_default();
    for b in &mut out[i].bodies {
      b.component = component.clone();
    }
  }
  if out.iter().all(|n| n.bodies.is_empty()) {
    return Err(KernelError::Empty);
  }
  Ok(out)
}

/// The name a sub-assembly occurrence goes by: its own, else its prototype's, else a number.
fn component_name(n: &StepNode, i: usize) -> String {
  [&n.instance, &n.name]
    .into_iter()
    .find(|s| !s.trim().is_empty())
    .map(|s| s.trim().to_string())
    .unwrap_or_else(|| format!("Component {}", i + 1))
}

/// Reads a STEP file's bodies with their names, colours and components (each body's top-level
/// sub-assembly): every solid of each part, flattened out of the assembly tree.
pub fn read_step_assembly(path: &Path) -> Result<Vec<StepBody>> {
  Ok(read_step_tree(path)?.into_iter().flat_map(|n| n.bodies).collect())
}

/// A STEP (AP214) document being built: assemblies and parts are prototypes, numbered as they
/// are made ([`StepWriter::ROOT`] is the top assembly), and [`StepWriter::place`] puts one into
/// an assembly, as often as needed.
pub struct StepWriter {
  doc: cxx::UniquePtr<sys::StepDoc>,
}

impl StepWriter {
  /// The top assembly.
  pub const ROOT: usize = 0;

  /// A document whose top assembly is named `name`.
  pub fn new(name: &str) -> Result<StepWriter> {
    Ok(StepWriter { doc: sys::step_doc_new(name)? })
  }

  /// A new, empty sub-assembly.
  pub fn assembly(&mut self, name: &str) -> Result<usize> {
    Ok(sys::step_doc_assembly(self.doc.pin_mut(), name)?)
  }

  /// A new part: one body with its colour, and the colours of single faces (1-based face ids as
  /// in [`Shape::faces`]). Its shape stays where it is when placed at the identity.
  pub fn part(
    &mut self,
    name: &str,
    shape: &Shape,
    color: Option<[u8; 3]>,
    face_colors: &[(u32, [u8; 3])],
  ) -> Result<usize> {
    let color = color.map(rgb_f64).unwrap_or_default();
    let faces: Vec<f64> = face_colors.iter().flat_map(|(f, c)| std::iter::once(*f as f64).chain(rgb_f64(*c))).collect();
    Ok(sys::step_doc_part(self.doc.pin_mut(), name, shape.get()?, &color, &faces)?)
  }

  /// Places assembly or part `child` in assembly `parent`, moved rigidly by `at`, as an instance
  /// named `name`.
  pub fn place(&mut self, parent: usize, child: usize, name: &str, at: DMat4) -> Result<()> {
    if !is_rigid(at) {
      return Err(KernelError::Input("a STEP placement must be a rotation and a translation".into()));
    }
    let at = if at.abs_diff_eq(DMat4::IDENTITY, 1e-12) { Vec::new() } else { rows(at) };
    Ok(sys::step_doc_place(self.doc.pin_mut(), parent, child, name, &at)?)
  }

  pub fn write(mut self, path: &Path) -> Result<()> {
    let p = path_str(path)?;
    Ok(sys::step_doc_write(self.doc.pin_mut(), p)?)
  }
}

/// Is `m` a rotation followed by a translation (no scale, shear or mirror)?
pub fn is_rigid(m: DMat4) -> bool {
  let r = glam::DMat3::from_mat4(m);
  let ortho = (r.transpose() * r).abs_diff_eq(glam::DMat3::IDENTITY, 1e-6);
  ortho && (r.determinant() - 1.0).abs() < 1e-6 && m.row(3).abs_diff_eq(DVec4::W, 1e-12) && m.w_axis.is_finite()
}

/// Writes a STEP (AP214) assembly named `name`: one sub-assembly per component, one named part
/// per body with its colour.
pub fn write_step_assembly(name: &str, parts: &[StepPart<'_>], path: &Path) -> Result<()> {
  let mut w = StepWriter::new(name)?;
  let mut components: Vec<(&str, usize)> = Vec::new();
  for part in parts {
    let parent = if part.component.is_empty() {
      StepWriter::ROOT
    } else if let Some(c) = components.iter().find(|c| c.0 == part.component) {
      c.1
    } else {
      let a = w.assembly(part.component)?;
      components.push((part.component, a));
      a
    };
    let p = w.part(part.name, part.shape, part.color, &[])?;
    w.place(parent, p, part.name, DMat4::IDENTITY)?;
  }
  for (name, a) in components {
    w.place(StepWriter::ROOT, a, name, DMat4::IDENTITY)?;
  }
  w.write(path)
}
