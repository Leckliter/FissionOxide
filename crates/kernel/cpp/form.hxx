// Form (T-spline / subdivision) bodies (cpp/form.cpp): B-spline faces from poles and knots.
// Same rules as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> bspline_face(uint32_t degree, uint32_t nu, uint32_t nv, rust::Slice<const double> poles,
                                    rust::Slice<const double> uknots, rust::Slice<const uint32_t> umults,
                                    rust::Slice<const double> vknots, rust::Slice<const uint32_t> vmults);

} // namespace fox
