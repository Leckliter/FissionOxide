// A body's silhouette on a plane (cpp/silhouette.cpp), for projecting a body into a sketch. Same
// rules as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> silhouette(const Shape &s, rust::Slice<const double> plane);

} // namespace fox
