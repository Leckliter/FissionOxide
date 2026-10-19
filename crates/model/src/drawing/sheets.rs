//! Several sheets per drawing, and the revision table.
//!
//! A drawing without a sheet list has one sheet, [`FIRST_SHEET`], of the drawing's own size and
//! scale, which is how drawings saved before sheets existed load. Views carry the id of the sheet
//! they are on; dimensions and annotations follow their view, notes carry a sheet of their own.

use serde::{Deserialize, Serialize};

use super::{Drawing, DrawingDim, MARGIN, SheetSize, TITLE_W};
use crate::plot::{Anchor, Item, Pen, Plot};

/// The id of a drawing's first sheet (and of every view saved before sheets existed).
pub const FIRST_SHEET: u32 = 1;

pub(super) fn first() -> u32 {
  FIRST_SHEET
}

pub(super) fn is_first(s: &u32) -> bool {
  *s == FIRST_SHEET
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
  pub id: u32,
  pub name: String,
  /// The sheet's size; `None` = the drawing's.
  #[serde(default)]
  pub size: Option<SheetSize>,
  /// Sheet mm per model mm; `None` = the drawing's scale.
  #[serde(default)]
  pub scale: Option<f64>,
}

impl Sheet {
  pub fn new(id: u32, name: impl Into<String>) -> Sheet {
    Sheet { id, name: name.into(), size: None, scale: None }
  }
}

/// A row of the revision table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Revision {
  /// "A", "B", … or "01".
  pub rev: String,
  #[serde(default)]
  pub description: String,
  #[serde(default)]
  pub date: String,
  /// Who approved it.
  #[serde(default)]
  pub by: String,
}

impl Drawing {
  /// The sheets in order (the implicit first sheet when the list is empty).
  pub fn sheet_list(&self) -> Vec<Sheet> {
    if self.sheets.is_empty() { vec![Sheet::new(FIRST_SHEET, "Sheet1")] } else { self.sheets.clone() }
  }

  pub fn has_sheet(&self, id: u32) -> bool {
    self.sheet_list().iter().any(|s| s.id == id)
  }

  /// Appends a sheet; returns its id.
  pub fn add_sheet(&mut self, name: &str) -> u32 {
    if self.sheets.is_empty() {
      self.sheets = self.sheet_list();
    }
    let id = self.sheets.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    let name = if name.trim().is_empty() { format!("Sheet{}", self.sheets.len() + 1) } else { name.to_string() };
    self.sheets.push(Sheet::new(id, name));
    id
  }

  /// Removes a sheet with its views, their dimensions and its notes. The last sheet stays.
  pub fn remove_sheet(&mut self, id: u32) -> Result<(), String> {
    if !self.has_sheet(id) {
      return Err(format!("no sheet {id} in the drawing"));
    }
    if self.sheet_list().len() == 1 {
      return Err("a drawing keeps at least one sheet".into());
    }
    let gone: Vec<u32> = self.views.iter().filter(|v| v.sheet == id).map(|v| v.id).collect();
    self.dimensions.retain(|x| match x {
      DrawingDim::Note { sheet, .. } => *sheet != id,
      o => o.view().is_none_or(|v| !gone.contains(&v)),
    });
    self.views.retain(|v| v.sheet != id);
    self.sheets.retain(|s| s.id != id);
    Ok(())
  }

  pub fn sheet_mut(&mut self, id: u32) -> Option<&mut Sheet> {
    if self.sheets.is_empty() && id == FIRST_SHEET {
      self.sheets = self.sheet_list();
    }
    self.sheets.iter_mut().find(|s| s.id == id)
  }
}

const ROW: f64 = 6.0;
const COLS: [f64; 4] = [15.0, 105.0, 35.0, 25.0];

/// Sheet mm the revision table takes at the top of the sheet.
pub(super) fn revision_table_height(revs: &[Revision]) -> f64 {
  if revs.is_empty() { 0.0 } else { (revs.len() + 1) as f64 * ROW }
}

