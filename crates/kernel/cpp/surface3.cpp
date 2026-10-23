// Fusion's surface Patch with a continuity per boundary edge and interior rails, its Ruled
// surface grown from an edge along (or across) the face beside it, and its Sweep and Loft, which
// make open surfaces from curves where the solid commands make solids from profiles.
#include "fox-kernel/cpp/surface3.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_CompCurve.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepFill.hxx>
#include <BRepFill_Filling.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_UniformAbscissa.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomLProp_SLProps.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Surface.hxx>
#include <Standard_Failure.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>

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

std::unique_ptr<Shape> out(const TopoDS_Shape &s) {
  if (s.IsNull()) throw std::runtime_error("nothing was made");
  auto r = std::make_unique<Shape>();
  r->s = s;
  return r;
}

GeomAbs_Shape order_of(uint8_t o) {
  switch (o) {
    case 1:
      return GeomAbs_G1;
    case 2:
      return GeomAbs_G2;
    default:
      return GeomAbs_C0;
  }
}

// A wire from a wire, an edge or the edges of a compound.
TopoDS_Wire as_wire(const TopoDS_Shape &s) {
  if (s.ShapeType() == TopAbs_WIRE) return TopoDS::Wire(s);
  BRepBuilderAPI_MakeWire mw;
  for (TopExp_Explorer ex(s, TopAbs_EDGE); ex.More(); ex.Next()) mw.Add(TopoDS::Edge(ex.Current()));
  if (!mw.IsDone()) throw std::runtime_error("the curves don't join end to end");
  return mw.Wire();
}

} // namespace

// boundary: a compound of the boundary edges, in order; orders: one per edge (0 connected,
// 1 tangent, 2 curvature); supports: a compound holding, in order, the face beside each edge
// whose order isn't 0; interior: a compound of curves the patch passes through (may be empty).
std::unique_ptr<Shape> patch_with(const Shape &boundary, const Shape &supports, rust::Slice<const uint8_t> orders,
                                  const Shape &interior) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    std::vector<TopoDS_Face> faces;
    for (TopoDS_Iterator it(supports.s); it.More(); it.Next()) {
      if (it.Value().ShapeType() == TopAbs_FACE) faces.push_back(TopoDS::Face(it.Value()));
    }
    BRepFill_Filling fill;
    size_t n = 0, k = 0;
    for (TopExp_Explorer ex(boundary.s, TopAbs_EDGE); ex.More(); ex.Next(), ++n) {
      const TopoDS_Edge &e = TopoDS::Edge(ex.Current());
      uint8_t o = n < orders.size() ? orders[n] : 0;
      if (o > 0 && k < faces.size()) {
        fill.Add(e, faces[k++], order_of(o), Standard_True);
      } else {
        fill.Add(e, GeomAbs_C0, Standard_True);
      }
    }
    if (n == 0) throw std::runtime_error("the boundary has no edges");
    for (TopExp_Explorer ex(interior.s, TopAbs_EDGE); ex.More(); ex.Next()) {
      fill.Add(TopoDS::Edge(ex.Current()), GeomAbs_C0, Standard_False);
    }
    fill.Build();
    if (!fill.IsDone()) throw std::runtime_error("the patch could not be computed");
    return out(fill.Face());
  } catch (...) {
    fail("patch");
  }
}

