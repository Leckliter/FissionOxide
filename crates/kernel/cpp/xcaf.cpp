// STEP assemblies through an XCAF document (STEPCAFControl): nested sub-assemblies, parts placed
// more than once (instances of one prototype), names, and colours on parts and single faces. The
// document is a bare TDocStd_Document with the XCAF tools attached, so no application framework
// or visualisation is linked in.
#include "fox-kernel/cpp/xcaf.hxx"
#include "fox-kernel/src/ffi.rs.h"

#include <array>
#include <map>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepBuilderAPI_Copy.hxx>
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
#include <TCollection_AsciiString.hxx>
#include <TCollection_ExtendedString.hxx>
#include <TDF_Label.hxx>
#include <TDF_LabelSequence.hxx>
#include <TDF_Tool.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shape.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <XCAFDoc_ColorType.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
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

using Rgb = std::array<double, 3>;

// One occurrence of a part or sub-assembly, placed in the file's world.
struct Node {
  int64_t parent = -1;
  bool assembly = false;
  std::string name, instance;
  size_t proto = 0;
  size_t uses = 0;
  TopLoc_Location loc;
  bool has_color = false;
  Rgb rgb = {0, 0, 0};
  TopoDS_Shape shape;
};

// A colour on a label: surface colour first, then the generic one.
bool label_color(const Handle(XCAFDoc_ColorTool) &ct, const TDF_Label &l, Quantity_Color &c) {
  return !l.IsNull() && (ct->GetColor(l, XCAFDoc_ColorSurf, c) || ct->GetColor(l, XCAFDoc_ColorGen, c));
}

Rgb rgb_of(const Quantity_Color &c) {
  Rgb out;
  c.Values(out[0], out[1], out[2], Quantity_TOC_sRGB);
  return out;
}

} // namespace

struct StepDoc::Impl {
  Handle(TDocStd_Document) doc;
  Handle(XCAFDoc_ShapeTool) shapes;
  Handle(XCAFDoc_ColorTool) colors;
  // writing: the labels handed out (0 = the top assembly), and where each part's shape sits
  // relative to its label's prototype
  std::vector<TDF_Label> labels;
  std::vector<TopLoc_Location> inner;
  // reading: the occurrences, and per prototype the faces that carry their own colour
  std::vector<Node> nodes;
  std::map<std::string, size_t> protos;
  std::vector<std::vector<std::pair<TopoDS_Shape, Rgb>>> face_colors;
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

TDF_Label label_at(StepDoc::Impl &m, size_t i) {
  if (i >= m.labels.size()) throw std::runtime_error("no such assembly or part");
  return m.labels[i];
}

// The number a prototype label goes by; the first time it is seen, also the colours on its faces.
size_t proto_index(StepDoc::Impl &d, const TDF_Label &proto) {
  TCollection_AsciiString entry;
  TDF_Tool::Entry(proto, entry);
  std::string key(entry.ToCString());
  auto it = d.protos.find(key);
  if (it != d.protos.end()) return it->second;
  size_t i = d.face_colors.size();
  d.protos.emplace(key, i);
  std::vector<std::pair<TopoDS_Shape, Rgb>> faces;
  if (!XCAFDoc_ShapeTool::IsAssembly(proto)) {
    // colours on sub-shapes (faces, or shells and solids of a compound) colour their faces
    TDF_LabelSequence subs;
    XCAFDoc_ShapeTool::GetSubShapes(proto, subs);
    for (Standard_Integer k = 1; k <= subs.Length(); k++) {
      Quantity_Color c;
      if (!label_color(d.colors, subs.Value(k), c)) continue;
      TopoDS_Shape sub = XCAFDoc_ShapeTool::GetShape(subs.Value(k));
      for (TopExp_Explorer ex(sub, TopAbs_FACE); ex.More(); ex.Next()) faces.emplace_back(ex.Current(), rgb_of(c));
    }
  }
  d.face_colors.push_back(std::move(faces));
  return i;
}

// Records the occurrence `l` (an instance or a free shape) placed by `loc` and everything below
// it. `skip` leaves the top assembly itself out: its children are the design's top level.
void walk(StepDoc::Impl &d, const TDF_Label &l, const TopLoc_Location &loc, int64_t parent, bool skip, bool has_color,
          const Quantity_Color &inherited) {
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
  Node n;
  n.parent = parent;
  n.assembly = XCAFDoc_ShapeTool::IsAssembly(proto);
  n.name = name_of(proto);
  n.instance = l == proto ? "" : name_of(l);
  n.proto = proto_index(d, proto);
  TDF_LabelSequence users;
  n.uses = size_t(XCAFDoc_ShapeTool::GetUsers(proto, users, Standard_False));
  n.loc = here;
  n.has_color = colored;
  if (colored) n.rgb = rgb_of(c);
  if (n.assembly) {
    int64_t me = parent;
    if (!skip) {
      me = int64_t(d.nodes.size());
      d.nodes.push_back(n);
    }
    TDF_LabelSequence kids;
    XCAFDoc_ShapeTool::GetComponents(proto, kids, Standard_False);
    for (Standard_Integer i = 1; i <= kids.Length(); i++) walk(d, kids.Value(i), here, me, false, colored, c);
    return;
  }
  TopoDS_Shape s = XCAFDoc_ShapeTool::GetShape(proto);
  if (s.IsNull()) return;
  s.Move(here);
  n.shape = s;
  d.nodes.push_back(n);
}

const Node &node_at(const StepDoc &d, size_t i) {
  if (i >= d.impl->nodes.size()) throw std::runtime_error("no such part");
  return d.impl->nodes[i];
}

} // namespace

