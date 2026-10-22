//! Fusion's material libraries: the PHYSICAL MATERIAL and APPEARANCE palettes (In This Design
//! and the Library's folders as swatch thumbnails that drag onto the model) and Manage
//! Materials' Material Browser window.

use egui::{Color32, Pos2, Rect, RichText, Stroke, vec2};
use fox_engine::Sel;
use fox_engine::model::BodyId;
use fox_engine::model::assembly::{MATERIAL_FOLDERS, MATERIALS, material_folder};
use serde_json::{Value, json};

use crate::App;
use crate::dialogs::{SwatchDrag, apply_appearance, in_this_design};
use crate::theme;

/// Fusion's physical material library.
pub(crate) const MATERIAL_LIBRARY: &str = "Fusion Material Library";
/// Fusion's appearance library.
pub(crate) const APPEARANCE_LIBRARY: &str = "Fusion Appearance Library";
/// Fusion's nonlinear material library (for simulation; FissionOxide has none of its materials).
pub(crate) const NONLINEAR_LIBRARY: &str = "Fusion Nonlinear Material Library";
/// What a body is made of when nothing says otherwise, as in Fusion.
const DEFAULT_MATERIAL: &str = "Steel";

/// One entry of a library: an appearance or a physical material.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Swatch {
  pub name: String,
  pub color: [u8; 3],
  pub folder: String,
}

/// The Appearance tab of the palette's Library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum LibTab {
  #[default]
  Fusion,
  Mine,
  Favorites,
}

impl LibTab {
  const ALL: [LibTab; 3] = [LibTab::Fusion, LibTab::Mine, LibTab::Favorites];

  fn label(self) -> &'static str {
    match self {
      LibTab::Fusion => "Fusion Appearances",
      LibTab::Mine => "My Appearances",
      LibTab::Favorites => "Favorites",
    }
  }
}

/// A palette's own state (Appearance and Physical Material).
#[derive(Debug, Clone)]
pub(crate) struct Palette {
  /// Appearance applies to faces rather than bodies and components.
  pub faces: bool,
  pub search: String,
  pub tab: LibTab,
  /// "Show downloadable materials" (on, as in Fusion).
  pub downloadable: bool,
  /// Appearance names added to Favorites from a swatch's context menu.
  pub favorites: Vec<String>,
}

impl Default for Palette {
  fn default() -> Self {
    Palette { faces: false, search: String::new(), tab: LibTab::Fusion, downloadable: true, favorites: Vec::new() }
  }
}

/// The appearance library with its folders, as the engine knows it.
pub(crate) fn appearances(app: &mut App) -> Vec<Swatch> {
  let Ok(v) = app.s.execute("appearances", &json!({})) else { return Vec::new() };
  let rgb = |c: &Value| -> Option<[u8; 3]> {
    let a = c.as_array()?;
    Some([a.first()?.as_u64()? as u8, a.get(1)?.as_u64()? as u8, a.get(2)?.as_u64()? as u8])
  };
  v.as_array()
    .map(|a| {
      a.iter()
        .filter_map(|x| {
          Some(Swatch {
            name: x.get("name")?.as_str()?.to_string(),
            color: rgb(x.get("color")?)?,
            folder: x.get("folder")?.as_str()?.to_string(),
          })
        })
        .collect()
    })
    .unwrap_or_default()
}

/// The colour of a physical material's default appearance (its thumbnail).
pub(crate) fn material_color(name: &str) -> [u8; 3] {
  match name {
    "Steel" => [205, 200, 190],
    "Stainless Steel" => [192, 194, 198],
    "Aluminum" => [205, 207, 210],
    "Brass" => [204, 170, 84],
    "Copper" => [190, 112, 72],
    "Titanium" => [160, 160, 166],
    "Cast Iron" => [92, 92, 94],
    "ABS Plastic" => [236, 236, 232],
    "PLA" => [228, 228, 222],
    "Nylon" => [240, 236, 224],
    "Polycarbonate" => [218, 224, 230],
    "Acrylic" => [200, 224, 234],
    "Wood" => [180, 132, 82],
    "Rubber" => [36, 36, 36],
    _ => [160, 160, 160],
  }
}

/// Fusion's physical material library: each material with its folder and thumbnail colour.
pub(crate) fn materials() -> Vec<Swatch> {
  MATERIALS
    .iter()
    .map(|(n, _)| Swatch { name: n.to_string(), color: material_color(n), folder: material_folder(n).to_string() })
    .collect()
}

