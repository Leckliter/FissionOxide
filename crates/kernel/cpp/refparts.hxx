// Reference-part fixes (cpp/refparts.cpp): fillets with a radius law, and merging hole walls split
// by a tangent neighbour. Same rules as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> fillet_law(const Shape &s, rust::Slice<const uint32_t> edges, rust::Slice<const double> at,
                                  rust::Slice<const double> radii);
std::unique_ptr<Shape> unify_faces_internal(const Shape &s);

} // namespace fox
