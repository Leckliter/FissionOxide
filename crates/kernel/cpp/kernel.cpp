// C++ side of the FissionOxide kernel bridge. See kernel.hxx.
#include "fox-kernel/cpp/kernel.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <cmath>
#include <deque>
#include <map>
#include <sstream>
#include <vector>
#include <stdexcept>
#include <string>

#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFill.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepBuilderAPI_GTransform.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepFeat_SplitShape.hxx>
#include <BRepFill_Filling.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <Geom2d_Line.hxx>
#include <gp_GTrsf.hxx>
#include <gp_Lin2d.hxx>
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
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <LocOpe_DPrism.hxx>
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
#include <Geom_BezierCurve.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <TColgp_Array1OfPnt.hxx>
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

namespace fox {

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

#define FOX_TRY                                                                                                        \
  try {                                                                                                                \
    catch_signals();                                                                                                   \
    OCC_CATCH_SIGNALS
#define FOX_CATCH(what)                                                                                                \
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
// Edge `seg` of a segment list (13 doubles each, see make_face).
TopoDS_Edge seg_edge(rust::Slice<const double> segs, size_t seg) {
  size_t o = seg * 13;
  need(segs, o + 13, "segments");
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
  if (kind == 3) {
    TColgp_Array1OfPnt poles(1, 4);
    for (int k = 0; k < 4; ++k) poles.SetValue(k + 1, pnt(segs, o + 1 + 3 * k));
    Handle(Geom_BezierCurve) bz = new Geom_BezierCurve(poles);
    BRepBuilderAPI_MakeEdge me(bz);
    if (!me.IsDone()) throw std::runtime_error("degenerate spline in profile");
    return me.Edge();
  }
  gp_Circ c(gp_Ax2(pnt(segs, o + 1), dir(segs, o + 4)), segs[o + 7]);
  BRepBuilderAPI_MakeEdge me(c);
  if (!me.IsDone()) throw std::runtime_error("degenerate circle in profile");
  return me.Edge();
}
} // namespace

// plane: origin(3) normal(3) xdir(3)
// segs: 13 doubles each: kind (0 line, 1 arc through 3 points, 2 circle, 3 cubic Bezier), p0..p3
//   line: p0 → p1;  arc: start p0, interior p1, end p2;  circle: centre p0, normal p1, radius p2.x
// loop_lens: number of segments per loop; the first loop is the outer boundary, the rest holes.
std::unique_ptr<Shape> make_face(rust::Slice<const double> plane, rust::Slice<const double> segs,
                                 rust::Slice<const uint32_t> loop_lens) {
  FOX_TRY
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
  FOX_CATCH("profile")
}

std::unique_ptr<Shape> make_prism(const Shape &face, double dx, double dy, double dz) {
  FOX_TRY
  BRepPrimAPI_MakePrism mk(face.s, gp_Vec(dx, dy, dz), Standard_True);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("extrusion failed");
  return wrap(checked(mk.Shape(), "extrusion"));
  FOX_CATCH("extrude")
}

// axis: origin(3) dir(3)
std::unique_ptr<Shape> make_revol(const Shape &face, rust::Slice<const double> axis, double angle) {
  FOX_TRY
  need(axis, 6, "revolve axis");
  BRepPrimAPI_MakeRevol mk(face.s, gp_Ax1(pnt(axis, 0), dir(axis, 3)), angle, Standard_True);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("revolve failed");
  return wrap(checked(mk.Shape(), "revolve"));
  FOX_CATCH("revolve")
}

std::unique_ptr<Shape> make_box(double x0, double y0, double z0, double x1, double y1, double z1) {
  FOX_TRY
  BRepPrimAPI_MakeBox mk(gp_Pnt(x0, y0, z0), gp_Pnt(x1, y1, z1));
  return wrap(mk.Shape());
  FOX_CATCH("box")
}

std::unique_ptr<Shape> make_cylinder(rust::Slice<const double> axis, double radius, double height) {
  FOX_TRY
  need(axis, 6, "cylinder axis");
  BRepPrimAPI_MakeCylinder mk(gp_Ax2(pnt(axis, 0), dir(axis, 3)), radius, height);
  return wrap(mk.Shape());
  FOX_CATCH("cylinder")
}

std::unique_ptr<Shape> make_sphere(double cx, double cy, double cz, double radius) {
  FOX_TRY
  BRepPrimAPI_MakeSphere mk(gp_Pnt(cx, cy, cz), radius);
  return wrap(mk.Shape());
  FOX_CATCH("sphere")
}

std::unique_ptr<Shape> make_torus(rust::Slice<const double> axis, double major, double minor) {
  FOX_TRY
  need(axis, 6, "torus axis");
  BRepPrimAPI_MakeTorus mk(gp_Ax2(pnt(axis, 0), dir(axis, 3)), major, minor);
  return wrap(mk.Shape());
  FOX_CATCH("torus")
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
  FOX_TRY
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
  FOX_CATCH("boolean")
}

std::unique_ptr<Shape> fillet(const Shape &s, rust::Slice<const uint32_t> edges, double radius) {
  FOX_TRY
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
  FOX_CATCH("fillet")
}

std::unique_ptr<Shape> chamfer(const Shape &s, rust::Slice<const uint32_t> edges, double dist) {
  FOX_TRY
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
  FOX_CATCH("chamfer")
}

// Hollows a solid, removing `faces` (1-based); negative thickness keeps the outside surface.
// Walls meet in sharp corners, or in rounded ones when `round` is set.
std::unique_ptr<Shape> shell(const Shape &s, rust::Slice<const uint32_t> faces, double thickness, bool round) {
  FOX_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_FACE, map);
  TopTools_ListOfShape remove;
  for (uint32_t i : faces) {
    if (i >= 1 && static_cast<int>(i) <= map.Extent()) remove.Append(map(static_cast<int>(i)));
  }
  BRepOffsetAPI_MakeThickSolid mk;
  mk.MakeThickSolidByJoin(s.s, remove, thickness, 1e-3, BRepOffset_Skin, Standard_False, Standard_False,
                          round ? GeomAbs_Arc : GeomAbs_Intersection);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("shell failed (thickness too large for the geometry?)");
  return wrap(checked(mk.Shape(), "shell"));
  FOX_CATCH("shell")
}

