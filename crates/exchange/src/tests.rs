use super::*;
use crate::archive::{Archive, ArchivedVersion, inspect_fusion, read_foxz, write_foxz};
use crate::dxf::{Drawing, Entity, bulge_arc, read_dxf, write_dxf};
use crate::svg::{SvgUnits, read_svg, read_svg_as, write_svg};
use std::io::Write as _;

/// A closed unit-ish cube: 8 vertices, 12 outward-facing triangles.
fn cube(size: f32) -> Mesh {
  let s = size;
  let positions = vec![
    [0.0, 0.0, 0.0],
    [s, 0.0, 0.0],
    [s, s, 0.0],
    [0.0, s, 0.0],
    [0.0, 0.0, s],
    [s, 0.0, s],
    [s, s, s],
    [0.0, s, s],
  ];
  #[rustfmt::skip]
  let indices = vec![
    0, 2, 1, 0, 3, 2, // bottom
    4, 5, 6, 4, 6, 7, // top
    0, 1, 5, 0, 5, 4, // front
    1, 2, 6, 1, 6, 5, // right
    2, 3, 7, 2, 7, 6, // back
    3, 0, 4, 3, 4, 7, // left
  ];
  Mesh { name: "Cube".into(), positions, normals: Vec::new(), indices }
}

fn volume(m: &Mesh) -> f32 {
  m.indices
    .chunks_exact(3)
    .map(|t| {
      let (a, b, c) = (m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]);
      (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]))
        / 6.0
    })
    .sum()
}

fn same_shape(a: &Mesh, b: &Mesh) {
  assert_eq!(a.triangle_count(), b.triangle_count());
  assert_eq!(a.positions.len(), b.positions.len());
  assert!((volume(a) - volume(b)).abs() < 1e-3, "{} vs {}", volume(a), volume(b));
}

#[test]
fn stl_binary_and_ascii_round_trip() {
  let c = cube(10.0);
  let bin = write_stl(std::slice::from_ref(&c));
  assert_eq!(bin.len(), 84 + 12 * 50);
  let back = read_stl(&bin).unwrap();
  same_shape(&c, &back);
  assert!((volume(&back) - 1000.0).abs() < 1e-3);
  let ascii = write_stl_ascii(std::slice::from_ref(&c));
  assert!(ascii.starts_with("solid Cube"));
  same_shape(&c, &read_stl(ascii.as_bytes()).unwrap());
}

#[test]
fn binary_stl_whose_header_says_solid() {
  let mut bin = write_stl(&[cube(2.0)]);
  bin[..5].copy_from_slice(b"solid");
  assert_eq!(read_stl(&bin).unwrap().triangle_count(), 12);
}

#[test]
fn obj_round_trip_with_objects_quads_and_negative_indices() {
  let text = write_obj(&[cube(1.0), Mesh { name: "Second".into(), ..cube(3.0) }]);
  let back = read_obj(text.as_bytes()).unwrap();
  assert_eq!(back.len(), 2);
  assert_eq!(back[1].name, "Second");
  assert!((volume(&back[1]) - 27.0).abs() < 1e-3);
  // a quad written with relative indices and texture/normal refs
  let quad = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf -4/1/1 -3/2/1 -2/3/1 -1/4/1\n";
  let q = read_obj(quad.as_bytes()).unwrap();
  assert_eq!(q[0].triangle_count(), 2);
  assert!(read_obj(b"v 0 0 0\nf 1 2 3\n").is_err());
}

#[test]
fn ply_binary_and_ascii() {
  let c = cube(4.0);
  let bin = write_ply(std::slice::from_ref(&c));
  same_shape(&c, &read_ply(&bin).unwrap());
  let ascii = "ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0 255\n1 0 0 255\n1 1 0 255\n0 1 0 255\n4 0 1 2 3\n";
  let m = read_ply(ascii.as_bytes()).unwrap();
  assert_eq!(m.triangle_count(), 2);
  assert_eq!(m.positions[2], [1.0, 1.0, 0.0]);
}

#[test]
fn threemf_round_trip_and_units() {
  let c = cube(5.0);
  let bytes = write_3mf(std::slice::from_ref(&c), Some(b"\x89PNG fake")).unwrap();
  let back = read_3mf(&bytes).unwrap();
  assert_eq!(back.len(), 1);
  same_shape(&c, &back[0]);
  assert_eq!(back[0].name, "Cube");

  // a hand-written package in inches with a component and a translated build item
  let model = r#"<?xml version="1.0"?>
<model unit="inch" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <resources>
  <object id="1" type="model"><mesh><vertices>
   <vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/><vertex x="0" y="0" z="1"/>
  </vertices><triangles>
   <triangle v1="0" v2="2" v3="1"/><triangle v1="0" v2="1" v3="3"/><triangle v1="0" v2="3" v3="2"/><triangle v1="1" v2="2" v3="3"/>
  </triangles></mesh></object>
  <object id="2" name="Assembly" type="model"><components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 2 0 0"/></components></object>
 </resources>
 <build><item objectid="2" transform="1 0 0 0 1 0 0 0 1 0 0 1"/></build>
</model>"#;
  let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
  let o = zip::write::SimpleFileOptions::default();
  w.start_file("3D/3dmodel.model", o).unwrap();
  w.write_all(model.as_bytes()).unwrap();
  let bytes = w.finish().unwrap().into_inner();
  let m = read_3mf(&bytes).unwrap();
  assert_eq!(m[0].name, "Assembly");
  let (lo, hi) = m[0].bounds().unwrap();
  assert!((lo[0] - 50.8).abs() < 1e-4 && (lo[2] - 25.4).abs() < 1e-4, "{lo:?}");
  assert!((hi[0] - 76.2).abs() < 1e-4 && (hi[2] - 50.8).abs() < 1e-4, "{hi:?}");
}

#[test]
fn glb_is_well_formed() {
  let glb = write_glb(&[cube(1.0)]).unwrap();
  assert_eq!(&glb[..4], b"glTF");
  let total = u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize;
  assert_eq!(total, glb.len());
  let jlen = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
  let doc: serde_json::Value = serde_json::from_slice(&glb[20..20 + jlen]).unwrap();
  assert_eq!(doc["accessors"][0]["count"], 8);
  assert_eq!(doc["accessors"][1]["count"], 36);
  let blen = u32::from_le_bytes(glb[20 + jlen..24 + jlen].try_into().unwrap()) as usize;
  assert_eq!(doc["buffers"][0]["byteLength"].as_u64().unwrap() as usize, blen);
}

#[test]
fn usdz_first_entry_is_aligned_usd() {
  let bytes = write_usdz(&[cube(1.0)]).unwrap();
  let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
  let f = z.by_index(0).unwrap();
  assert_eq!(f.name().unwrap(), "model.usda");
  assert_eq!(f.compression(), zip::CompressionMethod::Stored);
  assert_eq!(f.data_start().unwrap() % 64, 0);
  drop(f);
  let text = crate::usdz::usda(&[cube(1.0)]);
  assert!(text.contains("def Mesh \"Cube\"") && text.contains("metersPerUnit = 0.001"));
}

