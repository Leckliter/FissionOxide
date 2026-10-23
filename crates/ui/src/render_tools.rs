//! The Render workspace's docked tool dialogs beside Scene Settings and New Light: TEXTURE MAP
//! CONTROLS (SETUP), DIELECTRIC PRIORITY CONTROL (OPTICAL OVERRIDES) and IN-CANVAS RENDER
//! SETTINGS (IN-CANVAS RENDER), laid out as Fusion's command dialogs: a header, the inputs, and
//! OK / Cancel under a rule.

use egui::{Rect, Stroke, pos2, vec2};
use fox_engine::Sel;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::BodyId;
use fox_engine::rendering::raytrace::{Axis, Projection, TextureSpec};
use fox_engine::scene::{Pick, PickTable};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::render_panels::footer;
use crate::{App, dialogs, panels, theme};

/// A docked dialog: its header, `body`, then Cancel and OK (greyed, saying why, while `blocked`).
/// Returns (OK pressed, Cancel pressed).
fn docked(
  ctx: &egui::Context,
  id: &str,
  icon: Icon,
  title: &str,
  blocked: Option<&str>,
  canvas: Rect,
  body: impl FnOnce(&mut egui::Ui),
) -> (bool, bool) {
  let width = 320.0;
  let accent = icons::accent_of(icon);
  let mut out = (false, false);
  egui::Area::new(egui::Id::new(id))
    .pivot(egui::Align2::RIGHT_TOP)
    .default_pos(panels::dialog_dock(canvas, false))
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, icon, title, width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
          body(ui);
        });
        out = footer(ui, width, |ui| {
          let cancel = panels::secondary_button(ui, "Cancel").clicked();
          let resp = dialogs::ok_button(ui, accent, blocked.is_none());
          match blocked {
            None => (resp.clicked(), cancel),
            Some(why) => {
              resp.on_disabled_hover_text(why);
              (false, cancel)
            }
          }
        });
      });
    });
  out
}

/// The bodies of the selection: picked bodies, and the bodies of picked faces.
pub(crate) fn selected_bodies(app: &App) -> Vec<BodyId> {
  let mut out = Vec::new();
  for s in &app.s.selection {
    let b = match s {
      Sel::Body { body } | Sel::Face { body, .. } => *body,
      _ => continue,
    };
    if !out.contains(&b) {
      out.push(b);
    }
  }
  out
}

/// Opens one of the workspace's docked dialogs in place of the others (and of a command
/// dialog), as Fusion shows one at a time.
pub(crate) fn close_docked(app: &mut App) {
  app.cancel_dialog();
  let r = &mut app.render;
  r.scene_open = false;
  r.light = None;
  r.texture_map = None;
  r.priority = None;
  r.in_canvas_settings = None;
}

// ---------------------------------------------------------------- texture map controls

/// Fusion's projections, in its drop-down's order.
pub(crate) const PROJECTIONS: [Projection; 5] =
  [Projection::Planar, Projection::Box, Projection::Spherical, Projection::Cylindrical, Projection::Automatic];

/// The TEXTURE MAP CONTROLS dialog's values: the body (or face) they were read from and its
/// mapping.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TextureMapForm {
  pub body: Option<BodyId>,
  /// The picked face, when the texture mapped is its own material's (not its body's).
  pub face: Option<u32>,
  /// The mapping being edited (`None`: the appearance has no texture).
  pub spec: Option<TextureSpec>,
  /// What was picked when the values were read: the body, and the face if one was picked.
  picked: Option<(BodyId, Option<u32>)>,
  /// The canvas grip being dragged.
  pub(crate) drag: Option<MapDrag>,
}

impl TextureMapForm {
  /// Reads the mapping of the first picked body or face: a face with a textured material of its
  /// own maps that, any other face its body's texture.
  fn sync(&mut self, app: &App) {
    let picked = app.s.selection.iter().find_map(|s| match *s {
      Sel::Body { body } => Some((body, None)),
      Sel::Face { body, face } => Some((body, Some(face))),
      _ => None,
    });
    if picked == self.picked {
      return;
    }
    self.picked = picked;
    self.drag = None;
    self.body = picked.map(|p| p.0);
    (self.face, self.spec) = match picked {
      None => (None, None),
      Some((b, None)) => (None, fox_engine::rendering::body_texture(&app.s, b)),
      Some((b, Some(f))) => match fox_engine::rendering::face_texture(&app.s, b, f) {
        Some((t, own)) => (own.then_some(f), Some(t)),
        None => (None, None),
      },
    };
  }
}

