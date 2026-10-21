//! Form edits, as Fusion's Form MODIFY and SYMMETRY panels. Each changes the cage in place and
//! keeps it a valid mesh (or fails and leaves it as it was). Edits follow the cage's symmetry:
//! the picked elements' partners on the other sides are edited too.

use std::collections::{BTreeSet, HashMap, HashSet};

use glam::{DMat4, DQuat, DVec3};

use crate::mesh::{Cage, Topo, key};
use crate::subd::{Level, limit_mesh};
use crate::symmetry::{SymPlane, Symmetry, reflection};
use crate::{MAX_FACES, Selection};

/// Runs an edit on a copy and keeps it only if the result is a valid cage.
fn guarded<T>(cage: &mut Cage, f: impl FnOnce(&mut Cage) -> Result<T, String>) -> Result<T, String> {
  let mut c = cage.clone();
  let out = f(&mut c)?;
  if c.faces.len() > MAX_FACES {
    return Err(format!("that would give the form more than {MAX_FACES} faces"));
  }
  c.check()?;
  *cage = c;
  Ok(out)
}

/// Edit Form: moves, turns or scales the picked elements by `m` (in cage coordinates). With a
/// `soft` radius, vertices within it of the picked ones follow part of the way (Soft
/// Modification), falling off smoothly to nothing at the radius. Returns how many vertices moved.
pub fn transform(cage: &mut Cage, sel: &Selection, m: DMat4, soft: f64) -> Result<usize, String> {
  let picked = sel.vertex_set(cage);
  if picked.is_empty() {
    return Err("nothing picked to move".into());
  }
  if !m.is_finite() {
    return Err("the transform isn't finite".into());
  }
  let mut moves: Vec<(u32, DVec3, f64)> = picked.iter().map(|&v| (v, m.transform_point3(cage.pos(v)), 1.0)).collect();
  if soft > 0.0 {
    let centres: Vec<DVec3> = picked.iter().map(|&v| cage.pos(v)).collect();
    for v in 0..cage.vertices.len() as u32 {
      if picked.contains(&v) {
        continue;
      }
      let p = cage.pos(v);
      let d = centres.iter().map(|c| c.distance(p)).fold(f64::MAX, f64::min);
      if d < soft {
        let w = 0.5 * (1.0 + (std::f64::consts::PI * d / soft).cos());
        moves.push((v, p + w * (m.transform_point3(p) - p), w));
      }
    }
  }
  let moves = cage.symmetry.symmetric_moves(cage, &moves);
  for (v, p) in &moves {
    cage.vertices[*v as usize] = p.to_array();
  }
  Ok(moves.len())
}

/// A transform from a move, a turn (degrees about an axis through a pivot) and a scale (about
/// the pivot), applied in that order: scale, turn, move.
pub fn compose(translate: DVec3, axis: DVec3, degrees: f64, scale: DVec3, pivot: DVec3) -> DMat4 {
  let rot = match axis.try_normalize() {
    Some(a) if degrees != 0.0 => DQuat::from_axis_angle(a, degrees.to_radians()),
    _ => DQuat::IDENTITY,
  };
  DMat4::from_translation(translate + pivot)
    * DMat4::from_quat(rot)
    * DMat4::from_scale(scale)
    * DMat4::from_translation(-pivot)
}

/// The centre of the picked elements' vertices.
pub fn pivot(cage: &Cage, sel: &Selection) -> DVec3 {
  let vs = sel.vertex_set(cage);
  vs.iter().map(|&v| cage.pos(v)).sum::<DVec3>() / vs.len().max(1) as f64
}

/// Splits edge a-b at `t` (0 at a), in every face using it. Returns the new vertex.
fn split_edge(cage: &mut Cage, a: u32, b: u32, t: f64) -> u32 {
  let p = cage.pos(a).lerp(cage.pos(b), t);
  let n = cage.add_vertex(p);
  for f in &mut cage.faces {
    let len = f.len();
    if let Some(k) = (0..len).find(|&k| {
      let (x, y) = (f[k], f[(k + 1) % len]);
      (x, y) == (a, b) || (x, y) == (b, a)
    }) {
      f.insert(k + 1, n);
    }
  }
  if cage.is_creased(a, b) {
    cage.set_crease(a, b, false);
    cage.set_crease(a, n, true);
    cage.set_crease(n, b, true);
  }
  n
}

