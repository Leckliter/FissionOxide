// 3D curves (cpp/curves3d.cpp): where shapes cross, chained into wires (intersection curves), and
// the start of a wire. Same rules as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> section_wires(const Shape &a, const Shape &b, double tolerance);
rust::Vec<double> wire_start(const Shape &w);
size_t wire_count(const Shape &s);
std::unique_ptr<Shape> wire_at(const Shape &s, size_t i);
std::unique_ptr<Shape> wire_from_middle(const Shape &w);

} // namespace fox
