//! Form cage tests: primitives, edits, symmetry, and the smooth body's patches (watertight,
//! tangent-continuous across seams).

use glam::{DMat4, DVec3};

use crate::edit::{self, Fill};
use crate::subd::{limit_mesh, patches, seam_report};
use crate::{Cage, Selection, SymPlane, Symmetry, prim};

/// The volume the cage's faces enclose (fanned into triangles).
fn cage_volume(c: &Cage) -> f64 {
  let mut v = 0.0;
  for f in &c.faces {
    let a = c.pos(f[0]);
    for k in 1..f.len() - 1 {
      v += a.dot(c.pos(f[k]).cross(c.pos(f[k + 1]))) / 6.0;
    }
  }
  v
}

/// The smooth body's volume, from a fine limit mesh.
fn smooth_volume(c: &Cage) -> f64 {
  smooth_volume_at(c, 4)
}

fn smooth_volume_at(c: &Cage, levels: u32) -> f64 {
  let (pos, quads) = limit_mesh(c, levels).unwrap();
  quads.iter().map(|q| {
    let [a, b, cc, d] = q.map(|i| pos[i as usize]);
    (a.dot(b.cross(cc)) + a.dot(cc.cross(d))) / 6.0
  }).sum()
}

/// Checks the patches meet without gaps and (away from creases) without kinks.
fn assert_smooth_and_watertight(c: &Cage, creased: impl Fn(DVec3) -> bool, max_angle: f64) -> usize {
  let p = patches(c).unwrap();
  let (gap, angle, lonely) = seam_report(&p, creased, 8);
  assert!(gap < 1e-6, "gap {gap}");
  if c.is_closed() {
    assert_eq!(lonely, 0, "side points with no neighbour");
  }
  assert!(angle < max_angle, "kink of {angle} degrees across a seam");
  p.len()
}

#[test]
fn primitives_are_valid_cages() {
  let closed = [
    prim::quad_box(80.0, 60.0, 20.0, [2, 2, 2]).unwrap(),
    prim::cylinder(20.0, 30.0, 8, 2, true).unwrap(),
    prim::sphere(40.0, 8, 4).unwrap(),
    prim::torus(60.0, 16.0, 8, 4).unwrap(),
    prim::quadball(40.0, 2).unwrap(),
  ];
  for c in &closed {
    c.check().unwrap();
    assert!(c.is_closed());
    assert!(cage_volume(c) > 0.0, "inside out");
    assert!(smooth_volume(c) > 0.0);
  }
  let open = [prim::plane(40.0, 30.0, [3, 2]).unwrap(), prim::cylinder(20.0, 30.0, 8, 2, false).unwrap()];
  for c in &open {
    c.check().unwrap();
    assert!(!c.is_closed());
  }
  let f = prim::face(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 2.0], [0.0, 10.0, 0.0], [-3.0, 5.0, 0.0]]).unwrap();
  assert_eq!(f.faces[0].len(), 5);
  assert!(patches(&f).unwrap().len() == 5);
}

#[test]
fn round_primitives_have_the_size_asked_for() {
  let s = prim::sphere(40.0, 8, 4).unwrap();
  let (pos, _) = limit_mesh(&s, 4).unwrap();
  let mean = pos.iter().map(|p| p.length()).sum::<f64>() / pos.len() as f64;
  assert!((mean - 20.0).abs() < 0.05, "{mean}");
  let q = prim::quadball(30.0, 3).unwrap();
  let (pos, _) = limit_mesh(&q, 4).unwrap();
  let (lo, hi) = pos.iter().fold((f64::MAX, 0.0f64), |(lo, hi), p| (lo.min(p.length()), hi.max(p.length())));
  assert!(lo > 14.0 && hi < 16.0, "{lo} {hi}");
  // a sphere of 4/3 pi r^3
  let v = smooth_volume(&s);
  assert!((v / (4.0 / 3.0 * std::f64::consts::PI * 8000.0) - 1.0).abs() < 0.05, "{v}");
}

