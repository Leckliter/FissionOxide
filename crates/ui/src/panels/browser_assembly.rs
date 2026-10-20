//! Browser folders for joint origins, rigid groups, motion links, contact sets and components
//! inserted from the data hub.

use fox_engine::Sel;
use fox_engine::model::{ComponentId, FeatureId, FeatureKind};
use serde_json::{Value, json};

use super::{is_open, tree_row};
use crate::icons::Icon;
use crate::{App, RenameTarget, theme};

pub(super) fn sections(app: &mut App, ui: &mut egui::Ui) {
  feature_folder(app, ui, 1, "Joint Origins", Icon::JointOrigin, |k| matches!(k, FeatureKind::JointOrigin { .. }));
  linked_components(app, ui);
  feature_folder(app, ui, 1, "Cams", Icon::DriveJoint, |k| matches!(k, FeatureKind::CamFollower { .. }));
}

fn is_rigid_group(k: &FeatureKind) -> bool {
  matches!(k, FeatureKind::RigidGroup { .. })
}

fn is_motion_link(k: &FeatureKind) -> bool {
  matches!(k, FeatureKind::MotionLink { .. })
}

/// Whether the design has relationships besides joints (rigid groups, motion links, contact sets).
pub(super) fn has_relationships(app: &App) -> bool {
  !app.s.design.contact_sets.is_empty()
    || app.s.design.features.iter().any(|f| is_rigid_group(&f.kind) || is_motion_link(&f.kind))
}

/// The folders under Relationships besides Joints, at `depth`.
pub(super) fn relationship_folders(app: &mut App, ui: &mut egui::Ui, depth: usize) {
  feature_folder(app, ui, depth, "Rigid Groups", Icon::Component, is_rigid_group);
  feature_folder(app, ui, depth, "Motion Links", Icon::DriveJoint, is_motion_link);
  contact_sets(app, ui, depth);
}

/// What a feature row shows after its name.
fn detail(app: &App, kind: &FeatureKind) -> String {
  let name = |id: FeatureId| app.s.design.feature(id).map(|f| f.name.clone()).unwrap_or_else(|| "?".into());
  let comp = |id: ComponentId| app.s.design.component(id).map(|c| c.name.clone()).unwrap_or_else(|| "?".into());
  match kind {
    FeatureKind::JointOrigin { component, .. } => comp(*component),
    FeatureKind::RigidGroup { components, .. } => components.iter().map(|c| comp(*c)).collect::<Vec<_>>().join(", "),
    FeatureKind::MotionLink { leader, follower, .. } => format!("{} → {}", name(*leader), name(*follower)),
    FeatureKind::CamFollower { cam, follower, .. } => format!("{} → {}", comp(*cam), name(*follower)),
    _ => String::new(),
  }
}

/// The bodies of the components a feature works on (clicking a row selects them).
fn bodies(app: &App, kind: &FeatureKind) -> Vec<Sel> {
  let comps = match kind {
    FeatureKind::JointOrigin { component, .. } => vec![*component],
    FeatureKind::RigidGroup { components, .. } => components.clone(),
    FeatureKind::CamFollower { cam, .. } => vec![*cam],
    _ => Vec::new(),
  };
  comps
    .iter()
    .filter_map(|c| app.s.design.component(*c))
    .flat_map(|c| c.bodies.iter().map(|b| Sel::Body { body: *b }))
    .collect()
}

/// A folder of features of one kind: click to select their components, right-click to rename
/// or delete.
fn feature_folder(
  app: &mut App,
  ui: &mut egui::Ui,
  depth: usize,
  title: &str,
  icon: Icon,
  of: fn(&FeatureKind) -> bool,
) {
  let rows: Vec<(FeatureId, String, String, Vec<Sel>)> = app
    .s
    .design
    .features
    .iter()
    .filter(|f| of(&f.kind))
    .map(|f| (f.id, f.name.clone(), detail(app, &f.kind), bodies(app, &f.kind)))
    .collect();
  if rows.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-asm").with(title);
  tree_row(ui, id, depth, Some(true), None, Icon::Folder, title, false, theme::TEXT);
  if !is_open(ui, id, true) {
    return;
  }
  for (fid, name, detail, sel) in rows {
    let failed = app.s.model.status(fid).and_then(|st| st.error.clone());
    let label = if detail.is_empty() { name.clone() } else { format!("{name}  ({detail})") };
    let color = if failed.is_some() { theme::WARNING } else { theme::TEXT };
    let ev = tree_row(ui, id.with(fid.0), depth + 1, None, None, icon, &label, false, color);
    if ev.clicked && !sel.is_empty() {
      app.s.selection = sel;
    }
    if let Some(r) = ev.response {
      let r = match &failed {
        Some(e) => r.on_hover_text(e.as_str()),
        None => r,
      };
      r.context_menu(|ui| {
        if ui.button("Rename").clicked() {
          ui.close();
          app.rename = Some((RenameTarget::Feature(fid.0), name.clone()));
        }
        ui.separator();
        if ui.button("Delete").clicked() {
          ui.close();
          app.run("feature.delete", json!({"feature": fid.0}));
        }
      });
    }
  }
}