// A ruled surface from `edge` (on `face`) `distance` out along the face normal (or, `tangent`,
// along the face away from it), turned `angle` (radians) about the edge.
std::unique_ptr<Shape> ruled_from_edge(const Shape &edge, const Shape &face, double distance, double angle,
                                       bool tangent) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    if (edge.s.ShapeType() != TopAbs_EDGE || face.s.ShapeType() != TopAbs_FACE) {
      throw std::runtime_error("pick an edge of a face");
    }
    if (std::abs(distance) < 1e-9) throw std::runtime_error("the distance must not be zero");
    TopoDS_Edge e = TopoDS::Edge(edge.s.Oriented(TopAbs_FORWARD));
    const TopoDS_Face &f = TopoDS::Face(face.s);
    BRepAdaptor_Curve c(e);
    Handle(Geom_Surface) surf = BRep_Tool::Surface(f);
    double pf = 0, pl = 0;
    Handle(Geom2d_Curve) pc = BRep_Tool::CurveOnSurface(e, f, pf, pl);
    bool reversed = f.Orientation() == TopAbs_REVERSED;
    // a point inside the face, to tell which way is away from it
    double u0, u1, v0, v1;
    BRepTools::UVBounds(f, u0, u1, v0, v1);
    gp_Pnt inside = surf->Value((u0 + u1) / 2, (v0 + v1) / 2);
    double a = c.FirstParameter(), b = c.LastParameter();
    bool closed = c.Value(a).Distance(c.Value(b)) < 1e-7;
    int n = c.GetType() == GeomAbs_Line ? 2 : 33;
    // the direction out of the face at parameter t, before turning by the angle
    auto frame = [&](double t, gp_Pnt &p, gp_Vec &d, gp_Vec &tan) {
      gp_Vec d1;
      c.D1(t, p, d1);
      tan = d1.Normalized();
      double su, sv;
      if (!pc.IsNull()) {
        gp_Pnt2d uv = pc->Value(t);
        su = uv.X();
        sv = uv.Y();
      } else {
        GeomAPI_ProjectPointOnSurf proj(p, surf);
        if (proj.NbPoints() == 0) throw std::runtime_error("the edge is not on the face");
        proj.LowerDistanceParameters(su, sv);
      }
      GeomLProp_SLProps props(surf, su, sv, 1, 1e-9);
      if (!props.IsNormalDefined()) throw std::runtime_error("the face has no normal at the edge");
      gp_Vec nrm(props.Normal());
      if (reversed) nrm.Reverse();
      d = tangent ? tan.Crossed(nrm) : nrm;
    };
    // tangent: the side away from the face, decided at the middle and kept along the edge
    double side = 1.0;
    if (tangent) {
      gp_Pnt p;
      gp_Vec d, t;
      frame((a + b) / 2, p, d, t);
      if (d.Dot(gp_Vec(p, inside)) > 0) side = -1.0;
    }
    int count = closed ? n - 1 : n;
    Handle(TColgp_HArray1OfPnt) pts = new TColgp_HArray1OfPnt(1, count);
    for (int i = 0; i < count; ++i) {
      double t = a + (b - a) * i / (n - 1);
      gp_Pnt p;
      gp_Vec d, tan;
      frame(t, p, d, tan);
      d *= side;
      // turned about the edge: d stays at right angles to it
      gp_Vec turned = d * std::cos(angle) + tan.Crossed(d) * std::sin(angle);
      pts->SetValue(i + 1, p.Translated(turned * distance));
    }
    GeomAPI_Interpolate interp(pts, closed ? Standard_True : Standard_False, 1e-7);
    interp.Perform();
    if (!interp.IsDone()) throw std::runtime_error("the far edge could not be built");
    TopoDS_Edge far_edge = BRepBuilderAPI_MakeEdge(interp.Curve()).Edge();
    TopoDS_Face r = BRepFill::Face(e, far_edge);
    return out(r);
  } catch (...) {
    fail("ruled surface");
  }
}

// An open surface: `profile` (a wire, edge or compound of edges) swept along `spine` (a wire),
// square to it (or `parallel`: keeping its orientation), mitred at the path's corners.
std::unique_ptr<Shape> sweep_open(const Shape &profile, const Shape &spine, bool parallel) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    TopoDS_Wire path = as_wire(spine.s);
    TopoDS_Wire prof = as_wire(profile.s);
    BRepOffsetAPI_MakePipeShell ps(path);
    if (parallel) {
      ps.SetMode(gp_Ax2(gp::Origin(), gp::DZ(), gp::DX()));
    } else {
      ps.SetMode(Standard_False);
    }
    ps.SetTransitionMode(BRepBuilderAPI_RightCorner);
    ps.Add(prof, Standard_False, Standard_False);
    ps.Build();
    if (!ps.IsDone()) throw std::runtime_error("the sweep failed");
    return out(ps.Shape());
  } catch (...) {
    fail("sweep");
  }
}

