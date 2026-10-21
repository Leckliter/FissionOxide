// Surface workspace additions to the bridge: extending surface edges, untrimming faces, telling
// which side of a trimming tool a point is on, and offsetting with a distance per face.
#include "fox-kernel/cpp/surface2.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <cmath>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFill.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakeFilling.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomLProp_SLProps.hxx>
#include <GeomLib.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_ElementarySurface.hxx>
#include <Geom_BoundedSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListIteratorOfListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <Precision.hxx>

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

std::unique_ptr<Shape> out_shape(const TopoDS_Shape &s) {
  if (s.IsNull()) throw std::runtime_error("kernel returned an empty shape");
  auto out = std::make_unique<Shape>();
  out->s = s;
  return out;
}

// The unit normal of a face at (u, v), facing the way the face does.
gp_Vec face_normal(const TopoDS_Face &f, const Handle(Geom_Surface) &surf, double u, double v) {
  GeomLProp_SLProps props(surf, u, v, 1, 1e-9);
  if (!props.IsNormalDefined()) return gp_Vec();
  gp_Vec n(props.Normal());
  return f.Orientation() == TopAbs_REVERSED ? -n : n;
}

// The edge as it is oriented inside its face.
TopoDS_Edge edge_in_face(const TopoDS_Edge &e, const TopoDS_Face &f) {
  for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More(); ex.Next()) {
    if (ex.Current().IsSame(e)) return TopoDS::Edge(ex.Current());
  }
  return e;
}

