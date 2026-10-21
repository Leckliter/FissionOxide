//! Form primitives, as Fusion's Create Form panel: box, plane, cylinder, sphere, torus, quadball
//! and face. Each is a cage in its own coordinates (z up), faces counter-clockwise seen from
//! outside. The round ones are scaled so their smooth body has the size asked for.

use std::collections::HashMap;
use std::f64::consts::TAU;

use glam::DVec3;

use crate::mesh::Cage;
use crate::subd::limit_mesh;

fn check_sizes(sizes: &[f64]) -> Result<(), String> {
  if sizes.iter().all(|v| v.is_finite() && *v > 0.0) { Ok(()) } else { Err("sizes must be positive".into()) }
}

fn check_counts(counts: &[u32], lo: u32) -> Result<(), String> {
  if counts.iter().all(|c| (lo..=64).contains(c)) { Ok(()) } else { Err(format!("face counts must be {lo} to 64")) }
}

/// A cage built from a grid of points `pt(i, j)` with quads (i, j) → (i + 1, j) → (i + 1, j + 1);
/// `wrap_i` joins the last column to the first.
struct Builder {
  cage: Cage,
  index: HashMap<(i64, i64, i64), u32>,
}

impl Builder {
  fn new() -> Builder {
    Builder { cage: Cage::default(), index: HashMap::new() }
  }

  /// The vertex keyed `k`, made at `p` the first time.
  fn v(&mut self, k: (i64, i64, i64), p: DVec3) -> u32 {
    let cage = &mut self.cage;
    *self.index.entry(k).or_insert_with(|| {
      cage.vertices.push(p.to_array());
      (cage.vertices.len() - 1) as u32
    })
  }

  fn face(&mut self, f: Vec<u32>) {
    self.cage.faces.push(f);
  }
}

/// Fusion's Form > Box: a `length` (x) by `width` (y) by `height` (z) box cage centred on the
/// origin in x and y and standing on z = 0, with `counts` faces along each side.
pub fn quad_box(length: f64, width: f64, height: f64, counts: [u32; 3]) -> Result<Cage, String> {
  check_sizes(&[length, width, height]).map_err(|_| "the box's length, width and height must be positive")?;
  if counts.iter().any(|c| !(1..=32).contains(c)) {
    return Err("each side needs 1 to 32 faces".into());
  }
  let [nx, ny, nz] = counts.map(i64::from);
  let mut b = Builder::new();
  let vid = |i: i64, j: i64, k: i64, b: &mut Builder| {
    let p = DVec3::new(
      length * (i as f64 / nx as f64 - 0.5),
      width * (j as f64 / ny as f64 - 0.5),
      height * k as f64 / nz as f64,
    );
    b.v((i, j, k), p)
  };
  // each side's quads, outward
  for a in 0..nx {
    for c in 0..ny {
      let f =
        vec![vid(a, c, 0, &mut b), vid(a, c + 1, 0, &mut b), vid(a + 1, c + 1, 0, &mut b), vid(a + 1, c, 0, &mut b)];
      b.face(f);
      let f = vec![
        vid(a, c, nz, &mut b),
        vid(a + 1, c, nz, &mut b),
        vid(a + 1, c + 1, nz, &mut b),
        vid(a, c + 1, nz, &mut b),
      ];
      b.face(f);
    }
  }
  for a in 0..nx {
    for c in 0..nz {
      let f =
        vec![vid(a, 0, c, &mut b), vid(a + 1, 0, c, &mut b), vid(a + 1, 0, c + 1, &mut b), vid(a, 0, c + 1, &mut b)];
      b.face(f);
      let f = vec![
        vid(a, ny, c, &mut b),
        vid(a, ny, c + 1, &mut b),
        vid(a + 1, ny, c + 1, &mut b),
        vid(a + 1, ny, c, &mut b),
      ];
      b.face(f);
    }
  }
  for a in 0..ny {
    for c in 0..nz {
      let f =
        vec![vid(0, a, c, &mut b), vid(0, a, c + 1, &mut b), vid(0, a + 1, c + 1, &mut b), vid(0, a + 1, c, &mut b)];
      b.face(f);
      let f = vec![
        vid(nx, a, c, &mut b),
        vid(nx, a + 1, c, &mut b),
        vid(nx, a + 1, c + 1, &mut b),
        vid(nx, a, c + 1, &mut b),
      ];
      b.face(f);
    }
  }
  Ok(b.cage)
}

