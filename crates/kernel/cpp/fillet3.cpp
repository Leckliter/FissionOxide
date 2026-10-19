// Fillets whose radius follows a law along each edge: Fusion's variable-radius fillet, with radii
// given at points along the edge, including radii that run down to nothing.
#include "fox-kernel/cpp/fillet3.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <algorithm>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <ShapeFix_Shape.hxx>
#include <Standard_Failure.hxx>
#include <TColgp_Array1OfPnt2d.hxx>
#include <TopExp.hxx>
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

} // namespace fox
