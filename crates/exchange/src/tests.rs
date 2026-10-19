use super::*;
use crate::archive::{Archive, ArchivedVersion, inspect_fusion, read_clfz, write_clfz};
use crate::dxf::{Entity, bulge_arc, read_dxf, write_dxf};
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
  match d.entities[2] {
    (Entity::Arc { radius, start, end, .. }, true) => {
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
fn clfz_round_trip_with_history() {
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
  let bytes = write_clfz(&a).unwrap();
  assert_eq!(read_clfz(&bytes).unwrap(), a);
  assert!(read_clfz(&write_3mf(&[cube(1.0)], None).unwrap()).is_err());
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