#[test]
fn welding_merges_duplicate_vertices() {
  let c = cube(1.0);
  // explode: every triangle gets its own vertices
  let mut e = Mesh::default();
  for &i in &c.indices {
    e.positions.push(c.positions[i as usize]);
    e.indices.push(e.indices.len() as u32);
  }
  let w = e.welded();
  assert_eq!(w.positions.len(), 8);
  assert_eq!(w.triangle_count(), 12);
}

#[test]
fn formats_table() {
  assert_eq!(format_for_ext("STP").unwrap().id, "step");
  assert_eq!(format_for_ext(".f3z").unwrap().id, "f3d");
  assert!(format_for_ext("f3d").unwrap().import);
  assert!(!format_for_ext("f3d").unwrap().export);
  assert!(format_for_ext("xyz").is_none());
  for f in FORMATS {
    assert!(f.import || f.export, "{}", f.id);
  }
}

#[test]
fn dxf_round_trip() {
  let ents = vec![
    (Entity::Line { a: [0.0, 0.0], b: [10.0, 0.0] }, false),
    (Entity::Circle { center: [5.0, 5.0], radius: 2.0 }, false),
    (Entity::Arc { center: [0.0, 0.0], radius: 10.0, start: 0.0, end: std::f64::consts::FRAC_PI_2 }, true),
    (Entity::Point { at: [1.0, 2.0] }, false),
  ];
  let text = write_dxf(&ents);
  let d = read_dxf(text.as_bytes()).unwrap();
  assert_eq!(d.entities.len(), 4);
  assert!(d.skipped.is_empty());
  match &d.entities[2] {
    &(Entity::Arc { radius, start, end, .. }, true) => {
      assert!((radius - 10.0).abs() < 1e-9 && start.abs() < 1e-9 && (end - std::f64::consts::FRAC_PI_2).abs() < 1e-9)
    }
    e => panic!("{e:?}"),
  }
}

#[test]
fn dxf_polylines_units_and_mirrored_entities() {
  // a closed LWPOLYLINE in inches: square with one bulged (semicircle) side, plus a SPLINE
  let text = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n1\n20\n0\n42\n1\n10\n1\n20\n1\n10\n0\n20\n1\n\
0\nSPLINE\n8\n0\n\
0\nARC\n8\n0\n10\n2\n20\n0\n40\n1\n50\n0\n51\n90\n230\n-1\n\
0\nENDSEC\n0\nEOF\n";
  let d = read_dxf(text.as_bytes()).unwrap();
  assert_eq!(d.skipped.get("SPLINE"), Some(&1));
  let lines = d.entities.iter().filter(|e| matches!(e.0, Entity::Line { .. })).count();
  assert_eq!(lines, 3);
  let arcs: Vec<_> = d
    .entities
    .iter()
    .filter_map(|e| {
      if let Entity::Arc { center, radius, start, end } = e.0 { Some((center, radius, start, end)) } else { None }
    })
    .collect();
  assert_eq!(arcs.len(), 2);
  // the bulge arc: from (25.4, 0) to (25.4, 25.4), centre (25.4, 12.7), bulging outward (+x)
  let (c, r, s, e) = arcs[0];
  assert!((c[0] - 25.4).abs() < 1e-9 && (c[1] - 12.7).abs() < 1e-9 && (r - 12.7).abs() < 1e-9, "{c:?} {r}");
  let mid = Entity::arc_point(c, r, (s + e) / 2.0);
  assert!(mid[0] > 25.4 + 12.0, "{mid:?}");
  // the mirrored ARC: centre x flips, 0..90 deg becomes 90..180 deg
  let (c, _, s, e) = arcs[1];
  assert!((c[0] + 50.8).abs() < 1e-9);
  assert!((s - std::f64::consts::FRAC_PI_2).abs() < 1e-9 && (e - std::f64::consts::PI).abs() < 1e-9);
}

#[test]
fn negative_bulge_runs_clockwise() {
  let Some(Entity::Arc { center, start, end, .. }) = bulge_arc([0.0, 0.0], [2.0, 0.0], -1.0) else { panic!() };
  assert!(center[0] == 1.0 && center[1].abs() < 1e-12);
  // clockwise from a to b: the arc bulges to the left of the travel direction (over the top)
  let mid = Entity::arc_point(center, 1.0, (start + if end < start { end + std::f64::consts::TAU } else { end }) / 2.0);
  assert!(mid[1] > 0.99, "{mid:?}");
}

#[test]
fn foxz_round_trip_with_history() {
  let a = Archive {
    name: "Bracket".into(),
    design: "{\"name\": \"Bracket\"}".into(),
    thumbnail: Some(vec![1, 2, 3]),
    history: vec![
      ArchivedVersion {
        info: serde_json::json!({"number": 1, "description": "first"}),
        design: "{}".into(),
        thumbnail: None,
      },
      ArchivedVersion { info: serde_json::json!({"number": 2}), design: "{\"v\": 2}".into(), thumbnail: Some(vec![9]) },
    ],
  };
  let bytes = write_foxz(&a).unwrap();
  assert_eq!(read_foxz(&bytes).unwrap(), a);
  assert!(read_foxz(&write_3mf(&[cube(1.0)], None).unwrap()).is_err());
}

/// Designs saved before the rename to FissionOxide (`.clf`, `.clfz`) still open.
#[test]
fn legacy_clf_files_still_read() {
  assert_eq!(format_for_ext("clf").unwrap().id, "fox");
  assert_eq!(format_for_ext("CLFZ").unwrap().id, "foxz");
  let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
  let opts = zip::write::SimpleFileOptions::default();
  w.start_file("design.clf", opts).unwrap();
  w.write_all(b"{}").unwrap();
  w.start_file("manifest.json", opts).unwrap();
  w.write_all(br#"{"format": "clfz", "format_version": 1, "name": "Old", "design": "design.clf"}"#).unwrap();
  let a = read_foxz(&w.finish().unwrap().into_inner()).unwrap();
  assert_eq!((a.name.as_str(), a.design.as_str()), ("Old", "{}"));
}

#[test]
fn fusion_archive_inspection() {
  let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
  let o = zip::write::SimpleFileOptions::default();
  for (name, data) in [
    ("Manifest.dat", &b"x"[..]),
    ("FusionAssetName[Active]/Breps.BlobParts/BRep.1.smb", b"ShapeManager"),
    ("FusionAssetName[Active]/Breps.BlobParts/BRep.2.smb", b"ShapeManager"),
    ("FusionAssetName[Active]/Previews/small.png", b"\x89PNG"),
    ("FusionAssetName[Active]/Previews/large.png", b"\x89PNG large"),
    ("Attachments/part.stl", &write_stl(&[cube(1.0)])),
  ] {
    w.start_file(name, o).unwrap();
    w.write_all(data).unwrap();
  }
  let bytes = w.finish().unwrap().into_inner();
  let f = inspect_fusion(&bytes).unwrap();
  assert_eq!(f.brep_blobs, 2);
  assert_eq!(f.thumbnail.as_deref(), Some(&b"\x89PNG large"[..]));
  assert_eq!(f.embedded.len(), 1);
  assert!(f.designs.contains(&"FusionAssetName".to_string()), "{:?}", f.designs);
  assert!(inspect_fusion(b"not a zip").is_err());
}

// --- SVG ---------------------------------------------------------------------------------------

/// A 100 x 100 mm page with one user unit per millimetre: (x, y) lands on (x, 100 - y).
fn svg_doc(body: &str) -> Drawing {
  let s = format!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">{body}</svg>"#
  );
  read_svg(s.as_bytes()).unwrap()
}

