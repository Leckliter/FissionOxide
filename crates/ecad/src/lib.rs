//! Electronics design for FissionOxide, in the spirit of Fusion's Electronics workspace (EAGLE):
//! a component library format (symbol + footprint + 3D model), a starter library, KiCad library
//! import, schematics with nets, buses, labels and power symbols, the electrical rules check,
//! PCB layout with ratsnest, tracks, vias and copper pours, the design rules check, and fab
//! output: Gerber RS-274X (X2 attributes), Excellon drill files, BOM and pick-and-place CSV.
//!
//! Coordinates are millimetres with Y up, on the schematic sheet and on the board alike.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

pub mod board;
pub mod builtin;
pub mod example;
pub mod font;
pub mod geom;
pub mod gerber;
pub mod gerber_read;
pub mod kicad;
pub mod library;
pub mod project;
pub mod router;
pub mod schematic;
pub mod svg;

pub use board::{Board, Layer, Placement, Pour, Rules, Side, Track, Via};
pub use library::{Device, Footprint, Library, Model3d, Pad, PadShape, Pin, PinType, Symbol};
pub use project::Project;
pub use schematic::{Netlist, Schematic};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
  Error,
  Warning,
}

/// An ERC or DRC finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Violation {
  /// What rule: `unconnected_pin`, `clearance`, `short`, `unrouted`, ...
  pub code: String,
  pub severity: Severity,
  pub message: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub at: Option<geom::P>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub layer: Option<String>,
}

#[cfg(test)]
mod tests;
