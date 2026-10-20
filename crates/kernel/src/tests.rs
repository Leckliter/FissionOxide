use super::*;
use fox_sketch::{PtSpec, Sketch, find_profiles};

fn rect_profile(w: f64, h: f64) -> Profile {
  let mut s = Sketch::new();
  s.add_rectangle([0.0, 0.0], [w, h]).unwrap();
  find_profiles(&s).remove(0)
}

fn block() -> Shape {
  Shape::face(&Plane::XY, &rect_profile(40.0, 20.0)).unwrap().extrude(DVec3::new(0.0, 0.0, 10.0)).unwrap()
}

fn volume(s: &Shape) -> f64 {
  s.mass().unwrap().volume
}

#[test]
fn extrude_rectangle() {
  let b = block();
  assert!((volume(&b) - 8000.0).abs() < 1e-6);
  assert_eq!(b.faces().unwrap().len(), 6);
  assert_eq!(b.edges().unwrap().len(), 12);
  assert!(b.is_valid());
}

#[test]
fn extrude_downwards_and_on_other_planes() {
  let f = Shape::face(&Plane::XZ, &rect_profile(10.0, 10.0)).unwrap();
  let s = f.extrude(DVec3::new(0.0, -5.0, 0.0)).unwrap();
  assert!((volume(&s) - 500.0).abs() < 1e-6);
  let m = s.mass().unwrap();
  assert!((m.bbox_min.y + 5.0).abs() < 1e-6);
}

#[test]
fn ring_profile_with_hole() {
  let mut s = Sketch::new();
  s.add_circle(PtSpec::At(0.0, 0.0), 10.0).unwrap();
  s.add_circle(PtSpec::At(0.0, 0.0), 4.0).unwrap();
  let p = find_profiles(&s);
  let ring = p.iter().find(|p| !p.holes.is_empty()).unwrap();
  let solid = Shape::face(&Plane::XY, ring).unwrap().extrude(DVec3::Z * 2.0).unwrap();
  let expected = std::f64::consts::PI * (100.0 - 16.0) * 2.0;
  assert!((volume(&solid) - expected).abs() < 1e-3, "{}", volume(&solid));
}

#[test]
fn coplanar_booleans_that_pure_rust_kernels_fail() {
  let a = block();
  // boss standing on the top face
  let boss = Shape::cylinder(DVec3::new(20.0, 10.0, 10.0), DVec3::Z, 5.0, 8.0).unwrap();
  let u = a.boolean(&boss, BoolOp::Union).unwrap();
  let expected = 8000.0 + std::f64::consts::PI * 25.0 * 8.0;
  assert!((volume(&u) - expected).abs() < 1e-3);
  assert_eq!(u.solid_count(), 1);
  // through hole exactly flush with top and bottom
  let hole = Shape::cylinder(DVec3::new(8.0, 10.0, 0.0), DVec3::Z, 3.0, 10.0).unwrap();
  let c = u.boolean(&hole, BoolOp::Subtract).unwrap();
  assert!((volume(&c) - (expected - std::f64::consts::PI * 9.0 * 10.0)).abs() < 1e-3);
  // box on box sharing faces
  let top = Shape::make_box(DVec3::new(30.0, 0.0, 10.0), DVec3::new(40.0, 20.0, 15.0)).unwrap();
  let d = c.boolean(&top, BoolOp::Union).unwrap();
  assert!(d.is_valid());
  // coplanar faces were merged: the top block's side faces unify with the base's
  let faces = d.faces().unwrap().len();
  assert!(faces < 16, "{faces} faces");
}

#[test]
fn cut_splits_into_two_solids() {
  let a = block();
  let slab = Shape::make_box(DVec3::new(18.0, -1.0, -1.0), DVec3::new(22.0, 21.0, 11.0)).unwrap();
  let r = a.boolean(&slab, BoolOp::Subtract).unwrap();
  assert_eq!(r.solids().unwrap().len(), 2);
  assert!(matches!(a.boolean(&a, BoolOp::Subtract), Err(KernelError::Empty)));
}