/// Splits face `f` between two of its non-adjacent corners: `f` keeps the half from `u` to `w`,
/// the other half is added at the end.
fn split_face(cage: &mut Cage, f: usize, u: u32, w: u32) -> Result<(), String> {
  let face = cage.faces[f].clone();
  let n = face.len();
  let (Some(i), Some(j)) = (face.iter().position(|x| *x == u), face.iter().position(|x| *x == w)) else {
    return Err("the points aren't on one face".into());
  };
  if (i + 1) % n == j || (j + 1) % n == i || i == j {
    return Err("the points are already joined by an edge".into());
  }
  let walk = |from: usize, to: usize| {
    let mut out = vec![face[from]];
    let mut k = from;
    while k != to {
      k = (k + 1) % n;
      out.push(face[k]);
    }
    out
  };
  cage.faces[f] = walk(i, j);
  cage.faces.push(walk(j, i));
  Ok(())
}

/// The ring of edges across quads through edge a-b, each oriented with its first vertex on a's
/// side, and the quads it crosses (with their two ring edges).
fn edge_ring(cage: &Cage, topo: &Topo, a: u32, b: u32) -> (Vec<(u32, u32)>, Vec<(usize, (u32, u32), (u32, u32))>) {
  let mut edges = vec![(a, b)];
  let mut crossed = Vec::new();
  let start_faces: Vec<usize> =
    [topo.half.get(&(a, b)), topo.half.get(&(b, a))].into_iter().flatten().map(|h| h.0).collect();
  let mut closed = false;
  for (dir, &f0) in start_faces.iter().enumerate() {
    if closed {
      break;
    }
    let (mut e, mut f) = ((a, b), f0);
    loop {
      let face = &cage.faces[f];
      if face.len() != 4 || crossed.iter().any(|c: &(usize, _, _)| c.0 == f) {
        break;
      }
      let k = (0..4).find(|&k| key(face[k], face[(k + 1) % 4]) == key(e.0, e.1)).unwrap_or(0);
      let (q0, q1, q2, q3) = (face[k], face[(k + 1) % 4], face[(k + 2) % 4], face[(k + 3) % 4]);
      debug_assert!(key(q0, q1) == key(e.0, e.1));
      let next = if q0 == e.0 { (q3, q2) } else { (q2, q3) };
      crossed.push((f, e, next));
      if key(next.0, next.1) == key(a, b) {
        closed = true;
        break;
      }
      if dir == 0 {
        edges.push(next);
      } else {
        edges.insert(0, next);
      }
      match topo.across(f, next.0, next.1) {
        Some(g) => (e, f) = (next, g),
        None => break,
      }
    }
  }
  (edges, crossed)
}

/// Insert Edge: a new edge loop across the quads through each picked edge, at `t` along it
/// (0.5 is midway). Returns the new vertices.
pub fn insert_edge(cage: &mut Cage, edges: &[[u32; 2]], t: f64) -> Result<Vec<u32>, String> {
  if !(0.0..=1.0).contains(&t) || t == 0.0 || t == 1.0 {
    return Err("the position along the edge must be between 0 and 1".into());
  }
  let sym = cage.symmetry.clone();
  let mut picks: Vec<([u32; 2], Option<DVec3>)> = edges.iter().map(|e| (*e, None)).collect();
  for (m, p) in sym.partners(cage) {
    for e in edges {
      if let (Some(Some(x)), Some(Some(y))) = (p.get(e[0] as usize), p.get(e[1] as usize)) {
        let at = m.transform_point3(cage.pos(e[0]).lerp(cage.pos(e[1]), t));
        picks.push(([*x, *y], Some(at)));
      }
    }
  }
  guarded(cage, |cage| {
    let mut out = Vec::new();
    let mut done: HashSet<(u32, u32)> = HashSet::new();
    for (e, at) in picks {
      if done.contains(&key(e[0], e[1])) {
        continue;
      }
      let topo = Topo::of(cage)?;
      if !topo.half.contains_key(&(e[0], e[1])) && !topo.half.contains_key(&(e[1], e[0])) {
        if at.is_none() {
          return Err(format!("{}-{} isn't an edge of the form", e[0], e[1]));
        }
        continue;
      }
      let (ring, crossed) = edge_ring(cage, &topo, e[0], e[1]);
      // the partner's point is the image of the picked one
      let tt = match at {
        Some(p) => {
          let (pa, pb) = (cage.pos(e[0]), cage.pos(e[1]));
          let d = pb - pa;
          ((p - pa).dot(d) / d.length_squared().max(1e-30)).clamp(0.01, 0.99)
        }
        None => t,
      };
      let mut new_of: HashMap<(u32, u32), u32> = HashMap::new();
      for &(x, y) in &ring {
        let n = split_edge(cage, x, y, tt);
        new_of.insert(key(x, y), n);
        done.insert(key(x, n));
        done.insert(key(n, y));
        out.push(n);
      }
      for (f, e1, e2) in crossed {
        split_face(cage, f, new_of[&key(e1.0, e1.1)], new_of[&key(e2.0, e2.1)])?;
      }
    }
    Ok(out)
  })
}

