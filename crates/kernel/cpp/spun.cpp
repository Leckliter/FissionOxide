// Spun profiles: how far a body reaches from an axis along it. A spun profile is the region a body
// sweeps through in a half-plane bounded by the axis as it turns about the axis; at each station t
// along the axis it covers the distances from the axis that the body's cross-section there spans.
// The Rust side (fox-model's spun.rs) samples those spans and fits sketch curves to them.
#include "fox-kernel/cpp/spun.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepExtrema_ExtPC.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pln.hxx>

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

struct Axis {
  gp_Pnt o;
  gp_Dir d;

  double t(const gp_Pnt &p) const { return gp_Vec(o, p).Dot(gp_Vec(d)); }
  double r(const gp_Pnt &p) const {
    gp_Vec v(o, p);
    return (v - gp_Vec(d) * v.Dot(gp_Vec(d))).Magnitude();
  }
  gp_Pnt at(double t) const { return o.Translated(gp_Vec(d) * t); }
};

Axis axis_of(rust::Slice<const double> a) {
  if (a.size() < 6) throw std::runtime_error("an axis needs a point and a direction");
  gp_Vec v(a[3], a[4], a[5]);
  if (v.Magnitude() < 1e-12) throw std::runtime_error("the axis has no direction");
  return Axis{gp_Pnt(a[0], a[1], a[2]), gp_Dir(v)};
}

// Half the side of a square, centred on the axis, that covers every cross-section of the shape.
double reach(const TopoDS_Shape &s, const Axis &ax) {
  Bnd_Box box;
  BRepBndLib::Add(s, box);
  if (box.IsVoid()) throw std::runtime_error("the body is empty");
  double x0, y0, z0, x1, y1, z1;
  box.Get(x0, y0, z0, x1, y1, z1);
  double r = 0;
  for (int i = 0; i < 8; ++i) {
    gp_Pnt p(i & 1 ? x1 : x0, i & 2 ? y1 : y0, i & 4 ? z1 : z0);
    r = std::max(r, ax.r(p));
  }
  return r * 1.5 + 1.0;
}

// The square face across the axis at station t.
TopoDS_Face station(const Axis &ax, double t, double size) {
  gp_Pln pln(ax.at(t), ax.d);
  return BRepBuilderAPI_MakeFace(pln, -size, size, -size, size).Face();
}

// Stations along the axis where an edge turns back (its interior extremes of t).
void edge_turns(const TopoDS_Edge &e, const Axis &ax, std::vector<double> &out) {
  if (BRep_Tool::Degenerated(e)) return;
  BRepAdaptor_Curve c(e);
  const double f = c.FirstParameter(), l = c.LastParameter();
  if (!(l > f)) return;
  const int n = 48;
  std::vector<double> u(n + 1), t(n + 1);
  for (int i = 0; i <= n; ++i) {
    u[i] = f + (l - f) * i / n;
    t[i] = ax.t(c.Value(u[i]));
  }
  for (int i = 1; i < n; ++i) {
    const bool top = t[i] >= t[i - 1] && t[i] >= t[i + 1];
    const bool low = t[i] <= t[i - 1] && t[i] <= t[i + 1];
    if (!top && !low) continue;
    if (std::abs(t[i] - t[i - 1]) < 1e-12 && std::abs(t[i] - t[i + 1]) < 1e-12) continue; // flat run
    // golden-section search on the bracket for the extreme
    double a = u[i - 1], b = u[i + 1];
    const double g = (std::sqrt(5.0) - 1) / 2;
    auto val = [&](double x) { return top ? ax.t(c.Value(x)) : -ax.t(c.Value(x)); };
    double x1 = b - g * (b - a), x2 = a + g * (b - a);
    double f1 = val(x1), f2 = val(x2);
    for (int k = 0; k < 60 && b - a > 1e-12 * (l - f); ++k) {
      if (f1 < f2) {
        a = x1;
        x1 = x2;
        f1 = f2;
        x2 = a + g * (b - a);
        f2 = val(x2);
      } else {
        b = x2;
        x2 = x1;
        f2 = f1;
        x1 = b - g * (b - a);
        f1 = val(x1);
      }
    }
    out.push_back(ax.t(c.Value((a + b) / 2)));
  }
}

} // namespace

