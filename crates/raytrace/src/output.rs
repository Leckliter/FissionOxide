//! From accumulated samples to a picture: denoise, expose, tone map, put the background behind.

use glam::Vec3;

use crate::denoise::{Guides, denoise};
use crate::env::Background;
use crate::sampling::linear_to_srgb;
use crate::tracer::{Progressive, RenderImage, background_color};

pub(crate) fn finish(p: &Progressive, do_denoise: bool) -> RenderImage {
  let (w, h) = (p.accum.width, p.accum.height);
  let mut m = p.means();
  if do_denoise && p.accum.samples > 0 {
    let g = Guides { albedo: &m.albedo, normal: &m.normal, depth: &m.depth, variance: &m.variance };
    let mut c = m.radiance.clone();
    denoise(&mut c, w, h, &g);
    m.radiance = c;
  }
  let env = p.environment();
  let cam = p.camera();
  let exposure = 2f32.powf(cam.exposure);
  let transparent = env.background == Background::Transparent;
  let mut rgba = Vec::with_capacity((w * h * 4) as usize);
  for y in 0..h {
    let bg = background_color(env, y, h);
    for x in 0..w {
      let i = (y * w + x) as usize;
      let t = cam.tone_map.apply(m.radiance[i] * exposure);
      let c = Vec3::new(linear_to_srgb(t.x), linear_to_srgb(t.y), linear_to_srgb(t.z));
      let (rgb, a) = if env.background == Background::Environment {
        (c, 1.0)
      } else if transparent {
        // the model over nothing; shadows as translucent black
        let a = (m.coverage[i] + m.shadow[i]).clamp(0.0, 1.0);
        let rgb = if a > 1e-4 { c / a } else { Vec3::ZERO };
        (rgb.min(Vec3::ONE), a)
      } else {
        (c + bg * m.background[i], 1.0)
      };
      let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
      rgba.extend_from_slice(&[to8(rgb.x), to8(rgb.y), to8(rgb.z), to8(a)]);
    }
  }
  RenderImage { width: w, height: h, rgba, samples: p.accum.samples }
}
