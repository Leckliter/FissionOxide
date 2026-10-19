//! Cutting tools: their shapes (flat, ball, bull-nose end mills, drills, chamfer mills) and the
//! tool library, a JSON file in the data hub (`tools.json`) with feeds and speeds per tool.
//!
//! Operations keep a copy of their tool, so a design machines the same way on another computer
//! whatever its library holds.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Tool;

/// The shape of a tool's cutting end.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
  /// Flat end mill.
  #[default]
  Flat,
  /// Ball end mill (corner radius = half the diameter).
  Ball,
  /// Bull-nose end mill (a flat end with rounded corners of `corner_radius`).
  BullNose,
  /// Twist drill with a point of `angle` degrees (118° usually).
  Drill,
  /// Chamfer mill: a cone of `angle` degrees included, `tip_diameter` flat at its end.
  Chamfer,
}

impl ToolKind {
  pub const ALL: [ToolKind; 5] =
    [ToolKind::Flat, ToolKind::Ball, ToolKind::BullNose, ToolKind::Drill, ToolKind::Chamfer];

  pub fn id(self) -> &'static str {
    match self {
      ToolKind::Flat => "flat",
      ToolKind::Ball => "ball",
      ToolKind::BullNose => "bull_nose",
      ToolKind::Drill => "drill",
      ToolKind::Chamfer => "chamfer",
    }
  }

  pub fn parse(s: &str) -> Option<ToolKind> {
    let s = s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    match s.as_str() {
      "flat" | "flat_end" | "end_mill" => Some(ToolKind::Flat),
      "ball" | "ball_end" | "ball_nose" => Some(ToolKind::Ball),
      "bull" | "bull_nose" | "bullnose" | "radius" => Some(ToolKind::BullNose),
      "drill" => Some(ToolKind::Drill),
      "chamfer" | "chamfer_mill" | "spot" => Some(ToolKind::Chamfer),
      _ => None,
    }
  }

  /// Mills sideways (end mills); drills and chamfer mills only cut along their own edges.
  pub fn mills(self) -> bool {
    matches!(self, ToolKind::Flat | ToolKind::Ball | ToolKind::BullNose)
  }
}

impl Tool {
  /// A flat end mill with the default feeds and speeds.
  pub fn flat(number: u32, diameter: f64) -> Tool {
    Tool {
      number,
      diameter,
      rpm: super::rpm(),
      feed: super::feed(),
      plunge: super::plunge(),
      kind: ToolKind::Flat,
      name: String::new(),
      corner_radius: 0.0,
      angle: super::tip_angle(),
      tip_diameter: 0.0,
      flute_length: 0.0,
      flutes: 0,
    }
  }

  pub fn radius(&self) -> f64 {
    self.diameter / 2.0
  }

  /// The corner radius actually used: half the diameter for a ball, clamped for a bull nose.
  pub fn corner(&self) -> f64 {
    match self.kind {
      ToolKind::Ball => self.radius(),
      ToolKind::BullNose => self.corner_radius.clamp(0.0, self.radius()),
      _ => 0.0,
    }
  }

  /// Height of the cutting surface above the tool's tip at `rho` from its axis (None beyond
  /// the tool's radius).
  pub fn profile(&self, rho: f64) -> Option<f64> {
    let r = self.radius();
    if rho > r + 1e-9 {
      return None;
    }
    let rho = rho.min(r);
    let half = (self.angle.clamp(1.0, 179.0) / 2.0).to_radians();
    Some(match self.kind {
      ToolKind::Flat => 0.0,
      ToolKind::Ball | ToolKind::BullNose => {
        let c = self.corner();
        let flat = r - c;
        if rho <= flat {
          0.0
        } else {
          let d = rho - flat;
          c - (c * c - d * d).max(0.0).sqrt()
        }
      }
      ToolKind::Drill => rho / half.tan(),
      ToolKind::Chamfer => (rho - self.tip_diameter.clamp(0.0, self.diameter) / 2.0).max(0.0) / half.tan(),
    })
  }

  /// Checks the tool's dimensions.
  pub fn check(&self) -> Result<(), String> {
    if !(self.diameter.is_finite() && self.diameter > 0.0) {
      return Err("the tool diameter must be positive".into());
    }
    if self.kind == ToolKind::BullNose && !(self.corner_radius > 0.0 && self.corner_radius <= self.radius() + 1e-9) {
      return Err("a bull-nose tool needs a corner radius up to half its diameter".into());
    }
    if matches!(self.kind, ToolKind::Drill | ToolKind::Chamfer) && !(self.angle > 0.0 && self.angle < 180.0) {
      return Err("the tip angle must be between 0 and 180 degrees".into());
    }
    if !(self.rpm >= 0.0 && self.feed > 0.0 && self.plunge > 0.0) {
      return Err("the feeds must be positive".into());
    }
    Ok(())
  }

  /// A one-line description ("T3 6 mm ball").
  pub fn describe(&self) -> String {
    let d = format!("{}", (self.diameter * 1000.0).round() / 1000.0);
    let kind = match self.kind {
      ToolKind::Flat => "flat end mill".to_string(),
      ToolKind::Ball => "ball end mill".to_string(),
      ToolKind::BullNose => format!("bull-nose R{}", (self.corner() * 1000.0).round() / 1000.0),
      ToolKind::Drill => format!("drill {}°", self.angle),
      ToolKind::Chamfer => format!("chamfer mill {}°", self.angle),
    };
    if self.name.is_empty() {
      format!("T{} {d} mm {kind}", self.number)
    } else {
      format!("T{} {}", self.number, self.name)
    }
  }
}

