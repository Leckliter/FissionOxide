//! The offline path tracer: a scene of triangle meshes with materials, lit by an environment
//! map, traced progressively (a sample per pixel per pass) on every core.
//!
//! Each sample's random numbers depend only on its pixel and sample index, so the same scene
//! and settings give the same image bit for bit, however many threads ran it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::bsdf::Surface;
use crate::bvh::Bvh;
use crate::camera::{Lens, RenderCamera};
use crate::env::{Background, EnvMap, Environment};
use crate::material::Material;
use crate::sampling::{Rng, basis, luminance, power_heuristic, srgb3};
use crate::texture::{Texture, project};

/// One mesh of a render scene, all of one material.
#[derive(Debug, Clone, Default)]
pub struct RenderMesh {
  pub positions: Vec<[f32; 3]>,
  pub normals: Vec<[f32; 3]>,
  pub indices: Vec<u32>,
  /// Index into [`RenderScene::materials`].
  pub material: usize,
  /// Where texture projections are centred (the body's bounding-box centre).
  pub origin: [f32; 3],
}

/// Everything a render needs.
#[derive(Debug, Clone, Default)]
pub struct RenderScene {
  pub meshes: Vec<RenderMesh>,
  pub materials: Vec<Material>,
  pub environment: Environment,
  pub camera: RenderCamera,
}

/// Image size and quality.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderSettings {
  pub width: u32,
  pub height: u32,
  /// Samples per pixel.
  pub samples: u32,
  /// Longest light path (bounces).
  pub max_bounces: u32,
  /// Run the denoiser on the finished image.
  pub denoise: bool,
  /// Varies the noise pattern; the same seed gives the same image.
  pub seed: u32,
  /// Worker threads (0 = one per core).
  pub threads: usize,
}

impl Default for RenderSettings {
  fn default() -> Self {
    RenderSettings { width: 1280, height: 720, samples: 128, max_bounces: 8, denoise: true, seed: 0, threads: 0 }
  }
}

/// Largest image side.
pub const MAX_SIZE: u32 = 8192;
/// How strongly the ground's shadows darken it.
const SHADOW_STRENGTH: f32 = 0.85;
/// Indirect light above this luminance is clamped (kills fireflies at low sample counts).
const INDIRECT_CLAMP: f32 = 20.0;
const TILE: u32 = 32;
const EPS: f32 = 1e-4;

/// A scene prepared for tracing: one BVH over every triangle, textures loaded, the environment
/// map built.
pub struct Prepared {
  bvh: Bvh,
  /// Per triangle: vertex normals, material, mesh.
  normals: Vec<[Vec3; 3]>,
  tri_mesh: Vec<u32>,
  verts: Vec<[Vec3; 3]>,
  mesh_material: Vec<usize>,
  mesh_origin: Vec<Vec3>,
  materials: Vec<Material>,
  textures: Vec<Option<Texture>>,
  pub env: EnvMap,
  environment: Environment,
  camera: RenderCamera,
  /// Height of the ground plane (`None`: no ground).
  ground: Option<f32>,
  /// Scale of the scene (for ray offsets).
  eps: f32,
  /// Problems that didn't stop the render (an unreadable texture).
  pub warnings: Vec<String>,
}

