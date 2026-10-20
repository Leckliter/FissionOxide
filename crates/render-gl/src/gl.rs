//! The OpenGL side: shaders, buffers, the offscreen target and the draw passes.
#![allow(unsafe_code)]
// Every function here calls glow, whose calls are `unsafe` because they go straight to the
// driver. They are sound as long as the context passed in is current and the objects used came
// from it, which is the documented contract of [`Renderer`].

use std::ops::Range;

use fox_render::{Analysis, Camera, Frame, NO_ID, Scene, VisualStyle};
use glam::Vec3;
use glow::HasContext;

use crate::pack::{self, LINE_STRIDE, MARKER_STRIDE, MESH_STRIDE};

const MESH_VS: &str = r#"
uniform mat4 u_vp;
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_nrm;
layout(location = 2) in vec4 a_col;
layout(location = 3) in uint a_id;
out vec3 v_nrm;
out vec3 v_wpos;
out vec3 v_col;
flat out uint v_id;
void main() {
  v_nrm = a_nrm;
  v_wpos = a_pos;
  v_col = a_col.rgb;
  v_id = a_id;
  gl_Position = u_vp * vec4(a_pos, 1.0);
}
"#;

/// The CPU rasterizer's `raster_tri` shading, per fragment.
const MESH_FS: &str = r#"
uniform vec3 u_key;
uniform vec3 u_head;
uniform vec3 u_fill;
uniform vec3 u_eye;
uniform int u_persp;
uniform vec4 u_color;
uniform vec4 u_hl_color;
uniform uint u_hl;
uniform int u_has_hl;
uniform int u_vcolor;
uniform int u_mode;
uniform vec4 u_param;
in vec3 v_nrm;
in vec3 v_wpos;
in vec3 v_col;
flat in uint v_id;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
ID_COLOR
vec3 safe_norm(vec3 v) {
  float l = length(v);
  return l > 0.0 ? v / l : vec3(0.0);
}
void main() {
  bool hl = u_has_hl == 1 && v_id == u_hl;
  vec4 flat_c = hl ? u_hl_color : u_color;
  vec3 base = (u_vcolor == 1 && !hl) ? v_col : flat_c.rgb;
  vec3 n = safe_norm(v_nrm);
  vec3 view = u_persp == 1 ? safe_norm(u_eye - v_wpos) : u_head;
  if (dot(n, view) < 0.0) n = -n;
  float diff = 0.45 * max(dot(n, u_key), 0.0) + 0.3 * max(dot(n, u_head), 0.0) + 0.2 * max(dot(n, u_fill), 0.0);
  vec3 h = safe_norm(u_key + view);
  float spec = pow(max(dot(n, h), 0.0), 40.0) * 0.2;
  vec3 c;
  if (u_mode == 0) {
    c = base * (0.38 + diff) + vec3(spec);
  } else if (u_mode == 1) {
    vec3 r = n * (2.0 * dot(n, view)) - view;
    float band = floor((r.z * 0.5 + 0.5) * u_param.x);
    float tone = mod(band, 2.0) < 0.5 ? 0.1 : 0.95;
    c = vec3(tone * (0.75 + 0.25 * diff));
  } else if (u_mode == 2) {
    float d = dot(n, u_param.xyz);
    vec3 tone = d > u_param.w ? vec3(0.2, 0.75, 0.25) : (d < -u_param.w ? vec3(0.85, 0.2, 0.2) : vec3(0.95, 0.85, 0.2));
    c = tone * (0.45 + diff);
  } else {
    float d = dot(n, u_param.xyz);
    vec3 tone = d > 1e-3 ? vec3(0.2, 0.75, 0.25) : vec3(0.85, 0.2, 0.2);
    c = tone * (0.45 + diff);
  }
  o_color = vec4(clamp(c, 0.0, 1.0), flat_c.a);
  o_id = id_color(v_id);
}
"#;

