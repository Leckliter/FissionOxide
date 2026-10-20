// Threads: a helical groove cut into a cylinder leaves the cylinder's face as one strip per turn,
// because a face's parameters stop at the surface's seam. Merging the strips gives the single
// face a thread's crest is (as other CAD kernels model it): the strips' p-curves move by whole
// periods so the merged face runs on past the seam, once per turn.
#include "fox-kernel/cpp/threads.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <cmath>
#include <map>
#include <numeric>
#include <set>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepTools.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Surface.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec2d.hxx>

namespace fox {

namespace {

bool same_cylinder(const gp_Cylinder &a, const gp_Cylinder &b) {
  double tol = 1e-7 * std::max(1.0, a.Radius());
  if (std::abs(a.Radius() - b.Radius()) > tol) return false;
  if (!a.Axis().IsParallel(b.Axis(), 1e-9)) return false;
  return gp_Lin(a.Axis()).Distance(b.Axis().Location()) <= tol;
}

// The p-curve of an edge occurrence on a face, translated along u.
struct Occurrence {
  TopoDS_Edge edge; // oriented as in the (forward) face
  Handle(Geom2d_Curve) curve;
  double first, last;
  gp_Pnt2d start, end; // in the direction of travel
};

Occurrence occurrence(const TopoDS_Edge &e, const TopoDS_Face &f, double shift) {
  Occurrence o;
  o.edge = e;
  Handle(Geom2d_Curve) c = BRep_Tool::CurveOnSurface(e, f, o.first, o.last);
  if (c.IsNull()) throw Standard_Failure("an edge has no curve on its face");
  o.curve = Handle(Geom2d_Curve)::DownCast(c->Translated(gp_Vec2d(shift, 0.0)));
  gp_Pnt2d a = o.curve->Value(o.first), b = o.curve->Value(o.last);
  bool rev = e.Orientation() == TopAbs_REVERSED;
  o.start = rev ? b : a;
  o.end = rev ? a : b;
  return o;
}

// One face from faces on one cylinder that meet only each other along `internal` edges, or a
// null face when they can't be merged.
TopoDS_Face merge_group(const std::vector<TopoDS_Face> &faces, const TopTools_IndexedMapOfShape &internal) {
  TopAbs_Orientation ori = faces[0].Orientation();
  std::vector<TopoDS_Face> fw;
  for (const TopoDS_Face &f : faces) {
    if (f.Orientation() != ori) return TopoDS_Face();
    fw.push_back(TopoDS::Face(f.Oriented(TopAbs_FORWARD)));
  }
  TopLoc_Location loc;
  Handle(Geom_Surface) surf = BRep_Tool::Surface(fw[0], loc);
  if (surf.IsNull() || !surf->IsUPeriodic()) return TopoDS_Face();
  double period = surf->UPeriod();
  for (size_t i = 1; i < fw.size(); ++i) {
    TopLoc_Location l;
    if (BRep_Tool::Surface(fw[i], l) != surf || !l.IsEqual(loc)) return TopoDS_Face();
  }
  // each face's shift in u, by whole periods, so the faces' p-curves join across shared edges
  std::vector<double> shift(fw.size(), 0.0);
  std::vector<bool> known(fw.size(), false);
  known[0] = true;
  auto u_mid = [](const TopoDS_Edge &e, const TopoDS_Face &f) {
    double a, b;
    Handle(Geom2d_Curve) c = BRep_Tool::CurveOnSurface(e, f, a, b);
    if (c.IsNull()) throw Standard_Failure("an edge has no curve on its face");
    return c->Value((a + b) / 2).X();
  };
  for (bool grew = true; grew;) {
    grew = false;
    for (int k = 1; k <= internal.Extent(); ++k) {
      const TopoDS_Edge &e = TopoDS::Edge(internal(k));
      // the edge as each face uses it: on one surface, its orientation picks its p-curve
      int fi = -1, fj = -1;
      TopoDS_Edge ei, ej;
      for (size_t i = 0; i < fw.size(); ++i) {
        for (TopExp_Explorer x(fw[i], TopAbs_EDGE); x.More(); x.Next()) {
          if (x.Current().IsSame(e)) {
            (fi < 0 ? ei : ej) = TopoDS::Edge(x.Current());
            (fi < 0 ? fi : fj) = static_cast<int>(i);
            break;
          }
        }
      }
      if (fi < 0 || fj < 0) continue;
      if (known[fi] == known[fj]) continue;
      if (known[fj]) {
        std::swap(fi, fj);
        std::swap(ei, ej);
      }
      double d = shift[fi] + u_mid(ei, fw[fi]) - u_mid(ej, fw[fj]);
      shift[fj] = period * std::round(d / period);
      known[fj] = true;
      grew = true;
    }
  }
  for (bool k : known) {
    if (!k) return TopoDS_Face();
  }
  // the outline: every edge occurrence not between two of the faces, its p-curve shifted
  std::vector<Occurrence> occ;
  for (size_t i = 0; i < fw.size(); ++i) {
    for (TopExp_Explorer x(fw[i], TopAbs_EDGE); x.More(); x.Next()) {
      if (internal.Contains(x.Current())) continue;
      occ.push_back(occurrence(TopoDS::Edge(x.Current()), fw[i], shift[i]));
    }
  }
  BRep_Builder bb;
  TopoDS_Face nf;
  bb.MakeFace(nf, surf, loc, BRep_Tool::Tolerance(fw[0]));
  // new p-curves: a seam edge (twice on its face) gets both of its curves
  std::map<const TopoDS_TShape *, std::vector<size_t>> by_edge;
  for (size_t i = 0; i < occ.size(); ++i) by_edge[occ[i].edge.TShape().get()].push_back(i);
  for (auto &[_, list] : by_edge) {
    const Occurrence &a = occ[list[0]];
    double tol = BRep_Tool::Tolerance(a.edge);
    if (list.size() == 1) {
      bb.UpdateEdge(a.edge, a.curve, nf, tol);
    } else if (list.size() == 2) {
      const Occurrence &b = occ[list[1]];
      bool a_fwd = a.edge.Orientation() == TopAbs_FORWARD;
      bb.UpdateEdge(a.edge, a_fwd ? a.curve : b.curve, a_fwd ? b.curve : a.curve, nf, tol);
    } else {
      return TopoDS_Face();
    }
    bb.Range(a.edge, nf, a.first, a.last);
  }
  // chain the occurrences into loops, each starting where the last ended in (u, v)
  std::vector<bool> used(occ.size(), false);
  for (size_t s = 0; s < occ.size(); ++s) {
    if (used[s]) continue;
    TopoDS_Wire w;
    bb.MakeWire(w);
    size_t cur = s;
    used[s] = true;
    bb.Add(w, occ[s].edge);
    for (size_t guard = 0; guard < occ.size(); ++guard) {
      double best = 1e-4;
      size_t next = occ.size();
      for (size_t j = 0; j < occ.size(); ++j) {
        if (used[j]) continue;
        double d = occ[j].start.Distance(occ[cur].end);
        if (d < best) {
          best = d;
          next = j;
        }
      }
      if (next == occ.size()) break;
      used[next] = true;
      bb.Add(w, occ[next].edge);
      cur = next;
    }
    if (occ[cur].end.Distance(occ[s].start) > 1e-4) return TopoDS_Face();
    w.Closed(Standard_True);
    bb.Add(nf, w);
  }
  return TopoDS::Face(nf.Oriented(ori));
}

} // namespace

bool wrapping_axis(const TopoDS_Shape &s, gp_Ax1 &axis) {
  bool found = false;
  for (TopExp_Explorer x(s, TopAbs_FACE); x.More(); x.Next()) {
    const TopoDS_Face &f = TopoDS::Face(x.Current());
    TopLoc_Location loc;
    Handle(Geom_Surface) surf = BRep_Tool::Surface(f, loc);
    if (surf.IsNull() || !surf->IsUPeriodic()) continue;
    double u0, u1, v0, v1;
    BRepTools::UVBounds(f, u0, u1, v0, v1);
    if (u1 - u0 <= surf->UPeriod() * (1.0 + 1e-6)) continue;
    BRepAdaptor_Surface a(f, Standard_False);
    if (a.GetType() != GeomAbs_Cylinder) return false;
    gp_Ax1 ax = a.Cylinder().Axis();
    if (found && !ax.IsParallel(axis, 1e-9)) return false;
    axis = ax;
    found = true;
  }
  return found;
}

std::unique_ptr<Shape> merge_cylinder_strips(const Shape &s) {
  auto out = std::make_unique<Shape>();
  out->s = s.s;
  try {
    // work on a copy of the topology: the merge rewrites edges' curves on the faces
    TopoDS_Shape c = BRepBuilderAPI_Copy(s.s, Standard_False).Shape();
    TopTools_IndexedMapOfShape fmap;
    TopExp::MapShapes(c, TopAbs_FACE, fmap);
    int n = fmap.Extent();
    std::vector<bool> is_cyl(n + 1, false);
    std::vector<gp_Cylinder> cyl(n + 1);
    for (int i = 1; i <= n; ++i) {
      BRepAdaptor_Surface a(TopoDS::Face(fmap(i)), Standard_False);
      if (a.GetType() == GeomAbs_Cylinder) {
        is_cyl[i] = true;
        cyl[i] = a.Cylinder();
      }
    }
    // faces on one cylinder that share edges, joined
    std::vector<int> parent(n + 1);
    std::iota(parent.begin(), parent.end(), 0);
    auto root = [&](int i) {
      while (parent[i] != i) i = parent[i] = parent[parent[i]];
      return i;
    };
    TopTools_IndexedDataMapOfShapeListOfShape ef;
    TopExp::MapShapesAndUniqueAncestors(c, TopAbs_EDGE, TopAbs_FACE, ef);
    TopTools_IndexedMapOfShape internal;
    for (int k = 1; k <= ef.Extent(); ++k) {
      if (ef(k).Extent() != 2) continue;
      int a = fmap.FindIndex(ef(k).First()), b = fmap.FindIndex(ef(k).Last());
      if (a == b || !is_cyl[a] || !is_cyl[b] || !same_cylinder(cyl[a], cyl[b])) continue;
      internal.Add(ef.FindKey(k));
      parent[root(a)] = root(b);
    }
    if (internal.IsEmpty()) return out;
    std::map<int, std::vector<TopoDS_Face>> groups;
    for (int i = 1; i <= n; ++i) {
      if (is_cyl[i]) groups[root(i)].push_back(TopoDS::Face(fmap(i)));
    }
    BRepTools_ReShape reshape;
    bool changed = false;
    for (auto &[_, faces] : groups) {
      if (faces.size() < 2) continue;
      TopoDS_Face merged = merge_group(faces, internal);
      if (merged.IsNull()) continue;
      reshape.Replace(faces[0], merged);
      for (size_t i = 1; i < faces.size(); ++i) reshape.Remove(faces[i]);
      changed = true;
    }
    if (!changed) return out;
    TopoDS_Shape r = reshape.Apply(c);
    if (BRepCheck_Analyzer(r).IsValid()) out->s = r;
  } catch (...) {
  }
  return out;
}

} // namespace fox
