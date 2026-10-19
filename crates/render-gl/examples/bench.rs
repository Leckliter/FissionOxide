//! Orbit benchmark: the CPU rasterizer against the GPU renderer on a large patterned part.
//!
//! `cargo run --release -p clf-render-gl --features headless --example bench [-- N SEGMENTS WIDTH HEIGHT]`
//!
//! The part is a plate with an N × N pattern of bosses, each a tessellated cylinder with a domed
//! top and its own pick ids per face, with edge loops like a B-rep body's. Without a GPU, Mesa's
//! software OpenGL (llvmpipe) stands in, so the GPU numbers are an upper bound.

use std::f32::consts::{PI, TAU};
use std::time::Instant;

use clf_render::{Camera, Mesh, Polyline, Scene};
use clf_render_gl::Renderer;

fn boss(m: &mut Mesh, lines: &mut Vec<Polyline>, c: [f32; 2], r: f32, h: f32, seg: u32, id: &mut u32) {
  let base = m.positions.len() as u32;
  let rings = seg / 2;
  // side wall
  for k in 0..=seg {
    let a = TAU * k as f32 / seg as f32;
    let (s, co) = a.sin_cos();
    for z in [0.0, h] {
      m.positions.push([c[0] + r * co, c[1] + r * s, z]);
      m.normals.push([co, s, 0.0]);
    }
  }
  let side = *id;
  for k in 0..seg {
    let i = base + k * 2;
    m.indices.extend_from_slice(&[i, i + 2, i + 1, i + 1, i + 2, i + 3]);
    m.tri_ids.extend_from_slice(&[side, side]);
  }
  // domed top
  let top = *id + 1;
  let dome = m.positions.len() as u32;
  for j in 0..=rings {
    let t = PI / 2.0 * j as f32 / rings as f32;
    for k in 0..=seg {
      let a = TAU * k as f32 / seg as f32;
      let d = [t.cos() * a.cos(), t.cos() * a.sin(), t.sin()];
      m.positions.push([c[0] + r * d[0], c[1] + r * d[1], h + r * 0.5 * d[2]]);
      m.normals.push(d);
    }
  }
  let row = seg + 1;
  for j in 0..rings {
    for k in 0..seg {
      let a = dome + j * row + k;
      m.indices.extend_from_slice(&[a, a + 1, a + row, a + 1, a + row + 1, a + row]);
      m.tri_ids.extend_from_slice(&[top, top]);
    }
  }
  for (z, e) in [(0.0, *id + 2), (h, *id + 3)] {
    let pts = (0..=seg)
      .map(|k| {
        let a = TAU * k as f32 / seg as f32;
        [c[0] + r * a.cos(), c[1] + r * a.sin(), z]
      })
      .collect();
    lines.push(Polyline {
      points: pts,
      color: [35, 38, 44, 255],
      width: 1.0,
      id: e,
      on_top: false,
      dashed: false,
      behind: false,
    });
  }
  *id += 4;
}

fn part(n: u32, seg: u32) -> Scene {
  let pitch = 12.0;
  let half = n as f32 * pitch / 2.0;
  let mut m = Mesh { color: [186, 190, 196, 255], analyze: true, ..Default::default() };
  let mut lines = Vec::new();
  m.positions.extend_from_slice(&[[-half, -half, 0.0], [half, -half, 0.0], [half, half, 0.0], [-half, half, 0.0]]);
  m.normals.extend_from_slice(&[[0.0, 0.0, 1.0]; 4]);
  m.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
  m.tri_ids.extend_from_slice(&[1, 1]);
  let mut id = 2;
  for i in 0..n {
    for j in 0..n {
      let c = [-half + pitch * (i as f32 + 0.5), -half + pitch * (j as f32 + 0.5)];
      boss(&mut m, &mut lines, c, 4.0, 8.0, seg, &mut id);
    }
  }
  Scene {
    meshes: vec![m],
    lines,
    background_top: [250, 250, 251, 255],
    background_bottom: [234, 236, 239, 255],
    ..Default::default()
  }
}

fn ms(t: Instant) -> f64 {
  t.elapsed().as_secs_f64() * 1e3
}

fn main() -> Result<(), String> {
  let args: Vec<u32> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
  let (n, seg, w, h) = (
    args.first().copied().unwrap_or(40),
    args.get(1).copied().unwrap_or(64),
    args.get(2).copied().unwrap_or(1600),
    args.get(3).copied().unwrap_or(1000),
  );
  let scene = part(n, seg);
  let tris: usize = scene.meshes.iter().map(|m| m.indices.len() / 3).sum();
  let segs: usize = scene.lines.iter().map(|l| l.points.len() - 1).sum();
  let hx = clf_render_gl::headless::context()?;
  let gl = &hx.gl;
  let renderer_name = hx.renderer_name();
  println!("{n}×{n} bosses: {tris} triangles, {segs} edge segments, {w}×{h} px; OpenGL: {renderer_name}");

  let mut cam = Camera::default();
  cam.fit(glam::DVec3::new(-250.0, -250.0, 0.0), glam::DVec3::new(250.0, 250.0, 12.0), w as f64 / h as f64);
  let frames = 10;

  let t = Instant::now();
  for k in 0..frames.min(4) {
    let mut c = cam;
    c.orbit(0.02 * k as f64, 0.0);
    std::hint::black_box(clf_render::render(&scene, &c, w, h));
  }
  let cpu = ms(t) / frames.min(4) as f64;

  let mut r = Renderer::new(gl)?;
  let t = Instant::now();
  r.set_scene(gl, &scene, false)?;
  r.draw(gl, &cam, w, h)?;

  hx.finish();
  let upload = ms(t);
  let t = Instant::now();
  for k in 0..frames {
    let mut c = cam;
    c.orbit(0.02 * k as f64, 0.0);
    r.draw(gl, &c, w, h)?;
    hx.finish();
  }
  let orbit = ms(t) / frames as f64;
  let t = Instant::now();
  let ids = r.read_ids(gl).ok_or("no ids")?;
  let readback = ms(t);
  let t = Instant::now();
  let mut hovered = scene.clone();
  hovered.meshes[0].highlight = Some(3);
  hovered.meshes[0].highlight_color = [128, 190, 238, 255];
  r.set_scene(gl, &hovered, true)?;
  r.draw(gl, &cam, w, h)?;
  hx.finish();
  let hover = ms(t);
  println!("CPU rasterizer, per orbit frame:          {cpu:8.1} ms");
  println!("GPU first frame (upload + draw):          {upload:8.1} ms");
  println!("GPU per orbit frame (draw only):          {orbit:8.1} ms");
  println!("GPU id readback (once the view settles):  {readback:8.1} ms ({} px)", ids.ids.len());
  println!("GPU hover change (lines + draw):          {hover:8.1} ms");
  r.destroy(gl);
  Ok(())
}