/// Insert Point: splits each picked edge at its position (0 at its first vertex) and joins
/// successive new points that share a face with a new edge. Returns the new vertices.
pub fn insert_point(cage: &mut Cage, points: &[([u32; 2], f64)]) -> Result<Vec<u32>, String> {
  if points.is_empty() {
    return Err("pick at least one edge".into());
  }
  guarded(cage, |cage| {
    let mut out: Vec<u32> = Vec::new();
    for (e, t) in points {
      if !(*t > 0.0 && *t < 1.0) {
        return Err("each point's position along its edge must be between 0 and 1".into());
      }
      if !cage.edges().contains(&[e[0].min(e[1]), e[0].max(e[1])]) {
        return Err(format!("{}-{} isn't an edge of the form", e[0], e[1]));
      }
      out.push(split_edge(cage, e[0], e[1], *t));
    }
    for w in out.windows(2) {
      if let Some(f) = cage.faces.iter().position(|f| f.contains(&w[0]) && f.contains(&w[1])) {
        split_face(cage, f, w[0], w[1])?;
      }
    }
    Ok(out)
  })
}

/// Joins faces f (with a → b) and g (with b → a) into one, dropping edge a-b and any spike left.
fn merge_faces(cage: &mut Cage, f: usize, g: usize, a: u32, b: u32) -> Result<(), String> {
  let rot = |face: &Vec<u32>, first: u32| {
    let k = face.iter().position(|x| *x == first).unwrap_or(0);
    let mut r = face[k..].to_vec();
    r.extend(&face[..k]);
    r
  };
  let mut merged = rot(&cage.faces[f], b);
  let other = rot(&cage.faces[g], a);
  merged.extend(&other[1..other.len() - 1]);
  // remove spikes x, y, x
  loop {
    let n = merged.len();
    let Some(k) = (0..n).find(|&k| n > 2 && merged[k] == merged[(k + 2) % n]) else { break };
    let (i, j) = ((k + 1) % n, (k + 2) % n);
    let (hi, lo) = (i.max(j), i.min(j));
    merged.remove(hi);
    merged.remove(lo);
  }
  if merged.len() < 3 {
    return Err("deleting that would leave a face with fewer than three corners".into());
  }
  cage.faces[f] = merged;
  cage.faces.remove(g);
  Ok(())
}

/// Removes vertices left with only two edges inside the surface (between two faces).
fn dissolve(cage: &mut Cage, candidates: &BTreeSet<u32>) {
  for &v in candidates {
    let faces: Vec<usize> = (0..cage.faces.len()).filter(|&f| cage.faces[f].contains(&v)).collect();
    if faces.len() != 2 || faces.iter().any(|&f| cage.faces[f].len() <= 3) {
      continue;
    }
    let mut nbrs = BTreeSet::new();
    for &f in &faces {
      let face = &cage.faces[f];
      let k = face.iter().position(|x| *x == v).unwrap_or(0);
      nbrs.insert(face[(k + 1) % face.len()]);
      nbrs.insert(face[(k + face.len() - 1) % face.len()]);
    }
    if nbrs.len() != 2 {
      continue;
    }
    let ends: Vec<u32> = nbrs.into_iter().collect();
    let creased = cage.is_creased(v, ends[0]) && cage.is_creased(v, ends[1]);
    for &f in &faces {
      cage.faces[f].retain(|x| *x != v);
    }
    if creased {
      cage.set_crease(ends[0], ends[1], true);
    }
  }
}

