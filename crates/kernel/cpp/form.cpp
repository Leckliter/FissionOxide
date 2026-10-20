// Form (T-spline / subdivision) bodies: each smooth region of a form's limit surface becomes one
// B-spline face, built here from its poles and knots; the faces are sewn into a solid on the Rust
// side (Shape::sewn).
#include "fox-kernel/cpp/form.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <stdexcept>
#include <string>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Standard_Failure.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <TopoDS_Face.hxx>

namespace fox {

// A non-rational B-spline face: `poles` holds nu x nv points (x, y, z), u-major (pole (i, j) at
// (i * nv + j) * 3); the knots are distinct values with their multiplicities.
std::unique_ptr<Shape> bspline_face(uint32_t degree, uint32_t nu, uint32_t nv, rust::Slice<const double> poles,
                                    rust::Slice<const double> uknots, rust::Slice<const uint32_t> umults,
                                    rust::Slice<const double> vknots, rust::Slice<const uint32_t> vmults) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    if (degree < 1 || degree > 9 || nu < degree + 1 || nv < degree + 1) throw std::runtime_error("too few poles");
    if (poles.size() != size_t(nu) * nv * 3) throw std::runtime_error("wrong number of poles");
    if (uknots.size() < 2 || uknots.size() != umults.size() || vknots.size() < 2 || vknots.size() != vmults.size()) {
      throw std::runtime_error("bad knot vectors");
    }
    TColgp_Array2OfPnt p(1, int(nu), 1, int(nv));
    for (uint32_t i = 0; i < nu; ++i) {
      for (uint32_t j = 0; j < nv; ++j) {
        size_t k = (size_t(i) * nv + j) * 3;
        p.SetValue(int(i) + 1, int(j) + 1, gp_Pnt(poles[k], poles[k + 1], poles[k + 2]));
      }
    }
    auto knots = [](rust::Slice<const double> k, rust::Slice<const uint32_t> m, TColStd_Array1OfReal &ko,
                    TColStd_Array1OfInteger &mo) {
      for (size_t i = 0; i < k.size(); ++i) {
        if (i > 0 && !(k[i] > k[i - 1])) throw std::runtime_error("the knots must increase");
        ko.SetValue(int(i) + 1, k[i]);
        mo.SetValue(int(i) + 1, int(m[i]));
      }
    };
    TColStd_Array1OfReal uk(1, int(uknots.size())), vk(1, int(vknots.size()));
    TColStd_Array1OfInteger um(1, int(umults.size())), vm(1, int(vmults.size()));
    knots(uknots, umults, uk, um);
    knots(vknots, vmults, vk, vm);
    Handle(Geom_BSplineSurface) s = new Geom_BSplineSurface(p, uk, vk, um, vm, int(degree), int(degree));
    BRepBuilderAPI_MakeFace mf(s, 1e-7);
    if (!mf.IsDone()) throw std::runtime_error("could not make the face");
    auto out = std::make_unique<Shape>();
    out->s = mf.Face();
    return out;
  } catch (const Standard_Failure &e) {
    const char *msg = e.GetMessageString();
    throw std::runtime_error(std::string("form surface: ") + (msg && *msg ? msg : e.DynamicType()->Name()));
  } catch (const std::exception &e) {
    throw std::runtime_error(std::string("form surface: ") + e.what());
  } catch (...) {
    throw std::runtime_error("form surface: unknown kernel error");
  }
}

} // namespace fox
