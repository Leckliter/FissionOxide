// Spun profiles (cpp/spun.cpp): how far a body reaches from an axis along it, for the sketch's
// Spun Profile. Same rules as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

rust::Vec<double> axial_events(const Shape &s, rust::Slice<const double> axis);
rust::Vec<double> radial_slices(const Shape &s, rust::Slice<const double> axis, rust::Slice<const double> ts);

} // namespace fox