#[test]
fn box_body_is_six_watertight_tangent_patches() {
  let c = prim::quad_box(80.0, 60.0, 20.0, [2, 2, 2]).unwrap();
  let n = assert_smooth_and_watertight(&c, |_| false, 2.5);
  assert_eq!(n, 6);
  let c = prim::quad_box(80.0, 60.0, 20.0, [4, 3, 2]).unwrap();
  assert_eq!(assert_smooth_and_watertight(&c, |_| false, 2.5), 6);
}

#[test]
fn round_bodies_are_watertight() {
  for c in [
    prim::sphere(40.0, 8, 4).unwrap(),
    prim::torus(60.0, 16.0, 8, 4).unwrap(),
    prim::quadball(40.0, 2).unwrap(),
    prim::cylinder(20.0, 30.0, 6, 2, true).unwrap(),
  ] {
    let n = assert_smooth_and_watertight(&c, |_| false, 3.0);
    assert!(n >= 2 && n < c.faces.len() * 4, "{n}");
  }
  // a torus is regular everywhere: a few big patches, not one per face
  let t = prim::torus(60.0, 16.0, 8, 4).unwrap();
  assert!(patches(&t).unwrap().len() <= 8);
}

#[test]
fn open_plane_keeps_its_corners() {
  let c = prim::plane(40.0, 30.0, [3, 2]).unwrap();
  let p = patches(&c).unwrap();
  assert_eq!(p.len(), 1);
  assert!(p[0].eval(0.0, 0.0).distance(DVec3::new(-20.0, -15.0, 0.0)) < 1e-9);
}

#[test]
fn creases_stay_sharp() {
  let mut c = prim::quad_box(20.0, 20.0, 20.0, [1, 1, 1]).unwrap();
  let soft = smooth_volume(&c);
  let n = edit::crease(&mut c, &Selection { faces: (0..6).collect(), ..Default::default() }, true).unwrap();
  assert_eq!(n, 12);
  // every edge creased: the body is the cage itself
  assert!((smooth_volume(&c) - 8000.0).abs() < 1e-6 && soft < 8000.0 * 0.6);
  // crease only the top's edges: corners at the top stay put
  let mut c = prim::quad_box(20.0, 20.0, 20.0, [2, 2, 2]).unwrap();
  let top: Vec<u32> = (0..c.faces.len() as u32).filter(|&f| c.faces[f as usize].iter().all(|&v| c.pos(v).z == 20.0)).collect();
  let rim: Vec<[u32; 2]> = c
    .edges()
    .into_iter()
    .filter(|e| {
      let (a, b) = (c.pos(e[0]), c.pos(e[1]));
      a.z == 20.0 && b.z == 20.0 && (a.x.abs() == 10.0 && b.x.abs() == 10.0 || a.y.abs() == 10.0 && b.y.abs() == 10.0)
    })
    .collect();
  assert_eq!(top.len(), 4);
  assert_eq!(rim.len(), 8);
  edit::crease(&mut c, &Selection { edges: rim, ..Default::default() }, true).unwrap();
  // the rim's limit is the B-spline curve of its vertices: through the middle of each side
  let (pos, _) = limit_mesh(&c, 3).unwrap();
  assert!(pos.iter().any(|p| p.distance(DVec3::new(10.0, 0.0, 20.0)) < 1e-9));
  let sharp = |p: DVec3| (p.z - 20.0).abs() < 1e-6;
  let n = assert_smooth_and_watertight(&c, sharp, 3.0);
  assert!(n >= 6);
  // and uncrease
  edit::crease(&mut c, &Selection { faces: top, ..Default::default() }, false).unwrap();
  assert!(c.creases.is_empty());
}