/// Form > Plane: an open `length` (x) by `width` (y) grid on z = 0, centred, facing +z.
pub fn plane(length: f64, width: f64, counts: [u32; 2]) -> Result<Cage, String> {
  check_sizes(&[length, width])?;
  check_counts(&counts, 1)?;
  let [nx, ny] = counts.map(i64::from);
  let mut b = Builder::new();
  let vid = |i: i64, j: i64, b: &mut Builder| {
    b.v((i, j, 0), DVec3::new(length * (i as f64 / nx as f64 - 0.5), width * (j as f64 / ny as f64 - 0.5), 0.0))
  };
  for i in 0..nx {
    for j in 0..ny {
      let f = vec![vid(i, j, &mut b), vid(i + 1, j, &mut b), vid(i + 1, j + 1, &mut b), vid(i, j + 1, &mut b)];
      b.face(f);
    }
  }
  Ok(b.cage)
}

/// Form > Cylinder: a tube of `diameter` and `height` standing on z = 0 round the z axis, open at
/// both ends unless `capped` (then each end is one many-sided face).
pub fn cylinder(diameter: f64, height: f64, around: u32, along: u32, capped: bool) -> Result<Cage, String> {
  check_sizes(&[diameter, height])?;
  check_counts(&[around], 3)?;
  check_counts(&[along], 1)?;
  let (na, nh) = (i64::from(around), i64::from(along));
  let r = diameter / 2.0;
  let mut b = Builder::new();
  let vid = |i: i64, k: i64, b: &mut Builder| {
    let a = TAU * (i.rem_euclid(na)) as f64 / na as f64;
    b.v((i.rem_euclid(na), k, 0), DVec3::new(r * a.cos(), r * a.sin(), height * k as f64 / nh as f64))
  };
  for i in 0..na {
    for k in 0..nh {
      let f = vec![vid(i, k, &mut b), vid(i + 1, k, &mut b), vid(i + 1, k + 1, &mut b), vid(i, k + 1, &mut b)];
      b.face(f);
    }
  }
  if capped {
    let bottom: Vec<u32> = (0..na).rev().map(|i| vid(i, 0, &mut b)).collect();
    let top: Vec<u32> = (0..na).map(|i| vid(i, nh, &mut b)).collect();
    b.face(bottom);
    b.face(top);
  }
  let mut cage = b.cage;
  fit_radius(&mut cage, |p| DVec3::new(p.x, p.y, 0.0), r, |p| p.z > 0.25 * height && p.z < 0.75 * height)?;
  Ok(cage)
}

/// Form > Sphere: `around` faces round the z axis and `rings` from pole to pole (three-sided
/// faces at the poles), centred on the origin.
pub fn sphere(diameter: f64, around: u32, rings: u32) -> Result<Cage, String> {
  check_sizes(&[diameter])?;
  check_counts(&[around], 3)?;
  check_counts(&[rings], 2)?;
  let (na, nr) = (i64::from(around), i64::from(rings));
  let r = diameter / 2.0;
  let mut b = Builder::new();
  let vid = |i: i64, k: i64, b: &mut Builder| {
    let polar = std::f64::consts::PI * k as f64 / nr as f64;
    if k == 0 || k == nr {
      return b.v((0, k, 0), DVec3::new(0.0, 0.0, -r * polar.cos()));
    }
    let a = TAU * (i.rem_euclid(na)) as f64 / na as f64;
    let ring = r * polar.sin();
    b.v((i.rem_euclid(na), k, 0), DVec3::new(ring * a.cos(), ring * a.sin(), -r * polar.cos()))
  };
  for i in 0..na {
    for k in 0..nr {
      let f = if k == 0 {
        vec![vid(i, 0, &mut b), vid(i + 1, 1, &mut b), vid(i, 1, &mut b)]
      } else if k == nr - 1 {
        vec![vid(i, k, &mut b), vid(i + 1, k, &mut b), vid(i, nr, &mut b)]
      } else {
        vec![vid(i, k, &mut b), vid(i + 1, k, &mut b), vid(i + 1, k + 1, &mut b), vid(i, k + 1, &mut b)]
      };
      b.face(f);
    }
  }
  let mut cage = b.cage;
  fit_radius(&mut cage, |p| p, r, |_| true)?;
  Ok(cage)
}