/// Segments as screen-space quads: the CPU brush is a square of `width` pixels swept along the
/// segment, so the quad is that wide and reaches half a width past each end.
const LINE_VS: &str = r#"
uniform mat4 u_vp;
uniform vec2 u_size;
uniform int u_hidden;
layout(location = 0) in vec2 a_corner;
layout(location = 1) in vec3 a_a;
layout(location = 2) in vec3 a_b;
layout(location = 3) in vec4 a_color;
layout(location = 4) in uint a_id;
layout(location = 5) in vec3 a_info;
out vec4 v_color;
flat out uint v_id;
noperspective out float v_along;
flat out float v_dashed;
void main() {
  v_color = a_color;
  v_id = a_id;
  v_dashed = a_info.z;
  v_along = 0.0;
  vec4 ca = u_vp * vec4(a_a, 1.0);
  vec4 cb = u_vp * vec4(a_b, 1.0);
  float da = ca.z + ca.w;
  float db = cb.z + cb.w;
  if (da <= 0.0 && db <= 0.0) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  if (da <= 0.0) {
    ca = ca + (cb - ca) * (da / (da - db));
  } else if (db <= 0.0) {
    cb = ca + (cb - ca) * (da / (da - db));
  }
  vec3 na = ca.xyz / ca.w;
  vec3 nb = cb.xyz / cb.w;
  vec2 sa = (na.xy * 0.5 + 0.5) * u_size;
  vec2 sb = (nb.xy * 0.5 + 0.5) * u_size;
  vec2 d = sb - sa;
  float len = length(d);
  if (!(len <= 1e6)) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  vec2 dir = len > 1e-6 ? d / len : vec2(1.0, 0.0);
  vec2 nrm = vec2(-dir.y, dir.x);
  // hidden edges are a pixel wide
  float hw = (u_hidden == 1 ? 1.0 : a_info.x) * 0.5;
  bool end = a_corner.x > 0.5;
  vec2 s = (end ? sb : sa) + nrm * (a_corner.y * hw) + dir * (end ? hw : -hw);
  v_along = dot(s - sa, dir);
  float z = clamp((end ? nb.z : na.z) + a_info.y, -1.0, 1.0);
  gl_Position = vec4(s / u_size * 2.0 - 1.0, z, 1.0);
}
"#;

const LINE_FS: &str = r#"
uniform int u_hidden;
uniform float u_hidden_alpha;
in vec4 v_color;
flat in uint v_id;
noperspective in float v_along;
flat in float v_dashed;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
ID_COLOR
void main() {
  bool dashed = v_dashed > 0.5 || u_hidden == 1;
  if (dashed && mod(floor(max(v_along, 0.0) / 6.0), 2.0) > 0.5) discard;
  o_color = u_hidden == 1 ? vec4(v_color.rgb, v_color.a * u_hidden_alpha) : v_color;
  o_id = id_color(v_id);
}
"#;

/// Markers are discs of whole pixels around the pixel holding the point, as on the CPU.
const MARKER_VS: &str = r#"
uniform mat4 u_vp;
uniform vec2 u_size;
layout(location = 0) in vec2 a_corner;
layout(location = 1) in vec3 a_at;
layout(location = 2) in vec4 a_color;
layout(location = 3) in uint a_id;
layout(location = 4) in float a_r;
out vec4 v_color;
flat out uint v_id;
flat out vec2 v_center;
flat out float v_r;
void main() {
  v_color = a_color;
  v_id = a_id;
  v_r = a_r;
  v_center = vec2(0.0);
  vec4 c = u_vp * vec4(a_at, 1.0);
  if (c.w <= 1e-9) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  vec3 n = c.xyz / c.w;
  // top-down pixel coordinates, like the CPU frame
  v_center = floor(vec2((n.x + 1.0) * 0.5 * u_size.x, (1.0 - n.y) * 0.5 * u_size.y));
  vec2 s = vec2(v_center.x + 0.5, u_size.y - v_center.y - 0.5) + a_corner * (a_r + 1.0);
  gl_Position = vec4(s / u_size * 2.0 - 1.0, clamp(n.z - 5e-5, -1.0, 1.0), 1.0);
}
"#;

const MARKER_FS: &str = r#"
uniform vec2 u_size;
in vec4 v_color;
flat in uint v_id;
flat in vec2 v_center;
flat in float v_r;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
ID_COLOR
void main() {
  vec2 p = vec2(floor(gl_FragCoord.x), u_size.y - 1.0 - floor(gl_FragCoord.y));
  vec2 d = p - v_center;
  if (dot(d, d) > v_r * v_r) discard;
  o_color = v_color;
  o_id = id_color(v_id);
}
"#;

/// A triangle covering the viewport, from `gl_VertexID` alone.
const FULLSCREEN_VS: &str = r#"
out vec2 v_uv;
void main() {
  vec2 p = vec2(float((gl_VertexID & 1) * 4 - 1), float((gl_VertexID & 2) * 2 - 1));
  v_uv = p * 0.5 + 0.5;
  gl_Position = vec4(p, 0.0, 1.0);
}
"#;

