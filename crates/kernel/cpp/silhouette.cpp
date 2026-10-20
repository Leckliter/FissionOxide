// A body's silhouette on a plane: the outline of its shadow cast along the plane's normal, as
// Fusion projects a whole body into a sketch. Hidden-line removal gives every sharp edge and
// outline seen along the normal, flattened onto the plane; the general fuse splits them where they
// cross and merges the ones that land on each other; an edge is kept when the shadow lies on one
// side of it only.
#include "fox-kernel/cpp/silhouette.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <string>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_BuilderAlgo.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepBndLib.hxx>
#include <BRep_Builder.hxx>
#include <BRepLib.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Ax3.hxx>
#include <gp_Lin.hxx>
#include <gp_Trsf.hxx>

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

// A curve that runs straight from one end to the other (a circle seen edge on comes out of the
// hidden-line pass as a flattened ellipse or spline) as a line.
TopoDS_Edge straightened(const TopoDS_Edge &e, const BRepAdaptor_Curve &a) {
  if (a.GetType() == GeomAbs_Line) return e;
  gp_Pnt p0 = a.Value(a.FirstParameter()), p1 = a.Value(a.LastParameter());
  gp_Vec chord(p0, p1);
  double len = chord.Magnitude();
  if (len < 1e-6) return e;
  chord /= len;
  double along = 0;
  const int n = 32;
  for (int i = 1; i < n; ++i) {
    gp_Pnt p = a.Value(a.FirstParameter() + (a.LastParameter() - a.FirstParameter()) * i / n);
    gp_Vec v(p0, p);
    double t = v.Dot(chord);
    if (v.Crossed(chord).Magnitude() > 1e-7 * std::max(1.0, len) || t < along - 1e-9) return e;
    along = t;
  }
  BRepBuilderAPI_MakeEdge mk(p0, p1);
  return mk.IsDone() ? mk.Edge() : e;
}

} // namespace

// plane: origin(3) normal(3) xdir(3), xdir perpendicular to the normal.
std::unique_ptr<Shape> silhouette(const Shape &s, rust::Slice<const double> plane) {
  try {
    if (plane.size() < 9) throw std::runtime_error("plane needs 9 numbers");
    gp_Pnt o(plane[0], plane[1], plane[2]);
    gp_Dir n(plane[3], plane[4], plane[5]);
    gp_Dir x(plane[6], plane[7], plane[8]);
    Bnd_Box box;
    BRepBndLib::Add(s.s, box);
    if (box.IsVoid()) throw std::runtime_error("the body is empty");
    double size = std::sqrt(box.SquareExtent());
    double eps = std::clamp(size * 1e-4, 1e-4, 0.05);

    // everything seen along the normal, in the projector's 2D frame (z = 0)
    Handle(HLRBRep_Algo) algo = new HLRBRep_Algo();
    algo->Add(s.s);
    gp_Ax2 ax(gp::Origin(), n, x);
    algo->Projector(HLRAlgo_Projector(ax));
    algo->Update();
    algo->Hide();
    HLRBRep_HLRToShape h(algo);
    TopTools_ListOfShape edges;
    for (const TopoDS_Shape &c :
         {h.VCompound(), h.Rg1LineVCompound(), h.RgNLineVCompound(), h.OutLineVCompound(), h.HCompound(),
          h.Rg1LineHCompound(), h.RgNLineHCompound(), h.OutLineHCompound()}) {
      if (c.IsNull()) continue;
      for (TopExp_Explorer ex(c, TopAbs_EDGE); ex.More(); ex.Next()) {
        try {
          TopoDS_Edge e = TopoDS::Edge(ex.Current());
          BRepLib::BuildCurves3d(e);
          if (!BRep_Tool::IsGeometric(e)) continue;
          BRepAdaptor_Curve a(e);
          if (GCPnts_AbscissaPoint::Length(a) > 1e-6) edges.Append(straightened(e, a));
        } catch (...) {
          continue;
        }
      }
    }
    if (edges.IsEmpty()) throw std::runtime_error("the body has no outline seen from this plane");

    // split where they cross and merge the ones that coincide (a prism's top and bottom loops)
    BRepAlgoAPI_BuilderAlgo fuse;
    fuse.SetArguments(edges);
    fuse.SetFuzzyValue(1e-6 * std::max(1.0, size));
    fuse.SetRunParallel(false);
    fuse.Build();
    if (fuse.HasErrors()) {
      Standard_SStream ss;
      fuse.DumpErrors(ss);
      throw std::runtime_error("the outline curves could not be joined: " + ss.str());
    }

    // the projector frame placed on the sketch plane: local (u, v) lands where the plane's own
    // coordinates are (u, v) less the origin's
    gp_Vec on(gp_Vec(o.XYZ()).Dot(gp_Vec(n)) * gp_Vec(n));
    gp_Trsf t;
    t.SetDisplacement(gp_Ax3(gp::XOY()), gp_Ax3(gp_Pnt(on.XYZ()), n, x));
    TopoDS_Shape placed = BRepBuilderAPI_Transform(fuse.Shape(), t, Standard_True).Shape();

    IntCurvesFace_ShapeIntersector hit;
    hit.Load(s.s, 1e-7);
    double reach = 10.0 * size + 1e3 + gp_Vec(box.CornerMin().XYZ()).Magnitude() + gp_Vec(box.CornerMax().XYZ()).Magnitude();
    auto shadowed = [&](const gp_Pnt &q) {
      hit.Perform(gp_Lin(q, n), -reach, reach);
      return hit.IsDone() && hit.NbPnt() > 0;
    };

    TopoDS_Compound out;
    BRep_Builder b;
    b.MakeCompound(out);
    int kept = 0;
    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(placed, TopAbs_EDGE, map);
    for (int i = 1; i <= map.Extent(); ++i) {
      try {
        const TopoDS_Edge &e = TopoDS::Edge(map(i));
        BRepAdaptor_Curve a(e);
        gp_Pnt p;
        gp_Vec d;
        a.D1(0.5 * (a.FirstParameter() + a.LastParameter()), p, d);
        gp_Vec side = gp_Vec(n).Crossed(d);
        if (side.Magnitude() < 1e-12) continue;
        side.Normalize();
        side *= eps;
        if (shadowed(p.Translated(side)) != shadowed(p.Translated(-side))) {
          b.Add(out, e);
          ++kept;
        }
      } catch (...) {
        continue;
      }
    }
    if (kept == 0) throw std::runtime_error("the body casts no outline on this plane");
    auto r = std::make_unique<Shape>();
    r->s = out;
    return r;
  } catch (...) {
    fail("silhouette");
  }
}

} // namespace fox
