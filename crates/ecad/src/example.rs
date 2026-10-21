//! The example board: a USB-C powered ATmega328P with a 3.3 V regulator, a status LED and a
//! programming header, drawn EAGLE-style with net labels and power symbols, placed, routed and
//! poured.

use crate::board::{Layer, Pour, Side};
use crate::geom::{P, add, rot};
use crate::project::Project;
use crate::schematic::{Label, Wire};

const POWER: [&str; 3] = ["GND", "+5V", "+3V3"];

/// A short wire from a pin with a net label, power symbol or no-connect marker at its end.
pub fn stub(pr: &mut Project, refdes: &str, pin: &str, net: &str) -> Result<(), String> {
  let libs = pr.libs();
  let part = pr.schematic.parts.iter().find(|p| p.refdes == refdes).ok_or(format!("no part {refdes}"))?;
  let dev = libs.device(&part.device).ok_or(format!("{refdes}: unknown device"))?;
  let sym = libs.symbol(&dev.symbol).ok_or(format!("{refdes}: unknown symbol"))?;
  let p = sym.pins.iter().find(|p| p.number == pin).ok_or(format!("{refdes} has no pin {pin}"))?;
  let xf = part.xf();
  let at = xf.apply(p.at);
  let out = xf.angle(p.dir) + 180.0;
  drop(libs);
  if net == "nc" {
    pr.schematic.no_connects.push(at);
    return Ok(());
  }
  let end = add(at, rot([2.54, 0.0], out));
  pr.schematic.wires.push(Wire { a: at, b: end });
  if POWER.contains(&net) {
    let r = pr.next_refdes("#PWR");
    pr.add_part(net, end, Some(&r), Some(net))?;
    if let Some(sym) = pr.schematic.parts.last_mut() {
      sym.rot = (out + if net == "GND" { 90.0 } else { -90.0 }).rem_euclid(360.0);
    }
  } else {
    pr.schematic.labels.push(Label { name: net.into(), at: end });
  }
  Ok(())
}

fn place(pr: &mut Project, refdes: &str, at: P, rot: f64) {
  if let Some(p) = pr.board.placements.iter_mut().find(|p| p.refdes == refdes) {
    p.at = at;
    p.rot = rot;
    p.side = Side::Top;
  }
}