// Where things happen along the axis: [tmin, tmax, n, n stations, then (t, r) per vertex]. The
// stations are the vertices' and the edges' turning points; tmin and tmax are the body's exact
// extent along the axis.
rust::Vec<double> axial_events(const Shape &s, rust::Slice<const double> axis) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    const Axis ax = axis_of(axis);
    const double size = reach(s.s, ax);
    Bnd_Box box;
    BRepBndLib::Add(s.s, box);
    double x0, y0, z0, x1, y1, z1;
    box.Get(x0, y0, z0, x1, y1, z1);
    double lo = 1e300, hi = -1e300;
    for (int i = 0; i < 8; ++i) {
      const double t = ax.t(gp_Pnt(i & 1 ? x1 : x0, i & 2 ? y1 : y0, i & 4 ? z1 : z0));
      lo = std::min(lo, t);
      hi = std::max(hi, t);
    }
    lo -= 1.0;
    hi += 1.0;
    BRepExtrema_DistShapeShape below(station(ax, lo, size), s.s);
    BRepExtrema_DistShapeShape above(station(ax, hi, size), s.s);
    if (!below.IsDone() || !above.IsDone()) throw std::runtime_error("could not measure the body along the axis");
    const double tmin = lo + below.Value(), tmax = hi - above.Value();

    std::vector<double> stations, verts;
    TopTools_IndexedMapOfShape vmap, emap;
    TopExp::MapShapes(s.s, TopAbs_VERTEX, vmap);
    TopExp::MapShapes(s.s, TopAbs_EDGE, emap);
    for (int i = 1; i <= vmap.Extent(); ++i) {
      const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(vmap(i)));
      stations.push_back(ax.t(p));
      verts.push_back(ax.t(p));
      verts.push_back(ax.r(p));
    }
    for (int i = 1; i <= emap.Extent(); ++i) edge_turns(TopoDS::Edge(emap(i)), ax, stations);

    rust::Vec<double> out;
    out.push_back(tmin);
    out.push_back(tmax);
    out.push_back(static_cast<double>(stations.size()));
    for (double t : stations) out.push_back(t);
    for (double v : verts) out.push_back(v);
    return out;
  } catch (...) {
    fail("spun profile");
  }
}

// The body's cross-section at each station t: per station the number of section faces n, then n
// (nearest, farthest) distances from the axis.
rust::Vec<double> radial_slices(const Shape &s, rust::Slice<const double> axis, rust::Slice<const double> ts) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    const Axis ax = axis_of(axis);
    const double size = reach(s.s, ax);
    rust::Vec<double> out;
    for (double t : ts) {
      BRepAlgoAPI_Common common(s.s, station(ax, t, size));
      std::vector<double> spans;
      if (common.IsDone() && !common.HasErrors()) {
        const TopoDS_Vertex c = BRepBuilderAPI_MakeVertex(ax.at(t)).Vertex();
        for (TopExp_Explorer fx(common.Shape(), TopAbs_FACE); fx.More(); fx.Next()) {
          const TopoDS_Face &f = TopoDS::Face(fx.Current());
          BRepExtrema_DistShapeShape closest(c, f);
          if (!closest.IsDone()) continue;
          double farthest = 0;
          for (TopExp_Explorer vx(f, TopAbs_VERTEX); vx.More(); vx.Next())
            farthest = std::max(farthest, ax.r(BRep_Tool::Pnt(TopoDS::Vertex(vx.Current()))));
          for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More(); ex.Next()) {
            const TopoDS_Edge &e = TopoDS::Edge(ex.Current());
            if (BRep_Tool::Degenerated(e)) continue;
            BRepExtrema_ExtPC ext(c, e);
            if (!ext.IsDone()) continue;
            for (int k = 1; k <= ext.NbExt(); ++k) farthest = std::max(farthest, std::sqrt(ext.SquareDistance(k)));
          }
          spans.push_back(closest.Value());
          spans.push_back(std::max(farthest, closest.Value()));
        }
      }
      out.push_back(static_cast<double>(spans.size() / 2));
      for (double v : spans) out.push_back(v);
    }
    return out;
  } catch (...) {
    fail("spun profile");
  }
}

} // namespace fox