#[test]
fn edit_form_moves_with_soft_falloff_and_symmetry() {
  let mut c = prim::quad_box(40.0, 40.0, 20.0, [4, 4, 2]).unwrap();
  let top = c.nearest_vertex([0.0, 0.0, 20.0]).unwrap() as u32;
  let near = c.nearest_vertex([10.0, 0.0, 20.0]).unwrap() as u32;
  let far = c.nearest_vertex([20.0, 20.0, 20.0]).unwrap() as u32;
  let up = DMat4::from_translation(DVec3::new(0.0, 0.0, 10.0));
  let moved = edit::transform(&mut c, &Selection { vertices: vec![top], ..Default::default() }, up, 15.0).unwrap();
  assert!(moved > 1);
  assert_eq!(c.pos(top).z, 30.0);
  let dz = c.pos(near).z - 20.0;
  assert!(dz > 0.0 && dz < 10.0, "{dz}");
  assert_eq!(c.pos(far).z, 20.0);

  // mirror symmetry in x = 0: moving a vertex on +x moves its partner on -x
  let mut c = prim::quad_box(40.0, 40.0, 20.0, [4, 4, 2]).unwrap();
  edit::set_symmetry(&mut c, Symmetry::Mirror { planes: vec![SymPlane { origin: [0.0; 3], normal: [1.0, 0.0, 0.0] }] }).unwrap();
  let a = c.nearest_vertex([10.0, 10.0, 20.0]).unwrap() as u32;
  let b = c.nearest_vertex([-10.0, 10.0, 20.0]).unwrap() as u32;
  let m = edit::compose(DVec3::new(3.0, 0.0, 5.0), DVec3::Z, 0.0, DVec3::ONE, DVec3::ZERO);
  edit::transform(&mut c, &Selection { vertices: vec![a], ..Default::default() }, m, 0.0).unwrap();
  assert_eq!(c.pos(a), DVec3::new(13.0, 10.0, 25.0));
  assert_eq!(c.pos(b), DVec3::new(-13.0, 10.0, 25.0));
  // a vertex on the plane stays on it
  let mid = c.nearest_vertex([0.0, 10.0, 20.0]).unwrap() as u32;
  edit::transform(&mut c, &Selection { vertices: vec![mid], ..Default::default() }, m, 0.0).unwrap();
  assert!(c.pos(mid).x.abs() < 1e-12 && (c.pos(mid).z - 25.0).abs() < 1e-12);
  // a symmetry the cage doesn't have is refused
  assert!(edit::set_symmetry(&mut c, Symmetry::Mirror { planes: vec![SymPlane { origin: [5.0, 0.0, 0.0], normal: [1.0, 0.0, 0.0] }] }).is_err());

  // rotate and scale a face about its centre
  let mut c = prim::quad_box(40.0, 40.0, 20.0, [1, 1, 1]).unwrap();
  let f = c.nearest_face([0.0, 0.0, 20.0]).unwrap() as u32;
  let sel = Selection { faces: vec![f], ..Default::default() };
  let pivot = edit::pivot(&c, &sel);
  let m = edit::compose(DVec3::ZERO, DVec3::Z, 45.0, DVec3::splat(0.5), pivot);
  edit::transform(&mut c, &sel, m, 0.0).unwrap();
  let r = c.faces[f as usize].iter().map(|&v| (c.pos(v) - pivot).length()).fold(0.0, f64::max);
  assert!((r - 20.0 * 2f64.sqrt() * 0.5).abs() < 1e-9, "{r}");
  c.check().unwrap();
}

