//! Browser folders for joint origins, rigid groups, motion links, contact sets and components
//! inserted from the data hub.

use clf_engine::Sel;
use clf_engine::model::{ComponentId, FeatureId, FeatureKind};
use serde_json::{Value, json};

use super::{is_open, tree_row};
use crate::icons::Icon;
use crate::{App, RenameTarget, theme};

pub(super) fn sections(app: &mut App, ui: &mut egui::Ui) {
  feature_folder(app, ui, "Joint Origins", Icon::JointOrigin, |k| matches!(k, FeatureKind::JointOrigin { .. }));
  feature_folder(app, ui, "Rigid Groups", Icon::Component, |k| matches!(k, FeatureKind::RigidGroup { .. }));
  feature_folder(app, ui, "Motion Links", Icon::DriveJoint, |k| matches!(k, FeatureKind::MotionLink { .. }));
  contact_sets(app, ui);
  linked_components(app, ui);
}

/// What a feature row shows after its name.
fn detail(app: &App, kind: &FeatureKind) -> String {
  let name = |id: FeatureId| app.s.design.feature(id).map(|f| f.name.clone()).unwrap_or_else(|| "?".into());
  let comp = |id: ComponentId| app.s.design.component(id).map(|c| c.name.clone()).unwrap_or_else(|| "?".into());
  match kind {
    FeatureKind::JointOrigin { component, .. } => comp(*component),
    FeatureKind::RigidGroup { components, .. } => components.iter().map(|c| comp(*c)).collect::<Vec<_>>().join(", "),
    FeatureKind::MotionLink { leader, follower, .. } => format!("{} → {}", name(*leader), name(*follower)),
    _ => String::new(),
  }
}

/// The bodies of the components a feature works on (clicking a row selects them).
fn bodies(app: &App, kind: &FeatureKind) -> Vec<Sel> {
  let comps = match kind {
    FeatureKind::JointOrigin { component, .. } => vec![*component],
    FeatureKind::RigidGroup { components, .. } => components.clone(),
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
fn feature_folder(app: &mut App, ui: &mut egui::Ui, title: &str, icon: Icon, of: fn(&FeatureKind) -> bool) {
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
  tree_row(ui, id, 1, Some(true), None, Icon::Folder, title, false, theme::TEXT);
  if !is_open(ui, id, true) {
    return;
  }
  for (fid, name, detail, sel) in rows {
    let failed = app.s.model.status(fid).and_then(|st| st.error.clone());
    let label = if detail.is_empty() { name.clone() } else { format!("{name}  ({detail})") };
    let color = if failed.is_some() { theme::WARNING } else { theme::TEXT };
    let ev = tree_row(ui, id.with(fid.0), 2, None, None, icon, &label, false, color);
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
fn contact_sets(app: &mut App, ui: &mut egui::Ui) {
  let sets: Vec<(u32, String, bool, Vec<ComponentId>)> =
    app.s.design.contact_sets.iter().map(|c| (c.id, c.name.clone(), c.enabled, c.components.clone())).collect();
  if sets.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-contact-sets");
  tree_row(ui, id, 1, Some(true), None, Icon::Folder, "Contact Sets", false, theme::TEXT);
  if !is_open(ui, id, true) {
    return;
  }
  for (cid, name, enabled, comps) in sets {
    let ev = tree_row(ui, id.with(cid), 2, None, Some(enabled), Icon::Interference, &name, false, theme::TEXT);
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
