//! Vector output of drawings: pen strokes, fills and text in sheet millimetres (origin at the
//! bottom-left, y up), written as SVG, PDF or DXF.

use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pen {
  /// Visible edges (thick).
  Visible,
  /// Hidden edges (dashed).
  Hidden,
  /// Dimensions, hatching, tangent edges.
  Thin,
  /// Centre lines and bend lines (chain).
  Center,
  /// Sheet border and title block.
  Border,
  /// Datum target lines and alternate positions (long, short, short).
  Phantom,
}

impl Pen {
  fn width(self) -> f64 {
    match self {
      Pen::Visible => 0.5,
      Pen::Border => 0.7,
      Pen::Hidden => 0.3,
      Pen::Thin | Pen::Center | Pen::Phantom => 0.18,
    }
  }

  /// Dash pattern in mm (empty = solid).
  fn dash(self) -> &'static [f64] {
    match self {
      Pen::Hidden => &[3.0, 1.5],
      Pen::Center => &[8.0, 1.5, 1.5, 1.5],
      Pen::Phantom => &[8.0, 1.5, 1.5, 1.5, 1.5, 1.5],
      _ => &[],
    }
  }

  fn layer(self) -> &'static str {
    match self {
      Pen::Visible => "VISIBLE",
      Pen::Hidden => "HIDDEN",
      Pen::Thin => "THIN",
      Pen::Center => "CENTER",
      Pen::Border => "BORDER",
      Pen::Phantom => "PHANTOM",
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
  Start,
  Middle,
  End,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
  Line {
    pen: Pen,
    points: Vec<[f64; 2]>,
  },
  /// A filled polygon (arrowheads).
  Fill {
    points: Vec<[f64; 2]>,
  },
  Text {
    at: [f64; 2],
    size: f64,
    text: String,
    anchor: Anchor,
  },
  /// Centred text turned `angle` degrees (dimension values).
  RotatedText {
    at: [f64; 2],
    size: f64,
    text: String,
    angle: f64,
  },
}

#[derive(Debug, Clone, Default)]
pub struct Plot {
  /// mm
  pub width: f64,
  pub height: f64,
  pub items: Vec<Item>,
}

impl Plot {
  /// Text items (for tests and checks).
  pub fn texts(&self) -> Vec<&str> {
    self
      .items
      .iter()
      .filter_map(|i| match i {
        Item::Text { text, .. } | Item::RotatedText { text, .. } => Some(text.as_str()),
        _ => None,
      })
      .collect()
  }

  pub fn svg(&self) -> String {
    let (w, h) = (self.width, self.height);
    let mut s = format!(
      "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n\
       <rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>\n<g fill=\"none\" stroke=\"black\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n"
    );
    let y = |v: f64| h - v;
    for it in &self.items {
      match it {
        Item::Line { pen, points } if points.len() >= 2 => {
          let pts: Vec<String> = points.iter().map(|p| format!("{:.3},{:.3}", p[0], y(p[1]))).collect();
          let dash = pen.dash();
          let dash = if dash.is_empty() {
            String::new()
          } else {
            format!(" stroke-dasharray=\"{}\"", dash.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(","))
          };
          let _ = writeln!(s, "<polyline points=\"{}\" stroke-width=\"{}\"{dash}/>", pts.join(" "), pen.width());
        }
        Item::Fill { points } => {
          let pts: Vec<String> = points.iter().map(|p| format!("{:.3},{:.3}", p[0], y(p[1]))).collect();
          let _ = writeln!(s, "<polygon points=\"{}\" fill=\"black\" stroke=\"none\"/>", pts.join(" "));
        }
        Item::Text { at, size, text, anchor } => {
          let a = match anchor {
            Anchor::Start => "start",
            Anchor::Middle => "middle",
            Anchor::End => "end",
          };
          let _ = writeln!(
            s,
            "<text x=\"{:.3}\" y=\"{:.3}\" font-family=\"Arial, Helvetica, sans-serif\" font-size=\"{size}\" text-anchor=\"{a}\" fill=\"black\" stroke=\"none\">{}</text>",
            at[0],
            y(at[1]),
            xml(text)
          );
        }
        Item::RotatedText { at, size, text, angle } => {
          let _ = writeln!(
            s,
            "<text transform=\"translate({:.3},{:.3}) rotate({:.3})\" font-family=\"Arial, Helvetica, sans-serif\" font-size=\"{size}\" text-anchor=\"middle\" fill=\"black\" stroke=\"none\">{}</text>",
            at[0],
            y(at[1]),
            -angle,
            xml(text)
          );
        }
        _ => {}
      }
    }
    s.push_str("</g>\n</svg>\n");
    s
  }

  /// A one-page PDF (vector, Helvetica text).
  pub fn pdf(&self) -> Vec<u8> {
    pdf_pages(std::slice::from_ref(self))
  }

