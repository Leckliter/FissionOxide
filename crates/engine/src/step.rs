//! STEP assemblies in and out, as in Fusion: sub-assemblies are nested components, a part or
//! sub-assembly the file places more than once comes in once, with linked copies
//! ([`FeatureKind::CopyComponent`]) for its other placements, and colours sit on bodies and single
//! faces. Export writes the same structure back: components as sub-assemblies, linked copies as
//! further instances of their source, face colours on their faces.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use fox_kernel::glam::{DMat4, DVec3};
use fox_kernel::{StepNode, StepWriter};
use fox_model::refs::FaceRef;
use fox_model::{BodyId, ComponentId, Design, FaceAppearance, FeatureId, FeatureKind};
use serde_json::{Value, json};

use crate::{CmdError, CmdResult, Session};

/// Where an occurrence of the file goes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Role {
  /// Its bodies go to the nearest component above it (or the root).
  Bodies,
  /// A component of its own with the occurrence's geometry.
  Original,
  /// A linked copy of an earlier occurrence of the same prototype.
  Copy(usize),
  /// Inside a linked copy: the copy brings it along.
  Mirrored,
}

/// Imports a STEP file: its bodies as one Import feature, then the components, linked copies,
/// names and colours, all in one undo step. Files whose structure can't be read come in as plain
/// bodies.
pub(crate) fn import(s: &mut Session, path: &Path, file_name: String) -> CmdResult {
  let nodes = match fox_kernel::read_step_tree(path) {
    Ok(n) => n,
    Err(_) => {
      let shapes = fox_kernel::read_step(path)?;
      let breps = shapes.iter().map(|sh| sh.to_brep()).collect::<Result<Vec<_>, _>>()?;
      return crate::files::add_import(s, FeatureKind::Import { file_name, breps, meshes: Vec::new() });
    }
  };
  let n = nodes.len();
  let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
  for (i, node) in nodes.iter().enumerate() {
    if let Some(p) = node.parent {
      children[p].push(i);
    }
  }
  // the roles, parents first
  let mut role = vec![Role::Bodies; n];
  let mut first: HashMap<usize, usize> = HashMap::new();
  for i in 0..n {
    let node = &nodes[i];
    if node.parent.is_some_and(|p| matches!(role[p], Role::Copy(_) | Role::Mirrored)) {
      role[i] = Role::Mirrored;
    } else if node.assembly || node.uses > 1 {
      role[i] = match first.get(&node.proto) {
        Some(&o) => Role::Copy(o),
        None => {
          first.insert(node.proto, i);
          Role::Original
        }
      };
    }
  }
  // the geometry: every body not brought by a copy
  let mut placed: Vec<(usize, usize)> = Vec::new();
  for (i, node) in nodes.iter().enumerate() {
    if matches!(role[i], Role::Bodies | Role::Original) {
      placed.extend((0..node.bodies.len()).map(|k| (i, k)));
    }
  }
  let breps = placed.iter().map(|&(i, k)| nodes[i].bodies[k].shape.to_brep()).collect::<Result<Vec<_>, _>>()?;
  let had: HashSet<BodyId> = s.model.bodies.iter().map(|b| b.id).collect();
  let mut out = crate::files::add_import(s, FeatureKind::Import { file_name, breps, meshes: Vec::new() })?;
  let ids: Vec<BodyId> = out["bodies"]
    .as_array()
    .map(|a| a.iter().filter_map(Value::as_u64).map(|v| BodyId(v as u32)).collect())
    .unwrap_or_default();
  let structured = role.iter().any(|r| *r != Role::Bodies);
  let named = nodes.iter().flat_map(|n| &n.bodies).any(|b| !b.name.is_empty() || b.color.is_some());
  if ids.len() != placed.len() || !(structured || named) {
    return Ok(out);
  }
  let body_of: HashMap<(usize, usize), BodyId> = placed.iter().copied().zip(ids.iter().copied()).collect();

  // body names and colours, components, then the linked copies
  let edit = s.edit(|d| {
    let mut face_colors: BTreeMap<BodyId, Vec<(u32, [u8; 3])>> = BTreeMap::new();
    for (&(i, k), &b) in placed.iter().zip(&ids) {
      let body = &nodes[i].bodies[k];
      let set = d.body_settings_mut(b);
      if !body.name.trim().is_empty() {
        set.name = Some(body.name.trim().to_string());
      }
      set.color = body.color;
      if !body.face_colors.is_empty() {
        face_colors.insert(b, body.face_colors.clone());
      }
    }
    let mut comp: HashMap<usize, ComponentId> = HashMap::new();
    let mut made = Vec::new();
    for i in 0..n {
      let parent = holder_of(&nodes, &role, &comp, i);
      match role[i] {
        Role::Original => {
          let mut bodies: Vec<BodyId> =
            (0..nodes[i].bodies.len()).filter_map(|k| body_of.get(&(i, k)).copied()).collect();
          for &c in &children[i] {
            if role[c] == Role::Bodies {
              bodies.extend((0..nodes[c].bodies.len()).filter_map(|k| body_of.get(&(c, k)).copied()));
            }
          }
          let name = unique_name(d, &occurrence_name(&nodes[i], i));
          let id = d.add_component(&name, bodies)?;
          set_parent(d, id, parent);
          comp.insert(i, id);
          made.push(id.0);
        }
        Role::Copy(o) => {
          let at = nodes[i].placement * nodes[o].placement.inverse();
          mirror(d, &nodes, &children, &mut comp, &mut face_colors, i, o, parent, at, &mut made)?;
        }
        Role::Bodies | Role::Mirrored => {}
      }
    }
    Ok((made, face_colors))
  });
  let Ok((made, face_colors)) = edit else { return Ok(out) };
  // one undo step with the import
  s.undo.pop();
  if !made.is_empty() {
    out["components"] = json!(made);
  }
  // the copies' bodies are the import's too
  let all: Vec<u32> = s.model.bodies.iter().filter(|b| !had.contains(&b.id)).map(|b| b.id.0).collect();
  if all.len() > ids.len() {
    out["linked_bodies"] = json!(all.len() - ids.len());
    out["bodies"] = json!(all);
  }
  // face colours, on the bodies as built
  let mut appearances = Vec::new();
  for (b, faces) in &face_colors {
    appearances.extend(face_appearances(s, *b, faces));
  }
  if !appearances.is_empty() && s.edit(|d| Ok(d.face_appearances.extend(appearances))).is_ok() {
    s.undo.pop();
  }
  Ok(out)
}