fn near(a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
  (a[0] - b[0]).hypot(a[1] - b[1]) < tol
}

fn ents(d: &Drawing) -> Vec<Entity> {
  d.entities.iter().map(|e| e.0.clone()).collect()
}

fn lines(d: &Drawing) -> Vec<([f64; 2], [f64; 2])> {
  d.entities.iter().filter_map(|e| if let Entity::Line { a, b } = e.0 { Some((a, b)) } else { None }).collect()
}

fn beziers(d: &Drawing) -> Vec<[[f64; 2]; 4]> {
  d.entities.iter().filter_map(|e| if let Entity::Bezier { p } = e.0 { Some(p) } else { None }).collect()
}

fn arcs(d: &Drawing) -> Vec<([f64; 2], f64, f64, f64)> {
  d.entities
    .iter()
    .filter_map(|e| {
      if let Entity::Arc { center, radius, start, end } = e.0 { Some((center, radius, start, end)) } else { None }
    })
    .collect()
}

/// Counter-clockwise sweep of an arc, in (0, 2 pi].
fn sweep(start: f64, end: f64) -> f64 {
  let s = (end - start).rem_euclid(std::f64::consts::TAU);
  if s == 0.0 { std::f64::consts::TAU } else { s }
}

/// Start and end points of a curve entity.
fn ends_of(e: &Entity) -> ([f64; 2], [f64; 2]) {
  match *e {
    Entity::Line { a, b } => (a, b),
    Entity::Bezier { p } => (p[0], p[3]),
    Entity::Arc { center, radius, start, end } => {
      (Entity::arc_point(center, radius, start), Entity::arc_point(center, radius, end))
    }
    Entity::Circle { center, .. } | Entity::Point { at: center } => (center, center),
    Entity::Spline(_) | Entity::Ellipse { .. } => {
      let bz = e.beziers();
      (bz.first().map_or([0.0; 2], |b| b[0]), bz.last().map_or([0.0; 2], |b| b[3]))
    }
  }
}

#[test]
fn svg_line_polyline_polygon() {
  let d = svg_doc(
    r#"<line x1="10" y1="10" x2="20" y2="30"/>
       <polyline points="0,0 10,0 10,10"/>
       <polygon points="50 50, 60 50 55 60"/>"#,
  );
  let l = lines(&d);
  assert_eq!(l.len(), 1 + 2 + 3);
  assert!(near(l[0].0, [10.0, 90.0], 1e-12) && near(l[0].1, [20.0, 70.0], 1e-12), "{l:?}");
  assert!(near(l[2].1, [10.0, 90.0], 1e-12));
  // the polygon closes back to its first point exactly
  assert_eq!(l[5].1, l[3].0);
  assert!(near(l[3].0, [50.0, 50.0], 1e-12));
}

