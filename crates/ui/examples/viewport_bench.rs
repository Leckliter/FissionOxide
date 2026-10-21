//! The viewport's cost per orbit frame on a real model, CPU path against GPU path.
//!
//! `cargo run --release -p fox-ui --example viewport_bench [-- COUNT WIDTH HEIGHT]`
//!
//! Builds the example bracket (`examples/bracket.jsonl`), patterns it COUNT × COUNT, then orbits:
//! the CPU path rebuilds the scene and rasterizes it every frame (the render key includes the
//! camera); the GPU path built and uploaded the scene once and only draws. Without a GPU, Mesa's
//! software OpenGL (llvmpipe) stands in.

use std::time::Instant;

use fox_engine::Session;
use fox_engine::render::Camera;
use fox_engine::scene::{SceneBuilder, ViewOptions};
use fox_render_gl::Renderer;
use serde_json::{Value, json};

fn ms(t: Instant) -> f64 {
  t.elapsed().as_secs_f64() * 1e3
}

fn main() -> Result<(), String> {
  let args: Vec<u32> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
  let (n, w, h) =
    (args.first().copied().unwrap_or(8), args.get(1).copied().unwrap_or(1600), args.get(2).copied().unwrap_or(1000));
  let mut s = Session::default();
  let script = include_str!("../../../examples/bracket.jsonl");
  for line in script.lines().filter(|l| l.trim_start().starts_with('{')) {
    let mut v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    let cmd = v["cmd"].as_str().unwrap_or_default().to_string();
    if let Some(o) = v.as_object_mut() {
      o.remove("cmd");
    }
    s.execute(&cmd, &v).map_err(|e| format!("{cmd}: {}", e.0))?;
  }
  let p = json!({"direction": [1, 0, 0], "count": n, "spacing": "120 mm", "direction2": [0, 1, 0], "count2": n, "spacing2": "120 mm", "join": false});
  s.execute("pattern.rectangular", &p).map_err(|e| e.0)?;

  let mut builder = SceneBuilder::default();
  let opts = ViewOptions::default();
  let t = Instant::now();
  let (scene, _) = builder.build(&s, &opts);
  let first_build = ms(t);
  let tris: usize = scene.meshes.iter().map(|m| m.indices.len() / 3).sum();
  let segs: usize = scene.lines.iter().map(|l| l.points.len().saturating_sub(1)).sum();
  let mut cam = Camera::default();
  let (lo, hi) = s.model.bounds().ok_or("no model")?;
  cam.fit(lo, hi, w as f64 / h as f64);

  let frames = 6;
  let t = Instant::now();
  for k in 0..frames {
    let mut c = cam;
    c.orbit(0.02 * k as f64, 0.0);
    let (scene, _) = builder.build(&s, &opts);
    std::hint::black_box(fox_engine::render::render(&scene, &c, w, h));
  }
  let cpu = ms(t) / frames as f64;

  let hx = fox_render_gl::headless::context()?;
  let gl = &hx.gl;
  let name = hx.renderer_name();
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
  let gpu = ms(t) / frames as f64;
  let t = Instant::now();
  for k in 0..frames {
    let mut c = cam;
    c.orbit(0.02 * k as f64, 0.0);
    r.draw_navigating(gl, &c, w, h)?;
    hx.finish();
  }
  let navigating = ms(t) / frames as f64;
  let coarse = r.stats();
  let mut lined = scene.clone();
  lined.style = fox_engine::render::VisualStyle::HiddenLine;
  let t = Instant::now();
  r.set_scene(gl, &lined, true)?;
  r.draw(gl, &cam, w, h)?;
  hx.finish();
  let lined_switch = ms(t);
  let t = Instant::now();
  for k in 0..frames {
    let mut c = cam;
    c.orbit(0.02 * k as f64, 0.0);
    r.draw(gl, &c, w, h)?;
    hx.finish();
  }
  let lined_gpu = ms(t) / frames as f64;
  r.set_scene(gl, &scene, true)?;
  r.draw(gl, &cam, w, h)?;
  let t = Instant::now();
  let ids = r.read_ids(gl).ok_or("no ids")?;
  let readback = ms(t);
  println!("bracket × {n}×{n}: {tris} triangles, {segs} edge segments, {w}×{h} px; OpenGL: {name}");
  println!("scene build (tessellation cached after the first):  first {first_build:.1} ms");
  println!("CPU path per orbit frame (scene build + raster):    {cpu:8.1} ms");
  println!("GPU path first frame (upload + draw):               {upload:8.1} ms");
  println!("GPU path per orbit frame (draw):                    {gpu:8.1} ms");
  println!("GPU id readback when the view settles:              {readback:8.1} ms ({} px)", ids.ids.len());
  println!(
    "GPU path per orbit frame, navigating (LOD):         {navigating:8.1} ms ({} of {} chunks coarse, {} tris)",
    coarse.coarse, coarse.chunks, coarse.triangles
  );
  println!("GPU Hidden Line: switch {lined_switch:.1} ms, per orbit frame {lined_gpu:8.1} ms");
  r.destroy(gl);
  Ok(())
}