  /// The PDF content stream of the page.
  fn pdf_content(&self) -> String {
    let k = 72.0 / 25.4;
    let mut c = String::new();
    c.push_str("1 J 1 j\n");
    for it in &self.items {
      match it {
        Item::Line { pen, points } if points.len() >= 2 => {
          let dash: Vec<String> = pen.dash().iter().map(|d| format!("{:.2}", d * k)).collect();
          let _ = writeln!(c, "{:.3} w [{}] 0 d", pen.width() * k, dash.join(" "));
          for (i, p) in points.iter().enumerate() {
            let _ = writeln!(c, "{:.3} {:.3} {}", p[0] * k, p[1] * k, if i == 0 { "m" } else { "l" });
          }
          c.push_str("S\n");
        }
        Item::Fill { points } if points.len() >= 3 => {
          for (i, p) in points.iter().enumerate() {
            let _ = writeln!(c, "{:.3} {:.3} {}", p[0] * k, p[1] * k, if i == 0 { "m" } else { "l" });
          }
          c.push_str("h f\n");
        }
        Item::Text { at, size, text, anchor } => {
          let width = text_width(text, *size);
          let dx = match anchor {
            Anchor::Start => 0.0,
            Anchor::Middle => -width / 2.0,
            Anchor::End => -width,
          };
          let _ = writeln!(
            c,
            "BT /F1 {:.2} Tf {:.3} {:.3} Td ({}) Tj ET",
            size * k,
            (at[0] + dx) * k,
            at[1] * k,
            pdf_text(text)
          );
        }
        Item::RotatedText { at, size, text, angle } => {
          let (sn, cs) = angle.to_radians().sin_cos();
          let dx = -text_width(text, *size) / 2.0;
          let (x, y) = (at[0] + cs * dx, at[1] + sn * dx);
          let _ = writeln!(
            c,
            "BT /F1 {:.2} Tf {cs:.5} {sn:.5} {:.5} {cs:.5} {:.3} {:.3} Tm ({}) Tj ET",
            size * k,
            -sn,
            x * k,
            y * k,
            pdf_text(text)
          );
        }
        _ => {}
      }
    }
    c
  }

  /// DXF (R12, mm): one layer per pen, text as TEXT entities.
  pub fn dxf(&self) -> String {
    let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n");
    s.push_str("0\nSECTION\n2\nENTITIES\n");
    let line = |s: &mut String, layer: &str, a: [f64; 2], b: [f64; 2]| {
      let _ = write!(s, "0\nLINE\n8\n{layer}\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n", a[0], a[1], b[0], b[1]);
    };
    for it in &self.items {
      match it {
        Item::Line { pen, points } => {
          for w in points.windows(2) {
            line(&mut s, pen.layer(), w[0], w[1]);
          }
        }
        Item::Fill { points } => {
          for i in 0..points.len() {
            line(&mut s, "THIN", points[i], points[(i + 1) % points.len()]);
          }
        }
        Item::Text { at, size, text, anchor } => {
          let h = match anchor {
            Anchor::Start => 0,
            Anchor::Middle => 1,
            Anchor::End => 2,
          };
          let _ = write!(
            s,
            "0\nTEXT\n8\nTEXT\n10\n{}\n20\n{}\n30\n0\n40\n{size}\n1\n{}\n72\n{h}\n11\n{}\n21\n{}\n31\n0\n",
            at[0],
            at[1],
            dxf_text(text),
            at[0],
            at[1]
          );
        }
        Item::RotatedText { at, size, text, angle } => {
          let _ = write!(
            s,
            "0\nTEXT\n8\nTEXT\n10\n{}\n20\n{}\n30\n0\n40\n{size}\n1\n{}\n50\n{angle}\n72\n1\n11\n{}\n21\n{}\n31\n0\n",
            at[0],
            at[1],
            dxf_text(text),
            at[0],
            at[1]
          );
        }
      }
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
  }
}

/// A PDF with one page per plot (each its own size), sharing one font.
pub fn pdf_pages(plots: &[Plot]) -> Vec<u8> {
  let k = 72.0 / 25.4;
  // 1 catalog, 2 pages, 3 font, then a page and its contents per plot
  let page = |i: usize| 4 + 2 * i;
  let kids: Vec<String> = (0..plots.len()).map(|i| format!("{} 0 R", page(i))).collect();
  let mut objs = vec![
    "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
    format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), plots.len()),
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
  ];
  for (i, p) in plots.iter().enumerate() {
    let c = p.pdf_content();
    objs.push(format!(
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
      p.width * k,
      p.height * k,
      page(i) + 1
    ));
    objs.push(format!("<< /Length {} >>\nstream\n{c}endstream", c.len()));
  }
  let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
  let mut offsets = Vec::new();
  for (i, o) in objs.iter().enumerate() {
    offsets.push(out.len());
    out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
  }
  let xref = out.len();
  let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1);
  for o in offsets {
    let _ = writeln!(tail, "{o:010} 00000 n ");
  }
  let _ = write!(tail, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1);
  out.extend_from_slice(tail.as_bytes());
  out
}

