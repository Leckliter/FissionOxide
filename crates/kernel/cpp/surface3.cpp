// Fusion's surface Patch with a continuity per boundary edge and interior rails, its Ruled
// surface grown from an edge along (or across) the face beside it, and its Sweep and Loft, which
// make open surfaces from curves where the solid commands make solids from profiles.
#include "fox-kernel/cpp/surface3.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <cmath>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepFill.hxx>
#include <BRepFill_Filling.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
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
    TopoDS_Edge far = BRepBuilderAPI_MakeEdge(interp.Curve()).Edge();
    TopoDS_Face r = BRepFill::Face(e, far);
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

} // namespace fox