/// The physical materials the design's bodies are made of (how many bodies each), in the order
/// first met: a body's own material, else its component's, else Steel.
pub(crate) fn materials_in_design(app: &App) -> Vec<(Swatch, usize)> {
  let d = &app.s.design;
  let mut out: Vec<(Swatch, usize)> = Vec::new();
  for b in &app.s.model.bodies {
    let m = d
      .body_settings(b.id)
      .and_then(|s| s.material.as_deref())
      .or_else(|| d.component_of(b.id).map(|c| c.material.as_str()).filter(|m| !m.is_empty()))
      .unwrap_or(DEFAULT_MATERIAL);
    match out.iter_mut().find(|x| x.0.name == m) {
      Some(x) => x.1 += 1,
      None => {
        out.push((Swatch { name: m.to_string(), color: material_color(m), folder: material_folder(m).to_string() }, 1))
      }
    }
  }
  out
}

/// Applies a physical material to the components of the bodies in `targets` (the material is the
/// component's), and to the bodies outside a component themselves. Returns how many components
/// and bodies took it, or what is missing.
pub(crate) fn apply_material(app: &mut App, name: &str, targets: &[Sel]) -> Result<usize, String> {
  let mut bodies: Vec<BodyId> = Vec::new();
  for t in targets {
    if let Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } | Sel::Vertex { body, .. } = t
      && !bodies.contains(body)
    {
      bodies.push(*body);
    }
  }
  if bodies.is_empty() {
    return Err("Select bodies first, or drag the material onto a body".into());
  }
  let mut comps: Vec<u32> = Vec::new();
  let mut loose: Vec<u32> = Vec::new();
  for b in &bodies {
    match app.s.design.component_of(*b).map(|c| c.id.0) {
      Some(c) if !comps.contains(&c) => comps.push(c),
      Some(_) => {}
      None => loose.push(b.0),
    }
  }
  for c in &comps {
    if app.run("component.material", json!({"component": c, "material": name})).is_none() {
      return Err(format!("Couldn't apply {name}"));
    }
  }
  if !loose.is_empty() && app.run("body.material", json!({"bodies": loose, "material": name})).is_none() {
    return Err(format!("Couldn't apply {name}"));
  }
  Ok(comps.len() + loose.len())
}

/// The swatches of `lib` in `folder` (any with `None`) whose name contains `search`, ignoring case.
pub(crate) fn matching<'a>(lib: &'a [Swatch], folder: Option<&str>, search: &str) -> Vec<&'a Swatch> {
  let q = search.trim().to_lowercase();
  lib
    .iter()
    .filter(|s| folder.is_none_or(|f| s.folder == f) && (q.is_empty() || s.name.to_lowercase().contains(&q)))
    .collect()
}

// ---------------------------------------------------------------- the palettes

/// The thumbnail size of a palette swatch.
const TILE: f32 = 52.0;

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> Color32 {
  let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
  Color32::from_rgb(m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]))
}

/// A shaded ball in `color`, lit from the top left, as Fusion's rendered thumbnails.
pub(crate) fn ball(p: &egui::Painter, c: Pos2, r: f32, color: [u8; 3]) {
  let dark = color.map(|x| (x as f32 * 0.45) as u8);
  let light = color.map(|x| x + ((255 - x) as f32 * 0.55) as u8);
  const STEPS: usize = 9;
  for k in 0..STEPS {
    let t = k as f32 / STEPS as f32;
    let fill = if t < 0.5 { mix(dark, color, t * 2.0) } else { mix(color, light, (t - 0.5) * 2.0) };
    p.circle_filled(c + vec2(-0.3, -0.3) * r * t, r * (1.0 - 0.8 * t), fill);
  }
}

