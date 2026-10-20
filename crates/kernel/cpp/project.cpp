// Project To Surface: sketch curves draped onto faces along a direction (each point pushed along
// it until it meets a face) or to the closest point (each point dropped onto the nearest point of
// the faces), as Fusion's Sketch > Project To Surface; and round pipes along the 3D curves that
// gives, which are often closed loops.
//
// As in Fusion, each projected run comes back as one curve (a periodic B-spline when it closes),
// starting where the source curve's start lands. A pipe along a closed curve is built, like
// Fusion's, from two half tubes (split at the start and halfway round) whose circle seam runs
// along the side the loop winds around, so the cut it makes has the faces Fusion's has.
#include "fox-kernel/cpp/project.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <algorithm>
#include <cstdlib>
#include <cmath>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_CompCurve.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepProj_Projection.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GCPnts_UniformAbscissa.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Surface.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <ShapeFix_Shape.hxx>
#include <Standard_Failure.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_HSequenceOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Lin.hxx>

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
  auto o = std::make_unique<Shape>();
  o->s = s;
  return o;
}

// The edges of a shape chained end to end into wires (closed where they close up).
Handle(TopTools_HSequenceOfShape) chain(const TopoDS_Shape &s, double tol) {
  Handle(TopTools_HSequenceOfShape) edges = new TopTools_HSequenceOfShape;
  for (TopExp_Explorer ex(s, TopAbs_EDGE); ex.More(); ex.Next()) edges->Append(ex.Current());
  Handle(TopTools_HSequenceOfShape) wires = new TopTools_HSequenceOfShape;
  if (edges->Length() > 0) ShapeAnalysis_FreeBounds::ConnectEdgesToWires(edges, tol, Standard_False, wires);
  return wires;
}

TopoDS_Wire as_wire(const TopoDS_Shape &s) {
  if (s.ShapeType() == TopAbs_WIRE) return TopoDS::Wire(s);
  if (s.ShapeType() == TopAbs_EDGE) return BRepBuilderAPI_MakeWire(TopoDS::Edge(s)).Wire();
  Handle(TopTools_HSequenceOfShape) w = chain(s, 1e-6);
  if (w->Length() != 1) throw std::runtime_error("the path is not one connected chain of curves");
  return TopoDS::Wire(w->Value(1));
}

// The start point and the unit tangent there of a wire (in its direction of travel).
void wire_start(const TopoDS_Wire &w, gp_Pnt &p, gp_Vec &t) {
  BRepTools_WireExplorer ex(w);
  if (!ex.More()) throw std::runtime_error("the path is empty");
  BRepAdaptor_Curve c(ex.Current());
  bool rev = ex.Current().Orientation() == TopAbs_REVERSED;
  c.D1(rev ? c.LastParameter() : c.FirstParameter(), p, t);
  if (rev) t.Reverse();
  if (t.Magnitude() < 1e-12) throw std::runtime_error("the path has no direction at its start");
  t.Normalize();
}

// Does the wire close up on itself?
bool is_closed(const TopoDS_Wire &w) {
  TopoDS_Vertex v0, v1;
  TopExp::Vertices(w, v0, v1);
  if (v0.IsNull() || v1.IsNull()) return BRep_Tool::IsClosed(w);
  return v0.IsSame(v1) || BRep_Tool::Pnt(v0).Distance(BRep_Tool::Pnt(v1)) < 1e-6;
}

// Points evenly spaced by arc length along a wire, with their unit tangents; a closed wire
// doesn't repeat its start at the end.
void sample(const TopoDS_Wire &w, std::vector<gp_Pnt> &pts, std::vector<gp_Vec> &tans) {
  BRepAdaptor_CompCurve c(w);
  const double len = GCPnts_AbscissaPoint::Length(c);
  if (!(len > 1e-9)) throw std::runtime_error("the curve has no length");
  const int n = std::clamp(static_cast<int>(len / 0.2), 64, 2000);
  GCPnts_UniformAbscissa ua(c, n + 1);
  if (!ua.IsDone()) throw std::runtime_error("the curve could not be sampled");
  const bool closed = is_closed(w);
  for (int i = 1; i <= ua.NbPoints(); ++i) {
    if (closed && i == ua.NbPoints()) break;
    gp_Pnt p;
    gp_Vec d;
    c.D1(ua.Parameter(i), p, d);
    pts.push_back(p);
    tans.push_back(d.Magnitude() > 1e-12 ? d.Normalized() : gp_Vec());
  }
}

