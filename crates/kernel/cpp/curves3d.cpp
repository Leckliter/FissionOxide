// 3D curves: the curves where two shapes cross (a sketch curve pushed along its normal against
// faces, for Fusion's Intersection Curve), chained into wires, and where a wire starts and which
// way it heads (to place a pipe's section).
#include "fox-kernel/cpp/curves3d.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <algorithm>
#include <stdexcept>
#include <string>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <Geom_Curve.hxx>
#include <Precision.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <Standard_Failure.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_HSequenceOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>

#include <vector>

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

} // namespace

// The curves where `a` and `b` cross, as a compound of wires: edges that meet end to end (within
// `tolerance`) are chained into one wire, so a section split at a face's seam comes back whole.
std::unique_ptr<Shape> section_wires(const Shape &a, const Shape &b, double tolerance) {
  try {
    BRepAlgoAPI_Section sec(a.s, b.s, Standard_False);
    sec.Approximation(Standard_True);
    sec.ComputePCurveOn1(Standard_False);
    sec.ComputePCurveOn2(Standard_False);
    sec.Build();
    if (!sec.IsDone()) throw std::runtime_error("the shapes could not be intersected");
    Handle(TopTools_HSequenceOfShape) edges = new TopTools_HSequenceOfShape;
    for (TopExp_Explorer ex(sec.Shape(), TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge &e = TopoDS::Edge(ex.Current());
      BRepAdaptor_Curve c(e);
      if (c.LastParameter() - c.FirstParameter() < Precision::PConfusion()) continue;
      edges->Append(e);
    }
    if (edges->IsEmpty()) throw std::runtime_error("the curves don't meet the faces");
    Handle(TopTools_HSequenceOfShape) wires;
    ShapeAnalysis_FreeBounds::ConnectEdgesToWires(edges, tolerance, Standard_False, wires);
    TopoDS_Compound out;
    BRep_Builder bb;
    bb.MakeCompound(out);
    for (int i = 1; i <= wires->Length(); ++i) bb.Add(out, wires->Value(i));
    auto r = std::make_unique<Shape>();
    r->s = out;
    return r;
  } catch (...) {
    fail("intersection curve");
  }
}

// Where a wire starts and the unit direction it leaves in: [x, y, z, dx, dy, dz].
rust::Vec<double> wire_start(const Shape &w) {
  try {
    if (w.s.IsNull() || w.s.ShapeType() != TopAbs_WIRE) throw std::runtime_error("not a wire");
    BRepTools_WireExplorer ex(TopoDS::Wire(w.s));
    if (!ex.More()) throw std::runtime_error("the wire is empty");
    BRepAdaptor_Curve c(ex.Current());
    bool rev = ex.Current().Orientation() == TopAbs_REVERSED;
    gp_Pnt p;
    gp_Vec d;
    c.D1(rev ? c.LastParameter() : c.FirstParameter(), p, d);
    if (rev) d.Reverse();
    if (d.Magnitude() < 1e-12) throw std::runtime_error("the wire has no direction at its start");
    d.Normalize();
    rust::Vec<double> out;
    for (double v : {p.X(), p.Y(), p.Z(), d.X(), d.Y(), d.Z()}) out.push_back(v);
    return out;
  } catch (...) {
    fail("wire start");
  }
}

// The wires in a shape (a section's compound), in order.
size_t wire_count(const Shape &s) {
  size_t n = 0;
  if (s.s.IsNull()) return 0;
  for (TopExp_Explorer ex(s.s, TopAbs_WIRE); ex.More(); ex.Next()) ++n;
  return n;
}

std::unique_ptr<Shape> wire_at(const Shape &s, size_t i) {
  try {
    size_t n = 0;
    for (TopExp_Explorer ex(s.s, TopAbs_WIRE); ex.More(); ex.Next(), ++n) {
      if (n == i) {
        auto r = std::make_unique<Shape>();
        r->s = ex.Current();
        return r;
      }
    }
    throw std::runtime_error("no such wire");
  } catch (...) {
    fail("wire");
  }
}


// A closed wire restarted at the middle (by length) of its first edge, whose halves become its
// first and last edges, as Fusion's pipes start on closed paths. An open wire comes back as is.
std::unique_ptr<Shape> wire_from_middle(const Shape &w) {
  try {
    if (w.s.IsNull() || w.s.ShapeType() != TopAbs_WIRE) throw std::runtime_error("not a wire");
    const TopoDS_Wire &wire = TopoDS::Wire(w.s);
    auto same = std::make_unique<Shape>();
    same->s = w.s;
    TopoDS_Vertex v0, v1;
    TopExp::Vertices(wire, v0, v1);
    if (v0.IsNull() || v1.IsNull()) return same;
    double gap = BRep_Tool::Pnt(v0).Distance(BRep_Tool::Pnt(v1));
    if (!v0.IsSame(v1) && gap > std::max(BRep_Tool::Tolerance(v0), BRep_Tool::Tolerance(v1))) return same;
    std::vector<TopoDS_Edge> edges;
    for (BRepTools_WireExplorer ex(wire); ex.More(); ex.Next()) edges.push_back(ex.Current());
    if (edges.empty()) throw std::runtime_error("the wire is empty");
    const TopoDS_Edge &first = edges.front();
    BRepAdaptor_Curve c(first);
    double t0 = c.FirstParameter(), t1 = c.LastParameter();
    GCPnts_AbscissaPoint mid(c, GCPnts_AbscissaPoint::Length(c) / 2.0, t0);
    if (!mid.IsDone()) throw std::runtime_error("the middle of the path's first curve could not be found");
    double tm = mid.Parameter();
    double f, l;
    Handle(Geom_Curve) g = BRep_Tool::Curve(first, f, l);
    if (g.IsNull()) throw std::runtime_error("the path's first curve has no geometry");
    TopoDS_Vertex vm = BRepBuilderAPI_MakeVertex(c.Value(tm));
    TopoDS_Vertex a = TopExp::FirstVertex(first, Standard_False), b = TopExp::LastVertex(first, Standard_False);
    TopoDS_Edge lo = BRepBuilderAPI_MakeEdge(g, a, vm, t0, tm);
    TopoDS_Edge hi = BRepBuilderAPI_MakeEdge(g, vm, b, tm, t1);
    bool rev = first.Orientation() == TopAbs_REVERSED;
    if (rev) {
      lo.Reverse();
      hi.Reverse();
    }
    // along the wire: the first edge runs lo then hi (hi then lo when reversed)
    TopoDS_Edge head = rev ? lo : hi, tail = rev ? hi : lo;
    BRepBuilderAPI_MakeWire mw;
    mw.Add(head);
    for (size_t i = 1; i < edges.size(); ++i) mw.Add(edges[i]);
    mw.Add(tail);
    if (!mw.IsDone()) throw std::runtime_error("the path could not be restarted at its middle");
    auto r = std::make_unique<Shape>();
    r->s = mw.Wire();
    return r;
  } catch (...) {
    fail("pipe path");
  }
}

} // namespace fox