#[test]
fn insert_and_delete_edge_loops() {
  let mut c = prim::quad_box(40.0, 40.0, 20.0, [2, 2, 1]).unwrap();
  let (faces, verts) = (c.faces.len(), c.vertices.len());
  let shape = smooth_volume(&c);
  let e = c.nearest_edge([20.0, 0.0, 10.0]).unwrap();
  let new = edit::insert_edge(&mut c, &[e], 0.25).unwrap();
  // a loop round the box: 4 ring edges on the sides... and across top and bottom
  assert!(new.len() >= 4);
  assert_eq!(c.vertices.len(), verts + new.len());
  assert!(c.faces.len() > faces);
  assert!(c.is_closed());
  let new_z: Vec<f64> = new.iter().map(|&v| c.pos(v).z).collect();
  assert!(new_z.iter().any(|z| (z - 5.0).abs() < 1e-9 || (z - 15.0).abs() < 1e-9), "{new_z:?}");
  // deleting the loop's edges gives the box back
  let loop_edges: Vec<[u32; 2]> = c.edges().into_iter().filter(|e| new.contains(&e[0]) && new.contains(&e[1])).collect();
  assert_eq!(loop_edges.len(), new.len());
  edit::delete(&mut c, &Selection { edges: loop_edges, ..Default::default() }).unwrap();
  assert_eq!((c.faces.len(), c.vertices.len()), (faces, verts));
  assert!((smooth_volume(&c) - shape).abs() < 1e-6);

  // insert point: split two edges of one face and join them
  let mut c = prim::plane(20.0, 20.0, [1, 1]).unwrap();
  let pts = edit::insert_point(&mut c, &[([0, 1], 0.5), ([2, 3], 0.5)]).unwrap();
  assert_eq!(pts.len(), 2);
  assert_eq!(c.faces.len(), 2);
  assert!(c.edges().contains(&[pts[0].min(pts[1]), pts[0].max(pts[1])]));
}

#[test]
fn delete_faces_and_fill_holes() {
  let mut c = prim::quad_box(30.0, 30.0, 30.0, [2, 2, 2]).unwrap();
  let top: Vec<u32> = (0..c.faces.len() as u32).filter(|&f| c.faces[f as usize].iter().all(|&v| c.pos(v).z == 30.0)).collect();
  edit::delete(&mut c, &Selection { faces: top, ..Default::default() }).unwrap();
  assert!(!c.is_closed());
  let b = c.boundary_edges();
  assert_eq!(b.len(), 8);
  let mut star = c.clone();
  edit::fill_hole(&mut star, b[0], Fill::Star).unwrap();
  assert!(star.is_closed());
  assert_eq!(star.faces.len(), 24 - 4 + 4);
  edit::fill_hole(&mut c, b[0], Fill::Polygon).unwrap();
  assert!(c.is_closed());
  assert!(assert_smooth_and_watertight(&c, |_| false, 3.0) > 1);
  // deleting a vertex joins its faces
  let mut c = prim::quad_box(30.0, 30.0, 30.0, [2, 2, 2]).unwrap();
  let v = c.nearest_vertex([0.0, 0.0, 30.0]).unwrap() as u32;
  edit::delete(&mut c, &Selection { vertices: vec![v], ..Default::default() }).unwrap();
  assert!(c.is_closed());
  assert_eq!(c.faces.len(), 24 - 3);
  assert!(c.faces.iter().any(|f| f.len() == 8 && f.iter().all(|&v| c.pos(v).z == 30.0)));
}

#[test]
fn subdivide_locally_and_exactly() {
  let mut c = prim::quad_box(30.0, 30.0, 30.0, [1, 1, 1]).unwrap();
  let shape = smooth_volume(&c);
  let mut exact = c.clone();
  edit::subdivide(&mut exact, &[], true).unwrap();
  assert_eq!(exact.faces.len(), 24);
  assert!((smooth_volume_at(&exact, 3) - shape).abs() < 1e-9 * shape);
  let f = c.nearest_face([0.0, 0.0, 30.0]).unwrap() as u32;
  edit::subdivide(&mut c, &[f], false).unwrap();
  assert_eq!(c.faces.len(), 6 - 1 + 4);
  assert!(c.is_closed());
  // the four side faces now have a T-point each on their top edge
  assert_eq!(c.faces.iter().filter(|f| f.len() == 5).count(), 4);
  assert_smooth_and_watertight(&c, |_| false, 4.0);
}