#[test]
fn fillet_chamfer_shell() {
  let a = block();
  let edges: Vec<u32> = a.edges().unwrap().iter().filter(|e| (e.mid.z - 10.0).abs() < 1e-6).map(|e| e.id).collect();
  assert_eq!(edges.len(), 4);
  let f = a.fillet(&edges, 2.0).unwrap();
  assert!(f.is_valid());
  assert!(volume(&f) < 8000.0);
  // an impossible fillet is an error, not a crash
  assert!(a.fillet(&edges, 50.0).is_err());
  let c = a.chamfer(&edges, 1.0).unwrap();
  assert!(volume(&c) < 8000.0);
  let top = a.faces().unwrap().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  let sh = a.shell(&[top], 1.0, false).unwrap();
  let inner = 38.0 * 18.0 * 9.0;
  assert!((volume(&sh) - (8000.0 - inner)).abs() < 1e-2, "{}", volume(&sh));
}

#[test]
fn revolve_profile() {
  // a 10x5 rectangle offset 5 from the Z axis on the XZ plane → a tube
  let mut s = Sketch::new();
  s.add_rectangle([5.0, 0.0], [15.0, 5.0]).unwrap();
  let p = find_profiles(&s).remove(0);
  let f = Shape::face(&Plane::XZ, &p).unwrap();
  let r = f.revolve(DVec3::ZERO, DVec3::Z, std::f64::consts::TAU).unwrap();
  let expected = std::f64::consts::PI * (225.0 - 25.0) * 5.0;
  assert!((volume(&r) - expected).abs() < 1e-2, "{}", volume(&r));
}