/// The revision table in the sheet's top right corner: a header, then one row per revision,
/// oldest first.
pub(super) fn revision_table(plot: &mut Plot, revs: &[Revision]) {
  if revs.is_empty() {
    return;
  }
  let x1 = plot.width - MARGIN;
  let x0 = x1 - TITLE_W;
  let top = plot.height - MARGIN;
  let n = revs.len() + 1;
  let bottom = top - n as f64 * ROW;
  for i in 0..=n {
    let y = top - i as f64 * ROW;
    let pen = if i == n { Pen::Border } else { Pen::Thin };
    plot.items.push(Item::Line { pen, points: vec![[x0, y], [x1, y]] });
  }
  plot.items.push(Item::Line { pen: Pen::Border, points: vec![[x0, top], [x0, bottom]] });
  let mut x = x0;
  for w in &COLS[..COLS.len() - 1] {
    x += w;
    plot.items.push(Item::Line { pen: Pen::Thin, points: vec![[x, top], [x, bottom]] });
  }
  let cell = |plot: &mut Plot, col: usize, row: usize, text: &str| {
    let x = x0 + COLS[..col].iter().sum::<f64>();
    let y = top - (row + 1) as f64 * ROW + 1.8;
    plot.items.push(Item::Text { at: [x + 1.5, y], size: 2.5, text: text.to_string(), anchor: Anchor::Start });
  };
  for (k, h) in ["REV", "DESCRIPTION", "DATE", "BY"].iter().enumerate() {
    cell(plot, k, 0, h);
  }
  for (i, r) in revs.iter().enumerate() {
    for (k, t) in [&r.rev, &r.description, &r.date, &r.by].iter().enumerate() {
      cell(plot, k, i + 1, t);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::drawing::{DrawingView, ViewKind};

  #[test]
  fn sheets_add_and_remove_with_their_views() {
    let mut d = Drawing::new(1, "D", vec![ViewKind::Front, ViewKind::Top]);
    assert_eq!(d.sheet_list().len(), 1);
    assert!(d.sheets.is_empty());
    let two = d.add_sheet("");
    assert_eq!(two, 2);
    assert_eq!(d.sheet_list().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Sheet1", "Sheet2"]);
    let v = d.add_view(ViewKind::Iso, None);
    d.views.iter_mut().find(|x| x.id == v).unwrap().sheet = two;
    d.dimensions.push(DrawingDim::Note { at: [1.0, 1.0], text: "on 2".into(), sheet: two });
    d.dimensions.push(DrawingDim::Note { at: [1.0, 1.0], text: "on 1".into(), sheet: FIRST_SHEET });
    d.dimensions.push(DrawingDim::Datum { view: v, point: [0.0; 3], offset: [0.0, 9.0], label: "A".into() });
    assert_eq!(d.dimensions[2].sheet(&d), Some(two));
    d.remove_sheet(two).unwrap();
    assert_eq!(d.views.len(), 2);
    assert_eq!(d.dimensions.len(), 1);
    assert_eq!(d.remove_sheet(FIRST_SHEET), Err("a drawing keeps at least one sheet".into()));
    assert!(d.remove_sheet(7).is_err());
  }

  #[test]
  fn old_drawings_load_with_one_sheet() {
    // a drawing saved before sheets, revisions, welds and composite frames
    let old = r#"{"id":3,"name":"Old","views":[{"id":1,"kind":{"type":"front"}}],
      "dimensions":[{"type":"note","at":[20,30],"text":"hi"},
      {"type":"feature_control","view":1,"point":[0,0,0],"characteristic":"flatness","tolerance":0.1}]}"#;
    let d: Drawing = serde_json::from_str(old).unwrap();
    assert_eq!(d.views[0].sheet, FIRST_SHEET);
    assert!(matches!(d.dimensions[0], DrawingDim::Note { sheet: FIRST_SHEET, .. }));
    assert!(matches!(d.dimensions[1], DrawingDim::FeatureControl { lower: None, .. }));
    assert_eq!(d.sheet_list().len(), 1);
    assert!(d.revisions.is_empty());
    // and save the way they did: no sheet fields while there is one sheet
    let back = serde_json::to_string(&d).unwrap();
    assert!(!back.contains("sheets") && !back.contains("\"sheet\":1") && !back.contains("revisions"), "{back}");
    assert_eq!(serde_json::from_str::<Drawing>(&back).unwrap(), d);
    let v = DrawingView::new(4, ViewKind::Front);
    assert_eq!(v.sheet, FIRST_SHEET);
  }

  #[test]
  fn revision_table_rows() {
    let mut plot = Plot { width: 420.0, height: 297.0, items: Vec::new() };
    let revs = vec![
      Revision { rev: "A".into(), description: "First issue".into(), date: "2026-01-02".into(), by: "CL".into() },
      Revision { rev: "B".into(), description: "Hole moved".into(), date: "2026-03-04".into(), by: "CL".into() },
    ];
    revision_table(&mut plot, &revs);
    let t = plot.texts();
    assert_eq!(&t[..4], ["REV", "DESCRIPTION", "DATE", "BY"]);
    assert_eq!(&t[4..], ["A", "First issue", "2026-01-02", "CL", "B", "Hole moved", "2026-03-04", "CL"]);
    assert_eq!(revision_table_height(&revs), 18.0);
    // everything inside the top right corner of the border
    for it in &plot.items {
      if let Item::Line { points, .. } = it {
        assert!(points.iter().all(|p| p[0] >= 420.0 - 10.0 - 180.0 - 1e-9 && p[1] >= 297.0 - 10.0 - 18.0 - 1e-9));
      }
    }
    // row B sits under row A
    let y = |s: &str| {
      plot
        .items
        .iter()
        .find_map(|i| if let Item::Text { at, text, .. } = i { (text == s).then_some(at[1]) } else { None })
    };
    assert!(y("B").unwrap() < y("A").unwrap());
  }
}
