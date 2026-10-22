//! A ribbon button's tooltip, as in Fusion: a dark grey rounded panel under the button, its left
//! edge at the button's, holding a white bold title with the shortcut in parentheses ("Extrude
//! (E)") and the description and usage paragraphs in light grey. After resting on the button for
//! [`EXPAND_SECS`] the panel grows downward with a large illustration in a light grey frame and
//! "Press Ctrl+/ for more help." under it; a command without an illustration shows that line at
//! once and never grows. A few commands (Bill of Materials) show only their name in a small pill.

use egui::{Rect, RichText, Stroke, Vec2, vec2};

use crate::icons::{self, Icon, Tint};
use crate::theme;
use crate::tips::{self, Scope};

/// Resting this long on a command expands its tooltip.
pub(crate) const EXPAND_SECS: f64 = 2.0;
/// The panel's width, as Fusion's.
pub(crate) const WIDTH: f32 = 300.0;
/// The height of the expanded tooltip's illustration, as Fusion's.
pub(crate) const ILLUSTRATION_H: f32 = 190.0;

/// The tooltip's title: the command's name, with its shortcut in parentheses when it has one.
pub(crate) fn title(label: &str, key: &str) -> String {
  // a menu entry's "..." isn't part of the title ("Scripts and Add-Ins (Shift+S)")
  let label = label.strip_suffix("...").unwrap_or(label);
  if key.is_empty() { label.to_string() } else { format!("{label} ({key})") }
}

/// How long the pointer has rested on `resp`, in seconds (0 when it isn't on it). Call it every
/// frame the widget is drawn: a gap means the pointer left and came back.
pub(crate) fn rested(resp: &egui::Response) -> f64 {
  let now = resp.ctx.input(|i| i.time);
  if !resp.hovered() {
    return 0.0;
  }
  // (hovered since, last seen hovered)
  let id = resp.id.with("tip-rest");
  let since = resp.ctx.data_mut(|d| {
    let (since, last) = d.get_temp::<(f64, f64)>(id).unwrap_or((now, now));
    let since = if now - last > 0.25 { now } else { since };
    d.insert_temp(id, (since, now));
    since
  });
  now - since
}