/// The tool library of a hub: tools by number, each with its feeds and speeds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Library {
  #[serde(default)]
  pub tools: Vec<Tool>,
}

impl Library {
  /// The library file in a hub directory.
  pub fn path(hub: &Path) -> PathBuf {
    hub.join("tools.json")
  }

  /// Reads a hub's library; a hub without one gets [`Library::standard`].
  pub fn load(hub: &Path) -> Result<Library, String> {
    let path = Library::path(hub);
    match std::fs::read_to_string(&path) {
      Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Library::standard()),
      Err(e) => Err(format!("{}: {e}", path.display())),
    }
  }

  /// Writes the library (to a temporary file first, so a reader never sees half of it).
  pub fn save(&self, hub: &Path) -> Result<(), String> {
    std::fs::create_dir_all(hub).map_err(|e| e.to_string())?;
    let path = Library::path(hub);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text)
      .and_then(|_| std::fs::rename(&tmp, &path))
      .map_err(|e| format!("{}: {e}", path.display()))
  }

  pub fn get(&self, number: u32) -> Option<&Tool> {
    self.tools.iter().find(|t| t.number == number)
  }

  /// Adds a tool, replacing the one with the same number; keeps the list in number order.
  pub fn put(&mut self, tool: Tool) {
    self.tools.retain(|t| t.number != tool.number);
    self.tools.push(tool);
    self.tools.sort_by_key(|t| t.number);
  }

  pub fn remove(&mut self, number: u32) -> bool {
    let n = self.tools.len();
    self.tools.retain(|t| t.number != number);
    self.tools.len() != n
  }

  /// A starter set for aluminium: end mills, a ball, a bull nose, a drill and a chamfer mill.
  pub fn standard() -> Library {
    let t = |number: u32, name: &str, kind: ToolKind, d: f64, rpm: f64, feed: f64, plunge: f64| Tool {
      number,
      name: name.to_string(),
      kind,
      rpm,
      feed,
      plunge,
      flute_length: (d * 3.0).max(10.0),
      flutes: if kind == ToolKind::Drill { 2 } else { 3 },
      ..Tool::flat(number, d)
    };
    Library {
      tools: vec![
        t(1, "20 mm face mill", ToolKind::Flat, 20.0, 8000.0, 1500.0, 300.0),
        t(2, "10 mm flat end mill", ToolKind::Flat, 10.0, 9000.0, 1200.0, 300.0),
        t(3, "6 mm flat end mill", ToolKind::Flat, 6.0, 10000.0, 1000.0, 300.0),
        t(4, "3 mm flat end mill", ToolKind::Flat, 3.0, 12000.0, 600.0, 200.0),
        t(5, "6 mm ball end mill", ToolKind::Ball, 6.0, 10000.0, 1000.0, 300.0),
        Tool { corner_radius: 1.0, ..t(6, "8 mm bull nose R1", ToolKind::BullNose, 8.0, 9500.0, 1100.0, 300.0) },
        t(7, "6 mm drill", ToolKind::Drill, 6.0, 3000.0, 300.0, 200.0),
        Tool { tip_diameter: 0.5, ..t(8, "10 mm 90° chamfer mill", ToolKind::Chamfer, 10.0, 8000.0, 800.0, 200.0) },
      ],
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn profiles() {
    let flat = Tool::flat(1, 6.0);
    assert_eq!(flat.profile(2.9), Some(0.0));
    assert_eq!(flat.profile(3.1), None);
    let ball = Tool { kind: ToolKind::Ball, ..flat.clone() };
    assert!((ball.profile(3.0).unwrap() - 3.0).abs() < 1e-9);
    assert!(ball.profile(0.0).unwrap().abs() < 1e-12);
    let bull = Tool { kind: ToolKind::BullNose, corner_radius: 1.0, ..flat.clone() };
    assert_eq!(bull.profile(2.0), Some(0.0));
    assert!((bull.profile(3.0).unwrap() - 1.0).abs() < 1e-9);
    let drill = Tool { kind: ToolKind::Drill, angle: 90.0, ..flat.clone() };
    assert!((drill.profile(3.0).unwrap() - 3.0).abs() < 1e-9);
    let chamfer = Tool { kind: ToolKind::Chamfer, angle: 90.0, tip_diameter: 2.0, ..flat };
    assert!((chamfer.profile(3.0).unwrap() - 2.0).abs() < 1e-9 && chamfer.profile(0.5) == Some(0.0));
    assert!(bull.check().is_ok() && Tool { corner_radius: 4.0, ..bull }.check().is_err());
  }

  #[test]
  fn library_file_round_trip() {
    let dir = std::env::temp_dir().join(format!("clf-tools-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // a hub without a library gets the standard one
    let mut lib = Library::load(&dir).unwrap();
    assert_eq!(lib, Library::standard());
    assert!(lib.tools.iter().all(|t| t.check().is_ok()));
    lib.put(Tool { name: "custom".into(), ..Tool::flat(3, 5.0) });
    assert!(lib.remove(8) && !lib.remove(8));
    lib.save(&dir).unwrap();
    let back = Library::load(&dir).unwrap();
    assert_eq!(back.get(3).unwrap().diameter, 5.0);
    assert!(back.get(8).is_none());
    // older files without the newer fields still read
    std::fs::write(Library::path(&dir), r#"{"tools": [{"number": 9, "diameter": 4}]}"#).unwrap();
    let old = Library::load(&dir).unwrap();
    assert_eq!(old.get(9).unwrap().kind, ToolKind::Flat);
    assert_eq!(old.get(9).unwrap().feed, 1000.0);
    let _ = std::fs::remove_dir_all(&dir);
  }
}