/// Opens the docked TEXTURE MAP CONTROLS on the selected body (a body menu's Texture Map
/// Controls, in any workspace), closing the other docked dialogs.
pub(crate) fn open_texture_map(app: &mut App) {
  let sel = app.s.selection.clone();
  close_docked(app);
  app.s.selection = sel;
  app.render.texture_map = Some(TextureMapForm::default());
}

/// The docked TEXTURE MAP CONTROLS dialog: Object, Projection, Axis, Scale, Rotation, Offset and
/// Reset; OK changes the body's texture mapping.
pub(crate) fn texture_map_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.render.texture_map.clone() else { return };
  form.sync(app);
  let accent = icons::accent_of(Icon::TextureMap);
  let has = form.body.is_some();
  let blocked = match (has, form.spec.is_some()) {
    (false, _) => Some("Select a body or face"),
    (true, false) => Some("The selected appearance has no texture to map"),
    (true, true) => None,
  };
  let mut reset = false;
  let (ok, cancel) = docked(ctx, "texture-map", Icon::TextureMap, "TEXTURE MAP CONTROLS", blocked, app.canvas, |ui| {
    egui::Grid::new("texture-map-grid").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
      ui.label("Object");
      dialogs::sel_chip(ui, accent, &dialogs::chip_text(has as usize, "selected"), has);
      ui.end_row();
      let Some(t) = &mut form.spec else {
        if has {
          ui.label("");
          ui.label(egui::RichText::new("No texture").color(theme::MUTED));
          ui.end_row();
        }
        return;
      };
      ui.label("Projection");
      egui::ComboBox::from_id_salt("texture-map-proj").selected_text(t.projection.label()).width(150.0).show_ui(
        ui,
        |ui| {
          for p in PROJECTIONS {
            ui.selectable_value(&mut t.projection, p, p.label());
          }
        },
      );
      ui.end_row();
      if !matches!(t.projection, Projection::Box | Projection::Automatic) {
        ui.label("Axis");
        ui.horizontal(|ui| {
          for (a, n) in [(Axis::X, "X"), (Axis::Y, "Y"), (Axis::Z, "Z")] {
            ui.selectable_value(&mut t.axis, a, n);
          }
        });
        ui.end_row();
      }
      ui.label("Scale");
      ui.add(egui::DragValue::new(&mut t.scale).range(0.1..=10000.0).speed(0.5).suffix(" mm"));
      ui.end_row();
      ui.label("Rotation");
      ui.add(egui::DragValue::new(&mut t.rotation).range(-360.0..=360.0).speed(1.0).suffix(" deg"));
      ui.end_row();
      ui.label("Offset");
      ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut t.offset[0]).speed(0.01).prefix("U "));
        ui.add(egui::DragValue::new(&mut t.offset[1]).speed(0.01).prefix("V "));
      });
      ui.end_row();
      ui.label("");
      reset = ui.button("Reset").on_hover_text("Puts the texture back where the appearance had it").clicked();
      ui.end_row();
    });
  });
  if reset && let Some(t) = &mut form.spec {
    let name = match (form.body, form.face) {
      (Some(b), Some(f)) => fox_engine::rendering::face_material(&app.s, b, f).map(|m| m.name).unwrap_or_default(),
      (Some(b), None) => fox_engine::rendering::body_material(&app.s, b).name,
      _ => String::new(),
    };
    let lib = fox_engine::rendering::raytrace::material::find(&name).and_then(|m| m.texture).unwrap_or_default();
    (t.projection, t.axis, t.scale, t.rotation, t.offset) =
      (lib.projection, lib.axis, lib.scale, lib.rotation, lib.offset);
  }
  if ok && let (Some(b), Some(t)) = (form.body, &form.spec) {
    let p = json!({
      "body": b.0, "face": form.face, "projection": t.projection.label().to_ascii_lowercase(),
      "axis": format!("{:?}", t.axis).to_ascii_lowercase(), "scale": t.scale, "rotation": t.rotation,
      "offset": t.offset,
    });
    if app.run("render.texture_map", p).is_some() {
      app.render.texture_map = None;
      return;
    }
  }
  app.render.texture_map = (!cancel).then_some(form);
}