// One B-spline through the points (periodic when `closed`), as an edge.
TopoDS_Edge fit(const std::vector<gp_Pnt> &raw, bool closed) {
  std::vector<gp_Pnt> pts;
  for (const gp_Pnt &p : raw) {
    if (pts.empty() || p.Distance(pts.back()) > 1e-6) pts.push_back(p);
  }
  if (closed && pts.size() > 1 && pts.back().Distance(pts.front()) <= 1e-6) pts.pop_back();
  if (pts.size() < (closed ? 3u : 2u)) throw std::runtime_error("the projected curve has no length");
  Handle(TColgp_HArray1OfPnt) arr = new TColgp_HArray1OfPnt(1, static_cast<int>(pts.size()));
  for (size_t i = 0; i < pts.size(); ++i) arr->SetValue(static_cast<int>(i) + 1, pts[i]);
  GeomAPI_Interpolate ip(arr, closed ? Standard_True : Standard_False, 1e-7);
  ip.Perform();
  if (!ip.IsDone()) throw std::runtime_error("the projected curve could not be fitted");
  return BRepBuilderAPI_MakeEdge(ip.Curve()).Edge();
}

// The nearest point of the faces to `p`.
gp_Pnt closest_on(const TopoDS_Shape &faces, const gp_Pnt &p) {
  double best = std::numeric_limits<double>::infinity();
  gp_Pnt hit;
  bool found = false;
  for (TopExp_Explorer fx(faces, TopAbs_FACE); fx.More(); fx.Next()) {
    const TopoDS_Face &f = TopoDS::Face(fx.Current());
    Handle(Geom_Surface) s = BRep_Tool::Surface(f);
    if (s.IsNull()) continue;
    GeomAPI_ProjectPointOnSurf pr(p, s);
    for (int i = 1; i <= pr.NbPoints(); ++i) {
      if (pr.Distance(i) >= best) continue;
      double u, v;
      pr.Parameters(i, u, v);
      BRepClass_FaceClassifier fc(f, gp_Pnt2d(u, v), 1e-7);
      if (fc.State() == TopAbs_OUT) continue;
      best = pr.Distance(i);
      hit = pr.Point(i);
      found = true;
    }
  }
  if (found) return hit;
  // the nearest point is on a face's boundary
  BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(p).Vertex(), faces);
  if (!d.IsDone() || d.NbSolution() == 0) throw std::runtime_error("no face is near the curve");
  return d.PointOnShape2(1);
}

// The unit normal of the face (with its orientation) nearest `p`.
gp_Vec face_normal(const TopoDS_Shape &faces, const gp_Pnt &p) {
  BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(p).Vertex(), faces);
  if (!d.IsDone() || d.NbSolution() == 0 || d.SupportTypeShape2(1) != BRepExtrema_IsInFace) return gp_Vec();
  const TopoDS_Face f = TopoDS::Face(d.SupportOnShape2(1));
  double u, v;
  d.ParOnFaceS2(1, u, v);
  BRepAdaptor_Surface s(f);
  BRepLProp_SLProps props(s, u, v, 1, 1e-9);
  if (!props.IsNormalDefined()) return gp_Vec();
  gp_Vec n(props.Normal());
  if (f.Orientation() == TopAbs_REVERSED) n.Reverse();
  return n;
}

// The normal the loop winds around (right-handed with its direction), by Newell's method.
gp_Vec winding_normal(const std::vector<gp_Pnt> &pts) {
  gp_Vec n;
  for (size_t i = 0; i < pts.size(); ++i) {
    const gp_Pnt &a = pts[i], &b = pts[(i + 1) % pts.size()];
    n += gp_Vec((a.Y() - b.Y()) * (a.Z() + b.Z()), (a.Z() - b.Z()) * (a.X() + b.X()), (a.X() - b.X()) * (a.Y() + b.Y()));
  }
  return n;
}

// A wire of the closed B-spline edge `e` cut into two edges, at its start and halfway along.
TopoDS_Wire halves(const TopoDS_Edge &e) {
  BRepAdaptor_Curve c(e);
  double f = c.FirstParameter(), l = c.LastParameter();
  GCPnts_AbscissaPoint mid(c, GCPnts_AbscissaPoint::Length(c) / 2, f);
  if (!mid.IsDone()) throw std::runtime_error("the path could not be halved");
  Handle(Geom_Curve) g = BRep_Tool::Curve(e, f, l);
  TopoDS_Vertex v0 = BRepBuilderAPI_MakeVertex(c.Value(f)).Vertex();
  TopoDS_Vertex v1 = BRepBuilderAPI_MakeVertex(c.Value(mid.Parameter())).Vertex();
  BRepBuilderAPI_MakeWire mw;
  mw.Add(BRepBuilderAPI_MakeEdge(g, v0, v1, f, mid.Parameter()).Edge());
  mw.Add(BRepBuilderAPI_MakeEdge(g, v1, v0, mid.Parameter(), l).Edge());
  if (!mw.IsDone()) throw std::runtime_error("the path could not be halved");
  return mw.Wire();
}

} // namespace