std::unique_ptr<StepDoc> step_doc_new(rust::Str name) {
  try {
    auto d = std::make_unique<StepDoc>();
    init_doc(*d->impl);
    TDF_Label root = d->impl->shapes->NewShape();
    TDataStd_Name::Set(root, ext(str(name)));
    d->impl->labels.push_back(root);
    d->impl->inner.emplace_back();
    return d;
  } catch (...) {
    fail("STEP export");
  }
}

size_t step_doc_assembly(StepDoc &d, rust::Str name) {
  try {
    StepDoc::Impl &m = *d.impl;
    TDF_Label a = m.shapes->NewShape();
    TDataStd_Name::Set(a, ext(str(name)));
    m.labels.push_back(a);
    m.inner.emplace_back();
    return m.labels.size() - 1;
  } catch (...) {
    fail("STEP export");
  }
}

size_t step_doc_part(StepDoc &d, rust::Str name, const Shape &s, rust::Slice<const double> color,
                     rust::Slice<const double> face_colors) {
  try {
    StepDoc::Impl &m = *d.impl;
    // Every part gets its own prototype, so names and colours never land on another part that
    // shares its geometry; the shape's location goes into where the part is placed.
    TopoDS_Shape shape = s.s;
    TDF_Label found;
    if (m.shapes->FindShape(shape.Located(TopLoc_Location()), found)) {
      BRepBuilderAPI_Copy copy(shape);
      shape = copy.Shape();
    }
    TopoDS_Shape bare = shape.Located(TopLoc_Location());
    TDF_Label proto = m.shapes->AddShape(bare, Standard_False);
    TDataStd_Name::Set(proto, ext(str(name)));
    if (color.size() == 3) {
      Quantity_Color c(color[0], color[1], color[2], Quantity_TOC_sRGB);
      m.colors->SetColor(proto, c, XCAFDoc_ColorSurf);
    }
    if (face_colors.size() >= 4) {
      TopTools_IndexedMapOfShape faces;
      TopExp::MapShapes(bare, TopAbs_FACE, faces);
      for (size_t k = 0; k + 3 < face_colors.size(); k += 4) {
        int id = int(face_colors[k]);
        if (id < 1 || id > faces.Extent()) continue;
        TDF_Label sub = m.shapes->AddSubShape(proto, faces(id));
        if (sub.IsNull() && !m.shapes->FindSubShape(proto, faces(id), sub)) continue;
        Quantity_Color c(face_colors[k + 1], face_colors[k + 2], face_colors[k + 3], Quantity_TOC_sRGB);
        m.colors->SetColor(sub, c, XCAFDoc_ColorSurf);
      }
    }
    m.labels.push_back(proto);
    m.inner.push_back(shape.Location());
    return m.labels.size() - 1;
  } catch (...) {
    fail("STEP export");
  }
}