// ---------------------------------------------------------------- texture map handles

/// A grip of Texture Map Controls' canvas manipulator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grip {
  /// Slides the texture: across its plane (planar and box), along the axis (cylindrical).
  Move,
  /// Turns it: in its plane, or round the axis (cylindrical, spherical).
  Turn,
  /// Sizes it (one repeat's length).
  Scale,
}

/// A grip being dragged: the mapping and where the pointer was when it started.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MapDrag {
  grip: Grip,
  spec: TextureSpec,
  /// The pointer on the grip's plane (Move across a plane), or its distance along the grip's line
  /// or angle round its ring.
  at: DVec3,
  t: f64,
}

/// Where the manipulator sits: the projection's centre (the body's box centre, as the viewport
/// and renders centre it), the texture's plane (`u`, `v`, with `n = u × v`; for cylindrical and
/// spherical the plane across the axis, `n` the axis) and the ring's radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MapFrame {
  pub origin: DVec3,
  pub u: DVec3,
  pub v: DVec3,
  pub n: DVec3,
  pub radius: f64,
  /// Wrapped round an axis (cylindrical, spherical) rather than laid across a plane.
  pub around: bool,
}

/// The two world axes a planar projection across axis `a` maps to u and v (as
/// `fox_raytrace::texture::project`'s `planar`), and those round axis `a` (its `around`).
fn plane_axes(a: usize, around: bool) -> (DVec3, DVec3) {
  match (a, around) {
    (0, _) => (DVec3::Y, DVec3::Z),
    (1, false) => (DVec3::X, DVec3::Z),
    (1, true) => (DVec3::Z, DVec3::X),
    _ => (DVec3::X, DVec3::Y),
  }
}

/// The manipulator's frame for `spec` on `body`. A box projection shows the plane of the box
/// side that faces the camera most (`toward` points at the camera).
pub(crate) fn map_frame(s: &fox_engine::Session, body: BodyId, spec: &TextureSpec, toward: DVec3) -> Option<MapFrame> {
  let m = s.model.body(body)?.mesh()?;
  let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
  for p in &m.positions {
    let p = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
    lo = lo.min(p);
    hi = hi.max(p);
  }
  if lo.x > hi.x {
    return None;
  }
  // the scene centres the projection on the box's middle in single precision
  let origin = ((lo + hi) / 2.0).as_vec3().as_dvec3();
  let radius = ((hi - lo).max_element() * 0.4).max(1.0);
  let around = matches!(spec.projection, Projection::Cylindrical | Projection::Spherical);
  let a = match spec.projection {
    Projection::Box | Projection::Automatic => {
      let t = toward.abs();
      if t.x >= t.y && t.x >= t.z {
        0
      } else if t.y >= t.z {
        1
      } else {
        2
      }
    }
    _ => spec.axis.index(),
  };
  let (u, v) = plane_axes(a, around);
  Some(MapFrame { origin, u, v, n: u.cross(v), radius, around })
}

/// `v` turned by the texture's rotation (degrees), as `project` turns its coordinates.
fn turned(rotation: f32, v: [f64; 2]) -> [f64; 2] {
  let (s, c) = (rotation as f64).to_radians().sin_cos();
  [v[0] * c - v[1] * s, v[0] * s + v[1] * c]
}

/// The texture's u direction on the canvas: its plane's u turned the other way by the rotation
/// (the texture turns opposite to its coordinates).
fn u_dir(f: &MapFrame, spec: &TextureSpec) -> DVec3 {
  let a = -(spec.rotation as f64).to_radians();
  f.u * a.cos() + f.v * a.sin()
}