/// A swatch thumbnail, with its name under it when `caption`. It drags as `drag`, follows the
/// pointer while dragged, and shows `tip` on hover.
fn tile(
  ui: &mut egui::Ui,
  salt: impl std::hash::Hash + std::fmt::Debug,
  s: &Swatch,
  caption: bool,
  tip: &str,
  drag: SwatchDrag,
) -> egui::Response {
  let size = vec2(TILE, if caption { TILE + 26.0 } else { TILE });
  let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
  let resp = ui.interact(rect, ui.id().with(("tile", salt)), egui::Sense::click_and_drag());
  resp.dnd_set_drag_payload(drag);
  let p = ui.painter();
  if resp.hovered() {
    p.rect_filled(rect.expand(2.0), 3.0, theme::HOVER);
  }
  let thumb = Rect::from_min_size(rect.min, vec2(TILE, TILE)).shrink(1.0);
  p.rect(thumb, 2.0, theme::DIALOG_BODY, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  ball(p, thumb.center(), TILE * 0.36, s.color);
  if caption {
    let mut job = egui::text::LayoutJob::single_section(
      s.name.clone(),
      egui::TextFormat::simple(egui::FontId::proportional(10.5), theme::TEXT),
    );
    job.wrap = egui::text::TextWrapping {
      max_width: TILE + 4.0,
      max_rows: 2,
      break_anywhere: false,
      overflow_character: Some('…'),
    };
    job.halign = egui::Align::Center;
    let galley = p.layout_job(job);
    p.galley(egui::pos2(rect.center().x, thumb.max.y + 3.0), galley, theme::TEXT);
  }
  if resp.dragged()
    && let Some(at) = ui.ctx().pointer_latest_pos()
  {
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("swatch-drag"));
    ball(&ui.ctx().layer_painter(layer), at + vec2(16.0, 16.0), 10.0, s.color);
  }
  resp.on_hover_text(tip)
}

/// A collapsible group of a palette (In This Design, Library, a folder).
fn group<R>(ui: &mut egui::Ui, id: &str, title: &str, open: bool, body: impl FnOnce(&mut egui::Ui) -> R) -> Option<R> {
  egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(id).default_open(open).show(ui, body).body_returned
}

/// Tiles in rows that wrap at the palette's width.
fn tile_rows<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
  ui.horizontal_wrapped(|ui| {
    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
    body(ui)
  })
  .inner
}

fn search_box(ui: &mut egui::Ui, text: &mut String, width: f32) {
  ui.add(egui::TextEdit::singleline(text).hint_text("Search").desired_width(width));
}

fn appearance_drag(s: &Swatch, faces: bool) -> SwatchDrag {
  SwatchDrag { name: s.name.clone(), color: s.color, faces, material: false }
}

fn material_drag(s: &Swatch) -> SwatchDrag {
  SwatchDrag { name: s.name.clone(), color: s.color, faces: false, material: true }
}

const APPEARANCE_TIP: &str = "Drag onto a body or face, or click to apply to the selection";
const MATERIAL_TIP: &str = "Drag onto a body or component, or click to apply to the selection";