// Natural extension: the face's own surface continued past an edge that runs along one of its
// parameter lines. Returns a null face when the edge isn't such a boundary.
TopoDS_Face natural_strip(const TopoDS_Face &f, const TopoDS_Edge &e, double d) {
  double f0, l0;
  Handle(Geom2d_Curve) c2 = BRep_Tool::CurveOnSurface(e, f, f0, l0);
  if (c2.IsNull()) return TopoDS_Face();
  gp_Pnt2d a = c2->Value(f0), b = c2->Value(l0), m = c2->Value(0.5 * (f0 + l0));
  double umin, umax, vmin, vmax;
  BRepTools::UVBounds(f, umin, umax, vmin, vmax);
  double ptol = 1e-7 * (1.0 + std::abs(umax - umin) + std::abs(vmax - vmin));
  bool in_u;
  if (std::abs(a.X() - b.X()) < ptol && std::abs(a.X() - m.X()) < ptol) {
    in_u = true;
  } else if (std::abs(a.Y() - b.Y()) < ptol && std::abs(a.Y() - m.Y()) < ptol) {
    in_u = false;
  } else {
    return TopoDS_Face();
  }
  double at = in_u ? a.X() : a.Y();
  double lo = in_u ? umin : vmin, hi = in_u ? umax : vmax;
  bool after;
  double span_tol = 1e-6 * (1.0 + hi - lo);
  if (std::abs(at - hi) < span_tol) {
    after = true;
  } else if (std::abs(at - lo) < span_tol) {
    after = false;
  } else {
    return TopoDS_Face();
  }
  // the other parameter's range along the edge
  double o0 = in_u ? std::min(a.Y(), b.Y()) : std::min(a.X(), b.X());
  double o1 = in_u ? std::max(a.Y(), b.Y()) : std::max(a.X(), b.X());
  if (!(o1 - o0 > ptol)) return TopoDS_Face();

  Handle(Geom_Surface) surf = BRep_Tool::Surface(f);
  Handle(Geom_Surface) basis = surf;
  while (auto t = Handle(Geom_RectangularTrimmedSurface)::DownCast(basis)) basis = t->BasisSurface();
  double bu0, bu1, bv0, bv1;
  basis->Bounds(bu0, bu1, bv0, bv1);
  bool periodic = in_u ? basis->IsUPeriodic() : basis->IsVPeriodic();
  double blo = in_u ? bu0 : bv0, bhi = in_u ? bu1 : bv1;
  bool open_ended = periodic || (after ? Precision::IsPositiveInfinite(bhi) : Precision::IsNegativeInfinite(blo));

  TopoDS_Face strip;
  if (open_ended) {
    // the surface goes on: step the parameter by the distance over the speed along it
    gp_Pnt p;
    gp_Vec du, dv;
    double om = 0.5 * (o0 + o1);
    basis->D1(in_u ? at : om, in_u ? om : at, p, du, dv);
    double speed = (in_u ? du : dv).Magnitude();
    if (speed < 1e-12) return TopoDS_Face();
    double dp = d / speed;
    if (periodic) {
      double period = in_u ? basis->UPeriod() : basis->VPeriod();
      dp = std::min(dp, period - (hi - lo));
      if (!(dp > 1e-9)) throw std::runtime_error("the surface already closes on itself");
    }
    double s0 = after ? at : at - dp, s1 = after ? at + dp : at;
    BRepBuilderAPI_MakeFace mf(basis, in_u ? s0 : o0, in_u ? s1 : o1, in_u ? o0 : s0, in_u ? o1 : s1, 1e-7);
    if (!mf.IsDone()) return TopoDS_Face();
    strip = mf.Face();
  } else {
    // a bounded surface: continue it as a B-spline with curvature continuity
    Handle(Geom_BoundedSurface) bs = new Geom_RectangularTrimmedSurface(basis, umin, umax, vmin, vmax);
    GeomLib::ExtendSurfByLength(bs, d, 2, in_u ? Standard_True : Standard_False, after ? Standard_True : Standard_False);
    double nu0, nu1, nv0, nv1;
    bs->Bounds(nu0, nu1, nv0, nv1);
    // find the old boundary on the new surface (the extension may reparametrize)
    gp_Pnt pa = surf->Value(in_u ? at : o0, in_u ? o0 : at);
    gp_Pnt pb = surf->Value(in_u ? at : o1, in_u ? o1 : at);
    GeomAPI_ProjectPointOnSurf ja(pa, bs), jb(pb, bs);
    if (ja.NbPoints() == 0 || jb.NbPoints() == 0) return TopoDS_Face();
    double ua, va, ub, vb;
    ja.LowerDistanceParameters(ua, va);
    jb.LowerDistanceParameters(ub, vb);
    double s_at = in_u ? 0.5 * (ua + ub) : 0.5 * (va + vb);
    double q0 = in_u ? std::min(va, vb) : std::min(ua, ub), q1 = in_u ? std::max(va, vb) : std::max(ua, ub);
    double nlo = in_u ? nu0 : nv0, nhi = in_u ? nu1 : nv1;
    double s0 = after ? s_at : nlo, s1 = after ? nhi : s_at;
    if (!(s1 - s0 > 1e-9)) return TopoDS_Face();
    BRepBuilderAPI_MakeFace mf(bs, in_u ? s0 : q0, in_u ? s1 : q1, in_u ? q0 : s0, in_u ? q1 : s1, 1e-7);
    if (!mf.IsDone()) return TopoDS_Face();
    strip = mf.Face();
  }
  if (f.Orientation() == TopAbs_REVERSED) strip.Reverse();
  return strip;
}