/// Delete: removes picked faces (leaving holes), edges (joining the faces either side, or
/// removing the face of an open edge) and vertices (joining the faces round them). Vertices
/// left between just two edges are dissolved.
pub fn delete(cage: &mut Cage, sel: &Selection) -> Result<(), String> {
  let sel = cage.symmetry.expand(cage, sel);
  guarded(cage, |cage| {
    let mut drop_faces: Vec<usize> = sel.faces.iter().map(|f| *f as usize).filter(|f| *f < cage.faces.len()).collect();
    let mut touched = BTreeSet::new();
    let mut edges: Vec<[u32; 2]> = sel.edges.clone();
    for &v in &sel.vertices {
      for e in cage.edges() {
        if e.contains(&v) {
          edges.push(e);
        }
      }
    }
    // faces first, by index from the end
    drop_faces.sort_unstable();
    drop_faces.dedup();
    for &f in drop_faces.iter().rev() {
      cage.faces.remove(f);
    }
    for e in edges {
      let (a, b) = (e[0], e[1]);
      let has = |cage: &Cage, x: u32, y: u32| {
        cage.faces.iter().position(|f| (0..f.len()).any(|k| f[k] == x && f[(k + 1) % f.len()] == y))
      };
      match (has(cage, a, b), has(cage, b, a)) {
        (Some(f), Some(g)) if f != g => {
          merge_faces(cage, f, g, a, b)?;
          touched.extend([a, b]);
        }
        (Some(f), None) | (None, Some(f)) => {
          cage.faces.remove(f);
        }
        _ => {}
      }
      cage.set_crease(a, b, false);
    }
    // a deleted vertex left on one face is taken out of it
    for &v in &sel.vertices {
      for f in &mut cage.faces {
        if f.len() > 3 {
          f.retain(|x| *x != v);
        }
      }
      touched.remove(&v);
    }
    dissolve(cage, &touched);
    if cage.faces.is_empty() {
      return Err("that would delete the whole form".into());
    }
    cage.compact();
    Ok(())
  })
}

/// Subdivide: splits the picked faces (all if none), each n-sided face into n quads round a
/// centre point; neighbouring faces gain the new edge midpoints. With `exact` and no faces
/// picked, the whole cage takes a Catmull-Clark step, which keeps the smooth shape exactly.
pub fn subdivide(cage: &mut Cage, faces: &[u32], exact: bool) -> Result<(), String> {
  if exact && faces.is_empty() {
    return guarded(cage, |cage| {
      let l = Level::of(cage).subdivide();
      cage.vertices = l.pos.iter().map(|p| p.to_array()).collect();
      cage.faces = l.polys;
      cage.creases = l.creases.into_iter().map(|(a, b)| [a, b]).collect();
      cage.creases.sort_unstable();
      Ok(())
    });
  }
  let picked: Vec<u32> = if faces.is_empty() { (0..cage.faces.len() as u32).collect() } else { faces.to_vec() };
  let sel = cage.symmetry.expand(cage, &Selection { faces: picked, ..Selection::default() });
  guarded(cage, |cage| {
    let originals: Vec<Vec<u32>> = sel.faces.iter().map(|&f| cage.faces[f as usize].clone()).collect();
    let mut mid: HashMap<(u32, u32), u32> = HashMap::new();
    for face in &originals {
      for k in 0..face.len() {
        let (a, b) = (face[k], face[(k + 1) % face.len()]);
        mid.entry(key(a, b)).or_insert_with(|| split_edge(cage, a, b, 0.5));
      }
    }
    let mut remove = Vec::new();
    for face in &originals {
      let n = face.len();
      let c = cage.add_vertex(face.iter().map(|&v| cage.pos(v)).sum::<DVec3>() / n as f64);
      // the face now has its midpoints in: find it by its corners
      let fi = cage
        .faces
        .iter()
        .position(|f| {
          f.len() >= 2 * n && face.iter().all(|v| f.contains(v)) && f.iter().any(|v| mid.values().any(|m| m == v))
        })
        .ok_or("lost a face while subdividing")?;
      remove.push(fi);
      for i in 0..n {
        let (prev, v, next) = (face[(i + n - 1) % n], face[i], face[(i + 1) % n]);
        cage.faces.push(vec![v, mid[&key(v, next)], c, mid[&key(prev, v)]]);
      }
    }
    remove.sort_unstable();
    for f in remove.into_iter().rev() {
      cage.faces.remove(f);
    }
    Ok(())
  })
}