/// Fusion's APPEARANCE palette: Apply To, the appearances In This Design and the Library (search,
/// tabs, folders). A swatch drags onto the model; a click applies it to the selection.
pub(crate) fn appearance_palette(app: &mut App, ui: &mut egui::Ui, pal: &mut Palette) {
  let lib = appearances(app);
  let named: Vec<(String, [u8; 3])> = lib.iter().map(|s| (s.name.clone(), s.color)).collect();
  let used = in_this_design(app, &named);
  let mut apply: Option<Swatch> = None;
  group(ui, "ap-apply-to", "Apply To", true, |ui| {
    ui.radio_value(&mut pal.faces, false, "Bodies/Components");
    ui.radio_value(&mut pal.faces, true, "Faces");
  });
  let faces = pal.faces;
  group(ui, "ap-used", "In This Design", true, |ui| {
    tile_rows(ui, |ui| {
      for (i, (name, color, n)) in used.iter().enumerate() {
        let s = Swatch { name: name.clone(), color: *color, folder: String::new() };
        if tile(ui, ("used", i), &s, false, &format!("{name} ({n})"), appearance_drag(&s, faces)).clicked() {
          apply = Some(s);
        }
      }
    });
  });
  group(ui, "ap-library", "Library", true, |ui| {
    ui.horizontal(|ui| {
      search_box(ui, &mut pal.search, 220.0);
      ui.add(egui::Button::new("⟳").frame(false)).on_hover_text("Refresh");
    });
    ui.horizontal(|ui| {
      for t in LibTab::ALL {
        if ui.selectable_label(pal.tab == t, t.label()).clicked() {
          pal.tab = t;
        }
      }
    });
    ui.checkbox(&mut pal.downloadable, "Show downloadable materials");
    ui.add_enabled(false, egui::Button::new("Cancel all Downloads")).on_disabled_hover_text("Nothing is downloading");
    let mut tiles = |ui: &mut egui::Ui, list: Vec<&Swatch>, favorites: &mut Vec<String>| {
      tile_rows(ui, |ui| {
        for s in list {
          let r = tile(ui, ("lib", &s.name), s, true, APPEARANCE_TIP, appearance_drag(s, faces));
          if r.clicked() {
            apply = Some(s.clone());
          }
          r.context_menu(|ui| {
            let fav = favorites.contains(&s.name);
            if ui.button(if fav { "Remove from Favorites" } else { "Add to Favorites" }).clicked() {
              toggle_favorite(favorites, &s.name);
              ui.close();
            }
          });
        }
      });
    };
    match pal.tab {
      LibTab::Fusion if !pal.search.trim().is_empty() => {
        tiles(ui, matching(&lib, None, &pal.search), &mut pal.favorites)
      }
      LibTab::Fusion => {
        for f in fox_engine::APPEARANCE_FOLDERS {
          group(ui, &format!("ap-folder-{f}"), f, false, |ui| {
            tiles(ui, matching(&lib, Some(f), ""), &mut pal.favorites)
          });
        }
      }
      LibTab::Mine => {
        ui.label(RichText::new("No appearances").color(theme::MUTED));
      }
      LibTab::Favorites => {
        let favs: Vec<&Swatch> =
          matching(&lib, None, &pal.search).into_iter().filter(|s| pal.favorites.contains(&s.name)).collect();
        if favs.is_empty() {
          ui.label(RichText::new("No favorites").color(theme::MUTED));
        } else {
          tiles(ui, favs, &mut pal.favorites);
        }
      }
    }
  });
  if let Some(s) = apply {
    let sel = app.s.selection.clone();
    if !apply_appearance(app, &s.name, s.color, faces, &sel) {
      let what = if faces { "faces" } else { "bodies" };
      app.status = Some((format!("Select {what} first, or drag the swatch onto the model"), false));
    }
  }
}

/// Adds `name` to the favorites, or takes it out if it's there.
pub(crate) fn toggle_favorite(favorites: &mut Vec<String>, name: &str) {
  if let Some(i) = favorites.iter().position(|f| f == name) {
    favorites.remove(i);
  } else {
    favorites.push(name.to_string());
  }
}

/// Fusion's PHYSICAL MATERIAL palette: the materials In This Design and the Library (search, the
/// library, folders). A material drags onto a body (its component takes it); a click applies it
/// to the selection's components.
pub(crate) fn material_palette(app: &mut App, ui: &mut egui::Ui, pal: &mut Palette) {
  let lib = materials();
  let used = materials_in_design(app);
  let mut apply: Option<String> = None;
  group(ui, "pm-used", "In This Design", true, |ui| {
    tile_rows(ui, |ui| {
      for (i, (s, n)) in used.iter().enumerate() {
        if tile(ui, ("used", i), s, false, &format!("{} ({n})", s.name), material_drag(s)).clicked() {
          apply = Some(s.name.clone());
        }
      }
    });
  });
  group(ui, "pm-library", "Library", true, |ui| {
    search_box(ui, &mut pal.search, 250.0);
    ui.add_space(2.0);
    egui::ComboBox::from_id_salt("pm-library-list").selected_text(MATERIAL_LIBRARY).width(250.0).show_ui(ui, |ui| {
      let _ = ui.selectable_label(true, MATERIAL_LIBRARY);
    });
    let mut tiles = |ui: &mut egui::Ui, list: Vec<&Swatch>| {
      tile_rows(ui, |ui| {
        for s in list {
          if tile(ui, ("lib", &s.name), s, true, MATERIAL_TIP, material_drag(s)).clicked() {
            apply = Some(s.name.clone());
          }
        }
      });
    };
    if pal.search.trim().is_empty() {
      for f in MATERIAL_FOLDERS {
        group(ui, &format!("pm-folder-{f}"), f, false, |ui| tiles(ui, matching(&lib, Some(f), "")));
      }
    } else {
      tiles(ui, matching(&lib, None, &pal.search));
    }
  });
  if let Some(name) = apply {
    let sel = app.s.selection.clone();
    if let Err(e) = apply_material(app, &name, &sel) {
      app.status = Some((e, false));
    }
  }
}