/// The name a component made from an occurrence goes by: the occurrence's own, else its
/// prototype's, else a number.
fn occurrence_name(n: &StepNode, i: usize) -> String {
  [&n.instance, &n.name]
    .into_iter()
    .map(|s| s.trim())
    .find(|s| !s.is_empty())
    .map(str::to_string)
    .unwrap_or_else(|| format!("Component{}", i + 1))
}

/// The component an occurrence sits in: the component made for the nearest occurrence above it.
fn holder_of(nodes: &[StepNode], role: &[Role], comp: &HashMap<usize, ComponentId>, i: usize) -> Option<ComponentId> {
  let mut at = nodes[i].parent;
  while let Some(k) = at {
    if matches!(role[k], Role::Original | Role::Copy(_)) {
      return comp.get(&k).copied();
    }
    at = nodes[k].parent;
  }
  None
}

fn set_parent(d: &mut Design, id: ComponentId, parent: Option<ComponentId>) {
  if let Some(c) = d.component_mut(id) {
    c.parent = parent;
  }
}

/// `name`, or the next free `base:k` when a component already has it (`Wheel:1` taken: `Wheel:2`).
fn unique_name(d: &Design, name: &str) -> String {
  let taken = |n: &str| d.components.iter().any(|c| c.name == n);
  if !taken(name) {
    return name.to_string();
  }
  let base = match name.rsplit_once(':') {
    Some((b, k)) if !b.is_empty() && k.parse::<u32>().is_ok() => b,
    _ => name,
  };
  (2..).map(|k| format!("{base}:{k}")).find(|n| !taken(n)).unwrap_or_default()
}

