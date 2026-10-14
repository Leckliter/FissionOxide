// C++ side of the CLFusion kernel bridge to OpenCASCADE.
//
// Every entry point catches *all* exceptions (OCCT's Standard_Failure does not derive from
// std::exception) and rethrows std::runtime_error, which cxx turns into a Rust Err. Nothing that
// goes wrong inside the kernel may abort the process.
#pragma once

#include <memory>
#include <TopoDS_Shape.hxx>
#include "rust/cxx.h"

namespace clf {

struct Shape {
  TopoDS_Shape s;
};

struct MeshData;
struct EdgeData;
struct FaceInfo;
struct EdgeInfo;
struct MassProps;

std::unique_ptr<Shape> shape_clone(const Shape &s);
bool shape_is_null(const Shape &s);
int32_t shape_type(const Shape &s);

std::unique_ptr<Shape> make_face(rust::Slice<const double> plane, rust::Slice<const double> segs,
                                 rust::Slice<const uint32_t> loop_lens);
std::unique_ptr<Shape> make_prism(const Shape &face, double dx, double dy, double dz);
std::unique_ptr<Shape> make_revol(const Shape &face, rust::Slice<const double> axis, double angle);
std::unique_ptr<Shape> make_box(double x0, double y0, double z0, double x1, double y1, double z1);
std::unique_ptr<Shape> make_cylinder(rust::Slice<const double> axis, double radius, double height);
std::unique_ptr<Shape> make_sphere(double cx, double cy, double cz, double radius);
std::unique_ptr<Shape> make_torus(rust::Slice<const double> axis, double major, double minor);

std::unique_ptr<Shape> boolean_op(const Shape &a, const Shape &b, int32_t op, double fuzzy);
std::unique_ptr<Shape> fillet(const Shape &s, rust::Slice<const uint32_t> edges, double radius);
std::unique_ptr<Shape> chamfer(const Shape &s, rust::Slice<const uint32_t> edges, double dist);
std::unique_ptr<Shape> shell(const Shape &s, rust::Slice<const uint32_t> faces, double thickness);
std::unique_ptr<Shape> transform(const Shape &s, rust::Slice<const double> m);
std::unique_ptr<Shape> mirror(const Shape &s, rust::Slice<const double> plane);
std::unique_ptr<Shape> unify(const Shape &s);

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

} // namespace clf