// Tangent or perpendicular extension: a ruled strip between the edge and a copy of it pushed
// out along the surface (tangent) or along the surface normal (perpendicular).
TopoDS_Face ruled_strip(const TopoDS_Face &f, const TopoDS_Edge &e, double d, bool perpendicular) {
  BRepAdaptor_Curve c(e);
  double t0 = c.FirstParameter(), t1 = c.LastParameter();
  double f0, l0;
  Handle(Geom2d_Curve) c2 = BRep_Tool::CurveOnSurface(e, f, f0, l0);
  if (c2.IsNull()) throw std::runtime_error("the edge has no curve on its face");
  Handle(Geom_Surface) surf = BRep_Tool::Surface(f);
  // in parameter space the face lies left of a forward edge (of the forward face), right of a
  // reversed one, whichever way the face itself faces
  TopoDS_Edge ef = edge_in_face(e, TopoDS::Face(f.Oriented(TopAbs_FORWARD)));
  double off_side = ef.Orientation() == TopAbs_REVERSED ? 1.0 : -1.0;
  const int n = c.GetType() == GeomAbs_Line ? 2 : 33;
  std::vector<gp_Pnt> pts(n);
  std::vector<gp_Vec> dirs(n);
  for (int i = 0; i < n; ++i) {
    double t = t0 + (t1 - t0) * i / (n - 1);
    gp_Pnt p;
    gp_Vec tan;
    c.D1(t, p, tan);
    double s = f0 + (l0 - f0) * (t - t0) / (t1 - t0);
    gp_Pnt2d uv;
    gp_Vec2d duv;
    c2->D1(s, uv, duv);
    gp_Vec out;
    if (perpendicular) {
      out = face_normal(f, surf, uv.X(), uv.Y());
    } else {
      gp_Pnt sp;
      gp_Vec du, dv;
      surf->D1(uv.X(), uv.Y(), sp, du, dv);
      // the parameter-space normal pointing off the face, mapped onto the surface
      gp_Vec2d o2(-duv.Y() * off_side, duv.X() * off_side);
      out = du * o2.X() + dv * o2.Y();
      if (tan.Magnitude() > 1e-12) {
        gp_Vec tn = tan.Normalized();
        out -= tn * out.Dot(tn);
      }
    }
    if (out.Magnitude() < 1e-12) throw std::runtime_error("the edge has no direction to extend in");
    pts[i] = p;
    dirs[i] = out.Normalized();
  }
  bool constant = true;
  for (int i = 1; i < n; ++i) {
    if (dirs[i].Angle(dirs[0]) > 1e-9) constant = false;
  }
  TopoDS_Edge bare = TopoDS::Edge(e.Oriented(TopAbs_FORWARD));
  if (constant) {
    BRepPrimAPI_MakePrism mk(bare, dirs[0] * d);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("the extension could not be built");
    for (TopExp_Explorer ex(mk.Shape(), TopAbs_FACE); ex.More(); ex.Next()) return TopoDS::Face(ex.Current());
    throw std::runtime_error("the extension could not be built");
  }
  bool closed = pts.front().Distance(pts.back()) < 1e-7;
  int m = closed ? n - 1 : n;
  Handle(TColgp_HArray1OfPnt) arr = new TColgp_HArray1OfPnt(1, m);
  for (int i = 0; i < m; ++i) arr->SetValue(i + 1, pts[i].Translated(dirs[i] * d));
  GeomAPI_Interpolate interp(arr, closed ? Standard_True : Standard_False, 1e-7);
  interp.Perform();
  if (!interp.IsDone()) throw std::runtime_error("the extended edge could not be built");
  TopoDS_Edge outer = BRepBuilderAPI_MakeEdge(interp.Curve()).Edge();
  return BRepFill::Face(bare, outer);
}

// The side edge of an extension strip that leaves the point v (not the strip's copy of the
// extended edge `base`), and its far end. Null when the strip has none there.
TopoDS_Edge side_edge(const TopoDS_Face &strip, const TopoDS_Edge &base, const gp_Pnt &v, double tol, gp_Pnt &far_end) {
  for (TopExp_Explorer ex(strip, TopAbs_EDGE); ex.More(); ex.Next()) {
    TopoDS_Edge e = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(e)) continue;
    TopoDS_Vertex a, b;
    TopExp::Vertices(e, a, b);
    if (a.IsNull() || b.IsNull()) continue;
    gp_Pnt pa = BRep_Tool::Pnt(a), pb = BRep_Tool::Pnt(b);
    gp_Pnt other;
    if (pa.Distance(v) < tol) {
      other = pb;
    } else if (pb.Distance(v) < tol) {
      other = pa;
    } else {
      continue;
    }
    if (other.Distance(v) < tol) continue;
    // the strip's copy of the extended edge has its middle on that edge
    BRepAdaptor_Curve c(e);
    gp_Pnt mid = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    BRepExtrema_DistShapeShape dist(BRepBuilderAPI_MakeVertex(mid).Vertex(), base);
    if (dist.IsDone() && dist.NbSolution() > 0 && dist.Value() < tol) continue;
    far_end = other;
    return e;
  }
  return TopoDS_Edge();
}