impl Prepared {
  pub fn new(scene: &RenderScene) -> Result<Prepared, String> {
    let env = EnvMap::build(&scene.environment)?;
    let mut verts = Vec::new();
    let mut normals = Vec::new();
    let mut tri_mesh = Vec::new();
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for (mi, m) in scene.meshes.iter().enumerate() {
      for t in m.indices.chunks_exact(3) {
        let Some(p) =
          t.iter().map(|&i| m.positions.get(i as usize).map(|p| Vec3::from(*p))).collect::<Option<Vec<_>>>()
        else {
          continue;
        };
        let face_n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if face_n == Vec3::ZERO {
          continue;
        }
        let n = [t[0], t[1], t[2]].map(|i| {
          m.normals
            .get(i as usize)
            .map(|n| Vec3::from(*n).normalize_or_zero())
            .filter(|n| *n != Vec3::ZERO)
            .unwrap_or(face_n)
        });
        for q in &p {
          lo = lo.min(*q);
          hi = hi.max(*q);
        }
        verts.push([p[0], p[1], p[2]]);
        normals.push(n);
        tri_mesh.push(mi as u32);
      }
    }
    let bvh = Bvh::new(&verts);
    let mut warnings = Vec::new();
    let textures = scene
      .materials
      .iter()
      .map(|m| match Texture::for_material(m) {
        Some(Ok(t)) => Some(t),
        Some(Err(e)) => {
          warnings.push(format!("material '{}': {e}", m.name));
          None
        }
        None => None,
      })
      .collect();
    let size = if verts.is_empty() { 100.0 } else { (hi - lo).length().max(1e-3) };
    let ground =
      (scene.environment.ground && !verts.is_empty()).then(|| lo.z - scene.environment.ground_offset.max(0.0));
    Ok(Prepared {
      bvh,
      normals,
      tri_mesh,
      verts,
      mesh_material: scene.meshes.iter().map(|m| m.material).collect(),
      mesh_origin: scene.meshes.iter().map(|m| Vec3::from(m.origin)).collect(),
      materials: scene.materials.clone(),
      textures,
      env,
      environment: scene.environment.clone(),
      camera: scene.camera,
      ground,
      eps: size * EPS,
      warnings,
    })
  }

  pub fn triangle_count(&self) -> usize {
    self.verts.len()
  }

  fn surface(&self, mi: usize, p: Vec3, n: Vec3) -> Surface {
    let fallback = Material::default();
    let m = self.materials.get(mi).unwrap_or(&fallback);
    let mut base = srgb3(m.color);
    let mut roughness = m.roughness;
    if let (Some(spec), Some(Some(tex))) = (&m.texture, self.textures.get(mi)) {
      let c = tex.sample(project(spec, p, n));
      base = c;
      if spec.roughness_variation != 0.0 {
        roughness = (roughness + (luminance(c) / luminance(srgb3(m.color)).max(1e-3) - 1.0) * spec.roughness_variation)
          .clamp(0.0, 1.0);
      }
    }
    Surface {
      base,
      metallic: m.metallic.clamp(0.0, 1.0),
      alpha: roughness.clamp(0.0, 1.0).powi(2),
      clearcoat: m.clearcoat.clamp(0.0, 1.0),
      cc_alpha: m.clearcoat_roughness.clamp(0.0, 1.0).powi(2),
      transmission: m.transmission.clamp(0.0, 1.0),
      ior: m.ior.clamp(1.0, 3.0),
      emission: srgb3(m.emission) * m.emission_strength.max(0.0),
    }
  }

  fn ground_hit(&self, o: Vec3, d: Vec3) -> Option<f32> {
    let z = self.ground?;
    if d.z >= -1e-9 || o.z <= z {
      return None;
    }
    Some((z - o.z) / d.z)
  }

  /// Whether light from direction `d` at `p` is blocked (by the model or, below the horizon, the
  /// ground).
  fn blocked(&self, p: Vec3, d: Vec3) -> bool {
    if self.ground.is_some() && d.z < 0.0 {
      return true;
    }
    self.bvh.occluded(p, d, f32::INFINITY)
  }

