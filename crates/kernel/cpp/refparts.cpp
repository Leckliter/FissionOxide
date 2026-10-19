// Reference-part fixes: fillets whose radius follows a law along each edge (Fusion's
// variable-radius fillet, with radii at points along the edge, down to nothing), and merging hole
// walls that a tangent neighbour split in two.
#include "fox-kernel/cpp/refparts.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <algorithm>
#include <mutex>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Wire.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <ShapeFix_Shape.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TColgp_Array1OfPnt2d.hxx>
#include <TopExp.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>

namespace fox {

namespace {

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

std::unique_ptr<Shape> valid_shape(const TopoDS_Shape &s, const char *what) {
  if (s.IsNull()) throw std::runtime_error(std::string(what) + " produced nothing");
  TopoDS_Shape r = s;
  if (!BRepCheck_Analyzer(r).IsValid()) {
    ShapeFix_Shape fix(r);
    fix.Perform();
    r = fix.Shape();
    if (!BRepCheck_Analyzer(r).IsValid()) throw std::runtime_error(std::string(what) + " produced an invalid solid");
  }
  auto out = std::make_unique<Shape>();
  out->s = r;
  return out;
}

} // namespace

// Fillets edges (1-based) with a radius law: radii[i] at the fraction at[i] along each edge (0 at
// its start, 1 at its end), interpolated smoothly in between. OpenCASCADE needs a radius above
// zero everywhere, so a radius of zero becomes a hair-thin round (a thousandth of the largest).
std::unique_ptr<Shape> fillet_law(const Shape &s, rust::Slice<const uint32_t> edges, rust::Slice<const double> at,
                                  rust::Slice<const double> radii) {
  // OpenCASCADE's fillet code keeps some state in statics, and a radius law built from points
  // fails now and then when two threads fillet at once; one law fillet at a time.
  static std::mutex lock;
  std::lock_guard<std::mutex> hold(lock);
  try {
    if (at.size() != radii.size() || at.size() < 2) throw std::runtime_error("a radius law needs two or more points");
    std::vector<std::pair<double, double>> pts;
    double rmax = 0;
    for (size_t i = 0; i < at.size(); ++i) {
      pts.emplace_back(std::clamp(at[i], 0.0, 1.0), radii[i]);
      rmax = std::max(rmax, radii[i]);
    }
    if (!(rmax > 0)) throw std::runtime_error("fillet radii must not all be zero");
    std::sort(pts.begin(), pts.end());
    const double rmin = rmax * 1e-3;
    std::vector<std::pair<double, double>> law;
    for (auto p : pts) {
      if (p.second < 0) throw std::runtime_error("fillet radii must not be negative");
      p.second = std::max(p.second, rmin);
      if (!law.empty() && p.first - law.back().first < 1e-9) law.back() = p;
      else law.push_back(p);
    }
    if (law.size() < 2) throw std::runtime_error("the radius points are all at one place");
    if (law.front().first > 1e-9) law.insert(law.begin(), {0.0, law.front().second});
    if (law.back().first < 1 - 1e-9) law.push_back({1.0, law.back().second});
    TColgp_Array1OfPnt2d uandr(1, static_cast<int>(law.size()));
    for (size_t i = 0; i < law.size(); ++i) uandr.SetValue(static_cast<int>(i) + 1, gp_Pnt2d(law[i].first, law[i].second));

    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(s.s, TopAbs_EDGE, map);
    BRepFilletAPI_MakeFillet mk(s.s);
    int added = 0;
    for (uint32_t i : edges) {
      if (i < 1 || static_cast<int>(i) > map.Extent()) continue;
      const TopoDS_Edge &e = TopoDS::Edge(map(static_cast<int>(i)));
      mk.Add(e);
      const int ic = mk.Contour(e);
      if (ic == 0) continue;
      for (int j = 1; j <= mk.NbEdges(ic); ++j) {
        if (mk.Edge(ic, j).IsSame(e)) {
          mk.SetRadius(uandr, ic, j);
          ++added;
        }
      }
    }
    if (added == 0) throw std::runtime_error("no edges to fillet");
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("the fillet could not be computed (radius too large?)");
    return valid_shape(mk.Shape(), "fillet");
  } catch (...) {
    fail("variable fillet");
  }
}

// Merges faces lying on one surface across edges that more than two faces share: where a hole is
// exactly tangent to another, the plate between them is a line, and the boolean splits each hole
// wall there. The edge stays with the faces that need it (a seam) and the merged wall just touches
// it. Shapes without such edges, or where the merge doesn't give a valid solid, come back as they
// were, so ordinary results keep their faces.
std::unique_ptr<Shape> unify_faces_internal(const Shape &s) {
  auto out = std::make_unique<Shape>();
  out->s = s.s;
  try {
    TopTools_IndexedDataMapOfShapeListOfShape ef;
    TopExp::MapShapesAndUniqueAncestors(s.s, TopAbs_EDGE, TopAbs_FACE, ef);
    bool shared = false;
    for (int i = 1; i <= ef.Extent() && !shared; ++i) shared = ef(i).Extent() > 2;
    if (!shared) return out;
    ShapeUpgrade_UnifySameDomain u(s.s, Standard_True, Standard_True, Standard_False);
    u.AllowInternalEdges(Standard_True);
    u.Build();
    TopoDS_Shape merged = u.Shape();
    // merged faces keep such edges as wires of internal edges; the faces only touch there
    BRepTools_ReShape reshape;
    BRep_Builder bb;
    bool changed = false;
    for (TopExp_Explorer fx(merged, TopAbs_FACE); fx.More(); fx.Next()) {
      const TopoDS_Face &f = TopoDS::Face(fx.Current());
      bool internal = false;
      for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More() && !internal; ex.Next()) {
        internal = ex.Current().Orientation() == TopAbs_INTERNAL;
      }
      if (!internal) continue;
      TopoDS_Face nf = TopoDS::Face(f.EmptyCopied().Oriented(TopAbs_FORWARD));
      for (TopoDS_Iterator wi(f, Standard_False); wi.More(); wi.Next()) {
        bool all_internal = true;
        for (TopoDS_Iterator ei(wi.Value(), Standard_False); ei.More() && all_internal; ei.Next()) {
          all_internal = ei.Value().Orientation() == TopAbs_INTERNAL;
        }
        if (!all_internal) bb.Add(nf, wi.Value());
      }
      nf.Orientation(f.Orientation());
      reshape.Replace(f, nf);
      changed = true;
    }
    if (changed) merged = reshape.Apply(merged);
    if (BRepCheck_Analyzer(merged).IsValid()) out->s = merged;
  } catch (...) {
    // keep the shape as it was
  }
  return out;
}

} // namespace fox