const BACKGROUND_FS: &str = r#"
uniform vec2 u_size;
uniform vec4 u_top;
uniform vec4 u_bottom;
in vec2 v_uv;
layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_id;
void main() {
  float t = (u_size.y - gl_FragCoord.y - 0.5) / max(u_size.y - 1.0, 1.0);
  o_color = mix(u_top, u_bottom, clamp(t, 0.0, 1.0));
  o_id = vec4(0.0);
}
"#;

const COMPOSITE_FS: &str = r#"
uniform sampler2D u_tex;
uniform vec4 u_tint;
in vec2 v_uv;
layout(location = 0) out vec4 o_color;
void main() {
  o_color = vec4(texture(u_tex, v_uv).rgb * u_tint.rgb, 1.0);
}
"#;

/// Pick ids as the bytes of an RGBA8 attachment (alpha 1, so blending leaves them intact).
const ID_COLOR: &str = r#"
vec4 id_color(uint id) {
  return vec4(float(id & 255u), float((id >> 8) & 255u), float((id >> 16) & 255u), 255.0) / 255.0;
}
"#;

type Gl = glow::Context;

struct Prog {
  program: glow::Program,
  uniforms: Vec<(&'static str, Option<glow::UniformLocation>)>,
}

impl Prog {
  fn loc(&self, name: &str) -> Option<&glow::UniformLocation> {
    self.uniforms.iter().find(|(n, _)| *n == name).and_then(|(_, l)| l.as_ref())
  }
}

struct GpuMesh {
  vao: glow::VertexArray,
  vbo: glow::Buffer,
  ebo: glow::Buffer,
  count: i32,
  /// How many source vertices / indices it was built from (to check a reuse is sound).
  shape: (usize, usize, usize),
  color: [u8; 4],
  highlight: Option<u32>,
  highlight_color: [u8; 4],
  vertex_colors: bool,
  analyze: bool,
}

struct GpuInstances {
  vao: glow::VertexArray,
  vbo: glow::Buffer,
  groups: [Range<usize>; 4],
}

/// The offscreen frame: colour, pick ids and depth.
struct Target {
  fbo: glow::Framebuffer,
  color: glow::Texture,
  ids: glow::Texture,
  depth: glow::Renderbuffer,
  size: (u32, u32),
}

/// A GPU renderer bound to one OpenGL context.
///
/// [`Renderer::set_scene`] uploads a scene, [`Renderer::draw`] renders it from a camera into an
/// offscreen target, [`Renderer::composite`] puts that image into the current framebuffer and
/// [`Renderer::read_ids`] / [`Renderer::read_rgba`] read it back. Every method must be called
/// with the context it was created with, current on this thread; [`Renderer::destroy`] frees the
/// GPU objects.
pub struct Renderer {
  mesh: Prog,
  line: Prog,
  marker: Prog,
  background: Prog,
  composite: Prog,
  /// Corner offsets for the instanced quads (4 vertices of a triangle strip).
  quad: glow::Buffer,
  empty_vao: glow::VertexArray,
  meshes: Vec<GpuMesh>,
  lines: Option<GpuInstances>,
  markers: Option<GpuInstances>,
  background_colors: [[u8; 4]; 2],
  analysis: Option<Analysis>,
  style: VisualStyle,
  target: Option<Target>,
}

fn unit(c: [u8; 4]) -> [f32; 4] {
  c.map(|v| v as f32 / 255.0)
}

impl Renderer {
  /// Compiles the shaders. Fails (and the caller falls back to the CPU rasterizer) without
  /// OpenGL 3.3 or OpenGL ES 3.0, or when a shader doesn't compile.
  pub fn new(gl: &Gl) -> Result<Renderer, String> {
    let v = gl.version();
    let header = if v.is_embedded {
      if v.major < 3 {
        return Err(format!("OpenGL ES {}.{} is too old (3.0 needed)", v.major, v.minor));
      }
      "#version 300 es\nprecision highp float;\nprecision highp int;\n"
    } else {
      if (v.major, v.minor) < (3, 3) {
        return Err(format!("OpenGL {}.{} is too old (3.3 needed)", v.major, v.minor));
      }
      "#version 330 core\n"
    };
    let mvp = ["u_vp", "u_size"];
    let mesh = program(
      gl,
      header,
      MESH_VS,
      MESH_FS,
      &[
        "u_vp",
        "u_key",
        "u_head",
        "u_fill",
        "u_eye",
        "u_persp",
        "u_color",
        "u_hl_color",
        "u_hl",
        "u_has_hl",
        "u_vcolor",
        "u_mode",
        "u_param",
      ],
    )?;
    let line = program(gl, header, LINE_VS, LINE_FS, &["u_vp", "u_size", "u_hidden", "u_hidden_alpha"])?;
    let marker = program(gl, header, MARKER_VS, MARKER_FS, &mvp)?;
    let background = program(gl, header, FULLSCREEN_VS, BACKGROUND_FS, &["u_size", "u_top", "u_bottom"])?;
    let composite = program(gl, header, FULLSCREEN_VS, COMPOSITE_FS, &["u_tex", "u_tint"])?;
    unsafe {
      let quad = gl.create_buffer()?;
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad));
      let corners: [f32; 8] = [0.0, -1.0, 1.0, -1.0, 0.0, 1.0, 1.0, 1.0];
      gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(&corners), glow::STATIC_DRAW);
      gl.bind_buffer(glow::ARRAY_BUFFER, None);
      let empty_vao = gl.create_vertex_array()?;
      let r = Renderer {
        mesh,
        line,
        marker,
        background,
        composite,
        quad,
        empty_vao,
        meshes: Vec::new(),
        lines: None,
        markers: None,
        background_colors: [[0, 0, 0, 255]; 2],
        analysis: None,
        style: VisualStyle::default(),
        target: None,
      };
      Ok(r)
    }
  }

  /// Uploads a scene. With `reuse_meshes`, mesh buffers from the previous scene are kept when
  /// the new one has the same meshes (same vertex and index counts): only their colours,
  /// highlights and analysis flags are taken from it. Callers pass it when only the hover
  /// changed, so hovering over a large model doesn't re-upload it.
  pub fn set_scene(&mut self, gl: &Gl, scene: &Scene, reuse_meshes: bool) -> Result<(), String> {
    let same = reuse_meshes
      && self.meshes.len() == scene.meshes.len()
      && self.meshes.iter().zip(&scene.meshes).all(|(g, m)| g.shape == shape(m));
    if same {
      for (g, m) in self.meshes.iter_mut().zip(&scene.meshes) {
        g.color = m.color;
        g.highlight = m.highlight;
        g.highlight_color = m.highlight_color;
        g.analyze = m.analyze;
      }
    } else {
      for m in self.meshes.drain(..) {
        free_mesh(gl, m);
      }
      for m in &scene.meshes {
        let g = upload_mesh(gl, m)?;
        self.meshes.push(g);
      }
    }
    if let Some(l) = self.lines.take() {
      free_instances(gl, l);
    }
    if let Some(m) = self.markers.take() {
      free_instances(gl, m);
    }
    self.lines = Some(upload_instances(gl, self.quad, &pack::lines(&scene.lines), LINE_STRIDE, line_attribs)?);
    self.markers =
      Some(upload_instances(gl, self.quad, &pack::markers(&scene.markers), MARKER_STRIDE, marker_attribs)?);
    self.background_colors = [scene.background_top, scene.background_bottom];
    self.analysis = scene.analysis;
    self.style = scene.style;
    Ok(())
  }

  /// Renders the uploaded scene into the offscreen target (resized to `width` × `height`), in the
  /// CPU rasterizer's order: background, opaque meshes, lines and markers, transparent meshes,
  /// then what's drawn on top.
  pub fn draw(&mut self, gl: &Gl, cam: &Camera, width: u32, height: u32) -> Result<(), String> {
    let (w, h) = (width.clamp(1, fox_render::MAX_DIM), height.clamp(1, fox_render::MAX_DIM));
    self.ensure_target(gl, w, h)?;
    let Some(t) = &self.target else { return Err("no render target".into()) };
    let vp = cam.view_proj(w, h).as_mat4().to_cols_array();
    let size = [w as f32, h as f32];
    let (right, up, back) = cam.basis();
    let key = (back * 0.8 + up * 0.5 - right * 0.3).normalize().as_vec3();
    let head = back.as_vec3();
    let fill = (right * 0.7 - up * 0.2 + back * 0.3).normalize().as_vec3();
    let eye = cam.eye().as_vec3();
    unsafe {
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.fbo));
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
      gl.viewport(0, 0, w as i32, h as i32);
      gl.disable(glow::SCISSOR_TEST);
      gl.disable(glow::CULL_FACE);
      gl.disable(glow::BLEND);
      gl.disable(glow::DITHER);
      gl.color_mask(true, true, true, true);
      gl.depth_mask(true);
      gl.clear_depth_f32(1.0);
      gl.clear(glow::DEPTH_BUFFER_BIT);

      // background (and id 0 everywhere)
      gl.disable(glow::DEPTH_TEST);
      let p = &self.background;
      gl.use_program(Some(p.program));
      gl.uniform_2_f32(p.loc("u_size"), size[0], size[1]);
      let [top, bottom] = self.background_colors.map(unit);
      gl.uniform_4_f32_slice(p.loc("u_top"), &top);
      gl.uniform_4_f32_slice(p.loc("u_bottom"), &bottom);
      gl.bind_vertex_array(Some(self.empty_vao));
      gl.draw_arrays(glow::TRIANGLES, 0, 3);

      // meshes
      gl.enable(glow::DEPTH_TEST);
      let p = &self.mesh;
      gl.use_program(Some(p.program));
      gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, &vp);
      gl.uniform_3_f32_slice(p.loc("u_key"), &key.to_array());
      gl.uniform_3_f32_slice(p.loc("u_head"), &head.to_array());
      gl.uniform_3_f32_slice(p.loc("u_fill"), &fill.to_array());
      gl.uniform_3_f32_slice(p.loc("u_eye"), &eye.to_array());
      gl.uniform_1_i32(p.loc("u_persp"), !cam.ortho as i32);
      gl.depth_func(glow::LESS);
      let style = self.style;
      let bodies = (!style.shaded()).then_some(false);
      self.draw_meshes(gl, false, bodies);
      if !style.shaded() && style.occludes() {
        // wireframe surfaces only hide what's behind them
        gl.color_mask(false, false, false, false);
        self.draw_meshes(gl, false, Some(true));
        gl.color_mask(true, true, true, true);
      }

      // lines and markers blend over what's there; the id attachment keeps its alpha of 1
      gl.enable(glow::BLEND);
      gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ZERO);
      if style.hidden_edges() {
        // the edges behind the surfaces, before any line writes depth
        gl.depth_func(glow::GREATER);
        gl.depth_mask(false);
        self.draw_hidden_edges(gl, size, &vp);
        gl.depth_mask(true);
      }
      gl.depth_func(glow::LEQUAL);
      self.draw_instances(gl, false, size, &vp);

      // transparent meshes test against the depth but don't write it
      gl.depth_func(glow::LESS);
      gl.depth_mask(false);
      let p = &self.mesh;
      gl.use_program(Some(p.program));
      self.draw_meshes(gl, true, bodies);

      // sketch geometry in sketch mode and other on-top overlays
      gl.disable(glow::DEPTH_TEST);
      self.draw_instances(gl, true, size, &vp);

      gl.depth_mask(true);
      gl.disable(glow::BLEND);
      gl.bind_vertex_array(None);
      gl.use_program(None);
      gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    }
    Ok(())
  }

  /// The opaque or the transparent meshes: with `bodies` set, only the body surfaces (true) or
  /// all but them (false).
  fn draw_meshes(&self, gl: &Gl, transparent: bool, bodies: Option<bool>) {
    let p = &self.mesh;
    unsafe {
      let wanted = |m: &&GpuMesh| (m.color[3] < 255) == transparent && bodies.is_none_or(|b| m.analyze == b);
      for m in self.meshes.iter().filter(wanted) {
        let (mode, param) = match self.analysis.filter(|_| m.analyze) {
          None => (0, [0.0; 4]),
          Some(Analysis::Zebra { stripes }) => (1, [stripes, 0.0, 0.0, 0.0]),
          Some(Analysis::Draft { pull, angle }) => {
            let d = Vec3::from(pull).normalize_or_zero();
            (2, [d.x, d.y, d.z, angle.sin()])
          }
          Some(Analysis::Accessibility { dir }) => {
            let d = Vec3::from(dir).normalize_or_zero();
            (3, [d.x, d.y, d.z, 0.0])
          }
        };
        gl.uniform_4_f32_slice(p.loc("u_color"), &unit(m.color));
        gl.uniform_4_f32_slice(p.loc("u_hl_color"), &unit(m.highlight_color));
        gl.uniform_1_u32(p.loc("u_hl"), m.highlight.unwrap_or(0));
        gl.uniform_1_i32(p.loc("u_has_hl"), m.highlight.is_some() as i32);
        gl.uniform_1_i32(p.loc("u_vcolor"), m.vertex_colors as i32);
        gl.uniform_1_i32(p.loc("u_mode"), mode);
        gl.uniform_4_f32_slice(p.loc("u_param"), &param);
        gl.bind_vertex_array(Some(m.vao));
        gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
      }
    }
  }

  /// Lines, then markers, of one layer (`on_top` or not). Things without a pick id leave the id
  /// attachment alone, as the CPU path leaves `Frame::ids`.
  fn draw_instances(&self, gl: &Gl, on_top: bool, size: [f32; 2], vp: &[f32; 16]) {
    let kinds = [
      (&self.line, &self.lines, LINE_STRIDE, line_attribs as AttribFn),
      (&self.marker, &self.markers, MARKER_STRIDE, marker_attribs),
    ];
    unsafe {
      for (p, inst, stride, attribs) in kinds {
        let Some(inst) = inst else { continue };
        gl.use_program(Some(p.program));
        gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, vp);
        gl.uniform_2_f32(p.loc("u_size"), size[0], size[1]);
        gl.bind_vertex_array(Some(inst.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(inst.vbo));
        for has_id in [false, true] {
          let range = &inst.groups[on_top as usize * 2 + has_id as usize];
          if range.is_empty() {
            continue;
          }
          if has_id {
            gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
          } else {
            gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::NONE]);
          }
          attribs(gl, (range.start * stride * 4) as i32);
          gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, range.len() as i32);
        }
        gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
      }
    }
  }

  /// The depth-tested edges (lines with pick ids) as hidden edges, with the depth test the caller
  /// set; colour only.
  fn draw_hidden_edges(&self, gl: &Gl, size: [f32; 2], vp: &[f32; 16]) {
    let (p, Some(inst)) = (&self.line, &self.lines) else { return };
    let range = &inst.groups[1];
    if range.is_empty() {
      return;
    }
    unsafe {
      gl.use_program(Some(p.program));
      gl.uniform_matrix_4_f32_slice(p.loc("u_vp"), false, vp);
      gl.uniform_2_f32(p.loc("u_size"), size[0], size[1]);
      gl.uniform_1_i32(p.loc("u_hidden"), 1);
      gl.uniform_1_f32(p.loc("u_hidden_alpha"), fox_render::HIDDEN_EDGE_ALPHA);
      gl.bind_vertex_array(Some(inst.vao));
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(inst.vbo));
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::NONE]);
      line_attribs(gl, (range.start * LINE_STRIDE * 4) as i32);
      gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, range.len() as i32);
      gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
      gl.uniform_1_i32(p.loc("u_hidden"), 0);
      gl.bind_buffer(glow::ARRAY_BUFFER, None);
    }
  }

  fn ensure_target(&mut self, gl: &Gl, w: u32, h: u32) -> Result<(), String> {
    if self.target.as_ref().is_some_and(|t| t.size == (w, h)) {
      return Ok(());
    }
    if let Some(t) = self.target.take() {
      free_target(gl, t);
    }
    unsafe {
      let tex = |filter: u32| -> Result<glow::Texture, String> {
        let t = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        gl.tex_image_2d(
          glow::TEXTURE_2D,
          0,
          glow::RGBA8 as i32,
          w as i32,
          h as i32,
          0,
          glow::RGBA,
          glow::UNSIGNED_BYTE,
          glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        Ok(t)
      };
      let color = tex(glow::LINEAR)?;
      let ids = tex(glow::NEAREST)?;
      gl.bind_texture(glow::TEXTURE_2D, None);
      let depth = gl.create_renderbuffer()?;
      gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
      gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT32F, w as i32, h as i32);
      gl.bind_renderbuffer(glow::RENDERBUFFER, None);
      let fbo = gl.create_framebuffer()?;
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT1, glow::TEXTURE_2D, Some(ids), 0);
      gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth));
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, None);
      let t = Target { fbo, color, ids, depth, size: (w, h) };
      if status != glow::FRAMEBUFFER_COMPLETE {
        free_target(gl, t);
        return Err(format!("the offscreen framebuffer is incomplete (status {status:#x})"));
      }
      self.target = Some(t);
    }
    Ok(())
  }

  /// Size of the last drawn image.
  pub fn size(&self) -> Option<(u32, u32)> {
    self.target.as_ref().map(|t| t.size)
  }

  /// Draws the last image stretched over `viewport` (x, y from the bottom, width, height in
  /// pixels) of `target` (`None` = the default framebuffer), clipped to `scissor` and multiplied
  /// by `tint`. Leaves the state egui's painter expects: scissor on, depth test off, blending on.
  pub fn composite(
    &self,
    gl: &Gl,
    target: Option<glow::Framebuffer>,
    viewport: [i32; 4],
    scissor: [i32; 4],
    tint: [f32; 4],
  ) {
    let Some(t) = &self.target else { return };
    let p = &self.composite;
    unsafe {
      gl.bind_framebuffer(glow::FRAMEBUFFER, target);
      gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
      gl.enable(glow::SCISSOR_TEST);
      gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
      gl.disable(glow::DEPTH_TEST);
      gl.disable(glow::BLEND);
      gl.use_program(Some(p.program));
      gl.active_texture(glow::TEXTURE0);
      gl.bind_texture(glow::TEXTURE_2D, Some(t.color));
      gl.uniform_1_i32(p.loc("u_tex"), 0);
      gl.uniform_4_f32_slice(p.loc("u_tint"), &tint);
      gl.bind_vertex_array(Some(self.empty_vao));
      gl.draw_arrays(glow::TRIANGLES, 0, 3);
      gl.bind_vertex_array(None);
      gl.bind_texture(glow::TEXTURE_2D, None);
      gl.use_program(None);
      gl.enable(glow::BLEND);
    }
  }

  fn read(&self, gl: &Gl, attachment: u32) -> Option<(u32, u32, Vec<u8>)> {
    let t = self.target.as_ref()?;
    let (w, h) = t.size;
    let mut px = vec![0u8; (w * h * 4) as usize];
    unsafe {
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(t.fbo));
      gl.read_buffer(attachment);
      gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
      gl.read_pixels(
        0,
        0,
        w as i32,
        h as i32,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelPackData::Slice(Some(&mut px)),
      );
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
    }
    // OpenGL rows run bottom-up; frames run top-down
    let row = (w * 4) as usize;
    let mut out = Vec::with_capacity(px.len());
    for r in px.chunks_exact(row).rev() {
      out.extend_from_slice(r);
    }
    Some((w, h, out))
  }

  /// The pick ids of the last image as a frame (`rgba` and `depth` left empty), for
  /// [`Frame::pick`] and window selection.
  pub fn read_ids(&self, gl: &Gl) -> Option<Frame> {
    let (w, h, px) = self.read(gl, glow::COLOR_ATTACHMENT1)?;
    let ids = px.chunks_exact(4).map(|p| u32::from_le_bytes([p[0], p[1], p[2], 0])).collect();
    Some(Frame { width: w, height: h, rgba: Vec::new(), depth: Vec::new(), ids })
  }

  /// The last image, top-down RGBA.
  pub fn read_rgba(&self, gl: &Gl) -> Option<Vec<u8>> {
    let (_, _, mut px) = self.read(gl, glow::COLOR_ATTACHMENT0)?;
    for p in px.chunks_exact_mut(4) {
      p[3] = 255;
    }
    Some(px)
  }

  /// Renders a scene into a complete [`Frame`] (colour and ids), like [`fox_render::render`].
  pub fn render(&mut self, gl: &Gl, scene: &Scene, cam: &Camera, width: u32, height: u32) -> Result<Frame, String> {
    self.set_scene(gl, scene, false)?;
    self.draw(gl, cam, width, height)?;
    let mut f = self.read_ids(gl).ok_or("nothing drawn")?;
    f.rgba = self.read_rgba(gl).ok_or("nothing drawn")?;
    f.depth = vec![f32::INFINITY; f.ids.len()];
    debug_assert!(f.ids.iter().all(|&i| i == NO_ID || i < crate::MAX_ID));
    Ok(f)
  }

  /// Frees every GPU object.
  pub fn destroy(mut self, gl: &Gl) {
    for m in self.meshes.drain(..) {
      free_mesh(gl, m);
    }
    for i in [self.lines.take(), self.markers.take()].into_iter().flatten() {
      free_instances(gl, i);
    }
    if let Some(t) = self.target.take() {
      free_target(gl, t);
    }
    unsafe {
      for p in [&self.mesh, &self.line, &self.marker, &self.background, &self.composite] {
        gl.delete_program(p.program);
      }
      gl.delete_buffer(self.quad);
      gl.delete_vertex_array(self.empty_vao);
    }
  }
}

