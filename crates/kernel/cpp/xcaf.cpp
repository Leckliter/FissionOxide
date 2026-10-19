// STEP assemblies: components as sub-assemblies, bodies as named parts with surface colours, read
// and written through an XCAF document (STEPCAFControl). The document is a bare TDocStd_Document
// with the XCAF tools attached, so no application framework or visualisation is linked in.
#include "fox-kernel/cpp/xcaf.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <stdexcept>
#include <string>
#include <vector>

#include <IFSelect_ReturnStatus.hxx>
#include <Interface_Static.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <Quantity_Color.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <STEPControl_StepModelType.hxx>
#include <Standard_Failure.hxx>
#include <TCollection_ExtendedString.hxx>
#include <TDF_Label.hxx>
#include <TDF_LabelSequence.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Shape.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <XCAFDoc_ColorType.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>

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

void quiet() {
  static bool done = false;
  if (!done) {
    done = true;
    Message::DefaultMessenger()->RemovePrinters(STANDARD_TYPE(Message_PrinterOStream));
  }
}

std::string str(rust::Str s) { return std::string(s.data(), s.size()); }

TCollection_ExtendedString ext(const std::string &s) { return TCollection_ExtendedString(s.c_str(), Standard_True); }

std::string name_of(const TDF_Label &l) {
  Handle(TDataStd_Name) n;
  if (l.IsNull() || !l.FindAttribute(TDataStd_Name::GetID(), n)) return "";
  const TCollection_ExtendedString &e = n->Get();
  std::string buf(size_t(e.LengthOfCString()) + 1, '\0');
  Standard_PCharacter p = &buf[0];
  int len = e.ToUTF8CString(p);
  buf.resize(len > 0 ? size_t(len) : 0);
  return buf;
}

struct Part {
  TopoDS_Shape shape;
  std::string name, component;
  bool has_color = false;
  double rgb[3] = {0, 0, 0};
};

// A colour on a label: surface colour first, then the generic one.
bool label_color(const Handle(XCAFDoc_ColorTool) &ct, const TDF_Label &l, Quantity_Color &c) {
  return !l.IsNull() && (ct->GetColor(l, XCAFDoc_ColorSurf, c) || ct->GetColor(l, XCAFDoc_ColorGen, c));
}

} // namespace

struct StepDoc::Impl {
  Handle(TDocStd_Document) doc;
  Handle(XCAFDoc_ShapeTool) shapes;
  Handle(XCAFDoc_ColorTool) colors;
  TDF_Label root;
  // sub-assemblies by name, in the order they were first used
  std::vector<std::pair<std::string, TDF_Label>> components;
  std::vector<Part> parts;
};

StepDoc::StepDoc() : impl(new Impl) {}
StepDoc::~StepDoc() = default;

namespace {

void init_doc(StepDoc::Impl &d) {
  d.doc = new TDocStd_Document("BinXCAF");
  XCAFDoc_DocumentTool::Set(d.doc->Main(), Standard_True);
  d.shapes = XCAFDoc_DocumentTool::ShapeTool(d.doc->Main());
  d.colors = XCAFDoc_DocumentTool::ColorTool(d.doc->Main());
}

// Collects the simple shapes under `l` (an instance or a free shape), placed by `loc`. Children
// of the top assembly that are assemblies themselves name the component of everything below them.
void walk(StepDoc::Impl &d, const TDF_Label &l, const TopLoc_Location &loc, int depth, const std::string &component,
          bool has_color, const Quantity_Color &inherited) {
  TDF_Label proto = l;
  TopLoc_Location here = loc;
  if (XCAFDoc_ShapeTool::IsReference(l)) {
    XCAFDoc_ShapeTool::GetReferredShape(l, proto);
    here = loc * XCAFDoc_ShapeTool::GetLocation(l);
  }
  Quantity_Color c = inherited;
  bool colored = has_color;
  Quantity_Color own;
  if (label_color(d.colors, l, own) || label_color(d.colors, proto, own)) {
    c = own;
    colored = true;
  }
  std::string name = name_of(proto);
  if (name.empty()) name = name_of(l);
  if (XCAFDoc_ShapeTool::IsAssembly(proto)) {
    TDF_LabelSequence kids;
    XCAFDoc_ShapeTool::GetComponents(proto, kids, Standard_False);
    for (Standard_Integer i = 1; i <= kids.Length(); i++) {
      const TDF_Label &k = kids.Value(i);
      std::string comp = component;
      if (depth == 0) {
        TDF_Label kp = k;
        if (XCAFDoc_ShapeTool::IsReference(k)) XCAFDoc_ShapeTool::GetReferredShape(k, kp);
        if (XCAFDoc_ShapeTool::IsAssembly(kp)) {
          comp = name_of(k);
          if (comp.empty()) comp = name_of(kp);
          if (comp.empty()) comp = "Component " + std::to_string(i);
        }
      }
      walk(d, k, here, depth + 1, comp, colored, c);
    }
    return;
  }
  TopoDS_Shape s = XCAFDoc_ShapeTool::GetShape(proto);
  if (s.IsNull()) return;
  s.Move(here);
  Part p;
  p.shape = s;
  p.name = name;
  p.component = component;
  if (!colored) {
    // colours often sit on the faces (sub-shapes) only: take the first one
    TDF_LabelSequence subs;
    XCAFDoc_ShapeTool::GetSubShapes(proto, subs);
    for (Standard_Integer i = 1; i <= subs.Length() && !colored; i++) {
      if (label_color(d.colors, subs.Value(i), own)) {
        c = own;
        colored = true;
      }
    }
  }
  if (colored) {
    p.has_color = true;
    c.Values(p.rgb[0], p.rgb[1], p.rgb[2], Quantity_TOC_sRGB);
  }
  d.parts.push_back(p);
}

} // namespace

