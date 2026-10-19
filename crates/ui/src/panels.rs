//! Application bar, toolbar, browser tree, timeline, status bar, parameters window.

use clf_engine::model::{BodyId, FeatureId, FeatureKind};
use clf_engine::render::StandardView;
use clf_engine::{Sel, Session};
use egui::{Color32, RichText};
use serde_json::json;

use crate::dialogs::Kind;
use crate::icons::{self, Icon};
use crate::theme;
use crate::{App, RenameTarget, Tool};

pub fn app_bar(app: &mut App, ui: &mut egui::Ui) {
  egui::Panel::top("app_bar")
    .exact_size(30.0)
    .frame(egui::Frame::NONE.fill(theme::APP_BAR).inner_margin(egui::Margin::symmetric(8, 0)))
    .show(ui, |ui| {
      ui.horizontal_centered(|ui| {
        ui.visuals_mut().override_text_color = Some(theme::APP_BAR_TEXT);
        ui.label(RichText::new("CLFusion").strong().color(Color32::WHITE));
        ui.add_space(8.0);
        egui::MenuBar::new().ui(ui, |ui| {
          ui.menu_button("File", |ui| {
            if ui.button("New design").clicked() {
              app.new_design();
              ui.close();
            }
            if ui.button("Open…  (Ctrl+O)").clicked() {
              ui.close();
              app.open_dialog();
            }
            if ui.button("Save  (Ctrl+S)").clicked() {
              ui.close();
              app.save(false);
            }
            if ui.button("Save as…").clicked() {
              ui.close();
              app.save(true);
            }
            ui.separator();
            if ui.button("Import STEP…").clicked() {
              ui.close();
              app.import();
            }
            ui.menu_button("Export", |ui| {
              for (label, ext) in [("STEP (.step)", "step"), ("STL mesh (.stl)", "stl"), ("OBJ mesh (.obj)", "obj")] {
                if ui.button(label).clicked() {
                  ui.close();
                  app.export(ext);
                }
              }
            });
          });
          ui.menu_button("Edit", |ui| {
            if ui.add_enabled(app.s.can_undo(), egui::Button::new("Undo  (Ctrl+Z)")).clicked() {
              app.cancel_dialog();
              app.s.undo();
              ui.close();
            }
            if ui.add_enabled(app.s.can_redo(), egui::Button::new("Redo  (Ctrl+Y)")).clicked() {
              app.s.redo();
              ui.close();
            }
            ui.separator();
            if ui.button("Delete  (Del)").clicked() {
              app.delete_selection();
              ui.close();
            }
            if ui.button("Parameters…").clicked() {
              app.show_params = true;
              ui.close();
            }
          });
          ui.menu_button("View", |ui| {
            for (label, v) in [
              ("Home  (Home)", StandardView::Home),
              ("Top", StandardView::Top),
              ("Front", StandardView::Front),
              ("Right", StandardView::Right),
              ("Back", StandardView::Back),
              ("Left", StandardView::Left),
              ("Bottom", StandardView::Bottom),
            ] {
              if ui.button(label).clicked() {
                app.cam.set_view(v);
                ui.close();
              }
            }
            ui.separator();
            if ui.button("Fit  (F6)").clicked() {
              app.fit_view();
              ui.close();
            }
            ui.checkbox(&mut app.cam.ortho, "Orthographic");
            ui.checkbox(&mut app.show_edges, "Show edges");
          });
          ui.menu_button("Help", |ui| {
            ui.label("Navigate: right-drag orbit · middle-drag pan · wheel zoom");
            ui.label("Sketch: L line · R rectangle · C circle · A arc · D dimension · X construction");
            ui.label("Model: E extrude · F fillet · Esc cancel · Del delete");
          });
        });
        ui.add_space(12.0);
        if ui.add_enabled(app.s.can_undo(), egui::Button::new("↶").frame(false)).on_hover_text("Undo").clicked() {
          app.cancel_dialog();
          app.s.undo();
        }
        if ui.add_enabled(app.s.can_redo(), egui::Button::new("↷").frame(false)).on_hover_text("Redo").clicked() {
          app.s.redo();
        }
        if ui.add(egui::Button::new("💾").frame(false)).on_hover_text("Save").clicked() {
          app.save(false);
        }
      });
    });
}

