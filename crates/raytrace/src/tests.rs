//! Deterministic low-sample renders checked against reference images in `tests/reference`.
//! `FOX_BLESS_RENDERS=1 cargo test -p fox-raytrace` rewrites the references.

use std::f32::consts::{PI, TAU};
use std::path::PathBuf;

use fox_render::Camera;

use super::*;

pub fn sphere(c: [f32; 3], r: f32, material: usize) -> RenderMesh {
  let (nu, nv) = (48u32, 24u32);
  let mut m = RenderMesh { material, origin: c, ..RenderMesh::default() };
  for j in 0..=nv {
    let th = j as f32 / nv as f32 * PI;
    for i in 0..=nu {
      let ph = i as f32 / nu as f32 * TAU;
      let n = [th.sin() * ph.cos(), th.sin() * ph.sin(), th.cos()];
      m.positions.push([c[0] + n[0] * r, c[1] + n[1] * r, c[2] + n[2] * r]);
      m.normals.push(n);
    }
  }
  for j in 0..nv {
    for i in 0..nu {
      let a = j * (nu + 1) + i;
      let b = a + nu + 1;
      m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
    }
  }
  m
}

pub fn cuboid(min: [f32; 3], max: [f32; 3], material: usize) -> RenderMesh {
  let mut m = RenderMesh {
    material,
    origin: [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0, (min[2] + max[2]) / 2.0],
    ..RenderMesh::default()
  };
  for axis in 0..3 {
    for side in [0, 1] {
      let mut n = [0.0; 3];
      n[axis] = if side == 1 { 1.0 } else { -1.0 };
      let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
      let base = m.positions.len() as u32;
      for (a, b) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
        let mut p = [0.0; 3];
        p[axis] = if side == 1 { max[axis] } else { min[axis] };
        p[u] = if a == 1 { max[u] } else { min[u] };
        p[v] = if b == 1 { max[v] } else { min[v] };
        m.positions.push(p);
        m.normals.push(n);
      }
      if side == 1 {
        m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
      } else {
        m.indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
      }
    }
  }
  m
}

fn camera() -> RenderCamera {
  let view = Camera { target: [0.0, 0.0, 12.0], distance: 160.0, pitch: 0.35, ..Camera::default() };
  RenderCamera { view, ..RenderCamera::default() }
}

fn lib(name: &str) -> Material {
  material::find(name).unwrap_or_default()
}

/// A metal sphere, a glass sphere and a textured block on a shadow-catching ground.
pub fn showcase() -> RenderScene {
  RenderScene {
    meshes: vec![
      sphere([-28.0, 0.0, 14.0], 14.0, 0),
      sphere([4.0, -6.0, 10.0], 10.0, 1),
      cuboid([16.0, -10.0, 0.0], [40.0, 14.0, 18.0], 2),
    ],
    materials: vec![lib("Gold - Polished"), lib("Glass - Clear"), lib("Wood - Oak")],
    environment: Environment::default(),
    camera: camera(),
    lights: Vec::new(),
  }
}

fn settings(samples: u32) -> RenderSettings {
  RenderSettings { width: 96, height: 64, samples, max_bounces: 6, denoise: false, seed: 1, threads: 0 }
}

fn reference_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/reference")
}

/// Compares with (or with `FOX_BLESS_RENDERS` set, writes) `tests/reference/<name>.png`.
fn check(name: &str, img: &RenderImage) {
  let path = reference_dir().join(format!("{name}.png"));
  if std::env::var("FOX_BLESS_RENDERS").is_ok_and(|v| !v.is_empty() && v != "0") {
    std::fs::create_dir_all(reference_dir()).unwrap();
    img.save_png(&path).unwrap();
    return;
  }
  let want =
    image::open(&path).unwrap_or_else(|e| panic!("{}: {e} (FOX_BLESS_RENDERS=1 writes it)", path.display())).to_rgba8();
  assert_eq!((want.width(), want.height()), (img.width, img.height), "{name}: size");
  // the same maths on another CPU or compiler may round differently: allow small differences
  let diffs: Vec<i32> = want.as_raw().iter().zip(&img.rgba).map(|(a, b)| (*a as i32 - *b as i32).abs()).collect();
  let mean = diffs.iter().sum::<i32>() as f64 / diffs.len() as f64;
  let big = diffs.iter().filter(|d| **d > 40).count() as f64 / diffs.len() as f64;
  assert!(mean < 2.0 && big < 0.01, "{name}: mean difference {mean:.2}, {:.2}% of channels off by > 40", big * 100.0);
}