/// The grips' heads (world): Move, Turn (its knob on the ring), Scale; `None` where the
/// projection has no such grip (a sphere is only turned).
fn grip_points(f: &MapFrame, spec: &TextureSpec) -> [Option<DVec3>; 3] {
  let o = f.origin;
  if f.around {
    let turn = -std::f64::consts::TAU * spec.offset[0] as f64;
    let knob = o + (f.u * turn.cos() + f.v * turn.sin()) * f.radius;
    let cyl = spec.projection == Projection::Cylindrical;
    return [cyl.then(|| o + f.n * f.radius * 1.2), Some(knob), cyl.then(|| o - f.n * f.radius * 0.8)];
  }
  let ud = u_dir(f, spec);
  let vd = f.n.cross(ud);
  [Some(o), Some(o + vd * f.radius), Some(o + ud * f.radius * 0.6)]
}

/// The mapping a drag gives with the pointer's ray at (`ro`, `rd`).
fn drag_to(f: &MapFrame, d: &MapDrag, ro: DVec3, rd: DVec3) -> Option<TextureSpec> {
  use crate::manipulators::{axis_param, ring_angle};
  let mut t = d.spec.clone();
  let scale = d.spec.scale.max(1e-3) as f64;
  let shift = |t: &mut TextureSpec, by: [f64; 2]| {
    let r = turned(d.spec.rotation, by);
    t.offset = [d.spec.offset[0] - r[0] as f32, d.spec.offset[1] - r[1] as f32];
  };
  match (d.grip, f.around) {
    (Grip::Move, false) => {
      let w = fox_engine::render::ray_plane(ro, rd, f.origin, f.n)? - d.at;
      shift(&mut t, [w.dot(f.u) / scale, w.dot(f.v) / scale]);
    }
    (Grip::Move, true) => {
      let h = axis_param(f.origin, f.n, ro, rd)? - d.t;
      shift(&mut t, [0.0, h / scale]);
    }
    (Grip::Turn, around) => {
      let a = crate::manipulators::wrap(ring_angle(f.origin, f.n, f.u, ro, rd)? - d.t);
      if around {
        shift(&mut t, [a / std::f64::consts::TAU, 0.0]);
      } else {
        let r = (d.spec.rotation as f64 - a.to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
        t.rotation = r as f32;
      }
    }
    (Grip::Scale, around) => {
      let dir = if around { -f.n } else { u_dir(f, &d.spec) };
      let now = axis_param(f.origin, dir, ro, rd)?;
      if d.t.abs() < 1e-9 || now <= 0.0 {
        return None;
      }
      t.scale = (d.spec.scale as f64 * now / d.t).clamp(0.1, 10000.0) as f32;
    }
  }
  Some(t)
}

/// Starts dragging `grip` with the pointer's ray at (`ro`, `rd`).
fn begin_drag(f: &MapFrame, spec: &TextureSpec, grip: Grip, ro: DVec3, rd: DVec3) -> Option<MapDrag> {
  use crate::manipulators::{axis_param, ring_angle};
  let (at, t) = match (grip, f.around) {
    (Grip::Move, false) => (fox_engine::render::ray_plane(ro, rd, f.origin, f.n)?, 0.0),
    (Grip::Move, true) => (DVec3::ZERO, axis_param(f.origin, f.n, ro, rd)?),
    (Grip::Turn, _) => (DVec3::ZERO, ring_angle(f.origin, f.n, f.u, ro, rd)?),
    (Grip::Scale, around) => (DVec3::ZERO, axis_param(f.origin, if around { -f.n } else { u_dir(f, spec) }, ro, rd)?),
  };
  Some(MapDrag { grip, spec: spec.clone(), at, t })
}

/// Pointer distance (points) that catches a grip.
const GRIP_PX: f32 = 10.0;
const GRIP_IDLE: egui::Color32 = egui::Color32::from_rgb(30, 110, 235);
const GRIP_HOT: egui::Color32 = egui::Color32::from_rgb(255, 140, 20);

/// Texture Map Controls' manipulator in the canvas, as Fusion's: a ring that turns the texture,
/// a centre square (or, round an axis, an arrow along it) that slides it, and an arrow with a
/// square head that sizes it; the canvas shows the mapping as it is dragged, OK keeps it.
/// Returns whether it has the pointer.
pub(crate) fn texture_map_handles(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  project: &dyn Fn(DVec3) -> Option<egui::Pos2>,
  ray: &dyn Fn(egui::Pos2) -> (DVec3, DVec3),
) -> bool {
  if !app.render.active {
    return false;
  }
  let Some(form) = app.render.texture_map.as_ref() else { return false };
  let (Some(body), Some(spec)) = (form.body, form.spec.clone()) else { return false };
  let (_, _, back) = app.cam.basis();
  let Some(f) = map_frame(&app.s, body, &spec, back) else { return false };
  let heads = grip_points(&f, &spec).map(|p| p.and_then(project));
  let ring: Vec<egui::Pos2> = (0..=48)
    .filter_map(|i| {
      let a = i as f64 / 48.0 * std::f64::consts::TAU;
      project(f.origin + (f.u * a.cos() + f.v * a.sin()) * f.radius)
    })
    .collect();
  let grips = [Grip::Move, Grip::Turn, Grip::Scale];
  let hit = |p: egui::Pos2| -> Option<Grip> {
    let near = |h: Option<egui::Pos2>| h.map(|h| h.distance(p)).filter(|d| *d <= GRIP_PX);
    let best = grips.iter().zip(heads).filter_map(|(g, h)| near(h).map(|d| (*g, d))).min_by(|a, b| a.1.total_cmp(&b.1));
    best.map(|b| b.0).or_else(|| {
      // anywhere on the ring turns
      let on_ring = ring.windows(2).any(|w| {
        egui::Rect::from_two_pos(w[0], w[1]).expand(5.0).contains(p) && {
          let (a, b) = (w[0], w[1]);
          let ab = b - a;
          let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
          (a + ab * t).distance(p) <= 5.0
        }
      });
      on_ring.then_some(Grip::Turn)
    })
  };
  let mut held = app.render.texture_map.as_ref().and_then(|f| f.drag.clone());
  if let Some(d) = &held {
    if resp.dragged_by(egui::PointerButton::Primary)
      && let Some(p) = resp.interact_pointer_pos()
    {
      let (o, dir) = ray(p);
      if let Some(t) = drag_to(&f, d, o, dir)
        && let Some(form) = app.render.texture_map.as_mut()
      {
        form.spec = Some(t);
      }
    }
    if !resp.dragged_by(egui::PointerButton::Primary) {
      held = None;
      if let Some(form) = app.render.texture_map.as_mut() {
        form.drag = None;
      }
    }
  }
  let hot = held.as_ref().map(|d| d.grip).or_else(|| resp.hover_pos().and_then(hit));
  let press = resp.ctx.input(|i| i.pointer.press_origin());
  if held.is_none()
    && app.drag.is_none()
    && resp.drag_started_by(egui::PointerButton::Primary)
    && let Some(p) = press
    && let Some(g) = hit(p)
  {
    let (o, dir) = ray(p);
    if let Some(d) = begin_drag(&f, &spec, g, o, dir)
      && let Some(form) = app.render.texture_map.as_mut()
    {
      form.drag = Some(d);
      held = form.drag.clone();
    }
  }
  // the grips where the mapping now puts them
  let spec = app.render.texture_map.as_ref().and_then(|f| f.spec.clone()).unwrap_or(spec);
  let heads = grip_points(&f, &spec).map(|p| p.and_then(project));
  let color = |g: Grip| if hot == Some(g) { GRIP_HOT } else { GRIP_IDLE };
  painter.add(egui::Shape::line(ring, Stroke::new(if hot == Some(Grip::Turn) { 2.5 } else { 1.5 }, color(Grip::Turn))));
  let centre = project(f.origin);
  for (g, h) in grips.iter().zip(heads) {
    let Some(h) = h else { continue };
    let c = color(*g);
    match g {
      Grip::Turn => {
        painter.circle(h, 5.5, egui::Color32::WHITE, Stroke::new(2.0, c));
      }
      Grip::Move if !f.around => {
        painter.rect(
          Rect::from_center_size(h, vec2(11.0, 11.0)),
          1.0,
          c,
          Stroke::new(1.0, egui::Color32::WHITE),
          egui::StrokeKind::Inside,
        );
      }
      Grip::Move => {
        if let Some(o) = centre {
          painter.line_segment([o, h], Stroke::new(2.0, c));
        }
        let dir = centre.map(|o| (h - o).normalized()).unwrap_or(vec2(0.0, -1.0));
        let side = vec2(-dir.y, dir.x);
        painter.add(egui::Shape::convex_polygon(vec![h + dir * 9.0, h - side * 5.5, h + side * 5.5], c, Stroke::NONE));
      }
      Grip::Scale => {
        if let Some(o) = centre {
          painter.line_segment([o, h], Stroke::new(2.0, c));
        }
        painter.rect_filled(Rect::from_center_size(h, vec2(9.0, 9.0)), 0.0, c);
      }
    }
  }
  if hot.is_some() {
    resp.ctx.set_cursor_icon(if held.is_some() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
  }
  if held.is_some() {
    resp.ctx.request_repaint();
  }
  hot.is_some() || held.is_some()
}

/// While TEXTURE MAP CONTROLS is open, the canvas shows the mapping being edited on the meshes
/// that wear the texture: the picked face's own, or its body's (`picks` names the meshes' faces).
/// They show it even while selected, so the texture can be seen as it is moved.
pub(crate) fn preview_texture_map(app: &mut App, scene: &mut fox_engine::render::Scene, picks: &PickTable) {
  if !app.render.active {
    return;
  }
  let Some(form) = &app.render.texture_map else { return };
  let (Some(body), face, Some(spec)) = (form.body, form.face, form.spec.clone()) else { return };
  let (bodies, own) = fox_engine::rendering::drawn_materials(&app.s);
  let Some(material) = (match face {
    Some(f) => own.get(&(body, f)),
    None => bodies.get(&body),
  }) else {
    return;
  };
  let (_, _, back) = app.cam.basis();
  let origin = map_frame(&app.s, body, &spec, back).map(|f| f.origin.as_vec3().to_array());
  let tile = fox_engine::rendering::texture_tile(material, &mut app.render.tiles);
  for m in &mut scene.meshes {
    let first = m.tri_ids.first().copied().unwrap_or(m.id);
    let Some(Pick::Face { body: b, face: f }) = picks.get(first) else { continue };
    let wears = b == body
      && match face {
        Some(x) => x == f,
        None => !own.contains_key(&(b, f)),
      };
    if !wears {
      continue;
    }
    match &mut m.texture {
      Some(t) => t.projection = fox_engine::rendering::tex_projection(&spec, t.projection.origin),
      None if m.color == fox_engine::scene::SELECT_COLOR => {
        if let (Some(tile), Some(o)) = (&tile, origin) {
          let projection = fox_engine::rendering::tex_projection(&spec, o);
          m.texture = Some(fox_engine::render::MeshTexture { tile: tile.clone(), projection });
          m.color = [material.color[0], material.color[1], material.color[2], m.color[3]];
        }
      }
      None => {}
    }
  }
}

/// What the canvas preview of TEXTURE MAP CONTROLS depends on (part of the render key).
pub(crate) fn texture_map_key(app: &App) -> String {
  match &app.render.texture_map {
    Some(f) if app.render.active => format!("{:?}{:?}{:?}", f.body, f.face, f.spec),
    _ => String::new(),
  }
}

// ---------------------------------------------------------------- dielectric priority

/// The DIELECTRIC PRIORITY CONTROL dialog's priority (`None` until picked: the bodies' own).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PriorityForm {
  pub priority: Option<u8>,
}

/// The docked DIELECTRIC PRIORITY CONTROL dialog: Bodies and Priority (0, 1, 2); OK gives the
/// selected bodies the priority.
pub(crate) fn priority_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.render.priority else { return };
  let bodies = selected_bodies(app);
  let accent = icons::accent_of(Icon::DielectricPriority);
  let current = bodies.first().map_or(0, |b| app.s.design.render.priority(*b));
  let mut prio = form.priority.unwrap_or(current);
  let (ok, cancel) = docked(
    ctx,
    "dielectric-priority",
    Icon::DielectricPriority,
    "DIELECTRIC PRIORITY CONTROL",
    bodies.is_empty().then_some("Select the bodies to give a priority"),
    app.canvas,
    |ui| {
      egui::Grid::new("priority-grid").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
        ui.label("Bodies");
        dialogs::sel_chip(ui, accent, &dialogs::chip_text(bodies.len(), "selected"), !bodies.is_empty());
        ui.end_row();
        ui.label("Priority");
        egui::ComboBox::from_id_salt("priority-value").selected_text(prio.to_string()).width(150.0).show_ui(ui, |ui| {
          for p in [0u8, 1, 2] {
            ui.selectable_value(&mut prio, p, p.to_string());
          }
        });
        ui.end_row();
      });
    },
  );
  if prio != current || form.priority.is_some() {
    form.priority = Some(prio);
  }
  if ok {
    let ids: Vec<u32> = bodies.iter().map(|b| b.0).collect();
    if app.run("render.priority", json!({"bodies": ids, "priority": prio})).is_some() {
      app.render.priority = None;
      return;
    }
  }
  app.render.priority = (!cancel).then_some(form);
}

