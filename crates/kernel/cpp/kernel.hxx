// C++ side of the FissionOxide kernel bridge to OpenCASCADE.
//
// Every entry point catches *all* exceptions (OCCT's Standard_Failure does not derive from
// std::exception) and rethrows std::runtime_error, which cxx turns into a Rust Err. Nothing that
// goes wrong inside the kernel may abort the process. That includes crashes inside OCCT itself
// (a null dereference in a fillet, say): with OCC_CONVERT_SIGNALS, OCC_CATCH_SIGNALS in each
// entry point's try block turns the signal into a Standard_Failure (OCCT is built with it too).
#pragma once

#if !defined(_WIN32) && !defined(OCC_CONVERT_SIGNALS)
#define OCC_CONVERT_SIGNALS
#endif

#include <memory>
#include <mutex>
#include <OSD.hxx>
#include <Standard_ErrorHandler.hxx>
#include <TopoDS_Shape.hxx>
#include "rust/cxx.h"

namespace fox {

// Installs OCCT's signal handler once (without floating-point traps), so OCC_CATCH_SIGNALS can
// turn a crash inside OCCT into an exception.
inline void catch_signals() {
  static std::once_flag once;
  std::call_once(once, [] { OSD::SetSignal(Standard_False); });
}

struct Shape {
  TopoDS_Shape s;
};

struct MeshData;
struct EdgeData;
struct FaceInfo;
struct EdgeInfo;
struct MassProps;
struct HlrData;

std::unique_ptr<Shape> shape_clone(const Shape &s);
bool shape_is_null(const Shape &s);
int32_t shape_type(const Shape &s);

std::unique_ptr<Shape> make_face(rust::Slice<const double> plane, rust::Slice<const double> segs,
                                 rust::Slice<const uint32_t> loop_lens, rust::Slice<const uint8_t> merge);
std::unique_ptr<Shape> make_prism(const Shape &face, double dx, double dy, double dz);
std::unique_ptr<Shape> make_revol(const Shape &face, rust::Slice<const double> axis, double angle);
std::unique_ptr<Shape> make_box(double x0, double y0, double z0, double x1, double y1, double z1);
std::unique_ptr<Shape> make_cylinder(rust::Slice<const double> axis, double radius, double height);
std::unique_ptr<Shape> make_sphere(double cx, double cy, double cz, double radius);
std::unique_ptr<Shape> make_torus(rust::Slice<const double> axis, double major, double minor);

std::unique_ptr<Shape> boolean_op(const Shape &a, const Shape &b, int32_t op, double fuzzy);
std::unique_ptr<Shape> fillet(const Shape &s, rust::Slice<const uint32_t> edges, double radius);
std::unique_ptr<Shape> chamfer(const Shape &s, rust::Slice<const uint32_t> edges, double dist);
std::unique_ptr<Shape> shell(const Shape &s, rust::Slice<const uint32_t> faces, double thickness, bool round);
std::unique_ptr<Shape> transform(const Shape &s, rust::Slice<const double> m);
std::unique_ptr<Shape> mirror(const Shape &s, rust::Slice<const double> plane);
std::unique_ptr<Shape> unify(const Shape &s);
std::unique_ptr<Shape> unfold(const Shape &s, uint32_t face, double thickness, double k_factor,
                              rust::Vec<double> &bends);
std::unique_ptr<Shape> face_at_index(const Shape &s, uint32_t face);
std::unique_ptr<Shape> refold_cuts(const Shape &folded, uint32_t face, double thickness, double k_factor,
                                   const Shape &removed);
std::unique_ptr<Shape> fillet_radii(const Shape &s, rust::Slice<const uint32_t> edges,
                                    rust::Slice<const double> radii);
std::unique_ptr<Shape> draft_prism(const Shape &face, double height, double angle);
std::unique_ptr<Shape> offset_face(const Shape &face, double d);
std::unique_ptr<Shape> chamfer2(const Shape &s, rust::Slice<const uint32_t> edges, double d1, double d2, bool angle);
std::unique_ptr<Shape> fillet_var(const Shape &s, rust::Slice<const uint32_t> edges, double r1, double r2);
std::unique_ptr<Shape> split_by_shape(const Shape &s, const Shape &tool);
HlrData hlr_project(const Shape &s, rust::Slice<const double> view, double deflection);
std::unique_ptr<Shape> make_wire(rust::Slice<const double> segs, rust::Slice<const uint8_t> merge);
std::unique_ptr<Shape> make_pipe(const Shape &face, const Shape &spine);
std::unique_ptr<Shape> make_loft(const Shape &sections, bool ruled, bool closed);
std::unique_ptr<Shape> split_by_plane(const Shape &s, rust::Slice<const double> plane, double size);
std::unique_ptr<Shape> draft(const Shape &s, rust::Slice<const uint32_t> faces, rust::Slice<const double> plane,
                             double angle);

std::unique_ptr<Shape> edge_at_index(const Shape &s, uint32_t edge);
rust::Vec<double> min_distance(const Shape &a, const Shape &b);
std::unique_ptr<Shape> helix_sweep(const Shape &profile, rust::Slice<const double> axis, double radius, double pitch,
                                   double height, bool left);
std::unique_ptr<Shape> thicken(const Shape &s, double d);
std::unique_ptr<Shape> scale(const Shape &s, double cx, double cy, double cz, double sx, double sy, double sz);
std::unique_ptr<Shape> remove_faces(const Shape &s, rust::Slice<const uint32_t> faces);
std::unique_ptr<Shape> split_faces(const Shape &s, rust::Slice<const uint32_t> faces, const Shape &tool);
std::unique_ptr<Shape> patch(const Shape &boundary);
EdgeData face_loops(const Shape &s, double deflection);
std::unique_ptr<Shape> reversed(const Shape &s);
std::unique_ptr<Shape> make_vertex(double x, double y, double z);
std::unique_ptr<Shape> ruled(const Shape &a, const Shape &b);
std::unique_ptr<Shape> offset_shape(const Shape &s, double d);
EdgeData offset_polylines(const Shape &face, double d, double deflection);

uint32_t solid_count(const Shape &s);
std::unique_ptr<Shape> solid_at(const Shape &s, uint32_t i);
std::unique_ptr<Shape> compound_new();
void compound_add(Shape &c, const Shape &s);

MeshData mesh_shape(const Shape &s, double linear, double angular);
EdgeData edge_polylines(const Shape &s, double deflection);
rust::Vec<FaceInfo> face_infos(const Shape &s);
rust::Vec<EdgeInfo> edge_infos(const Shape &s);
MassProps mass_props(const Shape &s);
bool is_valid(const Shape &s);

void write_step(const Shape &s, rust::Str path);
std::unique_ptr<Shape> read_step(rust::Str path);
rust::String write_brep(const Shape &s);
std::unique_ptr<Shape> read_brep(rust::Str data);
std::unique_ptr<Shape> solidify(const Shape &s, double tol);
void write_iges(const Shape &s, rust::Str path);
std::unique_ptr<Shape> read_iges(rust::Str path);
std::unique_ptr<Shape> mesh_to_shape(rust::Slice<const double> pos, rust::Slice<const uint32_t> tris, bool merge);

// Second batch of solid features (features2.cpp).
std::unique_ptr<Shape> sweep_rail(const Shape &profile, const Shape &spine, const Shape &rail);
std::unique_ptr<Shape> sweep_twist(const Shape &profile, const Shape &spine, double twist);
std::unique_ptr<Shape> sweep_sections(const Shape &sections, const Shape &spine);
std::unique_ptr<Shape> loft_rails(const Shape &sections, const Shape &rails);
std::unique_ptr<Shape> emboss_tool(const Shape &body, uint32_t face, const Shape &profiles,
                                   rust::Slice<const double> dir, double depth, bool deboss, bool wrap);
std::unique_ptr<Shape> boundary_cells(const Shape &tools);
int32_t classify(const Shape &s, double x, double y, double z);
std::unique_ptr<Shape> plane_face(rust::Slice<const double> plane, double size);
std::unique_ptr<Shape> extended_face(const Shape &face, double size, double offset, rust::Slice<const double> at,
                                     rust::Slice<const double> toward);
double side_of(const Shape &face, double x, double y, double z);
std::unique_ptr<Shape> setback_corners(const Shape &s, rust::Slice<const double> points, double radius);
// sheet2.cpp
std::unique_ptr<Shape> ruled_bspline(rust::Slice<const double> a, rust::Slice<const double> b,
                                     rust::Slice<const double> u);

} // namespace fox
