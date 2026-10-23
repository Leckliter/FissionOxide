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

  /// Per face of a shape (1-based face k spans `offsets[k-1]..offsets[k]`), the labels its history
  /// traces it to, each flagged when the face was generated from the labelled shape rather than
  /// modified from it.
  struct LabelData {
    offsets: Vec<u32>,
    labels: Vec<u64>,
    generated: Vec<u8>,
  }

  unsafe extern "C++" {
    include!("fox-kernel/cpp/kernel.hxx");

    type Shape;

    fn shape_clone(s: &Shape) -> UniquePtr<Shape>;
    fn shape_is_null(s: &Shape) -> bool;
    fn shape_type(s: &Shape) -> i32;

    /// `merge`: empty, or one flag per segment: nonzero lets it become one edge with the next
    /// in its loop (see `Shape::face_with`).
    fn make_face(plane: &[f64], segs: &[f64], loop_lens: &[u32], merge: &[u8]) -> Result<UniquePtr<Shape>>;
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
    fn chamfer2(
      s: &Shape,
      edges: &[u32],
      d1: f64,
      d2: f64,
      angle: bool,
      flip: bool,
      forward_face: bool,
    ) -> Result<UniquePtr<Shape>>;
    fn fillet_var(s: &Shape, edges: &[u32], r1: f64, r2: f64) -> Result<UniquePtr<Shape>>;
    fn split_by_shape(s: &Shape, tool: &Shape) -> Result<UniquePtr<Shape>>;
    fn hlr_project(s: &Shape, view: &[f64], deflection: f64) -> Result<HlrData>;
    /// `merge` as for `make_face`.
    fn make_wire(segs: &[f64], merge: &[u8]) -> Result<UniquePtr<Shape>>;
    fn make_pipe(face: &Shape, spine: &Shape) -> Result<UniquePtr<Shape>>;
    fn make_loft(sections: &Shape, ruled: bool, closed: bool) -> Result<UniquePtr<Shape>>;
    fn split_by_plane(s: &Shape, plane: &[f64], size: f64) -> Result<UniquePtr<Shape>>;
    fn draft(s: &Shape, faces: &[u32], plane: &[f64], angle: f64) -> Result<UniquePtr<Shape>>;
    fn draft_two_sided(s: &Shape, faces: &[u32], plane: &[f64], angle: f64, angle2: f64) -> Result<UniquePtr<Shape>>;

    fn edge_at_index(s: &Shape, edge: u32) -> Result<UniquePtr<Shape>>;
    fn min_distance(a: &Shape, b: &Shape) -> Result<Vec<f64>>;
    fn curve_along(s: &Shape, t: f64, from: &[f64]) -> Result<Vec<f64>>;
    fn helix_sweep(
      profile: &Shape,
      axis: &[f64],
      radius: f64,
      pitch: f64,
      height: f64,
      left: bool,
      taper: f64,
    ) -> Result<UniquePtr<Shape>>;
    fn thicken(s: &Shape, d: f64) -> Result<UniquePtr<Shape>>;
    fn scale(s: &Shape, cx: f64, cy: f64, cz: f64, sx: f64, sy: f64, sz: f64) -> Result<UniquePtr<Shape>>;
    fn remove_faces(s: &Shape, faces: &[u32]) -> Result<UniquePtr<Shape>>;
    fn split_faces(s: &Shape, faces: &[u32], tool: &Shape) -> Result<UniquePtr<Shape>>;
    fn split_faces_by_edges(s: &Shape, edges: &Shape) -> Result<UniquePtr<Shape>>;
    fn face_edge_loops(s: &Shape) -> Result<Vec<i32>>;
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
    fn sweep_rail_legacy(profile: &Shape, spine: &Shape, rail: &Shape) -> Result<UniquePtr<Shape>>;
    fn sweep_twist_legacy(profile: &Shape, spine: &Shape, twist: f64) -> Result<UniquePtr<Shape>>;
    fn sweep_guided(
      profile: &Shape,
      spine: &Shape,
      rail: &Shape,
      has_rail: bool,
      scale: bool,
      twist: f64,
      taper: f64,
      parallel: bool,
    ) -> Result<UniquePtr<Shape>>;
    fn sweep_rails(profile: &Shape, spine: &Shape, rail1: &Shape, rail2: &Shape, mode: i32)
    -> Result<UniquePtr<Shape>>;
    fn sweep_surface(profile: &Shape, spine: &Shape, surface: &Shape) -> Result<UniquePtr<Shape>>;
    fn sweep_sections(sections: &Shape, spine: &Shape) -> Result<UniquePtr<Shape>>;
    fn loft_rails(sections: &Shape, rails: &Shape, spread: bool) -> Result<UniquePtr<Shape>>;
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
    fn loft_closed(sections: &Shape) -> Result<UniquePtr<Shape>>;
    fn loft_point_tangent(profile: &Shape, apex: &Shape, axis: &[f64], weight: f64) -> Result<UniquePtr<Shape>>;
    // surface workspace (cpp/surface2.cpp)
    include!("fox-kernel/cpp/surface2.hxx");
    fn extend_edges(s: &Shape, edges: &[u32], d: f64, mode: i32, corners: bool) -> Result<UniquePtr<Shape>>;
    fn untrim_faces(s: &Shape, faces: &[u32], loops: i32, extend: f64) -> Result<UniquePtr<Shape>>;
    fn classify_point(tool: &Shape, x: f64, y: f64, z: f64) -> Result<f64>;
    fn offset_variable(s: &Shape, d: f64, faces: &[u32], dists: &[f64]) -> Result<UniquePtr<Shape>>;
    fn offset_joined(s: &Shape, d: f64, round: bool, thicken: bool) -> Result<UniquePtr<Shape>>;
    // sheet2.cpp
    fn ruled_bspline(a: &[f64], b: &[f64], u: &[f64]) -> Result<UniquePtr<Shape>>;
    fn edge_nurbs(s: &Shape, edge: u32) -> Result<Vec<f64>>;

    // STEP assemblies with names and colours (cpp/xcaf.cpp)
    include!("fox-kernel/cpp/xcaf.hxx");
    type StepDoc;
    fn step_doc_new(name: &str) -> Result<UniquePtr<StepDoc>>;
    fn step_doc_assembly(d: Pin<&mut StepDoc>, name: &str) -> Result<usize>;
    fn step_doc_part(d: Pin<&mut StepDoc>, name: &str, s: &Shape, color: &[f64], face_colors: &[f64]) -> Result<usize>;
    fn step_doc_place(d: Pin<&mut StepDoc>, parent: usize, child: usize, name: &str, at: &[f64]) -> Result<()>;
    fn step_doc_write(d: Pin<&mut StepDoc>, path: &str) -> Result<()>;
    fn step_doc_read(path: &str) -> Result<UniquePtr<StepDoc>>;
    fn step_doc_len(d: &StepDoc) -> usize;
    fn step_doc_parent(d: &StepDoc, i: usize) -> i64;
    fn step_doc_is_assembly(d: &StepDoc, i: usize) -> bool;
    fn step_doc_name(d: &StepDoc, i: usize) -> String;
    fn step_doc_instance(d: &StepDoc, i: usize) -> String;
    fn step_doc_proto(d: &StepDoc, i: usize) -> usize;
    fn step_doc_uses(d: &StepDoc, i: usize) -> usize;
    fn step_doc_placement(d: &StepDoc, i: usize) -> Vec<f64>;
    fn step_doc_color(d: &StepDoc, i: usize) -> Vec<f64>;
    fn step_doc_shape(d: &StepDoc, i: usize) -> Result<UniquePtr<Shape>>;
    fn step_doc_face_colors(d: &StepDoc, i: usize, body: &Shape) -> Result<Vec<f64>>;

    // fillets with a radius law, merging split hole walls (cpp/refparts.cpp)
    include!("fox-kernel/cpp/refparts.hxx");
    fn fillet_law(s: &Shape, edges: &[u32], at: &[f64], radii: &[f64]) -> Result<UniquePtr<Shape>>;
    fn fillet_laws(s: &Shape, edges: &[u32], counts: &[u32], at: &[f64], radii: &[f64]) -> Result<UniquePtr<Shape>>;
    fn unify_faces_internal(s: &Shape) -> Result<UniquePtr<Shape>>;

    // form (T-spline / subdivision) bodies (cpp/form.cpp)
    include!("fox-kernel/cpp/form.hxx");
    fn bspline_face(
      degree: u32,
      nu: u32,
      nv: u32,
      poles: &[f64],
      uknots: &[f64],
      umults: &[u32],
      vknots: &[f64],
      vmults: &[u32],
    ) -> Result<UniquePtr<Shape>>;
    // Insert T-Spline faces
    fn grid_face(points: &[f64], rows: u32, cols: u32) -> Result<UniquePtr<Shape>>;

    // Project To Surface and pipes along the 3D wires it gives (cpp/project.cpp)
    include!("fox-kernel/cpp/project.hxx");
    fn project_curves(curves: &Shape, faces: &Shape, dir: &[f64]) -> Result<UniquePtr<Shape>>;
    fn round_pipe(spine: &Shape, radius: f64, inner: f64) -> Result<UniquePtr<Shape>>;
    fn wire_ends(wire: &Shape) -> Result<Vec<f64>>;
    fn split_seams(s: &Shape) -> Result<UniquePtr<Shape>>;
    fn child_count(s: &Shape) -> usize;
    fn child_at(s: &Shape, i: usize) -> Result<UniquePtr<Shape>>;

    // spun profiles: a body's reach from an axis along it (cpp/spun.cpp)
    include!("fox-kernel/cpp/spun.hxx");
    fn axial_events(s: &Shape, axis: &[f64]) -> Result<Vec<f64>>;
    fn radial_slices(s: &Shape, axis: &[f64], ts: &[f64]) -> Result<Vec<f64>>;
    // merging the strips a helical cut leaves on a cylinder (cpp/threads.cpp)
    include!("fox-kernel/cpp/threads.hxx");
    fn merge_cylinder_strips(s: &Shape, axis: &[f64], radius: f64, lo: f64, hi: f64) -> Result<UniquePtr<Shape>>;
    // where shapes cross, as wires; where a wire starts (cpp/curves3d.cpp)
    include!("fox-kernel/cpp/curves3d.hxx");
    fn section_wires(a: &Shape, b: &Shape, tolerance: f64) -> Result<UniquePtr<Shape>>;
    fn chain_wires(s: &Shape, tolerance: f64) -> Result<UniquePtr<Shape>>;
    fn split_at_corners(w: &Shape, angle: f64) -> Result<UniquePtr<Shape>>;
    fn wire_corners(w: &Shape, angle: f64) -> Result<Vec<f64>>;
    fn wire_start(w: &Shape) -> Result<Vec<f64>>;
    fn wire_count(s: &Shape) -> usize;
    fn wire_at(s: &Shape, i: usize) -> Result<UniquePtr<Shape>>;
    fn wire_from_middle(w: &Shape) -> Result<UniquePtr<Shape>>;
    // a body's silhouette on a plane (cpp/silhouette.cpp)
    include!("fox-kernel/cpp/silhouette.hxx");
    fn silhouette(s: &Shape, plane: &[f64]) -> Result<UniquePtr<Shape>>;
    // offsetting chosen faces of a solid, neighbours extending to meet them (cpp/offsetfaces.cpp)
    include!("fox-kernel/cpp/offsetfaces.hxx");
    fn offset_faces(s: &Shape, faces: &[u32], d: f64) -> Result<UniquePtr<Shape>>;
    // modification history: labels followed through kernel calls (cpp/history.cpp)
    include!("fox-kernel/cpp/history.hxx");
    fn set_labels(s: Pin<&mut Shape>, face_ids: &[u32], face_labels: &[u64], edges: &[u64]) -> Result<()>;
    fn has_labels(s: &Shape) -> bool;
    fn face_labels(s: &Shape) -> Result<LabelData>;
    fn same_faces(now: &Shape, before: &Shape) -> Result<Vec<u32>>;
    // surface Patch, Ruled, Sweep and Loft as Fusion's (cpp/surface3.cpp)
    include!("fox-kernel/cpp/surface3.hxx");
    fn patch_with(boundary: &Shape, supports: &Shape, orders: &[u8], interior: &Shape) -> Result<UniquePtr<Shape>>;
    fn ruled_from_edge(
      edge: &Shape,
      face: &Shape,
      distance: f64,
      angle: f64,
      tangent: bool,
    ) -> Result<UniquePtr<Shape>>;
    fn sweep_open(profile: &Shape, spine: &Shape, parallel: bool) -> Result<UniquePtr<Shape>>;
    fn loft_open(sections: &Shape, closed: bool) -> Result<UniquePtr<Shape>>;
  }
}

// SAFETY: a Shape is a TopoDS_Shape, whose handles OCCT reference-counts atomically. The engine
// moves a design's shapes to a worker thread for background regeneration and back; while that
// runs the other thread makes no kernel calls (see `fox_engine::Rebuild`), so no shape is used
// from two threads at once.
unsafe impl Send for ffi::Shape {}