/// Contact sets: the eye turns one on or off.
fn contact_sets(app: &mut App, ui: &mut egui::Ui, depth: usize) {
  let sets: Vec<(u32, String, bool, Vec<ComponentId>)> =
    app.s.design.contact_sets.iter().map(|c| (c.id, c.name.clone(), c.enabled, c.components.clone())).collect();
  if sets.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-contact-sets");
  tree_row(ui, id, depth, Some(true), None, Icon::Folder, "Contact Sets", false, theme::TEXT);
  if !is_open(ui, id, true) {
    return;
  }
  for (cid, name, enabled, comps) in sets {
    let ev = tree_row(ui, id.with(cid), depth + 1, None, Some(enabled), Icon::Interference, &name, false, theme::TEXT);
    if ev.eye {
      app.run("contact_set.enable", json!({"contact_set": cid, "enabled": !enabled}));
    }
    if ev.clicked {
      app.s.selection = comps
        .iter()
        .filter_map(|c| app.s.design.component(*c))
        .flat_map(|c| c.bodies.iter().map(|b| Sel::Body { body: *b }))
        .collect();
    }
    if let Some(r) = ev.response {
      r.on_hover_text("Driving a joint stops where these components touch").context_menu(|ui| {
        if ui.button(if enabled { "Disable" } else { "Enable" }).clicked() {
          ui.close();
          app.run("contact_set.enable", json!({"contact_set": cid, "enabled": !enabled}));
        }
        ui.separator();
        if ui.button("Delete").clicked() {
          ui.close();
          app.run("contact_set.delete", json!({"contact_set": cid}));
        }
      });
    }
  }
}

/// Components inserted from the data hub: their source version, and whether a newer one exists
/// (checked every few seconds).
fn linked_components(app: &mut App, ui: &mut egui::Ui) {
  if !app.s.design.features.iter().any(|f| matches!(f.kind, FeatureKind::InsertDesign { .. })) {
    return;
  }
  let id = egui::Id::new("tree-linked");
  let now = ui.input(|i| i.time);
  let key = (app.s.design.features.len(), app.s.design.components.len());
  let cached: Option<(f64, (usize, usize), Value)> = ui.data(|d| d.get_temp(id.with("links")));
  let links = match cached {
    Some((t, k, v)) if now - t < 3.0 && k == key => v,
    _ => {
      let v = app.s.execute("component.links", &json!({})).unwrap_or(Value::Null);
      ui.data_mut(|d| d.insert_temp(id.with("links"), (now, key, v.clone())));
      v
    }
  };
  let rows = links.as_array().cloned().unwrap_or_default();
  if rows.is_empty() {
    return;
  }
  tree_row(ui, id, 1, Some(true), None, Icon::Folder, "Inserted Components", false, theme::TEXT);
  if !is_open(ui, id, true) {
    return;
  }
  for l in rows {
    let cid = l["component"].as_u64().unwrap_or(0);
    let name = l["name"].as_str().unwrap_or_default().to_string();
    let linked = l["linked"].as_bool().unwrap_or(false);
    let stale = l["out_of_date"].as_bool().unwrap_or(false);
    let label = match (linked, l["version"].as_u64()) {
      (true, Some(v)) if stale => format!("{name}  (v{v}, newer version)"),
      (true, Some(v)) => format!("{name}  (v{v})"),
      _ => format!("{name}  (not linked)"),
    };
    let color = if stale || l["missing"].as_bool().unwrap_or(false) { theme::WARNING } else { theme::TEXT };
    let ev = tree_row(ui, id.with(cid), 2, None, None, Icon::InsertComponent, &label, false, color);
    if let Some(r) = ev.response {
      let tip = if linked { "Linked to a data hub design" } else { "A local copy of a data hub design" };
      r.on_hover_text(tip).context_menu(|ui| {
        if ui.add_enabled(linked && stale, egui::Button::new("Get Latest")).clicked() {
          ui.close();
          app.run("component.update_link", json!({"component": cid}));
          ui.data_mut(|d| d.remove::<(f64, (usize, usize), Value)>(id.with("links")));
        }
        if ui.add_enabled(linked, egui::Button::new("Break Link")).clicked() {
          ui.close();
          app.run("component.break_link", json!({"component": cid}));
          ui.data_mut(|d| d.remove::<(f64, (usize, usize), Value)>(id.with("links")));
        }
      });
    }
  }
}

/// Components in browser order (each followed by its sub-components) with their depth.
pub(super) fn component_tree(app: &App) -> Vec<(u32, usize)> {
  let d = &app.s.design;
  let mut out: Vec<(u32, usize)> = Vec::new();
  fn walk(d: &fox_engine::model::Design, parent: Option<ComponentId>, depth: usize, out: &mut Vec<(u32, usize)>) {
    for c in &d.components {
      let p = c.parent.filter(|p| d.component(*p).is_some());
      if p == parent && !out.iter().any(|(x, _)| *x == c.id.0) && depth < 64 {
        out.push((c.id.0, depth));
        walk(d, Some(c.id), depth + 1, out);
      }
    }
  }
  walk(d, None, 0, &mut out);
  for c in &d.components {
    if !out.iter().any(|(x, _)| *x == c.id.0) {
      out.push((c.id.0, 0));
    }
  }
  out
}

/// An inserted sub-assembly's rigid/flexible state, if `component` is one (or is in one).
pub(super) fn flexible_of(app: &App, component: u32) -> Option<bool> {
  app.s.design.features.iter().find_map(|f| match &f.kind {
    FeatureKind::InsertDesign { sub: Some(sub), .. }
      if sub.has_joints()
        && (sub.top == Some(ComponentId(component)) || sub.components.iter().any(|c| c.id.0 == component)) =>
    {
      Some(sub.flexible)
    }
    _ => None,
  })
}