// The unit tangent of an edge at its end point v, pointing into the edge.
gp_Vec tangent_into(const TopoDS_Edge &e, const gp_Pnt &v) {
  BRepAdaptor_Curve c(e);
  double t0 = c.FirstParameter(), t1 = c.LastParameter();
  bool at_start = c.Value(t0).Distance(v) <= c.Value(t1).Distance(v);
  gp_Pnt p;
  gp_Vec d;
  c.D1(at_start ? t0 : t1, p, d);
  if (d.Magnitude() < 1e-12) return gp_Vec();
  d.Normalize();
  return at_start ? d : -d;
}

// The patch closing the gap between two extension strips that leave the same corner v along
// the side edges s1 (to p1) and s2 (to p2). A straight side sweeps the other one along it (on a
// plane that is a parallelogram, on a cylinder the cylinder carried on); two curved sides are
// filled with a surface through them and the lines closing the quad.
TopoDS_Face corner_patch(const TopoDS_Edge &s1, const gp_Pnt &v, const gp_Pnt &p1, const TopoDS_Edge &s2,
                         const gp_Pnt &p2) {
  auto straight = [](const TopoDS_Edge &e) { return BRepAdaptor_Curve(e).GetType() == GeomAbs_Line; };
  auto prism = [](const TopoDS_Edge &e, const gp_Vec &along) {
    BRepPrimAPI_MakePrism mk(TopoDS::Edge(e.Oriented(TopAbs_FORWARD)), along);
    mk.Build();
    if (!mk.IsDone()) return TopoDS_Face();
    for (TopExp_Explorer ex(mk.Shape(), TopAbs_FACE); ex.More(); ex.Next()) return TopoDS::Face(ex.Current());
    return TopoDS_Face();
  };
  if (straight(s2)) return prism(s1, gp_Vec(v, p2));
  if (straight(s1)) return prism(s2, gp_Vec(v, p1));
  gp_Pnt q = p1.Translated(gp_Vec(v, p2));
  BRepOffsetAPI_MakeFilling fill;
  fill.Add(s1, GeomAbs_C0);
  fill.Add(s2, GeomAbs_C0);
  fill.Add(BRepBuilderAPI_MakeEdge(p1, q).Edge(), GeomAbs_C0);
  fill.Add(BRepBuilderAPI_MakeEdge(q, p2).Edge(), GeomAbs_C0);
  fill.Build();
  if (!fill.IsDone()) return TopoDS_Face();
  for (TopExp_Explorer ex(fill.Shape(), TopAbs_FACE); ex.More(); ex.Next()) return TopoDS::Face(ex.Current());
  return TopoDS_Face();
}