  /// One camera path. `bg` is the solid/gradient background's radiance behind this pixel (what
  /// glass shows of it). Returns what [`Accum`] collects for it.
  fn trace(&self, lens: &Lens, px: f32, py: f32, bg: Vec3, max_bounces: u32, rng: &mut Rng) -> PathResult {
    let (mut o, mut d) = lens.ray(px, py, rng);
    let env_bg = self.environment.background == Background::Environment;
    let mut out = PathResult::default();
    let mut throughput = Vec3::ONE;
    let mut last_pdf = 0.0f32;
    let mut last_delta = true;
    // every bounce so far was a mirror or glass one: the camera sees through them as through air
    let mut chain = true;
    let mut via_ground = false;
    for bounce in 0..=max_bounces {
      let hit = self.bvh.intersect(o, d, f32::INFINITY);
      let g = self.ground_hit(o, d).filter(|t| hit.is_none_or(|h| *t < h.t));
      if hit.is_none() && g.is_none() {
        // the environment
        let rad = self.env.lookup(d);
        if bounce == 0 {
          if env_bg {
            out.radiance += rad;
          } else {
            out.background += 1.0;
          }
          out.albedo = if env_bg { rad.min(Vec3::ONE) } else { Vec3::ONE };
          out.normal = -d;
          out.depth = f32::INFINITY;
        } else if chain && !env_bg {
          // the background seen through glass or in a mirror
          if !via_ground {
            self.add(&mut out, throughput * bg, bounce);
          }
        } else if env_bg || !via_ground {
          let w = if last_delta { 1.0 } else { power_heuristic(last_pdf, self.env.pdf(d)) };
          self.add(&mut out, throughput * rad * w, bounce);
        }
        break;
      }
      if let Some(t) = g {
        let p = o + d * t;
        if chain {
          // the camera sees the ground: a shadow catcher showing the background behind it
          let (wi, _, _) = self.env.sample(rng.f32(), rng.f32());
          let shadow =
            if self.environment.shadows && wi.z > 0.0 && self.bvh.occluded(p + Vec3::Z * self.eps, wi, f32::INFINITY) {
              SHADOW_STRENGTH
            } else {
              0.0
            };
          if env_bg {
            self.add(&mut out, throughput * self.env.lookup(d) * (1.0 - shadow), bounce);
          } else if bounce == 0 {
            out.background += 1.0 - shadow;
            out.shadow += shadow;
          } else {
            self.add(&mut out, throughput * bg * (1.0 - shadow), bounce);
          }
          if bounce == 0 {
            out.albedo = Vec3::ONE;
            out.normal = Vec3::Z;
            out.depth = t;
          }
          let r = self.environment.ground_reflection.clamp(0.0, 1.0);
          if r <= 0.0 || via_ground {
            break;
          }
          // a mirror-like ground reflects the model
          let fresnel = 0.04 + 0.96 * (1.0 - (-d.z)).powi(5);
          throughput *= r * fresnel.max(0.3);
          o = p + Vec3::Z * self.eps;
          d = Vec3::new(d.x, d.y, -d.z);
          last_delta = true;
          via_ground = true;
          continue;
        }
        // seen indirectly, the ground is a grey matte floor
        let s = Surface {
          base: Vec3::splat(0.5),
          metallic: 0.0,
          alpha: 0.8,
          clearcoat: 0.0,
          cc_alpha: 0.0,
          transmission: 0.0,
          ior: 1.5,
          emission: Vec3::ZERO,
        };
        if !self.shade(
          &mut out,
          &s,
          p,
          Vec3::Z,
          Vec3::Z,
          d,
          bounce,
          &mut throughput,
          &mut o,
          &mut d,
          &mut last_pdf,
          &mut last_delta,
          rng,
        ) {
          break;
        }
        chain = false;
        continue;
      }
      let Some(h) = hit else { break };
      let tri = h.tri as usize;
      let [a, b, c] = self.verts[tri];
      let p = o + d * h.t;
      let mut ng = (b - a).cross(c - a).normalize_or_zero();
      let [na, nb, nc] = self.normals[tri];
      let mut ns = (na * (1.0 - h.u - h.v) + nb * h.u + nc * h.v).normalize_or_zero();
      if ns == Vec3::ZERO {
        ns = ng;
      }
      let mesh = self.tri_mesh[tri] as usize;
      let mi = self.mesh_material[mesh];
      let s = self.surface(mi, p - self.mesh_origin[mesh], ns);
      let entering = d.dot(ng) < 0.0;
      if s.transmission <= 0.0 || s.metallic >= 1.0 {
        // opaque surfaces are two-sided
        if !entering {
          ng = -ng;
        }
        if ns.dot(ng) < 0.0 {
          ns = -ns;
        }
      } else if ns.dot(ng) < 0.0 {
        ns = -ns;
      }
      if bounce == 0 {
        out.albedo = s.base;
        out.normal = if entering { ns } else { -ns };
        out.depth = h.t;
        out.coverage = 1.0;
      }
      if s.emission != Vec3::ZERO {
        self.add(&mut out, throughput * s.emission, bounce);
      }
      if !self.shade(
        &mut out,
        &s,
        p,
        ng,
        ns,
        d,
        bounce,
        &mut throughput,
        &mut o,
        &mut d,
        &mut last_pdf,
        &mut last_delta,
        rng,
      ) {
        break;
      }
      chain &= last_delta;
      if bounce >= 3 {
        let q = throughput.max_element().clamp(0.05, 0.95);
        if rng.f32() > q {
          break;
        }
        throughput /= q;
      }
    }
    out
  }

