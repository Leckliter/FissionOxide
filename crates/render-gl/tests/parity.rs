//! The GPU renderer against the CPU rasterizer: the same scene must give the same picture and the
//! same pick ids, up to rasterization differences along edges. Skipped (with a note) where no
//! OpenGL context can be created.

use fox_render::{
  Analysis, Camera, Dome, Frame, Marker, Mesh, NO_ID, Polyline, Post, Scene, StandardView, VisualStyle,
};
use fox_render_gl::Renderer;

/// A UV sphere with one pick id per latitude band, so faces meet along seams like a B-rep body.
fn sphere(center: [f32; 3], r: f32, n: u32, id0: u32, color: [u8; 4]) -> Mesh {
  let mut m = Mesh { id: id0, color, analyze: true, ..Default::default() };
  for i in 0..=n {
    let th = std::f32::consts::PI * i as f32 / n as f32;
    for j in 0..=n * 2 {
      let ph = std::f32::consts::PI * j as f32 / n as f32;
      let d = [th.sin() * ph.cos(), th.sin() * ph.sin(), th.cos()];
      m.positions.push([center[0] + r * d[0], center[1] + r * d[1], center[2] + r * d[2]]);
      m.normals.push(d);
      m.colors.push([(i * 255 / n) as u8, 120, (j * 127 / n) as u8, 255]);
    }
  }
  let row = n * 2 + 1;
  for i in 0..n {
    for j in 0..n * 2 {
      let (a, b) = (i * row + j, (i + 1) * row + j);
      m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
      let band = id0 + i / 4;
      m.tri_ids.extend_from_slice(&[band, band]);
    }
  }
  m
}

fn quad(z: f32, half: f32, color: [u8; 4], id: u32) -> Mesh {
  Mesh {
    positions: vec![[-half, -half, z], [half, -half, z], [half, half, z], [-half, half, z]],
    normals: vec![[0.0, 0.0, 1.0]; 4],
    indices: vec![0, 1, 2, 0, 2, 3],
    id,
    color,
    analyze: true,
    ..Default::default()
  }
}

fn line(points: Vec<[f32; 3]>, id: u32, width: f32, on_top: bool, dashed: bool) -> Polyline {
  Polyline { points, color: [35, 38, 44, 255], width, id, on_top, dashed, dash_dot: false, behind: false }
}

fn scene() -> Scene {
  let mut s = sphere([0.0, 0.0, 10.0], 10.0, 24, 100, [186, 190, 196, 255]);
  s.highlight = Some(102);
  s.highlight_color = [128, 190, 238, 255];
  let mut ring = Vec::new();
  for k in 0..=64 {
    let a = std::f32::consts::TAU * k as f32 / 64.0;
    ring.push([10.0 * a.cos(), 10.0 * a.sin(), 10.0]);
  }
  Scene {
    meshes: vec![quad(0.0, 30.0, [200, 180, 150, 255], 7), s, quad(25.0, 6.0, [40, 140, 220, 120], 8)],
    lines: vec![
      line(ring, 50, 1.0, false, false),
      line(vec![[-30.0, -30.0, 0.0], [30.0, -30.0, 0.0], [30.0, 30.0, 0.0]], 51, 3.0, false, false),
      line(vec![[-30.0, 30.0, 0.0], [-30.0, -30.0, 0.0]], 0, 1.5, false, true),
      line(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 40.0]], 52, 2.0, true, false),
      Polyline { behind: true, ..line(vec![[-40.0, 0.0, 0.0], [40.0, 0.0, 0.0]], 0, 1.0, false, false) },
    ],
    markers: vec![
      Marker { at: [30.0, 30.0, 0.0], color: [200, 140, 40, 255], size: 5.0, id: 60, on_top: true, hollow: false },
      Marker { at: [-30.0, -30.0, 0.0], color: [30, 110, 235, 255], size: 3.0, id: 0, on_top: false, hollow: true },
    ],
    background_top: [250, 250, 251, 255],
    background_bottom: [234, 236, 239, 255],
    analysis: None,
    analysis_fade: 0.0,
    style: VisualStyle::default(),
    post: Default::default(),
  }
}

struct Diff {
  color: f64,
  ids: f64,
}