#[test]
fn svg_rect_circle_ellipse() {
  let d = svg_doc(r#"<rect x="10" y="20" width="30" height="40"/><circle cx="50" cy="50" r="5"/>"#);
  let l = lines(&d);
  assert_eq!(l.len(), 4);
  let corners: Vec<_> = l.iter().map(|l| l.0).collect();
  for c in [[10.0, 80.0], [40.0, 80.0], [40.0, 40.0], [10.0, 40.0]] {
    assert!(corners.iter().any(|p| near(*p, c, 1e-12)), "{c:?} {corners:?}");
  }
  assert_eq!(l[3].1, l[0].0);
  assert!(
    matches!(d.entities[4].0, Entity::Circle { center, radius } if near(center, [50.0, 50.0], 1e-12) && (radius - 5.0).abs() < 1e-12)
  );

  // an ellipse becomes four closed cubic pieces on the ellipse
  let d = svg_doc(r#"<ellipse cx="50" cy="50" rx="20" ry="10"/>"#);
  let b = beziers(&d);
  assert_eq!(b.len(), 4);
  assert_eq!(b[3][3], b[0][0]);
  for p in &b {
    for t in [0.0, 0.25, 0.5, 0.75] {
      let q = Entity::bezier_point(p, t);
      let r = ((q[0] - 50.0) / 20.0).hypot((q[1] - 50.0) / 10.0);
      assert!((r - 1.0).abs() < 1e-3, "{q:?}");
    }
  }
  // a circle with rx == ry in an ellipse element is a circle
  let d = svg_doc(r#"<ellipse cx="50" cy="50" rx="7" ry="7"/>"#);
  assert!(matches!(d.entities[0].0, Entity::Circle { radius, .. } if (radius - 7.0).abs() < 1e-12));
}

#[test]
fn svg_rounded_rect_corners_are_convex_arcs() {
  let d = svg_doc(r#"<rect x="10" y="10" width="40" height="20" rx="5"/>"#);
  assert_eq!(lines(&d).len(), 4);
  let a = arcs(&d);
  assert_eq!(a.len(), 4);
  // rect spans x 10..50, y (CAD) 70..90
  let corners = [[10.0, 70.0], [50.0, 70.0], [50.0, 90.0], [10.0, 90.0]];
  for (c, r, s, e) in a {
    assert!((r - 5.0).abs() < 1e-12);
    assert!((sweep(s, e) - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "{s} {e}");
    let mid = Entity::arc_point(c, r, s + sweep(s, e) / 2.0);
    let k = corners.iter().map(|k| (k[0] - mid[0]).hypot(k[1] - mid[1])).fold(f64::MAX, f64::min);
    assert!((k - 5.0 * (2f64.sqrt() - 1.0)).abs() < 1e-9, "{mid:?}");
  }
  // ry alone sets rx too
  let d = svg_doc(r#"<rect x="0" y="0" width="10" height="10" ry="2"/>"#);
  assert!(arcs(&d).iter().all(|a| (a.1 - 2.0).abs() < 1e-12));
}

#[test]
fn svg_relative_and_absolute_commands_agree() {
  let forms = [
    "M10 10 L20 10 L20 20 L10 20 Z",
    "m10 10 l10 0 0 10 -10 0 z",
    "M10,10H20V20H10z",
    "m10,10h10v10h-10Z",
    "M10 10 20 10 20 20 10 20z",
  ];
  let first = ents(&svg_doc(&format!(r#"<path d="{}"/>"#, forms[0])));
  assert_eq!(first.len(), 4);
  for f in &forms[1..] {
    let d = svg_doc(&format!(r#"<path d="{f}"/>"#));
    assert_eq!(ents(&d), first, "{f}");
  }
  // compact numbers: "1.5.5" is 1.5 0.5, "-" separates, exponents
  let d = svg_doc(r#"<path d="M1.5.5L1e1-2e-0l-.5.5"/>"#);
  let l = lines(&d);
  assert!(near(l[0].0, [1.5, 99.5], 1e-12) && near(l[0].1, [10.0, 102.0], 1e-12), "{l:?}");
  assert!(near(l[1].1, [9.5, 101.5], 1e-12));
  // after z, drawing continues from the subpath start
  let d = svg_doc(r#"<path d="M0 0 l10 0 l0 10 z l 5 5"/>"#);
  let l = lines(&d);
  assert_eq!(l.len(), 4);
  assert!(near(l[3].0, [0.0, 100.0], 1e-12) && near(l[3].1, [5.0, 95.0], 1e-12));
}

#[test]
fn svg_cubic_smooth_and_quadratic() {
  let d = svg_doc(r#"<path d="M0 0 C 0 10 10 10 10 0 S 20 -10 20 0 s 10 10 10 0"/>"#);
  let b = beziers(&d);
  assert_eq!(b.len(), 3);
  // S reflects the previous second control point (10, 10) about (10, 0): (10, -10) -> CAD (10, 110)
  assert!(near(b[1][1], [10.0, 110.0], 1e-12), "{:?}", b[1]);
  assert!(near(b[2][1], [20.0, 90.0], 1e-12) && near(b[2][3], [30.0, 100.0], 1e-12), "{:?}", b[2]);
  // S with no cubic before uses the current point
  let b = beziers(&svg_doc(r#"<path d="M0 0 L5 0 S 10 10 15 0"/>"#));
  assert!(near(b[0][1], [5.0, 100.0], 1e-12));

  // quadratics become exact cubics; T reflects the control point
  let d = svg_doc(r#"<path d="M0 0 Q 10 20 20 0 T 40 0 q 10 -20 20 0 t 20 0"/>"#);
  let b = beziers(&d);
  assert_eq!(b.len(), 4);
  let quad = |p0: [f64; 2], c: [f64; 2], p1: [f64; 2], t: f64| {
    let u = 1.0 - t;
    [0, 1].map(|k| u * u * p0[k] + 2.0 * u * t * c[k] + t * t * p1[k])
  };
  let user = |p: [f64; 2]| [p[0], 100.0 - p[1]];
  let expect = [
    ([0.0, 0.0], [10.0, 20.0], [20.0, 0.0]),
    ([20.0, 0.0], [30.0, -20.0], [40.0, 0.0]),
    ([40.0, 0.0], [50.0, -20.0], [60.0, 0.0]),
    ([60.0, 0.0], [70.0, 20.0], [80.0, 0.0]),
  ];
  for (p, (a, c, z)) in b.iter().zip(expect) {
    for t in [0.0, 0.2, 0.5, 0.9, 1.0] {
      assert!(near(Entity::bezier_point(p, t), user(quad(a, c, z, t)), 1e-12), "{p:?} {t}");
    }
  }
}

/// A page centred on the origin: user (x, y) lands on (x, -y).
fn centred(body: &str) -> Drawing {
  let s = format!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="-50 -50 100 100">{body}</svg>"#
  );
  read_svg(s.as_bytes()).unwrap()
}

#[test]
fn svg_circular_arcs_become_arcs_with_the_right_direction() {
  use std::f64::consts::{FRAC_PI_2, PI};
  // screen-clockwise quarter around (0, 0): in CAD (y up) it runs clockwise from (10, 0) to
  // (0, -10), so the counter-clockwise arc goes from -90 to 0 degrees
  let a = arcs(&centred(r#"<path d="M 10 0 A 10 10 0 0 1 0 10"/>"#));
  assert_eq!(a.len(), 1);
  let (c, r, s, e) = a[0];
  assert!(near(c, [0.0, 0.0], 1e-12) && (r - 10.0).abs() < 1e-12);
  assert!((s + FRAC_PI_2).abs() < 1e-12 && e.abs() < 1e-12, "{s} {e}");

  // sweep 0: the other centre (10, 10) -> CAD (10, -10); in CAD the arc runs counter-clockwise
  // from (10, 0) (90 degrees about the centre) to (0, -10) (180 degrees)
  let (c, _, s, e) = arcs(&centred(r#"<path d="M 10 0 A 10 10 0 0 0 0 10"/>"#))[0];
  assert!(near(c, [10.0, -10.0], 1e-12));
  assert!((s - FRAC_PI_2).abs() < 1e-12 && (e - PI).abs() < 1e-12, "{s} {e}");

  // large arc: three quarters
  let (c, _, s, e) = arcs(&centred(r#"<path d="M 10 0 a 10 10 0 1 1 -10 10"/>"#))[0];
  assert!(near(c, [10.0, -10.0], 1e-12));
  assert!((sweep(s, e) - 1.5 * PI).abs() < 1e-12);

  // radii too small grow to fit: a semicircle of radius 5
  let (c, r, s, e) = arcs(&centred(r#"<path d="M0 0 A 1 1 0 0 1 10 0"/>"#))[0];
  assert!(near(c, [5.0, 0.0], 1e-12) && (r - 5.0).abs() < 1e-12 && (sweep(s, e) - PI).abs() < 1e-12);

  // a similarity transform keeps it circular; a mirror flips the direction back
  let (c, r, ..) = arcs(&centred(r#"<path transform="rotate(30) scale(2)" d="M 10 0 A 10 10 0 0 1 0 10"/>"#))[0];
  assert!(near(c, [0.0, 0.0], 1e-12) && (r - 20.0).abs() < 1e-9);
  let (_, _, s, e) = arcs(&centred(r#"<path transform="scale(1,-1)" d="M 10 0 A 10 10 0 0 1 0 10"/>"#))[0];
  assert!(s.abs() < 1e-12 && (e - FRAC_PI_2).abs() < 1e-12, "{s} {e}");

  // two half arcs make a full circle's worth of arcs that meet the subpath ends
  let d = centred(r#"<path d="M -5 0 A 5 5 0 0 1 5 0 A 5 5 0 0 1 -5 0 Z"/>"#);
  assert_eq!(arcs(&d).len(), 2);
  assert_eq!(d.entities.len(), 2);
}

#[test]
fn svg_elliptical_arcs_become_beziers() {
  use std::f64::consts::PI;
  // a half ellipse 20 x 10, rotated 30 degrees: two pieces, end points exact
  let d = centred(r#"<path d="M 0 0 A 20 10 30 0 1 34.641016151377546 20"/>"#);
  let b = beziers(&d);
  assert!(arcs(&d).is_empty());
  assert_eq!(b.len(), 2, "{b:?}");
  assert_eq!(b[0][0], [0.0, 0.0]);
  assert!(near(b[1][3], [20.0 * 3f64.sqrt(), -20.0], 1e-12));
  assert_eq!(b[0][3], b[1][0]);
  // every point sits on the ellipse (in CAD: centre (17.32, -10), axis at -30 degrees)
  let (cx, cy, phi) = (10.0 * 3f64.sqrt(), -10.0, -PI / 6.0);
  for p in &b {
    for t in [0.0, 0.3, 0.5, 0.8] {
      let q = Entity::bezier_point(p, t);
      let (dx, dy) = (q[0] - cx, q[1] - cy);
      let (u, v) = (dx * phi.cos() + dy * phi.sin(), -dx * phi.sin() + dy * phi.cos());
      // a 90 degree cubic is within 3e-4 of the true curve
      assert!(((u / 20.0).hypot(v / 10.0) - 1.0).abs() < 5e-4, "{q:?}");
    }
  }
  // a circular arc under a non-uniform scale or a skew is no longer circular
  for t in ["scale(2,1)", "skewX(20)"] {
    let d = centred(&format!(r#"<path transform="{t}" d="M 10 0 A 10 10 0 0 1 0 10"/>"#));
    assert!(arcs(&d).is_empty() && beziers(&d).len() == 1, "{t}");
  }
  // a big elliptical arc is split into pieces of at most 90 degrees
  let d = centred(r#"<path d="M 20 0 A 20 10 0 1 1 0 10"/>"#);
  assert_eq!(beziers(&d).len(), 3);
  // zero radius is a straight line
  let d = centred(r#"<path d="M 0 0 A 0 10 0 0 1 10 0"/>"#);
  assert_eq!(lines(&d).len(), 1);
}

#[test]
fn svg_transforms_and_nested_groups() {
  let one = |body: &str| -> Vec<([f64; 2], [f64; 2])> { lines(&svg_doc(body)) };
  let cad = |x: f64, y: f64| [x, 100.0 - y];
  // nested groups compose outside-in
  let l = one(r#"<g transform="translate(10,20)"><g transform="scale(2)"><line x1="0" y1="0" x2="5" y2="0"/></g></g>"#);
  assert!(near(l[0].0, cad(10.0, 20.0), 1e-12) && near(l[0].1, cad(20.0, 20.0), 1e-12), "{l:?}");
  // rotate about a centre
  let l = one(r#"<line transform="rotate(90 10 10)" x1="10" y1="10" x2="20" y2="10"/>"#);
  assert!(near(l[0].1, cad(10.0, 20.0), 1e-12), "{l:?}");
  // a list applies right to left: rotate first, then translate
  let l = one(r#"<line transform="translate(10) rotate(90)" x1="0" y1="0" x2="1" y2="0"/>"#);
  assert!(near(l[0].0, cad(10.0, 0.0), 1e-12) && near(l[0].1, cad(10.0, 1.0), 1e-12), "{l:?}");
  // matrix, skews, comma-separated lists
  let l = one(r#"<line transform="matrix(1,0,0,1,5,6)" x1="0" y1="0" x2="1" y2="0"/>"#);
  assert!(near(l[0].0, cad(5.0, 6.0), 1e-12));
  let l = one(r#"<line transform="skewX(45)" x1="0" y1="10" x2="0" y2="20"/>"#);
  assert!(near(l[0].0, cad(10.0, 10.0), 1e-12) && near(l[0].1, cad(20.0, 20.0), 1e-12), "{l:?}");
  let l = one(r#"<line transform="skewY(45)" x1="10" y1="0" x2="20" y2="0"/>"#);
  assert!(near(l[0].0, cad(10.0, 10.0), 1e-12), "{l:?}");
  let l = one(r#"<g transform="scale(2, 3),translate(1 1)"><line x1="0" y1="0" x2="1" y2="0"/></g>"#);
  assert!(near(l[0].0, cad(2.0, 3.0), 1e-12) && near(l[0].1, cad(4.0, 3.0), 1e-12), "{l:?}");
  // transforms on a group and its shape both apply
  let l = one(r#"<g transform="translate(5,0)"><path transform="translate(0,5)" d="M0 0 h1"/></g>"#);
  assert!(near(l[0].0, cad(5.0, 5.0), 1e-12));
}

#[test]
fn svg_viewbox_and_units() {
  // 2 x 1 inches over 200 x 100 user units: 0.254 mm per unit
  let s = r#"<svg xmlns="http://www.w3.org/2000/svg" width="2in" height="1in" viewBox="0 0 200 100">
    <line x1="0" y1="0" x2="200" y2="100"/></svg>"#;
  let l = lines(&read_svg(s.as_bytes()).unwrap());
  assert!(near(l[0].0, [0.0, 25.4], 1e-12) && near(l[0].1, [50.8, 0.0], 1e-12), "{l:?}");
  // millimetres, half a millimetre per unit
  let s = r#"<svg xmlns="http://www.w3.org/2000/svg" width="50mm" height="25mm" viewBox="0 0 100 50">
    <circle cx="50" cy="10" r="4"/></svg>"#;
  let d = read_svg(s.as_bytes()).unwrap();
  assert!(
    matches!(d.entities[0].0, Entity::Circle { center, radius } if near(center, [25.0, 20.0], 1e-12) && (radius - 2.0).abs() < 1e-12)
  );
  // no viewBox: px at 96 dpi
  let s =
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="96px" height="96"><line x1="0" y1="0" x2="96" y2="0"/></svg>"#;
  let l = lines(&read_svg(s.as_bytes()).unwrap());
  assert!(near(l[0].0, [0.0, 25.4], 1e-12) && near(l[0].1, [25.4, 25.4], 1e-12), "{l:?}");
  // points and centimetres, and no width at all (px)
  let s = r#"<svg xmlns="http://www.w3.org/2000/svg" width="72pt" viewBox="0 0 72 72"><line x1="0" y1="72" x2="72" y2="72"/></svg>"#;
  let l = lines(&read_svg(s.as_bytes()).unwrap());
  assert!(near(l[0].1, [25.4, 0.0], 1e-12), "{l:?}");
  let s = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10cm" height="10cm" viewBox="0 0 10 10"><line x1="0" y1="10" x2="1" y2="10"/></svg>"#;
  let l = lines(&read_svg(s.as_bytes()).unwrap());
  assert!(near(l[0].1, [10.0, 0.0], 1e-12), "{l:?}");
  let s = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><line x1="0" y1="96" x2="96" y2="96"/></svg>"#;
  let l = lines(&read_svg(s.as_bytes()).unwrap());
  assert!(near(l[0].1, [25.4, 0.0], 1e-12), "{l:?}");
}

#[test]
fn svg_px_units_ignore_the_page_as_fusion_does() {
  // Fusion's Insert SVG: a user unit is a 96-dpi pixel whatever width/height/viewBox say, and the
  // SVG's 0,0 is the origin with y negated
  let s = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="30mm" viewBox="0 0 40 30">
    <line x1="0" y1="0" x2="96" y2="48"/><circle cx="10" cy="10" r="5"/></svg>"#;
  let d = read_svg_as(s.as_bytes(), SvgUnits::Px).unwrap();
  let l = lines(&d);
  assert!(near(l[0].0, [0.0, 0.0], 1e-12) && near(l[0].1, [25.4, -12.7], 1e-12), "{l:?}");
  let px = 25.4 / 96.0;
  assert!(d.entities.iter().any(|(e, _)| matches!(e, Entity::Circle { center, radius }
    if near(*center, [10.0 * px, -10.0 * px], 1e-12) && (radius - 5.0 * px).abs() < 1e-12)));
  // the document reading sizes the same file in millimetres, mirrored about the viewBox's middle
  let l = lines(&read_svg(s.as_bytes()).unwrap());
  assert!(near(l[0].0, [0.0, 30.0], 1e-12), "{l:?}");
}

#[test]
fn svg_closed_subpaths_end_at_their_start() {
  // rounding in relative coordinates leaves a sliver: it folds into the segment before
  let d = svg_doc(r#"<path d="M0 0 l10 0 l0 10 l-9.9998 -10.0002 z"/>"#);
  let l = lines(&d);
  assert_eq!(l.len(), 3, "{l:?}");
  assert_eq!(l[2].1, l[0].0);
  let d = svg_doc(r#"<path d="M0 0 L10 0 C 10 10 0 10 0.0003 0 Z"/>"#);
  assert_eq!(d.entities.len(), 2);
  assert_eq!(beziers(&d)[0][3], lines(&d)[0].0);
  // a real gap gets a closing line
  let d = svg_doc(r#"<path d="M0 0 L10 0 L10 10 Z"/>"#);
  let l = lines(&d);
  assert_eq!(l.len(), 3);
  assert_eq!(l[2].1, l[0].0);
  // an open path stays open
  assert_eq!(lines(&svg_doc(r#"<path d="M0 0 L10 0 L10 10"/>"#)).len(), 2);
  // a path data error keeps what came before
  let d = svg_doc(r#"<path d="M0 0 L10 0 L10 10 X 5 5"/>"#);
  assert_eq!(lines(&d).len(), 2);
  assert_eq!(d.skipped.len(), 1);
}

#[test]
fn svg_skips_non_drawing_content() {
  let d = svg_doc(
    r##"<!-- a comment -->
    <title>t</title><desc>d</desc>
    <style><![CDATA[ .a { stroke: red } ]]></style>
    <defs><path id="p" d="M0 0 L1 1"/><clipPath id="c"><rect width="5" height="5"/></clipPath></defs>
    <mask id="m"><circle r="3"/></mask>
    <symbol id="s"><line x2="5"/></symbol>
    <use href="#p"/><image href="x.png" width="5" height="5"/>
    <text x="0" y="0">hi<tspan>there</tspan></text>
    <path style="display:none" d="M0 0 L5 5"/>
    <line x1="0" y1="0" x2="1" y2="0" style="stroke:#000;fill:none;stroke-width:0.1"/>"##,
  );
  assert_eq!(d.entities.len(), 1);
  for k in ["use", "image", "text", "hidden element"] {
    assert_eq!(d.skipped.get(k), Some(&1), "{k}: {:?}", d.skipped);
  }
  // prefixed SVG elements are SVG too
  let s = r#"<svg:svg xmlns:svg="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
    <svg:g><svg:line x1="0" y1="0" x2="1" y2="0"/></svg:g></svg:svg>"#;
  assert_eq!(read_svg(s.as_bytes()).unwrap().entities.len(), 1);
}

#[test]
fn svg_inkscape_document() {
  let s = r##"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!-- Created with Inkscape (http://www.inkscape.org/) -->
<svg
   width="210mm"
   height="297mm"
   viewBox="0 0 210 297"
   version="1.1"
   id="svg1"
   inkscape:version="1.3 (0e150ed6c4, 2023-07-21)"
   sodipodi:docname="drawing.svg"
   xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
   xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd"
   xmlns="http://www.w3.org/2000/svg"
   xmlns:svg="http://www.w3.org/2000/svg">
  <sodipodi:namedview
     id="namedview1"
     pagecolor="#ffffff"
     inkscape:document-units="mm" />
  <defs
     id="defs1" />
  <g
     inkscape:label="Layer 1"
     inkscape:groupmode="layer"
     id="layer1"
     transform="translate(-10,5)">
    <path
       style="fill:none;stroke:#000000;stroke-width:0.264583"
       d="m 30,40 h 50 v 30 h -50 z"
       id="rect1"
       sodipodi:nodetypes="ccccc" />
    <circle
       style="fill:none;stroke:#000000;stroke-width:0.264583"
       id="path2"
       cx="60"
       cy="100"
       r="10" />
    <path
       sodipodi:type="arc"
       sodipodi:cx="0"
       sodipodi:rx="5"
       d="M 105,150 A 5,5 0 0 1 100,155 5,5 0 0 1 95,150"
       sodipodi:open="true" />
    <text xml:space="preserve" x="10" y="10" id="text1"><tspan id="tspan1" x="10" y="10">Hello</tspan></text>
  </g>
  <g inkscape:label="Construction" inkscape:groupmode="layer" id="layer2">
    <path d="M 0,0 L 10,10" />
  </g>
  <g inkscape:label="Hidden" inkscape:groupmode="layer" id="layer3" style="display:none">
    <path d="M 0,0 L 10,10" />
  </g>
</svg>
"##;
  let d = read_svg(s.as_bytes()).unwrap();
  let l = lines(&d);
  assert_eq!(l.len(), 4 + 1);
  // the rectangle's first corner: (30, 40) + (-10, 5) on an A4 page
  assert!(near(l[0].0, [20.0, 252.0], 1e-9), "{l:?}");
  assert_eq!(l[3].1, l[0].0);
  assert!(d.entities.iter().any(|e| matches!(e.0, Entity::Circle { center, .. } if near(center, [50.0, 192.0], 1e-9))));
  assert_eq!(arcs(&d).len(), 2);
  assert_eq!(d.entities.iter().filter(|e| e.1).count(), 1, "the construction layer");
  assert_eq!(d.skipped.get("text"), Some(&1));
  assert_eq!(d.skipped.get("hidden element"), Some(&1));
}

#[test]
fn svg_illustrator_document() {
  let s = "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<!-- Generator: Adobe Illustrator 27.0.0, SVG Export Plug-In . SVG Version: 6.00 Build 0)  -->
<svg version=\"1.1\" id=\"Layer_1\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" x=\"0px\" y=\"0px\"
\t viewBox=\"0 0 200 100\" style=\"enable-background:new 0 0 200 100;\" xml:space=\"preserve\">
<style type=\"text/css\">
\t.st0{fill:none;stroke:#000000;stroke-miterlimit:10;}
</style>
<g>
\t<path class=\"st0\" d=\"M10.5,20.25c0,5.52,4.48,10,10,10s10-4.48,10-10s-4.48-10-10-10S10.5,14.73,10.5,20.25z\"/>
\t<polygon class=\"st0\" points=\"50,10 70,10 60,30 \t\"/>
\t<rect x=\"100\" y=\"10\" class=\"st0\" width=\"40\" height=\"20\"/>
\t<line class=\"st0\" x1=\"150\" y1=\"10\" x2=\"190\" y2=\"90\"/>
</g>
</svg>
";
  let d = read_svg(s.as_bytes()).unwrap();
  let b = beziers(&d);
  assert_eq!(b.len(), 4);
  assert_eq!(b[3][3], b[0][0], "the circle-ish path is closed");
  assert_eq!(lines(&d).len(), 3 + 4 + 1);
  // no width: user units are px
  let px = 25.4 / 96.0;
  assert!(near(b[0][0], [10.5 * px, (100.0 - 20.25) * px], 1e-12), "{:?}", b[0][0]);
  assert!(d.skipped.is_empty(), "{:?}", d.skipped);
}

#[test]
fn svg_round_trip() {
  use std::f64::consts::{FRAC_PI_2, PI};
  let ents = vec![
    // a slot: line, arc, line, arc (one closed path)
    (Entity::Line { a: [0.0, 0.0], b: [20.0, 0.0] }, false),
    (Entity::Arc { center: [20.0, 5.0], radius: 5.0, start: -FRAC_PI_2, end: FRAC_PI_2 }, false),
    (Entity::Line { a: [20.0, 10.0], b: [0.0, 10.0] }, false),
    (Entity::Arc { center: [0.0, 5.0], radius: 5.0, start: FRAC_PI_2, end: 3.0 * FRAC_PI_2 }, false),
    (Entity::Circle { center: [10.0, 5.0], radius: 2.5 }, false),
    // an open chain whose first line runs backwards, and an arc traversed clockwise
    (Entity::Line { a: [10.0, 30.0], b: [0.0, 30.0] }, false),
    (Entity::Line { a: [10.0, 30.0], b: [10.0, 40.0] }, false),
    (Entity::Arc { center: [15.0, 40.0], radius: 5.0, start: 0.0, end: PI }, false),
    (Entity::Bezier { p: [[20.0, 40.0], [25.0, 50.0], [30.0, 30.0], [-35.0, -40.0]] }, false),
    // construction, and a point (which SVG can't hold)
    (Entity::Line { a: [-5.0, -5.0], b: [25.0, 15.0] }, true),
    (Entity::Point { at: [1.0, 2.0] }, false),
  ];
  let text = write_svg(&ents);
  assert_eq!(text.matches("<path").count(), 3, "{text}");
  assert_eq!(text.matches("<circle").count(), 1);
  assert_eq!(text.matches(" Z").count(), 1);
  assert!(text.contains("mm\" height=\""));
  let d = read_svg(text.as_bytes()).unwrap();
  assert!(d.skipped.is_empty());
  let back: Vec<_> = d.entities.clone();
  let want: Vec<_> = ents.iter().filter(|e| !matches!(e.0, Entity::Point { .. })).collect();
  assert_eq!(back.len(), want.len(), "{back:?}");
  let tol = 1e-6;
  let same = |x: &Entity, y: &Entity| -> bool {
    match (x.clone(), y.clone()) {
      (Entity::Circle { center: c0, radius: r0 }, Entity::Circle { center: c1, radius: r1 }) => {
        near(c0, c1, tol) && (r0 - r1).abs() < tol
      }
      (
        Entity::Arc { center: c0, radius: r0, start: s0, end: e0 },
        Entity::Arc { center: c1, radius: r1, start: s1, end: e1 },
      ) => {
        let (p0, q0) = ends_of(x);
        let (p1, q1) = ends_of(y);
        near(c0, c1, tol)
          && (r0 - r1).abs() < tol
          && near(p0, p1, tol)
          && near(q0, q1, tol)
          && (sweep(s0, e0) - sweep(s1, e1)).abs() < tol
      }
      (Entity::Line { a, b }, Entity::Line { a: c, b: e }) => {
        (near(a, c, tol) && near(b, e, tol)) || (near(a, e, tol) && near(b, c, tol))
      }
      (Entity::Bezier { p }, Entity::Bezier { p: q }) => {
        (0..4).all(|i| near(p[i], q[i], tol)) || (0..4).all(|i| near(p[i], q[3 - i], tol))
      }
      _ => false,
    }
  };
  for (e, cons) in &want {
    assert!(back.iter().any(|(b, c)| c == cons && same(e, b)), "{e:?} missing from {back:?}");
  }
  // and again: the second export is identical
  assert_eq!(write_svg(&back).matches("<path").count(), 3);
}

#[test]
fn svg_rejects_other_files() {
  for bad in
    [&b""[..], b"hello world", b"<html><body><svg/></body></html>", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR", b"\x1f\x8b\x08\0"]
  {
    let e = read_svg(bad).unwrap_err().to_string();
    assert!(e.contains("SVG"), "{e}");
  }
  assert!(read_svg(b"<svg xmlns='http://www.w3.org/2000/svg'><g><line x2='1'/></svg>").is_err());
  // an empty but valid SVG is fine
  assert!(read_svg(b"<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap().entities.is_empty());
}

#[test]
fn dxf_splines_and_ellipses_round_trip_as_entities() {
  use crate::dxf::Spline;
  use std::f64::consts::{FRAC_PI_2, PI};
  let control = Spline::clamped_uniform(3, vec![[0.0, 0.0], [10.0, 5.0], [20.0, -5.0], [30.0, 0.0], [40.0, 8.0]]);
  let fitted = Spline { degree: 3, fit: vec![[0.0, 0.0], [5.0, 5.0], [10.0, 0.0]], ..Spline::default() };
  let ents = vec![
    (Entity::Line { a: [0.0, 0.0], b: [10.0, 0.0] }, false),
    (Entity::Spline(control.clone()), false),
    (Entity::Spline(fitted), true),
    (Entity::ellipse([5.0, 5.0], [0.0, 2.0], 6.0), false),
    (Entity::Ellipse { center: [1.0, 2.0], major: [4.0, 0.0], ratio: 0.5, start: FRAC_PI_2, end: PI }, false),
    (Entity::Bezier { p: [[0.0, 0.0], [1.0, 2.0], [3.0, 2.0], [4.0, 0.0]] }, false),
  ];
  let text = write_dxf(&ents);
  assert!(text.contains("AC1015") && text.contains("SPLINE") && text.contains("ELLIPSE"), "R2000 with real entities");
  assert!(text.contains("AcDbSpline") && text.contains("*Model_Space"));
  let d = read_dxf(text.as_bytes()).unwrap();
  assert!(d.skipped.is_empty(), "{:?}", d.skipped);
  assert_eq!(d.entities.len(), ents.len());
  // the control spline comes back with the same knots and control points
  match &d.entities[1].0 {
    Entity::Spline(sp) => {
      assert_eq!((sp.degree, sp.control.len()), (3, 5));
      assert!(sp.is_uniform() && !sp.is_rational() && sp.fit.is_empty());
      for (a, b) in sp.control.iter().zip(&control.control) {
        assert!((a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
      }
      // and draws the same curve
      for (p, q) in sp.beziers().iter().flatten().zip(control.beziers().iter().flatten()) {
        assert!((p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9);
      }
    }
    e => panic!("{e:?}"),
  }
  // fit points survive, on the construction layer
  match &d.entities[2] {
    (Entity::Spline(sp), true) => assert_eq!(sp.fit.len(), 3),
    e => panic!("{e:?}"),
  }
  // a taller-than-wide ellipse is written with the axes swapped (ratio at most 1)
  match d.entities[3].0 {
    Entity::Ellipse { center, major, ratio, .. } => {
      assert!((center[0] - 5.0).abs() < 1e-9 && (major[0].hypot(major[1]) - 6.0).abs() < 1e-9);
      assert!((ratio - 2.0 / 6.0).abs() < 1e-9);
      assert!(d.entities[3].0.is_full_ellipse());
    }
    ref e => panic!("{e:?}"),
  }
  match d.entities[4].0 {
    Entity::Ellipse { start, end, .. } => {
      assert!((start - FRAC_PI_2).abs() < 1e-9 && (end - PI).abs() < 1e-9);
      assert!(!d.entities[4].0.is_full_ellipse());
      // the elliptical arc's pieces end where the arc does: (1, 2) + 0.5 * 4 * (0, 1) rotated to π → (-3, 2)
      let bz = d.entities[4].0.beziers();
      let last = bz.last().unwrap()[3];
      assert!((last[0] + 3.0).abs() < 1e-9 && (last[1] - 2.0).abs() < 1e-9, "{last:?}");
    }
    ref e => panic!("{e:?}"),
  }
  // a lone Bézier is a one-piece SPLINE
  match &d.entities[5].0 {
    Entity::Spline(sp) => assert_eq!(sp.beziers(), vec![[[0.0, 0.0], [1.0, 2.0], [3.0, 2.0], [4.0, 0.0]]]),
    e => panic!("{e:?}"),
  }
  // without splines or ellipses the file stays R12
  assert!(write_dxf(&ents[..1]).contains("AC1009"));
}

#[test]
fn dxf_reads_foreign_splines_and_mirrored_ellipses() {
  use std::f64::consts::FRAC_PI_2;
  // a quadratic spline with a non-uniform knot vector, a rational one (a quarter circle), a
  // fit-point-only one, and an ellipse arc on a flipped extrusion
  let text = "0\nSECTION\n2\nENTITIES\n\
0\nSPLINE\n8\n0\n70\n8\n71\n2\n72\n7\n73\n4\n40\n0\n40\n0\n40\n0\n40\n1\n40\n3\n40\n3\n40\n3\n\
10\n0\n20\n0\n30\n0\n10\n1\n20\n2\n30\n0\n10\n3\n20\n2\n30\n0\n10\n4\n20\n0\n30\n0\n\
0\nSPLINE\n8\n0\n70\n12\n71\n2\n72\n6\n73\n3\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n\
41\n1\n41\n0.7071067811865476\n41\n1\n10\n10\n20\n0\n10\n10\n20\n10\n10\n0\n20\n10\n\
0\nSPLINE\n8\n0\n70\n8\n71\n3\n74\n3\n11\n0\n21\n0\n11\n1\n21\n1\n11\n2\n21\n0\n\
0\nELLIPSE\n8\n0\n10\n5\n20\n0\n11\n2\n21\n0\n40\n0.5\n41\n0\n42\n1.5707963267948966\n230\n-1\n\
0\nENDSEC\n0\nEOF\n";
  let d = read_dxf(text.as_bytes()).unwrap();
  assert!(d.skipped.is_empty(), "{:?}", d.skipped);
  assert_eq!(d.entities.len(), 4);
  let Entity::Spline(quad) = &d.entities[0].0 else { panic!() };
  let pieces = quad.beziers();
  assert_eq!(pieces.len(), 2, "two knot spans");
  assert_eq!(pieces[0][0], [0.0, 0.0]);
  assert_eq!(pieces[1][3], [4.0, 0.0]);
  // exact: the pieces lie on the B-spline
  for (i, p) in pieces.iter().enumerate() {
    let (u0, u1) = if i == 0 { (0.0, 1.0) } else { (1.0, 3.0) };
    for t in [0.25, 0.5, 0.75] {
      let a = Entity::bezier_point(p, t);
      // compare against the curve at the matching parameter: quadratic pieces are uniform in t
      let b = quad.point(u0 + (u1 - u0) * t);
      assert!((a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9, "{a:?} {b:?}");
    }
  }
  // the rational quarter circle is fitted closely
  let Entity::Spline(circle) = &d.entities[1].0 else { panic!() };
  assert!(circle.is_rational());
  for p in circle.beziers() {
    for t in [0.0, 0.3, 0.5, 0.8, 1.0] {
      let q = Entity::bezier_point(&p, t);
      assert!((q[0].hypot(q[1]) - 10.0).abs() < 1e-4, "{q:?}");
    }
  }
  let Entity::Spline(fit) = &d.entities[2].0 else { panic!() };
  assert!(fit.control.is_empty() && fit.fit.len() == 3);
  assert_eq!(fit.beziers().len(), 2);
  // the flipped ellipse: world coordinates kept, direction reversed (−π/2 .. 0)
  let Entity::Ellipse { center, start, end, .. } = d.entities[3].0 else { panic!() };
  assert_eq!(center, [5.0, 0.0]);
  assert!((start + FRAC_PI_2).abs() < 1e-12 && end.abs() < 1e-12);
  let bz = d.entities[3].0.beziers();
  assert!((bz[0][0][1] + 1.0).abs() < 1e-9, "starts at the bottom of the minor axis: {:?}", bz[0][0]);
}

#[test]
fn dxf_written_in_each_export_unit_reads_back_in_mm() {
  use crate::dxf::{DXF_UNITS, write_dxf_in};
  let ents = vec![
    (Entity::Line { a: [0.0, 0.0], b: [25.4, 0.0] }, false),
    (Entity::Circle { center: [5.0, 5.0], radius: 2.0 }, true),
    (Entity::Point { at: [1.0, 2.0] }, false),
  ];
  for (key, _, mm, code) in DXF_UNITS {
    let text = write_dxf_in(&ents, key).unwrap();
    assert!(text.contains(&format!("$INSUNITS\n 70\n{code}\n")), "{key}");
    let d = read_dxf(text.as_bytes()).unwrap();
    assert_eq!(d.entities.len(), 3, "{key}");
    match d.entities[0].0 {
      Entity::Line { b, .. } => assert!((b[0] - 25.4).abs() < 1e-6 * mm.max(1.0), "{key}: {b:?}"),
      ref e => panic!("{e:?}"),
    }
    assert!(d.entities[1].1, "construction stays on its layer");
  }
  // an inch file holds inches
  let text = write_dxf_in(&ents, "in").unwrap();
  assert!(text.contains("\n 11\n1\n") && text.contains("(in)"), "{text}");
  assert!(write_dxf_in(&ents, "furlong").is_none());
}