#[test]
fn showcase_matches_reference() {
  let img = render(&showcase(), settings(16)).unwrap();
  check("showcase", &img);
}

#[test]
fn denoised_showcase_matches_reference() {
  let img = render(&showcase(), RenderSettings { denoise: true, ..settings(4) }).unwrap();
  check("showcase-denoised", &img);
}

#[test]
fn backgrounds_and_depth_of_field_match_references() {
  let mut s = showcase();
  s.environment.preset = Preset::Sky;
  s.environment.background = Background::Gradient;
  s.environment.ground_reflection = 0.4;
  s.camera.fstop = 1.4;
  s.camera.focus_distance = Some(150.0);
  s.camera.exposure = 0.5;
  check("sky-gradient-dof", &render(&s, settings(16)).unwrap());

  let mut s = showcase();
  s.materials = vec![lib("Paint - Metallic Red"), lib("Carbon Fiber"), lib("Steel - Brushed")];
  s.environment.background = Background::Transparent;
  s.camera.view.ortho = true;
  s.camera.view.distance = 100.0;
  check("ortho-transparent", &render(&s, settings(16)).unwrap());
}

#[test]
fn renders_are_the_same_whatever_the_thread_count() {
  let a = render(&showcase(), RenderSettings { threads: 1, ..settings(3) }).unwrap();
  let b = render(&showcase(), RenderSettings { threads: 7, ..settings(3) }).unwrap();
  assert_eq!(a, b);
  // and a progressive render in steps equals one in a single go
  let mut p = Progressive::new(&showcase(), settings(3)).unwrap();
  let cancel = std::sync::atomic::AtomicBool::new(false);
  p.step(1, &cancel);
  p.step(2, &cancel);
  assert_eq!(p.image(false), a);
}

#[test]
fn more_samples_converge() {
  // the noise of a 4-sample render is larger than that of a 64-sample one (both against 256)
  let s = showcase();
  let truth = render(&s, settings(256)).unwrap();
  let err = |n| {
    let img = render(&s, settings(n)).unwrap();
    img.rgba.iter().zip(&truth.rgba).map(|(a, b)| (*a as f64 - *b as f64).powi(2)).sum::<f64>()
  };
  let (e4, e64) = (err(4), err(64));
  assert!(e64 < e4 * 0.5, "{e4} vs {e64}");
}

#[test]
fn cancelling_keeps_every_pixel_at_the_same_sample_count() {
  let mut p = Progressive::new(&showcase(), settings(8)).unwrap();
  let cancel = std::sync::atomic::AtomicBool::new(true);
  assert_eq!(p.step(4, &cancel), 0);
  assert_eq!(p.accum.samples, 0);
}

#[test]
fn an_empty_scene_shows_the_background() {
  let s = RenderScene {
    environment: Environment {
      background: Background::Color,
      background_color: [10, 20, 30],
      ..Environment::default()
    },
    camera: camera(),
    ..RenderScene::default()
  };
  let img = render(&s, RenderSettings { width: 4, height: 4, samples: 2, ..settings(2) }).unwrap();
  assert!(img.rgba.chunks(4).all(|p| p == [10, 20, 30, 255]));
}