fn compare(a: &Frame, b: &Frame) -> Diff {
  let n = a.ids.len();
  assert_eq!((a.width, a.height), (b.width, b.height));
  let color =
    (0..n).filter(|&i| (0..3).any(|k| (a.rgba[i * 4 + k] as i32 - b.rgba[i * 4 + k] as i32).abs() > 12)).count() as f64
      / n as f64;
  let ids = (0..n).filter(|&i| a.ids[i] != b.ids[i]).count() as f64 / n as f64;
  Diff { color, ids }
}

fn cams() -> Vec<(&'static str, Camera)> {
  let mut out = vec![("home", Camera { distance: 120.0, ..Camera::default() })];
  let mut top = Camera { distance: 120.0, ortho: true, ..Camera::default() };
  top.set_view(StandardView::Top);
  out.push(("top-ortho", top));
  let mut front = Camera { distance: 90.0, ..Camera::default() };
  front.set_view(StandardView::Front);
  out.push(("front", front));
  // the eye inside the geometry: near-plane clipping of triangles and lines
  out.push(("inside", Camera { distance: 4.0, target: [0.0, 0.0, 10.0], ..Camera::default() }));
  out
}

#[test]
fn gpu_matches_cpu() {
  let h = match fox_render_gl::headless::context() {
    Ok(h) => h,
    Err(e) => {
      eprintln!("skipped: no OpenGL context ({e})");
      return;
    }
  };
  let mut r = match Renderer::new(&h.gl) {
    Ok(r) => r,
    Err(e) => {
      eprintln!("skipped: {e}");
      return;
    }
  };
  let analyses = [
    None,
    Some(Analysis::Zebra { stripes: 12.0, vertical: false }),
    Some(Analysis::Zebra { stripes: 12.0, vertical: true }),
    Some(Analysis::Draft { pull: [0.0, 0.0, 1.0], lo: -0.2, hi: 0.2, tol: 0.05 }),
    Some(Analysis::Accessibility { dir: [0.0, 0.0, 1.0] }),
    Some(Analysis::Environment { rotate: 0.5, mirror: true, sharp: true }),
  ];
  for analysis in analyses {
    let mut s = scene();
    s.analysis = analysis;
    if analysis.is_some() {
      // vertex colours (curvature) and analysis don't mix in the app
      s.meshes[1].colors.clear();
    }
    for (name, cam) in cams() {
      let cpu = fox_render::render(&s, &cam, 320, 240);
      let gpu = r.render(&h.gl, &s, &cam, 320, 240).expect("gpu render");
      let d = compare(&cpu, &gpu);
      eprintln!("{name} {analysis:?}: {:.2}% colour, {:.2}% ids differ", d.color * 100.0, d.ids * 100.0);
      if std::env::var_os("FOX_PARITY_PNG").is_some() {
        let _ = cpu.save_png(std::path::Path::new(&format!("/tmp/parity-{name}-cpu.png")));
        let _ = gpu.save_png(std::path::Path::new(&format!("/tmp/parity-{name}-gpu.png")));
      }
      assert!(d.color < 0.03, "{name} {analysis:?}: {:.2}% of pixels differ in colour", d.color * 100.0);
      assert!(d.ids < 0.02, "{name} {analysis:?}: {:.2}% of pixels differ in pick id", d.ids * 100.0);
    }
  }
  // the visual styles: wireframe surfaces, and hidden edges drawn faint and dashed
  for style in VisualStyle::ALL {
    let s = Scene { style, ..scene() };
    for (name, cam) in cams() {
      let cpu = fox_render::render(&s, &cam, 320, 240);
      let gpu = r.render(&h.gl, &s, &cam, 320, 240).expect("gpu render");
      let d = compare(&cpu, &gpu);
      eprintln!("{name} {style:?}: {:.2}% colour, {:.2}% ids differ", d.color * 100.0, d.ids * 100.0);
      assert!(d.color < 0.03, "{name} {style:?}: {:.2}% of pixels differ in colour", d.color * 100.0);
      assert!(d.ids < 0.02, "{name} {style:?}: {:.2}% of pixels differ in pick id", d.ids * 100.0);
    }
  }
  r.destroy(&h.gl);
}

