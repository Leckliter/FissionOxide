//! A small software rasterizer for the modeling viewport.
//!
//! It draws shaded triangle meshes, depth-tested polylines (B-rep edges, sketch curves, the
//! grid) and points into an RGBA frame, alongside a depth buffer and an id buffer used for
//! picking (what's under the cursor). Running on the CPU keeps the app portable and lets the CLI
//! render the exact same images headless.
//!
//! World space is Z-up, millimetres.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

use glam::{DMat4, DVec3, DVec4, Vec3};

/// Hostile-size guard for frames.
pub const MAX_DIM: u32 = 8192;

/// Id value meaning "nothing here".
pub const NO_ID: u32 = 0;

/// Orbit camera around a target point.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Camera {
  pub target: [f64; 3],
  /// Rotation about +Z (radians); 0 looks from +Y... see [`Camera::eye_dir`].
  pub yaw: f64,
  /// Elevation above the XY plane (radians), clamped to ±89.9°.
  pub pitch: f64,
  pub distance: f64,
  /// Vertical field of view in radians; ignored when `ortho`.
  pub fov: f64,
  pub ortho: bool,
}

impl Default for Camera {
  /// Fusion's "home" view: isometric-ish from the front-right-top.
  fn default() -> Self {
    Camera {
      target: [0.0, 0.0, 0.0],
      yaw: std::f64::consts::FRAC_PI_4,
      pitch: 0.6155,
      distance: 200.0,
      fov: 30f64.to_radians(),
      ortho: false,
    }
  }
}

/// Named standard views (the ViewCube faces).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardView {
  Home,
  Top,
  Bottom,
  Front,
  Back,
  Left,
  Right,
}

impl StandardView {
  pub fn parse(s: &str) -> Option<Self> {
    Some(match s.to_ascii_lowercase().as_str() {
      "home" | "iso" => StandardView::Home,
      "top" => StandardView::Top,
      "bottom" => StandardView::Bottom,
      "front" => StandardView::Front,
      "back" => StandardView::Back,
      "left" => StandardView::Left,
      "right" => StandardView::Right,
      _ => return None,
    })
  }
}

impl Camera {
  /// Unit vector from the target towards the eye.
  pub fn eye_dir(&self) -> DVec3 {
    let (sp, cp) = self.pitch.sin_cos();
    let (sy, cy) = self.yaw.sin_cos();
    // yaw = 0 → eye on -Y (looking at the front face), rotating counter-clockwise about +Z.
    DVec3::new(cp * sy, -cp * cy, sp)
  }

  pub fn eye(&self) -> DVec3 {
    DVec3::from(self.target) + self.eye_dir() * self.distance
  }

  /// Camera right / up vectors.
  pub fn basis(&self) -> (DVec3, DVec3, DVec3) {
    let back = self.eye_dir();
    let mut right = DVec3::Z.cross(back);
    if right.length_squared() < 1e-12 {
      right = DVec3::X;
    }
    let right = right.normalize();
    let up = back.cross(right).normalize();
    (right, up, back)
  }

  pub fn set_view(&mut self, v: StandardView) {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
    let top = FRAC_PI_2 - 1e-6;
    let (yaw, pitch) = match v {
      StandardView::Home => (FRAC_PI_4, 0.6155),
      StandardView::Top => (0.0, top),
      StandardView::Bottom => (0.0, -top),
      StandardView::Front => (0.0, 0.0),
      StandardView::Back => (PI, 0.0),
      StandardView::Right => (FRAC_PI_2, 0.0),
      StandardView::Left => (-FRAC_PI_2, 0.0),
    };
    self.yaw = yaw;
    self.pitch = pitch;
  }

  /// Rotates by screen-space drag deltas (radians).
  pub fn orbit(&mut self, dyaw: f64, dpitch: f64) {
    self.yaw = (self.yaw + dyaw).rem_euclid(std::f64::consts::TAU);
    let lim = std::f64::consts::FRAC_PI_2 - 1e-6;
    self.pitch = (self.pitch + dpitch).clamp(-lim, lim);
  }

  /// Pans by a screen-space delta in pixels for a viewport of height `h`.
  pub fn pan(&mut self, dx_px: f64, dy_px: f64, h: f64) {
    let (right, up, _) = self.basis();
    let per_px = self.view_height() / h.max(1.0);
    let t = DVec3::from(self.target) - right * dx_px * per_px + up * dy_px * per_px;
    self.target = t.to_array();
  }

