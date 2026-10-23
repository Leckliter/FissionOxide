//! The Render workspace's real-time shading: physically based materials over an environment,
//! checked against the offline path tracer it previews. Skipped (with a note) where no OpenGL
//! context can be created.

use fox_raytrace::{EnvMap, Environment, Material, RenderCamera, RenderMesh, RenderScene, RenderSettings, material};
use fox_render::{Camera, Mesh, Scene};
use fox_render_gl::Renderer;
use fox_render_gl::pbr::Look;

fn sphere(center: [f32; 3], r: f32, id: u32) -> (Mesh, RenderMesh) {
  let n = 32u32;
  let mut m = Mesh { id, color: [186, 190, 196, 255], analyze: true, ..Default::default() };
  for i in 0..=n {
    let th = std::f32::consts::PI * i as f32 / n as f32;
    for j in 0..=n * 2 {
      let ph = std::f32::consts::PI * j as f32 / n as f32;
      let d = [th.sin() * ph.cos(), th.sin() * ph.sin(), th.cos()];
      m.positions.push([center[0] + r * d[0], center[1] + r * d[1], center[2] + r * d[2]]);
      m.normals.push(d);
    }
  }
  let row = n * 2 + 1;
  for i in 0..n {
    for j in 0..n * 2 {
      let (a, b) = (i * row + j, (i + 1) * row + j);
      m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
    }
  }
  let rm = RenderMesh {
    positions: m.positions.clone(),
    normals: m.normals.clone(),
    indices: m.indices.clone(),
    material: 0,
    origin: center,
    priority: 0,
  };
  (m, rm)
}

struct Setup {
  scene: Scene,
  render: RenderScene,
  materials: Vec<Option<Material>>,
  origins: Vec<[f32; 3]>,
  cam: Camera,
}

fn setup() -> Setup {
  let (a, mut ra) = sphere([-14.0, 0.0, 12.0], 12.0, 10);
  let (b, mut rb) = sphere([14.0, 0.0, 12.0], 12.0, 11);
  ra.material = 0;
  rb.material = 1;
  let mats = vec![material::find("Gold - Polished").unwrap(), material::find("Checker (test)").unwrap()];
  let cam = Camera { target: [0.0, 0.0, 10.0], distance: 120.0, pitch: 0.3, ..Camera::default() };
  Setup {
    scene: Scene {
      meshes: vec![a, b],
      background_top: [250, 250, 251, 255],
      background_bottom: [234, 236, 239, 255],
      ..Default::default()
    },
    render: RenderScene {
      meshes: vec![ra, rb],
      materials: mats.clone(),
      environment: Environment::default(),
      camera: RenderCamera { view: cam, ..RenderCamera::default() },
      lights: Vec::new(),
    },
    materials: mats.into_iter().map(Some).collect(),
    origins: vec![[-14.0, 0.0, 12.0], [14.0, 0.0, 12.0]],
    cam,
  }
}

#[test]
fn preview_resembles_the_path_traced_render() {
  let h = match fox_render_gl::headless::context() {
    Ok(h) => h,
    Err(e) => {
      eprintln!("skipped: no OpenGL context ({e})");
      return;
    }
  };
  let gl = &h.gl;
  let mut r = match Renderer::new(gl) {
    Ok(r) => r,
    Err(e) => {
      eprintln!("skipped: {e}");
      return;
    }
  };
  let s = setup();
  let env = EnvMap::build(&s.render.environment).unwrap();
  let (w, hgt) = (160, 100);
  r.set_scene(gl, &s.scene, false).unwrap();
  let look = Look {
    env: &env,
    environment: &s.render.environment,
    camera: &s.render.camera,
    materials: &s.materials,
    origins: &s.origins,
    bounds: ([-26.0, -12.0, 0.0], [26.0, 12.0, 24.0]),
    lights: &[],
  };
  r.set_look(gl, Some(&look)).unwrap();
  r.draw(gl, &s.cam, w, hgt).unwrap();
  let gpu = r.read_rgba(gl).unwrap();
  // the pick ids are unchanged by the shading
  let ids = r.read_ids(gl).unwrap();
  let center = |x: u32, y: u32| ids.ids[(y * w + x) as usize];
  let traced = fox_raytrace::render(
    &s.render,
    RenderSettings { width: w, height: hgt, samples: 32, denoise: true, ..RenderSettings::default() },
  )
  .unwrap();
  if let Ok(dir) = std::env::var("FOX_PREVIEW_DIR") {
    image::save_buffer(format!("{dir}/gpu.png"), &gpu, w, hgt, image::ExtendedColorType::Rgba8).unwrap();
    traced.save_png(std::path::Path::new(&format!("{dir}/traced.png"))).unwrap();
  }
  // compare where both see a body
  let mut sum = 0.0;
  let mut n = 0;
  for y in 0..hgt {
    for x in 0..w {
      if center(x, y) == 0 {
        continue;
      }
      let i = ((y * w + x) * 4) as usize;
      for k in 0..3 {
        sum += (gpu[i + k] as f64 - traced.rgba[i + k] as f64).abs();
      }
      n += 3;
    }
  }
  assert!(n > 3000, "the spheres are on screen ({n})");
  let mean = sum / n as f64;
  eprintln!("mean difference from the path tracer on the bodies: {mean:.1}");
  assert!(mean < 50.0, "{mean}");
  // the background and ground show the environment, not the flat viewport gradient
  let corner = &gpu[0..3];
  assert_ne!(corner, &[250, 250, 251]);
  // and switching the look off goes back to the usual shading
  r.set_look(gl, None).unwrap();
  r.draw(gl, &s.cam, w, hgt).unwrap();
  let plain = r.read_rgba(gl).unwrap();
  assert_eq!(&plain[0..3], &[250, 250, 251]);
}