#[test]
#[ignore]
fn big_preview() {
  let dir = std::env::var("FOX_PREVIEW_DIR").unwrap();
  let mut s = showcase();
  let st = RenderSettings { width: 480, height: 320, samples: 64, denoise: false, ..settings(64) };
  render(&s, st).unwrap().save_png(&PathBuf::from(&dir).join("a.png")).unwrap();
  render(&s, RenderSettings { samples: 8, denoise: true, ..st })
    .unwrap()
    .save_png(&PathBuf::from(&dir).join("a8d.png"))
    .unwrap();
  s.environment.preset = Preset::Sky;
  s.materials = vec![lib("Paint - Metallic Red"), lib("Carbon Fiber"), lib("Steel - Brushed")];
  render(&s, st).unwrap().save_png(&PathBuf::from(&dir).join("b.png")).unwrap();
}

fn mean_luminance(img: &RenderImage) -> f32 {
  img.rgba.chunks(4).map(|p| (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0).sum::<f32>() / (img.rgba.len() / 4) as f32
}

/// A matte block on a dark environment, with and without a light over it.
fn lit_block(lights: Vec<Light>) -> RenderImage {
  let s = RenderScene {
    meshes: vec![cuboid([-20.0, -20.0, 0.0], [20.0, 20.0, 10.0], 0)],
    materials: vec![Material { color: [200, 200, 200], roughness: 0.9, ..Material::default() }],
    environment: Environment { preset: Preset::Dark, ground: false, ..Environment::default() },
    camera: camera(),
    lights,
  };
  render(&s, settings(4)).unwrap()
}

#[test]
fn a_light_brightens_what_it_shines_on() {
  let dark = lit_block(Vec::new());
  let light = Light { position: [0.0, 0.0, 40.0], intensity: 2000.0, ..Light::default() };
  let lit = lit_block(vec![light.clone()]);
  assert!(mean_luminance(&lit) > mean_luminance(&dark) + 5.0, "{} {}", mean_luminance(&lit), mean_luminance(&dark));
  // switched off, it's as if it weren't there
  assert_eq!(lit_block(vec![Light { on: false, ..light }]).rgba, dark.rgba);
}

/// A glass block holding a block of water that reaches into its walls.
fn glass_and_water(water_priority: u8, glass_priority: u8) -> RenderImage {
  let mut glass = cuboid([-15.0, -15.0, 0.0], [15.0, 15.0, 30.0], 0);
  glass.priority = glass_priority;
  let mut water = cuboid([-13.0, -13.0, 1.0], [13.0, 13.0, 20.0], 1);
  water.priority = water_priority;
  let water_mat =
    Material { color: [120, 170, 255], transmission: 1.0, roughness: 0.0, ior: 1.33, ..Material::default() };
  let s = RenderScene {
    meshes: vec![glass, water],
    materials: vec![lib("Glass - Clear"), water_mat],
    environment: Environment::default(),
    camera: camera(),
    lights: Vec::new(),
  };
  render(&s, settings(4)).unwrap()
}

#[test]
fn dielectric_priorities_change_how_overlapping_glass_renders() {
  // equal priorities trace as before; a higher-priority glass hides the water's faces inside it
  let same = glass_and_water(0, 0);
  assert_eq!(glass_and_water(1, 1).rgba, same.rgba);
  assert_ne!(glass_and_water(0, 2).rgba, same.rgba);
}

#[test]
fn flatten_ground_changes_only_the_background_below_the_horizon() {
  let mut s = showcase();
  s.environment.preset = Preset::Sky;
  s.environment.ground = false;
  // a level camera: the top row sees the sky, the bottom one the ground
  s.camera.view.pitch = 0.0;
  let plain = render(&s, settings(2)).unwrap();
  s.environment.flatten_ground = true;
  let flat = render(&s, settings(2)).unwrap();
  let row =
    |img: &RenderImage, y: u32| img.rgba[(y * img.width * 4) as usize..((y + 1) * img.width * 4) as usize].to_vec();
  // the top row looks at the sky; the bottom one at the ground
  assert_eq!(row(&plain, 0), row(&flat, 0));
  assert_ne!(row(&plain, plain.height - 1), row(&flat, flat.height - 1));
}
