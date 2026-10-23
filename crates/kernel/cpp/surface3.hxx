// Fusion's surface Patch (continuity and interior rails), Ruled (from one edge), Sweep and Loft
// (open surfaces from curves). Same rules as kernel.hxx: every entry point catches all kernel
// exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> patch_with(const Shape &boundary, const Shape &supports, rust::Slice<const uint8_t> orders,
                                  const Shape &interior);
std::unique_ptr<Shape> ruled_from_edge(const Shape &edge, const Shape &face, double distance, double angle,
                                       bool tangent);
std::unique_ptr<Shape> sweep_open(const Shape &profile, const Shape &spine, bool parallel);
std::unique_ptr<Shape> loft_open(const Shape &sections, bool closed);
std::unique_ptr<Shape> loft_open_rails(const Shape &sections, const Shape &rails);
std::unique_ptr<Shape> loft_open_centerline(const Shape &sections, const Shape &centerline);

} // namespace fox