fn shape(m: &fox_render::Mesh) -> (usize, usize, usize) {
  (m.positions.len(), m.indices.len(), m.tri_ids.len())
}

fn program(gl: &Gl, header: &str, vs: &str, fs: &str, uniforms: &[&'static str]) -> Result<Prog, String> {
  unsafe {
    let program = gl.create_program()?;
    let mut shaders = Vec::new();
    for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
      let s = gl.create_shader(kind)?;
      gl.shader_source(s, &format!("{header}{}", src.replace("ID_COLOR", ID_COLOR)));
      gl.compile_shader(s);
      if !gl.get_shader_compile_status(s) {
        let log = gl.get_shader_info_log(s);
        gl.delete_shader(s);
        gl.delete_program(program);
        return Err(format!("shader compile error: {log}"));
      }
      gl.attach_shader(program, s);
      shaders.push(s);
    }
    gl.link_program(program);
    for s in shaders {
      gl.detach_shader(program, s);
      gl.delete_shader(s);
    }
    if !gl.get_program_link_status(program) {
      let log = gl.get_program_info_log(program);
      gl.delete_program(program);
      return Err(format!("shader link error: {log}"));
    }
    let uniforms = uniforms.iter().map(|n| (*n, gl.get_uniform_location(program, n))).collect();
    Ok(Prog { program, uniforms })
  }
}

