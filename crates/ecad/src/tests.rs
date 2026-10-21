use crate::board::{Hole, Layer, Track, Via};
use crate::geom::P;
use crate::gerber_read::{parse, parse_drill};
use crate::project::{Project, starter};
use crate::schematic::{Bus, Label, Wire, bus_members};
use crate::{Severity, Violation, example, gerber, kicad, router};

fn codes(v: &[Violation]) -> Vec<&str> {
  v.iter().map(|x| x.code.as_str()).collect()
}

fn errors(v: &[Violation]) -> Vec<&Violation> {
  v.iter().filter(|x| x.severity == Severity::Error).collect()
}

fn pin_at(pr: &Project, refdes: &str, pin: &str) -> P {
  let libs = pr.libs();
  let part = pr.schematic.parts.iter().find(|p| p.refdes == refdes).unwrap();
  let sym = libs.symbol(&libs.device(&part.device).unwrap().symbol).unwrap();
  part.xf().apply(sym.pins.iter().find(|p| p.number == pin).unwrap().at)
}

#[test]
fn starter_library_is_consistent() {
  let lib = starter();
  assert!(lib.devices.len() >= 20);
  for d in &lib.devices {
    let sym = lib.symbol(&d.symbol).unwrap_or_else(|| panic!("{}: no symbol {}", d.name, d.symbol));
    if d.footprint.is_empty() {
      assert!(sym.power, "{} has no footprint but isn't a power symbol", d.name);
      continue;
    }
    let fp = lib.footprint(&d.footprint).unwrap_or_else(|| panic!("{}: no footprint {}", d.name, d.footprint));
    for pin in &sym.pins {
      for pad in d.pads_of(&pin.number) {
        assert!(fp.pads.iter().any(|p| p.number == pad), "{}: pin {} maps to missing pad {pad}", d.name, pin.number);
      }
    }
    assert!(fp.model.is_some(), "{} has no 3D model", fp.name);
  }
  for name in [
    "R_0603",
    "C_0805",
    "LED_0603",
    "PinHeader_1x04",
    "NE555D",
    "ATmega328P-MU",
    "XC6206P332MR",
    "MMBT3904",
    "USB_C_Receptacle",
  ] {
    assert!(lib.device(name).is_some(), "{name}");
  }
  for name in ["SOIC-8_3.9x4.9mm_P1.27mm", "QFN-32-1EP_5x5mm_P0.5mm", "SOT-23"] {
    assert!(lib.footprint(name).is_some(), "{name}");
  }
  assert_eq!(lib.footprint("QFN-32-1EP_5x5mm_P0.5mm").unwrap().pads.len(), 33);
}

/// R1 between +5V and an LED to GND, drawn with wires; a label joins a second, separate wire.
fn led_circuit() -> Project {
  let mut pr = Project::new("led");
  pr.add_part("R_0603", [0.0, 0.0], None, Some("330")).unwrap();
  pr.add_part("LED_0603", [0.0, -15.24], None, None).unwrap();
  // R1.2 (0,-5.08) down to D1.A (0,-10.16)
  let r2 = pin_at(&pr, "R1", "2");
  let da = pin_at(&pr, "D1", "2");
  pr.schematic.wires.push(Wire { a: r2, b: da });
  // power: +5V on R1.1, GND on D1.K
  let r1 = pin_at(&pr, "R1", "1");
  pr.add_part("+5V", r1, Some("#PWR1"), Some("+5V")).unwrap();
  let dk = pin_at(&pr, "D1", "1");
  pr.add_part("GND", dk, Some("#PWR2"), Some("GND")).unwrap();
  pr
}

