// C++ side of the CLFusion kernel bridge. See kernel.hxx.
#include "clf-kernel/cpp/kernel.hxx"
#include "clf-kernel/src/ffi.rs.h"

#include <cmath>
#include <deque>
#include <map>
#include <sstream>
#include <vector>
#include <stdexcept>
#include <string>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepOffsetAPI_MakePipe.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <TopoDS_Iterator.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GCE2d_MakeSegment.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <BRepLib.hxx>
#include <TColgp_HArray1OfPnt2d.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomLProp_SLProps.hxx>
#include <Geom_Surface.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <GProp_GProps.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepLib.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <IGESControl_Controller.hxx>
#include <IGESControl_Reader.hxx>
#include <IGESControl_Writer.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <Interface_Static.hxx>
#include <Poly_Triangulation.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeFix_Shape.hxx>
#include <ShapeFix_Solid.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>

namespace clf {

namespace {

// Converts any exception into std::runtime_error (which cxx maps to Err).
[[noreturn]] void rethrow(const char *what) {
  try {
    throw;
  } catch (const Standard_Failure &e) {
    const char *msg = e.GetMessageString();
    std::string m = std::string(what) + ": " + (msg && *msg ? msg : e.DynamicType()->Name());
    throw std::runtime_error(m);
  } catch (const std::exception &e) {
    throw std::runtime_error(std::string(what) + ": " + e.what());
  } catch (...) {
    throw std::runtime_error(std::string(what) + ": unknown kernel error");
  }
}

#define CLF_TRY try {
#define CLF_CATCH(what)                                                                                                \
  }                                                                                                                    \
  catch (...) { rethrow(what); }

// OCCT's data exchange prints progress to stdout; keep the console (and JSON output) clean.
void quiet() {
  static bool done = false;
  if (!done) {
    done = true;
    Message::DefaultMessenger()->RemovePrinters(STANDARD_TYPE(Message_PrinterOStream));
  }
}

std::unique_ptr<Shape> wrap(const TopoDS_Shape &s) {
  if (s.IsNull()) {
    throw std::runtime_error("kernel returned an empty shape");
  }
  auto out = std::make_unique<Shape>();
  out->s = s;
  return out;
}

gp_Pnt pnt(rust::Slice<const double> v, size_t i) { return gp_Pnt(v[i], v[i + 1], v[i + 2]); }
gp_Dir dir(rust::Slice<const double> v, size_t i) { return gp_Dir(v[i], v[i + 1], v[i + 2]); }

void need(rust::Slice<const double> v, size_t n, const char *what) {
  if (v.size() < n) {
    throw std::runtime_error(std::string(what) + ": too few values");
  }
}

// Validates a result; tries ShapeFix once if invalid.
TopoDS_Shape checked(const TopoDS_Shape &s, const char *what) {
  if (s.IsNull()) {
    throw std::runtime_error(std::string(what) + " produced nothing");
  }
  BRepCheck_Analyzer an(s);
  if (an.IsValid()) {
    return s;
  }
  ShapeFix_Shape fix(s);
  fix.Perform();
  TopoDS_Shape f = fix.Shape();
  BRepCheck_Analyzer an2(f);
  if (!an2.IsValid()) {
    throw std::runtime_error(std::string(what) + " produced an invalid solid");
  }
  return f;
}

void index_map(const TopoDS_Shape &s, TopAbs_ShapeEnum kind, TopTools_IndexedMapOfShape &map) {
  TopExp::MapShapes(s, kind, map);
}

std::array<double, 3> arr(const gp_XYZ &p) { return {p.X(), p.Y(), p.Z()}; }

} // namespace

std::unique_ptr<Shape> shape_clone(const Shape &s) {
  auto out = std::make_unique<Shape>();
  out->s = s.s;
  return out;
}

bool shape_is_null(const Shape &s) { return s.s.IsNull(); }

int32_t shape_type(const Shape &s) { return s.s.IsNull() ? -1 : static_cast<int32_t>(s.s.ShapeType()); }

namespace {
// Edge `seg` of a segment list (10 doubles each, see make_face).
TopoDS_Edge seg_edge(rust::Slice<const double> segs, size_t seg) {
  size_t o = seg * 10;
  need(segs, o + 10, "segments");
  int kind = static_cast<int>(segs[o]);
  if (kind == 0) {
    BRepBuilderAPI_MakeEdge me(pnt(segs, o + 1), pnt(segs, o + 4));
    if (!me.IsDone()) throw std::runtime_error("degenerate line in profile");
    return me.Edge();
  }
  if (kind == 1) {
    GC_MakeArcOfCircle arc(pnt(segs, o + 1), pnt(segs, o + 4), pnt(segs, o + 7));
    if (!arc.IsDone()) throw std::runtime_error("degenerate arc in profile");
    BRepBuilderAPI_MakeEdge me(arc.Value());
    if (!me.IsDone()) throw std::runtime_error("degenerate arc in profile");
    return me.Edge();
  }
  gp_Circ c(gp_Ax2(pnt(segs, o + 1), dir(segs, o + 4)), segs[o + 7]);
  BRepBuilderAPI_MakeEdge me(c);
  if (!me.IsDone()) throw std::runtime_error("degenerate circle in profile");
  return me.Edge();
}
} // namespace

// plane: origin(3) normal(3) xdir(3)
// segs: 10 doubles each: kind (0 line, 1 arc through 3 points, 2 circle), p0, p1, p2
//   line: p0 → p1;  arc: start p0, interior p1, end p2;  circle: centre p0, normal p1, radius p2.x
// loop_lens: number of segments per loop; the first loop is the outer boundary, the rest holes.
std::unique_ptr<Shape> make_face(rust::Slice<const double> plane, rust::Slice<const double> segs,
                                 rust::Slice<const uint32_t> loop_lens) {
  CLF_TRY
  need(plane, 9, "make_face plane");
  gp_Pln pln(gp_Ax3(pnt(plane, 0), dir(plane, 3), dir(plane, 6)));
  size_t seg = 0;
  std::vector<TopoDS_Wire> wires;
  for (uint32_t n : loop_lens) {
    BRepBuilderAPI_MakeWire mw;
    for (uint32_t k = 0; k < n; ++k, ++seg) {
      TopoDS_Edge e = seg_edge(segs, seg);
      mw.Add(e);
      if (!mw.IsDone()) throw std::runtime_error("profile loop is not connected");
    }
    if (!mw.IsDone()) throw std::runtime_error("empty profile loop");
    wires.push_back(mw.Wire());
  }
  if (wires.empty()) throw std::runtime_error("profile has no loops");
  BRepBuilderAPI_MakeFace mf(pln, wires[0], Standard_True);
  for (size_t i = 1; i < wires.size(); ++i) {
    mf.Add(wires[i]);
  }
  if (!mf.IsDone()) throw std::runtime_error("could not make a face from the profile");
  ShapeFix_Face fix(mf.Face());
  fix.FixOrientation();
  fix.Perform();
  return wrap(fix.Face());
  CLF_CATCH("profile")
}

std::unique_ptr<Shape> make_prism(const Shape &face, double dx, double dy, double dz) {
  CLF_TRY
  BRepPrimAPI_MakePrism mk(face.s, gp_Vec(dx, dy, dz), Standard_True);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("extrusion failed");
  return wrap(checked(mk.Shape(), "extrusion"));
  CLF_CATCH("extrude")
}

// axis: origin(3) dir(3)
std::unique_ptr<Shape> make_revol(const Shape &face, rust::Slice<const double> axis, double angle) {
  CLF_TRY
  need(axis, 6, "revolve axis");
  BRepPrimAPI_MakeRevol mk(face.s, gp_Ax1(pnt(axis, 0), dir(axis, 3)), angle, Standard_True);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("revolve failed");
  return wrap(checked(mk.Shape(), "revolve"));
  CLF_CATCH("revolve")
}