fn upload_mesh(gl: &Gl, m: &fox_render::Mesh) -> Result<GpuMesh, String> {
  let d = pack::mesh(m);
  let stride = (MESH_STRIDE * 4) as i32;
  unsafe {
    let vao = gl.create_vertex_array()?;
    let vbo = gl.create_buffer()?;
    let ebo = gl.create_buffer()?;
    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(&d.verts), glow::STATIC_DRAW);
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytemuck::cast_slice(&d.indices), glow::STATIC_DRAW);
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);
    gl.enable_vertex_attrib_array(2);
    gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, stride, 24);
    gl.enable_vertex_attrib_array(3);
    gl.vertex_attrib_pointer_i32(3, 1, glow::UNSIGNED_INT, stride, 28);
    gl.bind_vertex_array(None);
    gl.bind_buffer(glow::ARRAY_BUFFER, None);
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
    Ok(GpuMesh {
      vao,
      vbo,
      ebo,
      count: d.indices.len() as i32,
      shape: shape(m),
      color: m.color,
      highlight: m.highlight,
      highlight_color: m.highlight_color,
      vertex_colors: !m.colors.is_empty(),
      analyze: m.analyze,
    })
  }
}

fn free_mesh(gl: &Gl, m: GpuMesh) {
  unsafe {
    gl.delete_vertex_array(m.vao);
    gl.delete_buffer(m.vbo);
    gl.delete_buffer(m.ebo);
  }
}

