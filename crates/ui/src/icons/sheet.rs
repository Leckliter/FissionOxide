//! Renders every icon to a contact sheet PNG for side-by-side review, with egui's tessellator
//! and a small software rasteriser (no GPU). Run with
//! `cargo test -p fox-ui icon_sheet -- --ignored`; the sheet lands in `target/icon-sheet.png`.

use egui::{Rect, epaint::Primitive, pos2, vec2};

use super::{Icon, draw_color};

const ALL: &[Icon] = &[
  Icon::Sketch,
  Icon::Extrude,
  Icon::Revolve,
  Icon::Box,
  Icon::Cylinder,
  Icon::Sphere,
  Icon::Torus,
  Icon::Pattern,
  Icon::CircPattern,
  Icon::Mirror,
  Icon::Fillet,
  Icon::Chamfer,
  Icon::Shell,
  Icon::Combine,
  Icon::Move,
  Icon::Plane,
  Icon::Measure,
  Icon::Import,
  Icon::Remove,
  Icon::Params,
  Icon::Select,
  Icon::Line,
  Icon::Rect,
  Icon::CenterRect,
  Icon::Circle,
  Icon::Arc3,
  Icon::CenterArc,
  Icon::Polygon,
  Icon::Slot,
  Icon::Point,
  Icon::Dimension,
  Icon::Construction,
  Icon::Coincident,
  Icon::Horizontal,
  Icon::Vertical,
  Icon::Parallel,
  Icon::Perpendicular,
  Icon::Tangent,
  Icon::Equal,
  Icon::Concentric,
  Icon::Midpoint,
  Icon::Collinear,
  Icon::Symmetric,
  Icon::Fix,
  Icon::Finish,
  Icon::DataPanel,
  Icon::File,
  Icon::Save,
  Icon::Undo,
  Icon::Redo,
  Icon::Plus,
  Icon::Close,
  Icon::Bell,
  Icon::Help,
  Icon::Folder,
  Icon::Gear,
  Icon::Component,
  Icon::Body,
  Icon::SketchSmall,
  Icon::Eye,
  Icon::EyeOff,
  Icon::Origin,
  Icon::Views,
  Icon::Orbit,
  Icon::LookAt,
  Icon::Pan,
  Icon::Zoom,
  Icon::Fit,
  Icon::Display,
  Icon::Grid,
  Icon::Viewports,
  Icon::ToStart,
  Icon::StepBack,
  Icon::Play,
  Icon::StepForward,
  Icon::ToEnd,
  Icon::PressPull,
  Icon::Draft,
  Icon::Scale,
  Icon::OffsetFace,
  Icon::ReplaceFace,
  Icon::SplitFace,
  Icon::SplitBody,
  Icon::SilhouetteSplit,
  Icon::Align,
  Icon::RemoveBody,
  Icon::PhysMaterial,
  Icon::Appearance,
  Icon::ManageMaterials,
  Icon::ComputeAll,
  Icon::Bom,
  Icon::CreateForm,
  Icon::Derive,
  Icon::Sweep,
  Icon::Loft,
  Icon::Rib,
  Icon::Web,
  Icon::Emboss,
  Icon::Hole,
  Icon::Thread,
  Icon::Coil,
  Icon::Pipe,
  Icon::Thicken,
  Icon::BoundaryFill,
  Icon::BaseFeature,
  Icon::JointOrigin,
  Icon::Configure,
  Icon::ConfigTable,
  Icon::ConfigRules,
  Icon::Ucs,
  Icon::PlaneAlt,
  Icon::Axis,
  Icon::CPoint,
  Icon::Interference,
  Icon::Section,
  Icon::CurvComb,
  Icon::CurvMap,
  Icon::DraftAnalysis,
  Icon::EnvMap,
  Icon::Isocurve,
  Icon::Zebra,
  Icon::Accessibility,
  Icon::CenterOfMass,
  Icon::MinRadius,
  Icon::Navigator,
  Icon::ComponentColors,
  Icon::FaceGroups,
  Icon::History,
  Icon::Sustainability,
  Icon::Decal,
  Icon::Canvas,
  Icon::InsertSvg,
  Icon::InsertDxf,
  Icon::InsertMesh,
  Icon::InsertComponent,
  Icon::Fastener,
  Icon::DuplicateJoints,
  Icon::McMaster,
  Icon::Manufacturer,
  Icon::AddToAssembly,
  Icon::WindowSel,
  Icon::FreeformSel,
  Icon::PaintSel,
  Icon::Patch,
  Icon::Ruled,
  Icon::SurfOffset,
  Icon::Trim,
  Icon::Untrim,
  Icon::Extend,
  Icon::Stitch,
  Icon::Unstitch,
  Icon::ReverseNormal,
  Icon::Tessellate,
  Icon::MeshSection,
  Icon::BaseMesh,
  Icon::Repair,
  Icon::GenFaceGroups,
  Icon::CombineFaceGroups,
  Icon::DirectEdit,
  Icon::Remesh,
  Icon::Reduce,
  Icon::PlaneCut,
  Icon::Smooth,
  Icon::Separate,
  Icon::ScaleMesh,
  Icon::ConvertMesh,
  Icon::Flange,
  Icon::Hem,
  Icon::LoftedFlange,
  Icon::Fold,
  Icon::ConvertSheet,
  Icon::FlatPattern,
  Icon::Unfold,
  Icon::SmRules,
  Icon::CornerClosure,
  Icon::Rip,
  Icon::Refold,
  Icon::DriveJoint,
  Icon::MotionStudy,
  Icon::NewDrawing,
  Icon::BaseView,
  Icon::ProjectedView,
  Icon::SectionView,
  Icon::DetailView,
  Icon::PartsList,
  Icon::TitleBlock,
  Icon::SheetSettings,
  Icon::ExportPdf,
  Icon::ExportDxf,
  Icon::ExportSvg,
  Icon::Print3d,
  Icon::Scripts,
  Icon::Marketplace,
  Icon::Assistant,
  Icon::Gauge,
  Icon::Clock,
  Icon::MidpointLine,
  Icon::Rect3,
  Icon::Circle2P,
  Icon::Circle3P,
  Icon::Circle2T,
  Icon::Circle3T,
  Icon::TangentArc,
  Icon::PolygonCirc,
  Icon::PolygonEdge,
  Icon::Ellipse,
  Icon::OverallSlot,
  Icon::CenterPointSlot,
  Icon::ArcSlot3,
  Icon::ArcSlotCenter,
  Icon::SplineFit,
  Icon::SplineCtrl,
  Icon::Conic,
  Icon::SkText,
  Icon::TraceImage,
  Icon::SkMirror,
  Icon::SkCircPattern,
  Icon::SkRectPattern,
  Icon::Project,
  Icon::Intersect,
  Icon::Include3D,
  Icon::ProjectSurface,
  Icon::SkFillet,
  Icon::SkChamfer,
  Icon::BlendCurve,
  Icon::SkOffset,
  Icon::SkTrim,
  Icon::SkExtend,
  Icon::SkBreak,
  Icon::SkScale,
  Icon::HorizVert,
  Icon::Curvature,
  Icon::PolygonCon,
  Icon::FinishAuto,
  Icon::BreakView,
  Icon::OrdinateDim,
  Icon::AngleDim,
  Icon::FeatureControl,
  Icon::DatumId,
  Icon::SurfaceTexture,
  Icon::Balloon,
  Icon::Properties,
  Icon::SkScaleTo,
  Icon::SketchLibrary,
  Icon::SkRemoveDuplicates,
  Icon::SkCleanup,
  Icon::Centerline,
  Icon::ExtrudeSolid,
  Icon::ExtrudeThin,
  Icon::ProfilePlane,
  Icon::StartOffset,
  Icon::StartObject,
  Icon::OneSide,
  Icon::TwoSides,
  Icon::SymmetricExtent,
  Icon::ExtentDistance,
  Icon::ToObject,
  Icon::ThroughAll,
  Icon::WallSide1,
  Icon::WallSide2,
  Icon::WallCenter,
  Icon::OpJoin,
  Icon::OpCut,
  Icon::OpIntersect,
  Icon::OpNewBody,
  Icon::OpNewComponent,
  Icon::TextBox,
  Icon::TextOnPath,
  Icon::FilterBodies,
  Icon::FilterFaces,
  Icon::Bold,
  Icon::Italic,
  Icon::FlipH,
  Icon::FlipV,
  Icon::AlignLeft,
  Icon::AlignCenter,
  Icon::AlignRight,
  Icon::AlignTop,
  Icon::AlignMiddle,
  Icon::AlignBottom,
  Icon::DistFull,
  Icon::DistAngle,
  Icon::DistSymmetric,
  Icon::DistExtent,
  Icon::DistSpacing,
  Icon::DirOne,
  Icon::DirSymmetric,
  Icon::GridSettings,
  Icon::SetIncrements,
  Icon::ZoomWindow,
];