fn group(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
  ui.vertical(|ui| {
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 2.0;
      add(ui)
    });
    ui.label(RichText::new(label).size(10.0).color(theme::GROUP_LABEL));
  });
  ui.separator();
}

fn menu_tool(ui: &mut egui::Ui, icon: Icon, label: &str, hint: &str, add: impl FnOnce(&mut egui::Ui)) {
  let r = icons::tool_button(ui, icon, label, hint, false);
  // small drop-down chevron
  let c = r.rect.right_top() + egui::vec2(-7.0, 14.0);
  ui.painter().add(egui::Shape::convex_polygon(
    vec![c + egui::vec2(-3.5, -2.0), c + egui::vec2(3.5, -2.0), c + egui::vec2(0.0, 2.0)],
    Color32::from_gray(90),
    egui::Stroke::NONE,
  ));
  egui::Popup::menu(&r).show(add);
}

pub fn toolbar(app: &mut App, ui: &mut egui::Ui) {
  egui::Panel::top("toolbar")
    .exact_size(74.0)
    .frame(
      egui::Frame::NONE
        .fill(theme::TOOLBAR)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .stroke(egui::Stroke::new(1.0, theme::BORDER)),
    )
    .show(ui, |ui| {
      ui.horizontal(|ui| {
        let sketching = app.s.edit_sketch.is_some();
        let mode = if sketching { "SKETCH" } else { "SOLID" };
        let color = if sketching { theme::SKETCH_ACCENT } else { theme::ACCENT };
        ui.vertical(|ui| {
          ui.add_space(14.0);
          ui.label(RichText::new(mode).strong().color(color).size(14.0));
        });
        ui.separator();
        egui::ScrollArea::horizontal()
          .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
          .show(ui, |ui| {
            ui.horizontal(|ui| {
              if sketching {
                sketch_toolbar(app, ui);
              } else {
                solid_toolbar(app, ui);
              }
            });
          });
      });
    });
}

fn solid_toolbar(app: &mut App, ui: &mut egui::Ui) {
  group(ui, "CREATE", |ui| {
    if icons::tool_button(ui, Icon::Sketch, "Sketch", "Create Sketch: pick a plane or a planar face", false).clicked() {
      app.new_sketch();
    }
    if icons::tool_button(ui, Icon::Extrude, "Extrude", "Extrude (E): select profiles", false).clicked() {
      app.start(Kind::Extrude);
    }
    if icons::tool_button(ui, Icon::Revolve, "Revolve", "Revolve profiles around an axis", false).clicked() {
      app.start(Kind::Revolve);
    }
    menu_tool(ui, Icon::Box, "Primitive", "Box, cylinder, sphere, torus", |ui| {
      for (l, k) in [("Box", Kind::Box), ("Cylinder", Kind::Cylinder), ("Sphere", Kind::Sphere), ("Torus", Kind::Torus)]
      {
        if ui.button(l).clicked() {
          app.start(k);
        }
      }
    });
    menu_tool(ui, Icon::Pattern, "Pattern", "Rectangular or circular pattern of bodies", |ui| {
      if ui.button("Rectangular pattern").clicked() {
        app.start(Kind::RectPattern);
      }
      if ui.button("Circular pattern").clicked() {
        app.start(Kind::CircPattern);
      }
    });
    if icons::tool_button(ui, Icon::Mirror, "Mirror", "Mirror bodies across a plane", false).clicked() {
      app.start(Kind::Mirror);
    }
  });
  group(ui, "MODIFY", |ui| {
    if icons::tool_button(ui, Icon::Fillet, "Fillet", "Fillet (F): select edges", false).clicked() {
      app.start(Kind::Fillet);
    }
    if icons::tool_button(ui, Icon::Chamfer, "Chamfer", "Chamfer: select edges", false).clicked() {
      app.start(Kind::Chamfer);
    }
    if icons::tool_button(ui, Icon::Shell, "Shell", "Shell: select faces to remove", false).clicked() {
      app.start(Kind::Shell);
    }
    if icons::tool_button(ui, Icon::Combine, "Combine", "Combine bodies: the first selected is the target", false)
      .clicked()
    {
      app.start(Kind::Combine);
    }
    if icons::tool_button(ui, Icon::Move, "Move", "Move / rotate bodies", false).clicked() {
      app.start(Kind::Move);
    }
  });
  group(ui, "CONSTRUCT", |ui| {
    if icons::tool_button(ui, Icon::Plane, "Offset Plane", "Construction plane offset from a plane or face", false)
      .clicked()
    {
      app.start(Kind::OffsetPlane);
    }
  });
  group(ui, "INSPECT", |ui| {
    if icons::tool_button(ui, Icon::Measure, "Properties", "Measure the selection", false).clicked() {
      app.start(Kind::Inspect);
    }
    if icons::tool_button(ui, Icon::Equal, "Parameters", "User parameters", app.show_params).clicked() {
      app.show_params = !app.show_params;
    }
  });
  group(ui, "INSERT", |ui| {
    if icons::tool_button(ui, Icon::Import, "STEP", "Import a STEP file", false).clicked() {
      app.import();
    }
  });
}

