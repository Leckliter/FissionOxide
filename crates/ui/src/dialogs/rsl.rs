//! Fusion's REVOLVE, SWEEP and LOFT dialogs: Revolve's Axis pick (no default; the origin axes
//! are offered for it), Project Axis, Extent Type and the angle handle; Sweep's and Loft's
//! Feature and Analysis tabs, Sweep's Type with Profile, Path and guide picks, Loft's Profiles
//! and Rails list boxes (with + and ✕), Guide Type and Tangent Edges.

use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{BodyId, FeatureId};
use fox_engine::{Sel, Session};
use serde_json::Value;

use super::icon_row::{Choice, choice as ic, greyed, icon_row};
use super::{Dialog, Kind, sel_chip};
use crate::App;
use crate::handles::Handle;
use crate::icons::{self, Icon};
use crate::origin::OriginItem;
use crate::theme;

/// The rows' state (kept in [`super::solid::Extra`]).
#[derive(Debug, Clone, Default)]
pub(super) struct State {
  /// Sweep / Loft: the Analysis tab is showing (else Feature).
  pub(super) analysis_tab: bool,
  /// The Analysis tab's type ("" = none, else a key of [`ANALYSIS_TYPES`]).
  pub(super) analysis: &'static str,
  /// Sweep Type (see [`SWEEP_TYPES`]; "" = Single Path).
  pub(super) sweep_type: &'static str,
  /// Sweep: the path's curves (empty = every open curve of the path sketch).
  pub(super) path_curves: Vec<u32>,
  /// Sweep / Loft Chain Selection off (on by default, as in Fusion).
  pub(super) no_chain: bool,
  /// Loft Guide Type: "" (rails) or "centerline".
  pub(super) guide: &'static str,
  /// Loft Tangent Edges (see [`TANGENT_EDGES`]; "" = merge).
  pub(super) tangent: &'static str,
  /// Loft: the highlighted row of the Profiles list and of the Rails list.
  pub(super) row: [Option<usize>; 2],
}

/// Fusion's Sweep Type. Solid Sweep (a tool body along the path) has no engine support.
pub(super) const SWEEP_TYPES: [(&str, &str, bool); 4] = [
  ("single", "Single Path", true),
  ("rail", "Path + Guide Rail", true),
  ("surface", "Path + Guide Surface", true),
  ("solid", "Solid Sweep", false),
];
/// Sweep Orientation.
const ORIENTATIONS: [(&str, &str, bool); 2] =
  [("perpendicular", "Perpendicular", true), ("parallel", "Parallel", true)];
/// Sweep Profile Scaling (with a guide rail).
const SCALINGS: [(&str, &str, bool); 3] =
  [("stretch", "Stretch", true), ("scale", "Scale", true), ("none", "None", true)];
/// Revolve Extent Type. To (up to an object) has no engine support.
const REVOLVE_EXTENTS: [(&str, &str, bool); 3] =
  [("partial", "Partial", true), ("full", "Full", true), ("to", "To", false)];
/// Revolve Direction, as Fusion words it.
const REVOLVE_DIRECTIONS: [(&str, &str); 3] =
  [("one_side", "One Side"), ("two_sides", "Two Sides"), ("symmetric", "Symmetric")];
/// The Analysis tab's Analysis Type; Isocurve has no viewport analysis yet.
const ANALYSIS_TYPES: [Choice; 4] = [
  ic("zebra", Icon::Zebra, "Zebra Analysis"),
  ic("draft", Icon::DraftAnalysis, "Draft Analysis"),
  ic("curvature", Icon::CurvMap, "Curvature Map Analysis"),
  greyed("isocurve", Icon::Isocurve, "Isocurve Analysis"),
];
/// Loft Guide Type.
const GUIDE_TYPES: [Choice; 2] = [ic("", Icon::SplineFit, "Rails"), ic("centerline", Icon::Centerline, "Centerline")];
/// Loft Tangent Edges: the loft merges tangent faces; keeping their edges has no engine support.
const TANGENT_EDGES: [Choice; 2] =
  [ic("", Icon::Tangent, "Merge tangent edges"), greyed("keep", Icon::Ruled, "Keep tangent edges")];

