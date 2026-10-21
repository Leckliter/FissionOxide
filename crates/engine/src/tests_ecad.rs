//! Tests for the Electronics commands: schematic, netlist and ERC, the board from a sketch,
//! routing and DRC, fab output, and the PCB in 3D following the layout.

use fox_kernel::glam::DVec3;
use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn tmp(name: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("fox-ecad-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

fn bbox(s: &Session, name: &str) -> (DVec3, DVec3) {
  let b = s.model.bodies.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("no body {name}"));
  let m = b.shape.mass().unwrap();
  (m.bbox_min, m.bbox_max)
}

/// An LED with its resistor between +5V and GND, plus a two-pin header feeding it.
fn led_board(s: &mut Session) {
  run(s, "ecad.part.add", json!({"device": "R_0805", "at": [0, 0], "value": "330"}));
  run(s, "ecad.part.add", json!({"device": "LED_0603", "at": [10.16, -10.16]}));
  run(s, "ecad.part.add", json!({"device": "PinHeader_1x02", "at": [-20.32, 0]}));
  run(s, "ecad.connect", json!({"part": "J1", "pin": 1, "net": "+5V"}));
  run(s, "ecad.connect", json!({"part": "J1", "pin": 2, "net": "GND"}));
  run(s, "ecad.connect", json!({"part": "R1", "pin": 1, "net": "+5V"}));
  // R1.2 wired straight to the LED's anode, cathode to ground
  run(s, "ecad.wire", json!({"points": [{"part": "R1", "pin": 2}, {"part": "D1", "pin": "A"}]}));
  run(s, "ecad.connect", json!({"part": "D1", "pin": "K", "net": "GND"}));
}

#[test]
fn schematic_netlist_and_erc() {
  let mut s = Session::default();
  led_board(&mut s);
  let nets = run(&mut s, "ecad.netlist", json!({}));
  let net_of = |r: &str, p: &str| -> String {
    nets.as_array().unwrap().iter().find(|n| n["pins"].as_array().unwrap().iter().any(|x| x["refdes"] == r && x["pin"] == p)).unwrap()["name"].as_str().unwrap().to_string()
  };
  assert_eq!(net_of("J1", "1"), "+5V");
  assert_eq!(net_of("R1", "1"), "+5V");
  assert_eq!(net_of("D1", "1"), "GND");
  assert_eq!(net_of("R1", "2"), net_of("D1", "2"));
  assert_eq!(nets.as_array().unwrap().len(), 3);
  let erc = run(&mut s, "ecad.erc", json!({}));
  assert_eq!(erc["errors"], 0, "{erc}");
  assert_eq!(erc["warnings"], 0, "{erc}");
  // an unconnected part is reported
  run(&mut s, "ecad.part.add", json!({"device": "C_0603", "at": [30, 0]}));
  let erc = run(&mut s, "ecad.erc", json!({}));
  assert_eq!(erc["warnings"], 2, "{erc}");
  // one undo step per command
  s.undo();
  assert_eq!(run(&mut s, "ecad.erc", json!({}))["warnings"], 0);
  let lib = run(&mut s, "ecad.library", json!({"query": "qfn"}));
  assert!(lib.as_array().unwrap().iter().any(|d| d["device"] == "ATmega328P-MU"), "{lib}");
}

#[test]
fn board_from_sketch_routing_drc_and_3d() {
  let mut s = Session::default();
  let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [30, 20]}));
  run(&mut s, "sketch.finish", json!({}));
  led_board(&mut s);
  let o = run(&mut s, "ecad.board.outline", json!({"sketch": sk}));
  assert_eq!(o["outline"].as_array().unwrap().len(), 4, "{o}");
  run(&mut s, "ecad.place", json!({"refdes": "J1", "at": [5, 11.27]}));
  run(&mut s, "ecad.place", json!({"refdes": "R1", "at": [15, 10], "rot": 0}));
  // turned round so its anode faces the resistor
  run(&mut s, "ecad.place", json!({"refdes": "D1", "at": [24, 10], "rot": 180}));
  let drc = run(&mut s, "ecad.drc", json!({}));
  assert_eq!(drc["errors"], 0, "{drc}");
  assert!(drc["warnings"].as_u64().unwrap() >= 3, "unrouted: {drc}");
  // route R1.2 to D1.2 by hand on top; the net comes from the pad
  let r = run(&mut s, "ecad.route", json!({"points": [[15.9125, 10], [23.2125, 10]]}));
  assert!(r["net"].as_str().unwrap().starts_with("N$"), "{r}");
  // a track across another net's pad is a short
  let bad = run(&mut s, "ecad.route", json!({"points": [[14.0875, 8], [14.0875, 12]], "net": "GND"}));
  assert_eq!(bad["net"], "GND");
  let drc = run(&mut s, "ecad.drc", json!({}));
  assert!(drc["violations"].as_array().unwrap().iter().any(|v| v["code"] == "short"), "{drc}");
  s.undo();
  let a = run(&mut s, "ecad.autoroute", json!({}));
  assert_eq!(a["unrouted"], 0, "{a}");
  run(&mut s, "ecad.pour", json!({"net": "GND", "layer": "bottom"}));
  let drc = run(&mut s, "ecad.drc", json!({}));
  assert_eq!(drc["errors"], 0, "{drc}");
  assert_eq!(drc["warnings"], 0, "{drc}");

  // to 3D: the board body, drilled for the header, and the parts on it, in a component
  let p = run(&mut s, "ecad.push3d", json!({}));
  let feature = p["feature"].as_u64().unwrap();
  assert_eq!(p["bodies"].as_array().unwrap().len(), 4, "{p}");
  assert!(p["component"].is_u64(), "{p}");
  let (lo, hi) = bbox(&s, "PCB Untitled");
  assert!((lo - DVec3::ZERO).length() < 1e-6 && (hi - DVec3::new(30.0, 20.0, 1.6)).length() < 1e-6, "{lo} {hi}");
  let vias: usize = run(&mut s, "ecad.info", json!({}))["board"]["vias"].as_u64().unwrap() as usize;
  let board = s.model.bodies.iter().find(|b| b.name == "PCB Untitled").unwrap();
  let drilled = 30.0 * 20.0 * 1.6 - 2.0 * std::f64::consts::PI * 0.25 * 1.6;
  let via_vol = vias as f64 * std::f64::consts::PI * 0.15f64.powi(2) * 1.6;
  assert!((board.shape.mass().unwrap().volume - (drilled - via_vol)).abs() < 1e-3, "{}", board.shape.mass().unwrap().volume);
  let (rlo, rhi) = bbox(&s, "R1");
  assert!((rlo.z - 1.6).abs() < 1e-6 && (rhi.z - 2.1).abs() < 1e-6, "R1 sits on the board: {rlo} {rhi}");
  assert!(((rlo.x + rhi.x) / 2.0 - 15.0).abs() < 1e-6);

  // the layout moves the part in 3D
  run(&mut s, "ecad.place", json!({"refdes": "R1", "at": [15, 5], "side": "bottom"}));
  let (rlo, rhi) = bbox(&s, "R1");
  assert!((rhi.z - 0.0).abs() < 1e-6 && (rlo.z + 0.5).abs() < 1e-6, "R1 hangs under the board: {rlo} {rhi}");
  assert!(((rlo.y + rhi.y) / 2.0 - 5.0).abs() < 1e-6);
  assert!(s.model.status(fox_model::FeatureId(feature as u32)).and_then(|x| x.error.clone()).is_none());

  // an enclosure lid resting on the parts shows up as interference with the board's component
  let lid = run(&mut s, "box", json!({"width": 40, "depth": 30, "height": 3, "at": [15, 10]}))["feature"].as_u64().unwrap();
  let body = s.model.bodies.iter().find(|b| b.feature.0 == lid as u32).unwrap().id.0;
  run(&mut s, "component.create", json!({"name": "Lid", "bodies": [body]}));
  let clash = run(&mut s, "assembly.interference", json!({}));
  assert!(!clash.as_array().unwrap().is_empty(), "{clash}");
}