fn sketch_toolbar(app: &mut App, ui: &mut egui::Ui) {
  group(ui, "CREATE", |ui| {
    for (icon, label, hint, t) in [
      (Icon::Line, "Line", "Line (L): click points, Esc to finish", Tool::Line),
      (Icon::Rect, "Rectangle", "2-point rectangle (R)", Tool::Rect),
      (Icon::CenterRect, "Center Rect", "Centre rectangle", Tool::CenterRect),
      (Icon::Circle, "Circle", "Centre-diameter circle (C)", Tool::Circle),
      (Icon::Arc3, "3-Pt Arc", "Arc: start, end, then a point on the arc (A)", Tool::Arc3),
      (Icon::CenterArc, "Center Arc", "Arc: centre, start, end", Tool::CenterArc),
      (Icon::Polygon, "Polygon", "Inscribed hexagon", Tool::Polygon),
      (Icon::Slot, "Slot", "Centre-to-centre slot", Tool::Slot),
      (Icon::Point, "Point", "Sketch point (P)", Tool::Point),
    ] {
      if icons::tool_button(ui, icon, label, hint, app.tool == t).clicked() {
        app.set_tool(t);
      }
    }
  });
  group(ui, "CONSTRAINTS", |ui| {
    ui.vertical(|ui| {
      let items = [
        (Icon::Coincident, "Coincident: two points", "coincident"),
        (Icon::Horizontal, "Horizontal: lines or two points", "horizontal"),
        (Icon::Vertical, "Vertical: lines or two points", "vertical"),
        (Icon::Parallel, "Parallel: two lines", "parallel"),
        (Icon::Perpendicular, "Perpendicular: two lines", "perpendicular"),
        (Icon::Tangent, "Tangent: line/arc/circle pairs", "tangent"),
        (Icon::Equal, "Equal: two lines or two circles/arcs", "equal"),
        (Icon::Concentric, "Concentric: two circles/arcs", "concentric"),
        (Icon::Midpoint, "Midpoint: a point and a line", "midpoint"),
        (Icon::Collinear, "Collinear: two lines", "collinear"),
        (Icon::Symmetric, "Symmetric: two points and a line", "symmetric"),
        (Icon::Fix, "Fix: lock points in place", "fix"),
      ];
      for row in items.chunks(6) {
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing.x = 1.0;
          for (icon, hint, kind) in row {
            if icons::icon_button(ui, *icon, 22.0, hint, false, Color32::from_rgb(45, 52, 64)).clicked() {
              apply_constraint(app, kind);
            }
          }
        });
      }
    });
  });
  group(ui, "", |ui| {
    if icons::tool_button(
      ui,
      Icon::Dimension,
      "Dimension",
      "Sketch dimension (D): click geometry",
      app.tool == Tool::Dimension,
    )
    .clicked()
    {
      app.set_tool(Tool::Dimension);
    }
    if icons::tool_button(ui, Icon::Construction, "Construction", "Toggle construction (X)", false).clicked() {
      app.toggle_construction();
    }
  });
  ui.add_space(6.0);
  ui.vertical(|ui| {
    ui.add_space(6.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(104.0, 46.0), egui::Sense::click());
    let fill = if resp.hovered() { Color32::from_rgb(30, 130, 60) } else { theme::SKETCH_ACCENT };
    ui.painter().rect_filled(rect, 5.0, fill);
    icons::draw(
      ui.painter(),
      egui::Rect::from_center_size(rect.center() - egui::vec2(0.0, 8.0), egui::Vec2::splat(18.0)),
      Icon::Finish,
      Color32::WHITE,
    );
    ui.painter().text(
      rect.center() + egui::vec2(0.0, 12.0),
      egui::Align2::CENTER_CENTER,
      "FINISH SKETCH",
      egui::FontId::proportional(11.0),
      Color32::WHITE,
    );
    if resp.on_hover_text("Finish the sketch").clicked() {
      app.finish_sketch();
    }
  });
}