#[test]
fn extrude_faces_and_edges() {
  let mut c = prim::quad_box(30.0, 30.0, 10.0, [3, 3, 1]).unwrap();
  let v0 = cage_volume(&c);
  let f = c.nearest_face([0.0, 0.0, 10.0]).unwrap() as u32;
  let new = edit::extrude_faces(&mut c, &[f], 10.0).unwrap();
  assert_eq!(new.len(), 4);
  assert!(new.iter().all(|&v| (c.pos(v).z - 20.0).abs() < 1e-9));
  assert!(c.is_closed());
  assert!((cage_volume(&c) - v0 - 1000.0).abs() < 1e-6);
  assert!(assert_smooth_and_watertight(&c, |_| false, 4.0) > 6);

  let mut p = prim::plane(20.0, 10.0, [2, 1]).unwrap();
  let b = p.boundary_edges();
  let side: Vec<[u32; 2]> = b.iter().copied().filter(|e| p.pos(e[0]).x == 10.0 && p.pos(e[1]).x == 10.0).collect();
  assert_eq!(side.len(), 1);
  let new = edit::extrude_edges(&mut p, &side, 5.0, None).unwrap();
  assert_eq!(new.len(), 2);
  assert!(new.iter().all(|&v| (p.pos(v).x - 15.0).abs() < 1e-9));
  let up = edit::extrude_edges(&mut p, &[[new[0], new[1]]], 0.0, Some(DVec3::new(0.0, 0.0, 5.0))).unwrap();
  assert!(up.iter().all(|&v| (p.pos(v).z - 5.0).abs() < 1e-9));
  p.check().unwrap();
}

#[test]
fn bridge_two_tubes() {
  // two open tubes end to end with a gap, bridged into one
  let mut a = prim::cylinder(20.0, 10.0, 8, 1, false).unwrap();
  let b = prim::cylinder(20.0, 10.0, 8, 1, false).unwrap();
  let off = a.vertices.len() as u32;
  a.vertices.extend(b.vertices.iter().map(|v| [v[0], v[1], v[2] + 30.0]));
  a.faces.extend(b.faces.iter().map(|f| f.iter().map(|v| v + off).collect::<Vec<_>>()));
  let edges = a.boundary_edges();
  let lo: Vec<[u32; 2]> = edges.iter().copied().filter(|e| (a.pos(e[0]).z - 10.0).abs() < 1e-9).collect();
  let hi: Vec<[u32; 2]> = edges.iter().copied().filter(|e| (a.pos(e[0]).z - 30.0).abs() < 1e-9).collect();
  assert_eq!((lo.len(), hi.len()), (8, 8));
  edit::bridge(&mut a, &lo, &hi, 2).unwrap();
  assert_eq!(a.faces.len(), 16 + 16);
  assert_eq!(a.boundary_edges().len(), 16);
  // fill both ends: a closed capsule
  for z in [0.0, 40.0] {
    let e = a.boundary_edges().into_iter().find(|e| (a.pos(e[0]).z - z).abs() < 1e-9).unwrap();
    edit::fill_hole(&mut a, e, Fill::Star).unwrap();
  }
  assert!(a.is_closed());
  assert!(smooth_volume(&a) > 0.0);
  assert_smooth_and_watertight(&a, |_| false, 4.0);
}

