// Form (T-spline) geometry (cpp/form.cpp). Same rules as kernel.hxx: every entry point catches
// all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> grid_face(rust::Slice<const double> points, uint32_t rows, uint32_t cols);

} // namespace fox
