//! Selection behaviour, as in Fusion: selection priority (faces, edges, bodies or components),
//! selection filters, and Select Other (everything under the cursor, front to back).

use egui::{Pos2, RichText};
use fox_engine::model::BodyId;
use fox_engine::scene::Pick;
use fox_engine::{Sel, Session};

use crate::{App, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
  Face,
  Edge,
  Body,
  Component,
}

/// What can be picked; everything by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filters {
  pub bodies: bool,
  pub faces: bool,
  pub edges: bool,
  pub sketches: bool,
  pub profiles: bool,
}

impl Default for Filters {
  fn default() -> Self {
    Filters { bodies: true, faces: true, edges: true, sketches: true, profiles: true }
  }
}

impl Filters {
  pub fn get(&self, name: &str) -> bool {
    match name {
      "bodies" => self.bodies,
      "faces" => self.faces,
      "edges" => self.edges,
      "sketches" => self.sketches,
      _ => self.profiles,
    }
  }

  pub fn toggle(&mut self, name: &str) {
    let slot = match name {
      "bodies" => &mut self.bodies,
      "faces" => &mut self.faces,
      "edges" => &mut self.edges,
      "sketches" => &mut self.sketches,
      _ => &mut self.profiles,
    };
    *slot = !*slot;
  }

  /// Can this pick be hovered and selected with this priority?
  pub fn allows(&self, p: &Pick, priority: Priority) -> bool {
    let whole = matches!(priority, Priority::Body | Priority::Component);
    match p {
      Pick::Face { .. } if whole => self.bodies,
      Pick::Edge { .. } if whole => self.bodies,
      Pick::Face { .. } => self.faces && priority != Priority::Edge,
      Pick::Edge { .. } => self.edges,
      Pick::Profile { .. } => self.profiles,
      Pick::SketchPoint { .. } | Pick::SketchCurve { .. } => self.sketches,
    }
  }
}

/// The bodies of the component a body belongs to (just the body for a root body).
fn component_bodies(s: &Session, body: BodyId) -> Vec<BodyId> {
  match s.design.components.iter().find(|c| c.bodies.contains(&body)) {
    Some(c) => c.bodies.iter().copied().filter(|b| s.model.body(*b).is_some()).collect(),
    None => vec![body],
  }
}

/// What picking `p` selects under a priority.
pub fn targets(s: &Session, p: Pick, priority: Priority) -> Vec<Sel> {
  let body = match p {
    Pick::Face { body, .. } | Pick::Edge { body, .. } => Some(body),
    _ => None,
  };
  match (priority, body) {
    (Priority::Body, Some(b)) => vec![Sel::Body { body: b }],
    (Priority::Component, Some(b)) => component_bodies(s, b).into_iter().map(|body| Sel::Body { body }).collect(),
    _ => vec![p.to_sel()],
  }
}

/// Selects (or with `add`, toggles) what a click on `p` picks.
pub fn pick(app: &mut App, p: Pick, add: bool) {
  let items = targets(&app.s, p, app.sel_priority);
  if items.len() == 1 {
    crate::panels::toggle_sel(&mut app.s, items[0], add);
    return;
  }
  // a component: toggle the whole group together
  let all_in = items.iter().all(|i| app.s.selection.contains(i));
  if !add {
    app.s.selection.clear();
  }
  if add && all_in {
    app.s.selection.retain(|x| !items.contains(x));
  } else {
    for i in items {
      if !app.s.selection.contains(&i) {
        app.s.selection.push(i);
      }
    }
  }
}

/// The Select Other list: what lies under a point of the viewport, front to back.
pub(crate) struct OtherMenu {
  pub pos: Pos2,
  /// Entries: what selecting them selects, the pick to highlight while hovered, and a label.
  pub items: Vec<(Vec<Sel>, Option<Pick>, String)>,
}