// m: 3x4 row-major affine matrix
std::unique_ptr<Shape> transform(const Shape &s, rust::Slice<const double> m) {
  FOX_TRY
  need(m, 12, "transform matrix");
  gp_Trsf t;
  t.SetValues(m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8], m[9], m[10], m[11]);
  BRepBuilderAPI_Transform mk(s.s, t, Standard_True);
  if (!mk.IsDone()) throw std::runtime_error("transform failed");
  return wrap(mk.Shape());
  FOX_CATCH("transform")
}

// plane: origin(3) normal(3)
std::unique_ptr<Shape> mirror(const Shape &s, rust::Slice<const double> plane) {
  FOX_TRY
  need(plane, 6, "mirror plane");
  gp_Trsf t;
  t.SetMirror(gp_Ax2(pnt(plane, 0), dir(plane, 3)));
  BRepBuilderAPI_Transform mk(s.s, t, Standard_True);
  if (!mk.IsDone()) throw std::runtime_error("mirror failed");
  return wrap(mk.Shape());
  FOX_CATCH("mirror")
}

std::unique_ptr<Shape> unify(const Shape &s) {
  FOX_TRY
  ShapeUpgrade_UnifySameDomain u(s.s, Standard_True, Standard_True, Standard_False);
  u.Build();
  return wrap(u.Shape());
  FOX_CATCH("unify")
}

uint32_t solid_count(const Shape &s) {
  if (s.s.IsNull()) return 0;
  uint32_t n = 0;
  for (TopExp_Explorer ex(s.s, TopAbs_SOLID); ex.More(); ex.Next()) ++n;
  return n;
}

std::unique_ptr<Shape> solid_at(const Shape &s, uint32_t i) {
  FOX_TRY
  uint32_t n = 0;
  for (TopExp_Explorer ex(s.s, TopAbs_SOLID); ex.More(); ex.Next(), ++n) {
    if (n == i) return wrap(ex.Current());
  }
  throw std::runtime_error("no such solid");
  FOX_CATCH("solid")
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
  FOX_TRY
  if (s.s.IsNull()) return;
  BRep_Builder b;
  b.Add(c.s, s.s);
  FOX_CATCH("compound")
}

MeshData mesh_shape(const Shape &s, double linear, double angular) {
  FOX_TRY
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
  FOX_CATCH("mesh")
}

EdgeData edge_polylines(const Shape &s, double deflection) {
  FOX_TRY
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
  FOX_CATCH("edges")
}

rust::Vec<FaceInfo> face_infos(const Shape &s) {
  FOX_TRY
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
  FOX_CATCH("faces")
}

rust::Vec<EdgeInfo> edge_infos(const Shape &s) {
  FOX_TRY
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
  FOX_CATCH("edge info")
}

