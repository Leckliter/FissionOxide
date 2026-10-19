//! STEP with assembly structure, names and colours (see `cpp/xcaf.cpp`).

use std::path::Path;

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
  pub shape: Shape,
}

fn path_str(path: &Path) -> Result<&str> {
  path.to_str().ok_or(KernelError::Input("path is not UTF-8".into()))
}

/// Writes a STEP (AP214) assembly named `name`: one sub-assembly per component, one named part
/// per body with its colour.
pub fn write_step_assembly(name: &str, parts: &[StepPart<'_>], path: &Path) -> Result<()> {
  let p = path_str(path)?;
  let mut doc = sys::step_doc_new(name)?;
  for part in parts {
    let color: Vec<f64> = part.color.map(|c| c.iter().map(|v| *v as f64 / 255.0).collect()).unwrap_or_default();
    sys::step_doc_add(doc.pin_mut(), part.component, part.name, part.shape.get()?, &color)?;
  }
  Ok(sys::step_doc_write(doc.pin_mut(), p)?)
}

/// Reads a STEP file's bodies with their names, colours and components: every solid of each
/// part (a part of surfaces is sewn into shells).
pub fn read_step_assembly(path: &Path) -> Result<Vec<StepBody>> {
  let doc = sys::step_doc_read(path_str(path)?)?;
  let d = doc.as_ref().ok_or(KernelError::Empty)?;
  let mut out = Vec::new();
  for i in 0..sys::step_doc_len(d) {
    let Ok(shape) = Shape::wrap(sys::step_doc_shape(d, i)?) else { continue };
    let Ok(bodies) = crate::imported_bodies(shape) else { continue };
    let name = sys::step_doc_name(d, i);
    let component = sys::step_doc_component(d, i);
    let color = match sys::step_doc_color(d, i).as_slice() {
      [r, g, b] => Some([*r, *g, *b].map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)),
      _ => None,
    };
    let several = bodies.len() > 1;
    for (k, shape) in bodies.into_iter().enumerate() {
      let name = if several { format!("{name} ({})", k + 1) } else { name.clone() };
      out.push(StepBody { name, component: component.clone(), color, shape });
    }
  }
  if out.is_empty() {
    return Err(KernelError::Empty);
  }
  Ok(out)
}