  fn add(&self, out: &mut PathResult, c: Vec3, bounce: u32) {
    let c = if bounce > 1 {
      let l = luminance(c);
      if l > INDIRECT_CLAMP { c * (INDIRECT_CLAMP / l) } else { c }
    } else {
      c
    };
    if c.is_finite() {
      out.radiance += c;
    }
  }

  /// Light-samples the environment at a surface point and draws the next direction. False ends
  /// the path.
  #[allow(clippy::too_many_arguments)]
  fn shade(
    &self,
    out: &mut PathResult,
    s: &Surface,
    p: Vec3,
    ng: Vec3,
    ns: Vec3,
    d: Vec3,
    bounce: u32,
    throughput: &mut Vec3,
    o: &mut Vec3,
    dir: &mut Vec3,
    last_pdf: &mut f32,
    last_delta: &mut bool,
    rng: &mut Rng,
  ) -> bool {
    let (t, b) = basis(ns);
    let to_local = |v: Vec3| Vec3::new(v.dot(t), v.dot(b), v.dot(ns));
    let to_world = |v: Vec3| t * v.x + b * v.y + ns * v.z;
    let wo = to_local(-d);
    let entering = d.dot(ng) < 0.0;
    // next-event estimation toward the environment
    if !s.pure_glass() {
      let (wi, rad, pdf_l) = self.env.sample(rng.f32(), rng.f32());
      if pdf_l > 0.0 && wi.dot(ng) > 0.0 {
        let (f, pdf_b) = s.eval(wo, to_local(wi));
        if pdf_b > 0.0 && !self.blocked(p + ng * self.eps, wi) {
          let w = power_heuristic(pdf_l, pdf_b);
          let cos = wi.dot(ns).max(0.0);
          self.add(out, *throughput * f * rad * (cos * w / pdf_l), bounce + 1);
        }
      }
    }
    let Some(smp) = s.sample(wo, entering, rng) else { return false };
    let wi = to_world(smp.wi);
    *throughput *= smp.weight;
    if !throughput.is_finite() || throughput.max_element() <= 0.0 {
      return false;
    }
    // leave on the side the new ray goes
    let side = if wi.dot(ng) >= 0.0 { ng } else { -ng };
    *o = p + side * self.eps;
    *dir = wi.normalize();
    *last_pdf = smp.pdf;
    *last_delta = smp.delta;
    true
  }
}

/// What one camera path contributes to its pixel.
#[derive(Debug, Clone, Copy, Default)]
struct PathResult {
  /// Light reaching the camera (the model, and the environment when it's the background).
  radiance: Vec3,
  /// How much of a solid or gradient background shows (shadowed ground shows less).
  background: f32,
  /// Ground shadow (for a transparent background's alpha).
  shadow: f32,
  /// The model covers the pixel.
  coverage: f32,
  albedo: Vec3,
  normal: Vec3,
  depth: f32,
}

/// Per-pixel sums over the samples taken so far.
#[derive(Debug, Clone)]
pub struct Accum {
  pub width: u32,
  pub height: u32,
  pub samples: u32,
  radiance: Vec<Vec3>,
  lum2: Vec<f32>,
  background: Vec<f32>,
  shadow: Vec<f32>,
  coverage: Vec<f32>,
  albedo: Vec<Vec3>,
  normal: Vec<Vec3>,
  depth: Vec<f32>,
}

impl Accum {
  fn new(width: u32, height: u32) -> Accum {
    let n = (width * height) as usize;
    Accum {
      width,
      height,
      samples: 0,
      radiance: vec![Vec3::ZERO; n],
      lum2: vec![0.0; n],
      background: vec![0.0; n],
      shadow: vec![0.0; n],
      coverage: vec![0.0; n],
      albedo: vec![Vec3::ZERO; n],
      normal: vec![Vec3::ZERO; n],
      depth: vec![0.0; n],
    }
  }
}

/// A progressive render: call [`Progressive::step`] repeatedly, and [`Progressive::image`] for
/// the picture so far.
pub struct Progressive {
  pub prepared: Prepared,
  pub settings: RenderSettings,
  lens: Lens,
  pub accum: Accum,
}