/// A swatch dropped on a body: an appearance goes to the face or body, a physical material to
/// the body's component.
pub(crate) fn drop_swatch(app: &mut App, drag: &SwatchDrag, body: BodyId, face: u32) {
  if drag.material {
    if let Err(e) = apply_material(app, &drag.name, &[Sel::Body { body }]) {
      app.status = Some((e, false));
    }
    return;
  }
  let target = if drag.faces { Sel::Face { body, face } } else { Sel::Body { body } };
  apply_appearance(app, &drag.name, drag.color, drag.faces, &[target]);
}

// ---------------------------------------------------------------- Material Browser

/// The Material Browser's tab: physical materials or appearances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BrowserTab {
  #[default]
  Physical,
  Appearance,
}

/// Where the Material Browser is: Home (the libraries), Favorites, or a library and one of its
/// folders.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum Place {
  #[default]
  Home,
  Favorites,
  Library(&'static str, Option<String>),
}

/// Manage Materials' Material Browser window.
#[derive(Debug, Clone, Default)]
pub(crate) struct Browser {
  pub tab: BrowserTab,
  pub search: String,
  pub at: Place,
  /// Asset names added to Favorites.
  pub favorites: Vec<String>,
}

impl Browser {
  /// Opens on the Physical tab, in the Fusion Material Library.
  pub(crate) fn new() -> Self {
    Browser { at: Place::Library(MATERIAL_LIBRARY, None), ..Default::default() }
  }

  /// Switches tab, going to the tab's library.
  pub(crate) fn set_tab(&mut self, tab: BrowserTab) {
    self.tab = tab;
    self.at = Place::Library(library_of(tab), None);
  }

  /// Goes to `place`; a library switches to its tab.
  pub(crate) fn go(&mut self, place: Place) {
    if let Place::Library(lib, _) = &place {
      if *lib == APPEARANCE_LIBRARY {
        self.tab = BrowserTab::Appearance;
      } else {
        self.tab = BrowserTab::Physical;
      }
    }
    self.at = place;
  }

  /// The breadcrumbs: Home, then the library and folder.
  pub(crate) fn crumbs(&self) -> Vec<(String, Place)> {
    let mut out = vec![("Home".to_string(), Place::Home)];
    match &self.at {
      Place::Home => {}
      Place::Favorites => out.push(("Favorites".into(), Place::Favorites)),
      Place::Library(lib, folder) => {
        out.push((lib.to_string(), Place::Library(lib, None)));
        if let Some(f) = folder {
          out.push((f.clone(), Place::Library(lib, Some(f.clone()))));
        }
      }
    }
    out
  }

  /// The assets listed for the current place and search (a search looks through the whole
  /// library, as Fusion's does).
  pub(crate) fn listed(&self, app: &mut App) -> Vec<Swatch> {
    let lib = match self.tab {
      BrowserTab::Physical => materials(),
      BrowserTab::Appearance => appearances(app),
    };
    let search = self.search.trim();
    match &self.at {
      Place::Home => Vec::new(),
      Place::Favorites => {
        matching(&lib, None, search).into_iter().filter(|s| self.favorites.contains(&s.name)).cloned().collect()
      }
      Place::Library(l, _) if *l == NONLINEAR_LIBRARY => Vec::new(),
      Place::Library(_, folder) => {
        let folder = if search.is_empty() { folder.as_deref() } else { None };
        matching(&lib, folder, search).into_iter().cloned().collect()
      }
    }
  }
}

fn library_of(tab: BrowserTab) -> &'static str {
  match tab {
    BrowserTab::Physical => MATERIAL_LIBRARY,
    BrowserTab::Appearance => APPEARANCE_LIBRARY,
  }
}

/// The folders of a library.
fn folders_of(lib: &str) -> &'static [&'static str] {
  match lib {
    MATERIAL_LIBRARY => MATERIAL_FOLDERS,
    APPEARANCE_LIBRARY => &fox_engine::APPEARANCE_FOLDERS,
    _ => &[],
  }
}

/// MANAGE > Manage Materials: opens the Material Browser.
pub(crate) fn open_browser(app: &mut App) {
  if app.material_browser.is_none() {
    app.material_browser = Some(Browser::new());
  }
}