#[test]
fn mirror_and_circular_duplicates() {
  // half a box, open on x = 0, mirrored into a whole one
  let mut c = prim::quad_box(40.0, 20.0, 20.0, [2, 1, 1]).unwrap();
  let left: Vec<u32> = (0..c.faces.len() as u32).filter(|&f| c.centroid(f as usize).x < -1e-9).collect();
  edit::delete(&mut c, &Selection { faces: left, ..Default::default() }).unwrap();
  assert!(!c.is_closed());
  let plane = SymPlane { origin: [0.0; 3], normal: [1.0, 0.0, 0.0] };
  edit::mirror_duplicate(&mut c, plane).unwrap();
  assert!(c.is_closed());
  let whole = prim::quad_box(40.0, 20.0, 20.0, [2, 1, 1]).unwrap();
  assert_eq!(c.faces.len(), whole.faces.len());
  assert!((smooth_volume(&c) - smooth_volume(&whole)).abs() < 1e-6);
  assert!(matches!(c.symmetry, Symmetry::Mirror { .. }));

  // a wedge of a ring, copied round
  let mut ring = prim::torus(60.0, 16.0, 8, 4).unwrap();
  let keep: Vec<u32> = (0..ring.faces.len() as u32)
    .filter(|&f| {
      let p = ring.centroid(f as usize);
      let a = p.y.atan2(p.x).rem_euclid(std::f64::consts::TAU);
      a > std::f64::consts::FRAC_PI_2
    })
    .collect();
  edit::delete(&mut ring, &Selection { faces: keep, ..Default::default() }).unwrap();
  assert_eq!(ring.faces.len(), 8);
  edit::circular_duplicate(&mut ring, DVec3::ZERO, DVec3::Z, 4).unwrap();
  assert!(ring.is_closed());
  assert_eq!(ring.faces.len(), 32);
  // an edit on one copy shows on all four
  let v = ring.nearest_vertex([38.0, 0.0, 0.0]).unwrap() as u32;
  edit::transform(&mut ring, &Selection { vertices: vec![v], ..Default::default() }, DMat4::from_translation(DVec3::new(0.0, 0.0, 4.0)), 0.0).unwrap();
  let lifted = ring.vertices.iter().filter(|p| (p[2] - ring.pos(v).z).abs() < 1e-9).count();
  assert_eq!(lifted, 4);
}

#[test]
fn pull_onto_a_plane_and_make_uniform() {
  let mut c = prim::quad_box(40.0, 40.0, 20.0, [2, 2, 2]).unwrap();
  let top: Vec<u32> = (0..c.vertices.len() as u32).filter(|&v| c.pos(v).z == 20.0).collect();
  let to_plane = |p: DVec3| Some(DVec3::new(p.x, p.y, 25.0));
  let gap = edit::pull(&mut c, &top, true, to_plane).unwrap();
  assert!(gap < 1e-6, "{gap}");
  let (lim, _) = limit_mesh(&c, 1).unwrap();
  assert!(top.iter().all(|&v| (lim[v as usize].z - 25.0).abs() < 1e-6));
  edit::pull(&mut c, &top, false, |p| Some(DVec3::new(p.x, p.y, 30.0))).unwrap();
  assert!(top.iter().all(|&v| c.pos(v).z == 30.0));

  // a lopsided plane evens out
  let mut p = prim::plane(40.0, 40.0, [4, 4]).unwrap();
  let mid = p.nearest_vertex([0.0, 0.0, 0.0]).unwrap();
  p.vertices[mid] = [8.0, 7.0, 0.0];
  let n = edit::make_uniform(&mut p, 30).unwrap();
  assert_eq!(n, 9);
  assert!(p.pos(mid as u32).length() < 1.0, "{:?}", p.pos(mid as u32));
}

#[test]
fn old_quad_cages_still_load() {
  let c: Cage = serde_json::from_str(r#"{"vertices": [[0,0,0],[1,0,0],[1,1,0],[0,1,0]], "faces": [[0,1,2,3]]}"#).unwrap();
  assert_eq!(c.faces[0], vec![0, 1, 2, 3]);
  assert!(c.creases.is_empty() && c.symmetry.is_none() && c.display.is_smooth());
  let s = serde_json::to_string(&c).unwrap();
  assert!(!s.contains("creases") && !s.contains("symmetry"));
}