  /// World-space height visible at the target distance.
  pub fn view_height(&self) -> f64 {
    2.0 * self.distance * (self.fov / 2.0).tan()
  }

  /// Zooms by `factor` (< 1 zooms in), keeping the world point under the cursor fixed when given.
  pub fn zoom(&mut self, factor: f64, toward: Option<DVec3>) {
    let factor = factor.clamp(0.01, 100.0);
    let new_d = (self.distance * factor).clamp(1e-3, 1e7);
    if let Some(p) = toward {
      let t = DVec3::from(self.target);
      let k = 1.0 - new_d / self.distance;
      // move the target along (p - t) projected on the view plane
      let (_, _, back) = self.basis();
      let v = p - t;
      let v = v - back * v.dot(back);
      self.target = (t + v * k).to_array();
    }
    self.distance = new_d;
  }

  /// Frames an axis-aligned box.
  pub fn fit(&mut self, min: DVec3, max: DVec3, aspect: f64) {
    let c = (min + max) / 2.0;
    let r = ((max - min).length() / 2.0).max(1.0);
    self.target = c.to_array();
    let half_fov = (self.fov / 2.0).min((self.fov / 2.0).tan().atan2(1.0 / aspect.max(0.1)));
    self.distance = (r / half_fov.tan().max(1e-3)) * 1.15;
  }

  pub fn view_matrix(&self) -> DMat4 {
    DMat4::look_at_rh(self.eye(), DVec3::from(self.target), self.basis().1)
  }

  pub fn proj_matrix(&self, aspect: f64) -> DMat4 {
    let near = (self.distance * 0.01).max(1e-4);
    let far = self.distance * 100.0 + 1e4;
    if self.ortho {
      let h = self.view_height() / 2.0;
      let w = h * aspect;
      DMat4::orthographic_rh(-w, w, -h, h, -far, far)
    } else {
      DMat4::perspective_rh(self.fov, aspect, near, far)
    }
  }

  pub fn view_proj(&self, w: u32, h: u32) -> DMat4 {
    self.proj_matrix(w as f64 / h.max(1) as f64) * self.view_matrix()
  }

  /// Projects a world point to pixel coordinates and NDC depth (None if behind the camera).
  pub fn project(&self, p: DVec3, w: u32, h: u32) -> Option<(f64, f64, f64)> {
    let c = self.view_proj(w, h) * p.extend(1.0);
    if c.w <= 1e-9 {
      return None;
    }
    let n = c.truncate() / c.w;
    Some(((n.x + 1.0) * 0.5 * w as f64, (1.0 - n.y) * 0.5 * h as f64, n.z))
  }

  /// World-space ray through a pixel.
  pub fn ray(&self, px: f64, py: f64, w: u32, h: u32) -> (DVec3, DVec3) {
    let inv = self.view_proj(w, h).inverse();
    let nx = px / w.max(1) as f64 * 2.0 - 1.0;
    let ny = 1.0 - py / h.max(1) as f64 * 2.0;
    let a = inv * DVec4::new(nx, ny, -1.0, 1.0);
    let b = inv * DVec4::new(nx, ny, 1.0, 1.0);
    let a = a.truncate() / a.w;
    let b = b.truncate() / b.w;
    (a, (b - a).normalize_or_zero())
  }
}