/// An appearance's texture over a body looks the same on the GPU, and follows the hover.
#[test]
fn textures_match_cpu() {
  let Ok(h) = fox_render_gl::headless::context() else { return };
  let Ok(mut r) = Renderer::new(&h.gl) else { return };
  // a coarse 4 x 4 checker (soft edges would differ by the GPU's mipmaps only at the seams)
  let n = 4u32;
  let rgba =
    (0..n * n).flat_map(|i| if (i % n + i / n).is_multiple_of(2) { [230, 190, 120, 255] } else { [90, 60, 30, 255] });
  let tile = std::sync::Arc::new(fox_render::Tile { key: "checker".into(), width: n, height: n, rgba: rgba.collect() });
  let mut s = scene();
  for (m, kind) in [(0, 1), (1, 0)] {
    let projection = fox_render::TexProjection { kind, scale: 40.0, rotation: 20.0, ..Default::default() };
    s.meshes[m].texture = Some(fox_render::MeshTexture { tile: tile.clone(), projection });
    s.meshes[m].colors.clear();
  }
  for (name, cam) in cams() {
    let cpu = fox_render::render(&s, &cam, 320, 240);
    let gpu = r.render(&h.gl, &s, &cam, 320, 240).expect("gpu render");
    let d = compare(&cpu, &gpu);
    eprintln!("{name} textured: {:.2}% colour, {:.2}% ids differ", d.color * 100.0, d.ids * 100.0);
    assert!(d.color < 0.05, "{name} textured: {:.2}% of pixels differ in colour", d.color * 100.0);
  }
  r.destroy(&h.gl);
}

#[test]
fn picks_are_exact_and_hover_reuses_meshes() {
  let Ok(h) = fox_render_gl::headless::context() else { return };
  let Ok(mut r) = Renderer::new(&h.gl) else { return };
  let mut cam = Camera { distance: 120.0, ortho: true, ..Camera::default() };
  cam.set_view(StandardView::Top);
  let s = scene();
  let f = r.render(&h.gl, &s, &cam, 400, 300).expect("render");
  // the transparent quad on top, the on-top axis and marker, the sphere's top band, the base
  assert_eq!(f.pick(200.0, 150.0, 0, |_| true), 52, "the on-top axis covers the centre");
  assert_eq!(f.pick(205.0, 150.0, 0, |_| true), 8, "the transparent quad still picks");
  assert_eq!(f.pick(2.0, 2.0, 0, |_| true), NO_ID);
  let cpu = fox_render::render(&s, &cam, 400, 300);
  for (x, y) in [(200.0, 150.0), (205.0, 150.0), (230.0, 150.0), (300.0, 150.0), (60.0, 40.0)] {
    assert_eq!(f.pick(x, y, 0, |_| true), cpu.pick(x, y, 0, |_| true), "at {x},{y}");
  }
  // hover change only: the meshes stay, the highlight follows
  let mut hovered = s.clone();
  hovered.meshes[1].highlight = Some(101);
  r.set_scene(&h.gl, &hovered, true).expect("set");
  r.draw(&h.gl, &cam, 400, 300).expect("draw");
  let px = r.read_rgba(&h.gl).expect("rgba");
  let cpu = fox_render::render(&hovered, &cam, 400, 300);
  let same =
    px.chunks_exact(4).zip(cpu.rgba.chunks_exact(4)).filter(|(a, b)| (0..3).all(|k| a[k].abs_diff(b[k]) <= 12));
  assert!(same.count() as f64 / (400.0 * 300.0) > 0.97);
  r.destroy(&h.gl);
}

/// An assembly-like scene: `n` × `n` spheres, each its own body with edges around its equator.
fn assembly(n: u32) -> Scene {
  let mut s = Scene { background_top: [250, 250, 251, 255], background_bottom: [234, 236, 239, 255], ..scene() };
  s.meshes.clear();
  s.lines.clear();
  s.markers.clear();
  for i in 0..n {
    for j in 0..n {
      let c = [i as f32 * 30.0, j as f32 * 30.0, 10.0];
      let id = 1000 + (i * n + j) * 10;
      let mut m = sphere(c, 10.0, 24, id, [186, 190, 196, 255]);
      m.colors.clear();
      s.meshes.push(m);
      let ring = (0..=48)
        .map(|k| {
          let a = std::f32::consts::TAU * k as f32 / 48.0;
          [c[0] + 10.0 * a.cos(), c[1] + 10.0 * a.sin(), c[2]]
        })
        .collect();
      s.lines.push(line(ring, id + 9, 1.0, false, false));
    }
  }
  s
}