/// Occurrence `i` as a linked copy of occurrence `o` (already a component), moved by `at`, in
/// `parent`; its sub-components copy `o`'s, so the copy has the original's whole structure.
#[allow(clippy::too_many_arguments)]
fn mirror(
  d: &mut Design,
  nodes: &[StepNode],
  children: &[Vec<usize>],
  comp: &mut HashMap<usize, ComponentId>,
  face_colors: &mut BTreeMap<BodyId, Vec<(u32, [u8; 3])>>,
  i: usize,
  o: usize,
  parent: Option<ComponentId>,
  at: DMat4,
  made: &mut Vec<u32>,
) -> Result<(), CmdError> {
  let source = *comp.get(&o).ok_or(CmdError::new("a linked copy's original is missing"))?;
  let src = d.component(source).ok_or(CmdError::new("no such component"))?;
  let (src_bodies, material) = (src.bodies.clone(), src.material.clone());
  let fid = d.add_feature(FeatureKind::CopyComponent {
    source,
    translate: ["0".into(), "0".into(), "0".into()],
    transform: Some(at.to_cols_array()),
    component: None,
  })?;
  // the copies' body ids are fixed by the feature, in the source's order (see `component.copy`)
  let bodies: Vec<BodyId> = (0..src_bodies.len() as u32).map(|k| BodyId(fid.0 * 1000 + k)).collect();
  for (from, to) in src_bodies.iter().zip(&bodies) {
    let (name, color) = d.body_settings(*from).map(|b| (b.name.clone(), b.color)).unwrap_or_default();
    let set = d.body_settings_mut(*to);
    set.name = name;
    set.color = color;
    if let Some(f) = face_colors.get(from).cloned() {
      face_colors.insert(*to, f);
    }
  }
  let name = unique_name(d, &occurrence_name(&nodes[i], i));
  let id = d.add_component(&name, bodies)?;
  set_parent(d, id, parent);
  if let Some(c) = d.component_mut(id) {
    c.material = material;
  }
  if let Some(FeatureKind::CopyComponent { component, .. }) = d.feature_mut(fid).map(|f| &mut f.kind) {
    *component = Some(id);
  }
  comp.insert(i, id);
  made.push(id.0);
  // the original's sub-components, pairwise (occurrences of one prototype have the same children)
  for (&ci, &co) in children[i].iter().zip(&children[o]) {
    if comp.contains_key(&co) {
      mirror(d, nodes, children, comp, face_colors, ci, co, Some(id), at, made)?;
    }
  }
  Ok(())
}

/// Face appearances for the given faces of a body (1-based ids): each found again from a point
/// well inside it.
fn face_appearances(s: &Session, body: BodyId, faces: &[(u32, [u8; 3])]) -> Vec<FaceAppearance> {
  let Some(b) = s.model.body(body) else { return Vec::new() };
  let Some(mesh) = b.mesh() else { return Vec::new() };
  // the centre of each face's largest triangle
  let mut inside: HashMap<u32, (f64, DVec3)> = HashMap::new();
  for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
    let Some(&face) = mesh.tri_face.get(t) else { continue };
    let p: Vec<DVec3> =
      tri.iter().filter_map(|&i| mesh.positions.get(i as usize)).map(|q| DVec3::from(q.map(f64::from))).collect();
    if p.len() != 3 {
      continue;
    }
    let area = (p[1] - p[0]).cross(p[2] - p[0]).length();
    if inside.get(&face).is_none_or(|e| area > e.0) {
      inside.insert(face, (area, (p[0] + p[1] + p[2]) / 3.0));
    }
  }
  let mut out = Vec::new();
  for &(id, color) in faces {
    let (Some(info), Some(&(_, at))) = (b.faces().iter().find(|f| f.id == id), inside.get(&id)) else { continue };
    let r = FaceRef::from_face(body, info, at);
    if r.resolve(b.faces(), Some(mesh), b.diagonal()) == Some(id) {
      out.push(FaceAppearance { face: r, color });
    }
  }
  out
}

/// Writes bodies as a STEP assembly: components as sub-assemblies (nested as in the design),
/// linked copies as instances of their source, bodies as named parts with their colours and
/// face colours.
pub(crate) fn write(s: &Session, bodies: &[&fox_model::regen::Body], path: &Path) -> Result<(), CmdError> {
  let exported: HashSet<BodyId> = bodies.iter().map(|b| b.id).collect();
  let d = &s.design;
  let kids = |p: Option<ComponentId>| -> Vec<ComponentId> {
    d.components.iter().filter(|c| c.parent == p).map(|c| c.id).collect()
  };
  // components with an exported body somewhere below them
  let mut wanted: HashSet<ComponentId> = HashSet::new();
  for c in &d.components {
    if c.bodies.iter().any(|b| exported.contains(b)) {
      let mut at = Some(c.id);
      while let Some(k) = at {
        if !wanted.insert(k) {
          break;
        }
        at = d.component(k).and_then(|c| c.parent);
      }
    }
  }
  let links = Links::find(s, &exported);
  let mut w = StepWriter::new(&d.name)?;
  let mut labels: HashMap<ComponentId, usize> = HashMap::new();
  // bodies outside any component sit in the top assembly
  let mut in_component: HashSet<BodyId> = HashSet::new();
  for c in &d.components {
    in_component.extend(c.bodies.iter().copied());
  }
  for b in bodies.iter().filter(|b| !in_component.contains(&b.id)) {
    let p = part(s, &mut w, b)?;
    w.place(StepWriter::ROOT, p, &s.body_name(b.id), DMat4::IDENTITY)?;
  }
  for c in kids(None) {
    if wanted.contains(&c) {
      place_component(s, &mut w, &links, &wanted, &exported, &mut labels, StepWriter::ROOT, c)?;
    }
  }
  Ok(w.write(path)?)
}