fn render(icons: &[Icon], cell: f32, icon: f32, cols: usize, ppp: f32) -> image::RgbaImage {
  let rows = icons.len().div_ceil(cols);
  let size = vec2(cols as f32 * cell, rows as f32 * cell);
  let ctx = egui::Context::default();
  let mut input = egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)), ..Default::default() };
  input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
  let mut out = ctx.run_ui(input, |ui| {
    let ctx = ui.ctx();
    let p = ctx.layer_painter(egui::LayerId::background());
    p.rect_filled(Rect::from_min_size(pos2(0.0, 0.0), size), 0.0, crate::theme::TOOLBAR);
    for (i, &ic) in icons.iter().enumerate() {
      let c = pos2((i % cols) as f32 * cell + cell / 2.0, (i / cols) as f32 * cell + cell / 2.0);
      draw_color(&p, Rect::from_center_size(c, vec2(icon, icon)), ic);
    }
  });
  let mut tex = None;
  for ds in out.textures_delta.set.values() {
    for d in ds {
      if let (egui::ImageData::Color(img), None) = (&d.image, d.pos) {
        tex = Some(img.clone());
      }
    }
  }
  out.textures_delta.clear();
  let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
  let (w, h) = ((size.x * ppp) as u32, (size.y * ppp) as u32);
  let mut buf = vec![[0f32; 4]; (w * h) as usize];
  for cp in prims {
    let Primitive::Mesh(m) = cp.primitive else { continue };
    for t in m.indices.chunks(3) {
      let v = [&m.vertices[t[0] as usize], &m.vertices[t[1] as usize], &m.vertices[t[2] as usize]];
      let p: Vec<_> = v.iter().map(|v| (v.pos.x * ppp, v.pos.y * ppp)).collect();
      let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
      if area.abs() < 1e-9 {
        continue;
      }
      let x0 = p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor().max(0.0) as u32;
      let x1 = (p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil() as u32).min(w);
      let y0 = p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor().max(0.0) as u32;
      let y1 = (p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as u32).min(h);
      for y in y0..y1 {
        for x in x0..x1 {
          let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
          let b0 = ((p[1].0 - px) * (p[2].1 - py) - (p[2].0 - px) * (p[1].1 - py)) / area;
          let b1 = ((p[2].0 - px) * (p[0].1 - py) - (p[0].0 - px) * (p[2].1 - py)) / area;
          let b2 = 1.0 - b0 - b1;
          if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
            continue;
          }
          let mut s = [0f32; 4];
          for (k, vv) in v.iter().enumerate() {
            let b = [b0, b1, b2][k];
            let c = vv.color.to_array();
            for j in 0..4 {
              s[j] += b * c[j] as f32 / 255.0;
            }
          }
          if let Some(img) = &tex {
            let uv = v[0].uv * b0 + v[1].uv.to_vec2() * b1 + v[2].uv.to_vec2() * b2;
            let tx = ((uv.x * img.size[0] as f32) as usize).min(img.size[0] - 1);
            let ty = ((uv.y * img.size[1] as f32) as usize).min(img.size[1] - 1);
            let tc = img.pixels[ty * img.size[0] + tx].to_array();
            for j in 0..4 {
              s[j] *= tc[j] as f32 / 255.0;
            }
          }
          let d = &mut buf[(y * w + x) as usize];
          for j in 0..4 {
            d[j] = s[j] + d[j] * (1.0 - s[3]);
          }
        }
      }
    }
  }
  image::RgbaImage::from_fn(w, h, |x, y| {
    let c = buf[(y * w + x) as usize];
    image::Rgba([(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255])
  })
}

#[test]
#[ignore = "writes target/icon-sheet.png for review"]
fn icon_sheet() {
  let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
  // toolbar size (27 px) at 1x and 2x, as the app shows them
  render(ALL, 40.0, 27.0, 16, 1.0).save(dir.join("icon-sheet.png")).unwrap();
  render(ALL, 40.0, 27.0, 16, 2.0).save(dir.join("icon-sheet@2x.png")).unwrap();
}
