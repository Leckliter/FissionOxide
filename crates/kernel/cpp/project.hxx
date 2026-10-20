// Project To Surface (cpp/project.cpp): sketch curves draped onto faces, and round pipes along
// the 3D wires that gives (closed loops too). Same rules as kernel.hxx: every entry point catches
// all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

std::unique_ptr<Shape> project_curves(const Shape &curves, const Shape &faces, rust::Slice<const double> dir);
std::unique_ptr<Shape> round_pipe(const Shape &spine, double radius, double inner);
rust::Vec<double> wire_ends(const Shape &wire);
size_t child_count(const Shape &s);
std::unique_ptr<Shape> child_at(const Shape &s, size_t i);

} // namespace fox
