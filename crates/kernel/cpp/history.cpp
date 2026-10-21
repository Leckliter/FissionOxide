// Modification history: labels on faces and edges, followed through each kernel call (see
// history.hxx). Same rules as kernel.hxx: entry points catch every kernel exception.
#include "fox-kernel/cpp/history.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <stdexcept>
#include <string>

#include <BRep_Tool.hxx>
#include <Geom_Surface.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>

namespace fox {

namespace {

bool tracked_type(const TopoDS_Shape &s) {
  return !s.IsNull() && (s.ShapeType() == TopAbs_FACE || s.ShapeType() == TopAbs_EDGE);
}

[[noreturn]] void fail(const char *what) {
  try {
    throw;
  } catch (const Standard_Failure &e) {
    const char *msg = e.GetMessageString();
    throw std::runtime_error(std::string(what) + ": " + (msg && *msg ? msg : e.DynamicType()->Name()));
  } catch (const std::exception &e) {
    throw std::runtime_error(std::string(what) + ": " + e.what());
  } catch (...) {
    throw std::runtime_error(std::string(what) + ": unknown kernel error");
  }
}

} // namespace

Track::Track(std::initializer_list<const Shape *> inputs) {
  for (const Shape *in : inputs) {
    if (in && in->prov) p.entries.insert(p.entries.end(), in->prov->entries.begin(), in->prov->entries.end());
  }
}

void Track::apply(const BRepTools_History &h) {
  // what a shape of the previous step became: removed, its modified images, or itself
  auto image = [&](const TopoDS_Shape &v, TopTools_ListOfShape &out) {
    if (h.IsRemoved(v)) return;
    const TopTools_ListOfShape &m = h.Modified(v);
    if (m.IsEmpty()) {
      out.Append(v);
      return;
    }
    for (TopTools_ListIteratorOfListOfShape it(m); it.More(); it.Next()) {
      if (tracked_type(it.Value())) out.Append(it.Value());
    }
  };
  before.clear();
  before.reserve(p.entries.size());
  for (ProvEntry &e : p.entries) {
    before.push_back(e.mod);
    TopTools_ListOfShape mod, gen;
    for (TopTools_ListIteratorOfListOfShape it(e.mod); it.More(); it.Next()) {
      image(it.Value(), mod);
      const TopTools_ListOfShape &g = h.Generated(it.Value());
      for (TopTools_ListIteratorOfListOfShape jt(g); jt.More(); jt.Next()) {
        if (!jt.Value().IsNull() && jt.Value().ShapeType() == TopAbs_FACE) gen.Append(jt.Value());
      }
    }
    for (TopTools_ListIteratorOfListOfShape it(e.gen); it.More(); it.Next()) image(it.Value(), gen);
    e.mod = mod;
    e.gen = gen;
  }
}

void Track::step(const Handle(BRepTools_History) & h) {
  if (!live()) return;
  if (h.IsNull()) {
    broken = true;
    return;
  }
  try {
    apply(*h);
  } catch (...) {
    broken = true;
  }
}

void Track::relate(const TopoDS_Shape &from, const TopoDS_Shape &to, bool generated) {
  if (!live() || from.IsNull() || to.IsNull() || before.size() != p.entries.size()) return;
  for (size_t i = 0; i < p.entries.size(); ++i) {
    bool had = false;
    for (TopTools_ListIteratorOfListOfShape it(before[i]); it.More() && !had; it.Next()) had = it.Value().IsSame(from);
    if (had) (generated ? p.entries[i].gen : p.entries[i].mod).Append(to);
  }
}

void Track::attach(Shape &out) const {
  if (!live()) {
    out.prov.reset();
    return;
  }
  auto prov = std::make_shared<Prov>();
  for (const ProvEntry &e : p.entries) {
    if (!e.mod.IsEmpty() || !e.gen.IsEmpty()) prov->entries.push_back(e);
  }
  out.prov = std::move(prov);
}

void keep_history(Shape &out, const Shape &in) { out.prov = in.prov; }

// FNV-1a over the words: the label of an edge between two labelled faces. Labels end up in saved
// references, so this must never change.
uint64_t mix_labels(uint64_t a, uint64_t b) {
  uint64_t h = 0xcbf29ce484222325ULL;
  for (uint64_t w : {uint64_t{0x65646765}, a < b ? a : b, a < b ? b : a}) {
    for (int k = 0; k < 8; ++k) {
      h ^= (w >> (8 * k)) & 0xff;
      h *= 0x100000001b3ULL;
    }
  }
  return h == 0 ? 1 : h;
}

void set_labels(Shape &s, rust::Slice<const uint32_t> face_ids, rust::Slice<const uint64_t> face_labels,
                rust::Slice<const uint64_t> edges) {
  try {
    auto prov = std::make_shared<Prov>();
    TopTools_IndexedMapOfShape fmap, emap;
    TopExp::MapShapes(s.s, TopAbs_FACE, fmap);
    TopExp::MapShapes(s.s, TopAbs_EDGE, emap);
    // the smallest label of each face names the edges along it
    std::vector<uint64_t> first(static_cast<size_t>(fmap.Extent()), 0);
    for (size_t k = 0; k < face_ids.size() && k < face_labels.size(); ++k) {
      uint32_t i = face_ids[k];
      uint64_t l = face_labels[k];
      if (i < 1 || static_cast<int>(i) > fmap.Extent() || l == 0) continue;
      ProvEntry e;
      e.label = l;
      e.mod.Append(fmap(static_cast<int>(i)));
      prov->entries.push_back(std::move(e));
      uint64_t &f = first[i - 1];
      if (f == 0 || l < f) f = l;
    }
    TopTools_IndexedDataMapOfShapeListOfShape ef;
    if (edges.empty()) TopExp::MapShapesAndUniqueAncestors(s.s, TopAbs_EDGE, TopAbs_FACE, ef);
    for (int i = 1; i <= emap.Extent(); ++i) {
      uint64_t l = 0;
      if (!edges.empty()) {
        if (static_cast<size_t>(i) <= edges.size()) l = edges[static_cast<size_t>(i - 1)];
      } else if (ef.Contains(emap(i))) {
        const TopTools_ListOfShape &faces = ef.FindFromKey(emap(i));
        uint64_t a = 0, b = 0;
        int n = 0;
        for (TopTools_ListIteratorOfListOfShape it(faces); it.More(); it.Next(), ++n) {
          int k = fmap.FindIndex(it.Value());
          uint64_t v = k > 0 ? first[static_cast<size_t>(k - 1)] : 0;
          (n == 0 ? a : b) = v;
        }
        if (n == 1) b = a;
        if (a != 0 && b != 0) l = mix_labels(a, b);
      }
      if (l == 0) continue;
      ProvEntry e;
      e.label = l;
      e.mod.Append(emap(i));
      prov->entries.push_back(std::move(e));
    }
    s.prov = std::move(prov);
  } catch (...) {
    fail("labels");
  }
}

bool has_labels(const Shape &s) { return s.prov != nullptr; }

LabelData face_labels(const Shape &s) {
  try {
    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(s.s, TopAbs_FACE, map);
    std::vector<std::vector<std::pair<uint64_t, uint8_t>>> per(static_cast<size_t>(map.Extent()));
    if (s.prov) {
      for (const ProvEntry &e : s.prov->entries) {
        auto put = [&](const TopTools_ListOfShape &l, uint8_t generated) {
          for (TopTools_ListIteratorOfListOfShape it(l); it.More(); it.Next()) {
            if (it.Value().ShapeType() != TopAbs_FACE) continue;
            int i = map.FindIndex(it.Value());
            if (i > 0) per[static_cast<size_t>(i - 1)].emplace_back(e.label, generated);
          }
        };
        put(e.mod, 0);
        put(e.gen, 1);
      }
    }
    LabelData out;
    out.offsets.push_back(0);
    for (const auto &f : per) {
      for (const auto &[l, g] : f) {
        out.labels.push_back(l);
        out.generated.push_back(g);
      }
      out.offsets.push_back(static_cast<uint32_t>(out.labels.size()));
    }
    return out;
  } catch (...) {
    fail("labels");
  }
}

// Per face of `now`, the face of `before` it is (the same face, or one on the same surface), 1-based;
// 0 for none. For kernel calls that keep no history.
rust::Vec<uint32_t> same_faces(const Shape &now, const Shape &before) {
  try {
    TopTools_IndexedMapOfShape a, b;
    TopExp::MapShapes(now.s, TopAbs_FACE, a);
    TopExp::MapShapes(before.s, TopAbs_FACE, b);
    rust::Vec<uint32_t> out;
    for (int i = 1; i <= a.Extent(); ++i) {
      int j = b.FindIndex(a(i));
      if (j == 0) {
        TopLoc_Location la;
        Handle(Geom_Surface) sa = BRep_Tool::Surface(TopoDS::Face(a(i)), la);
        for (int k = 1; k <= b.Extent() && !sa.IsNull(); ++k) {
          TopLoc_Location lb;
          Handle(Geom_Surface) sb = BRep_Tool::Surface(TopoDS::Face(b(k)), lb);
          if (sb == sa && lb.IsEqual(la)) {
            j = k;
            break;
          }
        }
      }
      out.push_back(static_cast<uint32_t>(j));
    }
    return out;
  } catch (...) {
    fail("same faces");
  }
}

} // namespace fox