// The edges of `curves`, chained end to end, projected onto the faces of `faces`, each run as one
// curve (a compound of wires). `dir` is empty for the closest point, or the direction to project
// along followed by the source sketch's normal.
std::unique_ptr<Shape> project_curves(const Shape &curves, const Shape &faces, rust::Slice<const double> dir) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    std::vector<TopoDS_Wire> sources;
    Handle(TopTools_HSequenceOfShape) chains = chain(curves.s, 1e-6);
    for (int i = 1; i <= chains->Length(); ++i) sources.push_back(TopoDS::Wire(chains->Value(i)));
    if (sources.empty()) throw std::runtime_error("there are no curves to project");
    TopoDS_Compound res;
    BRep_Builder bb;
    bb.MakeCompound(res);
    int made = 0;
    for (const TopoDS_Wire &src : sources) {
      gp_Pnt p0;
      gp_Vec t0;
      wire_start(src, p0, t0);
      if (dir.size() < 3) {
        // each point to its nearest point of the faces; the curve keeps its direction
        std::vector<gp_Pnt> pts;
        std::vector<gp_Vec> tans;
        sample(src, pts, tans);
        for (gp_Pnt &p : pts) p = closest_on(faces.s, p);
        bb.Add(res, BRepBuilderAPI_MakeWire(fit(pts, is_closed(src))).Wire());
        ++made;
        continue;
      }
      gp_Vec d(dir[0], dir[1], dir[2]);
      if (d.Magnitude() < 1e-12) throw std::runtime_error("the projection direction has no length");
      d.Normalize();
      gp_Vec ns = dir.size() >= 6 ? gp_Vec(dir[3], dir[4], dir[5]) : gp_Vec();
      BRepProj_Projection proj(src, faces.s, gp_Dir(d));
      if (!proj.IsDone()) continue;
      TopoDS_Compound got;
      bb.MakeCompound(got);
      for (; proj.More(); proj.Next()) bb.Add(got, proj.Current());
      Handle(TopTools_HSequenceOfShape) runs = chain(got, 1e-5);
      const gp_Lin line(p0, gp_Dir(d));
      for (int i = 1; i <= runs->Length(); ++i) {
        const TopoDS_Wire &w = TopoDS::Wire(runs->Value(i));
        const bool closed = is_closed(w);
        std::vector<gp_Pnt> pts;
        std::vector<gp_Vec> tans;
        sample(w, pts, tans);
        // start where the source's start lands (an open run at its end nearer that line)
        size_t k = 0;
        if (closed) {
          for (size_t j = 1; j < pts.size(); ++j) {
            if (line.Distance(pts[j]) < line.Distance(pts[k])) k = j;
          }
          std::rotate(pts.begin(), pts.begin() + static_cast<long>(k), pts.end());
          std::rotate(tans.begin(), tans.begin() + static_cast<long>(k), tans.end());
        } else if (line.Distance(pts.back()) < line.Distance(pts.front())) {
          std::reverse(pts.begin(), pts.end());
          for (gp_Vec &t : tans) t.Reverse();
          std::reverse(tans.begin(), tans.end());
        }
        // run as the intersection of the swept source with the faces does: along the swept
        // surface's outer side crossed with the face normal
        gp_Vec nf = face_normal(faces.s, pts.front());
        if (ns.Magnitude() > 1e-12 && nf.Magnitude() > 1e-12) {
          gp_Vec want = t0.Crossed(ns).Crossed(nf);
          if (want.Dot(tans.front()) < 0) std::reverse(pts.begin() + (closed ? 1 : 0), pts.end());
        }
        bb.Add(res, BRepBuilderAPI_MakeWire(fit(pts, closed)).Wire());
        ++made;
      }
    }
    if (made == 0) throw std::runtime_error("the curves don't land on the faces");
    return out(res);
  } catch (...) {
    fail("project to surface");
  }
}

