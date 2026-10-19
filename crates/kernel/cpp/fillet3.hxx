// Fillets whose radius follows a law along each edge (cpp/fillet3.cpp). Same rules as
// kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> fillet_law(const Shape &s, rust::Slice<const uint32_t> edges, rust::Slice<const double> at,
                                  rust::Slice<const double> radii);

} // namespace fox