/// Shows the tooltip of the ribbon button `resp` for the command `label` on the `scope` toolbar,
/// its texts looked up under `lookup` (the label, or a tab's own version of it such as
/// [`tips::SURFACE_EXTRUDE`]); `missing` adds "Not in FissionOxide yet", `key` is its shortcut
/// (empty for none).
pub(crate) fn show(
  resp: egui::Response,
  scope: Scope,
  label: &'static str,
  lookup: &str,
  icon: Icon,
  tint: Tint,
  missing: bool,
  key: &str,
) -> egui::Response {
  let tip = tips::tip(scope, lookup);
  let illustrated = tips::illustrated(scope, lookup);
  let ctx = resp.ctx.clone();
  let rested = rested(&resp);
  let style = ctx.global_style();
  let frame = egui::Frame::new()
    .fill(theme::TOOLTIP)
    .corner_radius(egui::CornerRadius::same(4))
    .inner_margin(egui::Margin::symmetric(10, 8))
    .shadow(style.visuals.popup_shadow);
  let mut tooltip = egui::Tooltip::for_enabled(&resp);
  tooltip.popup = tooltip.popup.anchor(resp.rect).align(egui::RectAlign::BOTTOM_START).gap(4.0);
  if tips::title_only(label) {
    // a small pill holding the bare title, as Fusion's Bill of Materials
    let pill = frame.inner_margin(egui::Margin::symmetric(8, 4));
    tooltip.popup = tooltip.popup.frame(pill);
    tooltip.show(|ui| {
      let font = egui::TextStyle::Body.resolve(ui.style());
      ui.add(egui::Label::new(RichText::new(title(label, key)).font(font).color(theme::TOOLTIP_TITLE)).extend());
    });
    return resp;
  }
  tooltip.popup = tooltip.popup.width(WIDTH).frame(frame);
  tooltip.show(|ui| {
    ui.set_max_width(WIDTH);
    ui.spacing_mut().item_spacing.y = 10.0;
    let title = title(label, key);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let r = ui.label(RichText::new(&title).font(font.clone()).color(theme::TOOLTIP_TITLE)).rect;
    // the font has no bold face: a second pass half a pixel over gives the weight
    ui.painter().text(r.left_top() + vec2(0.6, 0.0), egui::Align2::LEFT_TOP, &title, font, theme::TOOLTIP_TITLE);
    let text = |ui: &mut egui::Ui, s: &str, c| ui.add(egui::Label::new(RichText::new(s).color(c)).wrap());
    if let Some(t) = tip {
      // a usage text ending in the help line has it drawn as the panel's last line below
      for p in t.paragraphs().filter(|p| *p != tips::MORE_HELP) {
        text(ui, p, theme::TOOLTIP_TEXT);
      }
    }
    if missing {
      text(ui, "Not in FissionOxide yet", theme::TOOLTIP_MUTED);
    }
    if tip.is_none() {
      return;
    }
    if illustrated && rested < EXPAND_SECS {
      ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(EXPAND_SECS - rested));
      return;
    }
    if illustrated {
      let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), ILLUSTRATION_H), egui::Sense::hover());
      ui.painter().rect(
        r,
        2.0,
        theme::ILLUSTRATION,
        Stroke::new(1.0, theme::ILLUSTRATION_FRAME),
        egui::StrokeKind::Inside,
      );
      // until pictures of each command's result exist, its icon painted large
      let side = ILLUSTRATION_H - 40.0;
      icons::draw_tinted(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(side)), icon, tint);
    }
    text(ui, tips::MORE_HELP, theme::TOOLTIP_TEXT);
  });
  resp
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The SOLID tab's buttons carry Fusion's texts, word for word.
  #[test]
  fn solid_tab_tooltips_are_fusions_texts() {
    let tip = |l| tips::tip(Scope::Design, l).unwrap_or_else(|| panic!("{l} has a tooltip"));
    let extrude = tip("Extrude");
    assert_eq!(extrude.line, "Adds depth to open or closed sketch profiles or faces.");
    assert_eq!(
      extrude.more,
      "Select an extrusion type, select sketch profiles or faces to extrude, then specify the distance and taper \
       angle. For the Thin Extrude type, specify the wall thickness."
    );
    assert_eq!(tip("Revolve").line, "Revolves a sketch profile or planar face around a selected axis.");
    assert_eq!(tip("Hole").line, "Creates a hole based on user-specified values and selections.");
    assert_eq!(tip("Measure").line, "Measures the distance, angle, area, or position of selected objects.");
    assert_eq!(tip("Canvas").more, "Select a face then select an image to import.");
    // one paragraph only
    for l in ["Press Pull", "Display Configuration Table"] {
      assert_eq!(tip(l).paragraphs().count(), 1, "{l}");
    }
    // Insert Mesh has two usage paragraphs
    let mesh: Vec<_> = tip("Insert Mesh").paragraphs().collect();
    assert_eq!(mesh.len(), 3, "{mesh:?}");
    assert!(mesh[2].starts_with("For STL or OBJ files, select the unit type"), "{mesh:?}");
  }

  /// `ui:plastic:tooltip-texts`: the PLASTIC and MODIFY buttons carry Fusion's texts; Bill of
  /// Materials is the bare title, never expanding.
  #[test]
  fn plastic_and_modify_tooltips_are_fusions_texts() {
    let tip = |l| tips::tip(Scope::Design, l).unwrap_or_else(|| panic!("{l} has a tooltip"));
    let thin = "Creates a thin feature from an open sketch profile, extruded to the nearest faces on a solid body.";
    let web = tip("Web");
    assert_eq!(
      web.line,
      format!("{thin} The web feature is extruded in a direction perpendicular to the sketch plane.")
    );
    assert_eq!(web.more, "Select an open sketch profile, then specify the thickness.");
    let rib = tip("Rib");
    assert_eq!(rib.line, format!("{thin} The rib feature is extruded in a direction parallel to the sketch plane."));
    assert_eq!(rib.more, web.more);
    assert!(tip("Create Form").line.starts_with("Enters Form mode and inserts a form feature into the timeline."));
    assert!(tip("Create Form").line.ends_with("This is commonly referred to as T-Spline modeling."));
    assert_eq!(tip("Create Form").more, "Select Finish Form to exit out of the mode.");
    assert_eq!(
      tip("Split Body").line,
      "Creates new bodies by dividing selected bodies using a profile, face, or plane."
    );
    assert_eq!(
      tip("Split Body").more,
      "Select the bodies to modify then select the profile, face, or plane that divides the bodies."
    );
    assert_eq!(
      tip("Move/Copy").line,
      "Moves the selected face, body, sketch, or construction geometry a specified distance or angle."
    );
    assert_eq!(
      tip("Move/Copy").more,
      "Select the objects to modify then specify the distance or angle. Use Set Pivot to reposition the manipulator."
    );
    for l in ["Web", "Rib", "Split Body", "Move/Copy"] {
      assert!(tips::illustrated(Scope::Design, l), "{l}");
    }
    assert!(tips::title_only("Bill of Materials"));
    assert!(tips::tip(Scope::Design, "Bill of Materials").is_none());
    assert!(!tips::illustrated(Scope::Design, "Bill of Materials"));
  }

  /// `ui:utilities:tooltip-texts`: the UTILITIES buttons carry Fusion's texts, and the ones
  /// Fusion shows without a picture don't grow.
  #[test]
  fn utilities_tooltips_are_fusions_texts() {
    let tip = |l| tips::tip(Scope::Design, l).unwrap_or_else(|| panic!("{l} has a tooltip"));
    let scripts: Vec<_> = tip("Scripts and Add-Ins...").paragraphs().collect();
    assert_eq!(
      scripts,
      [
        "Manage Scripts and Add-Ins",
        "Displays the Scripts and Add-Ins dialog box. Create, edit, run, stop, debug and manage Scripts and Add-Ins."
      ]
    );
    assert_eq!(
      tip("Manage Materials").line,
      "Opens the Material Browser, where you can manage physical materials and appearances in user-defined material \
       libraries."
    );
    assert!(tip("Manage Materials").more.starts_with("Navigate between libraries, create new libraries,"));
    assert_eq!(tip("Interference").line, "Reports the interference between selected solid bodies or components.");
    assert_eq!(
      tip("Interference").more,
      "Select the components then click compute. Choose to create components from the interferences."
    );
    assert!(tip("Draft Analysis").line.starts_with("Displays a color gradient on the faces of selected bodies"));
    assert_eq!(
      tip("Accessibility Analysis").more,
      "Inaccessible areas are highlighted in red. Useful for checking undercut areas through a plane."
    );
    assert!(tip("Display Component Colors").more.starts_with("Press Shift+N to show or hide component colors"));
    assert_eq!(tip("Sustainability Insights").line, "Displays an estimated carbon footprint of the current design.");
    for l in ["Scripts and Add-Ins...", "Compute All", "Manage Materials", "Interference", "Draft Analysis"] {
      assert!(!tips::illustrated(Scope::Design, l), "{l}");
    }
    for l in ["Accessibility Analysis", "Display Component Colors", "Sustainability Insights"] {
      assert!(tips::illustrated(Scope::Design, l), "{l}");
    }
  }

  /// The title carries the shortcut in parentheses; commands without one show the bare name.
  #[test]
  fn the_title_carries_the_shortcut() {
    assert_eq!(title("Extrude", "E"), "Extrude (E)");
    assert_eq!(title("Revolve", ""), "Revolve");
    assert_eq!(title("Scripts and Add-Ins...", "Shift+S"), "Scripts and Add-Ins (Shift+S)");
  }

  /// Fusion has no illustration for these three: they show the help line at once.
  #[test]
  fn which_commands_have_an_illustration() {
    for l in ["Display Configuration Table", "Offset Plane", "Add To Assembly"] {
      assert!(!tips::illustrated(Scope::Design, l), "{l}");
    }
    for l in ["Create Sketch", "Extrude", "Fillet", "Measure", "Insert Mesh"] {
      assert!(tips::illustrated(Scope::Design, l), "{l}");
    }
    // sketch commands whose text already ends with the help line don't grow either
    assert!(!tips::illustrated(Scope::Sketch, "Trim"));
  }
}
