//! The cxx bridge. Every fallible call returns `Result`; the C++ side converts all kernel
//! exceptions to errors, so nothing here can abort the process.
#![allow(unsafe_code, clippy::too_many_arguments, clippy::module_inception)]

#[cxx::bridge(namespace = "fox")]
pub(crate) mod ffi {
  struct MeshData {
    positions: Vec<f64>,
    normals: Vec<f64>,
    triangles: Vec<u32>,
    /// 1-based face index (into the shape's face map) per triangle.
    tri_face: Vec<u32>,
  }

  struct EdgeData {
    points: Vec<f64>,
    /// Point offsets: edge k spans points offsets[k]..offsets[k+1].
    offsets: Vec<u32>,
    /// 1-based edge index (into the shape's edge map) per polyline.
    edge_ids: Vec<u32>,
  }

  struct FaceInfo {
    /// 0 plane, 1 cylinder, 2 cone, 3 sphere, 4 torus, 5 other
    kind: u8,
    origin: [f64; 3],
    normal: [f64; 3],
    xdir: [f64; 3],
    center: [f64; 3],
    area: f64,
    radius: f64,
  }

  struct EdgeInfo {
    /// 0 line, 1 circle/arc, 2 other curve, 3 degenerated
    kind: u8,
    p0: [f64; 3],
    p1: [f64; 3],
    mid: [f64; 3],
    center: [f64; 3],
    axis: [f64; 3],
    radius: f64,
    length: f64,
    /// 1-based indices of the (up to two) faces meeting at this edge; 0 = none.
    face1: u32,
    face2: u32,
  }

  struct MassProps {
    volume: f64,
    area: f64,
    cog: [f64; 3],
    bbox_min: [f64; 3],
    bbox_max: [f64; 3],
  }

  /// A hidden-line projection: 2D polylines with a kind each (0 visible, 1 visible tangent edge,
  /// 2 visible silhouette, 3 hidden, 4 hidden silhouette) and full circles (cx, cy, r, kind).
  struct HlrData {
    points: Vec<f64>,
    offsets: Vec<u32>,
    kinds: Vec<u8>,
    circles: Vec<f64>,
  }