// Grows the parameter box [u0, u1] x [v0, v1] of a surface outward by the length d on every
// side that doesn't already close on itself. Planes, cylinders and the like step their
// parameters (up to their natural bounds); other surfaces that would run past theirs are
// continued as B-splines with curvature continuity, which replaces `surf`.
void grow_box(Handle(Geom_Surface) &surf, double &u0, double &u1, double &v0, double &v1, double d) {
  Handle(Geom_Surface) basis = surf;
  while (auto t = Handle(Geom_RectangularTrimmedSurface)::DownCast(basis)) basis = t->BasisSurface();
  double bu0, bu1, bv0, bv1;
  basis->Bounds(bu0, bu1, bv0, bv1);
  bool uper = basis->IsUPeriodic(), vper = basis->IsVPeriodic();
  bool ufull = uper && u1 - u0 >= basis->UPeriod() - 1e-9;
  bool vfull = vper && v1 - v0 >= basis->VPeriod() - 1e-9;
  double um = 0.5 * (u0 + u1), vm = 0.5 * (v0 + v1);
  auto step = [&](double u, double v, bool in_u) {
    gp_Pnt p;
    gp_Vec du, dv;
    basis->D1(u, v, p, du, dv);
    double speed = (in_u ? du : dv).Magnitude();
    return speed > 1e-12 ? d / speed : 0.0;
  };
  double nu0 = ufull ? u0 : u0 - step(u0, vm, true), nu1 = ufull ? u1 : u1 + step(u1, vm, true);
  double nv0 = vfull ? v0 : v0 - step(um, v0, false), nv1 = vfull ? v1 : v1 + step(um, v1, false);
  // a periodic direction grows to one period at most
  auto cap = [](double &a, double &b, double period) {
    double over = (b - a) - period;
    if (over > 0.0) {
      a += 0.5 * over;
      b -= 0.5 * over;
    }
  };
  if (uper) cap(nu0, nu1, basis->UPeriod());
  if (vper) cap(nv0, nv1, basis->VPeriod());
  bool past = (!uper && (nu0 < bu0 - 1e-9 || nu1 > bu1 + 1e-9)) || (!vper && (nv0 < bv0 - 1e-9 || nv1 > bv1 + 1e-9));
  if (!past || basis->IsKind(STANDARD_TYPE(Geom_ElementarySurface))) {
    // analytic surfaces stop at their natural bounds (a sphere at its poles)
    if (!uper) {
      nu0 = std::max(nu0, bu0);
      nu1 = std::min(nu1, bu1);
    }
    if (!vper) {
      nv0 = std::max(nv0, bv0);
      nv1 = std::min(nv1, bv1);
    }
    u0 = nu0, u1 = nu1, v0 = nv0, v1 = nv1;
    surf = basis;
    return;
  }
  Handle(Geom_BoundedSurface) bs = new Geom_RectangularTrimmedSurface(basis, u0, u1, v0, v1);
  for (int k = 0; k < 4; ++k) {
    bool in_u = k < 2, after = k % 2 == 0;
    if (in_u ? ufull : vfull) continue;
    GeomLib::ExtendSurfByLength(bs, d, 2, in_u ? Standard_True : Standard_False, after ? Standard_True : Standard_False);
  }
  bs->Bounds(u0, u1, v0, v1);
  surf = bs;
}

} // namespace

// Extends free edges of a surface body by d: mode 0 natural (the surface continued), 1 tangent,
// 2 perpendicular. With `corners`, the gap between the strips of two extended edges meeting at a
// corner is closed with a patch. The new strips are sewn on and same-surface faces merged.
std::unique_ptr<Shape> extend_edges(const Shape &s, rust::Slice<const uint32_t> edges, double d, int32_t mode,
                                    bool corners) {
  try {
    if (!(d > 0.0)) throw std::runtime_error("the distance must be positive");
    TopTools_IndexedMapOfShape emap;
    TopExp::MapShapes(s.s, TopAbs_EDGE, emap);
    TopTools_IndexedDataMapOfShapeListOfShape owners;
    TopExp::MapShapesAndAncestors(s.s, TopAbs_EDGE, TopAbs_FACE, owners);
    BRepBuilderAPI_Sewing sew(1e-4);
    sew.Add(s.s);
    std::vector<std::pair<TopoDS_Edge, TopoDS_Face>> strips;
    for (uint32_t id : edges) {
      if (id < 1 || static_cast<int>(id) > emap.Extent()) throw std::runtime_error("no such edge");
      TopoDS_Edge e = TopoDS::Edge(emap(static_cast<int>(id)));
      if (BRep_Tool::Degenerated(e)) throw std::runtime_error("the edge has no length");
      // distinct faces at the edge (a face can be listed once per use of the edge)
      TopoDS_Face f;
      int nfaces = 0;
      for (TopTools_ListIteratorOfListOfShape it(owners.FindFromKey(e)); it.More(); it.Next()) {
        if (f.IsNull() || !f.IsSame(it.Value())) {
          if (!f.IsNull()) ++nfaces;
          f = TopoDS::Face(it.Value());
        }
      }
      if (!f.IsNull()) ++nfaces;
      if (nfaces != 1 || BRep_Tool::IsClosed(e, f)) {
        throw std::runtime_error("only open (boundary) edges of a surface can be extended");
      }
      TopoDS_Edge ef = edge_in_face(e, f);
      TopoDS_Face strip;
      if (mode == 0) strip = natural_strip(f, ef, d);
      if (strip.IsNull()) strip = ruled_strip(f, ef, d, mode == 2);
      sew.Add(strip);
      strips.emplace_back(e, strip);
    }
    if (corners) {
      const double tol = 1e-4;
      for (size_t i = 0; i < strips.size(); ++i) {
        for (size_t j = i + 1; j < strips.size(); ++j) {
          const TopoDS_Edge &e1 = strips[i].first, &e2 = strips[j].first;
          if (e1.IsSame(e2)) continue;
          TopoDS_Vertex a1, b1, a2, b2;
          TopExp::Vertices(e1, a1, b1);
          TopExp::Vertices(e2, a2, b2);
          for (int k = 0; k < 2; ++k) {
            const TopoDS_Vertex &w = k == 0 ? a1 : b1;
            if (w.IsNull() || !(w.IsSame(a2) || w.IsSame(b2))) continue;
            if (k == 1 && a1.IsSame(b1)) continue;
            gp_Pnt v = BRep_Tool::Pnt(w), p1, p2;
            TopoDS_Edge s1 = side_edge(strips[i].second, e1, v, tol, p1);
            TopoDS_Edge s2 = side_edge(strips[j].second, e2, v, tol, p2);
            if (s1.IsNull() || s2.IsNull()) continue;
            gp_Vec d1(v, p1), d2(v, p2);
            d1.Normalize();
            d2.Normalize();
            // strips that run on together (a smooth chain) or share their side have no gap; at
            // a reflex corner they overlap instead
            if (d1.Angle(d2) < 1e-6) continue;
            gp_Vec t1 = tangent_into(e1, v), t2 = tangent_into(e2, v);
            if (!(d1.Dot(t2) < -1e-6 && d2.Dot(t1) < -1e-6)) continue;
            TopoDS_Face patch = corner_patch(s1, v, p1, s2, p2);
            if (patch.IsNull()) throw std::runtime_error("a corner between the extensions could not be closed");
            sew.Add(patch);
          }
        }
      }
    }
    sew.Perform();
    TopoDS_Shape sewn = sew.SewedShape();
    if (sewn.IsNull()) throw std::runtime_error("the extension could not be sewn on");
    ShapeUpgrade_UnifySameDomain unify(sewn, Standard_True, Standard_True, Standard_False);
    unify.Build();
    TopoDS_Shape r = unify.Shape();
    return out_shape(r.IsNull() ? sewn : r);
  } catch (...) {
    fail("extend");
  }
}