/// Applies a constraint to the current sketch selection.
pub fn apply_constraint(app: &mut App, kind: &str) {
  let Some(sk) = app.s.edit_sketch else { return };
  let mut points = Vec::new();
  let mut curves = Vec::new();
  for s in &app.s.selection {
    match s {
      Sel::SketchPoint { point, .. } => points.push(point.0),
      Sel::SketchCurve { curve, .. } => curves.push(curve.0),
      _ => {}
    }
  }
  // H/V on several lines: one constraint each
  if matches!(kind, "horizontal" | "vertical" | "fix") && points.len() != 2 {
    let mut any = false;
    if kind == "fix" {
      for p in &points {
        any |= app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "points": [p]})).is_some();
      }
    } else {
      for c in &curves {
        any |= app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "curves": [c]})).is_some();
      }
    }
    if any {
      app.s.selection.clear();
    } else if curves.is_empty() && points.is_empty() {
      app.status = Some((format!("Select geometry, then apply {kind}"), false));
    }
    return;
  }
  if app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "points": points, "curves": curves})).is_some() {
    app.s.selection.clear();
  }
}

pub fn status_bar(app: &mut App, ui: &mut egui::Ui) {
  egui::Panel::bottom("status")
    .exact_size(24.0)
    .frame(
      egui::Frame::NONE
        .fill(theme::TOOLBAR)
        .inner_margin(egui::Margin::symmetric(8, 0))
        .stroke(egui::Stroke::new(1.0, theme::BORDER)),
    )
    .show(ui, |ui| {
      ui.horizontal_centered(|ui| {
        if let Some((msg, err)) = &app.status {
          ui.label(RichText::new(msg).color(if *err { theme::ERROR } else { theme::MUTED }));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if let Some(sk) = app.s.edit_sketch
            && let Some(r) = app.s.model.sketches.get(&sk)
          {
            let txt = if !r.report.ok() {
              RichText::new("Sketch over-constrained / conflicting").color(theme::ERROR)
            } else if r.report.dof == 0 {
              RichText::new("Fully constrained").color(theme::SKETCH_ACCENT)
            } else {
              RichText::new(format!("{} degrees of freedom", r.report.dof)).color(theme::ACCENT)
            };
            ui.label(txt);
            ui.label(RichText::new(format!("{} profiles", r.profiles.len())).color(theme::MUTED));
          }
          ui.label(RichText::new(format!("{} bodies", app.s.model.bodies.len())).color(theme::MUTED));
          let n = app.s.selection.len();
          if n > 0 {
            ui.label(RichText::new(format!("{n} selected")).color(theme::MUTED));
          }
        });
      });
    });
}