/// Extrude: pulls the picked faces out along their normals by `distance` (negative pushes in),
/// with a wall of new faces round the region's edge. Returns the moved faces' new vertices.
pub fn extrude_faces(cage: &mut Cage, faces: &[u32], distance: f64) -> Result<Vec<u32>, String> {
  if faces.is_empty() {
    return Err("pick faces to extrude".into());
  }
  if !distance.is_finite() || distance == 0.0 {
    return Err("the extrude distance must be non-zero".into());
  }
  let sel = cage.symmetry.expand(cage, &Selection { faces: faces.to_vec(), ..Selection::default() });
  guarded(cage, |cage| {
    let picked: BTreeSet<usize> = sel.faces.iter().map(|f| *f as usize).filter(|f| *f < cage.faces.len()).collect();
    let mut normal: HashMap<u32, DVec3> = HashMap::new();
    for &f in &picked {
      let n = cage.face_normal(f);
      for &v in &cage.faces[f] {
        *normal.entry(v).or_default() += n;
      }
    }
    let directed: HashSet<(u32, u32)> = picked
      .iter()
      .flat_map(|&f| cage.faces[f].iter().enumerate().map(move |(k, _)| (f, k)))
      .map(|(f, k)| {
        let face = &cage.faces[f];
        (face[k], face[(k + 1) % face.len()])
      })
      .collect();
    let mut new_of: HashMap<u32, u32> = HashMap::new();
    let mut verts: Vec<u32> = normal.keys().copied().collect();
    verts.sort_unstable();
    for v in verts {
      let n = normal[&v].normalize_or_zero();
      let id = cage.add_vertex(cage.pos(v) + distance * n);
      new_of.insert(v, id);
    }
    // creases inside the region move with it
    let old_creases = cage.creases.clone();
    for c in old_creases {
      if directed.contains(&(c[0], c[1])) || directed.contains(&(c[1], c[0])) {
        let (a, b) = (new_of[&c[0]], new_of[&c[1]]);
        let rim = !(directed.contains(&(c[0], c[1])) && directed.contains(&(c[1], c[0])));
        if !rim {
          cage.set_crease(c[0], c[1], false);
        }
        cage.set_crease(a, b, true);
      }
    }
    for &(a, b) in &directed {
      if !directed.contains(&(b, a)) {
        cage.faces.push(vec![a, b, new_of[&b], new_of[&a]]);
      }
    }
    for &f in &picked {
      for v in cage.faces[f].iter_mut() {
        *v = new_of[v];
      }
    }
    cage.compact();
    let mut out: Vec<u32> = picked.iter().flat_map(|&f| cage.faces[f].clone()).collect();
    out.sort_unstable();
    out.dedup();
    Ok(out)
  })
}

/// Directed boundary edges for picked edges (each must be on the open boundary).
fn boundary_directed(cage: &Cage, edges: &[[u32; 2]]) -> Result<Vec<(u32, u32, usize)>, String> {
  let topo = Topo::of(cage)?;
  edges
    .iter()
    .map(|e| {
      let (a, b) = (e[0], e[1]);
      match (topo.half.get(&(a, b)), topo.half.get(&(b, a))) {
        (Some(h), None) => Ok((a, b, h.0)),
        (None, Some(h)) => Ok((b, a, h.0)),
        (Some(_), Some(_)) => Err(format!("{a}-{b} isn't an open edge")),
        (None, None) => Err(format!("{a}-{b} isn't an edge of the form")),
      }
    })
    .collect()
}

/// Extrude open edges: new faces off the picked boundary edges, moved by `offset` if given, else
/// `distance` straight out from their faces. Returns the new vertices.
pub fn extrude_edges(
  cage: &mut Cage,
  edges: &[[u32; 2]],
  distance: f64,
  offset: Option<DVec3>,
) -> Result<Vec<u32>, String> {
  let sel = cage.symmetry.expand(cage, &Selection { edges: edges.to_vec(), ..Selection::default() });
  let dir = boundary_directed(cage, &sel.edges)?;
  if dir.is_empty() {
    return Err("pick open edges to extrude".into());
  }
  guarded(cage, |cage| {
    let mut out_dir: HashMap<u32, DVec3> = HashMap::new();
    for &(a, b, f) in &dir {
      let d = match offset {
        Some(o) => o,
        None => (cage.pos(b) - cage.pos(a)).cross(cage.face_normal(f)).normalize_or_zero() * distance,
      };
      for v in [a, b] {
        *out_dir.entry(v).or_default() += d;
      }
    }
    let mut count: HashMap<u32, f64> = HashMap::new();
    for &(a, b, _) in &dir {
      for v in [a, b] {
        *count.entry(v).or_default() += 1.0;
      }
    }
    let mut new_of = HashMap::new();
    let mut verts: Vec<u32> = out_dir.keys().copied().collect();
    verts.sort_unstable();
    for v in verts {
      let d = out_dir[&v] / count[&v];
      let id = cage.add_vertex(cage.pos(v) + d);
      new_of.insert(v, id);
    }
    for &(a, b, _) in &dir {
      cage.faces.push(vec![b, a, new_of[&a], new_of[&b]]);
    }
    let mut out: Vec<u32> = new_of.values().copied().collect();
    out.sort_unstable();
    Ok(out)
  })
}