/// Form > Torus: a ring of `major` diameter (through the tube's centre) and tube `minor` diameter
/// round the z axis, with `around` faces round the ring and `tube` round the tube.
pub fn torus(major: f64, minor: f64, around: u32, tube: u32) -> Result<Cage, String> {
  check_sizes(&[major, minor])?;
  if minor >= major {
    return Err("the tube's diameter must be less than the ring's".into());
  }
  check_counts(&[around, tube], 3)?;
  let (na, nt) = (i64::from(around), i64::from(tube));
  let (big, small) = (major / 2.0, minor / 2.0);
  let mut b = Builder::new();
  let vid = |i: i64, k: i64, b: &mut Builder| {
    let (a, c) = (TAU * i.rem_euclid(na) as f64 / na as f64, TAU * k.rem_euclid(nt) as f64 / nt as f64);
    let rr = big + small * c.cos();
    b.v((i.rem_euclid(na), k.rem_euclid(nt), 0), DVec3::new(rr * a.cos(), rr * a.sin(), small * c.sin()))
  };
  for i in 0..na {
    for k in 0..nt {
      let f = vec![vid(i, k, &mut b), vid(i + 1, k, &mut b), vid(i + 1, k + 1, &mut b), vid(i, k + 1, &mut b)];
      b.face(f);
    }
  }
  let mut cage = b.cage;
  // move and scale each tube section so the smooth ring has the diameters asked for
  for _ in 0..3 {
    let (pos, _) = limit_mesh(&cage, 3)?;
    let ring = pos.iter().map(|p| p.x.hypot(p.y)).sum::<f64>() / pos.len() as f64;
    let tube = pos.iter().map(|p| (p.x.hypot(p.y) - ring).hypot(p.z)).sum::<f64>() / pos.len() as f64;
    let s = small / tube.max(1e-12);
    for v in &mut cage.vertices {
      let p = DVec3::from(*v);
      let dir = DVec3::new(p.x, p.y, 0.0).normalize_or_zero();
      let rel = p - dir * ring;
      *v = (dir * big + rel * s).to_array();
    }
  }
  Ok(cage)
}

/// Form > Quadball: a box cage of `faces` per side squeezed onto a sphere, centred on the origin;
/// all quads, with a three-edged star point at each of the eight corners.
pub fn quadball(diameter: f64, faces: u32) -> Result<Cage, String> {
  check_sizes(&[diameter])?;
  check_counts(&[faces], 1)?;
  let mut cage = quad_box(2.0, 2.0, 2.0, [faces; 3])?;
  for v in &mut cage.vertices {
    let p = DVec3::from(*v) - DVec3::Z;
    *v = (p.normalize() * diameter / 2.0).to_array();
  }
  fit_radius(&mut cage, |p| p, diameter / 2.0, |_| true)?;
  Ok(cage)
}

/// Form > Face: one face through the points, counter-clockwise seen from its front.
pub fn face(points: &[[f64; 3]]) -> Result<Cage, String> {
  if points.len() < 3 {
    return Err("a face needs at least three points".into());
  }
  let cage = Cage { vertices: points.to_vec(), faces: vec![(0..points.len() as u32).collect()], ..Cage::default() };
  cage.check()?;
  Ok(cage)
}

/// Scales the cage about `radial(p)`'s origin so the limit surface's mean distance from it (over
/// limit points passing `use_pt`) is `r`.
fn fit_radius(
  cage: &mut Cage,
  radial: impl Fn(DVec3) -> DVec3,
  r: f64,
  use_pt: impl Fn(DVec3) -> bool,
) -> Result<(), String> {
  let (pos, _) = limit_mesh(cage, 3)?;
  let pts: Vec<f64> = pos.iter().filter(|p| use_pt(**p)).map(|p| radial(*p).length()).collect();
  if pts.is_empty() {
    return Ok(());
  }
  let got = pts.iter().sum::<f64>() / pts.len() as f64;
  let s = r / got.max(1e-12);
  for v in &mut cage.vertices {
    let p = DVec3::from(*v);
    let rad = radial(p);
    *v = (p + rad * (s - 1.0)).to_array();
  }
  Ok(())
}
