// Form (T-spline) geometry: bicubic B-spline faces through grids of points on a limit surface.

#include "form.hxx"

#include <BRepBuilderAPI_MakeFace.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Standard_Failure.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColStd_HArray1OfReal.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TopoDS_Face.hxx>
#include <stdexcept>
#include <string>
#include <vector>

namespace fox {

namespace {

// Uniform parameters 0, 1, ..., n-1.
Handle(TColStd_HArray1OfReal) uniform(int n) {
  Handle(TColStd_HArray1OfReal) t = new TColStd_HArray1OfReal(1, n);
  for (int i = 1; i <= n; ++i) t->SetValue(i, i - 1);
  return t;
}

// The interpolating curve through `pts` at `params`. With the same parameters it is linear in
// the points, so every curve shares one knot vector.
Handle(Geom_BSplineCurve) through(const std::vector<gp_Pnt> &pts, const Handle(TColStd_HArray1OfReal) & params) {
  Handle(TColgp_HArray1OfPnt) a = new TColgp_HArray1OfPnt(1, static_cast<int>(pts.size()));
  for (size_t i = 0; i < pts.size(); ++i) a->SetValue(static_cast<int>(i) + 1, pts[i]);
  GeomAPI_Interpolate interp(a, params, Standard_False, 1e-9);
  interp.Perform();
  if (!interp.IsDone()) throw std::runtime_error("could not fit a curve through the surface points");
  return interp.Curve();
}

} // namespace

// A face on the B-spline surface through a grid of points (rows × cols, row by row, 3 doubles
// each) at uniform parameters: u along a row, v down the columns. Its boundary curves depend
// only on the boundary points, so faces built from grids that share a side share that edge.
std::unique_ptr<Shape> grid_face(rust::Slice<const double> points, uint32_t rows, uint32_t cols) {
  try {
    if (rows < 2 || cols < 2 || points.size() != static_cast<size_t>(rows) * cols * 3) {
      throw std::runtime_error("the grid needs at least 2 × 2 points");
    }
    const int nr = static_cast<int>(rows), nc = static_cast<int>(cols);
    Handle(TColStd_HArray1OfReal) up = uniform(nc), vp = uniform(nr);
    // interpolate each row in u, then each column of the rows' poles in v
    std::vector<Handle(Geom_BSplineCurve)> rowCurves;
    for (int r = 0; r < nr; ++r) {
      std::vector<gp_Pnt> pts;
      for (int c = 0; c < nc; ++c) {
        size_t k = 3 * (static_cast<size_t>(r) * nc + c);
        pts.emplace_back(points[k], points[k + 1], points[k + 2]);
      }
      rowCurves.push_back(through(pts, up));
    }
    const Handle(Geom_BSplineCurve) &u0 = rowCurves.front();
    const int npu = u0->NbPoles();
    std::vector<Handle(Geom_BSplineCurve)> colCurves;
    for (int i = 1; i <= npu; ++i) {
      std::vector<gp_Pnt> pts;
      for (const auto &rc : rowCurves) {
        if (rc->NbPoles() != npu) throw std::runtime_error("the rows fit differently");
        pts.push_back(rc->Pole(i));
      }
      colCurves.push_back(through(pts, vp));
    }
    const Handle(Geom_BSplineCurve) &v0 = colCurves.front();
    const int npv = v0->NbPoles();
    TColgp_Array2OfPnt poles(1, npu, 1, npv);
    for (int i = 1; i <= npu; ++i) {
      for (int j = 1; j <= npv; ++j) poles.SetValue(i, j, colCurves[i - 1]->Pole(j));
    }
    TColStd_Array1OfReal uk(1, u0->NbKnots()), vk(1, v0->NbKnots());
    TColStd_Array1OfInteger um(1, u0->NbKnots()), vm(1, v0->NbKnots());
    u0->Knots(uk);
    u0->Multiplicities(um);
    v0->Knots(vk);
    v0->Multiplicities(vm);
    Handle(Geom_BSplineSurface) s = new Geom_BSplineSurface(poles, uk, vk, um, vm, u0->Degree(), v0->Degree());
    BRepBuilderAPI_MakeFace mf(s, 1e-7);
    if (!mf.IsDone()) throw std::runtime_error("could not make the face");
    auto out = std::make_unique<Shape>();
    out->s = mf.Face();
    return out;
  } catch (const Standard_Failure &e) {
    const char *msg = e.GetMessageString();
    throw std::runtime_error(std::string("T-spline face: ") + (msg && *msg ? msg : e.DynamicType()->Name()));
  } catch (const std::exception &e) {
    throw std::runtime_error(std::string("T-spline face: ") + e.what());
  } catch (...) {
    throw std::runtime_error("T-spline face: unknown kernel error");
  }
}

} // namespace fox