impl Progressive {
  pub fn new(scene: &RenderScene, settings: RenderSettings) -> Result<Progressive, String> {
    let mut settings = settings;
    settings.width = settings.width.clamp(1, MAX_SIZE);
    settings.height = settings.height.clamp(1, MAX_SIZE);
    settings.samples = settings.samples.max(1);
    let prepared = Prepared::new(scene)?;
    let lens = Lens::new(&scene.camera, settings.width, settings.height);
    let accum = Accum::new(settings.width, settings.height);
    Ok(Progressive { prepared, settings, lens, accum })
  }

  pub fn done(&self) -> bool {
    self.accum.samples >= self.settings.samples
  }

  /// Adds up to `n` samples per pixel (fewer if the target count is reached or `cancel` is set).
  /// Returns how many were added.
  pub fn step(&mut self, n: u32, cancel: &AtomicBool) -> u32 {
    let n = n.min(self.settings.samples.saturating_sub(self.accum.samples));
    if n == 0 {
      return 0;
    }
    let (w, h) = (self.settings.width, self.settings.height);
    let tiles_x = w.div_ceil(TILE);
    let tiles = (tiles_x * h.div_ceil(TILE)) as usize;
    let threads = match self.settings.threads {
      0 => std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
      t => t,
    }
    .clamp(1, 256);
    let next = AtomicUsize::new(0);
    let first = self.accum.samples;
    let results: Mutex<Vec<(usize, Vec<PathResult>, Vec<f32>)>> = Mutex::new(Vec::with_capacity(tiles));
    let this = &*self;
    let exposure = 2f32.powf(self.prepared.camera.exposure);
    // the background behind each row, as radiance (what glass shows of a solid background)
    let rows: Vec<Vec3> = (0..h)
      .map(|y| {
        let c = background_color(&self.prepared.environment, y, h);
        Vec3::new(srgb_to_linear_f(c.x), srgb_to_linear_f(c.y), srgb_to_linear_f(c.z)) / exposure
      })
      .collect();
    let rows = &rows;
    std::thread::scope(|sc| {
      for _ in 0..threads.min(tiles) {
        sc.spawn(|| {
          loop {
            if cancel.load(Ordering::Relaxed) {
              return;
            }
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= tiles {
              return;
            }
            let (tx, ty) = (i as u32 % tiles_x * TILE, i as u32 / tiles_x * TILE);
            let (tw, th) = (TILE.min(w - tx), TILE.min(h - ty));
            let mut px_out = Vec::with_capacity((tw * th) as usize);
            let mut l2 = Vec::with_capacity((tw * th) as usize);
            for y in ty..ty + th {
              for x in tx..tx + tw {
                let mut sum = PathResult::default();
                let mut sq = 0.0;
                for k in first..first + n {
                  let mut rng = Rng::for_sample(x, y, k, this.settings.seed);
                  let (jx, jy) = rng.pair();
                  let r = this.prepared.trace(
                    &this.lens,
                    x as f32 + jx,
                    y as f32 + jy,
                    rows[y as usize],
                    this.settings.max_bounces,
                    &mut rng,
                  );
                  let l = luminance(r.radiance);
                  sq += l * l;
                  sum.radiance += r.radiance;
                  sum.background += r.background;
                  sum.shadow += r.shadow;
                  sum.coverage += r.coverage;
                  sum.albedo += r.albedo;
                  sum.normal += r.normal;
                  sum.depth += if r.depth.is_finite() { r.depth } else { 1e9 };
                }
                px_out.push(sum);
                l2.push(sq);
              }
            }
            if let Ok(mut v) = results.lock() {
              v.push((i, px_out, l2));
            }
          }
        });
      }
    });
    let results = results.into_inner().unwrap_or_default();
    if results.len() < tiles {
      // cancelled part way: drop the partial pass so every pixel keeps the same sample count
      return 0;
    }
    for (i, px, l2) in results {
      let (tx, ty) = (i as u32 % tiles_x * TILE, i as u32 / tiles_x * TILE);
      let tw = TILE.min(w - tx);
      for (k, r) in px.into_iter().enumerate() {
        let (x, y) = (tx + k as u32 % tw, ty + k as u32 / tw);
        let j = (y * w + x) as usize;
        let a = &mut self.accum;
        a.radiance[j] += r.radiance;
        a.lum2[j] += l2[k];
        a.background[j] += r.background;
        a.shadow[j] += r.shadow;
        a.coverage[j] += r.coverage;
        a.albedo[j] += r.albedo;
        a.normal[j] += r.normal;
        a.depth[j] += r.depth;
      }
    }
    self.accum.samples += n;
    n
  }