// ---------------------------------------------------------------- in-canvas render settings

/// In-canvas Render's mode: Fusion's Fast and Advanced tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InCanvasMode {
  Fast,
  #[default]
  Advanced,
}

/// How far In-canvas Render refines before it stops: the marks on its progress bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Quality {
  Excellent,
  Final,
  #[default]
  Infinite,
}

impl Quality {
  pub(crate) const ALL: [Quality; 3] = [Quality::Excellent, Quality::Final, Quality::Infinite];

  pub(crate) fn label(self) -> &'static str {
    match self {
      Quality::Excellent => "Excellent",
      Quality::Final => "Final",
      Quality::Infinite => "Infinite",
    }
  }

  /// The iteration count it stops at (`None`: it doesn't).
  pub(crate) fn iterations(self) -> Option<u32> {
    match self {
      Quality::Excellent => Some(crate::render_panels::EXCELLENT.0),
      Quality::Final => Some(crate::render_panels::FINAL.0),
      Quality::Infinite => None,
    }
  }
}

/// In-canvas Render's settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InCanvasSettings {
  pub mode: InCanvasMode,
  pub quality: Quality,
}

/// The docked IN-CANVAS RENDER SETTINGS dialog: Fast and Advanced tabs (the open one is the
/// mode), Advanced's Quality; OK keeps them.
pub(crate) fn in_canvas_settings_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.render.in_canvas_settings else { return };
  let (ok, cancel) =
    docked(ctx, "in-canvas-settings", Icon::InCanvasSettings, "IN-CANVAS RENDER SETTINGS", None, app.canvas, |ui| {
      mode_tabs(ui, &mut form.mode);
      ui.add_space(6.0);
      egui::Grid::new("in-canvas-settings-grid").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(
        ui,
        |ui| match form.mode {
          InCanvasMode::Fast => {
            ui.label("Limit Resolution");
            ui.add_enabled(false, egui::Checkbox::without_text(&mut false))
              .on_disabled_hover_text("FissionOxide's in-canvas render always draws at the canvas's resolution");
            ui.end_row();
          }
          InCanvasMode::Advanced => {
            ui.label("Quality");
            egui::ComboBox::from_id_salt("in-canvas-quality").selected_text(form.quality.label()).width(150.0).show_ui(
              ui,
              |ui| {
                for q in Quality::ALL {
                  ui.selectable_value(&mut form.quality, q, q.label());
                }
              },
            );
            ui.end_row();
          }
        },
      );
    });
  if ok {
    app.render.canvas_settings = form;
  }
  app.render.in_canvas_settings = (!ok && !cancel).then_some(form);
}

/// Fast | Advanced, the open one underlined in the accent.
fn mode_tabs(ui: &mut egui::Ui, mode: &mut InCanvasMode) {
  let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), egui::Sense::hover());
  ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, theme::SEPARATOR));
  let mut x = rect.left() + 6.0;
  for (m, label) in [(InCanvasMode::Fast, "Fast"), (InCanvasMode::Advanced, "Advanced")] {
    let on = *mode == m;
    let g = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), theme::TEXT);
    let r = Rect::from_min_size(pos2(x - 6.0, rect.top()), vec2(g.size().x + 12.0, rect.height()));
    let resp = ui.interact(r, ui.id().with(("in-canvas-tab", label)), egui::Sense::click());
    if resp.hovered() && !on {
      ui.painter().rect_filled(r, 2.0, theme::HOVER);
    }
    ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, if on { theme::TEXT } else { theme::MUTED });
    if on {
      ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(2.0, theme::ACCENT));
    }
    if resp.clicked() {
      *mode = m;
    }
    x = r.right() + 14.0;
  }
}
