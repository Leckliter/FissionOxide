// C++ side of the CLFusion kernel bridge. See kernel.hxx.
#include "clf-kernel/cpp/kernel.hxx"
#include "clf-kernel/src/ffi.rs.h"

#include <sstream>
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
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <Interface_Static.hxx>
#include <Poly_Triangulation.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeFix_Shape.hxx>
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
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
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
      size_t o = seg * 10;
      need(segs, o + 10, "make_face segments");
      int kind = static_cast<int>(segs[o]);
      TopoDS_Edge e;
      if (kind == 0) {
        BRepBuilderAPI_MakeEdge me(pnt(segs, o + 1), pnt(segs, o + 4));
        if (!me.IsDone()) throw std::runtime_error("degenerate line in profile");
        e = me.Edge();
      } else if (kind == 1) {
        GC_MakeArcOfCircle arc(pnt(segs, o + 1), pnt(segs, o + 4), pnt(segs, o + 7));
        if (!arc.IsDone()) throw std::runtime_error("degenerate arc in profile");
        BRepBuilderAPI_MakeEdge me(arc.Value());
        if (!me.IsDone()) throw std::runtime_error("degenerate arc in profile");
        e = me.Edge();
      } else {
        gp_Circ c(gp_Ax2(pnt(segs, o + 1), dir(segs, o + 4)), segs[o + 7]);
        BRepBuilderAPI_MakeEdge me(c);
        if (!me.IsDone()) throw std::runtime_error("degenerate circle in profile");
        e = me.Edge();
      }
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

} // namespace clf