#[test]
fn netlist_from_wires_and_power_symbols() {
  let pr = led_circuit();
  let nl = pr.netlist();
  assert_eq!(nl.net_of("R1", "1"), Some("+5V"));
  assert_eq!(nl.net_of("D1", "1"), Some("GND"));
  let mid = nl.net_of("R1", "2").unwrap();
  assert_eq!(nl.net_of("D1", "2"), Some(mid));
  assert!(mid.starts_with("N$"));
  assert_eq!(nl.nets.len(), 3);
  // power symbols aren't parts of a net
  assert!(nl.nets.iter().all(|n| n.pins.iter().all(|p| !p.refdes.starts_with('#'))));
  assert!(errors(&pr.erc()).is_empty(), "{:?}", pr.erc());
}

#[test]
fn labels_junctions_and_crossings() {
  let mut pr = Project::new("t");
  for at in [[0.0, 0.0], [20.32, 0.0], [40.64, 0.0], [60.96, 0.0]] {
    pr.add_part("R_0603", at, None, None).unwrap();
  }
  // R1.1 and R2.1 joined by a horizontal wire; R3.1 tees into its middle with a vertical wire
  let (a, b) = (pin_at(&pr, "R1", "1"), pin_at(&pr, "R2", "1"));
  pr.schematic.wires.push(Wire { a, b });
  let r3 = pin_at(&pr, "R3", "2");
  pr.schematic.wires.push(Wire { a: [10.16, a[1]], b: [10.16, r3[1]] });
  pr.schematic.wires.push(Wire { a: [10.16, r3[1]], b: r3 });
  // a wire crossing the first one without a junction: not connected
  pr.schematic.wires.push(Wire { a: [5.08, a[1] + 5.0], b: [5.08, a[1] - 5.0] });
  // R4.1 and R1.2 joined by labels with the same name
  let r4 = pin_at(&pr, "R4", "1");
  pr.schematic.wires.push(Wire { a: r4, b: [r4[0] + 5.08, r4[1]] });
  pr.schematic.labels.push(Label { name: "SIG".into(), at: [r4[0] + 5.08, r4[1]] });
  let r12 = pin_at(&pr, "R1", "2");
  pr.schematic.labels.push(Label { name: "SIG".into(), at: r12 });
  let nl = pr.netlist();
  let n = nl.net_of("R1", "1").unwrap();
  assert_eq!(nl.net_of("R2", "1"), Some(n));
  assert_eq!(nl.net_of("R3", "2"), Some(n));
  assert_ne!(nl.net_of("R3", "1"), Some(n));
  assert_eq!(nl.net_of("R4", "1"), Some("SIG"));
  assert_eq!(nl.net_of("R1", "2"), Some("SIG"));
  assert_eq!(nl.net("SIG").unwrap().pins.len(), 2);
  // the crossing wire dangles at both ends
  let erc = pr.erc();
  assert_eq!(codes(&erc).iter().filter(|c| **c == "dangling_wire").count(), 2, "{erc:?}");
}

#[test]
fn buses() {
  assert_eq!(bus_members("D[0..3]"), ["D0", "D1", "D2", "D3"]);
  assert_eq!(bus_members("A[2..0],SDA"), ["A2", "A1", "A0", "SDA"]);
  let mut pr = Project::new("b");
  pr.add_part("R_0603", [0.0, 0.0], None, None).unwrap();
  pr.add_part("R_0603", [20.32, 0.0], None, None).unwrap();
  pr.schematic.buses.push(Bus { name: "D[0..1]".into(), pts: vec![[-10.16, 20.0], [30.0, 20.0]] });
  let (a, b) = (pin_at(&pr, "R1", "1"), pin_at(&pr, "R2", "1"));
  pr.schematic.wires.push(Wire { a, b: [a[0], 20.0] });
  pr.schematic.wires.push(Wire { a: b, b: [b[0], 20.0] });
  pr.schematic.labels.push(Label { name: "D0".into(), at: [a[0], 15.0] });
  let erc = pr.erc();
  // R2's wire touches the bus without naming a member
  assert_eq!(codes(&erc).iter().filter(|c| **c == "bus_entry_unnamed").count(), 1, "{erc:?}");
  pr.schematic.labels.push(Label { name: "D1".into(), at: [b[0], 15.0] });
  assert!(!codes(&pr.erc()).contains(&"bus_entry_unnamed"));
  // the bus itself connects nothing
  let nl = pr.netlist();
  assert_ne!(nl.net_of("R1", "1"), nl.net_of("R2", "1"));
}