/// Intersects a ray with a plane (point + normal); returns the hit point.
pub fn ray_plane(origin: DVec3, dir: DVec3, p0: DVec3, n: DVec3) -> Option<DVec3> {
  let d = dir.dot(n);
  if d.abs() < 1e-12 {
    return None;
  }
  let t = (p0 - origin).dot(n) / d;
  (t.is_finite()).then(|| origin + dir * t)
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
  pub positions: Vec<[f32; 3]>,
  pub normals: Vec<[f32; 3]>,
  pub indices: Vec<u32>,
  /// Pick id per triangle (indices.len() / 3 entries), or empty to use `id` for all.
  pub tri_ids: Vec<u32>,
  pub id: u32,
  pub color: [u8; 4],
  /// Highlighted triangles' id (drawn tinted), e.g. the hovered face.
  pub highlight: Option<u32>,
  pub highlight_color: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct Polyline {
  pub points: Vec<[f32; 3]>,
  pub color: [u8; 4],
  /// Line width in pixels (1–4).
  pub width: f32,
  pub id: u32,
  /// Drawn on top of everything (sketch geometry in sketch mode).
  pub on_top: bool,
  /// Dashed (construction geometry).
  pub dashed: bool,
  /// Pushed back in depth so coincident faces/profiles cover it (the ground grid).
  #[doc(hidden)]
  pub behind: bool,
}

#[derive(Debug, Clone)]
pub struct Marker {
  pub at: [f32; 3],
  pub color: [u8; 4],
  pub size: f32,
  pub id: u32,
  pub on_top: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
  pub meshes: Vec<Mesh>,
  pub lines: Vec<Polyline>,
  pub markers: Vec<Marker>,
  pub background_top: [u8; 4],
  pub background_bottom: [u8; 4],
}

pub struct Frame {
  pub width: u32,
  pub height: u32,
  pub rgba: Vec<u8>,
  pub depth: Vec<f32>,
  pub ids: Vec<u32>,
}

impl Frame {
  pub fn new(width: u32, height: u32) -> Self {
    let (w, h) = (width.clamp(1, MAX_DIM), height.clamp(1, MAX_DIM));
    let n = (w * h) as usize;
    Frame { width: w, height: h, rgba: vec![0; n * 4], depth: vec![f32::INFINITY; n], ids: vec![NO_ID; n] }
  }

  /// Id under a pixel, searching a small radius so thin edges are easy to hit.
  pub fn pick(&self, x: f64, y: f64, radius: i32, prefer: impl Fn(u32) -> bool) -> u32 {
    let (cx, cy) = (x as i32, y as i32);
    let mut best = NO_ID;
    let mut best_d = i32::MAX;
    for dy in -radius..=radius {
      for dx in -radius..=radius {
        let (px, py) = (cx + dx, cy + dy);
        if px < 0 || py < 0 || px >= self.width as i32 || py >= self.height as i32 {
          continue;
        }
        let id = self.ids.get((py as u32 * self.width + px as u32) as usize).copied().unwrap_or(NO_ID);
        if id == NO_ID {
          continue;
        }
        let d = dx * dx + dy * dy;
        let d = if prefer(id) { d } else { d + 1000 };
        if d < best_d {
          best_d = d;
          best = id;
        }
      }
    }
    best
  }

  pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
    image::save_buffer(path, &self.rgba, self.width, self.height, image::ExtendedColorType::Rgba8)
      .map_err(|e| e.to_string())
  }

  /// The frame encoded as PNG.
  pub fn to_png(&self) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::write_buffer_with_format(
      &mut out,
      &self.rgba,
      self.width,
      self.height,
      image::ExtendedColorType::Rgba8,
      image::ImageFormat::Png,
    )
    .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
  }
}

/// Renders a scene from a camera into a new frame.
pub fn render(scene: &Scene, cam: &Camera, width: u32, height: u32) -> Frame {
  let mut f = Frame::new(width, height);
  let (w, h) = (f.width, f.height);
  // background gradient
  for y in 0..h {
    let t = y as f32 / (h.max(2) - 1) as f32;
    let c = lerp_color(scene.background_top, scene.background_bottom, t);
    for x in 0..w {
      let i = ((y * w + x) * 4) as usize;
      if let Some(px) = f.rgba.get_mut(i..i + 4) {
        px.copy_from_slice(&c);
      }
    }
  }
  let vp = cam.view_proj(w, h);
  let eye = cam.eye();
  let (_, _, back) = cam.basis();
  // light: a key light from upper-left of the view plus the headlight
  let (right, up, _) = cam.basis();
  let key = (back * 0.8 + up * 0.5 - right * 0.3).normalize().as_vec3();
  let head = back.as_vec3();
  let fill = (right * 0.7 - up * 0.2 + back * 0.3).normalize().as_vec3();
  for m in &scene.meshes {
    draw_mesh(&mut f, m, &vp, cam, eye, [key, head, fill]);
  }
  for l in scene.lines.iter().filter(|l| !l.on_top) {
    draw_polyline(&mut f, l, &vp);
  }
  for mk in scene.markers.iter().filter(|m| !m.on_top) {
    draw_marker(&mut f, mk, &vp);
  }
  for l in scene.lines.iter().filter(|l| l.on_top) {
    draw_polyline(&mut f, l, &vp);
  }
  for mk in scene.markers.iter().filter(|m| m.on_top) {
    draw_marker(&mut f, mk, &vp);
  }
  f
}