/// `ui:render:in-canvas-ground-slab`: In-canvas Render on a new design's scene (Solid Color grey)
/// draws an empty design as plain grey and a body on that grey with a soft shadow, even though
/// the viewport draws inside egui's paint callback, which leaves its scissor test on.
#[test]
fn the_ground_is_the_background_not_a_dark_slab() {
  let Ok(h) = fox_render_gl::headless::context() else {
    eprintln!("skipped: no OpenGL context");
    return;
  };
  let gl = &h.gl;
  let Ok(mut r) = Renderer::new(gl) else { return };
  let environment = Environment::new_design();
  let grey = environment.background_color;
  let env = EnvMap::build(&environment).unwrap();
  let camera = RenderCamera::default();
  let (w, hgt) = (160, 100);
  let cam = Camera { distance: 300.0, ..Camera::default() };
  for body in [false, true] {
    let mut scene = Scene::default();
    let mut materials = Vec::new();
    let mut bounds = ([-50.0; 3], [50.0; 3]);
    if body {
      scene.meshes.push(sphere([0.0, 0.0, 10.0], 10.0, 5).0);
      materials.push(Some(Material::default()));
      bounds = ([-10.0, -10.0, 0.0], [10.0, 10.0, 20.0]);
    }
    let origins = vec![[0.0; 3]; materials.len()];
    r.set_scene(gl, &scene, false).unwrap();
    let look = Look {
      env: &env,
      environment: &environment,
      camera: &camera,
      materials: &materials,
      origins: &origins,
      bounds,
      lights: &[],
    };
    r.set_look(gl, Some(&look)).unwrap();
    h.scissor_like_egui([10, 10, 100, 60]);
    r.draw(gl, &cam, w, hgt).unwrap();
    let gpu = r.read_rgba(gl).unwrap();
    let ids = r.read_ids(gl).unwrap();
    let ground: Vec<&[u8]> = gpu.chunks(4).zip(&ids.ids).filter(|(_, id)| **id == 0).map(|(c, _)| &c[..3]).collect();
    if !body {
      assert!(ground.iter().all(|c| *c == grey), "an empty design is plain grey");
      continue;
    }
    let plain = ground.iter().filter(|c| **c == grey).count();
    assert!(plain * 10 > ground.len() * 9, "the ground away from the body is the background");
    let darkest = ground.iter().map(|c| c[0]).min().unwrap();
    assert!(darkest > 80, "a soft grey shadow, not a black one: {darkest}");
  }
}

/// A box from `lo` to `hi` with a flat normal per face.
fn cube(lo: [f32; 3], hi: [f32; 3], id: u32) -> Mesh {
  let mut m = Mesh { id, color: [186, 190, 196, 255], analyze: true, ..Default::default() };
  let p = |x: usize, y: usize, z: usize| [[lo[0], hi[0]][x], [lo[1], hi[1]][y], [lo[2], hi[2]][z]];
  let faces: [([f32; 3], [[usize; 3]; 4]); 6] = [
    ([0.0, 0.0, 1.0], [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]),
    ([0.0, 0.0, -1.0], [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]]),
    ([1.0, 0.0, 0.0], [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]]),
    ([-1.0, 0.0, 0.0], [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]]),
    ([0.0, 1.0, 0.0], [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]]),
    ([0.0, -1.0, 0.0], [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]),
  ];
  for (n, quad) in faces {
    let base = m.positions.len() as u32;
    for c in quad {
      m.positions.push(p(c[0], c[1], c[2]));
      m.normals.push(n);
    }
    m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
  }
  m
}