fn frame_of(r: &mut Renderer, gl: &fox_render_gl::glow::Context) -> Frame {
  let mut f = r.read_ids(gl).expect("ids");
  f.rgba = r.read_rgba(gl).expect("rgba");
  f
}

#[test]
fn culling_leaves_out_what_is_off_screen_and_changes_nothing() {
  let Ok(h) = fox_render_gl::headless::context() else { return };
  let Ok(mut r) = Renderer::new(&h.gl) else { return };
  let s = assembly(10);
  r.set_scene(&h.gl, &s, false).expect("set");
  // close in on one corner of the assembly
  let cam = Camera { target: [0.0, 0.0, 10.0], distance: 60.0, ..Camera::default() };
  for style in [VisualStyle::ShadedVisibleEdges, VisualStyle::HiddenLine] {
    r.set_scene(&h.gl, &Scene { style, ..s.clone() }, true).expect("set");
    r.culling = false;
    r.draw(&h.gl, &cam, 320, 240).expect("draw");
    let all = r.stats();
    let whole = frame_of(&mut r, &h.gl);
    r.culling = true;
    r.draw(&h.gl, &cam, 320, 240).expect("draw");
    let culled = r.stats();
    let part = frame_of(&mut r, &h.gl);
    eprintln!("{style:?}: {all:?} → {culled:?}");
    assert_eq!(all.culled, 0);
    assert!(culled.culled > 50 && culled.triangles * 4 < all.triangles, "{culled:?}");
    assert!(culled.segments < all.segments);
    assert_eq!(whole.ids, part.ids, "{style:?}: the same picks");
    assert_eq!(whole.rgba, part.rgba, "{style:?}: the same picture");
  }
  // looking away from everything: nothing is drawn
  let mut away = Camera { target: [-500.0, -500.0, 10.0], distance: 50.0, ..Camera::default() };
  away.set_view(StandardView::Front);
  r.draw(&h.gl, &away, 320, 240).expect("draw");
  assert_eq!((r.stats().triangles, r.stats().chunks), (0, 0));
  r.destroy(&h.gl);
}

#[test]
fn levels_of_detail_while_navigating_and_exact_picks_after() {
  let Ok(h) = fox_render_gl::headless::context() else { return };
  let Ok(mut r) = Renderer::new(&h.gl) else { return };
  let s = assembly(10);
  r.set_scene(&h.gl, &s, false).expect("set");
  let mut cam = Camera::default();
  cam.fit(glam::DVec3::new(-10.0, -10.0, 0.0), glam::DVec3::new(280.0, 280.0, 20.0), 4.0 / 3.0);
  // far enough that each sphere is a few pixels across, its triangles well under one
  cam.distance *= 3.0;
  r.draw(&h.gl, &cam, 400, 300).expect("draw");
  let full = r.stats();
  let mut exact = r.read_ids(&h.gl).expect("ids");
  exact.rgba = r.read_rgba(&h.gl).expect("rgba");
  assert_eq!(full.coarse, 0);
  r.draw_navigating(&h.gl, &cam, 400, 300).expect("draw");
  let moving = r.stats();
  let rgba = r.read_rgba(&h.gl).expect("rgba");
  eprintln!("full {full:?}, navigating {moving:?}");
  assert!(moving.coarse > 50 && moving.triangles * 2 < full.triangles, "{moving:?}");
  // the coarse picture is close to the full one
  let n = exact.ids.len();
  let off = (0..n).filter(|&i| (0..3).any(|k| rgba[i * 4 + k].abs_diff(exact.rgba[i * 4 + k]) > 24)).count();
  assert!((off as f64) < n as f64 * 0.03, "{:.2}% differ", off as f64 * 100.0 / n as f64);
  // reading the ids redraws at full detail: the picks are the exact ones
  let ids = r.read_ids(&h.gl).expect("ids");
  assert_eq!(ids.ids, exact.ids);
  assert_eq!(r.stats(), full);
  // close up, every chunk in view is at full detail even while navigating
  let near = Camera { target: [0.0, 0.0, 10.0], distance: 40.0, ..Camera::default() };
  r.draw_navigating(&h.gl, &near, 400, 300).expect("draw");
  let close = r.stats();
  assert!(close.chunks > 0);
  assert!(close.coarse < close.chunks, "{close:?}");
  r.destroy(&h.gl);
}