/// Gap (mm) between the sheets of a multi-sheet DXF.
pub const SHEET_GAP: f64 = 20.0;

/// One DXF with the sheets side by side, left to right, [`SHEET_GAP`] apart.
pub fn dxf_sheets(plots: &[Plot]) -> String {
  let mut all = Plot::default();
  let mut x = 0.0;
  for p in plots {
    all.items.extend(p.items.iter().map(|it| it.moved(x)));
    all.width = x + p.width;
    all.height = all.height.max(p.height);
    x += p.width + SHEET_GAP;
  }
  all.dxf()
}

impl Item {
  /// The item moved `dx` mm to the right.
  fn moved(&self, dx: f64) -> Item {
    let mv = |p: &[f64; 2]| [p[0] + dx, p[1]];
    match self {
      Item::Line { pen, points } => Item::Line { pen: *pen, points: points.iter().map(mv).collect() },
      Item::Fill { points } => Item::Fill { points: points.iter().map(mv).collect() },
      Item::Text { at, size, text, anchor } => {
        Item::Text { at: mv(at), size: *size, text: text.clone(), anchor: *anchor }
      }
      Item::RotatedText { at, size, text, angle } => {
        Item::RotatedText { at: mv(at), size: *size, text: text.clone(), angle: *angle }
      }
    }
  }
}

/// Approximate Helvetica text width (mm).
pub fn text_width(text: &str, size: f64) -> f64 {
  text.chars().count() as f64 * size * 0.52
}

fn xml(s: &str) -> String {
  s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A PDF literal string in WinAnsi (Ø, °, × and · map to their single bytes).
fn pdf_text(s: &str) -> String {
  let mut out = String::new();
  for ch in s.chars() {
    match ch {
      '(' | ')' | '\\' => {
        out.push('\\');
        out.push(ch);
      }
      'Ø' => out.push_str("\\330"),
      '°' => out.push_str("\\260"),
      '×' => out.push_str("\\327"),
      '·' => out.push_str("\\267"),
      c if c.is_ascii() => out.push(c),
      _ => out.push('?'),
    }
  }
  out
}

fn dxf_text(s: &str) -> String {
  s.replace('Ø', "%%c").replace('°', "%%d").replace('×', "x").replace('·', "-")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn writers_produce_their_formats() {
    let p = Plot {
      width: 297.0,
      height: 210.0,
      items: vec![
        Item::Line { pen: Pen::Hidden, points: vec![[10.0, 10.0], [50.0, 10.0]] },
        Item::Fill { points: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]] },
        Item::Text { at: [20.0, 20.0], size: 3.5, text: "Ø10 (x)".into(), anchor: Anchor::Middle },
      ],
    };
    let svg = p.svg();
    assert!(svg.contains("stroke-dasharray") && svg.contains("Ø10"));
    let pdf = p.pdf();
    let text = String::from_utf8_lossy(&pdf);
    assert!(
      text.starts_with("%PDF-1.4") && text.contains("\\330") && text.contains("\\(x\\)") && text.ends_with("%%EOF\n")
    );
    // xref offsets point at the objects
    let xref = text.find("xref").unwrap();
    assert!(text.contains(&format!("startxref\n{xref}")));
    assert!(p.dxf().contains("%%c10"));
  }

  #[test]
  fn several_sheets() {
    let a = Plot {
      width: 297.0,
      height: 210.0,
      items: vec![Item::Line { pen: Pen::Thin, points: vec![[0.0, 0.0], [5.0, 0.0]] }],
    };
    let b = Plot {
      width: 420.0,
      height: 297.0,
      items: vec![Item::Line { pen: Pen::Thin, points: vec![[1.0, 2.0], [3.0, 4.0]] }],
    };
    let pdf = pdf_pages(&[a.clone(), b.clone()]);
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Kids [4 0 R 6 0 R] /Count 2"), "{text}");
    assert_eq!(text.matches("/Type /Page /Parent").count(), 2);
    // each page keeps its own size (A4 and A3 landscape, in points)
    assert!(text.contains("/MediaBox [0 0 841.89 595.28]") && text.contains("/MediaBox [0 0 1190.55 841.89]"));
    // every object the xref lists is where it says
    let xref = text.find("xref").unwrap();
    let offsets: Vec<usize> = text[xref..].lines().skip(3).take(7).map(|l| l[..10].parse().unwrap()).collect();
    for (i, o) in offsets.iter().enumerate() {
      assert!(text[*o..].starts_with(&format!("{} 0 obj", i + 1)), "object {}", i + 1);
    }
    // one page is what Plot::pdf writes
    assert!(String::from_utf8_lossy(&a.pdf()).contains("/Count 1"));
    // the DXF puts the second sheet right of the first, a gap apart
    let dxf = dxf_sheets(&[a, b]);
    assert!(dxf.contains(&format!("10\n{}\n20\n2\n", 297.0 + SHEET_GAP + 1.0)), "{dxf}");
  }
}