/// A combo box over `(key, label, enabled)` choices, a disabled one greyed with `why` as its
/// tooltip; true when the choice changed.
fn combo(
  ui: &mut egui::Ui,
  id: &str,
  cur: &mut &'static str,
  options: &[(&'static str, &str, bool)],
  why: &str,
) -> bool {
  let shown = options.iter().find(|(k, _, _)| k == cur).map(|(_, l, _)| *l).unwrap_or_default();
  let mut changed = false;
  egui::ComboBox::from_id_salt(id).selected_text(shown).width(150.0).show_ui(ui, |ui| {
    for (k, l, ok) in options {
      ui.add_enabled_ui(*ok, |ui| {
        if ui.selectable_label(cur == k, *l).clicked() && cur != k {
          *cur = k;
          changed = true;
        }
      })
      .response
      .on_disabled_hover_text(why);
    }
  });
  changed
}

/// "Select", "1 selected" or "3 selected", as Fusion's selection chips read.
fn picked(n: usize) -> String {
  if n == 0 { "Select".into() } else { format!("{n} selected") }
}

/// Fusion's selection list box: a framed column of rows, the highlighted one lit, with + and ✕
/// under it. Returns (+ clicked, ✕ clicked); a click on a row highlights it.
fn list_box(ui: &mut egui::Ui, id: &str, rows: &[String], sel: &mut Option<usize>) -> (bool, bool) {
  let (mut plus, mut remove) = (false, false);
  ui.vertical(|ui| {
    egui::Frame::new()
      .fill(egui::Color32::WHITE)
      .stroke(egui::Stroke::new(1.0, theme::BORDER))
      .inner_margin(egui::Margin::same(2))
      .show(ui, |ui| {
        ui.set_width(200.0);
        ui.set_min_height(54.0);
        ui.push_id(id, |ui| {
          ui.spacing_mut().item_spacing.y = 0.0;
          for (i, r) in rows.iter().enumerate() {
            let lit = *sel == Some(i);
            let resp = ui.add_sized([196.0, 18.0], egui::Button::selectable(lit, r.as_str()));
            if resp.clicked() {
              *sel = Some(i);
            }
          }
        });
      });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 2.0;
      plus = icons::icon_button(ui, Icon::Plus, 20.0, "Add", false, None).clicked();
      remove = ui
        .add_enabled_ui(!rows.is_empty(), |ui| icons::icon_button(ui, Icon::Close, 20.0, "Remove", false, None))
        .inner
        .clicked();
    });
  });
  (plus, remove)
}

/// Is this angle expression set to something other than nothing (a parameter counts as set)?
pub(super) fn angle_set(e: &str) -> bool {
  let e = e.trim();
  match fox_engine::expr::eval(e, fox_engine::expr::Kind::Angle, &fox_engine::expr::Empty) {
    Ok(v) => v.abs() >= 1e-12,
    Err(_) => !e.is_empty(),
  }
}

/// Sketch curves picked in the canvas, grouped by sketch in pick order, leaving out `skip`.
fn picked_curves(s: &Session, skip: &[FeatureId]) -> Vec<(FeatureId, Vec<u32>)> {
  let mut out: Vec<(FeatureId, Vec<u32>)> = Vec::new();
  for x in &s.selection {
    if let Sel::SketchCurve { sketch, curve } = x
      && !skip.contains(sketch)
    {
      match out.iter_mut().find(|(sk, _)| sk == sketch) {
        Some((_, c)) => c.push(curve.0),
        None => out.push((*sketch, vec![curve.0])),
      }
    }
  }
  out
}

impl Dialog {
  // ---- tabs ----