/// A body as a STEP part, with its colour and face colours.
fn part(s: &Session, w: &mut StepWriter, b: &fox_model::regen::Body) -> Result<usize, CmdError> {
  let color = s.design.body_settings(b.id).and_then(|x| x.color);
  let mut faces: Vec<(u32, [u8; 3])> = crate::views::body_face_colors(s, b.id).into_iter().collect();
  faces.sort();
  Ok(w.part(&s.body_name(b.id), &b.shape, color, &faces)?)
}

/// Puts component `c` into assembly label `parent`: an instance of its source when it is a
/// linked copy, else its own sub-assembly.
#[allow(clippy::too_many_arguments)]
fn place_component(
  s: &Session,
  w: &mut StepWriter,
  links: &Links,
  wanted: &HashSet<ComponentId>,
  exported: &HashSet<BodyId>,
  labels: &mut HashMap<ComponentId, usize>,
  parent: usize,
  c: ComponentId,
) -> Result<(), CmdError> {
  let name = s.design.component(c).map(|c| c.name.clone()).unwrap_or_default();
  let (proto, at) = links.resolved(c).unwrap_or((c, DMat4::IDENTITY));
  let label = assembly(s, w, links, wanted, exported, labels, proto)?;
  w.place(parent, label, &name, at)?;
  Ok(())
}

/// The sub-assembly label of an original component (made once, then placed as often as needed).
#[allow(clippy::too_many_arguments)]
fn assembly(
  s: &Session,
  w: &mut StepWriter,
  links: &Links,
  wanted: &HashSet<ComponentId>,
  exported: &HashSet<BodyId>,
  labels: &mut HashMap<ComponentId, usize>,
  c: ComponentId,
) -> Result<usize, CmdError> {
  if let Some(l) = labels.get(&c) {
    return Ok(*l);
  }
  let comp = s.design.component(c).ok_or(CmdError::new("no such component"))?;
  let label = w.assembly(&comp.name)?;
  labels.insert(c, label);
  for b in comp.bodies.iter().filter(|b| exported.contains(b)) {
    let Some(body) = s.model.body(*b) else { continue };
    let p = part(s, w, body)?;
    w.place(label, p, &s.body_name(*b), DMat4::IDENTITY)?;
  }
  for k in s.design.components.iter().filter(|k| k.parent == Some(c) && wanted.contains(&k.id)) {
    place_component(s, w, links, wanted, exported, labels, label, k.id)?;
  }
  Ok(label)
}

/// The linked copies that can be written as instances: copy -> (its source, how far the copy is
/// moved from it).
struct Links {
  direct: HashMap<ComponentId, (ComponentId, DMat4)>,
}

impl Links {
  fn find(s: &Session, exported: &HashSet<BodyId>) -> Links {
    let d = &s.design;
    let placed = |c: ComponentId| s.model.placed.get(&c).copied().unwrap_or(DMat4::IDENTITY);
    let mut direct = HashMap::new();
    for f in &d.features {
      let FeatureKind::CopyComponent { source, translate, transform, component } = &f.kind else { continue };
      if f.suppressed || s.model.status(f.id).is_some_and(|st| st.error.is_some()) {
        continue;
      }
      let Some(copy) = component.or_else(|| legacy_copy(d, f.id)) else { continue };
      let len = |e: &str| fox_expr::eval(e, fox_expr::Kind::Length, &s.model.params).ok();
      let (Some(x), Some(y), Some(z)) = (len(&translate[0]), len(&translate[1]), len(&translate[2])) else { continue };
      let t = DMat4::from_translation(DVec3::new(x, y, z))
        * transform.map(|m| DMat4::from_cols_array(&m)).unwrap_or(DMat4::IDENTITY);
      // the copy's bodies are the source's, moved by the copy and since by joints
      let at = placed(copy) * t * placed(*source).inverse();
      if fox_kernel::is_rigid(at) && same_bodies(s, exported, *source, copy, at) {
        direct.insert(copy, (*source, at));
      }
    }
    let mut links = Links { direct };
    // a copy whose sub-components aren't its source's, copied the same way, is written out whole
    loop {
      let bad: Vec<ComponentId> =
        links.direct.keys().copied().filter(|c| !links.children_match(s, *c, exported)).collect();
      if bad.is_empty() {
        break;
      }
      for c in bad {
        links.direct.remove(&c);
      }
    }
    links
  }

