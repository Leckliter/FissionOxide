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

#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

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

// An edge of a wire in order along it: where and which way it starts, and which way it ends.
struct PathEdge {
  TopoDS_Edge e;
  gp_Pnt p0;
  gp_Vec d0, d1;
};

bool wire_closed(const TopoDS_Wire &wire) {
  TopoDS_Vertex v0, v1;
  TopExp::Vertices(wire, v0, v1);
  if (v0.IsNull() || v1.IsNull()) return false;
  if (v0.IsSame(v1)) return true;
  return BRep_Tool::Pnt(v0).Distance(BRep_Tool::Pnt(v1)) <=
         std::max(BRep_Tool::Tolerance(v0), BRep_Tool::Tolerance(v1));
}

std::vector<PathEdge> path_edges(const TopoDS_Wire &wire) {
  std::vector<PathEdge> out;
  for (BRepTools_WireExplorer ex(wire); ex.More(); ex.Next()) {
    PathEdge pe;
    pe.e = ex.Current();
    BRepAdaptor_Curve c(pe.e);
    bool rev = pe.e.Orientation() == TopAbs_REVERSED;
    gp_Pnt p1;
    c.D1(rev ? c.LastParameter() : c.FirstParameter(), pe.p0, pe.d0);
    c.D1(rev ? c.FirstParameter() : c.LastParameter(), p1, pe.d1);
    if (rev) {
      pe.d0.Reverse();
      pe.d1.Reverse();
    }
    out.push_back(pe);
  }
  return out;
}

// Which edges start at a sharp corner, where the path's direction turns by more than `angle`
// radians. On an open wire the first edge never does; on a closed one it may.
std::vector<bool> corner_starts(const std::vector<PathEdge> &edges, bool closed, double angle) {
  std::vector<bool> out(edges.size(), false);
  for (size_t i = 0; i < edges.size(); ++i) {
    if (i == 0 && !closed) continue;
    const gp_Vec &in = edges[i == 0 ? edges.size() - 1 : i - 1].d1;
    const gp_Vec &o = edges[i].d0;
    // no direction to compare: treat it as a corner, which is always safe
    out[i] = in.Magnitude() < 1e-12 || o.Magnitude() < 1e-12 || in.Angle(o) > angle;
  }
  return out;
}

const TopoDS_Wire &as_path(const Shape &w) {
  if (w.s.IsNull() || w.s.ShapeType() != TopAbs_WIRE) throw std::runtime_error("not a wire");
  return TopoDS::Wire(w.s);
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

// The edges of `s` (wires from separate sections, say) chained end to end, within `tolerance`,
// into as few wires as they make: a compound of wires.
std::unique_ptr<Shape> chain_wires(const Shape &s, double tolerance) {
  try {
    Handle(TopTools_HSequenceOfShape) edges = new TopTools_HSequenceOfShape;
    for (TopExp_Explorer ex(s.s, TopAbs_EDGE); ex.More(); ex.Next()) edges->Append(ex.Current());
    if (edges->IsEmpty()) throw std::runtime_error("there are no curves to chain");
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
    fail("chain curves");
  }
}

// A wire split at its sharp corners (where its direction turns by more than `angle` radians) into
// smooth runs, as a compound of wires in order along it. A closed wire with corners splits into
// runs that each start and end at one; a wire without corners comes back whole.
std::unique_ptr<Shape> split_at_corners(const Shape &w, double angle) {
  try {
    const TopoDS_Wire &wire = as_path(w);
    std::vector<PathEdge> edges = path_edges(wire);
    if (edges.empty()) throw std::runtime_error("the wire is empty");
    bool closed = wire_closed(wire);
    std::vector<bool> corner = corner_starts(edges, closed, angle);
    TopoDS_Compound out;
    BRep_Builder bb;
    bb.MakeCompound(out);
    if (std::find(corner.begin(), corner.end(), true) == corner.end()) {
      bb.Add(out, wire);
    } else {
      // a closed wire starts its first run at its first corner
      size_t first = 0;
      if (closed) {
        while (!corner[first]) ++first;
      }
      std::vector<TopoDS_Edge> run;
      auto flush = [&]() {
        BRepBuilderAPI_MakeWire mw;
        for (const TopoDS_Edge &e : run) mw.Add(e);
        if (!mw.IsDone()) throw std::runtime_error("a run of the path could not be made");
        bb.Add(out, mw.Wire());
        run.clear();
      };
      for (size_t k = 0; k < edges.size(); ++k) {
        size_t i = (first + k) % edges.size();
        if (!run.empty() && corner[i]) flush();
        run.push_back(edges[i].e);
      }
      flush();
    }
    auto r = std::make_unique<Shape>();
    r->s = out;
    return r;
  } catch (...) {
    fail("split path at corners");
  }
}

// Where a wire turns sharply (by more than `angle` radians): [x, y, z] per corner.
rust::Vec<double> wire_corners(const Shape &w, double angle) {
  try {
    const TopoDS_Wire &wire = as_path(w);
    std::vector<PathEdge> edges = path_edges(wire);
    std::vector<bool> corner = corner_starts(edges, wire_closed(wire), angle);
    rust::Vec<double> out;
    for (size_t i = 0; i < edges.size(); ++i) {
      if (!corner[i]) continue;
      for (double v : {edges[i].p0.X(), edges[i].p0.Y(), edges[i].p0.Z()}) out.push_back(v);
    }
    return out;
  } catch (...) {
    fail("path corners");
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