std::unique_ptr<Shape> make_box(double x0, double y0, double z0, double x1, double y1, double z1) {
  CLF_TRY
  BRepPrimAPI_MakeBox mk(gp_Pnt(x0, y0, z0), gp_Pnt(x1, y1, z1));
  return wrap(mk.Shape());
  CLF_CATCH("box")
}

std::unique_ptr<Shape> make_cylinder(rust::Slice<const double> axis, double radius, double height) {
  CLF_TRY
  need(axis, 6, "cylinder axis");
  BRepPrimAPI_MakeCylinder mk(gp_Ax2(pnt(axis, 0), dir(axis, 3)), radius, height);
  return wrap(mk.Shape());
  CLF_CATCH("cylinder")
}

std::unique_ptr<Shape> make_sphere(double cx, double cy, double cz, double radius) {
  CLF_TRY
  BRepPrimAPI_MakeSphere mk(gp_Pnt(cx, cy, cz), radius);
  return wrap(mk.Shape());
  CLF_CATCH("sphere")
}

std::unique_ptr<Shape> make_torus(rust::Slice<const double> axis, double major, double minor) {
  CLF_TRY
  need(axis, 6, "torus axis");
  BRepPrimAPI_MakeTorus mk(gp_Ax2(pnt(axis, 0), dir(axis, 3)), major, minor);
  return wrap(mk.Shape());
  CLF_CATCH("torus")
}

namespace {
template <typename Op> TopoDS_Shape run_boolean(const TopoDS_Shape &a, const TopoDS_Shape &b, double fuzzy) {
  Op op;
  TopTools_ListOfShape args, tools;
  args.Append(a);
  tools.Append(b);
  op.SetArguments(args);
  op.SetTools(tools);
  if (fuzzy > 0.0) op.SetFuzzyValue(fuzzy);
  op.SetNonDestructive(Standard_True);
  op.Build();
  if (!op.IsDone() || op.HasErrors()) throw std::runtime_error("boolean operation failed");
  op.SimplifyResult(Standard_True, Standard_True);
  return op.Shape();
}
} // namespace

// op: 0 = union, 1 = subtract (a − b), 2 = intersect
std::unique_ptr<Shape> boolean_op(const Shape &a, const Shape &b, int32_t op, double fuzzy) {
  CLF_TRY
  TopoDS_Shape r;
  switch (op) {
  case 0: r = run_boolean<BRepAlgoAPI_Fuse>(a.s, b.s, fuzzy); break;
  case 1: r = run_boolean<BRepAlgoAPI_Cut>(a.s, b.s, fuzzy); break;
  case 2: r = run_boolean<BRepAlgoAPI_Common>(a.s, b.s, fuzzy); break;
  default: throw std::runtime_error("unknown boolean operation");
  }
  auto out = std::make_unique<Shape>();
  out->s = r.IsNull() ? r : checked(r, "boolean");
  return out;
  CLF_CATCH("boolean")
}

std::unique_ptr<Shape> fillet(const Shape &s, rust::Slice<const uint32_t> edges, double radius) {
  CLF_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_EDGE, map);
  BRepFilletAPI_MakeFillet mk(s.s);
  int added = 0;
  for (uint32_t i : edges) {
    if (i >= 1 && static_cast<int>(i) <= map.Extent()) {
      mk.Add(radius, TopoDS::Edge(map(static_cast<int>(i))));
      ++added;
    }
  }
  if (added == 0) throw std::runtime_error("no edges to fillet");
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the fillet could not be computed (radius too large?)");
  return wrap(checked(mk.Shape(), "fillet"));
  CLF_CATCH("fillet")
}

std::unique_ptr<Shape> chamfer(const Shape &s, rust::Slice<const uint32_t> edges, double dist) {
  CLF_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_EDGE, map);
  BRepFilletAPI_MakeChamfer mk(s.s);
  int added = 0;
  for (uint32_t i : edges) {
    if (i >= 1 && static_cast<int>(i) <= map.Extent()) {
      mk.Add(dist, TopoDS::Edge(map(static_cast<int>(i))));
      ++added;
    }
  }
  if (added == 0) throw std::runtime_error("no edges to chamfer");
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the chamfer could not be computed (distance too large?)");
  return wrap(checked(mk.Shape(), "chamfer"));
  CLF_CATCH("chamfer")
}

// Hollows a solid, removing `faces` (1-based); negative thickness keeps the outside surface.
std::unique_ptr<Shape> shell(const Shape &s, rust::Slice<const uint32_t> faces, double thickness) {
  CLF_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_FACE, map);
  TopTools_ListOfShape remove;
  for (uint32_t i : faces) {
    if (i >= 1 && static_cast<int>(i) <= map.Extent()) remove.Append(map(static_cast<int>(i)));
  }
  BRepOffsetAPI_MakeThickSolid mk;
  mk.MakeThickSolidByJoin(s.s, remove, thickness, 1e-3, BRepOffset_Skin, Standard_False, Standard_False, GeomAbs_Arc);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("shell failed (thickness too large for the geometry?)");
  return wrap(checked(mk.Shape(), "shell"));
  CLF_CATCH("shell")
}

// m: 3x4 row-major affine matrix
std::unique_ptr<Shape> transform(const Shape &s, rust::Slice<const double> m) {
  CLF_TRY
  need(m, 12, "transform matrix");
  gp_Trsf t;
  t.SetValues(m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8], m[9], m[10], m[11]);
  BRepBuilderAPI_Transform mk(s.s, t, Standard_True);
  if (!mk.IsDone()) throw std::runtime_error("transform failed");
  return wrap(mk.Shape());
  CLF_CATCH("transform")
}

// plane: origin(3) normal(3)
std::unique_ptr<Shape> mirror(const Shape &s, rust::Slice<const double> plane) {
  CLF_TRY
  need(plane, 6, "mirror plane");
  gp_Trsf t;
  t.SetMirror(gp_Ax2(pnt(plane, 0), dir(plane, 3)));
  BRepBuilderAPI_Transform mk(s.s, t, Standard_True);
  if (!mk.IsDone()) throw std::runtime_error("mirror failed");
  return wrap(mk.Shape());
  CLF_CATCH("mirror")
}

std::unique_ptr<Shape> unify(const Shape &s) {
  CLF_TRY
  ShapeUpgrade_UnifySameDomain u(s.s, Standard_True, Standard_True, Standard_False);
  u.Build();
  return wrap(u.Shape());
  CLF_CATCH("unify")
}

uint32_t solid_count(const Shape &s) {
  if (s.s.IsNull()) return 0;
  uint32_t n = 0;
  for (TopExp_Explorer ex(s.s, TopAbs_SOLID); ex.More(); ex.Next()) ++n;
  return n;
}

std::unique_ptr<Shape> solid_at(const Shape &s, uint32_t i) {
  CLF_TRY
  uint32_t n = 0;
  for (TopExp_Explorer ex(s.s, TopAbs_SOLID); ex.More(); ex.Next(), ++n) {
    if (n == i) return wrap(ex.Current());
  }
  throw std::runtime_error("no such solid");
  CLF_CATCH("solid")
}

std::unique_ptr<Shape> compound_new() {
  auto out = std::make_unique<Shape>();
  TopoDS_Compound c;
  BRep_Builder b;
  b.MakeCompound(c);
  out->s = c;
  return out;
}

void compound_add(Shape &c, const Shape &s) {
  CLF_TRY
  if (s.s.IsNull()) return;
  BRep_Builder b;
  b.Add(c.s, s.s);
  CLF_CATCH("compound")
}