  /// Runs to the target sample count, reporting progress (samples done) after each pass.
  pub fn run(&mut self, cancel: &AtomicBool, mut progress: impl FnMut(u32, u32)) {
    while !self.done() && !cancel.load(Ordering::Relaxed) {
      // early passes are short so a preview shows quickly
      let pass = (self.accum.samples / 2).clamp(1, 16);
      if self.step(pass, cancel) == 0 {
        break;
      }
      progress(self.accum.samples, self.settings.samples);
    }
  }

  /// The image so far (denoised when `denoise`).
  pub fn image(&self, denoise: bool) -> RenderImage {
    crate::output::finish(self, denoise)
  }

  pub(crate) fn environment(&self) -> &Environment {
    &self.prepared.environment
  }

  pub(crate) fn camera(&self) -> &RenderCamera {
    &self.prepared.camera
  }

  /// Per-pixel means for output: (radiance, background, shadow, coverage, albedo, normal,
  /// depth, variance of the radiance's mean luminance).
  pub(crate) fn means(&self) -> Means {
    let a = &self.accum;
    let k = 1.0 / a.samples.max(1) as f32;
    let var = a
      .radiance
      .iter()
      .zip(&a.lum2)
      .map(|(r, l2)| {
        let m = luminance(*r) * k;
        ((l2 * k - m * m).max(0.0)) * k
      })
      .collect();
    Means {
      radiance: a.radiance.iter().map(|c| *c * k).collect(),
      background: a.background.iter().map(|c| c * k).collect(),
      shadow: a.shadow.iter().map(|c| c * k).collect(),
      coverage: a.coverage.iter().map(|c| c * k).collect(),
      albedo: a.albedo.iter().map(|c| *c * k).collect(),
      normal: a.normal.iter().map(|c| c.normalize_or_zero()).collect(),
      depth: a.depth.iter().map(|c| c * k).collect(),
      variance: var,
    }
  }
}

pub(crate) struct Means {
  pub radiance: Vec<Vec3>,
  pub background: Vec<f32>,
  pub shadow: Vec<f32>,
  pub coverage: Vec<f32>,
  pub albedo: Vec<Vec3>,
  pub normal: Vec<Vec3>,
  pub depth: Vec<f32>,
  pub variance: Vec<f32>,
}

/// A finished (or in-progress) picture.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderImage {
  pub width: u32,
  pub height: u32,
  /// sRGB, straight alpha, top row first.
  pub rgba: Vec<u8>,
  pub samples: u32,
}

impl RenderImage {
  pub fn to_png(&self) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
      .write_image(&self.rgba, self.width, self.height, image::ExtendedColorType::Rgba8)
      .map_err(|e| e.to_string())?;
    Ok(out)
  }

  pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
    std::fs::write(path, self.to_png()?).map_err(|e| format!("{}: {e}", path.display()))
  }
}

use image::ImageEncoder;

/// Renders a scene to completion.
pub fn render(scene: &RenderScene, settings: RenderSettings) -> Result<RenderImage, String> {
  let mut p = Progressive::new(scene, settings)?;
  let cancel = AtomicBool::new(false);
  p.run(&cancel, |_, _| {});
  Ok(p.image(settings.denoise))
}

fn srgb_to_linear_f(c: f32) -> f32 {
  if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Background colour (display sRGB 0-1) at pixel row `y` of `height`.
pub(crate) fn background_color(env: &Environment, y: u32, height: u32) -> Vec3 {
  let top = Vec3::from(env.background_color.map(|c| c as f32 / 255.0));
  match env.background {
    Background::Gradient => {
      let bottom = Vec3::from(env.background_color2.map(|c| c as f32 / 255.0));
      top.lerp(bottom, y as f32 / height.saturating_sub(1).max(1) as f32)
    }
    _ => top,
  }
}