/// Orders directed boundary edges into one chain (or closed loop) of vertices.
fn chain(dir: &[(u32, u32, usize)]) -> Result<(Vec<u32>, bool), String> {
  let next: HashMap<u32, u32> = dir.iter().map(|&(a, b, _)| (a, b)).collect();
  let heads: HashSet<u32> = dir.iter().map(|d| d.1).collect();
  let start = dir.iter().map(|d| d.0).find(|a| !heads.contains(a));
  let closed = start.is_none();
  let first = start.unwrap_or(dir[0].0);
  let mut out = vec![first];
  let mut cur = first;
  while let Some(&n) = next.get(&cur) {
    if n == first {
      break;
    }
    out.push(n);
    cur = n;
    if out.len() > dir.len() + 1 {
      break;
    }
  }
  let edges = if closed { out.len() } else { out.len() - 1 };
  if edges != dir.len() {
    return Err("the picked edges don't make one chain".into());
  }
  Ok((out, closed))
}

/// Bridge: joins two chains (or loops) of open edges with a band of faces, `segments` faces
/// across. Both must have the same number of edges.
pub fn bridge(cage: &mut Cage, side_a: &[[u32; 2]], side_b: &[[u32; 2]], segments: u32) -> Result<(), String> {
  if !(1..=64).contains(&segments) {
    return Err("the bridge needs 1 to 64 faces across".into());
  }
  let (a, ca) = chain(&boundary_directed(cage, side_a)?)?;
  let (b, cb) = chain(&boundary_directed(cage, side_b)?)?;
  if side_a.len() != side_b.len() || ca != cb {
    return Err("both sides need the same number of edges, and both open or both closed".into());
  }
  if a.iter().any(|v| b.contains(v)) {
    return Err("the two sides share a vertex".into());
  }
  guarded(cage, |cage| {
    let mut b: Vec<u32> = b.into_iter().rev().collect();
    if ca {
      // the turn of the loop that lines its points up best
      let n = b.len();
      let cost = |off: usize| (0..n).map(|i| cage.pos(a[i]).distance(cage.pos(b[(i + off) % n]))).sum::<f64>();
      let off = (0..n).min_by(|x, y| cost(*x).total_cmp(&cost(*y))).unwrap_or(0);
      b.rotate_left(off);
    }
    let n = a.len();
    let mut rows = vec![a.clone()];
    for s in 1..segments {
      let t = f64::from(s) / f64::from(segments);
      let row = (0..n).map(|i| cage.add_vertex(cage.pos(a[i]).lerp(cage.pos(b[i]), t))).collect();
      rows.push(row);
    }
    rows.push(b);
    let spans = if ca { n } else { n - 1 };
    for r in 0..rows.len() - 1 {
      let (lo, hi) = (&rows[r], &rows[r + 1]);
      for i in 0..spans {
        let j = (i + 1) % n;
        cage.faces.push(vec![lo[j], lo[i], hi[i], hi[j]]);
      }
    }
    Ok(())
  })
}

/// How Fill Hole closes a hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
  /// One many-sided face.
  Polygon,
  /// A new point in the middle, joined to every other point round the hole (quads; triangles
  /// for an odd count).
  Star,
}

/// Fill Hole: closes the open loop through an edge. Returns the new faces.
pub fn fill_hole(cage: &mut Cage, edge: [u32; 2], mode: Fill) -> Result<Vec<u32>, String> {
  let dir = boundary_directed(cage, &[edge])?;
  let sym = cage.symmetry.clone();
  let mut starts = vec![(dir[0].0, dir[0].1)];
  for (_, p) in sym.partners(cage) {
    if let (Some(Some(x)), Some(Some(y))) = (p.get(edge[0] as usize), p.get(edge[1] as usize))
      && let Ok(d) = boundary_directed(cage, &[[*x, *y]])
    {
      starts.push((d[0].0, d[0].1));
    }
  }
  guarded(cage, |cage| {
    let mut out = Vec::new();
    let mut filled: HashSet<u32> = HashSet::new();
    for (a, b) in starts {
      if filled.contains(&a) {
        continue;
      }
      let topo = Topo::of(cage)?;
      let l = topo.boundary_loop(cage, a, b);
      let n = l.len();
      let closes = topo.half.contains_key(&(l[n - 1], l[0])) && !topo.half.contains_key(&(l[0], l[n - 1]));
      if n < 3 || !closes {
        return Err("that edge isn't on a closed hole".into());
      }
      filled.extend(l.iter().copied());
      match mode {
        Fill::Polygon => {
          cage.faces.push(l.iter().rev().copied().collect());
          out.push((cage.faces.len() - 1) as u32);
        }
        Fill::Star => {
          let c = cage.add_vertex(l.iter().map(|&v| cage.pos(v)).sum::<DVec3>() / n as f64);
          if n % 2 == 0 {
            for k in 0..n / 2 {
              cage.faces.push(vec![c, l[(2 * k + 2) % n], l[2 * k + 1], l[2 * k]]);
              out.push((cage.faces.len() - 1) as u32);
            }
          } else {
            for k in 0..n {
              cage.faces.push(vec![c, l[(k + 1) % n], l[k]]);
              out.push((cage.faces.len() - 1) as u32);
            }
          }
        }
      }
    }
    Ok(out)
  })
}