  /// The original a linked copy places in the end, and where: copies of copies chain back.
  fn resolved(&self, c: ComponentId) -> Option<(ComponentId, DMat4)> {
    let (mut at, mut m) = (*self.direct.get(&c)?, DMat4::IDENTITY);
    m = at.1 * m;
    for _ in 0..64 {
      match self.direct.get(&at.0) {
        Some(next) => {
          m *= next.1;
          at = *next;
        }
        None => return Some((at.0, m)),
      }
    }
    None
  }

  /// Do `c`'s sub-components stand one-to-one for its original's, each moved by the same amount?
  fn children_match(&self, s: &Session, c: ComponentId, exported: &HashSet<BodyId>) -> bool {
    let Some((proto, at)) = self.resolved(c) else { return false };
    // never an instance of something it sits in
    let mut up = Some(c);
    while let Some(k) = up {
      if k == proto {
        return false;
      }
      up = s.design.component(k).and_then(|x| x.parent);
    }
    let shown = |k: &fox_model::Component| subtree_exported(s, k.id, exported);
    let mine: Vec<ComponentId> =
      s.design.components.iter().filter(|k| k.parent == Some(c) && shown(k)).map(|k| k.id).collect();
    let theirs: Vec<ComponentId> =
      s.design.components.iter().filter(|k| k.parent == Some(proto) && shown(k)).map(|k| k.id).collect();
    if mine.len() != theirs.len() {
      return false;
    }
    let mut used = HashSet::new();
    for m in mine {
      // m must place what one of the original's sub-components places, moved by `at` more
      let (mp, mat) = self.resolved(m).unwrap_or((m, DMat4::IDENTITY));
      let hit = theirs.iter().find(|t| {
        let (tp, tat) = self.resolved(**t).unwrap_or((**t, DMat4::IDENTITY));
        !used.contains(*t) && tp == mp && mat.abs_diff_eq(at * tat, 1e-6 * (1.0 + mat.w_axis.length()))
      });
      match hit {
        Some(t) => {
          used.insert(*t);
        }
        None => return false,
      }
    }
    true
  }
}

/// Is any body of the component or below it exported?
fn subtree_exported(s: &Session, c: ComponentId, exported: &HashSet<BodyId>) -> bool {
  let d = &s.design;
  d.component(c).is_some_and(|k| k.bodies.iter().any(|b| exported.contains(b)))
    || d.components.iter().filter(|k| k.parent == Some(c)).any(|k| subtree_exported(s, k.id, exported))
}

/// The component a copy feature from before copies remembered theirs: the one holding exactly
/// the feature's bodies.
fn legacy_copy(d: &Design, fid: FeatureId) -> Option<ComponentId> {
  d.components
    .iter()
    .find(|c| !c.bodies.is_empty() && c.bodies.iter().enumerate().all(|(k, b)| b.0 == fid.0 * 1000 + k as u32))
    .map(|c| c.id)
}

/// Are the copy's bodies the source's moved by `at` (all exported, same count, same geometry)?
fn same_bodies(s: &Session, exported: &HashSet<BodyId>, source: ComponentId, copy: ComponentId, at: DMat4) -> bool {
  let (Some(a), Some(b)) = (s.design.component(source), s.design.component(copy)) else { return false };
  let alive = |ids: &[BodyId]| ids.iter().filter_map(|i| s.model.body(*i)).collect::<Vec<_>>();
  let (from, to) = (alive(&a.bodies), alive(&b.bodies));
  if from.len() != to.len() || from.iter().chain(&to).any(|x| !exported.contains(&x.id)) {
    return false;
  }
  from.iter().zip(&to).all(|(f, t)| {
    let (Ok(moved), Ok(want)) = (f.shape.transformed(at).and_then(|m| m.mass()), t.shape.mass()) else { return false };
    let tol = 1e-6 * (1.0 + (want.bbox_max - want.bbox_min).length());
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * b.abs().max(1.0);
    close(moved.volume, want.volume)
      && close(moved.area, want.area)
      && moved.center_of_mass.abs_diff_eq(want.center_of_mass, tol)
      && moved.bbox_min.abs_diff_eq(want.bbox_min, 10.0 * tol)
      && moved.bbox_max.abs_diff_eq(want.bbox_max, 10.0 * tol)
  })
}