fn lerp_color(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
  let mut o = [0u8; 4];
  for i in 0..4 {
    o[i] = (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round().clamp(0.0, 255.0) as u8;
  }
  o
}

/// Clip-space vertex.
#[derive(Clone, Copy)]
struct CV {
  c: DVec4,
  n: Vec3,
  wpos: Vec3,
}

fn draw_mesh(f: &mut Frame, m: &Mesh, vp: &DMat4, cam: &Camera, eye: DVec3, lights: [Vec3; 3]) {
  let tris = m.indices.len() / 3;
  for t in 0..tris {
    let idx = [m.indices[t * 3], m.indices[t * 3 + 1], m.indices[t * 3 + 2]];
    let mut v = [CV { c: DVec4::ZERO, n: Vec3::Z, wpos: Vec3::ZERO }; 3];
    let mut ok = true;
    for k in 0..3 {
      let Some(p) = m.positions.get(idx[k] as usize) else {
        ok = false;
        break;
      };
      let n = m.normals.get(idx[k] as usize).copied().unwrap_or([0.0, 0.0, 1.0]);
      let wp = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
      v[k] = CV { c: *vp * wp.extend(1.0), n: Vec3::from(n), wpos: Vec3::from(*p) };
    }
    if !ok {
      continue;
    }
    let id = m.tri_ids.get(t).copied().unwrap_or(m.id);
    let color = if m.highlight == Some(id) { m.highlight_color } else { m.color };
    // near-plane clipping (z > -w in GL clip space)
    let poly = clip_near(&v);
    if poly.len() < 3 {
      continue;
    }
    for i in 1..poly.len() - 1 {
      raster_tri(f, [poly[0], poly[i], poly[i + 1]], id, color, cam, eye, lights);
    }
  }
}

fn clip_near(v: &[CV; 3]) -> Vec<CV> {
  let inside = |p: &CV| p.c.z + p.c.w > 0.0;
  if v.iter().all(inside) {
    return v.to_vec();
  }
  let mut out = Vec::with_capacity(4);
  for i in 0..3 {
    let a = v[i];
    let b = v[(i + 1) % 3];
    let (ia, ib) = (inside(&a), inside(&b));
    if ia {
      out.push(a);
    }
    if ia != ib {
      let da = a.c.z + a.c.w;
      let db = b.c.z + b.c.w;
      let t = da / (da - db);
      out.push(CV {
        c: a.c + (b.c - a.c) * t,
        n: a.n + (b.n - a.n) * t as f32,
        wpos: a.wpos + (b.wpos - a.wpos) * t as f32,
      });
    }
  }
  out
}

#[allow(clippy::too_many_arguments)]
fn raster_tri(f: &mut Frame, v: [CV; 3], id: u32, color: [u8; 4], cam: &Camera, eye: DVec3, lights: [Vec3; 3]) {
  let [key, head, fill] = lights;
  let (w, h) = (f.width as f64, f.height as f64);
  let mut sx = [0.0f64; 3];
  let mut sy = [0.0f64; 3];
  let mut sz = [0.0f64; 3];
  let mut inv_w = [0.0f64; 3];
  for k in 0..3 {
    let c = v[k].c;
    if c.w.abs() < 1e-12 {
      return;
    }
    let iw = 1.0 / c.w;
    sx[k] = (c.x * iw + 1.0) * 0.5 * w;
    sy[k] = (1.0 - c.y * iw) * 0.5 * h;
    sz[k] = c.z * iw;
    inv_w[k] = iw;
  }
  let area = (sx[1] - sx[0]) * (sy[2] - sy[0]) - (sx[2] - sx[0]) * (sy[1] - sy[0]);
  if area.abs() < 1e-12 || !area.is_finite() {
    return;
  }
  let minx = sx.iter().cloned().fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
  let maxx = sx.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil().min(w - 1.0) as i64;
  let miny = sy.iter().cloned().fold(f64::INFINITY, f64::min).floor().max(0.0) as i64;
  let maxy = sy.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil().min(h - 1.0) as i64;
  if minx > maxx || miny > maxy {
    return;
  }
  // Flat-ish shading per triangle with the face normal blended with vertex normals per pixel.
  let base = Vec3::new(color[0] as f32, color[1] as f32, color[2] as f32) / 255.0;
  let fw = f.width as i64;
  let persp = !cam.ortho;
  for py in miny..=maxy {
    let y = py as f64 + 0.5;
    for px in minx..=maxx {
      let x = px as f64 + 0.5;
      let w0 = ((sx[1] - x) * (sy[2] - y) - (sx[2] - x) * (sy[1] - y)) / area;
      let w1 = ((sx[2] - x) * (sy[0] - y) - (sx[0] - x) * (sy[2] - y)) / area;
      let w2 = 1.0 - w0 - w1;
      if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
        continue;
      }
      let z = w0 * sz[0] + w1 * sz[1] + w2 * sz[2];
      if !(-1.0..=1.0).contains(&z) {
        continue;
      }
      let i = (py * fw + px) as usize;
      let Some(d) = f.depth.get_mut(i) else { continue };
      let zf = z as f32;
      if zf >= *d {
        continue;
      }
      *d = zf;
      // perspective-correct normal interpolation
      let (b0, b1, b2) = if persp {
        let a0 = w0 * inv_w[0];
        let a1 = w1 * inv_w[1];
        let a2 = w2 * inv_w[2];
        let s = a0 + a1 + a2;
        (a0 / s, a1 / s, a2 / s)
      } else {
        (w0, w1, w2)
      };
      let n = (v[0].n * b0 as f32 + v[1].n * b1 as f32 + v[2].n * b2 as f32).normalize_or_zero();
      let wpos = v[0].wpos * b0 as f32 + v[1].wpos * b1 as f32 + v[2].wpos * b2 as f32;
      let view = if persp { (eye.as_vec3() - wpos).normalize_or_zero() } else { head };
      // two-sided: flip normals facing away
      let n = if n.dot(view) < 0.0 { -n } else { n };
      let diff = 0.45 * n.dot(key).max(0.0) + 0.3 * n.dot(head).max(0.0) + 0.2 * n.dot(fill).max(0.0);
      let half = (key + view).normalize_or_zero();
      let spec = n.dot(half).max(0.0).powf(40.0) * 0.2;
      let c = base * (0.38 + diff) + Vec3::splat(spec);
      if let Some(p) = f.rgba.get_mut(i * 4..i * 4 + 4) {
        p[0] = (c.x * 255.0).clamp(0.0, 255.0) as u8;
        p[1] = (c.y * 255.0).clamp(0.0, 255.0) as u8;
        p[2] = (c.z * 255.0).clamp(0.0, 255.0) as u8;
        p[3] = 255;
      }
      if let Some(slot) = f.ids.get_mut(i) {
        *slot = id;
      }
    }
  }
}