// An open surface through the wires (or edges) of `sections`, in order; `closed` loops back to
// the first.
std::unique_ptr<Shape> loft_open(const Shape &sections, bool closed) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    BRepOffsetAPI_ThruSections mk(Standard_False, Standard_False, 1e-6);
    int n = 0;
    TopoDS_Wire first;
    for (TopoDS_Iterator it(sections.s); it.More(); it.Next(), ++n) {
      TopoDS_Wire w = as_wire(it.Value());
      if (n == 0) first = w;
      mk.AddWire(w);
    }
    if (n < 2) throw std::runtime_error("a loft needs at least two profiles");
    if (closed) mk.AddWire(first);
    mk.CheckCompatibility(Standard_True);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("the loft failed (are the profiles compatible?)");
    return out(mk.Shape());
  } catch (...) {
    fail("loft");
  }
}

namespace {

// Points at equal arc lengths along a wire.
std::vector<gp_Pnt> sample(const TopoDS_Wire &w, int n) {
  BRepAdaptor_CompCurve c(w);
  GCPnts_UniformAbscissa ua(c, n);
  if (!ua.IsDone() || ua.NbPoints() < 2) throw std::runtime_error("a curve could not be sampled");
  std::vector<gp_Pnt> p;
  for (int i = 1; i <= ua.NbPoints(); ++i) p.push_back(c.Value(ua.Parameter(i)));
  return p;
}

// A polyline with the arc length at each point.
struct Poly {
  std::vector<gp_Pnt> p;
  std::vector<double> s;
  explicit Poly(std::vector<gp_Pnt> pts) : p(std::move(pts)) {
    s.push_back(0);
    for (size_t i = 1; i < p.size(); ++i) s.push_back(s.back() + p[i].Distance(p[i - 1]));
  }
  // the nearest point's arc length and distance
  std::pair<double, double> nearest(const gp_Pnt &q) const {
    double best = 1e300, at = 0;
    for (size_t i = 0; i + 1 < p.size(); ++i) {
      gp_Vec ab(p[i], p[i + 1]);
      double l2 = ab.SquareMagnitude();
      double t = l2 > 0 ? std::clamp(gp_Vec(p[i], q).Dot(ab) / l2, 0.0, 1.0) : 0.0;
      double d = p[i].Translated(ab * t).Distance(q);
      if (d < best) {
        best = d;
        at = s[i] + t * (s[i + 1] - s[i]);
      }
    }
    return {at, best};
  }
  gp_Pnt at(double len) const {
    for (size_t i = 0; i + 1 < p.size(); ++i) {
      if (len <= s[i + 1] || i + 2 == p.size()) {
        double seg = s[i + 1] - s[i];
        double t = seg > 0 ? std::clamp((len - s[i]) / seg, 0.0, 1.0) : 0.0;
        return p[i].Translated(gp_Vec(p[i], p[i + 1]) * t);
      }
    }
    return p.front();
  }
};

double smooth(double t) { return t * t * (3.0 - 2.0 * t); }

TopoDS_Wire interp_wire(const std::vector<gp_Pnt> &pts) {
  Handle(TColgp_HArray1OfPnt) arr = new TColgp_HArray1OfPnt(1, static_cast<int>(pts.size()));
  for (size_t i = 0; i < pts.size(); ++i) arr->SetValue(static_cast<int>(i) + 1, pts[i]);
  GeomAPI_Interpolate in(arr, Standard_False, 1e-9);
  in.Perform();
  if (!in.IsDone()) throw std::runtime_error("a loft section could not be interpolated");
  BRepBuilderAPI_MakeEdge me(in.Curve());
  if (!me.IsDone()) throw std::runtime_error("a loft section could not be built");
  return BRepBuilderAPI_MakeWire(me.Edge()).Wire();
}

double diagonal(const std::vector<gp_Pnt> &p) {
  gp_XYZ lo = p.front().XYZ(), hi = lo;
  for (const auto &q : p) {
    lo.SetCoord(std::min(lo.X(), q.X()), std::min(lo.Y(), q.Y()), std::min(lo.Z(), q.Z()));
    hi.SetCoord(std::max(hi.X(), q.X()), std::max(hi.Y(), q.Y()), std::max(hi.Z(), q.Z()));
  }
  return (hi - lo).Modulus();
}

} // namespace

