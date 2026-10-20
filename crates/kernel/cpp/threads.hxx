// Threads (cpp/threads.cpp): merging the strips a helical cut leaves on a cylinder. Same rules as
// kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

#include <TopoDS_Shape.hxx>
#include <gp_Ax1.hxx>

namespace fox {

// Only faces on the cylinder of `radius` about `axis` (origin, direction) that lie within
// `lo`..`hi` along it take part, so other faces of the body keep their identity.
std::unique_ptr<Shape> merge_cylinder_strips(const Shape &s, rust::Slice<const double> axis, double radius, double lo,
                                             double hi);
// Whether a face of the shape runs past its surface's period in u (as a merged crest does), all
// such faces being cylinders about one axis (set to that axis).
bool wrapping_axis(const TopoDS_Shape &s, gp_Ax1 &axis);

} // namespace fox
