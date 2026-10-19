//! The GPU renderer against the CPU rasterizer: the same scene must give the same picture and the
//! same pick ids, up to rasterization differences along edges. Skipped (with a note) where no
//! OpenGL context can be created.

use clf_render::{Analysis, Camera, Frame, Marker, Mesh, NO_ID, Polyline, Scene, StandardView};
use clf_render_gl::Renderer;

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
  Polyline { points, color: [35, 38, 44, 255], width, id, on_top, dashed, behind: false }
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
      Marker { at: [30.0, 30.0, 0.0], color: [200, 140, 40, 255], size: 5.0, id: 60, on_top: true },
      Marker { at: [-30.0, -30.0, 0.0], color: [30, 110, 235, 255], size: 3.0, id: 0, on_top: false },
    ],
    background_top: [250, 250, 251, 255],
    background_bottom: [234, 236, 239, 255],
    analysis: None,
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
  let h = match clf_render_gl::headless::context() {
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
    Some(Analysis::Zebra { stripes: 12.0 }),
    Some(Analysis::Draft { pull: [0.0, 0.0, 1.0], angle: 0.2 }),
    Some(Analysis::Accessibility { dir: [0.0, 0.0, 1.0] }),
  ];
  for analysis in analyses {
    let mut s = scene();
    s.analysis = analysis;
    if analysis.is_some() {
      // vertex colours (curvature) and analysis don't mix in the app
      s.meshes[1].colors.clear();
    }
    for (name, cam) in cams() {
      let cpu = clf_render::render(&s, &cam, 320, 240);
      let gpu = r.render(&h.gl, &s, &cam, 320, 240).expect("gpu render");
      let d = compare(&cpu, &gpu);
      eprintln!("{name} {analysis:?}: {:.2}% colour, {:.2}% ids differ", d.color * 100.0, d.ids * 100.0);
      if std::env::var_os("CLF_PARITY_PNG").is_some() {
        let _ = cpu.save_png(std::path::Path::new(&format!("/tmp/parity-{name}-cpu.png")));
        let _ = gpu.save_png(std::path::Path::new(&format!("/tmp/parity-{name}-gpu.png")));
      }
      assert!(d.color < 0.03, "{name} {analysis:?}: {:.2}% of pixels differ in colour", d.color * 100.0);
      assert!(d.ids < 0.02, "{name} {analysis:?}: {:.2}% of pixels differ in pick id", d.ids * 100.0);
    }
  }
  r.destroy(&h.gl);
}

#[test]
fn picks_are_exact_and_hover_reuses_meshes() {
  let Ok(h) = clf_render_gl::headless::context() else { return };
  let Ok(mut r) = Renderer::new(&h.gl) else { return };
  let mut cam = Camera { distance: 120.0, ortho: true, ..Camera::default() };
  cam.set_view(StandardView::Top);
  let s = scene();
  let f = r.render(&h.gl, &s, &cam, 400, 300).expect("render");
  // the transparent quad on top, the on-top axis and marker, the sphere's top band, the base
  assert_eq!(f.pick(200.0, 150.0, 0, |_| true), 52, "the on-top axis covers the centre");
  assert_eq!(f.pick(205.0, 150.0, 0, |_| true), 8, "the transparent quad still picks");
  assert_eq!(f.pick(2.0, 2.0, 0, |_| true), NO_ID);
  let cpu = clf_render::render(&s, &cam, 400, 300);
  for (x, y) in [(200.0, 150.0), (205.0, 150.0), (230.0, 150.0), (300.0, 150.0), (60.0, 40.0)] {
    assert_eq!(f.pick(x, y, 0, |_| true), cpu.pick(x, y, 0, |_| true), "at {x},{y}");
  }
  // hover change only: the meshes stay, the highlight follows
  let mut hovered = s.clone();
  hovered.meshes[1].highlight = Some(101);
  r.set_scene(&h.gl, &hovered, true).expect("set");
  r.draw(&h.gl, &cam, 400, 300).expect("draw");
  let px = r.read_rgba(&h.gl).expect("rgba");
  let cpu = clf_render::render(&hovered, &cam, 400, 300);
  let same =
    px.chunks_exact(4).zip(cpu.rgba.chunks_exact(4)).filter(|(a, b)| (0..3).all(|k| a[k].abs_diff(b[k]) <= 12));
  assert!(same.count() as f64 / (400.0 * 300.0) > 0.97);
  r.destroy(&h.gl);
}