#[test]
fn erc_catches_violations() {
  let mut pr = Project::new("e");
  pr.add_part("NE555D", [0.0, 0.0], None, None).unwrap();
  pr.add_part("XC6206P332MR", [30.0, 0.0], None, None).unwrap();
  pr.add_part("XC6206P332MR", [60.0, 0.0], None, None).unwrap();
  // two regulator outputs tied together
  let (a, b) = (pin_at(&pr, "U2", "2"), pin_at(&pr, "U3", "2"));
  pr.schematic.wires.push(Wire { a, b: [a[0], 20.0] });
  pr.schematic.wires.push(Wire { a: [a[0], 20.0], b: [b[0], 20.0] });
  pr.schematic.wires.push(Wire { a: [b[0], 20.0], b });
  // the 555's VCC pin on a net with nothing powering it, joined to its TRIG input
  let (vcc, trig) = (pin_at(&pr, "U1", "8"), pin_at(&pr, "U1", "2"));
  pr.schematic.labels.push(Label { name: "X".into(), at: vcc });
  pr.schematic.labels.push(Label { name: "X".into(), at: trig });
  pr.schematic.parts.push(pr.schematic.parts[0].clone());
  let erc = pr.erc();
  let c = codes(&erc);
  for want in ["output_conflict", "power_not_driven", "unconnected_pin", "duplicate_refdes"] {
    assert!(c.contains(&want), "{want} missing: {erc:?}");
  }
}

#[test]
fn example_schematic_passes_erc() {
  let pr = example::usb_mcu_schematic().unwrap();
  let erc = pr.erc();
  assert!(erc.is_empty(), "{erc:#?}");
  let nl = pr.netlist();
  assert_eq!(nl.net_of("U2", "17"), Some("SCK"));
  assert_eq!(nl.net_of("J2", "6"), Some("SCK"));
  assert_eq!(nl.net_of("R4", "1"), Some("SCK"));
  assert_eq!(nl.net_of("U1", "2"), Some("+3V3"));
  assert_eq!(nl.net_of("J1", "VBUS"), Some("+5V"));
  let gnd = nl.net("GND").unwrap();
  assert!(gnd.pins.len() >= 12, "{gnd:?}");
  // pins with no connection aren't on any net with other pins
  assert_eq!(pr.board.placements.len(), 14);
}

/// Two resistors side by side on a small board, unrouted.
fn two_r_board() -> Project {
  let mut pr = Project::new("drc");
  pr.add_part("R_0603", [0.0, 0.0], None, None).unwrap();
  pr.add_part("R_0603", [20.0, 0.0], None, None).unwrap();
  let (a, b) = (pin_at(&pr, "R1", "1"), pin_at(&pr, "R2", "1"));
  pr.schematic.labels.push(Label { name: "A".into(), at: a });
  pr.schematic.labels.push(Label { name: "A".into(), at: b });
  let (a, b) = (pin_at(&pr, "R1", "2"), pin_at(&pr, "R2", "2"));
  pr.schematic.labels.push(Label { name: "B".into(), at: a });
  pr.schematic.labels.push(Label { name: "B".into(), at: b });
  pr.board.outline = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [0.0, 10.0]];
  pr.board.placements[0].at = [5.0, 5.0];
  pr.board.placements[1].at = [15.0, 5.0];
  pr
}

