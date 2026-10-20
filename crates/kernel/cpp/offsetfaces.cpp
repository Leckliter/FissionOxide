// Offset Face as Fusion does it: the chosen faces move by a distance along their normals and every
// other face stays on its surface, growing or shrinking to meet them (a slanted neighbour runs on
// along its slope). This is OCCT's offset algorithm with a zero offset for the solid and the
// distance set on the chosen faces, joining by intersection so neighbours extend rather than round.
#include "fox-kernel/cpp/offsetfaces.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <cmath>
#include <stdexcept>
#include <string>

#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <GProp_GProps.hxx>
#include <ShapeFix_Solid.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>

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

} // namespace

// faces: 1-based face indices of `s`; d > 0 moves them out of the material.
std::unique_ptr<Shape> offset_faces(const Shape &s, rust::Slice<const uint32_t> faces, double d) {
  try {
    catch_signals();
    OCC_CATCH_SIGNALS
    if (std::abs(d) < 1e-9) throw std::runtime_error("the distance must not be zero");
    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(s.s, TopAbs_FACE, map);
    BRepOffset_MakeOffset mo;
    mo.Initialize(s.s, 0.0, 1e-6, BRepOffset_Skin, Standard_False, Standard_False, GeomAbs_Intersection,
                  Standard_False);
    int n = 0;
    for (uint32_t i : faces) {
      if (i < 1 || static_cast<int>(i) > map.Extent()) continue;
      mo.SetOffsetOnFace(TopoDS::Face(map(static_cast<int>(i))), d);
      ++n;
    }
    if (n == 0) throw std::runtime_error("no faces to offset");
    mo.MakeOffsetShape();
    if (!mo.IsDone()) throw std::runtime_error("the faces could not be offset");
    TopoDS_Shape out = mo.Shape();
    if (out.IsNull()) throw std::runtime_error("the offset made nothing");
    int solids = 0;
    for (TopExp_Explorer ex(out, TopAbs_SOLID); ex.More(); ex.Next()) ++solids;
    if (solids == 0) {
      // the offset of a solid comes back as its closed skin: close it again, facing outward
      TopExp_Explorer ex(out, TopAbs_SHELL);
      if (!ex.More()) throw std::runtime_error("the offset made no solid");
      TopoDS_Shell shell = TopoDS::Shell(ex.Current());
      ex.Next();
      if (ex.More()) throw std::runtime_error("the offset made more than one skin");
      ShapeFix_Solid fix;
      out = fix.SolidFromShell(shell);
    }
    if (!BRepCheck_Analyzer(out).IsValid()) throw std::runtime_error("the offset made an invalid solid");
    GProp_GProps g;
    BRepGProp::VolumeProperties(out, g);
    if (!(g.Mass() > 1e-9)) throw std::runtime_error("the offset made an inside-out solid");
    auto r = std::make_unique<Shape>();
    r->s = out;
    return r;
  } catch (...) {
    fail("offset faces");
  }
}

} // namespace fox
