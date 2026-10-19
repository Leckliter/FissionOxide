// More sheet metal geometry: the smooth (die-formed) surface of a lofted flange.

#include "kernel.hxx"

#include <BRepBuilderAPI_MakeFace.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomFill.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Surface.hxx>
#include <Standard_Failure.hxx>
#include <TColStd_HArray1OfReal.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TopoDS_Face.hxx>
#include <cmath>
#include <stdexcept>
#include <string>

namespace clf {

namespace {

Handle(Geom_BSplineCurve) interpolate(rust::Slice<const double> p, const Handle(TColStd_HArray1OfReal) & params) {
  int n = params->Length();
  Handle(TColgp_HArray1OfPnt) pts = new TColgp_HArray1OfPnt(1, n);
  for (int i = 0; i < n; ++i) pts->SetValue(i + 1, gp_Pnt(p[3 * i], p[3 * i + 1], p[3 * i + 2]));
  GeomAPI_Interpolate interp(pts, params, Standard_False, 1e-7);
  interp.Perform();
  if (!interp.IsDone()) throw std::runtime_error("could not fit a curve through a profile");
  return interp.Curve();
}

} // namespace

// A ruled face between two polylines of the same number of points (3 doubles each): point i of
// one is joined to point i of the other. Both are interpolated at the given parameters (one per
// point, increasing), so faces built from the same points and parameters share their edges.
std::unique_ptr<Shape> ruled_bspline(rust::Slice<const double> a, rust::Slice<const double> b,
                                     rust::Slice<const double> u) {
  try {
    if (a.size() != b.size() || a.size() % 3 != 0 || a.size() < 6 || u.size() * 3 != a.size()) {
      throw std::runtime_error("the profiles need the same number of points");
    }
    int n = static_cast<int>(u.size());
    Handle(TColStd_HArray1OfReal) params = new TColStd_HArray1OfReal(1, n);
    for (int i = 0; i < n; ++i) {
      if (i > 0 && !(u[i] > u[i - 1] + 1e-12)) throw std::runtime_error("the parameters must increase");
      params->SetValue(i + 1, u[i]);
    }
    Handle(Geom_Surface) s = GeomFill::Surface(interpolate(a, params), interpolate(b, params));
    BRepBuilderAPI_MakeFace mf(s, 1e-7);
    if (!mf.IsDone()) throw std::runtime_error("could not make the lofted face");
    auto out = std::make_unique<Shape>();
    out->s = mf.Face();
    return out;
  } catch (const Standard_Failure &e) {
    const char *msg = e.GetMessageString();
    throw std::runtime_error(std::string("lofted flange: ") + (msg && *msg ? msg : e.DynamicType()->Name()));
  } catch (const std::exception &e) {
    throw std::runtime_error(std::string("lofted flange: ") + e.what());
  } catch (...) {
    throw std::runtime_error("lofted flange: unknown kernel error");
  }
}

} // namespace clf
