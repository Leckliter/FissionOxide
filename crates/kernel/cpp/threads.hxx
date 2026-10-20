// Threads (cpp/threads.cpp): merging the strips a helical cut leaves on a cylinder. Same rules as
// kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

#include <TopoDS_Shape.hxx>
#include <gp_Ax1.hxx>

namespace fox {

std::unique_ptr<Shape> merge_cylinder_strips(const Shape &s);
// Whether a face of the shape runs past its surface's period in u (as a merged crest does), all
// such faces being cylinders about one axis (set to that axis).
bool wrapping_axis(const TopoDS_Shape &s, gp_Ax1 &axis);

} // namespace fox