#[test]
fn outline_follows_its_sketch() {
  let mut s = Session::default();
  run(&mut s, "param.add", json!({"name": "w", "expr": "40"}));
  let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  let r = run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 25]}));
  let curves: Vec<u64> = r["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
  run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [curves[0]], "value": "w"}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "ecad.board.outline", json!({"sketch": sk, "thickness": 1.0}));
  run(&mut s, "ecad.push3d", json!({}));
  let name = "PCB Untitled";
  let (_, hi) = bbox(&s, name);
  assert!((hi.x - 40.0).abs() < 1e-6 && (hi.z - 1.0).abs() < 1e-6, "{hi}");
  run(&mut s, "param.set", json!({"name": "w", "expr": "60"}));
  let (lo, hi) = bbox(&s, name);
  assert!((hi.x - lo.x - 60.0).abs() < 1e-6, "the 3D board follows the sketch: {lo} {hi}");
  // the 2D outline catches up at the next electronics edit
  run(&mut s, "ecad.board.rules", json!({"clearance": 0.15}));
  let info = run(&mut s, "ecad.info", json!({}));
  assert!((info["board"]["size"][0].as_f64().unwrap() - 60.0).abs() < 1e-6, "{info}");
}

#[test]
fn board_on_a_face() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 50, "depth": 30, "height": 10, "at": [25, 15]}));
  run(&mut s, "ecad.board.outline", json!({"face": {"point": [25, 15, 10]}}));
  let info = run(&mut s, "ecad.info", json!({}));
  let size = info["board"]["size"].clone();
  assert!((size[0].as_f64().unwrap() - 50.0).abs() < 1e-6 && (size[1].as_f64().unwrap() - 30.0).abs() < 1e-6, "{info}");
  run(&mut s, "ecad.push3d", json!({}));
  let (lo, hi) = bbox(&s, "PCB Untitled");
  assert!((lo.z - 10.0).abs() < 1e-6 && (hi.z - 11.6).abs() < 1e-6, "the board sits on the face: {lo} {hi}");
}