MassProps mass_props(const Shape &s) {
  FOX_TRY
  MassProps out{};
  GProp_GProps vol;
  BRepGProp::VolumeProperties(s.s, vol);
  out.volume = vol.Mass();
  out.cog = arr(vol.CentreOfMass().XYZ());
  GProp_GProps surf;
  BRepGProp::SurfaceProperties(s.s, surf);
  out.area = surf.Mass();
  // the tight box: the default one is loose around curved (fillet, B-spline) faces
  Bnd_Box box;
  BRepBndLib::AddOptimal(s.s, box, Standard_False, Standard_False);
  if (!box.IsVoid()) {
    double x0, y0, z0, x1, y1, z1;
    box.Get(x0, y0, z0, x1, y1, z1);
    out.bbox_min = {x0, y0, z0};
    out.bbox_max = {x1, y1, z1};
  }
  return out;
  FOX_CATCH("mass properties")
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
  FOX_TRY
  quiet();
  STEPControl_Writer w;
  Interface_Static::SetCVal("write.step.unit", "MM");
  Interface_Static::SetCVal("write.step.schema", "AP214IS");
  if (w.Transfer(s.s, STEPControl_AsIs) != IFSelect_RetDone) throw std::runtime_error("STEP transfer failed");
  std::string p(path.data(), path.size());
  if (w.Write(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("could not write " + p);
  FOX_CATCH("STEP export")
}

std::unique_ptr<Shape> read_step(rust::Str path) {
  FOX_TRY
  quiet();
  STEPControl_Reader r;
  std::string p(path.data(), path.size());
  if (r.ReadFile(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("could not read " + p);
  r.TransferRoots();
  return wrap(r.OneShape());
  FOX_CATCH("STEP import")
}

rust::String write_brep(const Shape &s) {
  FOX_TRY
  std::ostringstream os;
  BRepTools::Write(s.s, os);
  return rust::String(os.str());
  FOX_CATCH("BRep write")
}

std::unique_ptr<Shape> read_brep(rust::Str data) {
  FOX_TRY
  std::istringstream is(std::string(data.data(), data.size()));
  TopoDS_Shape shape;
  BRep_Builder b;
  BRepTools::Read(shape, is, b);
  return wrap(shape);
  FOX_CATCH("BRep read")
}

std::unique_ptr<Shape> face_at_index(const Shape &s, uint32_t face) {
  FOX_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_FACE, map);
  if (face < 1 || static_cast<int>(face) > map.Extent()) throw std::runtime_error("no such face");
  return wrap(map(static_cast<int>(face)));
  FOX_CATCH("face")
}

// An open or closed chain of segments (see make_face) as a wire.
std::unique_ptr<Shape> make_wire(rust::Slice<const double> segs) {
  FOX_TRY
  BRepBuilderAPI_MakeWire mw;
  for (size_t i = 0; i * 13 < segs.size(); ++i) {
    mw.Add(seg_edge(segs, i));
    if (!mw.IsDone()) throw std::runtime_error("the path is not connected");
  }
  if (!mw.IsDone()) throw std::runtime_error("empty path");
  return wrap(mw.Wire());
  FOX_CATCH("path")
}

// Sweeps a planar face along a wire (corrected Frenet frame, as Fusion's default orientation).
namespace {
// Does a path turn sharply anywhere (a corner, not a tangent join)?
bool has_corners(const TopoDS_Wire &w) {
  gp_Vec prev;
  bool first = true;
  for (BRepTools_WireExplorer ex(w); ex.More(); ex.Next()) {
    BRepAdaptor_Curve c(ex.Current());
    bool rev = ex.Current().Orientation() == TopAbs_REVERSED;
    double t0 = rev ? c.LastParameter() : c.FirstParameter();
    double t1 = rev ? c.FirstParameter() : c.LastParameter();
    gp_Pnt p;
    gp_Vec d0, d1;
    c.D1(t0, p, d0);
    c.D1(t1, p, d1);
    if (rev) {
      d0.Reverse();
      d1.Reverse();
    }
    if (!first && d0.Magnitude() > 1e-12 && prev.Magnitude() > 1e-12 && d0.Angle(prev) > 1e-3) return true;
    prev = d1;
    first = false;
  }
  return false;
}

// Sweeps one closed profile wire along a path with sharp corners (mitred, as Fusion's pipes).
TopoDS_Shape corner_sweep(const TopoDS_Wire &spine, const TopoDS_Wire &profile) {
  BRepOffsetAPI_MakePipeShell ps(spine);
  ps.SetMode(Standard_False);
  ps.SetTransitionMode(BRepBuilderAPI_RightCorner);
  ps.Add(profile);
  ps.Build();
  if (!ps.IsDone()) throw std::runtime_error("the sweep failed at a corner of the path");
  ps.MakeSolid();
  return ps.Shape();
}
} // namespace

std::unique_ptr<Shape> make_pipe(const Shape &face, const Shape &spine) {
  FOX_TRY
  if (spine.s.ShapeType() != TopAbs_WIRE) throw std::runtime_error("the sweep path must be a wire");
  const TopoDS_Wire &path = TopoDS::Wire(spine.s);
  if (face.s.ShapeType() == TopAbs_FACE && has_corners(path)) {
    // a face sweep only follows smooth paths: sweep each loop with mitred corners and cut the
    // holes out of the outer solid
    const TopoDS_Face &f = TopoDS::Face(face.s);
    TopoDS_Wire outer = BRepTools::OuterWire(f);
    TopoDS_Shape out = corner_sweep(path, outer);
    for (TopExp_Explorer ex(f, TopAbs_WIRE); ex.More(); ex.Next()) {
      if (ex.Current().IsSame(outer)) continue;
      BRepAlgoAPI_Cut cut(out, corner_sweep(path, TopoDS::Wire(ex.Current())));
      if (!cut.IsDone()) throw std::runtime_error("the sweep's hole could not be cut");
      out = cut.Shape();
    }
    return wrap(checked(out, "sweep"));
  }
  BRepOffsetAPI_MakePipe mk(path, face.s, GeomFill_IsCorrectedFrenet, Standard_False);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the sweep failed");
  return wrap(checked(mk.Shape(), "sweep"));
  FOX_CATCH("sweep")
}

// Lofts through the outer wires of the faces (or the wires) of a compound, in order. The first
// and last sections may be vertices (a loft to a point).
std::unique_ptr<Shape> make_loft(const Shape &sections, bool ruled, bool closed) {
  FOX_TRY
  BRepOffsetAPI_ThruSections mk(Standard_True, ruled ? Standard_True : Standard_False, 1e-6);
  int n = 0, count = 0;
  for (TopoDS_Iterator it(sections.s); it.More(); it.Next()) ++count;
  TopoDS_Wire first;
  for (TopoDS_Iterator it(sections.s); it.More(); it.Next(), ++n) {
    const TopoDS_Shape &sh = it.Value();
    TopoDS_Wire w;
    if (sh.ShapeType() == TopAbs_VERTEX) {
      if (closed || (n != 0 && n != count - 1)) throw std::runtime_error("only the first or last loft section can be a point");
      mk.AddVertex(TopoDS::Vertex(sh));
      continue;
    }
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
  FOX_CATCH("loft")
}

// Splits a solid with an infinite plane (origin(3) normal(3)); the result holds every piece.
std::unique_ptr<Shape> split_by_plane(const Shape &s, rust::Slice<const double> plane, double size) {
  FOX_TRY
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
  FOX_CATCH("split")
}

// Tapers faces (1-based) by `angle` (rad) about the neutral plane (origin(3) normal(3)), pulling
// along the plane normal.
std::unique_ptr<Shape> draft(const Shape &s, rust::Slice<const uint32_t> faces, rust::Slice<const double> plane,
                             double angle) {
  FOX_TRY
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
  FOX_CATCH("draft")
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

// Whether a cylinder is the bend `p` (another face of it).
bool same_bend(const Place &p, const gp_Cylinder &cy) {
  return std::abs(cy.Radius() - p.R) < 1e-6 && cy.Axis().Direction().IsParallel(gp_Dir(p.a), 1e-6) &&
         gp_Lin(cy.Axis()).Distance(p.O) < 1e-6;
}

// The rigid 3D → flat map of the plane tangent to bend `p` along its line through X (where a face
// beyond the bend continues).
gp_Trsf bend_line_frame(const Place &p, const gp_Pnt &X) {
  double phi = bend_angle(p, X);
  gp_Vec r = p.r0 * std::cos(phi) + p.b * std::sin(phi);
  gp_Vec w = p.r0 * -std::sin(phi) + p.b * std::cos(phi);
  gp_Pnt o3 = p.O.Translated(r * p.R);
  gp_Pnt o2 = p.P0.Translated(p.V * (phi * p.Rn));
  gp_Ax3 from(o3, gp_Dir(p.a.Crossed(w)), gp_Dir(p.a));
  gp_Ax3 to(o2, gp_Dir(p.A.Crossed(p.V)), gp_Dir(p.A));
  gp_Trsf T;
  T.SetDisplacement(from, to);
  return T;
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
        } else if (gt == GeomAbs_Cylinder && (!pf.cyl || !same_bend(pf, gs.Cylinder()))) {
          gp_Cylinder cy = gs.Cylinder();
          BRepAdaptor_Curve ec(E);
          if (ec.GetType() != GeomAbs_Line || !ec.Line().Direction().IsParallel(cy.Axis().Direction(), 1e-6)) continue;
          // a bend running straight into another bend: unroll the first up to their shared line
          gp_Trsf T = pf.T;
          if (pf.cyl) T = bend_line_frame(pf, M);
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
          pg.P0 = pg.O.Translated(pg.r0 * pg.R).Transformed(T);
          pg.A = pg.a.Transformed(T);
          pg.V = pg.b.Transformed(T);
        } else if (gt == GeomAbs_Plane && pf.cyl) {
          BRepAdaptor_Curve ec(E);
          if (ec.GetType() != GeomAbs_Line) continue;
          pg.T = bend_line_frame(pf, M);
        } else if (gt == GeomAbs_Cylinder && pf.cyl) {
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
  FOX_TRY
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
  FOX_CATCH("unfold")
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
  FOX_TRY
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
  FOX_CATCH("refold")
}

// Like fillet, with a radius per edge.
std::unique_ptr<Shape> fillet_radii(const Shape &s, rust::Slice<const uint32_t> edges,
                                    rust::Slice<const double> radii) {
  FOX_TRY
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
  FOX_CATCH("fillet")
}

// Hidden-line projection. view: direction towards the viewer(3), screen right(3).
HlrData hlr_project(const Shape &s, rust::Slice<const double> view, double deflection) {
  FOX_TRY
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
  FOX_CATCH("projection")
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
  FOX_TRY
  if (s.s.IsNull()) throw std::runtime_error("empty shape");
  return wrap(sew_to_solids(s.s, tol));
  FOX_CATCH("sewing")
}

void write_iges(const Shape &s, rust::Str path) {
  FOX_TRY
  quiet();
  IGESControl_Controller::Init();
  // mode 1: B-rep entities (manifold solids), which modern CAD reads as solids
  IGESControl_Writer w("MM", 1);
  if (!w.AddShape(s.s)) throw std::runtime_error("IGES transfer failed");
  w.ComputeModel();
  std::string p(path.data(), path.size());
  if (!w.Write(p.c_str())) throw std::runtime_error("could not write " + p);
  FOX_CATCH("IGES export")
}

std::unique_ptr<Shape> read_iges(rust::Str path) {
  FOX_TRY
  quiet();
  IGESControl_Controller::Init();
  IGESControl_Reader r;
  std::string p(path.data(), path.size());
  if (r.ReadFile(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("could not read " + p);
  r.TransferRoots();
  return wrap(r.OneShape());
  FOX_CATCH("IGES import")
}

std::unique_ptr<Shape> mesh_to_shape(rust::Slice<const double> pos, rust::Slice<const uint32_t> tris, bool merge) {
  FOX_TRY
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
  FOX_CATCH("mesh conversion")
}

// A planar face extruded `height` along its normal with its sides drafted by `angle` (rad;
// positive tapers inward).
std::unique_ptr<Shape> draft_prism(const Shape &face, double height, double angle) {
  FOX_TRY
  if (face.s.ShapeType() != TopAbs_FACE) throw std::runtime_error("taper needs a face");
  LocOpe_DPrism dp(TopoDS::Face(face.s), height, angle);
  TopoDS_Shape r = dp.Shape();
  if (r.IsNull()) throw std::runtime_error("the tapered extrusion failed (taper too steep for the height?)");
  return wrap(checked(r, "tapered extrusion"));
  FOX_CATCH("tapered extrude")
}

// A planar face grown (d > 0) or shrunk (d < 0) by `d` all round, holes included.
std::unique_ptr<Shape> offset_face(const Shape &face, double d) {
  FOX_TRY
  if (face.s.ShapeType() != TopAbs_FACE) throw std::runtime_error("offset needs a face");
  const TopoDS_Face &f = TopoDS::Face(face.s);
  BRepOffsetAPI_MakeOffset mk(f, GeomAbs_Intersection);
  mk.Perform(d);
  if (!mk.IsDone()) throw std::runtime_error("the profile can't be offset that far");
  Handle(Geom_Surface) surf = BRep_Tool::Surface(f);
  std::vector<TopoDS_Wire> wires;
  for (TopExp_Explorer wx(mk.Shape(), TopAbs_WIRE); wx.More(); wx.Next()) wires.push_back(TopoDS::Wire(wx.Current()));
  if (wires.empty()) throw std::runtime_error("the offset profile vanished");
  // the outer wire is the one with the largest face
  size_t outer = 0;
  double best = -1;
  for (size_t i = 0; i < wires.size(); ++i) {
    BRepBuilderAPI_MakeFace t(surf, wires[i], Standard_True);
    if (!t.IsDone()) continue;
    GProp_GProps g;
    BRepGProp::SurfaceProperties(t.Face(), g);
    if (std::abs(g.Mass()) > best) {
      best = std::abs(g.Mass());
      outer = i;
    }
  }
  BRepBuilderAPI_MakeFace mf(surf, wires[outer], Standard_True);
  for (size_t i = 0; i < wires.size(); ++i) {
    if (i != outer) mf.Add(wires[i]);
  }
  if (!mf.IsDone()) throw std::runtime_error("could not make the offset profile");
  ShapeFix_Face fix(mf.Face());
  fix.FixOrientation();
  fix.Perform();
  return wrap(fix.Face());
  FOX_CATCH("offset profile")
}

// Chamfers edges (1-based) with two distances; `angle` mode: d2 is an angle (rad).
std::unique_ptr<Shape> chamfer2(const Shape &s, rust::Slice<const uint32_t> edges, double d1, double d2, bool angle) {
  FOX_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_EDGE, map);
  TopTools_IndexedDataMapOfShapeListOfShape ef;
  TopExp::MapShapesAndAncestors(s.s, TopAbs_EDGE, TopAbs_FACE, ef);
  BRepFilletAPI_MakeChamfer mk(s.s);
  int added = 0;
  for (uint32_t i : edges) {
    if (i < 1 || static_cast<int>(i) > map.Extent()) continue;
    const TopoDS_Edge &e = TopoDS::Edge(map(static_cast<int>(i)));
    if (!ef.Contains(e) || ef.FindFromKey(e).IsEmpty()) continue;
    const TopoDS_Face &f = TopoDS::Face(ef.FindFromKey(e).First());
    if (angle) mk.AddDA(d1, d2, e, f);
    else mk.Add(d1, d2, e, f);
    ++added;
  }
  if (added == 0) throw std::runtime_error("no edges to chamfer");
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the chamfer could not be computed (too large?)");
  return wrap(checked(mk.Shape(), "chamfer"));
  FOX_CATCH("chamfer")
}

// Fillets edges with a radius running from r1 at each edge's start to r2 at its end.
std::unique_ptr<Shape> fillet_var(const Shape &s, rust::Slice<const uint32_t> edges, double r1, double r2) {
  FOX_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_EDGE, map);
  BRepFilletAPI_MakeFillet mk(s.s);
  int added = 0;
  for (uint32_t i : edges) {
    if (i >= 1 && static_cast<int>(i) <= map.Extent()) {
      mk.Add(r1, r2, TopoDS::Edge(map(static_cast<int>(i))));
      ++added;
    }
  }
  if (added == 0) throw std::runtime_error("no edges to fillet");
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the fillet could not be computed (radius too large?)");
  return wrap(checked(mk.Shape(), "fillet"));
  FOX_CATCH("fillet")
}

// Splits a solid with another shape; the result holds every piece.
std::unique_ptr<Shape> split_by_shape(const Shape &s, const Shape &tool) {
  FOX_TRY
  BRepAlgoAPI_Splitter sp;
  TopTools_ListOfShape args, tools;
  args.Append(s.s);
  tools.Append(tool.s);
  sp.SetArguments(args);
  sp.SetTools(tools);
  sp.SetFuzzyValue(1e-6);
  sp.Build();
  if (!sp.IsDone() || sp.HasErrors()) throw std::runtime_error("the split failed");
  return wrap(sp.Shape());
  FOX_CATCH("split")
}


std::unique_ptr<Shape> edge_at_index(const Shape &s, uint32_t edge) {
  FOX_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_EDGE, map);
  if (edge < 1 || static_cast<int>(edge) > map.Extent()) throw std::runtime_error("no such edge");
  return wrap(map(static_cast<int>(edge)));
  FOX_CATCH("edge")
}

// Minimum distance between two shapes: [distance, point on a (3), point on b (3)].
rust::Vec<double> min_distance(const Shape &a, const Shape &b) {
  FOX_TRY
  BRepExtrema_DistShapeShape d(a.s, b.s);
  d.Perform();
  if (!d.IsDone() || d.NbSolution() < 1) throw std::runtime_error("the distance could not be computed");
  rust::Vec<double> out;
  out.push_back(d.Value());
  gp_Pnt p1 = d.PointOnShape1(1), p2 = d.PointOnShape2(1);
  for (double v : {p1.X(), p1.Y(), p1.Z(), p2.X(), p2.Y(), p2.Z()}) out.push_back(v);
  return out;
  FOX_CATCH("measure")
}

// Sweeps a profile (a face or wire in the plane through the axis and xdir, at the helix start
// origin + radius·xdir) along a helix. axis: origin(3) dir(3) xdir(3).
std::unique_ptr<Shape> helix_sweep(const Shape &profile, rust::Slice<const double> axis, double radius, double pitch,
                                   double height, bool left) {
  FOX_TRY
  need(axis, 9, "helix axis");
  if (radius <= 0 || pitch <= 0 || height <= 0) throw std::runtime_error("radius, pitch and height must be positive");
  if (height / pitch > 400) throw std::runtime_error("too many turns (at most 400)");
  gp_Ax3 ax(pnt(axis, 0), dir(axis, 3), dir(axis, 6));
  Handle(Geom_CylindricalSurface) cyl = new Geom_CylindricalSurface(ax, radius);
  gp_Dir2d d2(left ? -2.0 * M_PI : 2.0 * M_PI, pitch);
  gp_Lin2d line(gp_Pnt2d(0.0, 0.0), d2);
  double turns = height / pitch;
  double len = turns * std::sqrt(4.0 * M_PI * M_PI + pitch * pitch);
  Handle(Geom2d_TrimmedCurve) seg = GCE2d_MakeSegment(line, 0.0, len);
  TopoDS_Edge e = BRepBuilderAPI_MakeEdge(seg, cyl);
  BRepLib::BuildCurves3d(e, 1e-6, GeomAbs_C1, 14, 400);
  TopoDS_Wire spine = BRepBuilderAPI_MakeWire(e);
  TopoDS_Wire w;
  if (profile.s.ShapeType() == TopAbs_FACE) {
    w = BRepTools::OuterWire(TopoDS::Face(profile.s));
  } else if (profile.s.ShapeType() == TopAbs_WIRE) {
    w = TopoDS::Wire(profile.s);
  } else {
    throw std::runtime_error("the helix profile must be a face or a wire");
  }
  BRepOffsetAPI_MakePipeShell ps(spine);
  ps.SetMode(gp_Dir(axis[3], axis[4], axis[5]));
  ps.Add(w);
  ps.Build();
  if (!ps.IsDone()) throw std::runtime_error("the helical sweep failed");
  ps.MakeSolid();
  return wrap(checked(ps.Shape(), "helical sweep"));
  FOX_CATCH("helix")
}

// A face or shell thickened into a solid (d > 0 along the face normals).
std::unique_ptr<Shape> thicken(const Shape &s, double d) {
  FOX_TRY
  if (std::abs(d) < 1e-9) throw std::runtime_error("the thickness must not be zero");
  try {
    BRepOffsetAPI_MakeThickSolid mk;
    mk.MakeThickSolidBySimple(s.s, d);
    mk.Build();
    if (mk.IsDone()) return wrap(checked(mk.Shape(), "thicken"));
  } catch (...) {
  }
  BRepOffset_MakeOffset mo;
  mo.Initialize(s.s, d, 1e-6, BRepOffset_Skin, Standard_False, Standard_False, GeomAbs_Intersection, Standard_True);
  mo.MakeOffsetShape();
  if (!mo.IsDone()) throw std::runtime_error("the faces could not be thickened");
  return wrap(checked(mo.Shape(), "thicken"));
  FOX_CATCH("thicken")
}

// Scales about a centre: uniformly (exact) or per axis.
std::unique_ptr<Shape> scale(const Shape &s, double cx, double cy, double cz, double sx, double sy, double sz) {
  FOX_TRY
  if (sx <= 0 || sy <= 0 || sz <= 0) throw std::runtime_error("scale factors must be positive");
  if (std::abs(sx - sy) < 1e-12 && std::abs(sx - sz) < 1e-12) {
    gp_Trsf t;
    t.SetScale(gp_Pnt(cx, cy, cz), sx);
    BRepBuilderAPI_Transform tr(s.s, t, Standard_True);
    return wrap(tr.Shape());
  }
  gp_GTrsf g;
  g.SetValue(1, 1, sx);
  g.SetValue(2, 2, sy);
  g.SetValue(3, 3, sz);
  g.SetTranslationPart(gp_XYZ(cx * (1 - sx), cy * (1 - sy), cz * (1 - sz)));
  BRepBuilderAPI_GTransform tr(s.s, g, Standard_True);
  if (!tr.IsDone()) throw std::runtime_error("non-uniform scale failed");
  return wrap(checked(tr.Shape(), "scale"));
  FOX_CATCH("scale")
}

// Removes faces (1-based) and heals the gap from the neighbours (defeaturing).
std::unique_ptr<Shape> remove_faces(const Shape &s, rust::Slice<const uint32_t> faces) {
  FOX_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_FACE, map);
  BRepAlgoAPI_Defeaturing df;
  df.SetShape(s.s);
  int n = 0;
  for (uint32_t i : faces) {
    if (i >= 1 && static_cast<int>(i) <= map.Extent()) {
      df.AddFaceToRemove(map(static_cast<int>(i)));
      ++n;
    }
  }
  if (n == 0) throw std::runtime_error("no faces to remove");
  df.SetRunParallel(Standard_False);
  df.Build();
  if (!df.IsDone() || df.HasErrors()) throw std::runtime_error("the faces could not be removed");
  return wrap(checked(df.Shape(), "remove faces"));
  FOX_CATCH("remove faces")
}

// Splits faces (1-based) where a tool shape crosses them; the body stays one solid.
std::unique_ptr<Shape> split_faces(const Shape &s, rust::Slice<const uint32_t> faces, const Shape &tool) {
  FOX_TRY
  TopTools_IndexedMapOfShape map;
  index_map(s.s, TopAbs_FACE, map);
  BRepFeat_SplitShape ss(s.s);
  int n = 0;
  for (uint32_t i : faces) {
    if (i < 1 || static_cast<int>(i) > map.Extent()) continue;
    const TopoDS_Face &f = TopoDS::Face(map(static_cast<int>(i)));
    BRepAlgoAPI_Section sec(f, tool.s, Standard_False);
    sec.ComputePCurveOn1(Standard_True);
    sec.Approximation(Standard_True);
    sec.Build();
    if (!sec.IsDone()) continue;
    for (TopExp_Explorer ex(sec.Shape(), TopAbs_EDGE); ex.More(); ex.Next()) {
      ss.Add(TopoDS::Edge(ex.Current()), f);
      ++n;
    }
  }
  if (n == 0) throw std::runtime_error("the tool does not cross the faces");
  ss.Build();
  if (!ss.IsDone()) throw std::runtime_error("the faces could not be split");
  return wrap(ss.Shape());
  FOX_CATCH("split face")
}

// A surface patch filling a closed boundary (a wire or edges): planar when it can be.
std::unique_ptr<Shape> patch(const Shape &boundary) {
  FOX_TRY
  BRepBuilderAPI_MakeWire mw;
  for (TopExp_Explorer ex(boundary.s, TopAbs_EDGE); ex.More(); ex.Next()) mw.Add(TopoDS::Edge(ex.Current()));
  if (mw.IsDone() && mw.Wire().Closed()) {
    BRepBuilderAPI_MakeFace mf(mw.Wire(), Standard_True);
    if (mf.IsDone()) return wrap(mf.Face());
  }
  BRepFill_Filling fill;
  int n = 0;
  for (TopExp_Explorer ex(boundary.s, TopAbs_EDGE); ex.More(); ex.Next(), ++n) {
    fill.Add(TopoDS::Edge(ex.Current()), GeomAbs_C0, Standard_True);
  }
  if (n == 0) throw std::runtime_error("the boundary has no edges");
  fill.Build();
  if (!fill.IsDone()) throw std::runtime_error("the patch could not be computed");
  return wrap(fill.Face());
  FOX_CATCH("patch")
}

// The boundary loops of a face as ordered polylines (outer loop first).
EdgeData face_loops(const Shape &s, double deflection) {
  FOX_TRY
  EdgeData out;
  TopoDS_Face face;
  for (TopExp_Explorer ex(s.s, TopAbs_FACE); ex.More(); ex.Next()) {
    face = TopoDS::Face(ex.Current());
    break;
  }
  if (face.IsNull()) throw std::runtime_error("no face");
  TopoDS_Wire outer = BRepTools::OuterWire(face);
  std::vector<TopoDS_Wire> wires;
  wires.push_back(outer);
  for (TopExp_Explorer ex(face, TopAbs_WIRE); ex.More(); ex.Next()) {
    if (!ex.Current().IsSame(outer)) wires.push_back(TopoDS::Wire(ex.Current()));
  }
  uint32_t k = 0;
  for (const TopoDS_Wire &w : wires) {
    out.offsets.push_back(static_cast<uint32_t>(out.points.size() / 3));
    out.edge_ids.push_back(++k);
    bool first_edge = true;
    for (BRepTools_WireExplorer we(w, face); we.More(); we.Next()) {
      const TopoDS_Edge &e = we.Current();
      if (BRep_Tool::Degenerated(e)) continue;
      BRepAdaptor_Curve c(e);
      GCPnts_TangentialDeflection td(c, 0.1, deflection);
      int n = td.NbPoints();
      bool rev = e.Orientation() == TopAbs_REVERSED;
      // each edge after the first starts where the previous one ended
      for (int j = first_edge ? 1 : 2; j <= n; ++j) {
        gp_Pnt p = td.Value(rev ? n - j + 1 : j);
        out.points.push_back(p.X());
        out.points.push_back(p.Y());
        out.points.push_back(p.Z());
      }
      first_edge = false;
    }
  }
  out.offsets.push_back(static_cast<uint32_t>(out.points.size() / 3));
  return out;
  FOX_CATCH("face loops")
}

// Every loop of a planar face offset by d (d < 0 inwards), as ordered polylines; empty when the
// offset vanishes. Toolpaths use these: an offset can split a region into several.
EdgeData offset_polylines(const Shape &face, double d, double deflection) {
  FOX_TRY
  EdgeData out;
  if (face.s.ShapeType() != TopAbs_FACE) throw std::runtime_error("offset needs a face");
  BRepOffsetAPI_MakeOffset mk(TopoDS::Face(face.s), GeomAbs_Arc);
  try {
    mk.Perform(d);
  } catch (...) {
    return out;
  }
  if (!mk.IsDone()) return out;
  uint32_t k = 0;
  for (TopExp_Explorer wx(mk.Shape(), TopAbs_WIRE); wx.More(); wx.Next()) {
    out.offsets.push_back(static_cast<uint32_t>(out.points.size() / 3));
    out.edge_ids.push_back(++k);
    bool first_edge = true;
    for (BRepTools_WireExplorer we(TopoDS::Wire(wx.Current())); we.More(); we.Next()) {
      const TopoDS_Edge &e = we.Current();
      if (BRep_Tool::Degenerated(e)) continue;
      BRepAdaptor_Curve c(e);
      GCPnts_TangentialDeflection td(c, 0.1, deflection);
      int n = td.NbPoints();
      bool rev = e.Orientation() == TopAbs_REVERSED;
      for (int j = first_edge ? 1 : 2; j <= n; ++j) {
        gp_Pnt p = td.Value(rev ? n - j + 1 : j);
        out.points.push_back(p.X());
        out.points.push_back(p.Y());
        out.points.push_back(p.Z());
      }
      first_edge = false;
    }
  }
  out.offsets.push_back(static_cast<uint32_t>(out.points.size() / 3));
  return out;
  FOX_CATCH("offset loops")
}

// A ruled surface between two edges.
std::unique_ptr<Shape> ruled(const Shape &a, const Shape &b) {
  FOX_TRY
  if (a.s.ShapeType() != TopAbs_EDGE || b.s.ShapeType() != TopAbs_EDGE) throw std::runtime_error("pick two edges");
  TopoDS_Edge ea = TopoDS::Edge(a.s), eb = TopoDS::Edge(b.s);
  // run both edges the same way, or the surface twists
  auto ends = [](const TopoDS_Edge &e) {
    BRepAdaptor_Curve c(e);
    gp_Pnt p0 = c.Value(c.FirstParameter()), p1 = c.Value(c.LastParameter());
    if (e.Orientation() == TopAbs_REVERSED) std::swap(p0, p1);
    return std::make_pair(p0, p1);
  };
  auto [a0, a1] = ends(ea);
  auto [b0, b1] = ends(eb);
  if (a0.Distance(b0) + a1.Distance(b1) > a0.Distance(b1) + a1.Distance(b0)) eb = TopoDS::Edge(eb.Reversed());
  TopoDS_Face f = BRepFill::Face(ea, eb);
  return wrap(f);
  FOX_CATCH("ruled surface")
}

// Surfaces (or a solid's skin) offset by d along their normals.
std::unique_ptr<Shape> offset_shape(const Shape &s, double d) {
  FOX_TRY
  BRepOffsetAPI_MakeOffsetShape mk;
  mk.PerformBySimple(s.s, d);
  if (!mk.IsDone()) throw std::runtime_error("the surface can't be offset that far");
  return wrap(mk.Shape());
  FOX_CATCH("offset surface")
}

std::unique_ptr<Shape> make_vertex(double x, double y, double z) {
  FOX_TRY
  return wrap(BRepBuilderAPI_MakeVertex(gp_Pnt(x, y, z)).Vertex());
  FOX_CATCH("vertex")
}

std::unique_ptr<Shape> reversed(const Shape &s) {
  FOX_TRY
  return wrap(s.s.Reversed());
  FOX_CATCH("reverse")
}

} // namespace fox
