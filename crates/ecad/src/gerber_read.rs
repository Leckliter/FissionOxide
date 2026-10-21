//! Reading Gerber RS-274X and Excellon back: enough of each to check what was written (and to
//! show fab files from elsewhere).

use std::collections::BTreeMap;

use crate::geom::P;

#[derive(Debug, Clone, PartialEq)]
pub enum Aperture {
  Circle(f64),
  Rect(f64, f64),
  Obround(f64, f64),
  Polygon(f64, u32),
  Macro(String),
}

/// What a Gerber file draws.
#[derive(Debug, Clone, Default)]
pub struct GerberImage {
  pub metric: bool,
  pub apertures: BTreeMap<u32, Aperture>,
  /// (aperture, position, dark)
  pub flashes: Vec<(u32, P, bool)>,
  /// (aperture, from, to, dark)
  pub draws: Vec<(u32, P, P, bool)>,
  /// (contour, dark)
  pub regions: Vec<(Vec<P>, bool)>,
  /// File attributes (%TF...%), without the TF prefix.
  pub attributes: Vec<String>,
}

impl GerberImage {
  /// The extent of everything dark (apertures counted by their size).
  pub fn bounds(&self) -> Option<(P, P)> {
    let size = |a: u32| match self.apertures.get(&a) {
      Some(Aperture::Circle(d)) | Some(Aperture::Polygon(d, _)) => *d / 2.0,
      Some(Aperture::Rect(w, h)) | Some(Aperture::Obround(w, h)) => w.max(*h) / 2.0,
      _ => 0.0,
    };
    let mut pts = Vec::new();
    for (a, p, dark) in &self.flashes {
      if *dark {
        let r = size(*a);
        pts.extend([[p[0] - r, p[1] - r], [p[0] + r, p[1] + r]]);
      }
    }
    for (a, p, q, dark) in &self.draws {
      if *dark {
        let r = size(*a);
        for p in [p, q] {
          pts.extend([[p[0] - r, p[1] - r], [p[0] + r, p[1] + r]]);
        }
      }
    }
    for (c, dark) in &self.regions {
      if *dark {
        pts.extend(c.iter().copied());
      }
    }
    crate::geom::bounds(pts)
  }
}

fn num(s: &str) -> Result<f64, String> {
  s.trim().parse::<f64>().map_err(|_| format!("bad number '{s}'"))
}

fn aperture(def: &str) -> Result<(u32, Aperture), String> {
  // def is like "D10C,0.5" or "D11R,1X2" or "D12MYMACRO"
  let rest = def.strip_prefix('D').ok_or(format!("bad aperture '{def}'"))?;
  let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
  let code: u32 = digits.parse().map_err(|_| format!("bad aperture '{def}'"))?;
  if code < 10 {
    return Err(format!("aperture D{code} is reserved"));
  }
  let tmpl = &rest[digits.len()..];
  let (name, params) = tmpl.split_once(',').unwrap_or((tmpl, ""));
  let p: Vec<f64> =
    if params.is_empty() { Vec::new() } else { params.split('X').map(num).collect::<Result<_, _>>()? };
  let need = |k: usize| if p.len() >= k { Ok(()) } else { Err(format!("aperture D{code}: missing parameters")) };
  let a = match name {
    "C" => {
      need(1)?;
      Aperture::Circle(p[0])
    }
    "R" => {
      need(2)?;
      Aperture::Rect(p[0], p[1])
    }
    "O" => {
      need(2)?;
      Aperture::Obround(p[0], p[1])
    }
    "P" => {
      need(2)?;
      Aperture::Polygon(p[0], p[1] as u32)
    }
    m if !m.is_empty() => Aperture::Macro(m.to_string()),
    _ => return Err(format!("aperture D{code} has no template")),
  };
  Ok((code, a))
}

