// Surface workspace additions to the bridge: extend, untrim, trim classification and variable
// offset. Same rules as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "clf-kernel/cpp/kernel.hxx"

namespace clf {

std::unique_ptr<Shape> extend_edges(const Shape &s, rust::Slice<const uint32_t> edges, double d, int32_t mode);
std::unique_ptr<Shape> untrim_faces(const Shape &s, rust::Slice<const uint32_t> faces, int32_t loops);
double classify_point(const Shape &tool, double x, double y, double z);
std::unique_ptr<Shape> offset_variable(const Shape &s, double d, rust::Slice<const uint32_t> faces,
                                       rust::Slice<const double> dists);

} // namespace clf
