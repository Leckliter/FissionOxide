// STEP with assembly structure, names and colours through OCCT's XCAF document model. Same rules
// as kernel.hxx: every entry point catches all kernel exceptions.
#pragma once

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

// An XCAF document being written, or a STEP file's parts after reading.
struct StepDoc {
  struct Impl;
  std::unique_ptr<Impl> impl;
  StepDoc();
  ~StepDoc();
};

std::unique_ptr<StepDoc> step_doc_new(rust::Str name);
void step_doc_add(StepDoc &d, rust::Str component, rust::Str name, const Shape &s, rust::Slice<const double> color);
void step_doc_write(StepDoc &d, rust::Str path);
std::unique_ptr<StepDoc> step_doc_read(rust::Str path);
size_t step_doc_len(const StepDoc &d);
rust::String step_doc_name(const StepDoc &d, size_t i);
rust::String step_doc_component(const StepDoc &d, size_t i);
rust::Vec<double> step_doc_color(const StepDoc &d, size_t i);
std::unique_ptr<Shape> step_doc_shape(const StepDoc &d, size_t i);

} // namespace fox