fn feature_icon(k: &FeatureKind) -> Icon {
  use clf_engine::model::Primitive;
  match k {
    FeatureKind::Sketch { .. } => Icon::Sketch,
    FeatureKind::Extrude { .. } => Icon::Extrude,
    FeatureKind::Revolve { .. } => Icon::Revolve,
    FeatureKind::Fillet { .. } => Icon::Fillet,
    FeatureKind::Chamfer { .. } => Icon::Chamfer,
    FeatureKind::Shell { .. } => Icon::Shell,
    FeatureKind::Primitive { shape: Primitive::Box { .. }, .. } => Icon::Box,
    FeatureKind::Primitive { shape: Primitive::Cylinder { .. }, .. } => Icon::Cylinder,
    FeatureKind::Primitive { shape: Primitive::Sphere { .. }, .. } => Icon::Sphere,
    FeatureKind::Primitive { shape: Primitive::Torus { .. }, .. } => Icon::Torus,
    FeatureKind::ConstructionPlane { .. } => Icon::Plane,
    FeatureKind::Mirror { .. } => Icon::Mirror,
    FeatureKind::RectangularPattern { .. } => Icon::Pattern,
    FeatureKind::CircularPattern { .. } => Icon::CircPattern,
    FeatureKind::Move { .. } => Icon::Move,
    FeatureKind::Combine { .. } => Icon::Combine,
    FeatureKind::Import { .. } => Icon::Import,
    FeatureKind::RemoveBodies { .. } => Icon::Remove,
    FeatureKind::Flange { .. }
    | FeatureKind::ContourFlange { .. }
    | FeatureKind::EdgeFlange { .. }
    | FeatureKind::Hem { .. }
    | FeatureKind::Bend { .. } => Icon::Extrude,
    FeatureKind::ConvertToSheetMetal { .. }
    | FeatureKind::Unfold { .. }
    | FeatureKind::Refold { .. }
    | FeatureKind::Rip { .. } => Icon::Shell,
    FeatureKind::Loft { .. } | FeatureKind::Sweep { .. } => Icon::Extrude,
    FeatureKind::Hole { .. } => Icon::Cylinder,
    FeatureKind::SplitBody { .. } => Icon::Combine,
    FeatureKind::Draft { .. } => Icon::Chamfer,
  }
}