void step_doc_place(StepDoc &d, size_t parent, size_t child, rust::Str name, rust::Slice<const double> at) {
  try {
    StepDoc::Impl &m = *d.impl;
    TDF_Label p = label_at(m, parent), c = label_at(m, child);
    if (child == 0) throw std::runtime_error("the top assembly can't be placed");
    TopLoc_Location loc;
    if (at.size() == 12) {
      gp_Trsf t;
      t.SetValues(at[0], at[1], at[2], at[3], at[4], at[5], at[6], at[7], at[8], at[9], at[10], at[11]);
      loc = TopLoc_Location(t);
    }
    TDF_Label inst = m.shapes->AddComponent(p, c, loc * m.inner[child]);
    if (inst.IsNull()) throw std::runtime_error("could not place a component");
    TDataStd_Name::Set(inst, ext(str(name)));
  } catch (...) {
    fail("STEP export");
  }
}

void step_doc_write(StepDoc &d, rust::Str path) {
  try {
    quiet();
    StepDoc::Impl &m = *d.impl;
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
    // one top assembly is the design itself; several free shapes sit side by side at the top
    bool one = roots.Length() == 1;
    for (Standard_Integer i = 1; i <= roots.Length(); i++) walk(m, roots.Value(i), TopLoc_Location(), -1, one, false, none);
    return d;
  } catch (...) {
    fail("STEP import");
  }
}

size_t step_doc_len(const StepDoc &d) { return d.impl->nodes.size(); }

int64_t step_doc_parent(const StepDoc &d, size_t i) { return i < d.impl->nodes.size() ? d.impl->nodes[i].parent : -1; }

bool step_doc_is_assembly(const StepDoc &d, size_t i) {
  return i < d.impl->nodes.size() && d.impl->nodes[i].assembly;
}

rust::String step_doc_name(const StepDoc &d, size_t i) {
  return i < d.impl->nodes.size() ? rust::String(d.impl->nodes[i].name) : rust::String();
}

rust::String step_doc_instance(const StepDoc &d, size_t i) {
  return i < d.impl->nodes.size() ? rust::String(d.impl->nodes[i].instance) : rust::String();
}

size_t step_doc_proto(const StepDoc &d, size_t i) { return i < d.impl->nodes.size() ? d.impl->nodes[i].proto : 0; }

size_t step_doc_uses(const StepDoc &d, size_t i) { return i < d.impl->nodes.size() ? d.impl->nodes[i].uses : 0; }

rust::Vec<double> step_doc_placement(const StepDoc &d, size_t i) {
  rust::Vec<double> v;
  if (i < d.impl->nodes.size()) {
    gp_Trsf t = d.impl->nodes[i].loc.Transformation();
    for (int r = 1; r <= 3; r++)
      for (int c = 1; c <= 4; c++) v.push_back(t.Value(r, c));
  }
  return v;
}

rust::Vec<double> step_doc_color(const StepDoc &d, size_t i) {
  rust::Vec<double> v;
  if (i < d.impl->nodes.size() && d.impl->nodes[i].has_color) {
    for (double c : d.impl->nodes[i].rgb) v.push_back(c);
  }
  return v;
}

std::unique_ptr<Shape> step_doc_shape(const StepDoc &d, size_t i) {
  try {
    const Node &n = node_at(d, i);
    if (n.shape.IsNull()) throw std::runtime_error("an assembly has no shape of its own");
    auto out = std::make_unique<Shape>();
    out->s = n.shape;
    return out;
  } catch (...) {
    fail("STEP import");
  }
}

rust::Vec<double> step_doc_face_colors(const StepDoc &d, size_t i, const Shape &body) {
  try {
    rust::Vec<double> v;
    const Node &n = node_at(d, i);
    const auto &coloured = d.impl->face_colors.at(n.proto);
    if (coloured.empty()) return v;
    TopTools_IndexedMapOfShape faces;
    TopExp::MapShapes(body.s, TopAbs_FACE, faces);
    for (int k = 1; k <= faces.Extent(); k++) {
      // the body's faces are the prototype's, placed: the same underlying face
      for (const auto &fc : coloured) {
        if (faces(k).IsPartner(fc.first)) {
          v.push_back(double(k));
          for (double c : fc.second) v.push_back(c);
          break;
        }
      }
    }
    return v;
  } catch (...) {
    fail("STEP import");
  }
}

} // namespace fox