#[test]
fn ratsnest_and_routing() {
  let mut pr = two_r_board();
  let pads = pr.pads();
  assert_eq!(pads.len(), 4);
  let air = pr.board.ratsnest(&pads);
  assert_eq!(air.len(), 2, "{air:?}");
  assert!(codes(&pr.drc()).contains(&"unrouted"));
  // route net A by hand: R1.1 (4.175, 5) → R2.1 (14.175, 5) crosses R1.2, so go around below
  let w = 0.25;
  for (a, b) in [([4.175, 5.0], [4.175, 3.0]), ([4.175, 3.0], [14.175, 3.0]), ([14.175, 3.0], [14.175, 5.0])] {
    pr.board.tracks.push(Track { layer: Layer::Top, a, b, width: w, net: "A".into() });
  }
  let pads = pr.pads();
  let air = pr.board.ratsnest(&pads);
  assert_eq!(air.len(), 1);
  assert_eq!(air[0].net, "B");
  // the router does the other one, through a via if it must
  let rep = router::autoroute(&mut pr, &[], 0.25);
  assert_eq!(rep.routed, 1, "{rep:?}");
  assert!(rep.failed.is_empty());
  let drc = pr.drc();
  assert!(drc.is_empty(), "{drc:#?}");
}

#[test]
fn drc_catches_violations() {
  let mut pr = two_r_board();
  let r = pr.board.rules.clone();
  // a B track passing 0.1 mm from net A's track
  pr.board.tracks.push(Track { layer: Layer::Top, a: [2.0, 8.0], b: [18.0, 8.0], width: 0.25, net: "A".into() });
  pr.board.tracks.push(Track { layer: Layer::Top, a: [2.0, 8.35], b: [18.0, 8.35], width: 0.25, net: "B".into() });
  let c = pr.drc();
  assert!(codes(&c).contains(&"clearance"), "{c:#?}");
  // crossing tracks of different nets short
  pr.board.tracks.push(Track { layer: Layer::Bottom, a: [10.0, 1.0], b: [10.0, 9.0], width: 0.25, net: "A".into() });
  pr.board.tracks.push(Track { layer: Layer::Bottom, a: [8.0, 6.0], b: [12.0, 6.0], width: 0.25, net: "B".into() });
  // too narrow, too close to the edge, outside the board
  pr.board.tracks.push(Track {
    layer: Layer::Bottom,
    a: [2.0, 2.0],
    b: [3.0, 2.0],
    width: r.min_track / 2.0,
    net: "A".into(),
  });
  pr.board.tracks.push(Track { layer: Layer::Bottom, a: [2.0, 0.2], b: [6.0, 0.2], width: 0.2, net: "A".into() });
  pr.board.tracks.push(Track { layer: Layer::Bottom, a: [30.0, 5.0], b: [31.0, 5.0], width: 0.2, net: "A".into() });
  // a via with a thin annular ring, and a hole crowding it
  pr.board.vias.push(Via { at: [17.0, 2.0], drill: 0.4, diameter: 0.5, net: "A".into() });
  pr.board.holes.push(Hole { at: [17.5, 2.0], drill: 0.5 });
  let c = pr.drc();
  let cs = codes(&c);
  for want in [
    "clearance",
    "short",
    "track_width",
    "edge_clearance",
    "outside_board",
    "annular_ring",
    "hole_clearance",
    "unrouted",
  ] {
    assert!(cs.contains(&want), "{want} missing: {c:#?}");
  }
  // overlapping parts
  pr.board.placements[1].at = [5.5, 5.0];
  assert!(codes(&pr.drc()).contains(&"courtyard_overlap"));
}

#[test]
fn example_board_is_routed_and_clean() {
  let pr = example::usb_mcu_board().unwrap();
  let drc = pr.drc();
  let errs = errors(&drc);
  assert!(errs.is_empty(), "{errs:#?}");
  let unrouted: Vec<_> = drc.iter().filter(|v| v.code == "unrouted").collect();
  assert!(unrouted.is_empty(), "{unrouted:#?}");
  assert!(!pr.board.tracks.is_empty());
}