pub fn timeline(app: &mut App, ui: &mut egui::Ui) {
  egui::Panel::bottom("timeline")
    .exact_size(44.0)
    .frame(
      egui::Frame::NONE
        .fill(theme::TOOLBAR)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .stroke(egui::Stroke::new(1.0, theme::BORDER)),
    )
    .show(ui, |ui| {
      ui.horizontal_centered(|ui| {
        let n = app.s.design.features.len();
        let active = app.s.design.active_len();
        if ui.small_button("⏮").on_hover_text("Roll back to the beginning").clicked() {
          app.run("timeline.marker", json!({"at": 0}));
        }
        if ui.small_button("⏪").on_hover_text("Step back").clicked() && active > 0 {
          app.run("timeline.marker", json!({"at": active - 1}));
        }
        if ui.small_button("⏩").on_hover_text("Step forward").clicked() {
          let at = if active + 1 >= n { serde_json::Value::Null } else { json!(active + 1) };
          app.run("timeline.marker", json!({"at": at}));
        }
        if ui.small_button("⏭").on_hover_text("Roll forward to the end").clicked() {
          app.run("timeline.marker", json!({"at": null}));
        }
        ui.separator();
        egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
          ui.horizontal_centered(|ui| {
            let features: Vec<(FeatureId, String, Icon, bool)> = app
              .s
              .design
              .features
              .iter()
              .map(|f| (f.id, f.name.clone(), feature_icon(&f.kind), f.suppressed))
              .collect();
            for (i, (id, name, icon, suppressed)) in features.into_iter().enumerate() {
              if i == active {
                marker(ui);
              }
              let st = app.s.model.status(id).cloned().unwrap_or_default();
              let mut color = if st.error.is_some() {
                theme::ERROR
              } else if st.warning.is_some() {
                theme::WARNING
              } else {
                Color32::from_rgb(60, 66, 78)
              };
              if st.rolled_back || suppressed {
                color = Color32::from_gray(170);
              }
              let selected = app.s.selection.contains(&Sel::Feature { feature: id });
              let editing = app.s.edit_sketch == Some(id);
              let mut hover = name.clone();
              if let Some(e) = &st.error {
                hover = format!("{name}\n⚠ {e}");
              } else if let Some(w) = &st.warning {
                hover = format!("{name}\n{w}");
              }
              if suppressed {
                hover.push_str("\n(suppressed)");
              }
              let r = icons::icon_button(ui, icon, 30.0, &hover, selected || editing, color);
              if st.error.is_some() {
                ui.painter().circle_filled(r.rect.right_top() + egui::vec2(-4.0, 4.0), 3.5, theme::ERROR);
              }
              if r.clicked() {
                app.s.selection = vec![Sel::Feature { feature: id }];
              }
              if r.double_clicked() {
                edit_feature(app, id);
              }
              r.context_menu(|ui| {
                if ui.button("Edit feature").clicked() {
                  ui.close();
                  edit_feature(app, id);
                }
                if ui.button("Rename").clicked() {
                  ui.close();
                  app.rename = Some((RenameTarget::Feature(id.0), name.clone()));
                }
                if ui.button(if suppressed { "Unsuppress" } else { "Suppress" }).clicked() {
                  ui.close();
                  app.run("feature.suppress", json!({"feature": id.0}));
                }
                if ui.button("Roll back to here").clicked() {
                  ui.close();
                  app.run("timeline.marker", json!({"at": i + 1}));
                }
                ui.separator();
                if ui.button(RichText::new("Delete").color(theme::ERROR)).clicked() {
                  ui.close();
                  app.run("feature.delete", json!({"feature": id.0}));
                }
              });
            }
            if active >= n {
              marker(ui);
            }
          });
        });
      });
    });
  rename_popup(app, ui.ctx());
}

fn marker(ui: &mut egui::Ui) {
  let (r, _) = ui.allocate_exact_size(egui::vec2(6.0, 32.0), egui::Sense::hover());
  ui.painter().rect_filled(r.shrink2(egui::vec2(1.5, 0.0)), 1.0, theme::ACCENT);
}

/// Double-click in the timeline: sketches open for editing, other features get their dialog.
pub fn edit_feature(app: &mut App, id: FeatureId) {
  let Some(f) = app.s.design.feature(id) else { return };
  match &f.kind {
    FeatureKind::Sketch { .. } => app.edit_sketch(id),
    _ => {
      app.cancel_dialog();
      app.dialog = crate::dialogs::Dialog::edit(app, id);
      if app.dialog.is_none() {
        app.status = Some(("This feature has no editable properties yet".into(), false));
      }
    }
  }
}

fn rename_popup(app: &mut App, ctx: &egui::Context) {
  let Some((target, mut text)) = app.rename.take() else { return };
  let mut keep = true;
  egui::Window::new("Rename")
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
    .show(ctx, |ui| {
      let r = ui.text_edit_singleline(&mut text);
      r.request_focus();
      let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
      ui.horizontal(|ui| {
        if ui.button("OK").clicked() || enter {
          match target {
            RenameTarget::Feature(f) => {
              app.run("feature.rename", json!({"feature": f, "name": text}));
            }
            RenameTarget::Body(b) => {
              app.run("body.rename", json!({"body": b, "name": text}));
            }
          }
          keep = false;
        }
        if ui.button("Cancel").clicked() {
          keep = false;
        }
      });
    });
  if keep {
    app.rename = Some((target, text));
  }
}

