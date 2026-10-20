// Offsetting chosen faces of a solid (cpp/offsetfaces.cpp), as Fusion's Offset Face. Same rules
// as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> offset_faces(const Shape &s, rust::Slice<const uint32_t> faces, double d);

} // namespace fox