#[test]
fn gerbers_parse_back() {
  let pr = example::usb_mcu_board().unwrap();
  let files = gerber::fab_files(&pr);
  let get = |suffix: &str| &files.iter().find(|f| f.0.ends_with(suffix)).unwrap_or_else(|| panic!("{suffix}")).1;
  let pads = pr.pads();
  for (name, text) in files.iter().filter(|f| f.0.ends_with(".gbr")) {
    let img = parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(img.metric);
    assert!(img.attributes.iter().any(|a| a.starts_with("FileFunction")), "{name}");
  }
  let top = parse(get("-F_Cu.gbr")).unwrap();
  // two regions per pour plus any rotated pads; dark flashes: top pads and every via
  let top_pads = pads.iter().filter(|p| p.layers.contains(&Layer::Top) && !p.npth).count();
  let dark_flashes = top.flashes.iter().filter(|f| f.2).count();
  assert_eq!(dark_flashes, top_pads + pr.board.vias.len());
  assert!(top.regions.iter().any(|r| r.1), "the pour is a dark region");
  assert!(top.flashes.iter().any(|f| !f.2), "clearances are cut with clear polarity");
  let top_tracks = pr.board.tracks.iter().filter(|t| t.layer == Layer::Top).count();
  assert!(top.draws.iter().filter(|d| d.3).count() >= top_tracks);
  let (lo, hi) = parse(get("-Edge_Cuts.gbr")).unwrap().bounds().unwrap();
  assert!((lo[0] + 0.05).abs() < 1e-6 && (hi[0] - 50.05).abs() < 1e-6 && (hi[1] - 32.05).abs() < 1e-6, "{lo:?} {hi:?}");
  let paste = parse(get("-F_Paste.gbr")).unwrap();
  assert_eq!(paste.flashes.len() + paste.regions.len(), pads.iter().filter(|p| p.drill.is_none()).count());
  let silk = parse(get("-F_Silkscreen.gbr")).unwrap();
  assert!(silk.draws.len() > 50);
  // drills
  let pth = parse_drill(get("-PTH.drl")).unwrap();
  let plated = pads.iter().filter(|p| p.drill.is_some() && !p.npth).count() + pr.board.vias.len();
  assert_eq!(pth.len(), plated);
  let npth = parse_drill(get("-NPTH.drl")).unwrap();
  assert_eq!(npth.len(), 2);
  // BOM and pick-and-place
  let bom = get("-BOM.csv");
  assert!(bom.lines().any(|l| l.starts_with("4,C1 C3 C4 C5,100n") || l.contains("C3 C4 C5")), "{bom}");
  let pnp = get("-PnP.csv");
  assert_eq!(pnp.lines().count(), 1 + pr.board.placements.len());
  assert!(pnp.contains("U2,ATmega328P,QFN-32-1EP_5x5mm_P0.5mm,30.0000,14.0000,0,Top"), "{pnp}");
}

#[test]
fn gerber_reader_rejects_bad_files() {
  assert!(parse("%FSLAX46Y46*%\n%MOMM*%\nD10*\nM02*\n").is_err());
  assert!(parse("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.5*%\nD10*\nX0Y0D03*\n").is_err());
  assert!(parse("%MOMM*%\n%ADD10C,0.5*%\nD10*\nX0Y0D03*\nM02*\n").is_err());
  let ok = parse("%FSLAX46Y46*%\n%MOMM*%\n%ADD10R,1X2*%\nD10*\nX1000000Y-2000000D03*\nM02*\n").unwrap();
  assert_eq!(ok.flashes[0].1, [1.0, -2.0]);
}

