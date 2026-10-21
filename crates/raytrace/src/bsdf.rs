//! The surface model: a Lambert base and a GGX specular lobe (metallic/roughness), under an
//! optional clear coat, mixed with a rough dielectric for transmission. Directions are in the
//! local frame, +Z along the shading normal.

use std::f32::consts::PI;

use glam::Vec3;

use crate::sampling::{Rng, cosine_hemisphere, luminance};

/// A material evaluated at one point.
#[derive(Debug, Clone, Copy)]
pub struct Surface {
  pub base: Vec3,
  pub metallic: f32,
  /// GGX alpha (roughness²).
  pub alpha: f32,
  pub clearcoat: f32,
  pub cc_alpha: f32,
  pub transmission: f32,
  pub ior: f32,
  pub emission: Vec3,
}

const MIN_ALPHA: f32 = 1e-3;

fn ggx_d(m: Vec3, a: f32) -> f32 {
  let a2 = a * a;
  let c = m.z * m.z;
  let d = c * (a2 - 1.0) + 1.0;
  a2 / (PI * d * d).max(1e-12)
}

fn smith_g1(v: Vec3, a: f32) -> f32 {
  let c2 = (v.z * v.z).max(1e-8);
  let t2 = (1.0 - c2).max(0.0) / c2;
  2.0 / (1.0 + (1.0 + a * a * t2).sqrt())
}

/// Heitz's visible-normal sampling.
fn sample_vndf(v: Vec3, a: f32, u1: f32, u2: f32) -> Vec3 {
  let vh = Vec3::new(a * v.x, a * v.y, v.z).normalize();
  let lensq = vh.x * vh.x + vh.y * vh.y;
  let t1 = if lensq > 0.0 { Vec3::new(-vh.y, vh.x, 0.0) / lensq.sqrt() } else { Vec3::X };
  let t2 = vh.cross(t1);
  let r = u1.sqrt();
  let phi = 2.0 * PI * u2;
  let p1 = r * phi.cos();
  let s = 0.5 * (1.0 + vh.z);
  let p2 = (1.0 - s) * (1.0 - p1 * p1).max(0.0).sqrt() + s * r * phi.sin();
  let nh = t1 * p1 + t2 * p2 + vh * (1.0 - p1 * p1 - p2 * p2).max(0.0).sqrt();
  Vec3::new(a * nh.x, a * nh.y, nh.z.max(1e-6)).normalize()
}

/// pdf of reflecting `wo` to `wi` about a VNDF-sampled normal.
fn vndf_reflect_pdf(wo: Vec3, wi: Vec3, a: f32) -> f32 {
  let m = (wo + wi).normalize_or_zero();
  if m.z <= 0.0 || wo.z <= 0.0 {
    return 0.0;
  }
  ggx_d(m, a) * smith_g1(wo, a) / (4.0 * wo.z)
}

fn schlick(f0: Vec3, c: f32) -> Vec3 {
  f0 + (Vec3::ONE - f0) * (1.0 - c.clamp(0.0, 1.0)).powi(5)
}

fn schlick1(f0: f32, c: f32) -> f32 {
  f0 + (1.0 - f0) * (1.0 - c.clamp(0.0, 1.0)).powi(5)
}

/// Exact dielectric Fresnel reflectance; `eta` = n_incident / n_transmitted.
fn fresnel_dielectric(cos_i: f32, eta: f32) -> f32 {
  let sin2_t = eta * eta * (1.0 - cos_i * cos_i).max(0.0);
  if sin2_t >= 1.0 {
    return 1.0;
  }
  let cos_t = (1.0 - sin2_t).sqrt();
  let rs = (eta * cos_i - cos_t) / (eta * cos_i + cos_t);
  let rp = (cos_i - eta * cos_t) / (cos_i + eta * cos_t);
  0.5 * (rs * rs + rp * rp)
}

const CC_F0: f32 = 0.04;

