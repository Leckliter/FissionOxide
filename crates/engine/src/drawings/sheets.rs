//! Drawing commands for sheets, revisions, baseline and chain dimensions, welding symbols and
//! datum targets.

use fox_model::drawing::{BackWeld, DrawingDim, Revision, SheetSize, WeldContour, WeldKind, WeldSide, WeldSymbol};
use serde_json::{Value, json};

use super::{annotate, axis_of, drawing_id, offset_of, opt_num, parse_scale, points_of, sheet_of, view_of};
use crate::commands::{opt_str, req_str, req_u32, req_vec3};
use crate::{CmdError, CmdResult, Session};

pub(super) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "drawing.add_sheet" => (|| {
      let id = drawing_id(s, p)?;
      let name = opt_str(p, "name").unwrap_or("").to_string();
      let size = size_of(p)?;
      let scale = scale_of(p)?;
      let sheet = s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        let sheet = dr.add_sheet(&name);
        let sh = dr.sheet_mut(sheet).ok_or(CmdError::new("sheet vanished"))?;
        sh.size = size;
        sh.scale = scale.flatten();
        Ok(sheet)
      })?;
      s.drawing_sheet = Some((id, sheet));
      Ok(json!({"sheet": sheet}))
    })(),
    "drawing.set_sheet" => (|| {
      let id = drawing_id(s, p)?;
      let sheet = sheet_of(s, id, p)?;
      let name = opt_str(p, "name").map(str::to_string);
      let size = size_of(p)?;
      let scale = scale_of(p)?;
      s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        let sh = dr.sheet_mut(sheet).ok_or(CmdError::new("no such sheet"))?;
        if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
          sh.name = n;
        }
        if size.is_some() {
          sh.size = size;
        }
        if let Some(scale) = scale {
          sh.scale = scale;
        }
        Ok(())
      })?;
      Ok(json!({"sheet": sheet}))
    })(),
    "drawing.remove_sheet" => (|| {
      let id = drawing_id(s, p)?;
      let sheet = req_u32(p, "sheet")?;
      s.edit(|d| {
        d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?.remove_sheet(sheet).map_err(CmdError::new)
      })?;
      Ok(json!({}))
    })(),
    "drawing.baseline" => (|| {
      let id = drawing_id(s, p)?;
      let points = points_of(p, "points")?;
      if points.is_empty() {
        return Err(CmdError::new("a baseline dimension needs 'points'"));
      }
      let spacing = opt_num(s, p, "spacing")?.unwrap_or(8.0);
      if spacing <= 0.0 {
        return Err(CmdError::new("the spacing must be positive"));
      }
      let dim = DrawingDim::Baseline {
        view: view_of(s, id, p)?,
        origin: req_vec3(p, "origin")?,
        points,
        orientation: axis_of(p)?,
        offset: opt_num(s, p, "offset")?.unwrap_or(10.0),
        spacing,
      };
      annotate(s, id, dim)
    })(),
    "drawing.chain" => (|| {
      let id = drawing_id(s, p)?;
      let points = points_of(p, "points")?;
      if points.len() < 2 {
        return Err(CmdError::new("a chain dimension needs at least two 'points'"));
      }
      let dim = DrawingDim::Chain {
        view: view_of(s, id, p)?,
        points,
        orientation: axis_of(p)?,
        offset: opt_num(s, p, "offset")?.unwrap_or(10.0),
      };
      annotate(s, id, dim)
    })(),
    "drawing.weld" => (|| {
      let id = drawing_id(s, p)?;
      let kind = req_str(p, "kind")?;
      let kind = WeldKind::parse(kind)
        .ok_or(CmdError::new(format!("unknown weld '{kind}' ({})", WeldKind::ALL.map(|k| k.key()).join(", "))))?;
      let side = match opt_str(p, "side") {
        None => WeldSide::Arrow,
        Some(t) => WeldSide::parse(t).ok_or(CmdError::new(format!("unknown side '{t}' (arrow, other, both)")))?,
      };
      let text = |k: &str| match p.get(k) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(t)) => t.trim().to_string(),
        Some(v) => v.to_string(),
      };
      let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
      let contour = |k: &str| match opt_str(p, k).map(str::trim) {
        None | Some("") | Some("none") => Ok(None),
        Some(t) => WeldContour::parse(t)
          .map(Some)
          .ok_or(CmdError::new(format!("unknown contour '{t}' (flush, convex, concave)"))),
      };
      let back = match opt_str(p, "back").map(str::trim) {
        None | Some("") | Some("none") => None,
        Some(t) => {
          Some(BackWeld::parse(t).ok_or(CmdError::new(format!("unknown back weld '{t}' (back, melt_through)")))?)
        }
      };
      let symbol = WeldSymbol {
        size: text("size"),
        length: text("length"),
        all_around: flag("all_around"),
        field: flag("field"),
        tail: text("tail"),
        contour: contour("contour")?,
        finish: text("finish").to_ascii_uppercase(),
        other_contour: contour("other_contour")?,
        other_finish: text("other_finish").to_ascii_uppercase(),
        back,
        ..WeldSymbol::new(kind, side)
      };
      symbol.check().map_err(CmdError::new)?;
      let dim = DrawingDim::Weld {
        view: view_of(s, id, p)?,
        point: req_vec3(p, "point")?,
        offset: offset_of(p)?.unwrap_or([12.0, 12.0]),
        symbol,
      };
      annotate(s, id, dim)
    })(),
    "drawing.datum_target" => (|| {
      let id = drawing_id(s, p)?;
      let label = req_str(p, "label")?.trim().to_ascii_uppercase();
      // a datum letter and the target's number: A1, B2
      let letters = label.trim_end_matches(|c: char| c.is_ascii_digit());
      let valid = !letters.is_empty()
        && letters.len() < label.len()
        && letters.chars().all(|c| c.is_ascii_alphabetic())
        && !letters.contains(['I', 'O', 'Q']);
      if !valid {
        return Err(CmdError::new(format!("'{label}' isn't a datum target (a letter and a number, like A1)")));
      }
      let area = opt_num(s, p, "area")?.unwrap_or(0.0);
      if area < 0.0 {
        return Err(CmdError::new("the target area can't be negative"));
      }
      let point = req_vec3(p, "point")?;
      let end = match p.get("end") {
        None | Some(Value::Null) => None,
        Some(_) => Some(req_vec3(p, "end")?),
      };
      if let Some(e) = end {
        if area > 0.0 {
          return Err(CmdError::new("a datum target is a point, a line ('end') or an area, not both"));
        }
        if (0..3).map(|i| (e[i] - point[i]).powi(2)).sum::<f64>().sqrt() < 1e-9 {
          return Err(CmdError::new("a target line needs 'end' apart from 'point'"));
        }
      }
      let dim = DrawingDim::DatumTarget {
        view: view_of(s, id, p)?,
        point,
        end,
        offset: offset_of(p)?.unwrap_or([12.0, 12.0]),
        label,
        area,
      };
      annotate(s, id, dim)
    })(),
    "drawing.revision" => (|| {
      let id = drawing_id(s, p)?;
      let d = s.design.drawing(id).ok_or(CmdError::new("no such drawing"))?;
      let rev = match opt_str(p, "rev").map(str::trim) {
        Some(r) if !r.is_empty() => r.to_string(),
        _ => next_rev(d.revisions.last().map(|r| r.rev.as_str())),
      };
      let revision = Revision {
        rev: rev.clone(),
        description: opt_str(p, "description").unwrap_or("").to_string(),
        date: opt_str(p, "date").unwrap_or("").to_string(),
        by: opt_str(p, "by").unwrap_or("").to_string(),
      };
      let index = s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        dr.revisions.push(revision);
        Ok(dr.revisions.len() - 1)
      })?;
      Ok(json!({"index": index, "rev": rev}))
    })(),
    "drawing.remove_revision" => (|| {
      let id = drawing_id(s, p)?;
      let index = req_u32(p, "index")? as usize;
      s.edit(|d| {
        let dr = d.drawing_mut(id).ok_or(CmdError::new("no such drawing"))?;
        if index >= dr.revisions.len() {
          return Err(CmdError::new(format!("no revision {index} (the drawing has {})", dr.revisions.len())));
        }
        dr.revisions.remove(index);
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    _ => return None,
  })
}

/// `size`: a sheet size, or none.
fn size_of(p: &Value) -> Result<Option<SheetSize>, CmdError> {
  match opt_str(p, "size").or(opt_str(p, "sheet_size")) {
    None => Ok(None),
    Some(t) => {
      SheetSize::parse(t).map(Some).ok_or(CmdError::new(format!("unknown sheet '{t}' (a4..a0, letter, tabloid)")))
    }
  }
}

/// `scale`: absent = leave it, null or "auto" = the drawing's, else "1:2" or 0.5.
fn scale_of(p: &Value) -> Result<Option<Option<f64>>, CmdError> {
  Ok(match p.get("scale") {
    None => None,
    Some(Value::Null) => Some(None),
    Some(Value::String(t)) if t == "auto" || t.is_empty() => Some(None),
    Some(Value::String(t)) => Some(Some(parse_scale(t)?)),
    Some(v) => Some(Some(v.as_f64().filter(|x| *x > 0.0).ok_or(CmdError::new("scale must be positive"))?)),
  })
}

/// The revision after `last`: A, B, … Z, AA (letters skip I, O, Q, S, X, Z as ASME Y14.35
/// does), or the next number.
fn next_rev(last: Option<&str>) -> String {
  const LETTERS: &str = "ABCDEFGHJKLMNPRTUVWY";
  let Some(last) = last else { return "A".into() };
  if let Ok(n) = last.parse::<u32>() {
    return format!("{:0width$}", n + 1, width = last.len());
  }
  let letters: Vec<char> = LETTERS.chars().collect();
  let mut digits: Vec<usize> = Vec::new();
  for c in last.to_ascii_uppercase().chars() {
    match letters.iter().position(|l| *l == c) {
      Some(i) => digits.push(i),
      None => return "A".into(),
    }
  }
  // count up in base 20, AA after Y
  let mut i = digits.len();
  loop {
    if i == 0 {
      digits.insert(0, 0);
      break;
    }
    i -= 1;
    if digits[i] + 1 < letters.len() {
      digits[i] += 1;
      break;
    }
    digits[i] = 0;
  }
  digits.iter().map(|d| letters[*d]).collect()
}

#[cfg(test)]
mod tests {
  use super::next_rev;

  #[test]
  fn revisions_count_up() {
    assert_eq!(next_rev(None), "A");
    assert_eq!(next_rev(Some("A")), "B");
    assert_eq!(next_rev(Some("H")), "J");
    assert_eq!(next_rev(Some("Y")), "AA");
    assert_eq!(next_rev(Some("AY")), "BA");
    assert_eq!(next_rev(Some("03")), "04");
    assert_eq!(next_rev(Some("9")), "10");
  }
}
