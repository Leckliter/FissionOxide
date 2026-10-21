//! A project: one schematic, its board, and the libraries the design carries with it.

use serde::{Deserialize, Serialize};

use crate::board::{Board, BoardPad, Placement, Side};
use crate::geom::P;
use crate::library::{Device, Library, Libs};
use crate::schematic::{Netlist, SchPart, Schematic, natural};
use crate::{Violation, builtin};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
  pub name: String,
  #[serde(default)]
  pub schematic: Schematic,
  #[serde(default)]
  pub board: Board,
  /// Libraries imported into (or made in) the design, searched before the starter library.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub libraries: Vec<Library>,
  /// Where the board outline comes from in the 3D design (the engine's own reference, e.g. a
  /// sketch profile or a face); `None` = the outline is drawn in the board editor.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub outline_from: Option<serde_json::Value>,
}

thread_local! {
  static BUILTIN: &'static Library = Box::leak(Box::new(builtin::library()));
}

/// The starter library (built once per thread).
pub fn starter() -> &'static Library {
  BUILTIN.with(|b| *b)
}

impl Project {
  pub fn new(name: &str) -> Project {
    Project {
      name: name.into(),
      schematic: Schematic::default(),
      board: Board::default(),
      libraries: Vec::new(),
      outline_from: None,
    }
  }

  pub fn libs(&self) -> Libs<'_> {
    let mut v: Vec<&Library> = self.libraries.iter().collect();
    v.push(starter());
    Libs(v)
  }

  pub fn netlist(&self) -> Netlist {
    crate::schematic::netlist(&self.schematic, &self.libs())
  }

  pub fn erc(&self) -> Vec<Violation> {
    crate::schematic::erc(&self.schematic, &self.libs())
  }

  pub fn pads(&self) -> Vec<BoardPad> {
    self.board.pads(&self.schematic, &self.libs(), &self.netlist())
  }

  pub fn drc(&self) -> Vec<Violation> {
    self.board.drc(&self.schematic, &self.libs(), &self.netlist())
  }

  /// The next free reference designator with a prefix: R1, R2, ...
  pub fn next_refdes(&self, prefix: &str) -> String {
    let mut n = 1;
    loop {
      let r = format!("{prefix}{n}");
      if !self.schematic.parts.iter().any(|p| p.refdes == r) {
        return r;
      }
      n += 1;
    }
  }

  pub fn device(&self, name: &str) -> Option<Device> {
    self.libs().device(name).cloned()
  }

  /// Adds a part to the schematic (and its footprint beside the board); returns its refdes.
  pub fn add_part(&mut self, device: &str, at: P, refdes: Option<&str>, value: Option<&str>) -> Result<String, String> {
    let dev = self.device(device).ok_or_else(|| format!("no device '{device}' in the libraries"))?;
    let refdes = match refdes {
      Some(r) if self.schematic.parts.iter().any(|p| p.refdes == r) => return Err(format!("{r} is already used")),
      Some(r) => r.to_string(),
      None => self.next_refdes(&dev.prefix),
    };
    self.schematic.parts.push(SchPart {
      refdes: refdes.clone(),
      device: dev.name.clone(),
      value: value.map(str::to_string).unwrap_or(dev.value.clone()),
      at,
      rot: 0.0,
      mirror: false,
    });
    self.sync_board();
    Ok(refdes)
  }

  /// Forward annotation: every schematic part with a footprint gets a placement (new ones in a
  /// row below the board), and placements whose part is gone are removed with nothing else.
  pub fn sync_board(&mut self) {
    let libs = self.libs();
    let wanted: Vec<String> = self
      .schematic
      .parts
      .iter()
      .filter(|p| libs.device(&p.device).is_some_and(|d| !d.footprint.is_empty()))
      .map(|p| p.refdes.clone())
      .collect();
    drop(libs);
    self.board.placements.retain(|pl| wanted.contains(&pl.refdes));
    let (lo, hi) = crate::geom::bounds(self.board.outline.iter().copied()).unwrap_or(([0.0, 0.0], [0.0, 0.0]));
    let mut x = lo[0];
    let y = lo[1] - 8.0;
    let _ = hi;
    for r in wanted {
      if self.board.placement(&r).is_none() {
        let taken =
          |x: f64| self.board.placements.iter().any(|p| (p.at[0] - x).abs() < 4.0 && (p.at[1] - y).abs() < 4.0);
        while taken(x) {
          x += 6.0;
        }
        self.board.placements.push(Placement {
          refdes: r,
          footprint: String::new(),
          at: [x, y],
          rot: 0.0,
          side: Side::Top,
        });
      }
    }
  }

  /// Deletes a part from the schematic and the board.
  pub fn remove_part(&mut self, refdes: &str) -> bool {
    let n = self.schematic.parts.len();
    self.schematic.parts.retain(|p| p.refdes != refdes);
    self.sync_board();
    n != self.schematic.parts.len()
  }

  /// Parts sorted by reference designator.
  pub fn parts_sorted(&self) -> Vec<&SchPart> {
    let mut v: Vec<&SchPart> = self.schematic.parts.iter().collect();
    v.sort_by(|a, b| natural(&a.refdes, &b.refdes));
    v
  }
}
