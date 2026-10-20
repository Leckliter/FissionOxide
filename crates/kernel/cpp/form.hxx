// Form (T-spline / subdivision) bodies (cpp/form.cpp): B-spline faces from poles and knots, or
// through grids of points (Insert T-Spline).
// Same rules as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> bspline_face(uint32_t degree, uint32_t nu, uint32_t nv, rust::Slice<const double> poles,
                                    rust::Slice<const double> uknots, rust::Slice<const uint32_t> umults,
                                    rust::Slice<const double> vknots, rust::Slice<const uint32_t> vmults);
std::unique_ptr<Shape> grid_face(rust::Slice<const double> points, uint32_t rows, uint32_t cols);

} // namespace fox