// Untrims faces: loops 0 = all (the surface's natural boundary, no holes), 1 = inner (holes
// removed, outer boundary kept), 2 = outer (natural boundary, holes kept). Infinite parameter
// directions (planes, extrusions) keep the face's own extent. A positive `extend` then grows the
// restored boundary outward by that length (not with loops 1, which keeps the boundary).
std::unique_ptr<Shape> untrim_faces(const Shape &s, rust::Slice<const uint32_t> faces, int32_t loops,
                                    double extend) {
  try {
    if (extend < 0.0 || !std::isfinite(extend)) throw std::runtime_error("the extension can't be negative");
    TopTools_IndexedMapOfShape fmap;
    TopExp::MapShapes(s.s, TopAbs_FACE, fmap);
    BRepTools_ReShape reshape;
    BRep_Builder bld;
    for (uint32_t id : faces) {
      if (id < 1 || static_cast<int>(id) > fmap.Extent()) throw std::runtime_error("no such face");
      TopoDS_Face f = TopoDS::Face(fmap(static_cast<int>(id)));
      TopoDS_Face fwd = TopoDS::Face(f.Oriented(TopAbs_FORWARD));
      TopoDS_Wire outer = BRepTools::OuterWire(fwd);
      std::vector<TopoDS_Wire> holes;
      for (TopExp_Explorer ex(fwd, TopAbs_WIRE); ex.More(); ex.Next()) {
        if (!ex.Current().IsSame(outer)) holes.push_back(TopoDS::Wire(ex.Current()));
      }
      TopoDS_Face nf;
      if (loops == 1) {
        if (holes.empty()) continue;
        nf = TopoDS::Face(fwd.EmptyCopied());
        bld.Add(nf, outer);
      } else {
        Handle(Geom_Surface) surf = BRep_Tool::Surface(fwd);
        Handle(Geom_Surface) basis = surf;
        while (auto t = Handle(Geom_RectangularTrimmedSurface)::DownCast(basis)) basis = t->BasisSurface();
        double fu0, fu1, fv0, fv1, bu0, bu1, bv0, bv1;
        BRepTools::UVBounds(fwd, fu0, fu1, fv0, fv1);
        surf->Bounds(bu0, bu1, bv0, bv1);
        auto pick = [](double face, double nat, bool lower) {
          if (Precision::IsInfinite(nat)) return face;
          return lower ? std::min(face, nat) : std::max(face, nat);
        };
        double u0 = pick(fu0, bu0, true), u1 = pick(fu1, bu1, false);
        double v0 = pick(fv0, bv0, true), v1 = pick(fv1, bv1, false);
        // a periodic direction closes up over one period
        if (surf->IsUPeriodic() && u1 - u0 > surf->UPeriod()) u1 = u0 + surf->UPeriod();
        if (surf->IsVPeriodic() && v1 - v0 > surf->VPeriod()) v1 = v0 + surf->VPeriod();
        if (extend > 0.0) grow_box(surf, u0, u1, v0, v1, extend);
        BRepBuilderAPI_MakeFace mf(surf, u0, u1, v0, v1, BRep_Tool::Tolerance(fwd));
        if (!mf.IsDone()) throw std::runtime_error("the surface's boundary could not be restored");
        nf = mf.Face();
        if (loops == 2) {
          for (const TopoDS_Wire &w : holes) bld.Add(nf, w);
          ShapeFix_Face fix(nf);
          fix.Perform();
          nf = fix.Face();
        }
      }
      nf.Orientation(f.Orientation());
      reshape.Replace(f, nf);
    }
    TopoDS_Shape r = reshape.Apply(s.s);
    return out_shape(r);
  } catch (...) {
    fail("untrim");
  }
}