/// Parses a Gerber file. Errors on what a fab's viewer would reject: coordinates before the
/// format, an undefined aperture, drawing with no aperture, a missing M02.
pub fn parse(text: &str) -> Result<GerberImage, String> {
  let mut img = GerberImage { metric: true, ..Default::default() };
  let mut fmt: Option<(u32, u32)> = None;
  let mut cur: Option<u32> = None;
  let mut pos: P = [0.0, 0.0];
  let mut dark = true;
  let mut region: Option<Vec<Vec<P>>> = None;
  let mut ended = false;
  let mut chars = text.chars().peekable();
  let mut cmds: Vec<(bool, String)> = Vec::new();
  while let Some(&ch) = chars.peek() {
    if ch.is_whitespace() {
      chars.next();
      continue;
    }
    if ch == '%' {
      chars.next();
      let mut block = String::new();
      for c in chars.by_ref() {
        if c == '%' {
          break;
        }
        block.push(c);
      }
      for cmd in block.split('*').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        cmds.push((true, cmd.replace(['\n', '\r'], "")));
      }
    } else {
      let mut word = String::new();
      for c in chars.by_ref() {
        if c == '*' {
          break;
        }
        word.push(c);
      }
      cmds.push((false, word.trim().replace(['\n', '\r'], "")));
    }
  }
  let mut in_macro = false;
  for (ext, cmd) in cmds {
    if ended {
      return Err("content after M02".into());
    }
    if ext {
      let head = cmd.get(..2).unwrap_or("");
      in_macro = false;
      match head {
        "FS" => {
          let x = cmd.find('X').ok_or("bad FS")?;
          let d: Vec<u32> = cmd[x + 1..].chars().take(2).filter_map(|c| c.to_digit(10)).collect();
          if d.len() < 2 {
            return Err("bad FS".into());
          }
          fmt = Some((d[0], d[1]));
        }
        "MO" => img.metric = cmd == "MOMM",
        "AD" => {
          let (code, a) = aperture(&cmd[2..])?;
          img.apertures.insert(code, a);
        }
        "AM" => in_macro = true,
        "LP" => dark = cmd == "LPD",
        "TF" => img.attributes.push(cmd[3..].to_string()),
        _ => {}
      }
      continue;
    }
    if in_macro {
      continue;
    }
    let mut w = cmd.as_str();
    if w.starts_with("G04") || w.is_empty() {
      continue;
    }
    if w == "M02" || w == "M00" {
      ended = true;
      continue;
    }
    if w == "G36" {
      region = Some(vec![Vec::new()]);
      continue;
    }
    if w == "G37" {
      let contours = region.take().ok_or("G37 without G36")?;
      for c in contours.into_iter().filter(|c| c.len() >= 3) {
        img.regions.push((c, dark));
      }
      continue;
    }
    while let Some(g) = w.strip_prefix('G') {
      let d: String = g.chars().take_while(|c| c.is_ascii_digit()).collect();
      w = &g[d.len()..];
    }
    if w.is_empty() {
      continue;
    }
    if let Some(d) = w.strip_prefix('D')
      && let Ok(code) = d.parse::<u32>()
      && code >= 10
    {
      if !img.apertures.contains_key(&code) {
        return Err(format!("D{code} selected before it was defined"));
      }
      cur = Some(code);
      continue;
    }
    // a coordinate word
    let (_, dec) = fmt.ok_or("coordinates before %FS%")?;
    let scale = 10f64.powi(-(dec as i32));
    let mut next = pos;
    let mut op = None;
    let mut i = 0;
    let b = w.as_bytes();
    while i < b.len() {
      let k = b[i] as char;
      let mut j = i + 1;
      while j < b.len() && (b[j] == b'-' || b[j] == b'+' || b[j].is_ascii_digit()) {
        j += 1;
      }
      let v = &w[i + 1..j];
      match k {
        'X' => next[0] = v.parse::<i64>().map_err(|_| format!("bad X in '{w}'"))? as f64 * scale,
        'Y' => next[1] = v.parse::<i64>().map_err(|_| format!("bad Y in '{w}'"))? as f64 * scale,
        'I' | 'J' => {}
        'D' => op = Some(v.parse::<u32>().map_err(|_| format!("bad D in '{w}'"))?),
        _ => return Err(format!("unknown word '{w}'")),
      }
      i = j;
    }
    match op {
      Some(1) => {
        if let Some(r) = region.as_mut() {
          let c = r.last_mut().ok_or("bad region")?;
          if c.is_empty() {
            c.push(pos);
          }
          c.push(next);
        } else {
          let a = cur.ok_or("D01 with no aperture selected")?;
          img.draws.push((a, pos, next, dark));
        }
      }
      Some(2) => {
        if let Some(r) = region.as_mut() {
          r.push(vec![next]);
        }
      }
      Some(3) => {
        if region.is_some() {
          return Err("D03 inside a region".into());
        }
        let a = cur.ok_or("D03 with no aperture selected")?;
        img.flashes.push((a, next, dark));
      }
      _ => return Err(format!("no operation in '{w}'")),
    }
    pos = next;
  }
  if !ended {
    return Err("missing M02".into());
  }
  if region.is_some() {
    return Err("unclosed region".into());
  }
  Ok(img)
}

/// Excellon holes: (diameter, centre), in millimetres.
pub fn parse_drill(text: &str) -> Result<Vec<(f64, P)>, String> {
  let mut tools: BTreeMap<u32, f64> = BTreeMap::new();
  let mut header = true;
  let mut metric = true;
  let mut cur = None;
  let mut out = Vec::new();
  let mut pos = [0.0, 0.0];
  for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(';')) {
    if header {
      if line == "%" || line == "M95" {
        header = false;
      } else if line.starts_with("INCH") {
        metric = false;
      } else if line.starts_with("METRIC") {
        metric = true;
      } else if let Some(t) = line.strip_prefix('T')
        && let Some((n, d)) = t.split_once('C')
      {
        tools.insert(n.parse().map_err(|_| format!("bad tool '{line}'"))?, num(d)?);
      }
      continue;
    }
    if line == "M30" {
      return Ok(out);
    }
    if let Some(t) = line.strip_prefix('T') {
      let n: u32 = t.parse().map_err(|_| format!("bad tool select '{line}'"))?;
      if n != 0 && !tools.contains_key(&n) {
        return Err(format!("tool T{n} is not defined"));
      }
      cur = Some(n);
      continue;
    }
    if line.starts_with('X') || line.starts_with('Y') {
      let rest = line;
      if let Some(x) = rest.strip_prefix('X') {
        let end = x.find('Y').unwrap_or(x.len());
        pos[0] = num(&x[..end])?;
      }
      if let Some(i) = rest.find('Y') {
        pos[1] = num(&rest[i + 1..])?;
      }
      let t = cur.ok_or("a hole before any tool")?;
      let d = *tools.get(&t).ok_or("a hole with no tool")?;
      let f = if metric { 1.0 } else { 25.4 };
      out.push((d * f, [pos[0] * f, pos[1] * f]));
    }
  }
  Err("missing M30".into())
}