#[test]
fn mesh_edges_and_exchange() {
  let a = block().fillet(&[1], 2.0).unwrap();
  let m = a.tessellate(0.05).unwrap();
  assert!(!m.indices.is_empty());
  assert_eq!(m.tri_face.len(), m.indices.len() / 3);
  assert_eq!(m.positions.len(), m.normals.len());
  let lines = a.edge_lines(0.05).unwrap();
  assert!(lines.len() >= 12);
  let dir = std::env::temp_dir().join(format!("fox-kernel-test-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let step = dir.join("part.step");
  write_step([&a], &step).unwrap();
  let back = read_step(&step).unwrap();
  assert_eq!(back.len(), 1);
  assert!((volume(&back[0]) - volume(&a)).abs() < 1e-3);
  write_stl(std::slice::from_ref(&m), &dir.join("part.stl")).unwrap();
  let brep = a.to_brep().unwrap();
  let again = Shape::from_brep(&brep).unwrap();
  assert!((volume(&again) - volume(&a)).abs() < 1e-6);
  assert!(read_step(&dir.join("missing.step")).is_err());
  assert!(Shape::from_brep("garbage").is_err());
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn transforms() {
  let a = block();
  let t = a.translated(DVec3::new(100.0, 0.0, 0.0)).unwrap();
  assert!((t.mass().unwrap().bbox_min.x - 100.0).abs() < 1e-6);
  let r = a.rotated(DVec3::ZERO, DVec3::Z, std::f64::consts::FRAC_PI_2).unwrap();
  assert!((r.mass().unwrap().bbox_min.x + 20.0).abs() < 1e-6);
  let m = a.mirrored(DVec3::ZERO, DVec3::X).unwrap();
  assert!((m.mass().unwrap().bbox_min.x + 40.0).abs() < 1e-6);
  assert!((volume(&m) - 8000.0).abs() < 1e-6);
}

#[test]
fn hostile_inputs_are_errors() {
  let f = Shape::face(&Plane::XY, &rect_profile(10.0, 10.0)).unwrap();
  assert!(f.extrude(DVec3::ZERO).is_err());
  assert!(f.extrude(DVec3::new(f64::NAN, 0.0, 0.0)).is_err());
  assert!(f.revolve(DVec3::ZERO, DVec3::ZERO, 1.0).is_err());
  assert!(Shape::make_box(DVec3::ZERO, DVec3::ZERO).is_err());
  assert!(Shape::sphere(DVec3::ZERO, -1.0).is_err());
  assert!(block().fillet(&[999], 1.0).is_err());
  assert!(block().shell(&[1], 100.0, false).is_err());
}

#[test]
fn mesh_normals_agree_with_triangles() {
  let a = block();
  let edges: Vec<u32> = a.edges().unwrap().iter().filter(|e| (e.mid.z - 10.0).abs() < 1e-6).map(|e| e.id).collect();
  let f = a.fillet(&edges, 3.0).unwrap();
  let m = f.tessellate(0.05).unwrap();
  let mut bad = 0;
  for t in m.indices.chunks_exact(3) {
    let p: Vec<glam::Vec3> = t.iter().map(|&i| glam::Vec3::from(m.positions[i as usize])).collect();
    let tn = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
    for &i in t {
      let n = glam::Vec3::from(m.normals[i as usize]);
      if (n.length() - 1.0).abs() > 1e-3 || n.dot(tn) < 0.0 {
        bad += 1;
      }
    }
  }
  assert_eq!(bad, 0, "{bad} bad normals of {}", m.indices.len());
}

#[test]
fn hidden_line_projection() {
  // a 40 × 20 × 10 block with a vertical hole, seen from the front and from the top
  let b = Shape::make_box(DVec3::ZERO, DVec3::new(40.0, 20.0, 10.0)).unwrap();
  let c = Shape::cylinder(DVec3::new(20.0, 10.0, -1.0), DVec3::Z, 4.0, 12.0).unwrap();
  let part = b.boolean(&c, BoolOp::Subtract).unwrap();
  let front = part.project(-DVec3::Y, DVec3::X, 0.01).unwrap();
  let (lo, hi) = front.bounds().unwrap();
  assert!((hi[0] - lo[0] - 40.0).abs() < 1e-6 && (hi[1] - lo[1] - 10.0).abs() < 1e-6, "{lo:?} {hi:?}");
  assert!((lo[1] - 0.0).abs() < 1e-6, "front view y is world Z");
  assert!(front.lines.iter().any(|l| l.kind.hidden()), "the hole is hidden from the front");
  let top = part.project(DVec3::Z, DVec3::X, 0.01).unwrap();
  let (lo, hi) = top.bounds().unwrap();
  assert!((hi[1] - lo[1] - 20.0).abs() < 1e-6 && (lo[1] - 0.0).abs() < 1e-6, "top view y is world Y: {lo:?}");
  assert!(top.circles.iter().any(|c| (c.radius - 4.0).abs() < 1e-9 && !c.kind.hidden()));
}

#[test]
fn iges_and_brep_files() {
  let a = block().fillet(&[1], 2.0).unwrap();
  let dir = std::env::temp_dir().join(format!("fox-kernel-iges-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let iges = dir.join("part.igs");
  write_iges([&a], &iges).unwrap();
  let back = read_iges(&iges).unwrap();
  assert_eq!(back.len(), 1);
  assert_eq!(back[0].solid_count(), 1, "IGES faces are sewn back into a solid");
  assert!((volume(&back[0]) - volume(&a)).abs() < 0.5, "{} vs {}", volume(&back[0]), volume(&a));
  let brep = dir.join("part.brep");
  write_brep_file([&a, &block().translated(DVec3::X * 100.0).unwrap()], &brep).unwrap();
  assert_eq!(read_brep_file(&brep).unwrap().len(), 2);
  assert!(read_iges(&dir.join("missing.igs")).is_err());
  let _ = std::fs::remove_dir_all(dir);
}

/// The block's triangulation, welded so triangles share vertices.
fn welded_mesh(s: &Shape) -> (Vec<[f32; 3]>, Vec<u32>) {
  let m = s.tessellate(0.05).unwrap();
  let mut map = std::collections::HashMap::new();
  let mut pos = Vec::new();
  let remap: Vec<u32> = m
    .positions
    .iter()
    .map(|p| {
      *map.entry([p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]).or_insert_with(|| {
        pos.push(*p);
        pos.len() as u32 - 1
      })
    })
    .collect();
  (pos, m.indices.iter().map(|&i| remap[i as usize]).collect())
}

#[test]
fn closed_mesh_becomes_a_solid() {
  let b = block();
  let (pos, idx) = welded_mesh(&b);
  let s = Shape::from_mesh(&pos, &idx, false).unwrap();
  assert_eq!(s.solid_count(), 1);
  assert!((volume(&s) - 8000.0).abs() < 1e-2, "{}", volume(&s));
  assert_eq!(s.faces().unwrap().len(), idx.len() / 3);
  // merged: back to six planar faces
  let merged = Shape::from_mesh(&pos, &idx, true).unwrap();
  assert_eq!(merged.faces().unwrap().len(), 6);
  assert!((volume(&merged) - 8000.0).abs() < 1e-2);
  // a faceted body takes part in booleans like any other
  let hole = Shape::cylinder(DVec3::new(20.0, 10.0, -1.0), DVec3::Z, 3.0, 12.0).unwrap();
  let cut = merged.boolean(&hole, BoolOp::Subtract).unwrap();
  assert!(volume(&cut) < 8000.0 - 250.0);
  // inside-out triangles still give a positive volume
  let flipped: Vec<u32> = idx.chunks_exact(3).flat_map(|t| [t[0], t[2], t[1]]).collect();
  assert!(volume(&Shape::from_mesh(&pos, &flipped, false).unwrap()) > 7999.0);
}

#[test]
fn open_mesh_is_a_shell() {
  let (pos, idx) = welded_mesh(&block());
  // drop two triangles: a hole in the surface
  let s = Shape::from_mesh(&pos, &idx[6..], false).unwrap();
  assert_eq!(s.solid_count(), 0);
  assert_eq!(s.bodies().unwrap().len(), 1);
  assert!(Shape::from_mesh(&pos, &[0, 1], false).is_err());
  assert!(Shape::from_mesh(&pos, &[0, 1, 999_999], false).is_err());
}

#[test]
fn tapered_extrude_keeps_sharp_corners_both_ways() {
  let face = Shape::face(&Plane::XY, &rect_profile(20.0, 20.0)).unwrap();
  let (h, a) = (10.0, 10f64.to_radians());
  for (angle, top) in [(a, 20.0 - 2.0 * h * a.tan()), (-a, 20.0 + 2.0 * h * a.tan())] {
    let s = face.extrude_tapered(h, angle).unwrap();
    // a square frustum: V = h/3 (A1 + A2 + sqrt(A1 A2))
    let (a1, a2) = (400.0, top * top);
    let want = h / 3.0 * (a1 + a2 + (a1 * a2).sqrt());
    assert!((volume(&s) - want).abs() < 1e-6 * want, "taper {angle}: {} vs {want}", volume(&s));
    assert_eq!(s.faces().unwrap().len(), 6, "sharp corners: no rounded faces");
    let m = s.mass().unwrap();
    assert!((m.bbox_max.z - h).abs() < 1e-9);
    // and downwards, mirrored
    let s = face.extrude_tapered(-h, angle).unwrap();
    assert!((volume(&s) - want).abs() < 1e-6 * want, "taper {angle} down: {} vs {want}", volume(&s));
    assert!((s.mass().unwrap().bbox_min.z + h).abs() < 1e-9);
  }
}

#[test]
fn step_assembly_round_trip_with_names_and_colours() {
  let dir = std::env::temp_dir().join(format!("fox-kernel-xcaf-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("asm.step");
  let base = Shape::make_box(DVec3::ZERO, DVec3::new(20.0, 20.0, 5.0)).unwrap();
  let hub = Shape::cylinder(DVec3::new(40.0, 0.0, 0.0), DVec3::Z, 5.0, 10.0).unwrap();
  let pin = Shape::make_box(DVec3::new(0.0, 40.0, 0.0), DVec3::new(4.0, 44.0, 12.0)).unwrap();
  let parts = [
    StepPart { name: "Base", component: "", color: None, shape: &base },
    StepPart { name: "Hub ü", component: "Wheel", color: Some([200, 30, 40]), shape: &hub },
    StepPart { name: "Pin", component: "Wheel", color: Some([32, 80, 192]), shape: &pin },
  ];
  write_step_assembly("Cart", &parts, &path).unwrap();
  let back = read_step_assembly(&path).unwrap();
  assert_eq!(back.len(), 3);
  let find = |n: &str| back.iter().find(|b| b.name == n).unwrap_or_else(|| panic!("{n} in {back:?}"));
  assert_eq!(find("Base").component, "");
  assert_eq!(find("Base").color, None);
  assert_eq!(find("Hub ü").component, "Wheel", "non-ASCII names survive");
  assert_eq!(find("Hub ü").color, Some([200, 30, 40]));
  assert_eq!(find("Pin").color, Some([32, 80, 192]));
  // placed where they were
  let m = find("Pin").shape.mass().unwrap();
  assert!((m.volume - 4.0 * 4.0 * 12.0).abs() < 1e-6 && (m.center_of_mass.y - 42.0).abs() < 1e-6, "{m:?}");
  // the plain reader still sees the solids
  assert_eq!(read_step(&path).unwrap().len(), 3);
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn along_curve_is_on_the_exact_arc() {
  // a quarter arc of radius 20 from (20, 0) to (0, 20)
  let arc = Shape::path(
    &Plane::XY,
    &[Seg::Arc { center: [0.0, 0.0], radius: 20.0, start_angle: 0.0, sweep: std::f64::consts::FRAC_PI_2 }],
  )
  .unwrap();
  let a = 22.5_f64.to_radians();
  let (p, d) = arc.along_curve(0.25, None).unwrap();
  assert!((p - DVec3::new(20.0 * a.cos(), 20.0 * a.sin(), 0.0)).length() < 1e-6, "{p}");
  assert!((d - DVec3::new(-a.sin(), a.cos(), 0.0)).length() < 1e-6, "{d}");
  // measured from the far end, the tangent points back along the arc
  let (p, d) = arc.along_curve(0.25, Some(DVec3::new(0.0, 20.0, 0.0))).unwrap();
  let b = 67.5_f64.to_radians();
  assert!((p - DVec3::new(20.0 * b.cos(), 20.0 * b.sin(), 0.0)).length() < 1e-6, "{p}");
  assert!((d - DVec3::new(b.sin(), -b.cos(), 0.0)).length() < 1e-6, "{d}");
  // a body edge works too
  let line = block().edge_shape(1).unwrap();
  let (p, d) = line.along_curve(1.0, None).unwrap();
  assert!(p.is_finite() && (d.length() - 1.0).abs() < 1e-9);
}

#[test]
fn a_quintic_spline_is_one_exact_edge() {
  // a bulge from (20, 0) round to (20, 10), closed by three lines
  let p: [[f64; 2]; 10] = [
    [20.0, 0.0],
    [21.6, 0.0],
    [24.4, 0.4],
    [27.6, 2.2],
    [29.8, 3.3],
    [29.8, 6.7],
    [27.6, 7.8],
    [24.4, 9.6],
    [21.6, 10.0],
    [20.0, 10.0],
  ];
  let segs = vec![
    Seg::Quintic { p },
    Seg::Line { a: [20.0, 10.0], b: [0.0, 10.0] },
    Seg::Line { a: [0.0, 10.0], b: [0.0, 0.0] },
    Seg::Line { a: [0.0, 0.0], b: [20.0, 0.0] },
  ];
  let profile = Profile { outer: fox_sketch::Loop::new(segs.clone()), holes: Vec::new() };
  let solid = Shape::face(&Plane::XY, &profile).unwrap().extrude(DVec3::Z).unwrap();
  assert!(solid.is_valid());
  assert_eq!(solid.faces().unwrap().len(), 6, "the spline makes one side face");
  // the area inside the spline's chord, by the shoelace formula on fine samples
  let n = 20000;
  let pts: Vec<[f64; 2]> = (0..=n).map(|i| fox_sketch::quintic_at(&p, i as f64 / n as f64)).collect();
  let bulge =
    pts.windows(2).map(|w| w[0][0] * w[1][1] - w[1][0] * w[0][1]).sum::<f64>() / 2.0 + (20.0 * 0.0 - 20.0 * 10.0) / 2.0;
  // (OCCT's default volume integration is good to about 1e-5 on a spline side)
  assert!((volume(&solid) / (200.0 + bulge) - 1.0).abs() < 1e-4, "{} vs {}", volume(&solid), 200.0 + bulge);
  // and one edge of a path
  let path = Shape::path(&Plane::XY, &segs[..2]).unwrap();
  assert_eq!(path.edges().unwrap().len(), 2);
}

/// Merge flags count segments, not the kernel's records: after a quintic spline (three records)
/// they still land on the segments they name, in a face's loop and in a path.
#[test]
fn merge_flags_after_a_quintic_spline_name_segments() {
  use fox_sketch::{CurveId, SegSrc};
  let p: [[f64; 2]; 10] = [
    [20.0, 0.0],
    [21.6, 0.0],
    [24.4, 0.4],
    [27.6, 2.2],
    [29.8, 3.3],
    [29.8, 6.7],
    [27.6, 7.8],
    [24.4, 9.6],
    [21.6, 10.0],
    [20.0, 10.0],
  ];
  // the top line cut in two pieces of one curve, the only pieces flagged to merge
  let segs = vec![
    Seg::Quintic { p },
    Seg::Line { a: [20.0, 10.0], b: [10.0, 10.0] },
    Seg::Line { a: [10.0, 10.0], b: [0.0, 10.0] },
    Seg::Line { a: [0.0, 10.0], b: [0.0, 0.0] },
    Seg::Line { a: [0.0, 0.0], b: [20.0, 0.0] },
  ];
  let curves = [0, 1, 1, 2, 3];
  let merge = [false, true, false, false, false];
  let src = curves.iter().zip(merge).map(|(&c, m)| SegSrc { curve: CurveId(c), merge_next: m }).collect();
  let faces = |lp: fox_sketch::Loop| {
    let solid = Shape::face(&Plane::XY, &Profile { outer: lp, holes: Vec::new() }).unwrap().extrude(DVec3::Z).unwrap();
    assert!(solid.is_valid());
    (solid.faces().unwrap().len(), volume(&solid))
  };
  let (legacy, merged) =
    (faces(fox_sketch::Loop::new(segs.clone())), faces(fox_sketch::Loop { segs: segs.clone(), src }));
  assert_eq!(legacy.0, 7, "five sides, top and bottom");
  assert_eq!(merged.0, 6, "the top line's pieces are one side");
  assert!((legacy.1 - merged.1).abs() < 1e-6 * legacy.1, "{} vs {}", legacy.1, merged.1);
  let open = &segs[..3];
  assert_eq!(Shape::path(&Plane::XY, open).unwrap().edges().unwrap().len(), 3);
  assert_eq!(Shape::path_with(&Plane::XY, open, &merge[..3]).unwrap().edges().unwrap().len(), 2);
}