/// The Material Browser: Physical / Appearance tabs and a search box, the Home and library
/// breadcrumbs, the library tree, the Name list with swatches, and the bottom toolbar.
pub(crate) fn browser_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut b) = app.material_browser.take() else { return };
  let mut open = true;
  egui::Window::new("Material Browser")
    .id(egui::Id::new("material-browser"))
    .open(&mut open)
    .resizable(true)
    .collapsible(false)
    .default_size(vec2(720.0, 460.0))
    .show(ctx, |ui| browser_contents(app, ui, &mut b));
  if open {
    app.material_browser = Some(b);
  }
}

fn browser_contents(app: &mut App, ui: &mut egui::Ui, b: &mut Browser) {
  ui.horizontal(|ui| {
    for (t, label) in [(BrowserTab::Physical, "Physical"), (BrowserTab::Appearance, "Appearance")] {
      if ui.selectable_label(b.tab == t, label).clicked() && b.tab != t {
        b.set_tab(t);
      }
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      search_box(ui, &mut b.search, 200.0);
    });
  });
  ui.separator();
  ui.horizontal(|ui| {
    let crumbs = b.crumbs();
    let last = crumbs.len() - 1;
    for (i, (label, place)) in crumbs.into_iter().enumerate() {
      let text = if i == 0 { format!("🏠 {label}") } else { label };
      if ui.add(egui::Button::new(text).frame(false)).clicked() {
        b.go(place);
      }
      if i < last {
        ui.label(RichText::new("›").color(theme::MUTED));
      }
    }
  });
  ui.separator();
  let listed = b.listed(app);
  let bottom = 30.0;
  let height = (ui.available_height() - bottom).max(120.0);
  ui.horizontal_top(|ui| {
    ui.allocate_ui(vec2(220.0, height), |ui| {
      egui::ScrollArea::vertical().id_salt("mb-tree").show(ui, |ui| {
        ui.set_width(210.0);
        if ui.selectable_label(b.at == Place::Home, "Home").clicked() {
          b.go(Place::Home);
        }
        if ui.selectable_label(b.at == Place::Favorites, "Favorites").clicked() {
          b.go(Place::Favorites);
        }
        for lib in [MATERIAL_LIBRARY, APPEARANCE_LIBRARY, NONLINEAR_LIBRARY] {
          let folders = folders_of(lib);
          let here = matches!(&b.at, Place::Library(l, None) if *l == lib);
          if folders.is_empty() {
            if ui.selectable_label(here, lib).clicked() {
              b.go(Place::Library(lib, None));
            }
            continue;
          }
          let h = egui::CollapsingHeader::new(lib).id_salt(("mb-lib", lib)).default_open(lib == library_of(b.tab));
          let r = h.show(ui, |ui| {
            for f in folders {
              let on = matches!(&b.at, Place::Library(l, Some(x)) if *l == lib && x.as_str() == *f);
              if ui.selectable_label(on, *f).clicked() {
                b.go(Place::Library(lib, Some(f.to_string())));
              }
            }
          });
          if r.header_response.clicked() {
            b.go(Place::Library(lib, None));
          }
        }
      });
    });
    ui.separator();
    ui.allocate_ui(vec2(ui.available_width(), height), |ui| {
      egui::ScrollArea::vertical().id_salt("mb-list").show(ui, |ui| {
        ui.set_width(ui.available_width());
        if b.at == Place::Home && b.search.trim().is_empty() {
          ui.label(RichText::new("Libraries").strong());
          for lib in [MATERIAL_LIBRARY, APPEARANCE_LIBRARY, NONLINEAR_LIBRARY] {
            if ui.selectable_label(false, lib).clicked() {
              b.go(Place::Library(lib, None));
            }
          }
          return;
        }
        egui::Grid::new("mb-names").num_columns(2).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
          ui.label(RichText::new("Name").strong());
          ui.label(RichText::new("Category").strong());
          ui.end_row();
          for s in &listed {
            browser_row(ui, s, b.tab, &mut b.favorites);
            ui.label(RichText::new(&s.folder).color(theme::MUTED));
            ui.end_row();
          }
        });
        if listed.is_empty() {
          ui.label(RichText::new("No materials").color(theme::MUTED));
        }
      });
    });
  });
  ui.separator();
  // the bottom toolbar: library and asset actions (the library files themselves are Fusion's)
  ui.horizontal(|ui| {
    let later = "isn't in FissionOxide yet";
    ui.add_enabled(false, egui::Button::new("📚 ▾")).on_disabled_hover_text(format!("Creating libraries {later}"));
    ui.add_enabled(false, egui::Button::new("＋ ▾")).on_disabled_hover_text(format!("Creating materials {later}"));
    ui.add_enabled(false, egui::Button::new("▤")).on_disabled_hover_text(format!("The asset editor {later}"));
  });
}