/// Crease or Uncrease: makes the picked edges (and the edges of picked faces and round picked
/// vertices) sharp in the smooth body, or smooth again.
pub fn crease(cage: &mut Cage, sel: &Selection, on: bool) -> Result<usize, String> {
  let sel = cage.symmetry.expand(cage, sel);
  let all = cage.edges();
  let mut edges: BTreeSet<[u32; 2]> = sel.edges.iter().copied().collect();
  for e in &all {
    if sel.vertices.iter().any(|v| e.contains(v)) {
      edges.insert(*e);
    }
  }
  for &f in &sel.faces {
    if let Some(face) = cage.faces.get(f as usize) {
      for k in 0..face.len() {
        let (a, b) = key(face[k], face[(k + 1) % face.len()]);
        edges.insert([a, b]);
      }
    }
  }
  let mut n = 0;
  for e in edges {
    if all.contains(&e) {
      cage.set_crease(e[0], e[1], on);
      n += 1;
    }
  }
  if n == 0 {
    return Err("no edges picked".into());
  }
  Ok(n)
}

/// Merges vertices closer than `tol` (joining a mirrored or copied half to the original along
/// their shared boundary). Faces left degenerate are dropped.
pub fn weld(cage: &mut Cage, tol: f64) -> Result<usize, String> {
  guarded(cage, |cage| {
    let n = cage.vertices.len();
    let mut map: Vec<u32> = (0..n as u32).collect();
    let mut merged = 0;
    let cell = tol.max(1e-12) * 2.0;
    let mut grid: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
    for v in 0..n as u32 {
      let p = cage.pos(v);
      let c = ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64, (p.z / cell).floor() as i64);
      let mut found = None;
      'search: for dx in -1..=1 {
        for dy in -1..=1 {
          for dz in -1..=1 {
            for &w in grid.get(&(c.0 + dx, c.1 + dy, c.2 + dz)).into_iter().flatten() {
              if cage.pos(w).distance(p) <= tol {
                found = Some(w);
                break 'search;
              }
            }
          }
        }
      }
      match found {
        Some(w) => {
          map[v as usize] = w;
          merged += 1;
        }
        None => grid.entry(c).or_default().push(v),
      }
    }
    for f in &mut cage.faces {
      for v in f.iter_mut() {
        *v = map[*v as usize];
      }
      f.dedup();
      if f.len() > 1 && f.first() == f.last() {
        f.pop();
      }
    }
    cage.faces.retain(|f| f.len() >= 3);
    for c in &mut cage.creases {
      *c = [map[c[0] as usize], map[c[1] as usize]];
      c.sort_unstable();
    }
    cage.compact();
    Ok(merged)
  })
}

/// Reverses every face, turning the body inside out (Fusion's Reverse Normal).
pub fn flip(cage: &mut Cage) {
  for f in &mut cage.faces {
    f.reverse();
  }
}

/// Mirror - Duplicate: adds the cage's mirror image in the plane, joined to it where they meet
/// on the plane, and keeps the two halves symmetric from then on.
pub fn mirror_duplicate(cage: &mut Cage, plane: SymPlane) -> Result<(), String> {
  let m = reflection(DVec3::from(plane.origin), DVec3::from(plane.normal));
  duplicate(cage, &[m])?;
  let mut planes = match &cage.symmetry {
    Symmetry::Mirror { planes } => planes.clone(),
    _ => Vec::new(),
  };
  planes.push(plane);
  cage.symmetry = Symmetry::Mirror { planes };
  Ok(())
}

/// Circular - Duplicate: `count` copies of the cage round the axis, joined where they meet, kept
/// symmetric from then on.
pub fn circular_duplicate(cage: &mut Cage, origin: DVec3, axis: DVec3, count: u32) -> Result<(), String> {
  if !(2..=64).contains(&count) {
    return Err("circular symmetry needs 2 to 64 copies".into());
  }
  let sym = Symmetry::Circular { origin: origin.to_array(), axis: axis.to_array(), count };
  duplicate(cage, &sym.maps())?;
  cage.symmetry = sym;
  Ok(())
}