/// What a BSDF sample gave.
pub struct Sample {
  pub wi: Vec3,
  /// f·cos/pdf.
  pub weight: Vec3,
  pub pdf: f32,
  /// Sampled from a lobe that can't be light-sampled (transmission): no MIS.
  pub delta: bool,
}

impl Surface {
  fn f0(&self) -> Vec3 {
    let d = ((self.ior - 1.0) / (self.ior + 1.0)).powi(2);
    Vec3::splat(d).lerp(self.base, self.metallic)
  }

  fn alpha(&self) -> f32 {
    self.alpha.max(MIN_ALPHA)
  }

  /// Share of the glass lobe.
  fn glass_share(&self) -> f32 {
    (1.0 - self.metallic) * self.transmission
  }

  /// Lobe selection probabilities: (diffuse, specular, clearcoat) among the non-glass lobes.
  fn lobe_probs(&self, wo: Vec3) -> (f32, f32, f32) {
    let fs = luminance(schlick(self.f0(), wo.z));
    let diffuse = (1.0 - self.metallic) * (1.0 - self.transmission) * luminance(self.base) * (1.0 - fs);
    let spec = fs.max(0.05);
    let cc = self.clearcoat * schlick1(CC_F0, wo.z) * 2.0;
    let sum = (diffuse + spec + cc).max(1e-6);
    (diffuse / sum, spec / sum, cc / sum)
  }

  /// The non-glass lobes' f (without the cosine) and their mixture pdf (glass share removed).
  pub fn eval(&self, wo: Vec3, wi: Vec3) -> (Vec3, f32) {
    if wo.z <= 0.0 || wi.z <= 0.0 {
      return (Vec3::ZERO, 0.0);
    }
    let a = self.alpha();
    let m = (wo + wi).normalize();
    let f0 = self.f0();
    let fspec = schlick(f0, wo.dot(m));
    let spec = fspec * (ggx_d(m, a) * smith_g1(wo, a) * smith_g1(wi, a) / (4.0 * wo.z * wi.z));
    let fd = luminance(schlick(f0, wo.z));
    let diffuse =
      self.base * ((1.0 - self.metallic) * (1.0 - self.transmission) * (1.0 - fd) * (1.0 - schlick1(f0.x, wi.z)) / PI);
    let mut f = spec + diffuse;
    let (pd, ps, pc) = self.lobe_probs(wo);
    let mut pdf = pd * wi.z / PI + ps * vndf_reflect_pdf(wo, wi, a);
    if self.clearcoat > 0.0 {
      let ca = self.cc_alpha.max(MIN_ALPHA);
      let fc = schlick1(CC_F0, wo.dot(m)) * self.clearcoat;
      let coat = fc * ggx_d(m, ca) * smith_g1(wo, ca) * smith_g1(wi, ca) / (4.0 * wo.z * wi.z);
      // the coat dims what's under it
      let atten = 1.0 - self.clearcoat * schlick1(CC_F0, wo.z);
      f = f * atten + Vec3::splat(coat);
      pdf += pc * vndf_reflect_pdf(wo, wi, ca);
    }
    let g = self.glass_share();
    (f * (1.0 - g), pdf * (1.0 - g))
  }

  /// Draws a direction. `entering`: the ray comes from outside (for transmission).
  pub fn sample(&self, wo: Vec3, entering: bool, rng: &mut Rng) -> Option<Sample> {
    let g = self.glass_share();
    if g > 0.0 && rng.f32() < g {
      return self.sample_glass(wo, entering, rng);
    }
    if wo.z <= 0.0 {
      return None;
    }
    let (pd, ps, _) = self.lobe_probs(wo);
    let (u, (u1, u2)) = (rng.f32(), rng.pair());
    let wi = if u < pd {
      cosine_hemisphere(u1, u2)
    } else {
      let a = if u < pd + ps { self.alpha() } else { self.cc_alpha.max(MIN_ALPHA) };
      let m = sample_vndf(wo, a, u1, u2);
      let wi = 2.0 * wo.dot(m) * m - wo;
      if wi.z <= 0.0 {
        return None;
      }
      wi
    };
    let (f, pdf) = self.eval(wo, wi);
    if pdf <= 0.0 {
      return None;
    }
    Some(Sample { wi, weight: f * wi.z / pdf, pdf, delta: false })
  }