#[test]
fn example_exports_fab_files_and_saves() {
  let mut s = Session::default();
  let info = run(&mut s, "ecad.new", json!({"example": true}));
  assert_eq!(info["drc"]["errors"], 0, "{info}");
  assert_eq!(info["erc"]["errors"], 0, "{info}");
  assert_eq!(info["board"]["unrouted"], 0, "{info}");
  let dir = tmp("fab");
  let out = run(&mut s, "ecad.export", json!({"dir": dir}));
  let files: Vec<String> = serde_json::from_value(out["files"].clone()).unwrap();
  assert_eq!(files.len(), 13, "{out}");
  for f in files.iter().filter(|f| f.ends_with(".gbr")) {
    let text = std::fs::read_to_string(f).unwrap();
    fox_ecad::gerber_read::parse(&text).unwrap_or_else(|e| panic!("{f}: {e}"));
  }
  let drl = std::fs::read_to_string(files.iter().find(|f| f.ends_with("-PTH.drl")).unwrap()).unwrap();
  assert!(!fox_ecad::gerber_read::parse_drill(&drl).unwrap().is_empty());
  let svg = run(&mut s, "ecad.svg", json!({"view": "schematic"}));
  assert!(svg["svg"].as_str().unwrap().contains("<svg"));
  // the electronics are saved with the design
  let path = dir.join("board.fox");
  run(&mut s, "save", json!({"path": path}));
  let mut t = Session::default();
  run(&mut t, "open", json!({"path": path}));
  assert_eq!(t.design.electronics, s.design.electronics);
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn kicad_library_import() {
  let dir = tmp("kicad");
  let sym = dir.join("mine.kicad_sym");
  std::fs::write(
    &sym,
    r#"(kicad_symbol_lib (version 20220914)
  (symbol "Fuse" (in_bom yes) (on_board yes)
    (property "Reference" "F" (at 0 0 0)) (property "Value" "Fuse" (at 0 0 0))
    (property "Footprint" "mine:Fuse_1206" (at 0 0 0))
    (symbol "Fuse_0_1" (rectangle (start -0.762 -2.54) (end 0.762 2.54) (stroke (width 0.254)) (fill (type none))))
    (symbol "Fuse_1_1"
      (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
      (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2")))))"#,
  )
  .unwrap();
  let pretty = dir.join("mine.pretty");
  std::fs::create_dir_all(&pretty).unwrap();
  std::fs::write(
    pretty.join("Fuse_1206.kicad_mod"),
    r#"(footprint "Fuse_1206" (layer "F.Cu")
  (pad "1" smd roundrect (at -1.5 0) (size 1.2 1.8) (layers "F.Cu" "F.Paste" "F.Mask"))
  (pad "2" smd roundrect (at 1.5 0) (size 1.2 1.8) (layers "F.Cu" "F.Paste" "F.Mask")))"#,
  )
  .unwrap();
  let mut s = Session::default();
  let r = run(&mut s, "ecad.library.import", json!({"path": sym, "name": "mine"}));
  assert_eq!(r["devices"], 1);
  let r = run(&mut s, "ecad.library.import", json!({"path": pretty}));
  assert_eq!(r["footprints"], 1);
  run(&mut s, "ecad.part.add", json!({"device": "Fuse", "at": [0, 0]}));
  let info = run(&mut s, "ecad.info", json!({}));
  assert_eq!(info["board"]["pads"], 2, "{info}");
  // our own format round-trips
  let json_lib = dir.join("mine.json");
  run(&mut s, "ecad.library.export", json!({"path": json_lib, "library": "mine"}));
  let mut t = Session::default();
  let r = run(&mut t, "ecad.library.import", json!({"path": json_lib}));
  assert_eq!(r["devices"], 1);
  assert_eq!(r["footprints"], 1);
  let _ = std::fs::remove_dir_all(dir);
}