// Which side of a trimming tool a point is on: for a solid, +1 inside and -1 outside (0 on its
// skin); for surfaces, the distance along the nearest face's normal (+ in front, - behind).
double classify_point(const Shape &tool, double x, double y, double z) {
  try {
    gp_Pnt p(x, y, z);
    bool any_solid = false;
    bool on = false;
    for (TopExp_Explorer ex(tool.s, TopAbs_SOLID); ex.More(); ex.Next()) {
      any_solid = true;
      BRepClass3d_SolidClassifier sc(ex.Current(), p, 1e-6);
      TopAbs_State st = sc.State();
      if (st == TopAbs_IN) return 1.0;
      if (st == TopAbs_ON) on = true;
    }
    if (any_solid) return on ? 0.0 : -1.0;
    TopoDS_Vertex v = BRepBuilderAPI_MakeVertex(p).Vertex();
    double best = 1e300, side = 0.0;
    for (TopExp_Explorer ex(tool.s, TopAbs_FACE); ex.More(); ex.Next()) {
      TopoDS_Face f = TopoDS::Face(ex.Current());
      BRepExtrema_DistShapeShape dist(v, f);
      if (!dist.IsDone() || dist.NbSolution() == 0) continue;
      double dd = dist.Value();
      if (dd >= best) continue;
      gp_Pnt q = dist.PointOnShape2(1);
      Handle(Geom_Surface) surf = BRep_Tool::Surface(f);
      GeomAPI_ProjectPointOnSurf proj(q, surf);
      if (proj.NbPoints() == 0) continue;
      double u, w;
      proj.LowerDistanceParameters(u, w);
      gp_Vec n = face_normal(f, surf, u, w);
      if (n.Magnitude() < 1e-12) continue;
      best = dd;
      side = gp_Vec(q, p).Dot(n.Normalized());
    }
    if (best == 1e300) throw std::runtime_error("the tool has no faces");
    return side;
  } catch (...) {
    fail("trim side");
  }
}

