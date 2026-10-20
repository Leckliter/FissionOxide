// C++ side of the second batch of solid features: guided and twisted sweeps, lofts along a centre
// line or rails, emboss, boundary fill, surfaces extended for replace face / extrude to object,
// and setback fillet corners. See kernel.hxx; every entry point catches all exceptions.
#include "fox-kernel/cpp/kernel.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <string>
#include <tuple>
#include <vector>

#include <BOPAlgo_MakerVolume.hxx>
#include <BRepAdaptor_CompCurve.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepFill_Filling.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_UniformAbscissa.hxx>
#include <GProp_GProps.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_BezierCurve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <GeomLProp_SLProps.hxx>
#include <GeomLib.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_BoundedSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_OffsetSurface.hxx>
#include <Geom_Plane.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_SphericalSurface.hxx>
#include <Geom_Surface.hxx>
#include <Precision.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeFix_Shape.hxx>
#include <ShapeFix_Solid.hxx>
#include <Standard_Failure.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TColgp_HArray1OfPnt2d.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColgp_Array1OfPnt2d.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pln.hxx>
#include <Geom_BSplineSurface.hxx>
#include <TColStd_Array2OfReal.hxx>
#include <TColStd_HArray1OfReal.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Quaternion.hxx>

namespace fox {

namespace {

// Converts any exception into std::runtime_error (which cxx maps to Err); as in kernel.cpp.
[[noreturn]] void rethrow2(const char *what) {
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

#define FOX2_TRY                                                                                                       \
  try {                                                                                                                \
    catch_signals();                                                                                                   \
    OCC_CATCH_SIGNALS
#define FOX2_CATCH(what)                                                                                               \
  }                                                                                                                    \
  catch (...) { rethrow2(what); }

std::unique_ptr<Shape> out(const TopoDS_Shape &s) {
  if (s.IsNull()) throw std::runtime_error("kernel returned an empty shape");
  auto o = std::make_unique<Shape>();
  o->s = s;
  return o;
}

void need3(rust::Slice<const double> v, size_t n, const char *what) {
  if (v.size() < n) throw std::runtime_error(std::string(what) + ": too few values");
}

// Validates a result; tries ShapeFix once if invalid.
TopoDS_Shape valid(const TopoDS_Shape &s, const char *what) {
  if (s.IsNull()) throw std::runtime_error(std::string(what) + " produced nothing");
  if (BRepCheck_Analyzer(s).IsValid()) return s;
  ShapeFix_Shape fix(s);
  fix.Perform();
  TopoDS_Shape f = fix.Shape();
  if (!BRepCheck_Analyzer(f).IsValid()) throw std::runtime_error(std::string(what) + " produced an invalid solid");
  return f;
}

double volume(const TopoDS_Shape &s) {
  GProp_GProps g;
  BRepGProp::VolumeProperties(s, g);
  return g.Mass();
}

double diagonal(const TopoDS_Shape &s) {
  Bnd_Box b;
  BRepBndLib::Add(s, b);
  if (b.IsVoid()) return 0.0;
  return std::sqrt(b.SquareExtent());
}

// The wires of a profile: a face's outer wire (and its holes), or a wire as is.
std::vector<TopoDS_Wire> loops(const TopoDS_Shape &s, bool &holes) {
  std::vector<TopoDS_Wire> w;
  holes = false;
  if (s.ShapeType() == TopAbs_FACE) {
    const TopoDS_Face &f = TopoDS::Face(s);
    TopoDS_Wire outer = BRepTools::OuterWire(f);
    w.push_back(outer);
    for (TopExp_Explorer ex(f, TopAbs_WIRE); ex.More(); ex.Next()) {
      if (ex.Current().IsSame(outer)) continue;
      w.push_back(TopoDS::Wire(ex.Current()));
      holes = true;
    }
  } else if (s.ShapeType() == TopAbs_WIRE) {
    w.push_back(TopoDS::Wire(s));
  } else {
    throw std::runtime_error("a profile must be a face or a wire");
  }
  return w;
}

const TopoDS_Wire &as_wire(const Shape &s, const char *what) {
  if (s.s.IsNull() || s.s.ShapeType() != TopAbs_WIRE) throw std::runtime_error(std::string(what) + " must be a wire");
  return TopoDS::Wire(s.s);
}

// Sweeps each loop of a profile with the same pipe settings; holes are cut from the outer solid.
template <typename Setup> TopoDS_Shape sweep_loops(const TopoDS_Wire &spine, const TopoDS_Shape &profile, Setup setup) {
  bool holes = false;
  std::vector<TopoDS_Wire> ws = loops(profile, holes);
  TopoDS_Shape result;
  for (size_t i = 0; i < ws.size(); ++i) {
    BRepOffsetAPI_MakePipeShell ps(spine);
    setup(ps);
    ps.Add(ws[i], Standard_False, Standard_False);
    ps.Build();
    if (!ps.IsDone()) throw std::runtime_error("the sweep failed");
    ps.MakeSolid();
    TopoDS_Shape s = ps.Shape();
    if (volume(s) < 0) s.Reverse();
    if (i == 0) {
      result = s;
    } else {
      BRepAlgoAPI_Cut cut(result, s);
      if (!cut.IsDone()) throw std::runtime_error("the sweep's hole could not be cut");
      result = cut.Shape();
    }
  }
  return result;
}

// Points along a wire at equal arc lengths, with unit tangents.
void sample_wire(const TopoDS_Wire &w, int n, std::vector<gp_Pnt> &pts, std::vector<gp_Vec> &tans) {
  BRepAdaptor_CompCurve c(w);
  GCPnts_UniformAbscissa ua(c, n);
  if (!ua.IsDone() || ua.NbPoints() < 2) throw std::runtime_error("the path could not be sampled");
  for (int i = 1; i <= ua.NbPoints(); ++i) {
    gp_Pnt p;
    gp_Vec d;
    c.D1(ua.Parameter(i), p, d);
    if (d.Magnitude() < 1e-12) throw std::runtime_error("the path has a cusp");
    pts.push_back(p);
    tans.push_back(d.Normalized());
  }
}

// Any unit vector perpendicular to d.
gp_Vec perp(const gp_Vec &d) {
  gp_Vec a = std::abs(d.X()) < 0.9 ? gp_Vec(1, 0, 0) : gp_Vec(0, 1, 0);
  gp_Vec p = a - d * a.Dot(d);
  return p.Normalized();
}

TopoDS_Edge interp_edge(const std::vector<gp_Pnt> &pts, bool periodic) {
  Handle(TColgp_HArray1OfPnt) arr = new TColgp_HArray1OfPnt(1, static_cast<int>(pts.size()));
  for (size_t i = 0; i < pts.size(); ++i) arr->SetValue(static_cast<int>(i) + 1, pts[i]);
  GeomAPI_Interpolate in(arr, periodic ? Standard_True : Standard_False, 1e-9);
  in.Perform();
  if (!in.IsDone()) throw std::runtime_error("a curve could not be interpolated");
  BRepBuilderAPI_MakeEdge me(in.Curve());
  if (!me.IsDone()) throw std::runtime_error("a curve could not be built");
  return me.Edge();
}

// A thick solid grown from a face by d along its normal (outward for faces of a solid).
TopoDS_Shape thick(const TopoDS_Shape &face, double d) {
  BRepOffsetAPI_MakeThickSolid mk;
  mk.MakeThickSolidBySimple(face, d);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the face could not be thickened");
  TopoDS_Shape s = mk.Shape();
  if (volume(s) < 0) s.Reverse();
  return s;
}

TopoDS_Shape common(const TopoDS_Shape &a, const TopoDS_Shape &b) {
  BRepAlgoAPI_Common op(a, b);
  if (!op.IsDone() || op.HasErrors()) throw std::runtime_error("intersection failed");
  return op.Shape();
}

// A layer of `depth` on the right side of a face of `body`: outside it, or inside for a deboss.
TopoDS_Shape layer(const TopoDS_Shape &body, const TopoDS_Shape &face, double depth, bool inside) {
  TopoDS_Shape t = thick(face, depth);
  double v = volume(t);
  if (v <= 0) throw std::runtime_error("the face could not be thickened");
  double overlap = volume(common(t, body)) / v;
  if ((overlap > 0.5) != inside) t = thick(face, -depth);
  return t;
}

TopoDS_Face face_of(const Shape &s, uint32_t face) {
  TopTools_IndexedMapOfShape map;
  TopExp::MapShapes(s.s, TopAbs_FACE, map);
  if (face < 1 || static_cast<int>(face) > map.Extent()) throw std::runtime_error("no such face");
  return TopoDS::Face(map(static_cast<int>(face)));
}

// A face's surface, its trimming and placement removed.
Handle(Geom_Surface) basis(const TopoDS_Face &f) {
  Handle(Geom_Surface) s = BRep_Tool::Surface(f);
  while (s->IsKind(STANDARD_TYPE(Geom_RectangularTrimmedSurface))) {
    s = Handle(Geom_RectangularTrimmedSurface)::DownCast(s)->BasisSurface();
  }
  return s;
}

// The (oriented) unit normal of a face's surface at the point nearest p, and that point.
bool surface_normal(const TopoDS_Face &f, const gp_Pnt &p, gp_Pnt &at, gp_Vec &n) {
  Handle(Geom_Surface) s = BRep_Tool::Surface(f);
  GeomAPI_ProjectPointOnSurf pr(p, s);
  if (!pr.IsDone() || pr.NbPoints() < 1) return false;
  double u, v;
  pr.LowerDistanceParameters(u, v);
  GeomLProp_SLProps props(s, u, v, 1, 1e-9);
  if (!props.IsNormalDefined()) return false;
  at = pr.NearestPoint();
  n = gp_Vec(props.Normal());
  if (f.Orientation() == TopAbs_REVERSED) n.Reverse();
  return true;
}

} // namespace

// Sweeps a profile along a path, oriented and scaled so that it keeps touching a guide rail.
std::unique_ptr<Shape> sweep_rail(const Shape &profile, const Shape &spine, const Shape &rail) {
  FOX2_TRY
  const TopoDS_Wire &path = as_wire(spine, "the sweep path");
  const TopoDS_Wire &guide = as_wire(rail, "the guide rail");
  bool holes = false;
  loops(profile.s, holes);
  if (holes) throw std::runtime_error("a guide rail sweep needs a profile without holes");
  // the rail is matched to the path by relative arc length; each section is scaled about the path
  // until its outline touches the rail
  TopoDS_Shape s = sweep_loops(path, profile.s, [&](BRepOffsetAPI_MakePipeShell &ps) {
    ps.SetMode(guide, Standard_True, BRepFill_ContactOnBorder);
  });
  return out(valid(s, "guided sweep"));
  FOX2_CATCH("guided sweep")
}

// Sweeps a profile along a path, turning it by `twist` radians about the path from start to end.
std::unique_ptr<Shape> sweep_twist(const Shape &profile, const Shape &spine, double twist) {
  FOX2_TRY
  const TopoDS_Wire &path = as_wire(spine, "the sweep path");
  std::vector<gp_Pnt> pts;
  std::vector<gp_Vec> tans;
  sample_wire(path, 120, pts, tans);
  double length = 0;
  for (size_t i = 1; i < pts.size(); ++i) length += pts[i].Distance(pts[i - 1]);
  if (length < 1e-9) throw std::runtime_error("the path has no length");
  // rotation-minimizing frames (double reflection), the twist turned on top of them; the
  // auxiliary spine runs beside the path in the turning direction
  double d = 0.05 * length + 0.5;
  std::vector<gp_Pnt> aux;
  gp_Vec r = perp(tans[0]);
  double s = 0;
  for (size_t i = 0; i < pts.size(); ++i) {
    if (i > 0) {
      s += pts[i].Distance(pts[i - 1]);
      gp_Vec v1(pts[i - 1], pts[i]);
      double c1 = v1.Dot(v1);
      if (c1 > 1e-18) {
        gp_Vec rl = r - v1 * (2.0 / c1 * v1.Dot(r));
        gp_Vec tl = tans[i - 1] - v1 * (2.0 / c1 * v1.Dot(tans[i - 1]));
        gp_Vec v2 = tans[i] - tl;
        double c2 = v2.Dot(v2);
        r = c2 > 1e-18 ? rl - v2 * (2.0 / c2 * v2.Dot(rl)) : rl;
      }
      r = (r - tans[i] * r.Dot(tans[i])).Normalized();
    }
    double a = twist * s / length;
    gp_Vec b = tans[i].Crossed(r);
    aux.push_back(pts[i].Translated((r * std::cos(a) + b * std::sin(a)) * d));
  }
  TopoDS_Wire auxw = BRepBuilderAPI_MakeWire(interp_edge(aux, false));
  TopoDS_Shape out_s = sweep_loops(path, profile.s, [&](BRepOffsetAPI_MakePipeShell &ps) {
    ps.SetMode(auxw, Standard_True, BRepFill_NoContact);
  });
  return out(valid(out_s, "twisted sweep"));
  FOX2_CATCH("twisted sweep")
}

// Lofts through sections (faces or wires of a compound, in order) placed along a centre line.
std::unique_ptr<Shape> sweep_sections(const Shape &sections, const Shape &spine) {
  FOX2_TRY
  const TopoDS_Wire &path = as_wire(spine, "the centre line");
  BRepOffsetAPI_MakePipeShell ps(path);
  ps.SetMode(Standard_False);
  int n = 0;
  for (TopoDS_Iterator it(sections.s); it.More(); it.Next(), ++n) {
    bool holes = false;
    std::vector<TopoDS_Wire> ws = loops(it.Value(), holes);
    if (holes) throw std::runtime_error("loft sections can't have holes");
    ps.Add(ws[0], Standard_False, Standard_False);
  }
  if (n < 2) throw std::runtime_error("a loft needs at least two profiles");
  ps.Build();
  if (!ps.IsDone()) throw std::runtime_error("the loft along the centre line failed");
  ps.MakeSolid();
  TopoDS_Shape s = ps.Shape();
  if (volume(s) < 0) s.Reverse();
  return out(valid(s, "loft"));
  FOX2_CATCH("loft")
}

namespace {
// A closed section as runs of points: one run per edge when every section has as many edges,
// else one run around the whole loop. Runs follow each other; the last ends where the first starts.
struct Section {
  std::vector<std::vector<gp_Pnt>> runs;
  // every point of the loop in order (runs joined, shared ends once)
  std::vector<gp_Pnt> flat() const {
    std::vector<gp_Pnt> f;
    for (const auto &r : runs) f.insert(f.end(), r.begin(), r.end() - 1);
    return f;
  }
};

std::vector<TopoDS_Edge> ordered_edges(const TopoDS_Wire &w) {
  std::vector<TopoDS_Edge> es;
  for (BRepTools_WireExplorer ex(w); ex.More(); ex.Next()) {
    if (!BRep_Tool::Degenerated(ex.Current())) es.push_back(ex.Current());
  }
  return es;
}

std::vector<gp_Pnt> sample_edge(const TopoDS_Edge &e, int n) {
  BRepAdaptor_Curve c(e);
  GCPnts_UniformAbscissa ua(c, n);
  if (!ua.IsDone() || ua.NbPoints() != n) throw std::runtime_error("a section edge could not be sampled");
  std::vector<gp_Pnt> p;
  for (int i = 1; i <= n; ++i) p.push_back(c.Value(ua.Parameter(i)));
  if (e.Orientation() == TopAbs_REVERSED) std::reverse(p.begin(), p.end());
  return p;
}

// Newell normal of a closed polyline.
gp_Vec loop_normal(const std::vector<gp_Pnt> &p) {
  gp_Vec n(0, 0, 0);
  for (size_t i = 0; i < p.size(); ++i) {
    const gp_Pnt &a = p[i], &b = p[(i + 1) % p.size()];
    n += gp_Vec((a.Y() - b.Y()) * (a.Z() + b.Z()), (a.Z() - b.Z()) * (a.X() + b.X()), (a.X() - b.X()) * (a.Y() + b.Y()));
  }
  return n;
}

gp_Pnt centroid(const std::vector<gp_Pnt> &p) {
  gp_XYZ c(0, 0, 0);
  for (const auto &q : p) c += q.XYZ();
  return gp_Pnt(c / static_cast<double>(std::max<size_t>(p.size(), 1)));
}

// A polyline sampled densely along a wire, with the arc length at each point.
struct Rail {
  std::vector<gp_Pnt> p;
  std::vector<double> s;
  // nearest point's arc length and distance
  std::pair<double, double> nearest(const gp_Pnt &q) const {
    double best = 1e300, at = 0;
    for (size_t i = 0; i + 1 < p.size(); ++i) {
      gp_Vec ab(p[i], p[i + 1]);
      double l2 = ab.SquareMagnitude();
      double t = l2 > 0 ? std::clamp(gp_Vec(p[i], q).Dot(ab) / l2, 0.0, 1.0) : 0.0;
      gp_Pnt x = p[i].Translated(ab * t);
      double d = x.Distance(q);
      if (d < best) {
        best = d;
        at = s[i] + t * (s[i + 1] - s[i]);
      }
    }
    return {at, best};
  }
  gp_Pnt at(double len) const {
    for (size_t i = 0; i + 1 < p.size(); ++i) {
      if (len <= s[i + 1] || i + 2 == p.size()) {
        double seg = s[i + 1] - s[i];
        double t = seg > 0 ? std::clamp((len - s[i]) / seg, 0.0, 1.0) : 0.0;
        return p[i].Translated(gp_Vec(p[i], p[i + 1]) * t);
      }
    }
    return p.front();
  }
};

double smooth(double t) { return t * t * (3.0 - 2.0 * t); }

// How steeply a rail's bulge leaves a corner of a polygonal section, as the slope of a Hermite
// cubic over the corner-to-rail stretch, in thirds (Fusion's, measured on a square: pi/10).
const double corner_pull = M_PI / 10.0;

// The rails' pulls spread around a polygonal section: `pts` (closed, `k` points per edge) with
// rail pulls at point indices. As Fusion's loft with rails: a corner is pulled by the rails on
// either side, in proportion to the distance around the section to the far end of each rail's
// edge; an edge no rail touches stays straight (its pull runs linearly between its corners); an
// edge a rail touches bends from its corners to the rail as a cubic, flat where the rail is.
std::vector<gp_Vec> polygon_pulls(const std::vector<gp_Pnt> &pts, size_t k,
                                  const std::vector<std::pair<size_t, gp_Vec>> &pulls) {
  size_t np = pts.size(), ne = np / k;
  std::vector<double> s(np + 1, 0.0);
  for (size_t i = 1; i <= np; ++i) s[i] = s[i - 1] + pts[i - 1].Distance(pts[i % np]);
  double total = s[np];
  auto ahead = [&](size_t from, size_t to) { // distance around from vertex `from` on to vertex `to`
    double d = s[to * k] - s[from * k];
    return d <= 1e-12 ? d + total : d;
  };
  std::vector<std::vector<std::pair<size_t, gp_Vec>>> on_edge(ne);
  std::vector<int> at_vertex(ne, -1);
  for (size_t j = 0; j < pulls.size(); ++j) {
    size_t i = pulls[j].first % np;
    if (i % k == 0) {
      at_vertex[i / k] = static_cast<int>(j);
    } else {
      on_edge[i / k].push_back({i, pulls[j].second});
    }
  }
  for (auto &e : on_edge) std::sort(e.begin(), e.end(), [](const auto &a, const auto &b) { return a.first < b.first; });
  std::vector<gp_Vec> corner(ne);
  for (size_t v = 0; v < ne; ++v) {
    if (at_vertex[v] >= 0) {
      corner[v] = pulls[static_cast<size_t>(at_vertex[v])].second;
      continue;
    }
    gp_Vec fwd, back;
    double df = 0, db = 0;
    for (size_t step = 0; step < ne; ++step) {
      size_t w = (v + step) % ne;
      if (step > 0 && at_vertex[w] >= 0) {
        fwd = pulls[static_cast<size_t>(at_vertex[w])].second;
        df = ahead(v, w);
        break;
      }
      if (!on_edge[w].empty()) {
        fwd = on_edge[w].front().second;
        df = ahead(v, (w + 1) % ne);
        break;
      }
    }
    for (size_t step = 0; step < ne; ++step) {
      size_t w = (v + ne - step) % ne, e = (v + 2 * ne - 1 - step) % ne;
      if (step > 0 && at_vertex[w] >= 0) {
        back = pulls[static_cast<size_t>(at_vertex[w])].second;
        db = ahead(w, v);
        break;
      }
      if (!on_edge[e].empty()) {
        back = on_edge[e].back().second;
        db = ahead(e, v);
        break;
      }
    }
    corner[v] = df + db > 1e-12 ? (back * df + fwd * db) / (df + db) : fwd;
  }
  // a cubic from a corner (t = 0, leaving at corner_pull) to a rail (t = 1, flat)
  auto rise = [](double t) { return t * t * (3 - 2 * t) + 3 * corner_pull * t * (1 - t) * (1 - t); };
  std::vector<gp_Vec> d(np);
  for (size_t e = 0; e < ne; ++e) {
    // the edge's nodes: its corners and the rails on it, with whether each is a corner
    std::vector<std::tuple<size_t, gp_Vec, bool>> nodes{{e * k, corner[e], true}};
    for (const auto &c : on_edge[e]) nodes.push_back({c.first, c.second, false});
    nodes.push_back({(e + 1) * k, corner[(e + 1) % ne], true});
    for (size_t n = 0; n + 1 < nodes.size(); ++n) {
      auto [i0, p0, c0] = nodes[n];
      auto [i1, p1, c1] = nodes[n + 1];
      double len = s[i1] - s[i0];
      for (size_t i = i0; i < i1; ++i) {
        double t = len > 1e-12 ? (s[i] - s[i0]) / len : 0.0;
        double h;
        if (on_edge[e].empty()) {
          h = t;
        } else if (c0 && !c1) {
          h = rise(t);
        } else if (!c0 && c1) {
          h = 1 - rise(1 - t);
        } else {
          h = t * t * (3 - 2 * t);
        }
        d[i % np] = p0 * (1 - h) + p1 * h;
      }
    }
  }
  return d;
}
} // namespace

// Lofts through sections guided by rails: between the sections, every point is pulled along with
// the rails, so the loft's sides follow the rails. One rail pulls its side out, fading away round
// the loop; with more, around a polygonal section the pulls spread as `polygon_pulls` says, around
// a smooth one smoothly from one rail to the next. Without `spread` (designs from before it), more
// than one rail blends smoothly from one rail to the next around a polygonal section too.
std::unique_ptr<Shape> loft_rails(const Shape &sections, const Shape &rails, bool spread) {
  FOX2_TRY
  std::vector<TopoDS_Wire> ws;
  for (TopoDS_Iterator it(sections.s); it.More(); it.Next()) {
    bool holes = false;
    std::vector<TopoDS_Wire> l = loops(it.Value(), holes);
    if (holes) throw std::runtime_error("loft sections can't have holes");
    ws.push_back(l[0]);
  }
  if (ws.size() < 2) throw std::runtime_error("a loft needs at least two profiles");
  std::vector<Rail> rs;
  for (TopoDS_Iterator it(rails.s); it.More(); it.Next()) {
    if (it.Value().ShapeType() != TopAbs_WIRE) throw std::runtime_error("rails must be wires");
    Rail r;
    std::vector<gp_Vec> t;
    sample_wire(TopoDS::Wire(it.Value()), 400, r.p, t);
    r.s.push_back(0);
    for (size_t i = 1; i < r.p.size(); ++i) r.s.push_back(r.s.back() + r.p[i].Distance(r.p[i - 1]));
    rs.push_back(r);
  }
  if (rs.empty()) throw std::runtime_error("no rails");
  const int total = 96;
  // per-edge runs when every section has the same number of edges (corners stay sharp)
  size_t ne = ordered_edges(ws[0]).size();
  bool per_edge = ne > 1;
  for (const auto &w : ws) per_edge = per_edge && ordered_edges(w).size() == ne;
  std::vector<Section> secs;
  for (const auto &w : ws) {
    Section sec;
    std::vector<TopoDS_Edge> es = ordered_edges(w);
    if (per_edge) {
      int k = std::max(6, total / static_cast<int>(ne)) + 1;
      for (const auto &e : es) sec.runs.push_back(sample_edge(e, k));
    } else {
      std::vector<gp_Pnt> p;
      std::vector<gp_Vec> t;
      sample_wire(w, total + 1, p, t);
      p.back() = p.front();
      sec.runs.push_back(p);
    }
    secs.push_back(sec);
  }
  // the same winding everywhere, along the loft
  gp_Vec along(centroid(secs.front().flat()), centroid(secs.back().flat()));
  for (auto &sec : secs) {
    gp_Vec n = loop_normal(sec.flat());
    if (along.Magnitude() > 1e-9 && n.Dot(along) < 0) {
      std::reverse(sec.runs.begin(), sec.runs.end());
      for (auto &r : sec.runs) std::reverse(r.begin(), r.end());
    }
  }
  // start each section at the corner (or, around a smooth loop, the point) nearest the first rail
  if (per_edge) {
    for (auto &sec : secs) {
      size_t best = 0;
      double bd = 1e300;
      for (size_t i = 0; i < sec.runs.size(); ++i) {
        double d = rs[0].nearest(sec.runs[i].front()).second;
        if (d < bd) {
          bd = d;
          best = i;
        }
      }
      std::rotate(sec.runs.begin(), sec.runs.begin() + static_cast<long>(best), sec.runs.end());
    }
  } else {
    for (auto &sec : secs) {
      std::vector<gp_Pnt> f = sec.flat();
      size_t best = 0;
      double bd = 1e300;
      for (size_t i = 0; i < f.size(); ++i) {
        double d = rs[0].nearest(f[i]).second;
        if (d < bd) {
          bd = d;
          best = i;
        }
      }
      std::rotate(f.begin(), f.begin() + static_cast<long>(best), f.end());
      f.push_back(f.front());
      sec.runs = {f};
    }
  }
  // where each rail meets each section: the loop position (0..1) and the rail's arc length
  size_t np = secs[0].flat().size();
  for (const auto &sec : secs) {
    if (sec.flat().size() != np) throw std::runtime_error("the sections could not be matched up");
  }
  double size = 0;
  for (const auto &w : ws) size = std::max(size, diagonal(w));
  std::vector<std::vector<std::pair<double, double>>> hit(secs.size());
  for (size_t i = 0; i < secs.size(); ++i) {
    std::vector<gp_Pnt> f = secs[i].flat();
    for (size_t j = 0; j < rs.size(); ++j) {
      size_t best = 0;
      double bd = 1e300;
      for (size_t k = 0; k < f.size(); ++k) {
        double d = rs[j].nearest(f[k]).second;
        if (d < bd) {
          bd = d;
          best = k;
        }
      }
      if (bd > 0.02 * size + 1e-3) {
        throw std::runtime_error("rail " + std::to_string(j + 1) + " does not touch section " + std::to_string(i + 1));
      }
      hit[i].push_back({static_cast<double>(best) / static_cast<double>(np), rs[j].nearest(f[best]).first});
    }
  }
  BRepOffsetAPI_ThruSections mk(Standard_True, Standard_False, 1e-6);
  auto add = [&](const std::vector<gp_Pnt> &flat) {
    BRepBuilderAPI_MakeWire mw;
    if (per_edge) {
      size_t k = secs[0].runs[0].size() - 1;
      for (size_t e = 0; e < ne; ++e) {
        std::vector<gp_Pnt> run;
        for (size_t m = 0; m <= k; ++m) run.push_back(flat[(e * k + m) % flat.size()]);
        mw.Add(interp_edge(run, false));
      }
    } else {
      mw.Add(interp_edge(flat, true));
    }
    if (!mw.IsDone()) throw std::runtime_error("a loft section could not be built");
    mk.AddWire(mw.Wire());
  };
  const int steps = 8;
  for (size_t i = 0; i < secs.size(); ++i) {
    std::vector<gp_Pnt> a = secs[i].flat();
    add(a);
    if (i + 1 == secs.size()) break;
    std::vector<gp_Pnt> b = secs[i + 1].flat();
    for (int m = 1; m < steps; ++m) {
      double t = static_cast<double>(m) / steps;
      // the rails' pull at their contact positions
      std::vector<std::pair<double, gp_Vec>> pull;
      std::vector<std::pair<size_t, gp_Vec>> at;
      for (size_t j = 0; j < rs.size(); ++j) {
        auto [ua, sa] = hit[i][j];
        auto [ub, sb] = hit[i + 1][j];
        size_t ka = static_cast<size_t>(std::lround(ua * np)) % np, kb = static_cast<size_t>(std::lround(ub * np)) % np;
        gp_Pnt blended(a[ka].XYZ() * (1 - t) + b[kb].XYZ() * t);
        gp_Pnt target = rs[j].at(sa + (sb - sa) * t);
        pull.push_back({ua + (ub - ua) * t, gp_Vec(blended, target)});
        at.push_back({static_cast<size_t>(std::lround((ua + (ub - ua) * t) * np)) % np, gp_Vec(blended, target)});
      }
      std::sort(pull.begin(), pull.end(), [](const auto &x, const auto &y) { return x.first < y.first; });
      std::vector<gp_Pnt> mid;
      for (size_t k = 0; k < np; ++k) mid.push_back(gp_Pnt(a[k].XYZ() * (1 - t) + b[k].XYZ() * t));
      if (pull.size() == 1) {
        // one rail pulls its side out, fading away round the loop
        for (size_t k = 0; k < np; ++k) {
          double u = static_cast<double>(k) / static_cast<double>(np);
          mid[k].Translate(pull[0].second * ((1 + std::cos(2 * M_PI * (u - pull[0].first))) / 2));
        }
      } else if (per_edge && spread) {
        // the pulls' common part along the loft moves the whole section; the rest reshapes it
        gp_Vec ax(centroid(a), centroid(b));
        gp_Vec shift(0, 0, 0);
        if (ax.Magnitude() > 1e-9) {
          ax.Normalize();
          double along = 0;
          for (const auto &p : at) along += p.second.Dot(ax);
          shift = ax * (along / static_cast<double>(at.size()));
        }
        for (auto &p : at) p.second -= shift;
        std::vector<gp_Vec> d = polygon_pulls(mid, secs[0].runs[0].size() - 1, at);
        for (size_t k = 0; k < np; ++k) mid[k].Translate(d[k] + shift);
      } else {
        for (size_t k = 0; k < np; ++k) {
          double u = static_cast<double>(k) / static_cast<double>(np);
          // the contacts on either side of u, around the loop
          size_t hi = 0;
          while (hi < pull.size() && pull[hi].first <= u) ++hi;
          size_t lo = (hi + pull.size() - 1) % pull.size();
          hi %= pull.size();
          double u0 = pull[lo].first, u1 = pull[hi].first;
          double span = u1 - u0, off = u - u0;
          if (span <= 0) span += 1;
          if (off < 0) off += 1;
          double h = smooth(std::clamp(off / span, 0.0, 1.0));
          mid[k].Translate(pull[lo].second * (1 - h) + pull[hi].second * h);
        }
      }
      add(mid);
    }
  }
  mk.CheckCompatibility(Standard_False);
  mk.Build();
  if (!mk.IsDone()) throw std::runtime_error("the loft along the rails failed");
  TopoDS_Shape s = mk.Shape();
  if (volume(s) < 0) s.Reverse();
  return out(valid(s, "loft"));
  FOX2_CATCH("loft")
}

namespace {
// A cubic B-spline through points at equal parameter steps over 0..1 (periodic: closing back to the
// first point), so curves through equally many points share their knots and their poles blend.
Handle(Geom_BSplineCurve) interp_uniform(const std::vector<gp_Pnt> &pts, bool periodic) {
  int n = static_cast<int>(pts.size());
  Handle(TColgp_HArray1OfPnt) arr = new TColgp_HArray1OfPnt(1, n);
  for (int i = 0; i < n; ++i) arr->SetValue(i + 1, pts[static_cast<size_t>(i)]);
  int np = periodic ? n + 1 : n;
  Handle(TColStd_HArray1OfReal) par = new TColStd_HArray1OfReal(1, np);
  for (int i = 0; i < np; ++i) par->SetValue(i + 1, static_cast<double>(i) / static_cast<double>(np - 1));
  GeomAPI_Interpolate in(arr, par, periodic ? Standard_True : Standard_False, 1e-9);
  in.Perform();
  if (!in.IsDone()) throw std::runtime_error("a loft section could not be interpolated");
  return in.Curve();
}

// A solid bounded by faces: sewn into one closed shell, oriented outward.
TopoDS_Shape solid_of(const std::vector<TopoDS_Face> &faces, const char *what) {
  BRepBuilderAPI_Sewing sew(1e-4);
  for (const auto &f : faces) sew.Add(f);
  sew.Perform();
  TopoDS_Shape sewn = sew.SewedShape();
  TopoDS_Shell shell;
  int shells = 0;
  for (TopExp_Explorer ex(sewn, TopAbs_SHELL); ex.More(); ex.Next(), ++shells) shell = TopoDS::Shell(ex.Current());
  if (shells == 0) {
    // a single face closed on itself (a ring) sews to the bare face
    BRep_Builder bb;
    bb.MakeShell(shell);
    for (TopExp_Explorer ex(sewn, TopAbs_FACE); ex.More(); ex.Next(), ++shells) bb.Add(shell, ex.Current());
    if (shells != 1) throw std::runtime_error(std::string(what) + " is not closed");
    shell.Closed(Standard_True);
  } else if (shells != 1) {
    throw std::runtime_error(std::string(what) + " is not closed");
  }
  BRepBuilderAPI_MakeSolid ms(shell);
  if (!ms.IsDone()) throw std::runtime_error(std::string(what) + " is not closed");
  ShapeFix_Solid fs(ms.Solid());
  fs.Perform();
  TopoDS_Shape solid = fs.Solid();
  if (volume(solid) < 0) solid.Reverse();
  return valid(solid, what);
}

// A B-spline surface face from rows of poles (one row per v pole, each as long as `curve`'s
// poles): u takes `curve`'s knots, degree and periodicity, v the given ones.
TopoDS_Face grid_face(const Handle(Geom_BSplineCurve) &curve, const std::vector<std::vector<gp_Pnt>> &rows,
                      const std::vector<std::vector<double>> &weights, int vdeg, const std::vector<double> &vknots,
                      const std::vector<int> &vmults, bool vperiodic) {
  int nu = curve->NbPoles(), nv = static_cast<int>(rows.size());
  TColgp_Array2OfPnt poles(1, nu, 1, nv);
  TColStd_Array2OfReal w(1, nu, 1, nv);
  for (int j = 0; j < nv; ++j) {
    for (int i = 0; i < nu; ++i) {
      poles.SetValue(i + 1, j + 1, rows[static_cast<size_t>(j)][static_cast<size_t>(i)]);
      w.SetValue(i + 1, j + 1, weights[static_cast<size_t>(j)][static_cast<size_t>(i)]);
    }
  }
  TColStd_Array1OfReal vk(1, static_cast<int>(vknots.size()));
  TColStd_Array1OfInteger vm(1, static_cast<int>(vmults.size()));
  for (size_t i = 0; i < vknots.size(); ++i) {
    vk.SetValue(static_cast<int>(i) + 1, vknots[i]);
    vm.SetValue(static_cast<int>(i) + 1, vmults[i]);
  }
  Handle(Geom_BSplineSurface) s = new Geom_BSplineSurface(poles, w, curve->Knots(), vk, curve->Multiplicities(), vm,
                                                          curve->Degree(), vdeg, curve->IsPeriodic(), vperiodic);
  BRepBuilderAPI_MakeFace mf(s, Precision::Confusion());
  if (!mf.IsDone()) throw std::runtime_error("a loft face could not be built");
  return mf.Face();
}

std::vector<gp_Pnt> poles_of(const Handle(Geom_BSplineCurve) &c) {
  std::vector<gp_Pnt> p;
  for (int i = 1; i <= c->NbPoles(); ++i) p.push_back(c->Pole(i));
  return p;
}

std::vector<double> weights_of(const Handle(Geom_BSplineCurve) &c) {
  std::vector<double> w;
  for (int i = 1; i <= c->NbPoles(); ++i) w.push_back(c->Weight(i));
  return w;
}
} // namespace

// A closed loft through sections (faces or wires of a compound, in order): one surface that runs
// smoothly through every section and back to the first, with no seam. Corresponding points of the
// sections are joined by a periodic Catmull-Rom spline (each point's tangent is half the step
// from the section before to the one after), as Fusion's closed loft is.
std::unique_ptr<Shape> loft_closed(const Shape &sections) {
  FOX2_TRY
  std::vector<TopoDS_Wire> ws;
  for (TopoDS_Iterator it(sections.s); it.More(); it.Next()) {
    if (it.Value().ShapeType() == TopAbs_VERTEX) throw std::runtime_error("a closed loft can't pass through a point");
    bool holes = false;
    std::vector<TopoDS_Wire> l = loops(it.Value(), holes);
    if (holes) throw std::runtime_error("loft sections can't have holes");
    ws.push_back(l[0]);
  }
  if (ws.size() < 3) throw std::runtime_error("a closed loft needs at least three profiles");
  const size_t n = ws.size();
  const int total = 96;
  // one face per edge when every section has as many edges (corners stay sharp), else one around
  size_t ne = ordered_edges(ws[0]).size();
  bool per_edge = ne > 1;
  for (const auto &w : ws) per_edge = per_edge && ordered_edges(w).size() == ne;
  int k = per_edge ? std::max(6, total / static_cast<int>(ne)) : total;
  std::vector<std::vector<gp_Pnt>> flat(n);
  for (size_t i = 0; i < n; ++i) {
    if (per_edge) {
      for (const auto &e : ordered_edges(ws[i])) {
        std::vector<gp_Pnt> r = sample_edge(e, k + 1);
        flat[i].insert(flat[i].end(), r.begin(), r.end() - 1);
      }
    } else {
      std::vector<gp_Pnt> p;
      std::vector<gp_Vec> t;
      sample_wire(ws[i], total + 1, p, t);
      p.pop_back();
      flat[i] = p;
    }
  }
  size_t np = flat[0].size();
  for (const auto &f : flat) {
    if (f.size() != np) throw std::runtime_error("the sections could not be matched up");
  }
  // every section wound the same way along the loop, and started where the one before starts once
  // it is turned to face the same way (so a ring of circles doesn't twist)
  std::vector<gp_Pnt> cen;
  for (const auto &f : flat) cen.push_back(centroid(f));
  std::vector<gp_Vec> nrm(n);
  for (size_t i = 0; i < n; ++i) {
    gp_Vec along(cen[(i + n - 1) % n], cen[(i + 1) % n]);
    gp_Vec nv = loop_normal(flat[i]);
    if (along.Magnitude() > 1e-9 && nv.Dot(along) < 0) {
      // reversed, still starting at the same point (and so at a corner)
      std::reverse(flat[i].begin(), flat[i].end());
      std::rotate(flat[i].begin(), flat[i].end() - 1, flat[i].end());
      nv.Reverse();
    }
    if (nv.Magnitude() < 1e-12) throw std::runtime_error("a loft section has no area");
    nrm[i] = nv.Normalized();
  }
  for (size_t i = 1; i < n; ++i) {
    gp_Quaternion q(nrm[i], nrm[i - 1]);
    size_t step = per_edge ? static_cast<size_t>(k) : 1, best = 0;
    double bd = 1e300;
    for (size_t off = 0; off < np; off += step) {
      double d = 0;
      for (size_t j = 0; j < np; ++j) {
        gp_Vec a = q.Multiply(gp_Vec(cen[i], flat[i][(j + off) % np]));
        d += (a - gp_Vec(cen[i - 1], flat[i - 1][j])).SquareMagnitude();
      }
      if (d < bd) {
        bd = d;
        best = off;
      }
    }
    std::rotate(flat[i].begin(), flat[i].begin() + static_cast<long>(best), flat[i].end());
  }
  // v: section i at knot i, Bezier poles P, P + T/3, P' - T'/3 to the next section (periodic)
  std::vector<double> vknots;
  std::vector<int> vmults;
  for (size_t i = 0; i <= n; ++i) {
    vknots.push_back(static_cast<double>(i));
    vmults.push_back(3);
  }
  std::vector<TopoDS_Face> faces;
  size_t runs = per_edge ? ne : 1;
  for (size_t r = 0; r < runs; ++r) {
    std::vector<Handle(Geom_BSplineCurve)> cs;
    for (size_t i = 0; i < n; ++i) {
      std::vector<gp_Pnt> run;
      if (per_edge) {
        for (int m = 0; m <= k; ++m) run.push_back(flat[i][(r * static_cast<size_t>(k) + static_cast<size_t>(m)) % np]);
      } else {
        run = flat[i];
      }
      cs.push_back(interp_uniform(run, !per_edge));
    }
    std::vector<std::vector<gp_Pnt>> rows;
    std::vector<std::vector<double>> wts;
    for (size_t i = 0; i < n; ++i) {
      std::vector<gp_Pnt> p = poles_of(cs[i]), pn = poles_of(cs[(i + 1) % n]), pp = poles_of(cs[(i + n - 1) % n]),
                          pnn = poles_of(cs[(i + 2) % n]);
      std::vector<gp_Pnt> a, b;
      for (size_t j = 0; j < p.size(); ++j) {
        gp_XYZ t0 = (pn[j].XYZ() - pp[j].XYZ()) / 2.0, t1 = (pnn[j].XYZ() - p[j].XYZ()) / 2.0;
        a.push_back(gp_Pnt(p[j].XYZ() + t0 / 3.0));
        b.push_back(gp_Pnt(pn[j].XYZ() - t1 / 3.0));
      }
      rows.push_back(p);
      rows.push_back(a);
      rows.push_back(b);
      for (int m = 0; m < 3; ++m) wts.push_back(std::vector<double>(p.size(), 1.0));
    }
    faces.push_back(grid_face(cs[0], rows, wts, 3, vknots, vmults, true));
  }
  return out(solid_of(faces, "closed loft"));
  FOX2_CATCH("closed loft")
}

// A loft from a profile to a point (`apex`) that arrives tangent to the plane through the point
// across `axis`, as Fusion's point tangent end condition: each point P of the profile runs to the
// apex along a parabola whose middle pole is A + weight/2 * (P - A) with the axis part removed,
// so a circle under its centre gives a paraboloid.
std::unique_ptr<Shape> loft_point_tangent(const Shape &profile, const Shape &apex, rust::Slice<const double> axis,
                                          double weight) {
  FOX2_TRY
  need3(axis, 3, "axis");
  if (apex.s.IsNull() || apex.s.ShapeType() != TopAbs_VERTEX) throw std::runtime_error("the end must be a point");
  if (!(weight > 0)) throw std::runtime_error("the weight must be above zero");
  gp_Vec n(axis[0], axis[1], axis[2]);
  if (n.Magnitude() < 1e-12) throw std::runtime_error("zero axis");
  n.Normalize();
  gp_Pnt a = BRep_Tool::Pnt(TopoDS::Vertex(apex.s));
  bool holes = false;
  std::vector<TopoDS_Wire> l = loops(profile.s, holes);
  if (holes) throw std::runtime_error("loft sections can't have holes");
  TopoDS_Face cap;
  if (profile.s.ShapeType() == TopAbs_FACE) {
    cap = TopoDS::Face(profile.s);
  } else {
    BRepBuilderAPI_MakeFace mf(l[0], Standard_True);
    if (!mf.IsDone()) throw std::runtime_error("the profile is not flat");
    cap = mf.Face();
  }
  std::vector<TopoDS_Face> faces{cap};
  // each edge as a curve through points along it (a lone closed edge, periodic)
  std::vector<TopoDS_Edge> es = ordered_edges(l[0]);
  bool loop = es.size() == 1;
  int k = loop ? 96 : std::max(8, 96 / static_cast<int>(es.size()));
  for (const auto &e : es) {
    std::vector<gp_Pnt> pts = sample_edge(e, k + 1);
    if (loop) pts.pop_back();
    Handle(Geom_BSplineCurve) bs = interp_uniform(pts, loop);
    std::vector<gp_Pnt> p = poles_of(bs), mid, top;
    for (const auto &q : p) {
      gp_Vec d(a, q);
      d -= n * d.Dot(n);
      mid.push_back(a.Translated(d * (weight / 2.0)));
      top.push_back(a);
    }
    std::vector<double> w(p.size(), 1.0);
    faces.push_back(grid_face(bs, {p, mid, top}, {w, w, w}, 2, {0.0, 1.0}, {3, 3}, false));
  }
  return out(solid_of(faces, "loft"));
  FOX2_CATCH("loft")
}

// The material an emboss adds (or a deboss removes): profile faces projected along `dir` onto
// face `face` of `body`, `depth` deep. With `wrap` (cylindrical faces only), the profiles are
// wrapped around the cylinder instead, keeping their lengths.
std::unique_ptr<Shape> emboss_tool(const Shape &body, uint32_t face, const Shape &profiles,
                                   rust::Slice<const double> dir, double depth, bool deboss, bool wrap) {
  FOX2_TRY
  need3(dir, 3, "emboss direction");
  if (!(depth > 0)) throw std::runtime_error("the depth must be positive");
  TopoDS_Face target = face_of(body, face);
  gp_Vec n(dir[0], dir[1], dir[2]);
  if (n.Magnitude() < 1e-12) throw std::runtime_error("zero direction");
  n.Normalize();
  if (!wrap) {
    TopoDS_Shape lay = layer(body.s, target, depth, deboss);
    double l = 2.0 * (diagonal(body.s) + diagonal(profiles.s)) + 10.0;
    BRep_Builder bb;
    TopoDS_Compound prisms;
    bb.MakeCompound(prisms);
    for (TopExp_Explorer ex(profiles.s, TopAbs_FACE); ex.More(); ex.Next()) {
      gp_Trsf t;
      t.SetTranslation(n * -l);
      TopoDS_Shape f = BRepBuilderAPI_Transform(ex.Current(), t, Standard_True).Shape();
      BRepPrimAPI_MakePrism pr(f, n * (2 * l), Standard_True);
      pr.Build();
      if (!pr.IsDone()) throw std::runtime_error("a profile could not be projected");
      bb.Add(prisms, pr.Shape());
    }
    // only where the face looks back at the sketch: a projection also crosses the far side of a
    // closed face
    TopoDS_Compound tool;
    bb.MakeCompound(tool);
    double kept = 0;
    for (TopExp_Explorer ex(common(lay, prisms), TopAbs_SOLID); ex.More(); ex.Next()) {
      GProp_GProps g;
      BRepGProp::VolumeProperties(ex.Current(), g);
      gp_Pnt at;
      gp_Vec nrm;
      if (std::abs(g.Mass()) < 1e-12 || !surface_normal(target, g.CentreOfMass(), at, nrm) || nrm.Dot(n) >= 0) continue;
      bb.Add(tool, ex.Current());
      kept += std::abs(g.Mass());
    }
    if (kept < 1e-9) throw std::runtime_error("the profiles don't land on the face");
    return out(tool);
  }
  Handle(Geom_Surface) s = basis(target);
  Handle(Geom_CylindricalSurface) cyl = Handle(Geom_CylindricalSurface)::DownCast(s);
  if (cyl.IsNull()) throw std::runtime_error("wrapping works on cylindrical faces");
  gp_Ax1 axis = cyl->Axis();
  gp_Vec a(axis.Direction());
  double r = cyl->Radius();
  if (std::abs(n.Dot(a)) > 1e-6) throw std::runtime_error("the sketch must be parallel to the cylinder's axis");
  // the line where the sketch plane would touch the cylinder: the cylinder side facing the sketch
  // (against `dir`); arc length around the cylinder replaces distance across the sketch
  gp_Vec side = -n;
  gp_Vec across = a.Crossed(side);
  gp_Pnt o = axis.Location();
  Handle(Geom_CylindricalSurface) wrapcyl = new Geom_CylindricalSurface(gp_Ax3(o, gp_Dir(a), gp_Dir(side)), r);
  auto uv = [&](const gp_Pnt &p) {
    gp_Vec v(o, p);
    return gp_Pnt2d(v.Dot(across) / r, v.Dot(a));
  };
  BRep_Builder bb;
  TopoDS_Compound tools;
  bb.MakeCompound(tools);
  int nf = 0;
  for (TopExp_Explorer ex(profiles.s, TopAbs_FACE); ex.More(); ex.Next(), ++nf) {
    const TopoDS_Face &pf = TopoDS::Face(ex.Current());
    TopoDS_Wire outer = BRepTools::OuterWire(pf);
    std::vector<TopoDS_Wire> wires = {outer};
    for (TopExp_Explorer wx(pf, TopAbs_WIRE); wx.More(); wx.Next()) {
      if (!wx.Current().IsSame(outer)) wires.push_back(TopoDS::Wire(wx.Current()));
    }
    std::vector<TopoDS_Wire> mapped;
    for (const TopoDS_Wire &w : wires) {
      BRepBuilderAPI_MakeWire mw;
      for (BRepTools_WireExplorer we(w, pf); we.More(); we.Next()) {
        const TopoDS_Edge &e = we.Current();
        if (BRep_Tool::Degenerated(e)) continue;
        BRepAdaptor_Curve c(e);
        Handle(Geom2d_Curve) c2;
        bool rev = e.Orientation() == TopAbs_REVERSED;
        if (c.GetType() == GeomAbs_Line) {
          gp_Pnt2d p0 = uv(c.Value(rev ? c.LastParameter() : c.FirstParameter()));
          gp_Pnt2d p1 = uv(c.Value(rev ? c.FirstParameter() : c.LastParameter()));
          if (p0.Distance(p1) < 1e-12) continue;
          Handle(Geom2d_Line) ln = new Geom2d_Line(p0, gp_Dir2d(gp_Vec2d(p0, p1)));
          c2 = new Geom2d_TrimmedCurve(ln, 0.0, p0.Distance(p1));
        } else if (c.GetType() == GeomAbs_BSplineCurve || c.GetType() == GeomAbs_BezierCurve) {
          // uv is linear in the point, so a spline maps exactly through its poles (a sketch
          // spline is one edge however many spans it has: sampling it would lose its shape)
          double u0 = c.FirstParameter(), u1 = c.LastParameter();
          if (c.GetType() == GeomAbs_BSplineCurve) {
            Handle(Geom_BSplineCurve) b = c.BSpline();
            TColgp_Array1OfPnt2d poles(1, b->NbPoles());
            for (int i = 1; i <= b->NbPoles(); ++i) poles.SetValue(i, uv(b->Pole(i)));
            TColStd_Array1OfReal knots(1, b->NbKnots());
            TColStd_Array1OfInteger mults(1, b->NbKnots());
            b->Knots(knots);
            b->Multiplicities(mults);
            Handle(Geom2d_BSplineCurve) b2;
            if (b->IsRational()) {
              TColStd_Array1OfReal w(1, b->NbPoles());
              b->Weights(w);
              b2 = new Geom2d_BSplineCurve(poles, w, knots, mults, b->Degree(), b->IsPeriodic());
            } else {
              b2 = new Geom2d_BSplineCurve(poles, knots, mults, b->Degree(), b->IsPeriodic());
            }
            if (u0 > b2->FirstParameter() + 1e-12 || u1 < b2->LastParameter() - 1e-12) b2->Segment(u0, u1);
            if (rev) b2->Reverse();
            c2 = b2;
          } else {
            Handle(Geom_BezierCurve) b = c.Bezier();
            TColgp_Array1OfPnt2d poles(1, b->NbPoles());
            for (int i = 1; i <= b->NbPoles(); ++i) poles.SetValue(i, uv(b->Pole(i)));
            Handle(Geom2d_BezierCurve) b2;
            if (b->IsRational()) {
              TColStd_Array1OfReal w(1, b->NbPoles());
              b->Weights(w);
              b2 = new Geom2d_BezierCurve(poles, w);
            } else {
              b2 = new Geom2d_BezierCurve(poles);
            }
            if (u0 > 1e-12 || u1 < 1.0 - 1e-12) b2->Segment(u0, u1);
            if (rev) b2->Reverse();
            c2 = b2;
          }
        } else {
          GCPnts_UniformAbscissa ua(c, 33);
          if (!ua.IsDone()) throw std::runtime_error("a profile curve could not be sampled");
          int k = ua.NbPoints();
          Handle(TColgp_HArray1OfPnt2d) arr = new TColgp_HArray1OfPnt2d(1, k);
          for (int i = 1; i <= k; ++i) arr->SetValue(i, uv(c.Value(ua.Parameter(rev ? k - i + 1 : i))));
          Geom2dAPI_Interpolate in(arr, Standard_False, 1e-9);
          in.Perform();
          if (!in.IsDone()) throw std::runtime_error("a profile curve could not be wrapped");
          c2 = in.Curve();
        }
        BRepBuilderAPI_MakeEdge me(c2, wrapcyl);
        if (!me.IsDone()) throw std::runtime_error("a wrapped edge could not be built");
        mw.Add(me.Edge());
        if (!mw.IsDone()) throw std::runtime_error("a wrapped profile is not connected");
      }
      if (!mw.IsDone()) throw std::runtime_error("empty profile loop");
      TopoDS_Wire ww = mw.Wire();
      BRepLib::BuildCurves3d(ww, 1e-6);
      mapped.push_back(ww);
    }
    BRepBuilderAPI_MakeFace mf(wrapcyl, mapped[0], Standard_True);
    for (size_t i = 1; i < mapped.size(); ++i) mf.Add(mapped[i]);
    if (!mf.IsDone()) throw std::runtime_error("the wrapped profile could not be made into a face");
    ShapeFix_Face fix(mf.Face());
    fix.FixOrientation();
    fix.Perform();
    bb.Add(tools, layer(body.s, fix.Face(), depth, deboss));
  }
  if (nf == 0) throw std::runtime_error("no profiles");
  return out(tools);
  FOX2_CATCH("emboss")
}

// The closed cells a set of bodies and surfaces bound (boundary fill), as a compound of solids.
std::unique_ptr<Shape> boundary_cells(const Shape &tools) {
  FOX2_TRY
  BOPAlgo_MakerVolume mv;
  TopTools_ListOfShape args;
  for (TopoDS_Iterator it(tools.s); it.More(); it.Next()) args.Append(it.Value());
  if (args.IsEmpty()) throw std::runtime_error("no tools");
  mv.SetArguments(args);
  mv.SetIntersect(Standard_True);
  mv.SetRunParallel(Standard_False);
  mv.Perform();
  if (mv.HasErrors()) throw std::runtime_error("the tools don't bound any cells");
  return out(mv.Shape());
  FOX2_CATCH("boundary fill")
}

// 0 inside, 1 outside, 2 on the boundary of a solid.
int32_t classify(const Shape &s, double x, double y, double z) {
  FOX2_TRY
  BRepClass3d_SolidClassifier c(s.s, gp_Pnt(x, y, z), 1e-7);
  switch (c.State()) {
  case TopAbs_IN: return 0;
  case TopAbs_ON: return 2;
  default: return 1;
  }
  FOX2_CATCH("classify")
}

// A square planar face of half-size `size` centred at origin(3) with normal(3).
std::unique_ptr<Shape> plane_face(rust::Slice<const double> plane, double size) {
  FOX2_TRY
  need3(plane, 6, "plane");
  gp_Pln pln(gp_Pnt(plane[0], plane[1], plane[2]), gp_Dir(plane[3], plane[4], plane[5]));
  BRepBuilderAPI_MakeFace mf(pln, -size, size, -size, size);
  if (!mf.IsDone()) throw std::runtime_error("could not make the plane");
  return out(mf.Face());
  FOX2_CATCH("plane")
}

// A face's surface extended by `size` on every side (whole for closed surfaces), optionally
// offset by `offset` towards `toward` (judged where the surface is nearest `at`); it keeps the
// face's orientation.
std::unique_ptr<Shape> extended_face(const Shape &face, double size, double offset, rust::Slice<const double> at,
                                     rust::Slice<const double> toward) {
  FOX2_TRY
  need3(toward, 3, "direction");
  need3(at, 3, "point");
  if (face.s.ShapeType() != TopAbs_FACE) throw std::runtime_error("extend needs a face");
  const TopoDS_Face &f = TopoDS::Face(face.s);
  Handle(Geom_Surface) s = basis(f);
  double fu0, fu1, fv0, fv1;
  BRepTools::UVBounds(f, fu0, fu1, fv0, fv1);
  Handle(Geom_BoundedSurface) bs = Handle(Geom_BoundedSurface)::DownCast(s);
  if (!bs.IsNull()) {
    bs = Handle(Geom_BoundedSurface)::DownCast(bs->Copy());
    for (int i = 0; i < 4; ++i) {
      bool in_u = i < 2, after = i % 2 == 0;
      if ((in_u && bs->IsUPeriodic()) || (!in_u && bs->IsVPeriodic())) continue;
      try {
        GeomLib::ExtendSurfByLength(bs, size, 1, in_u ? Standard_True : Standard_False, after ? Standard_True : Standard_False);
      } catch (...) {
      }
    }
    s = bs;
  }
  double u0, u1, v0, v1;
  s->Bounds(u0, u1, v0, v1);
  if (bs.IsNull() && s->IsKind(STANDARD_TYPE(Geom_Plane))) {
    double cu = (fu0 + fu1) / 2, cv = (fv0 + fv1) / 2;
    u0 = cu - size;
    u1 = cu + size;
    v0 = cv - size;
    v1 = cv + size;
  } else if (bs.IsNull()) {
    // closed directions stay whole, open ones grow by `size` past the face
    if (Precision::IsInfinite(u0) || Precision::IsInfinite(u1)) {
      u0 = fu0 - size;
      u1 = fu1 + size;
    }
    if (Precision::IsInfinite(v0) || Precision::IsInfinite(v1)) {
      v0 = fv0 - size;
      v1 = fv1 + size;
    }
  }
  if (std::abs(offset) > 1e-12) {
    gp_Pnt foot;
    gp_Vec nrm;
    if (!surface_normal(f, gp_Pnt(at[0], at[1], at[2]), foot, nrm)) throw std::runtime_error("the face has no normal");
    // Geom_OffsetSurface offsets along the surface's own normal (orientation ignored)
    if (f.Orientation() == TopAbs_REVERSED) nrm.Reverse();
    double sign = nrm.Dot(gp_Vec(toward[0], toward[1], toward[2])) >= 0 ? 1.0 : -1.0;
    s = new Geom_OffsetSurface(s, sign * offset);
  }
  BRepBuilderAPI_MakeFace mf(s, u0, u1, v0, v1, 1e-6);
  if (!mf.IsDone()) throw std::runtime_error("the surface could not be extended");
  TopoDS_Face e = mf.Face();
  if (f.Orientation() == TopAbs_REVERSED) e.Reverse();
  return out(e);
  FOX2_CATCH("extend surface")
}

// Signed distance of a point from a face's surface, positive on the side its normal points to.
double side_of(const Shape &face, double x, double y, double z) {
  FOX2_TRY
  if (face.s.ShapeType() != TopAbs_FACE) throw std::runtime_error("side needs a face");
  gp_Pnt p(x, y, z), at;
  gp_Vec n;
  if (!surface_normal(TopoDS::Face(face.s), p, at, n)) throw std::runtime_error("the point could not be projected");
  return gp_Vec(at, p).Dot(n);
  FOX2_CATCH("side")
}

// Re-blends filleted corners: around each corner point (3 doubles each) everything within
// `radius` is cut away and the hole filled with a smooth patch tangent to the faces around it
// (Fusion's setback fillet corners).
std::unique_ptr<Shape> setback_corners(const Shape &s, rust::Slice<const double> points, double radius) {
  FOX2_TRY
  if (!(radius > 0)) throw std::runtime_error("the setback must be positive");
  TopoDS_Shape cur = s.s;
  for (size_t i = 0; i + 2 < points.size(); i += 3) {
    gp_Pnt c(points[i], points[i + 1], points[i + 2]);
    TopoDS_Shape ball = BRepPrimAPI_MakeSphere(c, radius).Shape();
    BRepAlgoAPI_Cut cut(cur, ball);
    if (!cut.IsDone() || cut.HasErrors()) throw std::runtime_error("the corner could not be cut");
    // the faces left, without the ball's
    std::vector<TopoDS_Face> keep;
    int cutfaces = 0;
    for (TopExp_Explorer ex(cut.Shape(), TopAbs_FACE); ex.More(); ex.Next()) {
      const TopoDS_Face &f = TopoDS::Face(ex.Current());
      Handle(Geom_SphericalSurface) sp = Handle(Geom_SphericalSurface)::DownCast(basis(f));
      if (!sp.IsNull() && sp->Location().Distance(c) < 1e-6 && std::abs(sp->Radius() - radius) < 1e-6) {
        ++cutfaces;
        continue;
      }
      keep.push_back(f);
    }
    if (cutfaces == 0) throw std::runtime_error("the setback doesn't reach the corner");
    BRep_Builder bb;
    TopoDS_Compound kept;
    bb.MakeCompound(kept);
    for (const auto &f : keep) bb.Add(kept, f);
    TopTools_IndexedDataMapOfShapeListOfShape ef;
    TopExp::MapShapesAndAncestors(kept, TopAbs_EDGE, TopAbs_FACE, ef);
    BRepFill_Filling fill;
    int n = 0;
    for (int k = 1; k <= ef.Extent(); ++k) {
      const TopoDS_Edge &e = TopoDS::Edge(ef.FindKey(k));
      if (BRep_Tool::Degenerated(e) || ef(k).Extent() != 1) continue;
      fill.Add(e, TopoDS::Face(ef(k).First()), GeomAbs_G1, Standard_True);
      ++n;
    }
    if (n < 3) throw std::runtime_error("the corner's boundary is not closed");
    fill.Build();
    if (!fill.IsDone()) throw std::runtime_error("the corner patch could not be computed");
    BRepBuilderAPI_Sewing sew(1e-3);
    for (const auto &f : keep) sew.Add(f);
    sew.Add(fill.Face());
    sew.Perform();
    TopoDS_Shape sewn = sew.SewedShape();
    TopoDS_Shell shell;
    int shells = 0;
    for (TopExp_Explorer ex(sewn, TopAbs_SHELL); ex.More(); ex.Next(), ++shells) shell = TopoDS::Shell(ex.Current());
    if (shells != 1) throw std::runtime_error("the corner patch does not close the body");
    BRepBuilderAPI_MakeSolid ms(shell);
    if (!ms.IsDone()) throw std::runtime_error("the corner patch does not close the body");
    ShapeFix_Solid fs(ms.Solid());
    fs.Perform();
    TopoDS_Shape solid = fs.Solid();
    if (volume(solid) < 0) solid.Reverse();
    cur = valid(solid, "setback");
  }
  return out(cur);
  FOX2_CATCH("setback fillet")
}

} // namespace fox