fn draw_polyline(f: &mut Frame, l: &Polyline, vp: &DMat4) {
  let mut dash_phase = 0.0f64;
  for seg in l.points.windows(2) {
    let a = DVec3::new(seg[0][0] as f64, seg[0][1] as f64, seg[0][2] as f64);
    let b = DVec3::new(seg[1][0] as f64, seg[1][1] as f64, seg[1][2] as f64);
    let mut ca = *vp * a.extend(1.0);
    let mut cb = *vp * b.extend(1.0);
    // clip to near plane
    let da = ca.z + ca.w;
    let db = cb.z + cb.w;
    if da <= 0.0 && db <= 0.0 {
      continue;
    }
    if da <= 0.0 {
      ca = ca + (cb - ca) * (da / (da - db));
    } else if db <= 0.0 {
      cb = ca + (cb - ca) * (da / (da - db));
    }
    let to_screen = |c: DVec4| {
      let iw = 1.0 / c.w;
      ((c.x * iw + 1.0) * 0.5 * f.width as f64, (1.0 - c.y * iw) * 0.5 * f.height as f64, c.z * iw)
    };
    let (x0, y0, z0) = to_screen(ca);
    let (x1, y1, z1) = to_screen(cb);
    let len = (x1 - x0).hypot(y1 - y0);
    if !len.is_finite() || len > 1e6 {
      continue;
    }
    let steps = len.ceil().max(1.0) as usize;
    let half = (l.width.clamp(1.0, 6.0) as f64 - 1.0) / 2.0;
    for s in 0..=steps {
      let t = s as f64 / steps as f64;
      if l.dashed && ((dash_phase + t * len) / 6.0) as i64 % 2 == 1 {
        continue;
      }
      let x = x0 + (x1 - x0) * t;
      let y = y0 + (y1 - y0) * t;
      // lines win depth ties against the faces they lie on (the grid loses them)
      let bias = if l.behind { 2e-5 } else { -2e-5 };
      let z = (z0 + (z1 - z0) * t) as f32 + bias;
      let r = half.ceil() as i64;
      for oy in -r..=r {
        for ox in -r..=r {
          plot(f, x as i64 + ox, y as i64 + oy, z, l.color, l.id, l.on_top);
        }
      }
    }
    dash_phase += len;
  }
}