// A round pipe of `radius` swept along a wire (open or closed), hollow inside `inner` when that's
// above zero. Open paths start with the circle square to the path at its start. A closed path is
// refitted as one smooth loop and swept in two halves with a fixed binormal (the normal the loop
// winds around), the circle's seam on that side, so the tube closes up on itself.
std::unique_ptr<Shape> round_pipe(const Shape &spine, double radius, double inner) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    if (!(radius > 0)) throw std::runtime_error("the diameter must be positive");
    TopoDS_Wire w = as_wire(spine.s);
    const bool closed = is_closed(w);
    gp_Vec binormal;
    if (closed) {
      std::vector<gp_Pnt> pts;
      std::vector<gp_Vec> tans;
      sample(w, pts, tans);
      binormal = winding_normal(pts);
      // one periodic curve: a sweep along it closes exactly
      TopExp_Explorer ex(w, TopAbs_EDGE);
      TopoDS_Edge e = TopoDS::Edge(ex.Current());
      ex.Next();
      double f, l;
      Handle(Geom_Curve) g = BRep_Tool::Curve(e, f, l);
      if (ex.More() || g.IsNull() || !g->IsPeriodic()) e = fit(pts, true);
      w = halves(e);
    }
    gp_Pnt p;
    gp_Vec t;
    wire_start(w, p, t);
    // the seam starts on the side the loop winds around
    const bool seamed = binormal.Magnitude() > 1e-9 && binormal.Normalized().CrossMagnitude(t) > 1e-3;
    auto sweep = [&](double r) {
      BRepOffsetAPI_MakePipeShell ps(w);
      gp_Ax2 ax(p, gp_Dir(t));
      if (seamed) ax = gp_Ax2(p, gp_Dir(t), gp_Dir(binormal - t * binormal.Dot(t)));
      // a closed loop follows its Frenet frame, which comes back to itself; open paths the
      // corrected one, which doesn't flip at straight runs
      ps.SetMode(closed && !std::getenv("FOX_NOFIX") ? Standard_True : Standard_False);
      ps.Add(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(gp_Circ(ax, r)).Edge()).Wire());
      ps.Build();
      if (!ps.IsDone()) throw std::runtime_error("the pipe could not follow the path");
      if (!ps.MakeSolid()) throw std::runtime_error("the pipe made no solid");
      return ps.Shape();
    };
    TopoDS_Shape s = sweep(radius);
    if (inner > 0) {
      if (inner >= radius) throw std::runtime_error("the wall must be thinner than the radius");
      BRepAlgoAPI_Cut cut(s, sweep(inner));
      if (!cut.IsDone()) throw std::runtime_error("the pipe's bore could not be cut");
      s = cut.Shape();
    }
    if (!BRepCheck_Analyzer(s).IsValid()) {
      ShapeFix_Shape fix(s);
      fix.Perform();
      s = fix.Shape();
      if (!BRepCheck_Analyzer(s).IsValid()) throw std::runtime_error("the pipe is not a valid solid");
    }
    return out(s);
  } catch (...) {
    fail("pipe");
  }
}

// A wire's start point, unit tangent there, end point and whether it closes: [x, y, z, tx, ty,
// tz, ex, ey, ez, closed].
rust::Vec<double> wire_ends(const Shape &wire) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    TopoDS_Wire w = as_wire(wire.s);
    gp_Pnt p;
    gp_Vec t;
    wire_start(w, p, t);
    TopoDS_Vertex v0, v1;
    TopExp::Vertices(w, v0, v1);
    gp_Pnt e = v1.IsNull() ? p : BRep_Tool::Pnt(v1);
    rust::Vec<double> r;
    for (double x : {p.X(), p.Y(), p.Z(), t.X(), t.Y(), t.Z(), e.X(), e.Y(), e.Z()}) r.push_back(x);
    r.push_back(is_closed(w) ? 1.0 : 0.0);
    return r;
  } catch (...) {
    fail("path");
  }
}

// The number of direct children of a shape (the wires of a compound of wires, say).
size_t child_count(const Shape &s) {
  size_t n = 0;
  for (TopoDS_Iterator it(s.s); it.More(); it.Next()) ++n;
  return n;
}

// A shape's direct child at a 0-based index.
std::unique_ptr<Shape> child_at(const Shape &s, size_t i) {
  size_t n = 0;
  for (TopoDS_Iterator it(s.s); it.More(); it.Next(), ++n) {
    if (n == i) return out(it.Value());
  }
  throw std::runtime_error("no such child shape");
}

} // namespace fox