  /// The glass lobe: reflect or refract about a GGX microfacet normal, tinted by the base colour
  /// on the way through.
  fn sample_glass(&self, wo: Vec3, entering: bool, rng: &mut Rng) -> Option<Sample> {
    let a = self.alpha();
    // inside the material the normal faces away; work with the side wo is on
    let flip = wo.z < 0.0;
    let wo_l = if flip { -wo } else { wo };
    let m = if self.alpha <= 0.002 {
      Vec3::Z
    } else {
      let (u1, u2) = rng.pair();
      sample_vndf(wo_l, a, u1, u2)
    };
    let eta = if entering { 1.0 / self.ior } else { self.ior };
    let c = wo_l.dot(m);
    let fr = fresnel_dielectric(c, eta);
    let wi = if rng.f32() < fr {
      2.0 * c * m - wo_l
    } else {
      let sin2_t = eta * eta * (1.0 - c * c).max(0.0);
      let cos_t = (1.0 - sin2_t).max(0.0).sqrt();
      let t = (eta * c - cos_t) * m - eta * wo_l;
      let wi = if flip { -t } else { t };
      // half the tint at each surface, so in and out again gives the colour once
      return Some(Sample { wi: wi.normalize(), weight: self.base.powf(0.5), pdf: 1.0, delta: true });
    };
    if wi.z <= 0.0 {
      return None;
    }
    let wi = if flip { -wi } else { wi };
    Some(Sample { wi, weight: Vec3::ONE, pdf: 1.0, delta: true })
  }

  /// Whether the surface is all glass (nothing to light-sample).
  pub fn pure_glass(&self) -> bool {
    self.glass_share() >= 0.999
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn white(metallic: f32, roughness: f32) -> Surface {
    Surface {
      base: Vec3::ONE,
      metallic,
      alpha: roughness * roughness,
      clearcoat: 0.0,
      cc_alpha: 0.01,
      transmission: 0.0,
      ior: 1.5,
      emission: Vec3::ZERO,
    }
  }

  /// White furnace: a white metal under uniform light reflects (nearly) all of it, and nothing
  /// reflects more than arrives.
  #[test]
  fn conserves_energy() {
    for (metallic, rough) in [(1.0, 0.2), (1.0, 0.7), (0.0, 0.5), (0.0, 0.05)] {
      let s = white(metallic, rough);
      let wo = Vec3::new(0.5, 0.0, 0.866);
      let mut rng = Rng::new(3);
      let n = 20000;
      let mut sum = Vec3::ZERO;
      for _ in 0..n {
        if let Some(x) = s.sample(wo, true, &mut rng) {
          sum += x.weight;
        }
      }
      let e = sum.x / n as f32;
      assert!(e <= 1.02, "metallic {metallic} rough {rough}: {e}");
      // single-scattering GGX loses some energy on rough surfaces
      assert!(e > if rough > 0.5 { 0.6 } else { 0.8 }, "metallic {metallic} rough {rough}: {e}");
    }
  }

  #[test]
  fn eval_and_sample_agree() {
    let s = Surface { clearcoat: 1.0, ..white(0.3, 0.4) };
    let wo = Vec3::new(0.3, 0.2, 0.93).normalize();
    let mut rng = Rng::new(9);
    for _ in 0..200 {
      if let Some(x) = s.sample(wo, true, &mut rng) {
        let (f, pdf) = s.eval(wo, x.wi);
        assert!((pdf - x.pdf).abs() < 1e-4 * pdf.max(1.0));
        assert!((f * x.wi.z / pdf - x.weight).length() < 1e-3);
      }
    }
  }
}
