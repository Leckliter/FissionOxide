// STEP with assembly structure, names and colours through OCCT's XCAF document model. Same rules
// as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

// An XCAF document being written, or a STEP file's assembly tree after reading.
struct StepDoc {
  struct Impl;
  std::unique_ptr<Impl> impl;
  StepDoc();
  ~StepDoc();
};

// Writing: labels are numbered, 0 is the top assembly.
std::unique_ptr<StepDoc> step_doc_new(rust::Str name);
size_t step_doc_assembly(StepDoc &d, rust::Str name);
size_t step_doc_part(StepDoc &d, rust::Str name, const Shape &s, rust::Slice<const double> color,
                     rust::Slice<const double> face_colors);
void step_doc_place(StepDoc &d, size_t parent, size_t child, rust::Str name, rust::Slice<const double> at);
void step_doc_write(StepDoc &d, rust::Str path);

// Reading: the occurrences of the file's assembly tree, parents before children.
std::unique_ptr<StepDoc> step_doc_read(rust::Str path);
size_t step_doc_len(const StepDoc &d);
int64_t step_doc_parent(const StepDoc &d, size_t i);
bool step_doc_is_assembly(const StepDoc &d, size_t i);
rust::String step_doc_name(const StepDoc &d, size_t i);
rust::String step_doc_instance(const StepDoc &d, size_t i);
size_t step_doc_proto(const StepDoc &d, size_t i);
size_t step_doc_uses(const StepDoc &d, size_t i);
rust::Vec<double> step_doc_placement(const StepDoc &d, size_t i);
rust::Vec<double> step_doc_color(const StepDoc &d, size_t i);
std::unique_ptr<Shape> step_doc_shape(const StepDoc &d, size_t i);
rust::Vec<double> step_doc_face_colors(const StepDoc &d, size_t i, const Shape &body);

} // namespace fox