// An open surface through the section curves whose sides follow rails, as Fusion's surface Loft
// with Rails: the sections are matched point for point (open ones run the same way, closed ones
// start nearest the first rail), and between them every point is pulled along with the rails
// where they touch, the pull blending smoothly between rails and fading out past the outermost
// rail of an open section (round the loop of a closed one).
std::unique_ptr<Shape> loft_open_rails(const Shape &sections, const Shape &rails) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    const int n = 97;
    std::vector<std::vector<gp_Pnt>> secs;
    for (TopoDS_Iterator it(sections.s); it.More(); it.Next()) secs.push_back(sample(as_wire(it.Value()), n));
    if (secs.size() < 2) throw std::runtime_error("a loft needs at least two profiles");
    std::vector<Poly> rs;
    for (TopoDS_Iterator it(rails.s); it.More(); it.Next()) rs.emplace_back(sample(as_wire(it.Value()), 400));
    if (rs.empty()) throw std::runtime_error("no rails");
    double size = 0;
    for (const auto &p : secs) size = std::max(size, diagonal(p));
    bool closed = secs[0].front().Distance(secs[0].back()) < 1e-6 * std::max(size, 1.0);
    for (const auto &p : secs) {
      if ((p.front().Distance(p.back()) < 1e-6 * std::max(size, 1.0)) != closed) {
        throw std::runtime_error("the sections must all be open or all closed");
      }
    }
    if (closed) {
      // the same winding everywhere, each loop starting at its point nearest the first rail
      gp_Vec along(secs.front()[0], secs.back()[0]);
      for (auto &p : secs) {
        p.pop_back();
        gp_XYZ c(0, 0, 0), nrm(0, 0, 0);
        for (const auto &q : p) c += q.XYZ();
        c /= static_cast<double>(p.size());
        for (size_t i = 0; i < p.size(); ++i) nrm += (p[i].XYZ() - c).Crossed(p[(i + 1) % p.size()].XYZ() - c);
        if (gp_Vec(nrm).Dot(along) < 0) std::reverse(p.begin(), p.end());
        size_t best = 0;
        double bd = 1e300;
        for (size_t i = 0; i < p.size(); ++i) {
          double d = rs[0].nearest(p[i]).second;
          if (d < bd) {
            bd = d;
            best = i;
          }
        }
        std::rotate(p.begin(), p.begin() + static_cast<long>(best), p.end());
      }
    } else {
      // each section runs the way the one before it does
      for (size_t i = 1; i < secs.size(); ++i) {
        const auto &a = secs[i - 1];
        auto &b = secs[i];
        double same = a.front().Distance(b.front()) + a.back().Distance(b.back());
        double flipped = a.front().Distance(b.back()) + a.back().Distance(b.front());
        if (flipped < same) std::reverse(b.begin(), b.end());
      }
    }
    size_t np = secs[0].size();
    // where each rail meets each section: the point's index and the rail's arc length
    std::vector<std::vector<std::pair<size_t, double>>> hit(secs.size());
    for (size_t i = 0; i < secs.size(); ++i) {
      for (size_t j = 0; j < rs.size(); ++j) {
        size_t best = 0;
        double bd = 1e300;
        for (size_t k = 0; k < np; ++k) {
          double d = rs[j].nearest(secs[i][k]).second;
          if (d < bd) {
            bd = d;
            best = k;
          }
        }
        if (bd > 0.02 * size + 1e-3) {
          throw std::runtime_error("rail " + std::to_string(j + 1) + " does not touch section " +
                                   std::to_string(i + 1));
        }
        hit[i].push_back({best, rs[j].nearest(secs[i][best]).first});
      }
    }
    BRepOffsetAPI_ThruSections mk(Standard_False, Standard_False, 1e-6);
    auto add = [&](std::vector<gp_Pnt> pts) {
      if (closed) pts.push_back(pts.front());
      mk.AddWire(interp_wire(pts));
    };
    const int steps = 8;
    for (size_t i = 0; i < secs.size(); ++i) {
      const auto &a = secs[i];
      add(a);
      if (i + 1 == secs.size()) break;
      const auto &b = secs[i + 1];
      for (int m = 1; m < steps; ++m) {
        double t = static_cast<double>(m) / steps;
        std::vector<gp_Pnt> mid;
        for (size_t k = 0; k < np; ++k) mid.push_back(gp_Pnt(a[k].XYZ() * (1 - t) + b[k].XYZ() * t));
        // the rails' pulls at their contact positions (0..1 along the section)
        std::vector<std::pair<double, gp_Vec>> pull;
        for (size_t j = 0; j < rs.size(); ++j) {
          auto [ka, sa] = hit[i][j];
          auto [kb, sb] = hit[i + 1][j];
          gp_Pnt blended(a[ka].XYZ() * (1 - t) + b[kb].XYZ() * t);
          double span = static_cast<double>(closed ? np : np - 1);
          double u = (static_cast<double>(ka) * (1 - t) + static_cast<double>(kb) * t) / span;
          pull.push_back({u, gp_Vec(blended, rs[j].at(sa + (sb - sa) * t))});
        }
        std::sort(pull.begin(), pull.end(), [](const auto &x, const auto &y) { return x.first < y.first; });
        for (size_t k = 0; k < np; ++k) {
          double u = static_cast<double>(k) / static_cast<double>(closed ? np : np - 1);
          gp_Vec d;
          if (closed && pull.size() == 1) {
            d = pull[0].second * ((1 + std::cos(2 * M_PI * (u - pull[0].first))) / 2);
          } else if (closed) {
            size_t hi = 0;
            while (hi < pull.size() && pull[hi].first <= u) ++hi;
            size_t lo = (hi + pull.size() - 1) % pull.size();
            hi %= pull.size();
            double span = pull[hi].first - pull[lo].first, off = u - pull[lo].first;
            if (span <= 0) span += 1;
            if (off < 0) off += 1;
            double h = smooth(std::clamp(off / span, 0.0, 1.0));
            d = pull[lo].second * (1 - h) + pull[hi].second * h;
          } else if (u <= pull.front().first) {
            double u0 = pull.front().first;
            d = pull.front().second * (u0 > 1e-12 ? smooth(u / u0) : 1.0);
          } else if (u >= pull.back().first) {
            double u1 = pull.back().first;
            d = pull.back().second * (u1 < 1 - 1e-12 ? smooth((1 - u) / (1 - u1)) : 1.0);
          } else {
            size_t hi = 1;
            while (hi + 1 < pull.size() && pull[hi].first < u) ++hi;
            const auto &[u0, p0] = pull[hi - 1];
            const auto &[u1, p1] = pull[hi];
            double h = u1 - u0 > 1e-12 ? smooth((u - u0) / (u1 - u0)) : 1.0;
            d = p0 * (1 - h) + p1 * h;
          }
          mid[k].Translate(d);
        }
        add(mid);
      }
    }
    mk.CheckCompatibility(Standard_False);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("the loft along the rails failed");
    return out(mk.Shape());
  } catch (...) {
    fail("loft");
  }
}

// An open surface through the section curves, in order, placed along a centre line wire.
std::unique_ptr<Shape> loft_open_centerline(const Shape &sections, const Shape &centerline) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    BRepOffsetAPI_MakePipeShell ps(as_wire(centerline.s));
    ps.SetMode(Standard_False);
    int n = 0;
    for (TopoDS_Iterator it(sections.s); it.More(); it.Next(), ++n) {
      ps.Add(as_wire(it.Value()), Standard_False, Standard_False);
    }
    if (n < 2) throw std::runtime_error("a loft needs at least two profiles");
    ps.Build();
    if (!ps.IsDone()) throw std::runtime_error("the loft along the centre line failed");
    return out(ps.Shape());
  } catch (...) {
    fail("loft");
  }
}

} // namespace fox