const KICAD_SYM: &str = r##"(kicad_symbol_lib (version 20220914) (generator kicad_symbol_editor)
  (symbol "R_Small" (pin_numbers hide) (pin_names (offset 0.254) hide) (in_bom yes) (on_board yes)
    (property "Reference" "R" (at 0.762 0.508 0) (effects (font (size 1.27 1.27)) (justify left)))
    (property "Value" "R_Small" (at 0.762 -1.016 0) (effects (font (size 1.27 1.27)) (justify left)))
    (property "Footprint" "Resistor_SMD:R_0402_1005Metric" (at 0 0 0) (effects (font (size 1.27 1.27)) hide))
    (symbol "R_Small_0_1"
      (rectangle (start -0.762 1.778) (end 0.762 -1.778) (stroke (width 0.2032) (type default)) (fill (type none)))
    )
    (symbol "R_Small_1_1"
      (pin passive line (at 0 2.54 270) (length 0.762) (name "~" (effects (font (size 1.27 1.27)))) (number "1" (effects (font (size 1.27 1.27)))))
      (pin passive line (at 0 -2.54 90) (length 0.762) (name "~" (effects (font (size 1.27 1.27)))) (number "2" (effects (font (size 1.27 1.27)))))
    )
  )
  (symbol "LM358" (in_bom yes) (on_board yes)
    (property "Reference" "U" (at 0 5.08 0) (effects (font (size 1.27 1.27))))
    (property "Value" "LM358" (at 0 -5.08 0) (effects (font (size 1.27 1.27))))
    (symbol "LM358_1_1"
      (polyline (pts (xy -5.08 5.08) (xy 5.08 0) (xy -5.08 -5.08) (xy -5.08 5.08)) (stroke (width 0.254) (type default)) (fill (type background)))
      (pin output line (at 7.62 0 180) (length 2.54) (name "~" (effects (font (size 1.27 1.27)))) (number "1" (effects (font (size 1.27 1.27)))))
      (pin input line (at -7.62 -2.54 0) (length 2.54) (name "-" (effects (font (size 1.27 1.27)))) (number "2" (effects (font (size 1.27 1.27)))))
      (pin input line (at -7.62 2.54 0) (length 2.54) (name "+" (effects (font (size 1.27 1.27)))) (number "3" (effects (font (size 1.27 1.27)))))
    )
    (symbol "LM358_2_1"
      (pin output line (at 7.62 0 180) (length 2.54) (name "~" (effects (font (size 1.27 1.27)))) (number "7" (effects (font (size 1.27 1.27)))))
    )
    (symbol "LM358_3_1"
      (pin power_in line (at -2.54 -7.62 90) (length 3.81) (name "V-" (effects (font (size 1.27 1.27)))) (number "4" (effects (font (size 1.27 1.27)))))
      (pin power_in line (at -2.54 7.62 270) (length 3.81) (name "V+" (effects (font (size 1.27 1.27)))) (number "8" (effects (font (size 1.27 1.27)))))
    )
  )
  (symbol "GND" (power) (pin_names (offset 0)) (in_bom yes) (on_board yes)
    (property "Reference" "#PWR" (at 0 -6.35 0) (effects (font (size 1.27 1.27)) hide))
    (property "Value" "GND" (at 0 -3.81 0) (effects (font (size 1.27 1.27))))
    (symbol "GND_1_1" (pin power_in line (at 0 0 270) (length 0) hide (name "GND" (effects (font (size 1.27 1.27)))) (number "1" (effects (font (size 1.27 1.27))))))
  )
)"##;

const KICAD_MOD: &str = r#"(footprint "R_0402_1005Metric" (version 20221018) (generator pcbnew)
  (layer "F.Cu")
  (attr smd)
  (fp_text reference "REF**" (at 0 -1.17) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
  (fp_line (start -0.153641 -0.38) (end 0.153641 -0.38) (stroke (width 0.12) (type solid)) (layer "F.SilkS"))
  (fp_rect (start -0.93 -0.47) (end 0.93 0.47) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd"))
  (fp_arc (start -0.5 0) (mid 0 -0.5) (end 0.5 0) (stroke (width 0.1) (type solid)) (layer "F.Fab"))
  (pad "1" smd roundrect (at -0.51 0.1) (size 0.54 0.64) (layers "F.Cu" "F.Paste" "F.Mask") (roundrect_rratio 0.25))
  (pad "2" smd roundrect (at 0.51 0 90) (size 0.54 0.64) (layers "F.Cu" "F.Paste" "F.Mask") (roundrect_rratio 0.25))
  (pad "3" thru_hole circle (at 0 2) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask"))
  (pad "" np_thru_hole circle (at 0 -2) (size 1.2 1.2) (drill 1.2) (layers "*.Cu" "*.Mask"))
  (model "${KICAD6_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.wrl" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0)))
)"#;