/// `ui:render:in-canvas-body-shading`: In-canvas Render on a new design's scene draws two boxes
/// with the default appearance (Steel - Satin) as bright satin steel, light grey with highlights,
/// over faint, soft shadows that stay near the boxes, as Fusion's.
#[test]
fn default_steel_renders_bright_with_faint_soft_shadows() {
  let Ok(h) = fox_render_gl::headless::context() else {
    eprintln!("skipped: no OpenGL context");
    return;
  };
  let gl = &h.gl;
  let Ok(mut r) = Renderer::new(gl) else { return };
  let environment = Environment::new_design();
  let grey = environment.background_color;
  let env = EnvMap::build(&environment).unwrap();
  let camera = RenderCamera::default();
  let (w, hgt) = (320, 200);
  let cam = Camera { target: [0.0, 0.0, 10.0], distance: 220.0, pitch: 0.5, ..Camera::default() };
  let scene = Scene {
    meshes: vec![cube([-45.0, -20.0, 0.0], [-5.0, 20.0, 30.0], 5), cube([5.0, -20.0, 0.0], [45.0, 20.0, 20.0], 6)],
    ..Default::default()
  };
  let steel = fox_raytrace::material::default_material();
  assert_eq!(steel.name, "Steel - Satin");
  let materials = vec![Some(steel.clone()), Some(steel)];
  let origins = vec![[-25.0, 0.0, 15.0], [25.0, 0.0, 10.0]];
  r.set_scene(gl, &scene, false).unwrap();
  let look = Look {
    env: &env,
    environment: &environment,
    camera: &camera,
    materials: &materials,
    origins: &origins,
    bounds: ([-45.0, -20.0, 0.0], [45.0, 20.0, 30.0]),
    lights: &[],
  };
  r.set_look(gl, Some(&look)).unwrap();
  r.draw(gl, &cam, w, hgt).unwrap();
  let gpu = r.read_rgba(gl).unwrap();
  let ids = r.read_ids(gl).unwrap();
  if let Ok(dir) = std::env::var("FOX_PREVIEW_DIR") {
    image::save_buffer(format!("{dir}/steel.png"), &gpu, w, hgt, image::ExtendedColorType::Rgba8).unwrap();
  }
  let px = |i: usize| &gpu[i * 4..i * 4 + 3];
  let lum = |c: &[u8]| (c[0] as f32 * 0.2126 + c[1] as f32 * 0.7152 + c[2] as f32 * 0.0722) / 255.0;
  // the bodies: light grey steel, not charcoal, with some highlight
  let body: Vec<f32> = (0..ids.ids.len()).filter(|i| ids.ids[*i] != 0).map(|i| lum(px(i))).collect();
  assert!(body.len() > 3000, "the boxes are on screen ({})", body.len());
  let mean = body.iter().sum::<f32>() / body.len() as f32;
  let darkest_face = body.iter().copied().fold(1.0, f32::min);
  let brightest = body.iter().copied().fold(0.0, f32::max);
  eprintln!("steel: mean {mean:.2}, darkest {darkest_face:.2}, brightest {brightest:.2}");
  assert!(mean > 0.55, "bright satin steel, not near black: {mean}");
  assert!(darkest_face > 0.3, "no charcoal faces: {darkest_face}");
  // the ground: the grey background with a faint shadow that stays close to the boxes
  let ground: Vec<usize> = (0..ids.ids.len()).filter(|i| ids.ids[*i] == 0).collect();
  let shadowed = ground.iter().filter(|i| px(**i) != grey).count();
  let darkest = ground.iter().map(|i| px(*i)[0]).min().unwrap();
  eprintln!("ground: {shadowed} of {} shadowed, darkest {darkest}", ground.len());
  assert!(darkest as f32 > grey[0] as f32 * 0.65, "a faint shadow, not a dark one: {darkest}");
  assert!(shadowed * 5 < ground.len(), "the shadow doesn't run far across the ground: {shadowed}");
}