pub fn browser(app: &mut App, ui: &mut egui::Ui) {
  egui::Panel::left("browser")
    .default_size(220.0)
    .resizable(true)
    .frame(
      egui::Frame::NONE
        .fill(theme::PANEL)
        .inner_margin(egui::Margin::same(8))
        .stroke(egui::Stroke::new(1.0, theme::BORDER)),
    )
    .show(ui, |ui| {
      ui.label(RichText::new("BROWSER").small().strong().color(theme::GROUP_LABEL));
      ui.add_space(2.0);
      egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let name = app.s.design.name.clone();
        egui::CollapsingHeader::new(RichText::new(format!("📄 {name}")).strong()).default_open(true).show(ui, |ui| {
          egui::CollapsingHeader::new("Origin").default_open(false).show(ui, |ui| {
            for (label, plane) in [("XY plane (Top)", "xy"), ("XZ plane (Front)", "xz"), ("YZ plane (Right)", "yz")] {
              ui.horizontal(|ui| {
                ui.label(format!("▱ {label}"));
                if ui.small_button("✏").on_hover_text("New sketch on this plane").clicked() {
                  app.create_sketch(json!(plane));
                }
              });
            }
          });
          bodies_section(app, ui);
          sketches_section(app, ui);
          planes_section(app, ui);
          let np = app.s.design.parameters.len();
          if ui.link(format!("ƒx Parameters ({np})")).clicked() {
            app.show_params = true;
          }
        });
      });
    });
}

fn bodies_section(app: &mut App, ui: &mut egui::Ui) {
  let bodies: Vec<(BodyId, String, bool)> =
    app.s.model.bodies.iter().map(|b| (b.id, app.s.body_name(b.id), app.s.body_visible(b.id))).collect();
  egui::CollapsingHeader::new(format!("Bodies ({})", bodies.len())).default_open(true).show(ui, |ui| {
    for (id, name, visible) in bodies {
      ui.horizontal(|ui| {
        let eye = if visible { "👁" } else { "◌" };
        if ui.small_button(eye).on_hover_text(if visible { "Hide" } else { "Show" }).clicked() {
          app.run("body.visible", json!({"body": id.0, "visible": !visible}));
        }
        let selected = app.s.selection.contains(&Sel::Body { body: id });
        let r = ui.selectable_label(selected, format!("⬛ {name}"));
        if r.clicked() {
          toggle_sel(&mut app.s, Sel::Body { body: id }, ui.input(|i| i.modifiers.shift || i.modifiers.command));
        }
        r.context_menu(|ui| {
          if ui.button("Rename").clicked() {
            ui.close();
            app.rename = Some((RenameTarget::Body(id.0), name.clone()));
          }
          if ui.button("Properties").clicked() {
            ui.close();
            app.s.selection = vec![Sel::Body { body: id }];
            app.start(Kind::Inspect);
          }
          if ui.button("Remove (timeline)").clicked() {
            ui.close();
            app.run("body.remove", json!({"bodies": [id.0]}));
          }
        });
      });
    }
  });
}

fn sketches_section(app: &mut App, ui: &mut egui::Ui) {
  let sketches: Vec<(FeatureId, String)> = app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::Sketch { .. }))
    .map(|f| (f.id, f.name.clone()))
    .collect();
  egui::CollapsingHeader::new(format!("Sketches ({})", sketches.len())).default_open(true).show(ui, |ui| {
    for (id, name) in sketches {
      let editing = app.s.edit_sketch == Some(id);
      let st = app.s.model.status(id).cloned().unwrap_or_default();
      let mut text = RichText::new(format!("✏ {name}"));
      if st.warning.is_some() || st.error.is_some() {
        text = text.color(theme::WARNING);
      }
      let r = ui.selectable_label(editing, text);
      if r.double_clicked() {
        app.edit_sketch(id);
      } else if r.clicked() {
        app.s.selection = vec![Sel::Feature { feature: id }];
      }
      r.on_hover_text("Double-click to edit");
    }
  });
}

