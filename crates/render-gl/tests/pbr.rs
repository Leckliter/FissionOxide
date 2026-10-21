//! The Render workspace's real-time shading: physically based materials over an environment,
//! checked against the offline path tracer it previews. Skipped (with a note) where no OpenGL
//! context can be created.

use fox_raytrace::{Environment, EnvMap, Material, RenderCamera, RenderMesh, RenderScene, RenderSettings, material};
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