impl OtherMenu {
  /// Builds the list from the picks under the cursor (nearest first): each face, the edge under
  /// the cursor, and the bodies and components they belong to.
  pub fn new(s: &Session, pos: Pos2, hover: Option<Pick>, hits: &[(f64, Pick)]) -> OtherMenu {
    let mut items: Vec<(Vec<Sel>, Option<Pick>, String)> = Vec::new();
    let mut bodies: Vec<BodyId> = Vec::new();
    if let Some(h @ (Pick::Edge { .. } | Pick::SketchCurve { .. } | Pick::SketchPoint { .. } | Pick::Profile { .. })) =
      hover
    {
      items.push((vec![h.to_sel()], Some(h), label(s, h)));
      if let Pick::Edge { body, .. } = h {
        bodies.push(body);
      }
    }
    for (_, p) in hits {
      items.push((vec![p.to_sel()], Some(*p), label(s, *p)));
      if let Pick::Face { body, .. } = p
        && !bodies.contains(body)
      {
        bodies.push(*body);
      }
    }
    for b in &bodies {
      items.push((vec![Sel::Body { body: *b }], None, format!("Body: {}", s.body_name(*b))));
    }
    for b in &bodies {
      if let Some(c) = s.design.components.iter().find(|c| c.bodies.contains(b))
        && !items.iter().any(|i| i.2 == format!("Component: {}", c.name))
      {
        let all = c.bodies.iter().map(|body| Sel::Body { body: *body }).collect();
        items.push((all, None, format!("Component: {}", c.name)));
      }
    }
    OtherMenu { pos, items }
  }
}

fn label(s: &Session, p: Pick) -> String {
  match p {
    Pick::Face { body, face } => {
      let kind = s
        .model
        .body(body)
        .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| format!("{:?}", f.kind)))
        .unwrap_or_default();
      format!("Face {face} ({kind}) of {}", s.body_name(body))
    }
    Pick::Edge { body, edge } => format!("Edge {edge} of {}", s.body_name(body)),
    Pick::SketchCurve { curve, .. } => format!("Sketch curve {}", curve.0),
    Pick::SketchPoint { point, .. } => format!("Sketch point {}", point.0),
    Pick::Profile { index, .. } => format!("Profile {}", index + 1),
  }
}

/// Shows the Select Other list; picking an entry selects it (Shift/Ctrl adds), hovering
/// highlights it in the viewport.
pub(crate) fn show_other(app: &mut App, ctx: &egui::Context) {
  let Some(menu) = app.other.take() else { return };
  let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command);
  let mut keep = true;
  let mut chosen: Option<Vec<Sel>> = None;
  let mut hovered: Option<Pick> = None;
  let resp = egui::Area::new(egui::Id::new("select-other"))
    .fixed_pos(menu.pos + egui::vec2(8.0, 8.0))
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      crate::panels::dialog_frame().show(ui, |ui| {
        ui.set_min_width(220.0);
        ui.label(RichText::new("SELECT OTHER").small().color(theme::GROUP_LABEL));
        ui.separator();
        if menu.items.is_empty() {
          ui.label(RichText::new("Nothing under the cursor").color(theme::MUTED));
        }
        for (sel, pick, text) in &menu.items {
          let r = ui.selectable_label(sel.iter().all(|x| app.s.selection.contains(x)), text);
          if r.hovered() {
            hovered = *pick;
          }
          if r.clicked() {
            chosen = Some(sel.clone());
          }
        }
      });
    })
    .response;
  if let Some(h) = hovered {
    app.hover = Some(h);
  }
  let picked = chosen.is_some();
  if let Some(sel) = chosen {
    if !add {
      app.s.selection.clear();
    }
    for x in sel {
      crate::panels::toggle_sel(&mut app.s, x, true);
    }
    keep = false;
  }
  if ctx.input(|i| i.pointer.any_pressed()) && !resp.hovered() && !picked {
    keep = false;
  }
  if keep {
    app.other = Some(menu);
  }
}
