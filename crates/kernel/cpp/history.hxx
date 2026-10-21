// Modification history (cpp/history.cpp): labels on a shape's faces and edges, carried through
// each kernel call by the algorithms' own history (BRepBuilderAPI_MakeShape's Generated, Modified
// and IsDeleted, BOPAlgo's and UnifySameDomain's BRepTools_History). The model names faces from
// it, so references follow the faces they were put on instead of being re-found by position.
#pragma once

#include <cstdint>
#include <initializer_list>
#include <memory>
#include <vector>

#include <BRepTools_History.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS_Shape.hxx>

#include "fox-kernel/cpp/kernel.hxx"

namespace fox {

struct LabelData;

// One labelled face or edge of a shape a history starts from, and what became of it.
struct ProvEntry {
  uint64_t label = 0;
  // Faces and edges of the current shape modified from it (or it, unchanged).
  TopTools_ListOfShape mod;
  // Faces generated from it (a fillet from an edge, the far cap of an extrusion).
  TopTools_ListOfShape gen;
};

struct Prov {
  std::vector<ProvEntry> entries;
};

// Builds a kernel call's history step by step: start from the inputs' labels, apply each
// algorithm the call runs, then attach the result to the output shape. A step that cannot be
// followed drops the history (the model then matches faces by identity and geometry).
class Track {
public:
  explicit Track(std::initializer_list<const Shape *> inputs);
  // An algorithm with Modified/Generated/IsDeleted, run on `args`.
  template <class Algo> void step(const TopoDS_Shape &arg, Algo &mk) {
    if (!live()) return;
    try {
      TopTools_ListOfShape args;
      args.Append(arg);
      BRepTools_History h(args, mk);
      apply(h);
    } catch (...) {
      broken = true;
    }
  }
  template <class Algo> void step(const TopTools_ListOfShape &args, Algo &mk) {
    if (!live()) return;
    try {
      BRepTools_History h(args, mk);
      apply(h);
    } catch (...) {
      broken = true;
    }
  }
  void step(const Handle(BRepTools_History) & h);
  // Records `to` as modified (or generated) from `from`, where the algorithm doesn't say (the caps
  // of a sweep): every entry that tracked `from` before the last step gets it.
  void relate(const TopoDS_Shape &from, const TopoDS_Shape &to, bool generated);
  // Nothing to follow: no input carried labels, or a step failed.
  bool live() const { return !broken && !p.entries.empty(); }
  void attach(Shape &out) const;

private:
  void apply(const BRepTools_History &h);
  Prov p;
  std::vector<TopTools_ListOfShape> before;
  bool broken = false;
};

// History-carrying copy of a shape result: `out` gets `in`'s labels (a sub-shape of a shape keeps
// its faces).
void keep_history(Shape &out, const Shape &in);

// Labels faces (pairs of 1-based id and label; a face may have several) and edges (by 1-based
// index; empty: each edge between two labelled faces gets a label made from theirs).
void set_labels(Shape &s, rust::Slice<const uint32_t> face_ids, rust::Slice<const uint64_t> face_labels,
                rust::Slice<const uint64_t> edges);
bool has_labels(const Shape &s);
LabelData face_labels(const Shape &s);
rust::Vec<uint32_t> same_faces(const Shape &now, const Shape &before);

} // namespace fox