MeshData mesh_shape(const Shape &s, double linear, double angular) {
  CLF_TRY
  MeshData out;
  BRepMesh_IncrementalMesh mesher(s.s, linear, Standard_False, angular, Standard_True);
  TopTools_IndexedMapOfShape faces;
  index_map(s.s, TopAbs_FACE, faces);
  for (int fi = 1; fi <= faces.Extent(); ++fi) {
    const TopoDS_Face &face = TopoDS::Face(faces(fi));
    TopLoc_Location loc;
    Handle(Poly_Triangulation) tri = BRep_Tool::Triangulation(face, loc);
    if (tri.IsNull()) continue;
    if (!tri->HasNormals()) BRepLib_ToolTriangulatedShape::ComputeNormals(face, tri);
    const gp_Trsf &tr = loc.Transformation();
    bool reversed = face.Orientation() == TopAbs_REVERSED;
    uint32_t base = static_cast<uint32_t>(out.positions.size() / 3);
    for (int i = 1; i <= tri->NbNodes(); ++i) {
      gp_Pnt p = tri->Node(i).Transformed(tr);
      out.positions.push_back(p.X());
      out.positions.push_back(p.Y());
      out.positions.push_back(p.Z());
      gp_Dir n = tri->HasNormals() ? tri->Normal(i) : gp_Dir(0, 0, 1);
      n.Transform(tr);
      if (reversed) n.Reverse();
      out.normals.push_back(n.X());
      out.normals.push_back(n.Y());
      out.normals.push_back(n.Z());
    }
    for (int i = 1; i <= tri->NbTriangles(); ++i) {
      int a, b, c;
      tri->Triangle(i).Get(a, b, c);
      if (reversed) std::swap(b, c);
      out.triangles.push_back(base + a - 1);
      out.triangles.push_back(base + b - 1);
      out.triangles.push_back(base + c - 1);
      out.tri_face.push_back(static_cast<uint32_t>(fi));
    }
  }
  return out;
  CLF_CATCH("mesh")
}

EdgeData edge_polylines(const Shape &s, double deflection) {
  CLF_TRY
  EdgeData out;
  TopTools_IndexedMapOfShape edges;
  index_map(s.s, TopAbs_EDGE, edges);
  out.offsets.push_back(0);
  for (int ei = 1; ei <= edges.Extent(); ++ei) {
    const TopoDS_Edge &e = TopoDS::Edge(edges(ei));
    if (BRep_Tool::Degenerated(e)) continue;
    try {
      BRepAdaptor_Curve c(e);
      GCPnts_TangentialDeflection disc(c, 0.1, deflection);
      for (int i = 1; i <= disc.NbPoints(); ++i) {
        gp_Pnt p = disc.Value(i);
        out.points.push_back(p.X());
        out.points.push_back(p.Y());
        out.points.push_back(p.Z());
      }
    } catch (...) {
      continue;
    }
    out.offsets.push_back(static_cast<uint32_t>(out.points.size() / 3));
    out.edge_ids.push_back(static_cast<uint32_t>(ei));
  }
  return out;
  CLF_CATCH("edges")
}

rust::Vec<FaceInfo> face_infos(const Shape &s) {
  CLF_TRY
  rust::Vec<FaceInfo> out;
  TopTools_IndexedMapOfShape faces;
  index_map(s.s, TopAbs_FACE, faces);
  for (int fi = 1; fi <= faces.Extent(); ++fi) {
    const TopoDS_Face &face = TopoDS::Face(faces(fi));
    FaceInfo info{};
    GProp_GProps props;
    BRepGProp::SurfaceProperties(face, props);
    info.area = props.Mass();
    info.center = arr(props.CentreOfMass().XYZ());
    bool reversed = face.Orientation() == TopAbs_REVERSED;
    BRepAdaptor_Surface surf(face, Standard_True);
    switch (surf.GetType()) {
    case GeomAbs_Plane: {
      gp_Pln p = surf.Plane();
      gp_Dir n = p.Axis().Direction();
      if (reversed) n.Reverse();
      gp_Dir x = p.XAxis().Direction();
      info.kind = 0;
      info.origin = info.center;
      info.normal = arr(n.XYZ());
      info.xdir = arr(x.XYZ());
      break;
    }
    case GeomAbs_Cylinder: {
      gp_Cylinder c = surf.Cylinder();
      info.kind = 1;
      info.origin = arr(c.Location().XYZ());
      info.normal = arr(c.Axis().Direction().XYZ());
      info.radius = c.Radius();
      break;
    }
    case GeomAbs_Cone: info.kind = 2; break;
    case GeomAbs_Sphere: {
      info.kind = 3;
      info.origin = arr(surf.Sphere().Location().XYZ());
      info.radius = surf.Sphere().Radius();
      break;
    }
    case GeomAbs_Torus: info.kind = 4; break;
    default: info.kind = 5; break;
    }
    out.push_back(info);
  }
  return out;
  CLF_CATCH("faces")
}