/// The schematic alone (parts, nets), with the footprints still beside the board.
pub fn usb_mcu_schematic() -> Result<Project, String> {
  let mut pr = Project::new("USB MCU board");
  pr.board.outline = vec![[0.0, 0.0], [50.0, 0.0], [50.0, 32.0], [0.0, 32.0]];
  let parts: [(&str, &str, &str, P); 14] = [
    ("J1", "USB_C_Receptacle", "USB-C", [-76.2, 0.0]),
    ("R1", "R_0603", "5.1k", [-50.8, -10.16]),
    ("R2", "R_0603", "5.1k", [-40.64, -10.16]),
    ("U1", "XC6206P332MR", "XC6206-3.3", [-25.4, 20.32]),
    ("C1", "C_0603", "1u", [-45.72, 20.32]),
    ("C2", "C_0603", "1u", [-7.62, 20.32]),
    ("U2", "ATmega328P-MU", "ATmega328P", [25.4, 0.0]),
    ("C3", "C_0603", "100n", [-5.08, -12.7]),
    ("C4", "C_0603", "100n", [2.54, -12.7]),
    ("R3", "R_0603", "10k", [-12.7, 5.08]),
    ("R4", "R_0603", "330", [60.96, 10.16]),
    ("D1", "LED_0603", "red", [71.12, 10.16]),
    ("J2", "PinHeader_1x06", "Conn_01x06", [76.2, -10.16]),
    ("C5", "C_0603", "100n", [10.16, -12.7]),
  ];
  for (r, d, v, at) in parts {
    pr.add_part(d, at, Some(r), Some(v))?;
  }
  let nets: &[(&str, &str, &str)] = &[
    ("J1", "VBUS", "+5V"),
    ("J1", "GND", "GND"),
    ("J1", "SHIELD", "GND"),
    ("J1", "CC1", "CC1"),
    ("J1", "CC2", "CC2"),
    ("J1", "D+", "nc"),
    ("J1", "D-", "nc"),
    ("J1", "SBU1", "nc"),
    ("J1", "SBU2", "nc"),
    ("R1", "1", "CC1"),
    ("R1", "2", "GND"),
    ("R2", "1", "CC2"),
    ("R2", "2", "GND"),
    ("U1", "3", "+5V"),
    ("U1", "1", "GND"),
    ("U1", "2", "+3V3"),
    ("C1", "1", "+5V"),
    ("C1", "2", "GND"),
    ("C2", "1", "+3V3"),
    ("C2", "2", "GND"),
    ("C3", "1", "+3V3"),
    ("C3", "2", "GND"),
    ("C4", "1", "+3V3"),
    ("C4", "2", "GND"),
    ("C5", "1", "+3V3"),
    ("C5", "2", "GND"),
    ("U2", "4", "+3V3"),
    ("U2", "6", "+3V3"),
    ("U2", "18", "+3V3"),
    ("U2", "3", "GND"),
    ("U2", "5", "GND"),
    ("U2", "21", "GND"),
    ("U2", "29", "RESET"),
    ("U2", "17", "SCK"),
    ("U2", "30", "RXD"),
    ("U2", "31", "TXD"),
    ("R3", "1", "+3V3"),
    ("R3", "2", "RESET"),
    ("R4", "1", "SCK"),
    ("R4", "2", "LED"),
    ("D1", "2", "LED"),
    ("D1", "1", "GND"),
    ("J2", "1", "GND"),
    ("J2", "2", "+3V3"),
    ("J2", "3", "RXD"),
    ("J2", "4", "TXD"),
    ("J2", "5", "RESET"),
    ("J2", "6", "SCK"),
  ];
  for (r, pin, net) in nets {
    stub(&mut pr, r, pin, net)?;
  }
  for pin in [
    "20", "19", "22", "7", "8", "12", "13", "14", "15", "16", "23", "24", "25", "26", "27", "28", "32", "1", "2", "9",
    "10", "11",
  ] {
    stub(&mut pr, "U2", pin, "nc")?;
  }
  Ok(pr)
}

/// The finished example: placed, autorouted, with ground pours on both sides.
pub fn usb_mcu_board() -> Result<Project, String> {
  let mut pr = usb_mcu_schematic()?;
  for (r, at, a) in [
    ("J1", [4.6, 16.0], 270.0),
    ("R1", [12.0, 24.0], 90.0),
    ("R2", [12.0, 8.0], 90.0),
    ("C1", [15.0, 16.0], 90.0),
    ("U1", [19.0, 23.0], 0.0),
    ("C2", [24.0, 26.0], 0.0),
    ("U2", [30.0, 14.0], 0.0),
    ("C3", [30.0, 21.0], 0.0),
    ("C4", [25.0, 9.0], 90.0),
    ("C5", [35.0, 7.0], 0.0),
    ("R3", [36.0, 22.0], 90.0),
    ("R4", [40.0, 27.0], 0.0),
    ("D1", [44.0, 27.0], 180.0),
    ("J2", [45.0, 20.0], 0.0),
  ] {
    place(&mut pr, r, at, a);
  }
  crate::router::autoroute(&mut pr, &[], 0.25);
  let inset = vec![[0.4, 0.4], [49.6, 0.4], [49.6, 31.6], [0.4, 31.6]];
  for layer in [Layer::Top, Layer::Bottom] {
    pr.board.pours.push(Pour { layer, net: "GND".into(), outline: inset.clone(), clearance: Some(0.3) });
  }
  Ok(pr)
}