#[test]
fn hidden_curves_are_dashed_along_their_whole_length() {
  let Ok(h) = fox_render_gl::headless::context() else { return };
  let Ok(mut r) = Renderer::new(&h.gl) else { return };
  // a ring of short segments under a face: hidden, it must show gaps, not one dash per segment
  let ring: Vec<[f32; 3]> = (0..=192)
    .map(|k| {
      let a = std::f32::consts::TAU * k as f32 / 192.0;
      [20.0 * a.cos(), 20.0 * a.sin(), 0.0]
    })
    .collect();
  let mut cam = Camera { distance: 120.0, ortho: true, ..Camera::default() };
  cam.set_view(StandardView::Top);
  let ink = |r: &mut Renderer, style| {
    let s = Scene {
      meshes: vec![quad(5.0, 30.0, [200, 200, 200, 255], 7)],
      lines: vec![line(ring.clone(), 9, 1.0, false, false)],
      background_top: [255; 4],
      background_bottom: [255; 4],
      style,
      ..Default::default()
    };
    let gpu = r.render(&h.gl, &s, &cam, 300, 300).expect("render");
    let cpu = fox_render::render(&s, &cam, 300, 300);
    let count = |f: &Frame| f.rgba.chunks_exact(4).filter(|p| p[0] < 200).count();
    (count(&gpu), count(&cpu))
  };
  let (solid, _) = ink(&mut r, VisualStyle::Wireframe);
  let (dashed, cpu_dashed) = ink(&mut r, VisualStyle::HiddenLine);
  let ratio = dashed as f64 / solid as f64;
  eprintln!("solid {solid}, dashed {dashed} (CPU {cpu_dashed})");
  // Fusion's pattern: 9 px dashes with 3 px gaps (fox_render::dash_gap)
  assert!((0.55..0.9).contains(&ratio), "dashed / solid = {ratio:.2}");
  assert!((dashed as f64 - cpu_dashed as f64).abs() < solid as f64 * 0.15);
  r.destroy(&h.gl);
}

/// The effects drawn into the picture (Environment Dome, Object Shadow, Ambient Occlusion,
/// Anti-Aliasing) look the same on the GPU as on the CPU, each alone and all together, and pick
/// as without them.
#[test]
fn effects_match_cpu() {
  let Ok(h) = fox_render_gl::headless::context() else { return };
  let Ok(mut r) = Renderer::new(&h.gl) else { return };
  let dome = Dome { zenith: [120, 150, 190, 255], horizon: [225, 228, 232, 255], ground: [150, 145, 140, 255] };
  let posts = [
    ("dome", Post { dome: Some(dome), ..Default::default() }),
    ("object shadow", Post { object_shadow: true, ..Default::default() }),
    ("ambient occlusion", Post { ambient_occlusion: true, ..Default::default() }),
    ("anti-aliasing", Post { anti_aliasing: true, ..Default::default() }),
    ("all", Post { dome: Some(dome), object_shadow: true, ambient_occlusion: true, anti_aliasing: true }),
  ];
  for (what, post) in posts {
    let s = Scene { post, ..scene() };
    let plain = fox_render::render(&scene(), &cams()[0].1, 320, 240);
    for (name, cam) in cams() {
      let cpu = fox_render::render(&s, &cam, 320, 240);
      let gpu = r.render(&h.gl, &s, &cam, 320, 240).expect("gpu render");
      let d = compare(&cpu, &gpu);
      eprintln!("{name} {what}: {:.2}% colour, {:.2}% ids differ", d.color * 100.0, d.ids * 100.0);
      assert!(d.color < 0.04, "{name} {what}: {:.2}% of pixels differ in colour", d.color * 100.0);
      assert!(d.ids < 0.02, "{name} {what}: {:.2}% of pixels differ in pick id", d.ids * 100.0);
    }
    let cpu = fox_render::render(&s, &cams()[0].1, 320, 240);
    assert_ne!(cpu.rgba, plain.rgba, "{what} changes the picture");
  }
  r.destroy(&h.gl);
}