/// Points a VAO's per-instance attributes at the instance data, starting `offset` bytes in.
type AttribFn = fn(&Gl, i32);

fn line_attribs(gl: &Gl, offset: i32) {
  let s = (LINE_STRIDE * 4) as i32;
  unsafe {
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, s, offset);
    gl.vertex_attrib_pointer_f32(2, 3, glow::FLOAT, false, s, offset + 12);
    gl.vertex_attrib_pointer_f32(3, 4, glow::UNSIGNED_BYTE, true, s, offset + 24);
    gl.vertex_attrib_pointer_i32(4, 1, glow::UNSIGNED_INT, s, offset + 28);
    gl.vertex_attrib_pointer_f32(5, 3, glow::FLOAT, false, s, offset + 32);
  }
}

fn marker_attribs(gl: &Gl, offset: i32) {
  let s = (MARKER_STRIDE * 4) as i32;
  unsafe {
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, s, offset);
    gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, s, offset + 12);
    gl.vertex_attrib_pointer_i32(3, 1, glow::UNSIGNED_INT, s, offset + 16);
    gl.vertex_attrib_pointer_f32(4, 1, glow::FLOAT, false, s, offset + 20);
  }
}

fn upload_instances(
  gl: &Gl,
  quad: glow::Buffer,
  inst: &pack::Instances,
  stride: usize,
  attribs: AttribFn,
) -> Result<GpuInstances, String> {
  let n_attribs = if stride == LINE_STRIDE { 5 } else { 4 };
  unsafe {
    let vao = gl.create_vertex_array()?;
    let vbo = gl.create_buffer()?;
    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad));
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(&inst.data), glow::STATIC_DRAW);
    for a in 1..=n_attribs {
      gl.enable_vertex_attrib_array(a);
      gl.vertex_attrib_divisor(a, 1);
    }
    attribs(gl, 0);
    gl.bind_vertex_array(None);
    gl.bind_buffer(glow::ARRAY_BUFFER, None);
    debug_assert_eq!(inst.data.len() % stride, 0);
    Ok(GpuInstances { vao, vbo, groups: inst.groups.clone() })
  }
}

fn free_instances(gl: &Gl, i: GpuInstances) {
  unsafe {
    gl.delete_vertex_array(i.vao);
    gl.delete_buffer(i.vbo);
  }
}

fn free_target(gl: &Gl, t: Target) {
  unsafe {
    gl.delete_framebuffer(t.fbo);
    gl.delete_texture(t.color);
    gl.delete_texture(t.ids);
    gl.delete_renderbuffer(t.depth);
  }
}