std::unique_ptr<StepDoc> step_doc_new(rust::Str name) {
  try {
    auto d = std::make_unique<StepDoc>();
    init_doc(*d->impl);
    d->impl->root = d->impl->shapes->NewShape();
    TDataStd_Name::Set(d->impl->root, ext(str(name)));
    return d;
  } catch (...) {
    fail("STEP export");
  }
}

void step_doc_add(StepDoc &d, rust::Str component, rust::Str name, const Shape &s, rust::Slice<const double> color) {
  try {
    StepDoc::Impl &m = *d.impl;
    TDF_Label parent = m.root;
    std::string comp = str(component);
    if (!comp.empty()) {
      auto it = m.components.begin();
      while (it != m.components.end() && it->first != comp) ++it;
      if (it == m.components.end()) {
        TDF_Label a = m.shapes->NewShape();
        TDataStd_Name::Set(a, ext(comp));
        m.components.emplace_back(comp, a);
        it = m.components.end() - 1;
      }
      parent = it->second;
    }
    TDF_Label part = m.shapes->AddShape(s.s, Standard_False);
    TDataStd_Name::Set(part, ext(str(name)));
    if (color.size() == 3) {
      Quantity_Color c(color[0], color[1], color[2], Quantity_TOC_sRGB);
      m.colors->SetColor(part, c, XCAFDoc_ColorSurf);
    }
    TDF_Label inst = m.shapes->AddComponent(parent, part, TopLoc_Location());
    TDataStd_Name::Set(inst, ext(str(name)));
  } catch (...) {
    fail("STEP export");
  }
}

void step_doc_write(StepDoc &d, rust::Str path) {
  try {
    quiet();
    StepDoc::Impl &m = *d.impl;
    // components become sub-assemblies of the root, in the order they were first used
    for (auto &kv : m.components) {
      TDF_Label inst = m.shapes->AddComponent(m.root, kv.second, TopLoc_Location());
      TDataStd_Name::Set(inst, ext(kv.first));
    }
    m.components.clear();
    m.shapes->UpdateAssemblies();
    Interface_Static::SetCVal("write.step.unit", "MM");
    Interface_Static::SetCVal("write.step.schema", "AP214IS");
    STEPCAFControl_Writer w;
    w.SetColorMode(Standard_True);
    w.SetNameMode(Standard_True);
    w.SetLayerMode(Standard_False);
    if (!w.Transfer(m.doc, STEPControl_AsIs)) throw std::runtime_error("STEP transfer failed");
    std::string p = str(path);
    if (w.Write(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("could not write " + p);
  } catch (...) {
    fail("STEP export");
  }
}

std::unique_ptr<StepDoc> step_doc_read(rust::Str path) {
  try {
    quiet();
    auto d = std::make_unique<StepDoc>();
    StepDoc::Impl &m = *d->impl;
    init_doc(m);
    STEPCAFControl_Reader r;
    r.SetColorMode(Standard_True);
    r.SetNameMode(Standard_True);
    r.SetLayerMode(Standard_False);
    std::string p = str(path);
    if (r.ReadFile(p.c_str()) != IFSelect_RetDone) throw std::runtime_error("could not read " + p);
    if (!r.Transfer(m.doc)) throw std::runtime_error("STEP transfer failed");
    TDF_LabelSequence roots;
    m.shapes->GetFreeShapes(roots);
    Quantity_Color none;
    if (roots.Length() == 1) {
      walk(m, roots.Value(1), TopLoc_Location(), 0, "", false, none);
    } else {
      // several free shapes: as if under one root assembly
      for (Standard_Integer i = 1; i <= roots.Length(); i++) {
        const TDF_Label &l = roots.Value(i);
        std::string comp;
        if (XCAFDoc_ShapeTool::IsAssembly(l)) {
          comp = name_of(l);
          if (comp.empty()) comp = "Component " + std::to_string(i);
        }
        walk(m, l, TopLoc_Location(), 1, comp, false, none);
      }
    }
    return d;
  } catch (...) {
    fail("STEP import");
  }
}

size_t step_doc_len(const StepDoc &d) { return d.impl->parts.size(); }

rust::String step_doc_name(const StepDoc &d, size_t i) {
  return i < d.impl->parts.size() ? rust::String(d.impl->parts[i].name) : rust::String();
}

rust::String step_doc_component(const StepDoc &d, size_t i) {
  return i < d.impl->parts.size() ? rust::String(d.impl->parts[i].component) : rust::String();
}

rust::Vec<double> step_doc_color(const StepDoc &d, size_t i) {
  rust::Vec<double> v;
  if (i < d.impl->parts.size() && d.impl->parts[i].has_color) {
    for (double c : d.impl->parts[i].rgb) v.push_back(c);
  }
  return v;
}

std::unique_ptr<Shape> step_doc_shape(const StepDoc &d, size_t i) {
  try {
    if (i >= d.impl->parts.size()) throw std::runtime_error("no such part");
    auto out = std::make_unique<Shape>();
    out->s = d.impl->parts[i].shape;
    return out;
  } catch (...) {
    fail("STEP import");
  }
}

} // namespace fox
