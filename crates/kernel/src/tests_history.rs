//! Labels followed through kernel calls (history.rs).
use super::*;
use fox_sketch::{Sketch, find_profiles};

fn rect_face(w: f64, h: f64) -> Shape {
  let mut s = Sketch::new();
  s.add_rectangle([0.0, 0.0], [w, h]).unwrap();
  Shape::face(&Plane::XY, &find_profiles(&s).remove(0)).unwrap()
}

/// Labels 1, 2, ... on a shape's faces and 100, 101, ... on its edges.
fn labelled(s: &Shape) -> Shape {
  let nf = s.faces().unwrap().len() as u64;
  let ne = s.edges().unwrap().len() as u64;
  s.labelled(&(1..=nf).map(|i| (i as u32, i)).collect::<Vec<_>>(), &(100..100 + ne).collect::<Vec<_>>()).unwrap()
}

fn modified(l: &FaceLabels, face: u32) -> Vec<u64> {
  l.faces[face as usize - 1].iter().filter(|(_, g)| !g).map(|(l, _)| *l).collect()
}

fn generated(l: &FaceLabels, face: u32) -> Vec<u64> {
  l.faces[face as usize - 1].iter().filter(|(_, g)| *g).map(|(l, _)| *l).collect()
}

#[test]
fn extrusion_faces_come_from_the_profile() {
  let f = labelled(&rect_face(40.0, 20.0));
  let b = f.extrude(DVec3::new(0.0, 0.0, 10.0)).unwrap();
  let l = b.face_labels().unwrap();
  let faces = b.faces().unwrap();
  for fi in &faces {
    let (m, g) = (modified(&l, fi.id), generated(&l, fi.id));
    if fi.normal.z < -0.99 {
      assert_eq!((m, g), (vec![1], vec![]), "start cap");
    } else if fi.normal.z > 0.99 {
      assert_eq!((m, g), (vec![], vec![1]), "end cap");
    } else {
      assert!(m.is_empty() && g.len() == 1 && g[0] >= 100, "side {fi:?}: {g:?}");
    }
  }
  // each side from its own profile edge
  let mut sides: Vec<u64> = faces.iter().flat_map(|fi| generated(&l, fi.id)).filter(|g| *g >= 100).collect();
  sides.sort();
  sides.dedup();
  assert_eq!(sides.len(), 4);
}

#[test]
fn fillets_and_cuts_keep_labels() {
  let b = labelled(&rect_face(40.0, 20.0).extrude(DVec3::new(0.0, 0.0, 10.0)).unwrap());
  let before = b.faces().unwrap();
  let top = before.iter().find(|f| f.normal.z > 0.99).unwrap().id as u64;
  let e = b.edges().unwrap().into_iter().find(|e| (e.mid - DVec3::new(20.0, 0.0, 10.0)).length() < 1e-6).unwrap();
  let r = b.fillet(&[e.id], 2.0).unwrap();
  let l = r.face_labels().unwrap();
  let after = r.faces().unwrap();
  assert_eq!(after.len(), 7);
  let round = after.iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap();
  assert_eq!(generated(&l, round.id), vec![99 + e.id as u64]);
  let t = after.iter().find(|f| f.normal.z > 0.99).unwrap();
  assert_eq!(modified(&l, t.id), vec![top]);
  // a slot across the top splits it: both pieces stay the top
  let slot = Shape::make_box(DVec3::new(15.0, -5.0, 5.0), DVec3::new(25.0, 25.0, 20.0)).unwrap();
  let c = r.boolean(&slot, BoolOp::Subtract).unwrap();
  let l = c.face_labels().unwrap();
  let tops: Vec<_> = c.faces().unwrap().into_iter().filter(|f| f.normal.z > 0.99 && f.origin.z > 9.99).collect();
  assert_eq!(tops.len(), 2);
  for t in tops {
    assert_eq!(modified(&l, t.id), vec![top]);
  }
}

#[test]
fn unlabelled_calls_pair_faces_by_identity() {
  let b = rect_face(40.0, 20.0).extrude(DVec3::new(0.0, 0.0, 10.0)).unwrap();
  assert!(!b.has_labels());
  let moved = b.translated(DVec3::new(0.0, 0.0, 0.0)).unwrap();
  let same = b.solids().unwrap()[0].same_faces(&b).unwrap();
  assert_eq!(same, (1..=6).collect::<Vec<u32>>());
  assert!(moved.face_labels().unwrap().faces.iter().all(|f| f.is_empty()));
}
