//! The cxx bridge. Every fallible call returns `Result`; the C++ side converts all kernel
//! exceptions to errors, so nothing here can abort the process.
#![allow(unsafe_code, clippy::too_many_arguments, clippy::module_inception)]

#[cxx::bridge(namespace = "clf")]
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

  unsafe extern "C++" {
    include!("clf-kernel/cpp/kernel.hxx");

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
    fn shell(s: &Shape, faces: &[u32], thickness: f64) -> Result<UniquePtr<Shape>>;
    fn transform(s: &Shape, m: &[f64]) -> Result<UniquePtr<Shape>>;
    fn mirror(s: &Shape, plane: &[f64]) -> Result<UniquePtr<Shape>>;
    fn unify(s: &Shape) -> Result<UniquePtr<Shape>>;

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
  }
}