/// One asset of the Name list: a swatch and its name; it drags onto the model like a palette
/// swatch, and its context menu adds it to Favorites.
fn browser_row(ui: &mut egui::Ui, s: &Swatch, tab: BrowserTab, favorites: &mut Vec<String>) {
  let drag = match tab {
    BrowserTab::Physical => material_drag(s),
    BrowserTab::Appearance => appearance_drag(s, false),
  };
  let size = vec2(260.0, 22.0);
  let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
  let resp = ui.interact(rect, ui.id().with(("mb-row", &s.name)), egui::Sense::click_and_drag());
  resp.dnd_set_drag_payload(drag);
  let p = ui.painter();
  if resp.hovered() {
    p.rect_filled(rect, 2.0, theme::HOVER);
  }
  ball(p, egui::pos2(rect.min.x + 11.0, rect.center().y), 9.0, s.color);
  p.text(
    egui::pos2(rect.min.x + 26.0, rect.center().y),
    egui::Align2::LEFT_CENTER,
    &s.name,
    egui::FontId::proportional(12.5),
    theme::TEXT,
  );
  resp.context_menu(|ui| {
    let fav = favorites.contains(&s.name);
    if ui.button(if fav { "Remove from Favorites" } else { "Add to Favorites" }).clicked() {
      toggle_favorite(favorites, &s.name);
      ui.close();
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::dialogs::{Dialog, Kind};
  use fox_engine::Session;

  fn frame(app: &mut App, mut d: Option<&mut Dialog>) -> bool {
    let ctx = egui::Context::default();
    let raw =
      egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))), ..Default::default() };
    let mut keep = true;
    ctx
      .run_ui(raw, |ui| {
        if let Some(d) = d.as_mut() {
          keep = d.show(app, ui.ctx());
        }
        browser_window(app, ui.ctx());
      })
      .drop_without_applying_deltas();
    keep
  }

  /// Two boxes, the second in a component.
  fn app() -> (App, BodyId, BodyId, u32) {
    let mut app = App::with_session(Session::default());
    app.s.execute("box", &json!({"width": 10, "depth": 20, "height": 30})).unwrap();
    app.s.execute("box", &json!({"width": 10, "depth": 10, "height": 10, "at": [50, 50]})).unwrap();
    let (a, b) = (app.s.model.bodies[0].id, app.s.model.bodies[1].id);
    app.s.execute("component.create", &json!({"name": "Part", "bodies": [b.0]})).unwrap();
    let comp = app.s.design.component_of(b).unwrap().id.0;
    (app, a, b, comp)
  }

  #[test]
  fn physical_material_is_a_palette_of_folders_applied_by_drag_or_click() {
    let (mut app, loose, part, comp) = app();
    let mut d = Dialog::open(&mut app, Kind::Material);
    assert!(d.kind.palette() && d.kind.reports(), "Close, no OK");
    assert_eq!(d.prompt(false), None, "no cursor prompt: materials drag onto the model");
    assert!(frame(&mut app, Some(&mut d)));

    // the library sits in Fusion's folders
    let lib = materials();
    assert_eq!(matching(&lib, Some("Metal"), "").len(), 7);
    assert!(matching(&lib, Some("Ceramic"), "").is_empty());
    assert_eq!(
      matching(&lib, None, "steel").iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
      ["Steel", "Stainless Steel"]
    );
    // everything starts as Steel
    let used = materials_in_design(&app);
    assert_eq!(used.len(), 1);
    assert_eq!((used[0].0.name.as_str(), used[0].1), ("Steel", 2));

    // a drop on a body gives its component the material
    let drag = SwatchDrag { name: "Aluminum".into(), color: [0; 3], faces: false, material: true };
    drop_swatch(&mut app, &drag, part, 0);
    assert_eq!(app.s.design.component(fox_engine::model::ComponentId(comp)).unwrap().material, "Aluminum");
    let used = materials_in_design(&app);
    assert_eq!(used.iter().map(|(s, n)| (s.name.as_str(), *n)).collect::<Vec<_>>(), [("Steel", 1), ("Aluminum", 1)]);
    // a body outside a component takes its own
    drop_swatch(&mut app, &drag, loose, 0);
    assert_eq!(app.s.design.body_settings(loose).and_then(|s| s.material.as_deref()), Some("Aluminum"));
    assert_eq!(
      materials_in_design(&app).iter().map(|(s, n)| (s.name.as_str(), *n)).collect::<Vec<_>>(),
      [("Aluminum", 2)]
    );
    // a click applies to the selection: the component and the loose body
    assert!(apply_material(&mut app, "Brass", &[]).is_err());
    assert_eq!(apply_material(&mut app, "Brass", &[Sel::Body { body: part }, Sel::Body { body: loose }]), Ok(2));
    assert_eq!(app.s.design.component_of(part).unwrap().material, "Brass");
    assert_eq!(app.s.design.body_settings(loose).and_then(|s| s.material.as_deref()), Some("Brass"));

    // searching lists matches across folders
    d.palette_mut().search = "ny".into();
    assert!(frame(&mut app, Some(&mut d)));
  }

  #[test]
  fn appearance_library_has_tabs_folders_search_and_favorites() {
    let (mut app, _, part, _) = app();
    let mut d = Dialog::open(&mut app, Kind::Form(&crate::forms::APPEARANCE));
    assert!(d.kind.palette());
    assert!(d.palette_mut().downloadable, "Show downloadable materials starts checked");
    assert!(frame(&mut app, Some(&mut d)));
    let lib = appearances(&mut app);
    assert!(lib.iter().all(|s| fox_engine::APPEARANCE_FOLDERS.contains(&s.folder.as_str())));
    assert_eq!(matching(&lib, Some("Metal"), "anodized").len(), 3);
    for tab in LibTab::ALL {
      d.palette_mut().tab = tab;
      assert!(frame(&mut app, Some(&mut d)));
    }
    let mut favs = Vec::new();
    toggle_favorite(&mut favs, "Copper");
    assert_eq!(favs, ["Copper"]);
    d.palette_mut().favorites = favs.clone();
    assert!(frame(&mut app, Some(&mut d)));
    toggle_favorite(&mut favs, "Copper");
    assert!(favs.is_empty());
    // a dropped appearance swatch still colours the body
    let copper = lib.iter().find(|s| s.name == "Copper").unwrap();
    drop_swatch(&mut app, &appearance_drag(copper, false), part, 0);
    assert_eq!(app.s.design.body_color(part), Some(copper.color));
  }

  #[test]
  fn manage_materials_opens_the_material_browser() {
    let (mut app, ..) = app();
    crate::panels::Cmd::ManageMaterials.run(&mut app);
    let b = app.material_browser.clone().expect("the browser opens");
    assert_eq!((b.tab, b.at.clone()), (BrowserTab::Physical, Place::Library(MATERIAL_LIBRARY, None)));
    assert_eq!(b.listed(&mut app).len(), MATERIALS.len());
    frame(&mut app, None);
    assert!(app.material_browser.is_some(), "it stays open");

    let mut b = Browser::new();
    b.go(Place::Library(MATERIAL_LIBRARY, Some("Plastic".into())));
    assert_eq!(b.crumbs().iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["Home", MATERIAL_LIBRARY, "Plastic"]);
    assert!(b.listed(&mut app).iter().all(|s| s.folder == "Plastic"));
    // a search looks through the whole library
    b.search = "steel".into();
    assert_eq!(b.listed(&mut app).len(), 2);
    b.search.clear();
    // the Appearance tab lists the appearance library
    b.set_tab(BrowserTab::Appearance);
    assert_eq!(b.at, Place::Library(APPEARANCE_LIBRARY, None));
    assert_eq!(b.listed(&mut app).len(), appearances(&mut app).len());
    // picking a library in the tree switches the tab to match
    b.go(Place::Library(MATERIAL_LIBRARY, None));
    assert_eq!(b.tab, BrowserTab::Physical);
    b.go(Place::Home);
    assert!(b.listed(&mut app).is_empty());
    b.favorites.push("Brass".into());
    b.go(Place::Favorites);
    assert_eq!(b.listed(&mut app).iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Brass"]);
    for at in [Place::Home, Place::Favorites, Place::Library(NONLINEAR_LIBRARY, None)] {
      b.go(at);
      app.material_browser = Some(b.clone());
      frame(&mut app, None);
    }
  }
}