rust::Vec<EdgeInfo> edge_infos(const Shape &s) {
  CLF_TRY
  rust::Vec<EdgeInfo> out;
  TopTools_IndexedMapOfShape edges, faces;
  index_map(s.s, TopAbs_EDGE, edges);
  index_map(s.s, TopAbs_FACE, faces);
  TopTools_IndexedDataMapOfShapeListOfShape edge_faces;
  TopExp::MapShapesAndAncestors(s.s, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  for (int ei = 1; ei <= edges.Extent(); ++ei) {
    const TopoDS_Edge &e = TopoDS::Edge(edges(ei));
    EdgeInfo info{};
    if (edge_faces.Contains(e)) {
      const TopTools_ListOfShape &fl = edge_faces.FindFromKey(e);
      int k = 0;
      for (TopTools_ListIteratorOfListOfShape it(fl); it.More() && k < 2; it.Next()) {
        int idx = faces.FindIndex(it.Value());
        if (idx <= 0) continue;
        if (k == 0) {
          info.face1 = static_cast<uint32_t>(idx);
          ++k;
        } else if (static_cast<uint32_t>(idx) != info.face1) {
          info.face2 = static_cast<uint32_t>(idx);
          ++k;
        }
      }
    }
    if (BRep_Tool::Degenerated(e)) {
      info.kind = 3;
      out.push_back(info);
      continue;
    }
    BRepAdaptor_Curve c(e);
    double a = c.FirstParameter(), b = c.LastParameter();
    info.p0 = arr(c.Value(a).XYZ());
    info.p1 = arr(c.Value(b).XYZ());
    info.mid = arr(c.Value((a + b) / 2).XYZ());
    GProp_GProps props;
    BRepGProp::LinearProperties(e, props);
    info.length = props.Mass();
    switch (c.GetType()) {
    case GeomAbs_Line: info.kind = 0; break;
    case GeomAbs_Circle: {
      info.kind = 1;
      gp_Circ ci = c.Circle();
      info.center = arr(ci.Location().XYZ());
      info.axis = arr(ci.Axis().Direction().XYZ());
      info.radius = ci.Radius();
      break;
    }
    default: info.kind = 2; break;
    }
    out.push_back(info);
  }
  return out;
  CLF_CATCH("edge info")
}

MassProps mass_props(const Shape &s) {
  CLF_TRY
  MassProps out{};
  GProp_GProps vol;
  BRepGProp::VolumeProperties(s.s, vol);
  out.volume = vol.Mass();
  out.cog = arr(vol.CentreOfMass().XYZ());
  GProp_GProps surf;
  BRepGProp::SurfaceProperties(s.s, surf);
  out.area = surf.Mass();
  Bnd_Box box;
  BRepBndLib::Add(s.s, box);
  if (!box.IsVoid()) {
    double x0, y0, z0, x1, y1, z1;
    box.Get(x0, y0, z0, x1, y1, z1);
    out.bbox_min = {x0, y0, z0};
    out.bbox_max = {x1, y1, z1};
  }
  return out;
  CLF_CATCH("mass properties")
}

bool is_valid(const Shape &s) {
  try {
    if (s.s.IsNull()) return false;
    BRepCheck_Analyzer an(s.s);
    return an.IsValid();
  } catch (...) {
    return false;
  }
}

void write_step(const Shape &s, rust::Str path) {
  CLF_TRY
  quiet();
  STEPControl_Writer w;
  Interface_Static::SetCVal("write.step.unit", "MM");
  Interface_Static::SetCVal("write.step.schema", "AP214IS");
  if (w.Transfer(s.s, STEPControl_AsIs) != IFSelect_RetDone) throw std::runtime_error("STEP transfer failed");
  std::string p(path.data(), path.size());
  if (w.Write(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("could not write " + p);
  CLF_CATCH("STEP export")
}

std::unique_ptr<Shape> read_step(rust::Str path) {
  CLF_TRY
  quiet();
  STEPControl_Reader r;
  std::string p(path.data(), path.size());
  if (r.ReadFile(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("could not read " + p);
  r.TransferRoots();
  return wrap(r.OneShape());
  CLF_CATCH("STEP import")
}

rust::String write_brep(const Shape &s) {
  CLF_TRY
  std::ostringstream os;
  BRepTools::Write(s.s, os);
  return rust::String(os.str());
  CLF_CATCH("BRep write")
}

std::unique_ptr<Shape> read_brep(rust::Str data) {
  CLF_TRY
  std::istringstream is(std::string(data.data(), data.size()));
  TopoDS_Shape shape;
  BRep_Builder b;
  BRepTools::Read(shape, is, b);
  return wrap(shape);
  CLF_CATCH("BRep read")
}

std::unique_ptr<Shape> face_at_index(const Shape &s, uint32_t face) {
  CLF_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_FACE, map);
  if (face < 1 || static_cast<int>(face) > map.Extent()) throw std::runtime_error("no such face");
  return wrap(map(static_cast<int>(face)));
  CLF_CATCH("face")
}

// An open or closed chain of segments (see make_face) as a wire.
std::unique_ptr<Shape> make_wire(rust::Slice<const double> segs) {
  CLF_TRY
  BRepBuilderAPI_MakeWire mw;
  for (size_t i = 0; i * 10 < segs.size(); ++i) {
    mw.Add(seg_edge(segs, i));
    if (!mw.IsDone()) throw std::runtime_error("the path is not connected");
  }
  if (!mw.IsDone()) throw std::runtime_error("empty path");
  return wrap(mw.Wire());
  CLF_CATCH("path")
}

// Sweeps a planar face along a wire (corrected Frenet frame, as Fusion's default orientation).
std::unique_ptr<Shape> make_pipe(const Shape &face, const Shape &spine) {
  CLF_TRY
  if (spine.s.ShapeType() != TopAbs_WIRE) throw std::runtime_error("the sweep path must be a wire");
  BRepOffsetAPI_MakePipe mk(TopoDS::Wire(spine.s), face.s, GeomFill_IsCorrectedFrenet, Standard_False);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the sweep failed");
  return wrap(checked(mk.Shape(), "sweep"));
  CLF_CATCH("sweep")
}

// Lofts through the outer wires of the faces (or the wires) of a compound, in order.
std::unique_ptr<Shape> make_loft(const Shape &sections, bool ruled, bool closed) {
  CLF_TRY
  BRepOffsetAPI_ThruSections mk(Standard_True, ruled ? Standard_True : Standard_False, 1e-6);
  int n = 0;
  TopoDS_Wire first;
  for (TopoDS_Iterator it(sections.s); it.More(); it.Next(), ++n) {
    const TopoDS_Shape &sh = it.Value();
    TopoDS_Wire w;
    if (sh.ShapeType() == TopAbs_FACE) {
      w = BRepTools::OuterWire(TopoDS::Face(sh));
    } else if (sh.ShapeType() == TopAbs_WIRE) {
      w = TopoDS::Wire(sh);
    } else {
      throw std::runtime_error("loft sections must be profiles");
    }
    if (n == 0) first = w;
    mk.AddWire(w);
  }
  if (n < 2) throw std::runtime_error("a loft needs at least two profiles");
  if (closed) mk.AddWire(first);
  mk.CheckCompatibility(Standard_True);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the loft failed (are the profiles compatible?)");
  return wrap(checked(mk.Shape(), "loft"));
  CLF_CATCH("loft")
}

// Splits a solid with an infinite plane (origin(3) normal(3)); the result holds every piece.
std::unique_ptr<Shape> split_by_plane(const Shape &s, rust::Slice<const double> plane, double size) {
  CLF_TRY
  need(plane, 6, "split plane");
  gp_Pln pln(pnt(plane, 0), dir(plane, 3));
  BRepBuilderAPI_MakeFace mf(pln, -size, size, -size, size);
  if (!mf.IsDone()) throw std::runtime_error("could not make the splitting plane");
  BRepAlgoAPI_Splitter sp;
  TopTools_ListOfShape args, tools;
  args.Append(s.s);
  tools.Append(mf.Face());
  sp.SetArguments(args);
  sp.SetTools(tools);
  sp.SetFuzzyValue(1e-6);
  sp.Build();
  if (!sp.IsDone() || sp.HasErrors()) throw std::runtime_error("the split failed");
  return wrap(sp.Shape());
  CLF_CATCH("split")
}

// Tapers faces (1-based) by `angle` (rad) about the neutral plane (origin(3) normal(3)), pulling
// along the plane normal.
std::unique_ptr<Shape> draft(const Shape &s, rust::Slice<const uint32_t> faces, rust::Slice<const double> plane,
                             double angle) {
  CLF_TRY
  need(plane, 6, "draft plane");
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_FACE, map);
  gp_Pln neutral(pnt(plane, 0), dir(plane, 3));
  gp_Dir pull = dir(plane, 3);
  BRepOffsetAPI_DraftAngle mk(s.s);
  int added = 0;
  for (uint32_t i : faces) {
    if (i < 1 || static_cast<int>(i) > map.Extent()) continue;
    mk.Add(TopoDS::Face(map(static_cast<int>(i))), pull, angle, neutral);
    if (!mk.AddDone()) throw std::runtime_error("a face can't be drafted (is it parallel to the pull direction?)");
    ++added;
  }
  if (added == 0) throw std::runtime_error("no faces to draft");
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the draft failed");
  return wrap(checked(mk.Shape(), "draft"));
  CLF_CATCH("draft")
}

// ---------------------------------------------------------------- sheet metal unfolding
//
// A sheet metal body is two parallel "skins" joined by thin side walls. Starting from the
// stationary face, the unfolder walks the skin it lies on: across edges where the surface is
// tangent-continuous (planes and the cylinders of bends), never onto the side walls, which meet
// the skin at a right angle. Planar faces are moved rigidly into the stationary face's plane;
// bend cylinders are rolled out to a strip as wide as the neutral layer's arc (angle × (r + k·t)).
// The flat faces are fused and thickened by t, so holes and cuts anywhere on the skin, including
// across bends, come out in the flat pattern.

namespace {

struct Place {
  bool cyl = false;
  gp_Trsf T; // planar faces: 3D → flat
  // bends: axis, start radial direction, rotation sense, radii and the flat frame
  gp_Pnt O;
  gp_Vec a, r0, b; // b = sense · (a × r0): direction of increasing angle at the start line
  double R = 0, Rn = 0;
  bool inner = false;
  bool repeat = false; // another face of a bend already placed
  gp_Pnt P0;
  gp_Vec A, V;
};

gp_Vec face_normal(const TopoDS_Face &F, const gp_Pnt &P) {
  Handle(Geom_Surface) S = BRep_Tool::Surface(F);
  GeomAPI_ProjectPointOnSurf proj(P, S);
  if (proj.NbPoints() == 0) throw std::runtime_error("unfold: could not evaluate a face normal");
  double u, v;
  proj.LowerDistanceParameters(u, v);
  GeomLProp_SLProps props(S, u, v, 1, 1e-9);
  if (!props.IsNormalDefined()) throw std::runtime_error("unfold: undefined face normal");
  gp_Vec n(props.Normal());
  if (F.Orientation() == TopAbs_REVERSED) n.Reverse();
  return n;
}

gp_Pnt edge_mid(const TopoDS_Edge &E) {
  BRepAdaptor_Curve c(E);
  return c.Value((c.FirstParameter() + c.LastParameter()) / 2);
}

// A point well inside the face (the middle of its parameter range).
gp_Pnt face_inner_point(const TopoDS_Face &F) {
  BRepAdaptor_Surface s(F, Standard_True);
  return s.Value((s.FirstUParameter() + s.LastUParameter()) / 2, (s.FirstVParameter() + s.LastVParameter()) / 2);
}

double bend_angle(const Place &p, const gp_Pnt &X) {
  gp_Vec q(p.O, X);
  q -= p.a * q.Dot(p.a);
  double phi = std::atan2(q.Dot(p.b), q.Dot(p.r0));
  if (phi < -0.25) phi += 2 * M_PI;
  return phi;
}

gp_Pnt bend_map(const Place &p, const gp_Pnt &X) {
  double s = gp_Vec(p.O, X).Dot(p.a);
  double phi = bend_angle(p, X);
  return p.P0.Translated(p.A * s + p.V * (phi * p.Rn));
}

TopoDS_Edge flat_edge(const TopoDS_Edge &e, const Place &p) {
  TopoDS_Vertex v1, v2;
  TopExp::Vertices(e, v1, v2, Standard_True);
  gp_Pnt q1 = bend_map(p, BRep_Tool::Pnt(v1)), q2 = bend_map(p, BRep_Tool::Pnt(v2));
  BRepAdaptor_Curve c(e);
  bool straight = c.GetType() == GeomAbs_Line ||
                  (c.GetType() == GeomAbs_Circle && c.Circle().Axis().Direction().IsParallel(gp_Dir(p.a), 1e-6));
  if (straight) {
    BRepBuilderAPI_MakeEdge me(q1, q2);
    if (!me.IsDone()) throw std::runtime_error("unfold: degenerate edge");
    return me.Edge();
  }
  const int n = 32;
  Handle(TColgp_HArray1OfPnt) pts = new TColgp_HArray1OfPnt(1, n + 1);
  double f = c.FirstParameter(), l = c.LastParameter();
  bool rev = e.Orientation() == TopAbs_REVERSED;
  for (int i = 0; i <= n; ++i) {
    double t = f + (l - f) * i / n;
    if (rev) t = l - (l - f) * i / n;
    pts->SetValue(i + 1, bend_map(p, c.Value(t)));
  }
  pts->SetValue(1, q1);
  pts->SetValue(n + 1, q2);
  GeomAPI_Interpolate interp(pts, Standard_False, 1e-7);
  interp.Perform();
  if (!interp.IsDone()) throw std::runtime_error("unfold: could not flatten a curve on a bend");
  BRepBuilderAPI_MakeEdge me(interp.Curve(), q1, q2);
  if (!me.IsDone()) throw std::runtime_error("unfold: could not flatten a curve on a bend");
  return me.Edge();
}

TopoDS_Face flat_bend_face(const TopoDS_Face &G, const Place &p, const gp_Pln &pln) {
  TopoDS_Wire outer = BRepTools::OuterWire(G);
  std::vector<TopoDS_Wire> wires;
  for (TopExp_Explorer wx(G, TopAbs_WIRE); wx.More(); wx.Next()) {
    const TopoDS_Wire &w = TopoDS::Wire(wx.Current());
    BRepBuilderAPI_MakeWire mw;
    for (BRepTools_WireExplorer we(w, G); we.More(); we.Next()) {
      const TopoDS_Edge &e = we.Current();
      if (BRep_Tool::Degenerated(e)) continue;
      TopoDS_Vertex v1, v2;
      TopExp::Vertices(e, v1, v2, Standard_True);
      if (bend_map(p, BRep_Tool::Pnt(v1)).Distance(bend_map(p, BRep_Tool::Pnt(v2))) < 1e-7 &&
          BRep_Tool::IsClosed(e, G)) {
        continue;
      }
      mw.Add(flat_edge(e, p));
      if (!mw.IsDone()) throw std::runtime_error("unfold: a bend outline did not close");
    }
    if (!mw.IsDone()) continue;
    if (w.IsSame(outer)) {
      wires.insert(wires.begin(), mw.Wire());
    } else {
      wires.push_back(mw.Wire());
    }
  }
  if (wires.empty()) throw std::runtime_error("unfold: a bend has no outline");
  BRepBuilderAPI_MakeFace mf(pln, wires[0], Standard_True);
  for (size_t i = 1; i < wires.size(); ++i) mf.Add(wires[i]);
  if (!mf.IsDone()) throw std::runtime_error("unfold: could not flatten a bend");
  ShapeFix_Face fix(mf.Face());
  fix.FixOrientation();
  fix.Perform();
  return fix.Face();
}

TopoDS_Shape fuse_all(const std::vector<TopoDS_Shape> &shapes, double fuzzy) {
  if (shapes.empty()) throw std::runtime_error("nothing to fuse");
  if (shapes.size() == 1) return shapes[0];
  BRepAlgoAPI_Fuse op;
  TopTools_ListOfShape args, tools;
  args.Append(shapes[0]);
  for (size_t i = 1; i < shapes.size(); ++i) tools.Append(shapes[i]);
  op.SetArguments(args);
  op.SetTools(tools);
  op.SetFuzzyValue(fuzzy);
  op.SetNonDestructive(Standard_True);
  op.Build();
  if (!op.IsDone() || op.HasErrors()) throw std::runtime_error("unfold: could not join the flat faces");
  ShapeUpgrade_UnifySameDomain u(op.Shape(), Standard_True, Standard_True, Standard_True);
  u.Build();
  return u.Shape();
}

} // namespace

namespace {

// The unfolding plan: where every face of the stationary skin goes in the flat.
struct Unfolding {
  TopTools_IndexedMapOfShape faces;
  std::map<int, Place> placed;
  gp_Pln pln;
  gp_Vec n0;
};

Unfolding plan_unfold(const TopoDS_Shape &s, uint32_t face, double thickness, double k_factor) {
  if (!(thickness > 0)) throw std::runtime_error("unfold: thickness must be positive");
  Unfolding u;
  TopTools_IndexedMapOfShape &faces = u.faces;
  index_map(s, TopAbs_FACE, faces);
  if (face < 1 || static_cast<int>(face) > faces.Extent()) throw std::runtime_error("unfold: no such face");
  TopTools_IndexedDataMapOfShapeListOfShape edge_faces;
  TopExp::MapShapesAndAncestors(s, TopAbs_EDGE, TopAbs_FACE, edge_faces);

  const TopoDS_Face &F0 = TopoDS::Face(faces(static_cast<int>(face)));
  if (BRepAdaptor_Surface(F0).GetType() != GeomAbs_Plane) throw std::runtime_error("the stationary face must be planar");
  gp_Pnt c0 = face_inner_point(F0);
  u.n0 = face_normal(F0, c0).Normalized();
  u.pln = gp_Pln(c0, gp_Dir(u.n0));

  std::map<int, Place> &placed = u.placed;
  placed[static_cast<int>(face)] = Place{};
  std::deque<int> queue{static_cast<int>(face)};
  while (!queue.empty()) {
    int fi = queue.front();
    queue.pop_front();
    const TopoDS_Face &F = TopoDS::Face(faces(fi));
    const Place pf = placed[fi];
    for (TopExp_Explorer ex(F, TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge &E = TopoDS::Edge(ex.Current());
      if (BRep_Tool::Degenerated(E) || !edge_faces.Contains(E)) continue;
      for (TopTools_ListIteratorOfListOfShape it(edge_faces.FindFromKey(E)); it.More(); it.Next()) {
        int gi = faces.FindIndex(it.Value());
        if (gi <= 0 || gi == fi || placed.count(gi)) continue;
        const TopoDS_Face &G = TopoDS::Face(faces(gi));
        gp_Pnt M = edge_mid(E);
        if (face_normal(F, M).Normalized().Dot(face_normal(G, M).Normalized()) < 0.999) continue;
        BRepAdaptor_Surface gs(G, Standard_True);
        GeomAbs_SurfaceType gt = gs.GetType();
        Place pg;
        if (gt == GeomAbs_Plane && !pf.cyl) {
          pg.T = pf.T; // coplanar neighbour
        } else if (gt == GeomAbs_Cylinder && !pf.cyl) {
          gp_Cylinder cy = gs.Cylinder();
          BRepAdaptor_Curve ec(E);
          if (ec.GetType() != GeomAbs_Line || !ec.Line().Direction().IsParallel(cy.Axis().Direction(), 1e-6)) continue;
          pg.cyl = true;
          pg.a = gp_Vec(cy.Axis().Direction());
          pg.O = cy.Location();
          pg.R = cy.Radius();
          gp_Vec q(pg.O, M);
          q -= pg.a * q.Dot(pg.a);
          if (q.Magnitude() < 1e-9) continue;
          pg.r0 = q.Normalized();
          pg.b = pg.a.Crossed(pg.r0);
          gp_Vec qi(pg.O, face_inner_point(G));
          if (std::atan2(qi.Dot(pg.b), qi.Dot(pg.r0)) < 0) pg.b.Reverse();
          // the inside of the bend: the material lies radially outward of it
          gp_Pnt on = pg.O.Translated(pg.r0 * pg.R + pg.a * gp_Vec(pg.O, M).Dot(pg.a));
          pg.inner = face_normal(G, on).Dot(pg.r0) < 0;
          double r_in = pg.inner ? pg.R : pg.R - thickness;
          if (r_in < -1e-9) throw std::runtime_error("unfold: a bend is tighter than the sheet thickness");
          pg.Rn = std::max(r_in, 0.0) + k_factor * thickness;
          pg.P0 = pg.O.Translated(pg.r0 * pg.R).Transformed(pf.T);
          pg.A = pg.a.Transformed(pf.T);
          pg.V = pg.b.Transformed(pf.T);
        } else if (gt == GeomAbs_Plane && pf.cyl) {
          BRepAdaptor_Curve ec(E);
          if (ec.GetType() != GeomAbs_Line) continue;
          double phi = bend_angle(pf, M);
          gp_Vec r = pf.r0 * std::cos(phi) + pf.b * std::sin(phi);
          gp_Vec w = pf.r0 * -std::sin(phi) + pf.b * std::cos(phi);
          gp_Pnt o3 = pf.O.Translated(r * pf.R);
          gp_Pnt o2 = pf.P0.Translated(pf.V * (phi * pf.Rn));
          gp_Ax3 from(o3, gp_Dir(pf.a.Crossed(w)), gp_Dir(pf.a));
          gp_Ax3 to(o2, gp_Dir(pf.A.Crossed(pf.V)), gp_Dir(pf.A));
          pg.T.SetDisplacement(from, to);
        } else if (gt == GeomAbs_Cylinder && pf.cyl) {
          gp_Cylinder cy = gs.Cylinder();
          if (std::abs(cy.Radius() - pf.R) > 1e-6 || !cy.Axis().Direction().IsParallel(gp_Dir(pf.a), 1e-6) ||
              gp_Lin(cy.Axis()).Distance(pf.O) > 1e-6) {
            throw std::runtime_error("unfold: bends that run straight into other bends are not supported");
          }
          pg = pf; // the same bend split into several faces
          pg.repeat = true;
        } else {
          throw std::runtime_error("unfold: the sheet has a curved face that is not a simple bend");
        }
        placed[gi] = pg;
        queue.push_back(gi);
      }
    }
  }

  return u;
}

} // namespace

// face: 1-based stationary planar face. bends: 8 doubles per bend, appended: centre line start(3)
// end(3) in the flat, the bend angle (rad) and 1 if the bend's inside is on the stationary skin.
std::unique_ptr<Shape> unfold(const Shape &s, uint32_t face, double thickness, double k_factor,
                              rust::Vec<double> &bends) {
  CLF_TRY
  Unfolding u = plan_unfold(s.s, face, thickness, k_factor);
  const TopTools_IndexedMapOfShape &faces = u.faces;
  const std::map<int, Place> &placed = u.placed;
  const gp_Pln &pln = u.pln;
  const gp_Vec &n0 = u.n0;
  std::vector<TopoDS_Shape> flat;
  for (const auto &[fi, pl] : placed) {
    const TopoDS_Face &F = TopoDS::Face(faces(fi));
    if (pl.cyl) {
      flat.push_back(flat_bend_face(F, pl, pln));
      // bend centre line and angle from the face's vertices
      double smin = 1e300, smax = -1e300, th = 0;
      TopTools_IndexedMapOfShape verts;
      TopExp::MapShapes(F, TopAbs_VERTEX, verts);
      for (int i = 1; i <= verts.Extent(); ++i) {
        gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(verts(i)));
        double sv = gp_Vec(pl.O, p).Dot(pl.a);
        smin = std::min(smin, sv);
        smax = std::max(smax, sv);
        th = std::max(th, bend_angle(pl, p));
      }
      if (pl.repeat) continue;
      gp_Pnt p0 = pl.P0.Translated(pl.A * smin + pl.V * (th / 2 * pl.Rn));
      gp_Pnt p1 = pl.P0.Translated(pl.A * smax + pl.V * (th / 2 * pl.Rn));
      for (double v : {p0.X(), p0.Y(), p0.Z(), p1.X(), p1.Y(), p1.Z(), th, pl.inner ? 1.0 : 0.0}) bends.push_back(v);
    } else {
      BRepBuilderAPI_Transform tr(F, pl.T, Standard_True);
      if (!tr.IsDone()) throw std::runtime_error("unfold: could not move a face");
      flat.push_back(tr.Shape());
    }
  }
  TopoDS_Shape skin = fuse_all(flat, 1e-5);
  std::vector<TopoDS_Shape> solids;
  gp_Vec down = n0.Normalized() * -thickness;
  for (TopExp_Explorer fx(skin, TopAbs_FACE); fx.More(); fx.Next()) {
    BRepPrimAPI_MakePrism mk(fx.Current(), down, Standard_True);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("unfold: could not thicken the flat pattern");
    solids.push_back(mk.Shape());
  }
  return wrap(checked(fuse_all(solids, 1e-5), "flat pattern"));
  CLF_CATCH("unfold")
}

namespace {

// A piece of a flat-pattern cut back on the folded part: the solid it removes there.
// Planar regions: the piece moved back and pushed through the thickness. Bends: the piece mapped
// onto the bend's cylinder (the flat → (angle, axial) map is affine) and thickened radially.
TopoDS_Shape refold_piece(const TopoDS_Face &piece, const Place &pl, double t, const gp_Vec &n0) {
  if (!pl.cyl) {
    gp_Trsf inv = pl.T.Inverted();
    BRepBuilderAPI_Transform tr(piece, inv, Standard_True);
    gp_Vec n = n0.Transformed(inv);
    BRepPrimAPI_MakePrism mk(tr.Shape(), n * -t, Standard_True);
    mk.Build();
    if (!mk.IsDone()) throw std::runtime_error("refold: could not rebuild a cut");
    return mk.Shape();
  }
  if (pl.Rn < 1e-9) throw std::runtime_error("refold: a bend has no length in the flat");
  // u runs from r0 towards a × r0 on a direct cylinder; our angle runs towards b
  double sense = pl.b.Dot(pl.a.Crossed(pl.r0)) > 0 ? 1.0 : -1.0;
  // pad the cut radially so it clears both skins
  double pad = 0.05 * t;
  double rb = pl.inner ? pl.R + t : pl.R - t;
  double r_start = pl.inner ? pl.R - pad : pl.R + pad;
  double depth = (pl.inner ? 1.0 : -1.0) * (t + 2 * pad);
  if (r_start <= 1e-9 || rb < -1e-9) throw std::runtime_error("refold: bend too tight");
  Handle(Geom_CylindricalSurface) cyl =
      new Geom_CylindricalSurface(gp_Ax3(pl.O, gp_Dir(pl.a), gp_Dir(pl.r0)), r_start);
  auto uv = [&](const gp_Pnt &q) {
    gp_Vec d(pl.P0, q);
    return gp_Pnt2d(sense * d.Dot(pl.V) / pl.Rn, d.Dot(pl.A));
  };
  TopoDS_Wire outer = BRepTools::OuterWire(piece);
  std::vector<TopoDS_Wire> wires;
  for (TopExp_Explorer wx(piece, TopAbs_WIRE); wx.More(); wx.Next()) {
    const TopoDS_Wire &w = TopoDS::Wire(wx.Current());
    BRepBuilderAPI_MakeWire mw;
    for (BRepTools_WireExplorer we(w, piece); we.More(); we.Next()) {
      const TopoDS_Edge &e = we.Current();
      if (BRep_Tool::Degenerated(e)) continue;
      BRepAdaptor_Curve c(e);
      TopoDS_Vertex v1, v2;
      TopExp::Vertices(e, v1, v2, Standard_True);
      gp_Pnt2d a2 = uv(BRep_Tool::Pnt(v1)), b2 = uv(BRep_Tool::Pnt(v2));
      Handle(Geom2d_Curve) c2;
      if (c.GetType() == GeomAbs_Line) {
        if (a2.Distance(b2) < 1e-9) continue;
        c2 = GCE2d_MakeSegment(a2, b2).Value();
      } else {
        const int n = 24;
        Handle(TColgp_HArray1OfPnt2d) pts = new TColgp_HArray1OfPnt2d(1, n + 1);
        double f = c.FirstParameter(), l = c.LastParameter();
        bool rev = e.Orientation() == TopAbs_REVERSED;
        for (int i = 0; i <= n; ++i) {
          double tt = rev ? l - (l - f) * i / n : f + (l - f) * i / n;
          pts->SetValue(i + 1, uv(c.Value(tt)));
        }
        pts->SetValue(1, a2);
        pts->SetValue(n + 1, b2);
        Geom2dAPI_Interpolate interp(pts, Standard_False, 1e-9);
        interp.Perform();
        if (!interp.IsDone()) throw std::runtime_error("refold: could not map a curve onto a bend");
        c2 = interp.Curve();
      }
      BRepBuilderAPI_MakeEdge me(c2, cyl);
      if (!me.IsDone()) throw std::runtime_error("refold: could not map an edge onto a bend");
      TopoDS_Edge oe = me.Edge();
      BRepLib::BuildCurves3d(oe);
      mw.Add(oe);
      if (!mw.IsDone()) throw std::runtime_error("refold: a cut outline did not close on a bend");
    }
    if (!mw.IsDone()) continue;
    if (w.IsSame(outer)) wires.insert(wires.begin(), mw.Wire());
    else wires.push_back(mw.Wire());
  }
  if (wires.empty()) throw std::runtime_error("refold: empty cut on a bend");
  BRepBuilderAPI_MakeFace mf(cyl, wires[0], Standard_True);
  for (size_t i = 1; i < wires.size(); ++i) mf.Add(wires[i]);
  if (!mf.IsDone()) throw std::runtime_error("refold: could not make a cut face on a bend");
  ShapeFix_Face fix(mf.Face());
  fix.Perform();
  BRepOffsetAPI_MakeThickSolid thick;
  thick.MakeThickSolidBySimple(fix.Face(), depth);
  thick.Build();
  if (!thick.IsDone()) throw std::runtime_error("refold: could not thicken a cut on a bend");
  TopoDS_Shape out = thick.Shape();
  // the face's orientation decides the solid's; turn it outside-out
  for (TopExp_Explorer sx(out, TopAbs_SOLID); sx.More(); sx.Next()) {
    TopoDS_Solid so = TopoDS::Solid(sx.Current());
    BRepLib::OrientClosedSolid(so);
    return so;
  }
  throw std::runtime_error("refold: thickening a cut on a bend made no solid");
}

} // namespace

// The material to remove from the folded part for cuts made in its flat pattern. `removed`
// holds the solids cut out of the flat (through the thickness).
std::unique_ptr<Shape> refold_cuts(const Shape &folded, uint32_t face, double thickness, double k_factor,
                                   const Shape &removed) {
  CLF_TRY
  Unfolding u = plan_unfold(folded.s, face, thickness, k_factor);
  std::vector<std::pair<TopoDS_Shape, const Place *>> regions;
  for (const auto &[fi, pl] : u.placed) {
    const TopoDS_Face &F = TopoDS::Face(u.faces(fi));
    if (pl.cyl) {
      regions.emplace_back(flat_bend_face(F, pl, u.pln), &pl);
    } else {
      BRepBuilderAPI_Transform tr(F, pl.T, Standard_True);
      regions.emplace_back(tr.Shape(), &pl);
    }
  }
  TopoDS_Compound out;
  BRep_Builder bld;
  bld.MakeCompound(out);
  int n = 0;
  for (TopExp_Explorer fx(removed.s, TopAbs_FACE); fx.More(); fx.Next()) {
    const TopoDS_Face &rf = TopoDS::Face(fx.Current());
    BRepAdaptor_Surface rs(rf);
    if (rs.GetType() != GeomAbs_Plane) continue;
    if (!rs.Plane().Axis().Direction().IsParallel(u.pln.Axis().Direction(), 1e-6) ||
        u.pln.Distance(rs.Plane().Location()) > 1e-5) {
      continue;
    }
    for (const auto &[region, pl] : regions) {
      BRepAlgoAPI_Common common(rf, region);
      common.SetFuzzyValue(1e-6);
      common.Build();
      if (!common.IsDone()) continue;
      for (TopExp_Explorer px(common.Shape(), TopAbs_FACE); px.More(); px.Next()) {
        bld.Add(out, refold_piece(TopoDS::Face(px.Current()), *pl, thickness, u.n0));
        ++n;
      }
    }
  }
  if (n == 0) throw std::runtime_error("refold: no cuts reach the flat pattern's face");
  return wrap(out);
  CLF_CATCH("refold")
}

// Like fillet, with a radius per edge.
std::unique_ptr<Shape> fillet_radii(const Shape &s, rust::Slice<const uint32_t> edges,
                                    rust::Slice<const double> radii) {
  CLF_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_EDGE, map);
  BRepFilletAPI_MakeFillet mk(s.s);
  int added = 0;
  for (size_t k = 0; k < edges.size() && k < radii.size(); ++k) {
    uint32_t i = edges[k];
    if (i >= 1 && static_cast<int>(i) <= map.Extent() && radii[k] > 0) {
      mk.Add(radii[k], TopoDS::Edge(map(static_cast<int>(i))));
      ++added;
    }
  }
  if (added == 0) throw std::runtime_error("no edges to fillet");
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the fillet could not be computed (radius too large?)");
  return wrap(checked(mk.Shape(), "fillet"));
  CLF_CATCH("fillet")
}

// Hidden-line projection. view: direction towards the viewer(3), screen right(3).
HlrData hlr_project(const Shape &s, rust::Slice<const double> view, double deflection) {
  CLF_TRY
  need(view, 6, "view");
  HlrData out;
  Handle(HLRBRep_Algo) algo = new HLRBRep_Algo();
  algo->Add(s.s);
  gp_Ax2 ax(gp::Origin(), dir(view, 0), dir(view, 3));
  algo->Projector(HLRAlgo_Projector(ax));
  algo->Update();
  algo->Hide();
  HLRBRep_HLRToShape h(algo);
  out.offsets.push_back(0);
  auto add = [&](const TopoDS_Shape &sh, uint8_t kind) {
    if (sh.IsNull()) return;
    for (TopExp_Explorer ex(sh, TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge &e = TopoDS::Edge(ex.Current());
      try {
        BRepAdaptor_Curve c(e);
        if (c.GetType() == GeomAbs_Circle && std::abs(c.LastParameter() - c.FirstParameter() - 2 * M_PI) < 1e-6) {
          gp_Circ ci = c.Circle();
          for (double v : {ci.Location().X(), ci.Location().Y(), ci.Radius(), static_cast<double>(kind)})
            out.circles.push_back(v);
        }
        GCPnts_TangentialDeflection disc(c, 0.1, deflection);
        if (disc.NbPoints() < 2) continue;
        for (int i = 1; i <= disc.NbPoints(); ++i) {
          gp_Pnt p = disc.Value(i);
          out.points.push_back(p.X());
          out.points.push_back(p.Y());
        }
        out.offsets.push_back(static_cast<uint32_t>(out.points.size() / 2));
        out.kinds.push_back(kind);
      } catch (...) {
        continue;
      }
    }
  };
  add(h.VCompound(), 0);
  add(h.Rg1LineVCompound(), 1);
  add(h.OutLineVCompound(), 2);
  add(h.HCompound(), 3);
  add(h.OutLineHCompound(), 4);
  return out;
  CLF_CATCH("projection")
}

namespace {

// Turns a closed shell into an outward-oriented solid; leaves open shells alone.
TopoDS_Shape shell_to_solid(const TopoDS_Shell &sh) {
  if (!BRep_Tool::IsClosed(sh)) return sh;
  BRepBuilderAPI_MakeSolid ms(sh);
  if (!ms.IsDone()) return sh;
  TopoDS_Solid solid = ms.Solid();
  BRepLib::OrientClosedSolid(solid);
  ShapeFix_Solid fix(solid);
  fix.Perform();
  return fix.Solid().IsNull() ? TopoDS_Shape(solid) : fix.Solid();
}

// Sews loose faces/shells (IGES, surface STEP) into shells and closes what can be closed.
TopoDS_Shape sew_to_solids(const TopoDS_Shape &s, double tol) {
  BRepBuilderAPI_Sewing sew(tol);
  sew.Add(s);
  sew.Perform();
  TopoDS_Shape sewn = sew.SewedShape();
  if (sewn.IsNull()) return s;
  TopoDS_Compound out;
  BRep_Builder b;
  b.MakeCompound(out);
  int n = 0;
  for (TopExp_Explorer ex(sewn, TopAbs_SHELL); ex.More(); ex.Next(), ++n) {
    b.Add(out, shell_to_solid(TopoDS::Shell(ex.Current())));
  }
  // faces that didn't join any shell
  for (TopExp_Explorer ex(sewn, TopAbs_FACE, TopAbs_SHELL); ex.More(); ex.Next(), ++n) {
    b.Add(out, ex.Current());
  }
  return n == 0 ? sewn : TopoDS_Shape(out);
}

} // namespace

std::unique_ptr<Shape> solidify(const Shape &s, double tol) {
  CLF_TRY
  if (s.s.IsNull()) throw std::runtime_error("empty shape");
  return wrap(sew_to_solids(s.s, tol));
  CLF_CATCH("sewing")
}

void write_iges(const Shape &s, rust::Str path) {
  CLF_TRY
  quiet();
  IGESControl_Controller::Init();
  // mode 1: B-rep entities (manifold solids), which modern CAD reads as solids
  IGESControl_Writer w("MM", 1);
  if (!w.AddShape(s.s)) throw std::runtime_error("IGES transfer failed");
  w.ComputeModel();
  std::string p(path.data(), path.size());
  if (!w.Write(p.c_str())) throw std::runtime_error("could not write " + p);
  CLF_CATCH("IGES export")
}

std::unique_ptr<Shape> read_iges(rust::Str path) {
  CLF_TRY
  quiet();
  IGESControl_Controller::Init();
  IGESControl_Reader r;
  std::string p(path.data(), path.size());
  if (r.ReadFile(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("could not read " + p);
  r.TransferRoots();
  return wrap(r.OneShape());
  CLF_CATCH("IGES import")
}

std::unique_ptr<Shape> mesh_to_shape(rust::Slice<const double> pos, rust::Slice<const uint32_t> tris, bool merge) {
  CLF_TRY
  const size_t nv = pos.size() / 3;
  if (tris.size() % 3 != 0 || tris.empty()) throw std::runtime_error("no triangles");
  std::vector<TopoDS_Vertex> verts(nv);
  auto vertex = [&](uint32_t i) -> const TopoDS_Vertex & {
    if (i >= nv) throw std::runtime_error("vertex index out of range");
    if (verts[i].IsNull()) verts[i] = BRepBuilderAPI_MakeVertex(pnt(pos, size_t(i) * 3)).Vertex();
    return verts[i];
  };
  // one edge per undirected vertex pair, shared by the triangles on either side
  std::map<std::pair<uint32_t, uint32_t>, std::pair<TopoDS_Edge, int>> edges;
  BRep_Builder b;
  TopoDS_Shell shell;
  b.MakeShell(shell);
  size_t faces = 0;
  for (size_t t = 0; t + 2 < tris.size(); t += 3) {
    uint32_t v[3] = {tris[t], tris[t + 1], tris[t + 2]};
    if (v[0] == v[1] || v[1] == v[2] || v[0] == v[2]) continue;
    gp_Pnt a = pnt(pos, size_t(v[0]) * 3), p1 = pnt(pos, size_t(v[1]) * 3), p2 = pnt(pos, size_t(v[2]) * 3);
    gp_Vec n = gp_Vec(a, p1).Crossed(gp_Vec(a, p2));
    if (n.Magnitude() < 1e-14) continue; // sliver
    TopoDS_Wire w;
    b.MakeWire(w);
    for (int k = 0; k < 3; ++k) {
      uint32_t i = v[k], j = v[(k + 1) % 3];
      auto key = std::make_pair(std::min(i, j), std::max(i, j));
      auto it = edges.find(key);
      if (it == edges.end()) {
        TopoDS_Edge e = BRepBuilderAPI_MakeEdge(vertex(key.first), vertex(key.second)).Edge();
        it = edges.emplace(key, std::make_pair(e, 0)).first;
      }
      it->second.second++;
      TopoDS_Edge e = it->second.first;
      b.Add(w, i < j ? e : TopoDS::Edge(e.Reversed()));
    }
    w.Closed(true);
    BRepBuilderAPI_MakeFace mf(gp_Pln(a, gp_Dir(n)), w, true);
    if (!mf.IsDone()) continue;
    b.Add(shell, mf.Face());
    ++faces;
  }
  if (faces == 0) throw std::runtime_error("the mesh has no usable triangles");
  bool closed = true;
  for (auto &e : edges) closed = closed && e.second.second == 2;
  shell.Closed(closed);
  TopoDS_Shape result = closed ? shell_to_solid(shell) : TopoDS_Shape(shell);
  if (!closed && faces <= 50000) {
    // small gaps or duplicated seams: sewing may still close it
    result = sew_to_solids(shell, 1e-4);
  }
  if (merge && faces <= 20000) {
    // CAD-made meshes: merge the coplanar triangles back into proper planar faces
    ShapeUpgrade_UnifySameDomain u(result, true, true, false);
    u.Build();
    if (!u.Shape().IsNull()) result = u.Shape();
  }
  return wrap(result);
  CLF_CATCH("mesh conversion")
}

} // namespace clf