  unsafe extern "C++" {
    include!("fox-kernel/cpp/kernel.hxx");

    type Shape;

    fn shape_clone(s: &Shape) -> UniquePtr<Shape>;
    fn shape_is_null(s: &Shape) -> bool;
    fn shape_type(s: &Shape) -> i32;

    fn make_face(plane: &[f64], segs: &[f64], loop_lens: &[u32]) -> Result<UniquePtr<Shape>>;
    fn make_prism(face: &Shape, dx: f64, dy: f64, dz: f64) -> Result<UniquePtr<Shape>>;
    fn make_revol(face: &Shape, axis: &[f64], angle: f64) -> Result<UniquePtr<Shape>>;
    fn make_box(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Result<UniquePtr<Shape>>;
    fn make_cylinder(axis: &[f64], radius: f64, height: f64) -> Result<UniquePtr<Shape>>;
    fn make_sphere(cx: f64, cy: f64, cz: f64, radius: f64) -> Result<UniquePtr<Shape>>;
    fn make_torus(axis: &[f64], major: f64, minor: f64) -> Result<UniquePtr<Shape>>;

    fn boolean_op(a: &Shape, b: &Shape, op: i32, fuzzy: f64) -> Result<UniquePtr<Shape>>;
    fn fillet(s: &Shape, edges: &[u32], radius: f64) -> Result<UniquePtr<Shape>>;
    fn chamfer(s: &Shape, edges: &[u32], dist: f64) -> Result<UniquePtr<Shape>>;
    fn shell(s: &Shape, faces: &[u32], thickness: f64, round: bool) -> Result<UniquePtr<Shape>>;
    fn transform(s: &Shape, m: &[f64]) -> Result<UniquePtr<Shape>>;
    fn mirror(s: &Shape, plane: &[f64]) -> Result<UniquePtr<Shape>>;
    fn unify(s: &Shape) -> Result<UniquePtr<Shape>>;
    fn unfold(s: &Shape, face: u32, thickness: f64, k_factor: f64, bends: &mut Vec<f64>) -> Result<UniquePtr<Shape>>;
    fn face_at_index(s: &Shape, face: u32) -> Result<UniquePtr<Shape>>;
    fn refold_cuts(
      folded: &Shape,
      face: u32,
      thickness: f64,
      k_factor: f64,
      removed: &Shape,
    ) -> Result<UniquePtr<Shape>>;
    fn fillet_radii(s: &Shape, edges: &[u32], radii: &[f64]) -> Result<UniquePtr<Shape>>;
    fn draft_prism(face: &Shape, height: f64, angle: f64) -> Result<UniquePtr<Shape>>;
    fn offset_face(face: &Shape, d: f64) -> Result<UniquePtr<Shape>>;
    fn chamfer2(s: &Shape, edges: &[u32], d1: f64, d2: f64, angle: bool) -> Result<UniquePtr<Shape>>;
    fn fillet_var(s: &Shape, edges: &[u32], r1: f64, r2: f64) -> Result<UniquePtr<Shape>>;
    fn split_by_shape(s: &Shape, tool: &Shape) -> Result<UniquePtr<Shape>>;
    fn hlr_project(s: &Shape, view: &[f64], deflection: f64) -> Result<HlrData>;
    fn make_wire(segs: &[f64]) -> Result<UniquePtr<Shape>>;
    fn make_pipe(face: &Shape, spine: &Shape) -> Result<UniquePtr<Shape>>;
    fn make_loft(sections: &Shape, ruled: bool, closed: bool) -> Result<UniquePtr<Shape>>;
    fn split_by_plane(s: &Shape, plane: &[f64], size: f64) -> Result<UniquePtr<Shape>>;
    fn draft(s: &Shape, faces: &[u32], plane: &[f64], angle: f64) -> Result<UniquePtr<Shape>>;

    fn edge_at_index(s: &Shape, edge: u32) -> Result<UniquePtr<Shape>>;
    fn min_distance(a: &Shape, b: &Shape) -> Result<Vec<f64>>;
    fn helix_sweep(
      profile: &Shape,
      axis: &[f64],
      radius: f64,
      pitch: f64,
      height: f64,
      left: bool,
    ) -> Result<UniquePtr<Shape>>;
    fn thicken(s: &Shape, d: f64) -> Result<UniquePtr<Shape>>;
    fn scale(s: &Shape, cx: f64, cy: f64, cz: f64, sx: f64, sy: f64, sz: f64) -> Result<UniquePtr<Shape>>;
    fn remove_faces(s: &Shape, faces: &[u32]) -> Result<UniquePtr<Shape>>;
    fn split_faces(s: &Shape, faces: &[u32], tool: &Shape) -> Result<UniquePtr<Shape>>;
    fn patch(boundary: &Shape) -> Result<UniquePtr<Shape>>;
    fn face_loops(s: &Shape, deflection: f64) -> Result<EdgeData>;
    fn reversed(s: &Shape) -> Result<UniquePtr<Shape>>;
    fn make_vertex(x: f64, y: f64, z: f64) -> Result<UniquePtr<Shape>>;
    fn ruled(a: &Shape, b: &Shape) -> Result<UniquePtr<Shape>>;
    fn offset_shape(s: &Shape, d: f64) -> Result<UniquePtr<Shape>>;
    fn offset_polylines(face: &Shape, d: f64, deflection: f64) -> Result<EdgeData>;

    fn solid_count(s: &Shape) -> u32;
    fn solid_at(s: &Shape, i: u32) -> Result<UniquePtr<Shape>>;
    fn compound_new() -> UniquePtr<Shape>;
    fn compound_add(c: Pin<&mut Shape>, s: &Shape) -> Result<()>;

    fn mesh_shape(s: &Shape, linear: f64, angular: f64) -> Result<MeshData>;
    fn edge_polylines(s: &Shape, deflection: f64) -> Result<EdgeData>;
    fn face_infos(s: &Shape) -> Result<Vec<FaceInfo>>;
    fn edge_infos(s: &Shape) -> Result<Vec<EdgeInfo>>;
    fn mass_props(s: &Shape) -> Result<MassProps>;
    fn is_valid(s: &Shape) -> bool;

    fn write_step(s: &Shape, path: &str) -> Result<()>;
    fn read_step(path: &str) -> Result<UniquePtr<Shape>>;
    fn write_brep(s: &Shape) -> Result<String>;
    fn read_brep(data: &str) -> Result<UniquePtr<Shape>>;
    fn solidify(s: &Shape, tol: f64) -> Result<UniquePtr<Shape>>;
    fn write_iges(s: &Shape, path: &str) -> Result<()>;
    fn read_iges(path: &str) -> Result<UniquePtr<Shape>>;
    fn mesh_to_shape(pos: &[f64], tris: &[u32], merge: bool) -> Result<UniquePtr<Shape>>;

    // second batch of solid features (cpp/features2.cpp)
    fn sweep_rail(profile: &Shape, spine: &Shape, rail: &Shape) -> Result<UniquePtr<Shape>>;
    fn sweep_twist(profile: &Shape, spine: &Shape, twist: f64) -> Result<UniquePtr<Shape>>;
    fn sweep_sections(sections: &Shape, spine: &Shape) -> Result<UniquePtr<Shape>>;
    fn loft_rails(sections: &Shape, rails: &Shape) -> Result<UniquePtr<Shape>>;
    fn emboss_tool(
      body: &Shape,
      face: u32,
      profiles: &Shape,
      dir: &[f64],
      depth: f64,
      deboss: bool,
      wrap: bool,
    ) -> Result<UniquePtr<Shape>>;
    fn boundary_cells(tools: &Shape) -> Result<UniquePtr<Shape>>;
    fn classify(s: &Shape, x: f64, y: f64, z: f64) -> Result<i32>;
    fn plane_face(plane: &[f64], size: f64) -> Result<UniquePtr<Shape>>;
    fn extended_face(face: &Shape, size: f64, offset: f64, at: &[f64], toward: &[f64]) -> Result<UniquePtr<Shape>>;
    fn side_of(face: &Shape, x: f64, y: f64, z: f64) -> Result<f64>;
    fn setback_corners(s: &Shape, points: &[f64], radius: f64) -> Result<UniquePtr<Shape>>;
    // surface workspace (cpp/surface2.cpp)
    include!("fox-kernel/cpp/surface2.hxx");
    fn extend_edges(s: &Shape, edges: &[u32], d: f64, mode: i32) -> Result<UniquePtr<Shape>>;
    fn untrim_faces(s: &Shape, faces: &[u32], loops: i32) -> Result<UniquePtr<Shape>>;
    fn classify_point(tool: &Shape, x: f64, y: f64, z: f64) -> Result<f64>;
    fn offset_variable(s: &Shape, d: f64, faces: &[u32], dists: &[f64]) -> Result<UniquePtr<Shape>>;
    fn offset_joined(s: &Shape, d: f64, round: bool, thicken: bool) -> Result<UniquePtr<Shape>>;
    // sheet2.cpp
    fn ruled_bspline(a: &[f64], b: &[f64], u: &[f64]) -> Result<UniquePtr<Shape>>;

    // STEP assemblies with names and colours (cpp/xcaf.cpp)
    include!("fox-kernel/cpp/xcaf.hxx");
    type StepDoc;
    fn step_doc_new(name: &str) -> Result<UniquePtr<StepDoc>>;
    fn step_doc_add(d: Pin<&mut StepDoc>, component: &str, name: &str, s: &Shape, color: &[f64]) -> Result<()>;
    fn step_doc_write(d: Pin<&mut StepDoc>, path: &str) -> Result<()>;
    fn step_doc_read(path: &str) -> Result<UniquePtr<StepDoc>>;
    fn step_doc_len(d: &StepDoc) -> usize;
    fn step_doc_name(d: &StepDoc, i: usize) -> String;
    fn step_doc_component(d: &StepDoc, i: usize) -> String;
    fn step_doc_color(d: &StepDoc, i: usize) -> Vec<f64>;
    fn step_doc_shape(d: &StepDoc, i: usize) -> Result<UniquePtr<Shape>>;

    // fillets with a radius law, merging split hole walls (cpp/refparts.cpp)
    include!("fox-kernel/cpp/refparts.hxx");
    fn fillet_law(s: &Shape, edges: &[u32], at: &[f64], radii: &[f64]) -> Result<UniquePtr<Shape>>;
    fn unify_faces_internal(s: &Shape) -> Result<UniquePtr<Shape>>;

    // a body's silhouette on a plane (cpp/silhouette.cpp)
    include!("fox-kernel/cpp/silhouette.hxx");
    fn silhouette(s: &Shape, plane: &[f64]) -> Result<UniquePtr<Shape>>;
  }
}

// SAFETY: a Shape is a TopoDS_Shape, whose handles OCCT reference-counts atomically. The engine
// moves a design's shapes to a worker thread for background regeneration and back; while that
// runs the other thread makes no kernel calls (see `fox_engine::Rebuild`), so no shape is used
// from two threads at once.
unsafe impl Send for ffi::Shape {}