fn draw_marker(f: &mut Frame, m: &Marker, vp: &DMat4) {
  let p = DVec3::new(m.at[0] as f64, m.at[1] as f64, m.at[2] as f64);
  let c = *vp * p.extend(1.0);
  if c.w <= 1e-9 {
    return;
  }
  let x = (c.x / c.w + 1.0) * 0.5 * f.width as f64;
  let y = (1.0 - c.y / c.w) * 0.5 * f.height as f64;
  let z = (c.z / c.w) as f32 - 5e-5;
  let r = m.size.clamp(1.0, 12.0) as i64;
  for oy in -r..=r {
    for ox in -r..=r {
      if ox * ox + oy * oy <= r * r {
        plot(f, x as i64 + ox, y as i64 + oy, z, m.color, m.id, m.on_top);
      }
    }
  }
}

fn plot(f: &mut Frame, x: i64, y: i64, z: f32, color: [u8; 4], id: u32, on_top: bool) {
  if x < 0 || y < 0 || x >= f.width as i64 || y >= f.height as i64 {
    return;
  }
  let i = (y * f.width as i64 + x) as usize;
  let Some(d) = f.depth.get_mut(i) else { return };
  if !on_top {
    if z > *d {
      return;
    }
    *d = z;
  }
  if let Some(p) = f.rgba.get_mut(i * 4..i * 4 + 4) {
    let a = color[3] as u32;
    for k in 0..3 {
      p[k] = ((color[k] as u32 * a + p[k] as u32 * (255 - a)) / 255) as u8;
    }
    p[3] = 255;
  }
  if id != NO_ID
    && let Some(slot) = f.ids.get_mut(i)
  {
    *slot = id;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn quad(z: f32, color: [u8; 4], id: u32) -> Mesh {
    Mesh {
      positions: vec![[-10.0, -10.0, z], [10.0, -10.0, z], [10.0, 10.0, z], [-10.0, 10.0, z]],
      normals: vec![[0.0, 0.0, 1.0]; 4],
      indices: vec![0, 1, 2, 0, 2, 3],
      tri_ids: vec![],
      id,
      color,
      highlight: None,
      highlight_color: [0; 4],
    }
  }

  #[test]
  fn depth_test_and_picking() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Top);
    cam.distance = 100.0;
    let scene =
      Scene { meshes: vec![quad(0.0, [255, 0, 0, 255], 1), quad(5.0, [0, 0, 255, 255], 2)], ..Default::default() };
    let f = render(&scene, &cam, 200, 100);
    assert_eq!(f.pick(100.0, 50.0, 0, |_| true), 2, "the upper quad is in front from the top");
    assert_eq!(f.pick(2.0, 2.0, 0, |_| true), NO_ID);
    cam.set_view(StandardView::Bottom);
    let f = render(&scene, &cam, 200, 100);
    assert_eq!(f.pick(100.0, 50.0, 0, |_| true), 1);
  }

  #[test]
  fn project_and_ray_round_trip() {
    let cam = Camera::default();
    let p = DVec3::new(10.0, -5.0, 3.0);
    let (x, y, _) = cam.project(p, 800, 600).unwrap();
    let (o, d) = cam.ray(x, y, 800, 600);
    let dist = (p - o).cross(d).length();
    assert!(dist < 1e-6, "{dist}");
    let hit = ray_plane(o, d, DVec3::new(0.0, 0.0, 3.0), DVec3::Z).unwrap();
    assert!((hit - p).length() < 1e-6);
  }

  #[test]
  fn camera_behind_geometry_and_hostile_sizes() {
    // inside the quad's extent: near-plane clipping kicks in
    let cam = Camera { distance: 1.0, pitch: 0.0, ..Camera::default() };
    let scene = Scene { meshes: vec![quad(0.0, [255, 0, 0, 255], 1)], ..Default::default() };
    let _ = render(&scene, &cam, 64, 64);
    let f = render(&scene, &cam, 0, u32::MAX);
    assert!(f.width >= 1 && f.height <= MAX_DIM);
  }

  #[test]
  fn standard_views_look_along_axes() {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Front);
    assert!((cam.eye_dir() - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-9);
    cam.set_view(StandardView::Right);
    assert!((cam.eye_dir() - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
    cam.set_view(StandardView::Top);
    assert!(cam.eye_dir().z > 0.999);
  }
}