fn duplicate(cage: &mut Cage, maps: &[DMat4]) -> Result<(), String> {
  let tol = 1e-6 * cage.size();
  guarded(cage, |cage| {
    let base = cage.clone();
    for m in maps {
      let off = cage.vertices.len() as u32;
      let flips = m.determinant() < 0.0;
      cage.vertices.extend(base.vertices.iter().map(|v| m.transform_point3(DVec3::from(*v)).to_array()));
      for f in &base.faces {
        let mut g: Vec<u32> = f.iter().map(|v| v + off).collect();
        if flips {
          g.reverse();
        }
        cage.faces.push(g);
      }
      cage.creases.extend(base.creases.iter().map(|c| [c[0] + off, c[1] + off]));
    }
    Ok(())
  })?;
  weld(cage, tol)?;
  Ok(())
}

/// Symmetry - Internal: sets (or with `Symmetry::None` clears) the symmetry kept on the body,
/// checking every vertex has its partners.
pub fn set_symmetry(cage: &mut Cage, sym: Symmetry) -> Result<(), String> {
  for (_, p) in sym.partners(cage) {
    if let Some(v) = p.iter().position(Option::is_none) {
      return Err(format!("the form isn't symmetric: vertex {v} has no partner"));
    }
  }
  cage.symmetry = sym;
  Ok(())
}

/// Make Uniform: evens out the spacing of the control points across the surface (each free
/// vertex slides along the surface towards the middle of its neighbours), keeping open edges,
/// creases and corners where they are.
pub fn make_uniform(cage: &mut Cage, iterations: u32) -> Result<usize, String> {
  let topo = Topo::of(cage)?;
  let n = cage.vertices.len();
  let fixed: Vec<bool> = (0..n as u32)
    .map(|v| {
      topo.is_boundary_vertex(v) || cage.creases.iter().any(|c| c.contains(&v)) || topo.nbrs[v as usize].is_empty()
    })
    .collect();
  for _ in 0..iterations.clamp(1, 100) {
    let mut moves = Vec::new();
    for v in 0..n as u32 {
      if fixed[v as usize] {
        continue;
      }
      let p = cage.pos(v);
      let nb = &topo.nbrs[v as usize];
      let mean = nb.iter().map(|&u| cage.pos(u)).sum::<DVec3>() / nb.len() as f64;
      let normal = cage.vertex_normal(v);
      let d = mean - p;
      moves.push((v, p + 0.5 * (d - normal * d.dot(normal)), 1.0));
    }
    for (v, p) in cage.symmetry.symmetric_moves(cage, &moves) {
      cage.vertices[v as usize] = p.to_array();
    }
  }
  Ok(fixed.iter().filter(|f| !**f).count())
}

/// Pull: snaps the picked vertices onto a target, given as its closest point to any point. With
/// `limit`, the smooth surface's points over the vertices are pulled onto it instead of the
/// control points. Returns the largest distance left between a pulled point and the target.
pub fn pull(
  cage: &mut Cage,
  vertices: &[u32],
  limit: bool,
  closest: impl Fn(DVec3) -> Option<DVec3>,
) -> Result<f64, String> {
  let sel = cage.symmetry.expand(cage, &Selection { vertices: vertices.to_vec(), ..Selection::default() });
  let picked = sel.vertex_set(cage);
  if picked.is_empty() {
    return Err("pick vertices to pull".into());
  }
  let mut gap = 0.0f64;
  if !limit {
    for &v in &picked {
      let p = cage.pos(v);
      let q = closest(p).ok_or("couldn't find the target")?;
      cage.vertices[v as usize] = q.to_array();
    }
    return Ok(0.0);
  }
  // move each control point by its surface point's miss until they all land
  for _ in 0..200 {
    let (lim, _) = limit_mesh(cage, 1)?;
    gap = 0.0;
    let mut moves = Vec::with_capacity(picked.len());
    for &v in &picked {
      let at = lim[v as usize];
      let q = closest(at).ok_or("couldn't find the target")?;
      gap = gap.max(q.distance(at));
      // over-relaxed: a control point moves its surface point about half as far
      moves.push((v, cage.pos(v) + 1.6 * (q - at)));
    }
    if gap < 1e-10 * cage.size() {
      break;
    }
    for (v, p) in moves {
      cage.vertices[v as usize] = p.to_array();
    }
  }
  Ok(gap)
}