// Offsets a face, shell or body's skin by d, with faces[i] (1-based face index) offset by
// dists[i] instead. Falls back to offsetting each face on its own when the faces can't be
// joined at their different distances.
std::unique_ptr<Shape> offset_variable(const Shape &s, double d, rust::Slice<const uint32_t> faces,
                                       rust::Slice<const double> dists) {
  try {
    if (faces.size() != dists.size()) throw std::runtime_error("one distance per face");
    TopTools_IndexedMapOfShape fmap;
    TopExp::MapShapes(s.s, TopAbs_FACE, fmap);
    auto dist_of = [&](const TopoDS_Shape &f) {
      for (size_t i = 0; i < faces.size(); ++i) {
        if (faces[i] >= 1 && static_cast<int>(faces[i]) <= fmap.Extent() && fmap(static_cast<int>(faces[i])).IsSame(f))
          return dists[i];
      }
      return d;
    };
    for (uint32_t id : faces) {
      if (id < 1 || static_cast<int>(id) > fmap.Extent()) throw std::runtime_error("no such face");
    }
    try {
      BRepOffset_MakeOffset mk;
      mk.Initialize(s.s, d, 1e-6, BRepOffset_Skin, Standard_False, Standard_False, GeomAbs_Arc);
      for (size_t i = 0; i < faces.size(); ++i) mk.SetOffsetOnFace(TopoDS::Face(fmap(static_cast<int>(faces[i]))), dists[i]);
      mk.MakeOffsetShape();
      if (mk.IsDone() && !mk.Shape().IsNull()) {
        TopoDS_Shape r = mk.Shape();
        int nf = 0;
        for (TopExp_Explorer ex(r, TopAbs_FACE); ex.More(); ex.Next()) ++nf;
        if (nf >= fmap.Extent()) return out_shape(r);
      }
    } catch (...) {
      // fall through to face by face
    }
    BRep_Builder b;
    TopoDS_Compound c;
    b.MakeCompound(c);
    for (int i = 1; i <= fmap.Extent(); ++i) {
      double di = dist_of(fmap(i));
      if (std::abs(di) < 1e-12) {
        b.Add(c, fmap(i));
        continue;
      }
      BRepOffsetAPI_MakeOffsetShape mk;
      mk.PerformBySimple(fmap(i), di);
      if (!mk.IsDone()) throw std::runtime_error("a face can't be offset that far");
      for (TopExp_Explorer ex(mk.Shape(), TopAbs_FACE); ex.More(); ex.Next()) b.Add(c, ex.Current());
    }
    BRepBuilderAPI_Sewing sew(1e-4);
    sew.Add(c);
    sew.Perform();
    TopoDS_Shape r = sew.SewedShape();
    return out_shape(r.IsNull() ? TopoDS_Shape(c) : r);
  } catch (...) {
    fail("variable offset");
  }
}

// A face or connected shell offset by d as one piece: where two faces meet at an angle their
// offsets are extended until they intersect (or joined by an arc about the edge when `round`), so
// the corner stays closed. With `thicken` the result is the solid between the skin and its offset,
// its coplanar (or same-surface) neighbouring faces merged.
std::unique_ptr<Shape> offset_joined(const Shape &s, double d, bool round, bool thicken) {
  try {
    if (std::abs(d) < 1e-9) throw std::runtime_error("the distance must not be zero");
    BRepOffset_MakeOffset mo;
    mo.Initialize(s.s, d, 1e-6, BRepOffset_Skin, Standard_False, Standard_False,
                  round ? GeomAbs_Arc : GeomAbs_Intersection, thicken ? Standard_True : Standard_False);
    mo.MakeOffsetShape();
    if (!mo.IsDone() || mo.Shape().IsNull()) throw std::runtime_error("the faces could not be offset together");
    TopoDS_Shape r = mo.Shape();
    if (thicken) {
      // the walls along the skin's boundary come one per skin face; merge those that continue
      // each other (the end caps of an extruded skin) into one face
      ShapeUpgrade_UnifySameDomain u(r, Standard_True, Standard_True, Standard_False);
      u.Build();
      if (!u.Shape().IsNull() && BRepCheck_Analyzer(u.Shape()).IsValid()) r = u.Shape();
    }
    if (!BRepCheck_Analyzer(r).IsValid()) throw std::runtime_error("the joined offset is not valid");
    return out_shape(r);
  } catch (...) {
    fail(thicken ? "thicken" : "offset");
  }
}

} // namespace fox