  /// Sweep's and Loft's Feature and Analysis tabs, above the rows.
  pub(super) fn tab_strip(&mut self, ui: &mut egui::Ui) {
    if !self.has_tabs() {
      return;
    }
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 14.0;
      for (analysis, name) in [(false, "Feature"), (true, "Analysis")] {
        let on = self.solid.rsl.analysis_tab == analysis;
        let text = egui::RichText::new(name).color(if on { theme::TAB_ACTIVE } else { theme::MUTED });
        let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
        if on {
          let r = resp.rect;
          ui.painter().line_segment(
            [egui::pos2(r.left(), r.bottom() + 3.0), egui::pos2(r.right(), r.bottom() + 3.0)],
            egui::Stroke::new(2.0, self.accent()),
          );
        }
        if resp.clicked() {
          self.solid.rsl.analysis_tab = analysis;
        }
      }
    });
    ui.add_space(10.0);
  }

  /// Sweep, Loft and Extend have Feature and Analysis tabs.
  pub(super) fn has_tabs(&self) -> bool {
    matches!(self.kind, Kind::Sweep | Kind::Loft | Kind::SurfSweep | Kind::SurfLoft)
      || matches!(self.kind, Kind::Form(f) if crate::forms::has_analysis_tab(f))
  }

  /// The Analysis tab: Analysis Type (four icon toggles; a lit one clicked again turns off) and
  /// Bodies.
  pub(super) fn analysis_rows(&mut self, ui: &mut egui::Ui) {
    ui.label("Analysis Type");
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 2.0;
      for c in &ANALYSIS_TYPES {
        let on = self.solid.rsl.analysis == c.key;
        if c.enabled {
          if icons::icon_button(ui, c.icon, 26.0, c.name, on, None).clicked() {
            self.solid.rsl.analysis = if on { "" } else { c.key };
          }
        } else {
          let hint = format!("{}: not available in the canvas yet", c.name);
          ui.add_enabled_ui(false, |ui| icons::icon_button(ui, c.icon, 26.0, &hint, false, Some(theme::MUTED)))
            .inner
            .on_disabled_hover_text(hint.clone());
        }
      }
    });
    ui.end_row();
    ui.label("Bodies");
    sel_chip(ui, self.accent(), "Select", false).on_hover_text("The analysis shows on every body");
    ui.end_row();
  }

  /// The analysis the open Sweep, Loft or Extend's Analysis tab shows in the canvas, if any.
  pub fn tab_analysis(&self) -> Option<&'static str> {
    (self.has_tabs() && !self.solid.rsl.analysis.is_empty()).then_some(self.solid.rsl.analysis)
  }

  // ---- revolve ----

  /// Fusion's REVOLVE rows: Profile, Axis (a pick, no default) and Project Axis, then once the
  /// axis is picked Extent Type, Angle, Direction and Operation.
  pub(super) fn revolve_fields(&mut self, ui: &mut egui::Ui) {
    let creating = self.editing.is_none();
    let accent = self.accent();
    ui.label("Profile");
    let n = self.profiles.len();
    if !creating {
      ui.label("(unchanged)");
    } else {
      sel_chip(ui, accent, &picked(n), n > 0);
    }
    ui.end_row();
    ui.label("Axis");
    if creating {
      let has = !self.axis.is_empty();
      sel_chip(ui, accent, &picked(usize::from(has)), has)
        .on_hover_text("Pick a sketch line, a straight edge, a construction axis or an origin axis");
    } else {
      ui.label(super::axis_label(&self.axis));
    }
    ui.end_row();
    ui.label("Project Axis");
    let mut project = true;
    ui.add_enabled(false, egui::Checkbox::without_text(&mut project)).on_disabled_hover_text(
      "Projecting an axis that is out of the profile's plane onto it needs engine support; the axis is used where it is",
    );
    ui.end_row();
    if creating && self.axis.is_empty() {
      return;
    }
    ui.label("Extent Type");
    let mut extent: &'static str = if self.solid.rev_full { "full" } else { "partial" };
    if combo(ui, "rev-extent", &mut extent, &REVOLVE_EXTENTS, "Revolving up to an object needs engine support") {
      self.solid.rev_full = extent == "full";
      self.dirty = true;
    }
    ui.end_row();
    if !self.solid.rev_full {
      ui.label("Angle");
      if super::value_edit(ui, &mut self.angle).changed() {
        self.dirty = true;
      }
      ui.end_row();
      ui.label("Direction");
      self.combo(ui, "rev_dir", &REVOLVE_DIRECTIONS, true);
      ui.end_row();
      if self.extent == "two_sides" {
        ui.label("Angle 2");
        if super::value_edit(ui, &mut self.angle2).changed() {
          self.dirty = true;
        }
        ui.end_row();
      }
    }
    self.operation_rows(ui);
    self.comp_name_row(ui);
  }

  /// Does the dialog show the origin planes and axes in the canvas while it is open (Revolve, for
  /// its axis pick, and Loft), whatever the Origin folder's eyes say?
  pub fn shows_origin(&self) -> bool {
    self.editing.is_none() && matches!(self.kind, Kind::Revolve | Kind::Loft)
  }

  /// Can an origin axis be picked for the dialog (Revolve's Axis)?
  pub fn takes_origin_axis(&self) -> bool {
    self.editing.is_none() && self.kind == Kind::Revolve
  }

  /// The origin axis the dialog is using (it draws as selected).
  pub(crate) fn origin_axis(&self) -> Option<OriginItem> {
    if !self.takes_origin_axis() {
      return None;
    }
    match self.axis.as_str() {
      "x" => Some(OriginItem::X),
      "y" => Some(OriginItem::Y),
      "z" => Some(OriginItem::Z),
      _ => None,
    }
  }

  /// An origin axis clicked in the canvas becomes Revolve's axis; false when the dialog takes
  /// none (the click is the canvas's).
  pub(crate) fn pick_origin_axis(&mut self, item: OriginItem) -> bool {
    let key = match item {
      OriginItem::X => "x",
      OriginItem::Y => "y",
      OriginItem::Z => "z",
      _ => return false,
    };
    if !self.takes_origin_axis() {
      return false;
    }
    if self.axis != key {
      self.axis = key.into();
      self.dirty = true;
    }
    true
  }

  /// The revolve axis as a world line (a point on it and its unit direction).
  fn revolve_axis_line(&self, s: &Session) -> Option<(DVec3, DVec3)> {
    if let Some(n) = self.axis.strip_prefix("line:") {
      let sk = self.sketch?;
      let r = s.model.sketches.get(&sk)?;
      let c = r.sketch.curve(fox_engine::sketch::CurveId(n.parse().ok()?))?;
      let fox_engine::sketch::CurveKind::Line { a, b } = c.kind else { return None };
      let (a, b) = (r.plane.to_world(r.sketch.pos(a)?), r.plane.to_world(r.sketch.pos(b)?));
      return Some((a, (b - a).try_normalize()?));
    }
    if self.axis == "edge" {
      let body = BodyId(self.solid.axis_edge.get("body")?.as_u64()? as u32);
      let edge = self.solid.axis_edge.get("edge")?.as_u64()? as u32;
      let e = *s.model.body(body)?.edges().iter().find(|e| e.id == edge)?;
      return Some((e.start, (e.end - e.start).try_normalize()?));
    }
    super::manip::axis_line(s, &self.axis)
  }

  /// Revolve's angle handle: a dial about the axis through the profile, for a Partial revolve
  /// once the axis is picked (its floating box edits the angle).
  pub(super) fn revolve_handles(&self, s: &Session) -> Option<Vec<Handle>> {
    if self.solid.rev_full || self.axis.is_empty() {
      return None;
    }
    let (o, d) = self.revolve_axis_line(s)?;
    let r = s.model.sketches.get(&self.sketch?)?;
    let at = self.profiles.first().copied().or_else(|| r.profiles.first().map(|p| p.interior_point()))?;
    let c = r.plane.to_world(at);
    let foot = o + d * (c - o).dot(d);
    let zero = (c - foot).try_normalize()?;
    let a = self.ang(s, &self.angle)?;
    Some(vec![Handle::dial(foot, d, zero, Some((c - foot).length()), a, super::manip::deg(a))])
  }

  // ---- operation ----

  /// Operation, then (when it cuts) the Objects To Cut group under it.
  pub(super) fn operation_rows(&mut self, ui: &mut egui::Ui) {
    ui.label("Operation");
    let before = self.op;
    self.combo(ui, "op", &super::OPS, false);
    if self.op != before {
      self.op_chosen();
    }
    ui.end_row();
    self.objects_to_cut_rows(ui);
  }

  // ---- sweep ----

  pub(super) fn sweep_type(&self) -> &'static str {
    if self.solid.rsl.sweep_type.is_empty() { "single" } else { self.solid.rsl.sweep_type }
  }

  /// Fusion's SWEEP: the Feature tab's Type, Profile, Path (and the guide rail or surface), Chain
  /// Selection, Orientation or Profile Scaling, the taper and twist once there is a path, and
  /// Operation; or the Analysis tab.
  pub(super) fn sweep_fields(&mut self, app: &App, ui: &mut egui::Ui, dirty: &mut bool) {
    if self.solid.rsl.analysis_tab {
      self.analysis_rows(ui);
      return;
    }
    let accent = self.accent();
    let creating = self.editing.is_none();
    ui.label("Type");
    let mut ty = self.sweep_type();
    if combo(ui, "sweep-type", &mut ty, &SWEEP_TYPES, "Sweeping a tool body along a path needs engine support") {
      self.solid.rsl.sweep_type = ty;
      // the guide the other types use goes
      if ty != "rail" {
        self.solid.rail = None;
        self.solid.rail2 = None;
      }
      if ty != "surface" {
        self.solid.guide_surface = Value::Null;
      }
      *dirty = true;
    }
    ui.end_row();
    ui.label("Profile");
    sel_chip(ui, accent, &picked(self.profiles.len()), !self.profiles.is_empty());
    ui.end_row();
    ui.label("Path");
    let n = if self.path.is_none() { 0 } else { self.solid.rsl.path_curves.len().max(1) };
    sel_chip(ui, accent, &picked(n), n > 0).on_hover_text(
      self.path.and_then(|p| app.s.design.feature(p)).map(|f| f.name.clone()).unwrap_or("A sketch path".into()),
    );
    ui.end_row();
    if ty == "rail" {
      ui.label("Guide Rail");
      let has = self.solid.rail.is_some();
      sel_chip(ui, accent, &picked(usize::from(has)), has);
      ui.end_row();
    }
    if ty == "surface" {
      ui.label("Guide Surface");
      let has = !self.solid.guide_surface.is_null();
      sel_chip(ui, accent, &picked(usize::from(has)), has);
      ui.end_row();
    }
    ui.label("Chain Selection");
    let mut chain = !self.solid.rsl.no_chain;
    if ui.add_enabled(creating, egui::Checkbox::without_text(&mut chain)).changed() {
      self.solid.rsl.no_chain = !chain;
    }
    ui.end_row();
    match ty {
      "rail" => {
        ui.label("Profile Scaling");
        let two = self.solid.rail2.is_some();
        if self.solid.scaling.is_empty() || (!two && self.solid.scaling == "stretch") {
          self.solid.scaling = if two { "stretch" } else { "scale" };
        }
        let one_rail = "Stretch needs a second rail";
        let options = [("stretch", "Stretch", two), SCALINGS[1], SCALINGS[2]];
        *dirty |= combo(ui, "sweep-scaling", &mut self.solid.scaling, &options, one_rail);
        ui.end_row();
      }
      "single" => {
        ui.label("Orientation");
        let mut o: &'static str = if self.solid.parallel { "parallel" } else { "perpendicular" };
        if combo(ui, "sweep-orient", &mut o, &ORIENTATIONS, "") {
          self.solid.parallel = o == "parallel";
          *dirty = true;
        }
        ui.end_row();
        if self.path.is_some() {
          for (label, v, shown) in
            [("Taper Angle", &mut self.solid.taper, true), ("Twist Angle", &mut self.solid.twist, !self.solid.parallel)]
          {
            if !shown {
              continue;
            }
            if v.trim().is_empty() {
              *v = "0 deg".into();
            }
            ui.label(label);
            *dirty |= super::value_edit(ui, v).changed();
            ui.end_row();
          }
        }
      }
      _ => {}
    }
    self.operation_rows(ui);
    super::newcomp::comp_name_field(ui, creating, self.op, &mut self.comp_name, dirty);
  }

  /// The path (and, for Path + Guide Rail, the rail) picked in the canvas: the curves of the
  /// first sketch picked other than the profile's, with the curves chained to them under Chain
  /// Selection; the rail is the next sketch's.
  pub(super) fn sync_sweep_picks(&mut self, s: &Session) {
    let ty = self.sweep_type();
    if ty == "surface" {
      self.sync_guide_surface(s);
    }
    let groups = picked_curves(s, &self.sketch.into_iter().collect::<Vec<_>>());
    let Some((sk, curves)) = groups.first() else { return };
    let curves = match s.design.sketch(*sk) {
      Ok(sketch) if !self.solid.rsl.no_chain => super::with_curve_chain(sketch, curves),
      _ => curves.clone(),
    };
    if self.path != Some(*sk) || self.solid.rsl.path_curves != curves {
      self.path = Some(*sk);
      self.solid.rsl.path_curves = curves;
      self.dirty = true;
    }
    if ty == "rail"
      && let Some((rail, _)) = groups.get(1)
      && self.solid.rail != Some(*rail)
    {
      self.solid.rail = Some(*rail);
      self.dirty = true;
    }
  }

  // ---- loft ----

  fn loft_guide_rows(&self) -> Vec<(FeatureId, Vec<u32>)> {
    if self.solid.rsl.guide == "centerline" {
      self.solid.centerline.clone().into_iter().collect()
    } else {
      self.solid.rails.clone()
    }
  }

  /// Fusion's LOFT: the Feature tab's Profiles list, Guide Type, Rails (or Centerline) list,
  /// Chain Selection, Closed, Tangent Edges and Operation; or the Analysis tab.
  pub(super) fn loft_fields(&mut self, app: &mut App, ui: &mut egui::Ui, dirty: &mut bool) {
    if self.solid.rsl.analysis_tab {
      self.analysis_rows(ui);
      return;
    }
    let accent = self.accent();
    let creating = self.editing.is_none();
    let name = |s: &Session, sk: u32| s.design.feature(FeatureId(sk)).map(|f| f.name.clone()).unwrap_or_default();
    ui.label("Profiles");
    sel_chip(ui, accent, &picked(self.sections.len()), !self.sections.is_empty());
    ui.end_row();
    ui.label("");
    let rows: Vec<String> = self
      .sections
      .iter()
      .enumerate()
      .map(|(i, (sk, _))| format!("Profile{}  ({})", i + 1, name(&app.s, *sk)))
      .collect();
    let (plus, remove) = list_box(ui, "loft-profiles", &rows, &mut self.solid.rsl.row[0]);
    if plus {
      self.pick_row = 0;
    }
    if remove && creating {
      let i = self.solid.rsl.row[0].take().unwrap_or(rows.len() - 1).min(rows.len() - 1);
      let (sk, _) = self.sections.remove(i);
      app.s.selection.retain(|x| !matches!(x, Sel::Profile { sketch, .. } if sketch.0 == sk));
      *dirty = true;
    }
    ui.end_row();
    ui.label("Guide Type");
    if icon_row(ui, &mut self.solid.rsl.guide, &GUIDE_TYPES, "") {
      // the other type's picks go
      self.solid.rails.clear();
      self.solid.centerline = None;
      *dirty = true;
    }
    ui.end_row();
    let centerline = self.solid.rsl.guide == "centerline";
    let guides = self.loft_guide_rows();
    ui.label(if centerline { "Centerline" } else { "Rails" });
    sel_chip(ui, accent, &picked(guides.len()), !guides.is_empty());
    ui.end_row();
    if !centerline {
      ui.label("");
      let rows: Vec<String> = guides.iter().map(|(sk, _)| name(&app.s, sk.0)).collect();
      let (plus, remove) = list_box(ui, "loft-rails", &rows, &mut self.solid.rsl.row[1]);
      if plus {
        self.pick_row = 1;
      }
      if remove && creating {
        let i = self.solid.rsl.row[1].take().unwrap_or(rows.len() - 1).min(rows.len() - 1);
        let (sk, _) = self.solid.rails.remove(i);
        app.s.selection.retain(|x| !matches!(x, Sel::SketchCurve { sketch, .. } if *sketch == sk));
        *dirty = true;
      }
      ui.end_row();
    }
    ui.label("Chain Selection");
    let mut chain = !self.solid.rsl.no_chain;
    if ui.add_enabled(creating, egui::Checkbox::without_text(&mut chain)).changed() {
      self.solid.rsl.no_chain = !chain;
    }
    ui.end_row();
    ui.label("Closed");
    *dirty |= ui.add(egui::Checkbox::without_text(&mut self.flag2)).changed();
    ui.end_row();
    ui.label("Tangent Edges");
    icon_row(ui, &mut self.solid.rsl.tangent, &TANGENT_EDGES, "needs engine support; the loft merges tangent faces");
    ui.end_row();
    self.operation_rows(ui);
    super::newcomp::comp_name_field(ui, creating, self.op, &mut self.comp_name, dirty);
  }

  /// The rails (or centerline) picked in the canvas: each sketch other than the sections' whose
  /// curves are picked, with the curves chained to them under Chain Selection.
  pub(super) fn sync_loft_guides(&mut self, s: &Session) {
    let skip: Vec<FeatureId> = self.sections.iter().map(|(sk, _)| FeatureId(*sk)).collect();
    let groups: Vec<(FeatureId, Vec<u32>)> = picked_curves(s, &skip)
      .into_iter()
      .map(|(sk, c)| match s.design.sketch(sk) {
        Ok(sketch) if !self.solid.rsl.no_chain => (sk, super::with_curve_chain(sketch, &c)),
        _ => (sk, c),
      })
      .collect();
    if groups.is_empty() {
      return;
    }
    if self.solid.rsl.guide == "centerline" {
      let c = groups.into_iter().next_back();
      if c != self.solid.centerline {
        self.solid.centerline = c;
        self.dirty = true;
      }
    } else if groups != self.solid.rails {
      self.solid.rails = groups;
      self.dirty = true;
    }
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::model::FeatureId;
  use fox_engine::sketch::CurveId;
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::super::{Dialog, Kind};
  use crate::origin::OriginItem;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  fn line(s: &mut Session, plane: &str, a: [f64; 2], b: [f64; 2]) -> FeatureId {
    run(s, "sketch.create", json!({"plane": plane}));
    run(s, "sketch.line", json!({"points": [a, b]}));
    let sk = s.edit_sketch.unwrap();
    run(s, "sketch.finish", json!({}));
    sk
  }

  fn first_curve(s: &Session, sk: FeatureId) -> CurveId {
    s.design.sketch(sk).unwrap().curves[0].id
  }

  #[test]
  fn revolve_waits_for_an_axis_and_takes_an_origin_axis() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.rect", json!({"corner1": [5, 0], "corner2": [10, 10]}));
    let sk = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Revolve);
    d.sketch = Some(sk);
    d.profiles = vec![s.model.sketches[&sk].profiles[0].interior_point()];
    d.axis.clear();
    // no axis: nothing to preview, OK greyed, the prompt asks for the axis
    assert!(d.command().is_none() && !d.ready() && !d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select the axis to revolve the profile around"));
    assert!(d.shows_origin() && d.takes_origin_axis());
    assert!(d.revolve_handles(&s).is_none());
    // the Z origin axis clicked in the canvas
    assert!(d.pick_origin_axis(OriginItem::Z));
    assert_eq!(d.axis, "z");
    assert_eq!(d.origin_axis(), Some(OriginItem::Z));
    assert!(d.ready() && d.ok_enabled());
    assert!(!d.pick_origin_axis(OriginItem::Xy), "a plane is no axis");
    // the angle handle: a dial about Z through the profile, 360 deg to start with
    let h = d.revolve_handles(&s).unwrap();
    assert_eq!(h.len(), 1);
    assert!((h[0].value - std::f64::consts::TAU).abs() < 1e-9);
    assert_eq!(d.handle_field(0).map(|v| v.clone()), Some("360 deg".to_string()));
    // dragged to a quarter turn
    d.set_handle(&s, 0, std::f64::consts::FRAC_PI_2, 1f64.to_radians());
    assert_eq!(d.angle, "90 deg");
    d.preview(&mut s);
    let ring = std::f64::consts::PI * 75.0 * 10.0;
    assert!((volume(&s) - ring / 4.0).abs() < 0.5, "{}", volume(&s));
    // Full: no handle
    d.solid.rev_full = true;
    assert!(d.revolve_handles(&s).is_none());
  }

  #[test]
  fn sweep_profile_and_path_are_picked_with_chain_selection() {
    let mut s = Session::default();
    // an L-shaped path of two lines, and a circle to sweep at its start
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30], [20, 30]]}));
    let path = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 3}));
    let prof = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Sweep);
    d.sketch = Some(prof);
    assert_eq!(d.sweep_type(), "single");
    assert!(!d.ready());
    assert_eq!(d.prompt(false).as_deref(), Some("Specify a sweep type or select an object to sweep"));
    // the profile, then one line of the path: Chain Selection takes the other too
    s.selection = vec![Sel::Profile { sketch: prof, index: 0 }];
    d.sync_selection(&s);
    assert_eq!(d.profiles.len(), 1);
    assert!(!d.ready(), "no path yet");
    let c = first_curve(&s, path);
    s.selection.push(Sel::SketchCurve { sketch: path, curve: c });
    d.sync_selection(&s);
    assert_eq!(d.path, Some(path));
    assert_eq!(d.solid.rsl.path_curves.len(), 2, "{:?}", d.solid.rsl.path_curves);
    assert!(d.ready());
    let (_, p) = d.command().unwrap();
    assert_eq!(p["path_curves"].as_array().map(Vec::len), Some(2), "{p}");
    // zero taper and twist stay out of the command
    d.solid.twist = "0 deg".into();
    d.solid.taper = "0 deg".into();
    let (_, p) = d.command().unwrap();
    assert!(p.get("twist").is_none() && p.get("taper").is_none(), "{p}");
    // Chain Selection off: just the picked line
    d.solid.rsl.no_chain = true;
    d.sync_selection(&s);
    assert_eq!(d.solid.rsl.path_curves, vec![c.0]);
    d.preview(&mut s);
    assert!((volume(&s) - std::f64::consts::PI * 9.0 * 30.0).abs() < 1.0, "{}", volume(&s));

    // Path + Guide Rail: the next sketch picked is the rail
    let mut s = Session::default();
    let path = line(&mut s, "xz", [5.0, 0.0], [5.0, 50.0]);
    let rail = line(&mut s, "xz", [0.0, 0.0], [0.0, 50.0]);
    let mut d = Dialog::blank(Kind::Sweep);
    d.solid.rsl.sweep_type = "rail";
    s.selection = vec![
      Sel::SketchCurve { sketch: path, curve: first_curve(&s, path) },
      Sel::SketchCurve { sketch: rail, curve: first_curve(&s, rail) },
    ];
    d.sync_selection(&s);
    assert_eq!((d.path, d.solid.rail), (Some(path), Some(rail)));
  }

  #[test]
  fn loft_lists_profiles_and_rails_and_shows_the_origin() {
    let mut s = Session::default();
    s.design.behaviours = fox_engine::model::Behaviours::legacy();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
    let a = s.edit_sketch.unwrap();
    run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 40}));
    let top = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": {"feature": top}}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
    let b = s.edit_sketch.unwrap();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.arc", json!({"start": [10, 0], "mid": [15, 20], "end": [10, 40]}));
    let rail = s.edit_sketch.unwrap();
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Loft);
    assert!(d.shows_origin() && !d.takes_origin_axis());
    s.selection = vec![
      Sel::Profile { sketch: a, index: 0 },
      Sel::Profile { sketch: b, index: 0 },
      Sel::SketchCurve { sketch: rail, curve: first_curve(&s, rail) },
    ];
    d.sync_selection(&s);
    assert_eq!(d.sections.len(), 2);
    assert_eq!(d.solid.rails.len(), 1);
    assert!(d.ok_enabled());
    let (_, p) = d.command().unwrap();
    assert_eq!(p["rails"][0]["sketch"], json!(rail.0), "{p}");
    d.preview(&mut s);
    let m = s.model.bodies[0].shape.mass().unwrap();
    assert!((m.bbox_max.x - 15.0).abs() < 0.3, "{m:?}");
    // Centerline instead: the same sketch as the centre line
    d.solid.rsl.guide = "centerline";
    d.solid.rails.clear();
    d.sync_selection(&s);
    assert!(d.solid.rails.is_empty());
    assert_eq!(d.solid.centerline.as_ref().map(|c| c.0), Some(rail));
    assert_eq!(d.command().unwrap().1["centerline"]["sketch"], json!(rail.0));
  }

  #[test]
  fn revolve_and_sweep_open_with_nothing_preset_and_the_origin_shown() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.rect", json!({"corner1": [5, 0], "corner2": [10, 10]}));
    run(&mut s, "sketch.finish", json!({}));
    let mut app = crate::App::with_session(s);
    let d = Dialog::open(&mut app, Kind::Revolve);
    assert!(d.axis.is_empty(), "no preset axis");
    assert_eq!(d.profiles.len(), 1, "the only profile is picked");
    assert!(!d.ok_enabled() && !d.preview_applied(), "nothing previews until the axis is picked");
    assert!(!crate::origin::visible(&app, OriginItem::Z), "the Origin folder is hidden");
    app.dialog = Some(d);
    assert!(crate::origin::visible(&app, OriginItem::Z) && crate::origin::visible(&app, OriginItem::Xy));
    let d = Dialog::open(&mut app, Kind::Sweep);
    assert!(d.profiles.is_empty() && d.path.is_none(), "Profile and Path start empty");
    assert!(!d.ok_enabled());
  }

  #[test]
  fn the_analysis_tab_shows_its_analysis() {
    let mut d = Dialog::blank(Kind::Sweep);
    assert_eq!(d.tab_analysis(), None);
    d.solid.rsl.analysis = "zebra";
    assert_eq!(d.tab_analysis(), Some("zebra"));
    let mut e = Dialog::blank(Kind::Extrude);
    e.solid.rsl.analysis = "zebra";
    assert_eq!(e.tab_analysis(), None);
  }
}