#[test]
fn kicad_import() {
  let lib = kicad::import_symbols(KICAD_SYM, "kicad").unwrap();
  assert_eq!(lib.symbols.len(), 3);
  let r = lib.symbol("R_Small").unwrap();
  assert_eq!(r.pins.len(), 2);
  assert_eq!(r.pins[0].at, [0.0, 2.54]);
  assert_eq!(r.pins[0].dir, 270.0);
  assert_eq!(lib.device("R_Small").unwrap().footprint, "R_0402_1005Metric");
  assert_eq!(lib.device("R_Small").unwrap().prefix, "R");
  let op = lib.symbol("LM358").unwrap();
  assert_eq!(op.pins.len(), 6);
  // unit 2 sits beside unit 1
  assert!(op.pins.iter().find(|p| p.number == "7").unwrap().at[0] > 20.0);
  assert_eq!(op.pins.iter().find(|p| p.number == "2").unwrap().kind, crate::PinType::Input);
  assert!(lib.symbol("GND").unwrap().power);
  assert!(lib.device("GND").unwrap().footprint.is_empty());
  let fp = kicad::import_footprint(KICAD_MOD).unwrap();
  assert_eq!(fp.name, "R_0402_1005Metric");
  assert_eq!(fp.pads.len(), 4);
  assert_eq!(fp.pads[0].at, [-0.51, -0.1]);
  assert_eq!(fp.pads[1].rot, -90.0);
  assert_eq!(fp.pads[2].drill, Some(1.0));
  assert!(fp.pads[3].npth);
  assert_eq!(fp.courtyard(), ([-0.93, -0.47], [0.93, 0.47]));
  assert!(matches!(fp.model, Some(crate::Model3d::File { .. })));
  // used in a design: the imported symbol finds the imported footprint
  let mut pr = Project::new("k");
  let mut both = lib.clone();
  both.footprints.push(fp);
  pr.libraries.push(both);
  pr.add_part("R_Small", [0.0, 0.0], None, None).unwrap();
  assert_eq!(pr.board.placements.len(), 1);
  assert_eq!(pr.pads().len(), 4);
  assert!(kicad::parse("(a (b").is_err());
  assert!(kicad::import_footprint(KICAD_SYM).is_err());
}

#[test]
fn project_round_trips_through_json() {
  let pr = example::usb_mcu_board().unwrap();
  let text = serde_json::to_string(&pr).unwrap();
  let back: Project = serde_json::from_str(&text).unwrap();
  assert_eq!(back, pr);
}

#[test]
fn silkscreen_font() {
  assert!(crate::font::strokes("R10", 1.0).len() > 8);
  assert_eq!(crate::font::width("AB", 1.0), 1.25);
}

#[test]
fn svg_pictures() {
  let pr = example::usb_mcu_board().unwrap();
  let s = crate::svg::schematic(&pr);
  assert!(s.starts_with("<svg") && s.contains("ATmega328P") && s.contains("SCK"));
  let b = crate::svg::board(&pr);
  assert!(b.starts_with("<svg") && b.contains("<polyline"));
  if let Ok(dir) = std::env::var("FOX_ECAD_SVG_DIR") {
    std::fs::write(format!("{dir}/schematic.svg"), s).unwrap();
    std::fs::write(format!("{dir}/board.svg"), b).unwrap();
  }
}