fn planes_section(app: &mut App, ui: &mut egui::Ui) {
  let planes: Vec<(FeatureId, String)> = app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::ConstructionPlane { .. }))
    .map(|f| (f.id, f.name.clone()))
    .collect();
  if planes.is_empty() {
    return;
  }
  egui::CollapsingHeader::new("Construction").default_open(true).show(ui, |ui| {
    for (id, name) in planes {
      ui.horizontal(|ui| {
        ui.label(format!("▱ {name}"));
        if ui.small_button("✏").on_hover_text("New sketch on this plane").clicked() {
          app.create_sketch(json!({"feature": id.0}));
        }
      });
    }
  });
}

pub fn toggle_sel(s: &mut Session, item: Sel, add: bool) {
  if add {
    if let Some(i) = s.selection.iter().position(|x| *x == item) {
      s.selection.remove(i);
    } else {
      s.selection.push(item);
    }
  } else {
    s.selection = vec![item];
  }
}

pub fn parameters_window(app: &mut App, ctx: &egui::Context) {
  let mut open = true;
  egui::Window::new("ƒx Parameters").open(&mut open).default_width(460.0).show(ctx, |ui| {
    ui.label(
      RichText::new("User parameters drive dimensions and features: type a name anywhere a value is expected.")
        .small()
        .color(theme::MUTED),
    );
    ui.add_space(4.0);
    let params = app.s.design.parameters.clone();
    egui::Grid::new("params").striped(true).num_columns(5).show(ui, |ui| {
      ui.label(RichText::new("Name").strong());
      ui.label(RichText::new("Expression").strong());
      ui.label(RichText::new("Value").strong());
      ui.label(RichText::new("Comment").strong());
      ui.label("");
      ui.end_row();
      for p in params {
        ui.label(&p.name);
        let id = egui::Id::new(("param-expr", &p.name));
        let mut text = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| p.expr.clone());
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0));
        if r.changed() {
          ui.data_mut(|d| d.insert_temp(id, text.clone()));
        }
        if r.lost_focus() {
          if text != p.expr {
            app.run("param.set", json!({"name": p.name, "expr": text}));
          }
          ui.data_mut(|d| d.remove::<String>(id));
        }
        let value = app.s.model.params.get(&p.name).copied();
        let shown = match (value, p.kind) {
          (Some(v), clf_engine::model::ParamKind::Angle) => {
            clf_engine::model::sketch::Dim::new(v.to_degrees()).expr + "°"
          }
          (Some(v), clf_engine::model::ParamKind::Length) => {
            format!("{} mm", clf_engine::model::sketch::Dim::new(v).expr)
          }
          (Some(v), _) => clf_engine::model::sketch::Dim::new(v).expr,
          (None, _) => "error".into(),
        };
        let err = app.s.model.param_errors.get(&p.name).cloned();
        let l = ui.label(RichText::new(shown).color(if err.is_some() { theme::ERROR } else { theme::MUTED }));
        if let Some(e) = err {
          l.on_hover_text(e);
        }
        ui.label(RichText::new(&p.comment).small());
        if ui.small_button("🗑").on_hover_text("Delete (only if unused)").clicked() {
          app.run("param.remove", json!({"name": p.name}));
        }
        ui.end_row();
      }
    });
    ui.separator();
    let id = egui::Id::new("new-param");
    let (mut name, mut expr) = ui.data_mut(|d| d.get_temp::<(String, String)>(id)).unwrap_or_default();
    ui.horizontal(|ui| {
      ui.label("New:");
      ui.add(egui::TextEdit::singleline(&mut name).hint_text("name").desired_width(90.0));
      ui.add(egui::TextEdit::singleline(&mut expr).hint_text("40 mm").desired_width(120.0));
      if ui.button("＋ Add").clicked() && !name.is_empty() && !expr.is_empty() {
        let kind = if expr.contains("deg") || expr.contains('°') { "angle" } else { "length" };
        if app.run("param.add", json!({"name": name, "expr": expr, "kind": kind})).is_some() {
          name.clear();
          expr.clear();
        }
      }
    });
    ui.data_mut(|d| d.insert_temp(id, (name, expr)));
  });
  if !open {
    app.show_params = false;
  }
}
