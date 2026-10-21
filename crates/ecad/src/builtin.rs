//! The starter library: passives, LEDs, headers, SOIC/QFN/SOT-23 ICs, a USB-C receptacle, an
//! MCU, and power symbols. Footprints follow the IPC/KiCad nominal land patterns.

use crate::geom::P;
use crate::library::{Device, FpGfx, FpLayer, Footprint, Gfx, Library, Model3d, Pad, PadShape, Pin, PinType, Symbol};

pub const NAME: &str = "fox";

const G: f64 = 2.54;

fn line(a: P, b: P) -> Gfx {
  Gfx::Line { a, b, width: 0.254 }
}

fn pin(number: &str, name: &str, at: P, dir: f64, kind: PinType) -> Pin {
  Pin { number: number.into(), name: name.into(), at, dir, length: G, kind, hidden: false }
}

/// A vertical two-pin symbol: pin 1 on top, pin 2 at the bottom, `body` drawn between.
fn two_pin(name: &str, names: [&str; 2], body: Vec<Gfx>, kind: PinType) -> Symbol {
  Symbol {
    name: name.into(),
    pins: vec![pin("1", names[0], [0.0, 2.0 * G], 270.0, kind), pin("2", names[1], [0.0, -2.0 * G], 90.0, kind)],
    gfx: body,
    power: false,
  }
}

fn resistor_sym() -> Symbol {
  two_pin("R", ["~", "~"], vec![Gfx::Rect { a: [-0.9, -G], b: [0.9, G], width: 0.254, fill: false }], PinType::Passive)
}

fn capacitor_sym() -> Symbol {
  two_pin(
    "C",
    ["~", "~"],
    vec![line([-1.5, 0.5], [1.5, 0.5]), line([-1.5, -0.5], [1.5, -0.5]), line([0.0, G], [0.0, 0.5]), line([0.0, -G], [0.0, -0.5])],
    PinType::Passive,
  )
}

fn led_sym() -> Symbol {
  // anode (pin 2) on top, cathode (pin 1) below: current flows down
  let mut s = two_pin(
    "LED",
    ["K", "A"],
    vec![
      Gfx::Poly { pts: vec![[-1.2, 0.9], [1.2, 0.9], [0.0, -0.9]], width: 0.254, fill: true },
      line([-1.2, -0.9], [1.2, -0.9]),
      line([0.0, G], [0.0, 0.9]),
      line([0.0, -G], [0.0, -0.9]),
      line([1.6, 0.4], [2.6, 1.2]),
      line([1.6, -0.3], [2.6, 0.5]),
    ],
    PinType::Passive,
  );
  s.pins[0].at = [0.0, -2.0 * G];
  s.pins[0].dir = 90.0;
  s.pins[1].at = [0.0, 2.0 * G];
  s.pins[1].dir = 270.0;
  s
}

/// A box symbol: pins down the left and right sides, top to bottom.
fn ic_sym(name: &str, left: &[(&str, &str, PinType)], right: &[(&str, &str, PinType)], width: f64) -> Symbol {
  let rows = left.len().max(right.len()) as f64;
  let top = ((rows - 1.0) / 2.0).ceil() * G;
  let hw = width / 2.0;
  let mut pins = Vec::new();
  for (i, (n, nm, k)) in left.iter().enumerate() {
    pins.push(pin(n, nm, [-hw - G, top - i as f64 * G], 0.0, *k));
  }
  for (i, (n, nm, k)) in right.iter().enumerate() {
    pins.push(pin(n, nm, [hw + G, top - i as f64 * G], 180.0, *k));
  }
  let bottom = top - (rows - 1.0) * G;
  Symbol {
    name: name.into(),
    pins,
    gfx: vec![Gfx::Rect { a: [-hw, bottom - G], b: [hw, top + G], width: 0.254, fill: false }],
    power: false,
  }
}

fn header_sym(n: usize) -> Symbol {
  let pins = (0..n).map(|i| pin(&(i + 1).to_string(), &format!("Pin_{}", i + 1), [-2.0 * G, -(i as f64) * G], 0.0, PinType::Passive));
  Symbol {
    name: format!("Conn_01x{n:02}"),
    pins: pins.collect(),
    gfx: vec![Gfx::Rect { a: [-G, -(n as f64 - 1.0) * G - 1.27], b: [1.27, 1.27], width: 0.254, fill: false }],
    power: false,
  }
}

fn power_sym(name: &str, ground: bool) -> Symbol {
  let gfx = if ground {
    vec![line([0.0, 0.0], [0.0, -1.27]), line([-1.27, -1.27], [1.27, -1.27]), line([-0.8, -1.8], [0.8, -1.8]), line([-0.3, -2.3], [0.3, -2.3])]
  } else {
    vec![line([0.0, 0.0], [0.0, 1.27]), line([-1.27, 1.27], [1.27, 1.27])]
  };
  Symbol {
    name: name.into(),
    pins: vec![Pin { number: "1".into(), name: name.into(), at: [0.0, 0.0], dir: 90.0, length: 0.0, kind: PinType::PowerOut, hidden: false }],
    gfx,
    power: true,
  }
}

fn npn_sym() -> Symbol {
  Symbol {
    name: "Q_NPN_BEC".into(),
    pins: vec![
      pin("1", "B", [-2.0 * G, 0.0], 0.0, PinType::Passive),
      pin("2", "E", [G, -2.0 * G], 90.0, PinType::Passive),
      pin("3", "C", [G, 2.0 * G], 270.0, PinType::Passive),
    ],
    gfx: vec![
      line([-0.6, -1.6], [-0.6, 1.6]),
      line([-G, 0.0], [-0.6, 0.0]),
      line([-0.6, 0.6], [G, G]),
      line([-0.6, -0.6], [G, -G]),
      Gfx::Poly { pts: vec![[G, -G], [1.4, -1.6], [1.9, -0.9]], width: 0.15, fill: true },
      Gfx::Circle { c: [0.6, 0.0], r: 2.8, width: 0.254, fill: false },
    ],
    power: false,
  }
}

fn silk_rect(lo: P, hi: P) -> Vec<FpGfx> {
  vec![FpGfx { layer: FpLayer::Silk, gfx: Gfx::Rect { a: lo, b: hi, width: 0.12, fill: false } }]
}

fn courtyard(lo: P, hi: P) -> FpGfx {
  FpGfx { layer: FpLayer::Courtyard, gfx: Gfx::Rect { a: lo, b: hi, width: 0.05, fill: false } }
}

fn smd(number: &str, at: P, size: P) -> Pad {
  Pad { number: number.into(), shape: PadShape::RoundRect, at, size, rot: 0.0, drill: None, npth: false }
}

/// A two-terminal chip (0603, 0805, 1206): pad centres ±`x`, pad `size`, body `body` (l, w, h).
fn chip(name: &str, x: f64, size: P, body: [f64; 3], color: [f32; 3]) -> Footprint {
  let (bl, bw) = (body[0] / 2.0, body[1] / 2.0);
  let cy = bw.max(size[1] / 2.0) + 0.25;
  let cx = x + size[0] / 2.0 + 0.25;
  Footprint {
    name: name.into(),
    pads: vec![smd("1", [-x, 0.0], size), smd("2", [x, 0.0], size)],
    gfx: vec![
      FpGfx { layer: FpLayer::Silk, gfx: Gfx::Line { a: [-bl * 0.3, bw + 0.15], b: [bl * 0.3, bw + 0.15], width: 0.12 } },
      FpGfx { layer: FpLayer::Silk, gfx: Gfx::Line { a: [-bl * 0.3, -bw - 0.15], b: [bl * 0.3, -bw - 0.15], width: 0.12 } },
      courtyard([-cx, -cy], [cx, cy]),
    ],
    model: Some(Model3d::Box { size: body, offset: [0.0, 0.0], color: Some(color) }),
  }
}

fn header_fp(n: usize) -> Footprint {
  let pads = (0..n)
    .map(|i| Pad {
      number: (i + 1).to_string(),
      shape: if i == 0 { PadShape::Rect } else { PadShape::Oval },
      at: [0.0, -(i as f64) * G],
      size: [1.7, 1.7],
      rot: 0.0,
      drill: Some(1.0),
      npth: false,
    })
    .collect();
  let len = n as f64 * G;
  Footprint {
    name: format!("PinHeader_1x{n:02}_P2.54mm_Vertical"),
    pads,
    gfx: [silk_rect([-1.33, -len + 1.27 - 0.06], [1.33, 1.33]), vec![courtyard([-1.8, -len + 1.27 - 0.5], [1.8, 1.8])]].concat(),
    model: Some(Model3d::Box { size: [G, len, 8.5], offset: [0.0, -(len - G) / 2.0], color: Some([0.1, 0.1, 0.1]) }),
  }
}

/// Gull-wing package with pins on two sides, 1 at top left, counter-clockwise.
fn dual_row(name: &str, n: usize, pitch: f64, x: f64, pad: P, body: [f64; 3]) -> Footprint {
  let per = n / 2;
  let top = (per as f64 - 1.0) * pitch / 2.0;
  let mut pads = Vec::new();
  for i in 0..per {
    pads.push(smd(&(i + 1).to_string(), [-x, top - i as f64 * pitch], pad));
  }
  for i in 0..per {
    pads.push(smd(&(per + i + 1).to_string(), [x, -top + i as f64 * pitch], pad));
  }
  let (bx, by) = (body[0] / 2.0, body[1] / 2.0);
  let mut gfx = silk_rect([-bx, -by], [bx, by]);
  gfx.push(FpGfx { layer: FpLayer::Silk, gfx: Gfx::Circle { c: [-x - pad[0] / 2.0, top + pad[1] + 0.3], r: 0.15, width: 0.12, fill: true } });
  let (cx, cy) = (x + pad[0] / 2.0 + 0.25, by.max(top + pad[1] / 2.0) + 0.25);
  gfx.push(courtyard([-cx, -cy], [cx, cy]));
  Footprint { name: name.into(), pads, gfx, model: Some(Model3d::Box { size: body, offset: [0.0, 0.0], color: Some([0.12, 0.12, 0.12]) }) }
}

fn sot23() -> Footprint {
  let pad = [1.325, 0.6];
  Footprint {
    name: "SOT-23".into(),
    pads: vec![smd("1", [-1.1375, 0.95], pad), smd("2", [-1.1375, -0.95], pad), smd("3", [1.1375, 0.0], pad)],
    gfx: vec![
      FpGfx { layer: FpLayer::Silk, gfx: Gfx::Line { a: [-0.76, 1.56], b: [0.76, 1.56], width: 0.12 } },
      FpGfx { layer: FpLayer::Silk, gfx: Gfx::Line { a: [-0.76, -1.56], b: [0.76, -1.56], width: 0.12 } },
      courtyard([-2.05, -1.7], [2.05, 1.7]),
    ],
    model: Some(Model3d::Box { size: [1.3, 2.9, 1.0], offset: [0.0, 0.0], color: Some([0.12, 0.12, 0.12]) }),
  }
}

/// QFN with `n` pins (n/4 per side) and an exposed pad numbered n+1.
fn qfn(name: &str, n: usize, pitch: f64, size: f64, ep: f64) -> Footprint {
  let per = n / 4;
  let first = (per as f64 - 1.0) * pitch / 2.0;
  let x = size / 2.0 - 0.05;
  let pl = [0.8, 0.25];
  let mut pads = Vec::new();
  let mut k = 1;
  for side in 0..4 {
    for i in 0..per {
      let t = first - i as f64 * pitch;
      let (at, sz) = match side {
        0 => ([-x, t], pl),
        1 => ([-t, -x], [pl[1], pl[0]]),
        2 => ([x, -t], pl),
        _ => ([t, x], [pl[1], pl[0]]),
      };
      pads.push(smd(&k.to_string(), at, sz));
      k += 1;
    }
  }
  pads.push(smd(&k.to_string(), [0.0, 0.0], [ep, ep]));
  let h = size / 2.0;
  let mut gfx = vec![];
  for (a, b) in [([-h, h - 0.6], [-h, h]), ([-h, h], [-h + 0.6, h]), ([h - 0.6, h], [h, h]), ([h, h], [h, h - 0.6])] {
    gfx.push(FpGfx { layer: FpLayer::Silk, gfx: Gfx::Line { a, b, width: 0.12 } });
  }
  for (a, b) in [([-h, -h + 0.6], [-h, -h]), ([-h, -h], [-h + 0.6, -h]), ([h - 0.6, -h], [h, -h]), ([h, -h], [h, -h + 0.6])] {
    gfx.push(FpGfx { layer: FpLayer::Silk, gfx: Gfx::Line { a, b, width: 0.12 } });
  }
  gfx.push(FpGfx { layer: FpLayer::Silk, gfx: Gfx::Circle { c: [-h - 0.5, h + 0.5], r: 0.15, width: 0.12, fill: true } });
  gfx.push(courtyard([-h - 0.6, -h - 0.6], [h + 0.6, h + 0.6]));
  Footprint { name: name.into(), pads, gfx, model: Some(Model3d::Box { size: [size, size, 0.9], offset: [0.0, 0.0], color: Some([0.12, 0.12, 0.12]) }) }
}

fn usb_c_fp() -> Footprint {
  let mut pads = Vec::new();
  let y = 2.5;
  for (num, x, w) in [
    ("A1B12", -3.25, 0.6),
    ("A4B9", -2.45, 0.6),
    ("B8", -1.75, 0.3),
    ("A5", -1.25, 0.3),
    ("B7", -0.75, 0.3),
    ("A6", -0.25, 0.3),
    ("A7", 0.25, 0.3),
    ("B6", 0.75, 0.3),
    ("A8", 1.25, 0.3),
    ("B5", 1.75, 0.3),
    ("B4A9", 2.45, 0.6),
    ("B1A12", 3.25, 0.6),
  ] {
    pads.push(Pad { number: num.into(), shape: PadShape::Rect, at: [x, y], size: [w, 1.3], rot: 0.0, drill: None, npth: false });
  }
  for (x, yy, h) in [(-4.32, 1.95, 2.1), (4.32, 1.95, 2.1), (-4.32, -2.2, 1.8), (4.32, -2.2, 1.8)] {
    pads.push(Pad { number: "S1".into(), shape: PadShape::Oval, at: [x, yy], size: [1.0, h], rot: 0.0, drill: Some(0.65), npth: false });
  }
  for x in [-2.89, 2.89] {
    pads.push(Pad { number: String::new(), shape: PadShape::Circle, at: [x, 1.45], size: [0.65, 0.65], rot: 0.0, drill: Some(0.65), npth: true });
  }
  Footprint {
    name: "USB_C_Receptacle_HRO_TYPE-C-31-M-12".into(),
    pads,
    gfx: vec![
      FpGfx { layer: FpLayer::Silk, gfx: Gfx::Line { a: [-4.7, -4.6], b: [4.7, -4.6], width: 0.12 } },
      courtyard([-5.3, -5.1], [5.3, 3.65]),
    ],
    model: Some(Model3d::Box { size: [8.94, 7.3, 3.2], offset: [0.0, -1.0], color: Some([0.75, 0.75, 0.78]) }),
  }
}

fn led_tht_fp() -> Footprint {
  Footprint {
    name: "LED_D5.0mm".into(),
    pads: vec![
      Pad { number: "1".into(), shape: PadShape::Rect, at: [0.0, 0.0], size: [1.8, 1.8], rot: 0.0, drill: Some(0.9), npth: false },
      Pad { number: "2".into(), shape: PadShape::Circle, at: [G, 0.0], size: [1.8, 1.8], rot: 0.0, drill: Some(0.9), npth: false },
    ],
    gfx: vec![
      FpGfx { layer: FpLayer::Silk, gfx: Gfx::Arc { c: [1.27, 0.0], r: 3.0, start: 30.0, end: 330.0, width: 0.12 } },
      courtyard([-1.95, -3.25], [4.5, 3.25]),
    ],
    model: Some(Model3d::Cylinder { diameter: 5.0, height: 8.6, offset: [1.27, 0.0], color: Some([0.9, 0.1, 0.1]) }),
  }
}

fn dev(name: &str, desc: &str, prefix: &str, value: &str, symbol: &str, footprint: &str) -> Device {
  Device {
    name: name.into(),
    description: desc.into(),
    prefix: prefix.into(),
    value: value.into(),
    symbol: symbol.into(),
    footprint: footprint.into(),
    model: None,
    pinmap: Vec::new(),
    manufacturer: String::new(),
    mpn: String::new(),
  }
}

fn map(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
  pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

/// The starter library.
pub fn library() -> Library {
  use PinType::*;
  let mut symbols = vec![resistor_sym(), capacitor_sym(), led_sym(), npn_sym()];
  for (n, gnd) in [("GND", true), ("VCC", false), ("+5V", false), ("+3V3", false), ("VBUS", false)] {
    symbols.push(power_sym(n, gnd));
  }
  for n in 1..=10 {
    symbols.push(header_sym(n));
  }
  symbols.push(ic_sym(
    "NE555",
    &[("2", "TRIG", Input), ("4", "R", Input), ("5", "CV", Input), ("6", "THR", Input)],
    &[("8", "VCC", PowerIn), ("3", "Q", Output), ("7", "DIS", OpenCollector), ("1", "GND", PowerIn)],
    10.16,
  ));
  symbols.push(ic_sym("LDO_GND_OUT_IN", &[("3", "VIN", PowerIn), ("1", "GND", PowerIn)], &[("2", "VOUT", PowerOut)], 7.62));
  symbols.push(ic_sym(
    "USB_C_Receptacle_USB2.0",
    &[("VBUS", "VBUS", Passive), ("CC1", "CC1", Bidirectional), ("CC2", "CC2", Bidirectional), ("D+", "D+", Bidirectional), ("D-", "D-", Bidirectional), ("SBU1", "SBU1", Bidirectional), ("SBU2", "SBU2", Bidirectional), ("GND", "GND", Passive), ("SHIELD", "SHIELD", Passive)],
    &[],
    10.16,
  ));
  let io = Bidirectional;
  symbols.push(ic_sym(
    "ATmega328P-M",
    &[
      ("29", "~RESET~/PC6", Input),
      ("4", "VCC", PowerIn),
      ("6", "VCC", PowerIn),
      ("18", "AVCC", PowerIn),
      ("20", "AREF", Passive),
      ("3", "GND", PowerIn),
      ("5", "GND", PowerIn),
      ("21", "GND", PowerIn),
      ("19", "ADC6", Input),
      ("22", "ADC7", Input),
      ("7", "XTAL1/PB6", io),
      ("8", "XTAL2/PB7", io),
    ],
    &[
      ("12", "PB0", io),
      ("13", "PB1", io),
      ("14", "PB2", io),
      ("15", "PB3/MOSI", io),
      ("16", "PB4/MISO", io),
      ("17", "PB5/SCK", io),
      ("23", "PC0", io),
      ("24", "PC1", io),
      ("25", "PC2", io),
      ("26", "PC3", io),
      ("27", "PC4/SDA", io),
      ("28", "PC5/SCL", io),
      ("30", "PD0/RXD", io),
      ("31", "PD1/TXD", io),
      ("32", "PD2", io),
      ("1", "PD3", io),
      ("2", "PD4", io),
      ("9", "PD5", io),
      ("10", "PD6", io),
      ("11", "PD7", io),
    ],
    15.24,
  ));

  let red = [0.85, 0.85, 0.8];
  let tan = [0.7, 0.55, 0.35];
  let mut footprints = vec![
    chip("R_0603_1608Metric", 0.825, [0.8, 0.95], [1.6, 0.8, 0.45], [0.1, 0.1, 0.1]),
    chip("R_0805_2012Metric", 0.9125, [1.025, 1.4], [2.0, 1.25, 0.5], [0.1, 0.1, 0.1]),
    chip("C_0603_1608Metric", 0.775, [0.9, 0.95], [1.6, 0.8, 0.8], tan),
    chip("C_0805_2012Metric", 0.95, [1.0, 1.45], [2.0, 1.25, 1.0], tan),
    chip("C_1206_3216Metric", 1.5, [1.15, 1.8], [3.2, 1.6, 1.2], tan),
    chip("LED_0603_1608Metric", 0.7875, [0.875, 0.95], [1.6, 0.8, 0.6], red),
    led_tht_fp(),
    dual_row("SOIC-8_3.9x4.9mm_P1.27mm", 8, 1.27, 2.475, [1.95, 0.6], [3.9, 4.9, 1.75]),
    dual_row("SOIC-14_3.9x8.7mm_P1.27mm", 14, 1.27, 2.475, [1.95, 0.6], [3.9, 8.7, 1.75]),
    sot23(),
    qfn("QFN-32-1EP_5x5mm_P0.5mm", 32, 0.5, 5.0, 3.45),
    usb_c_fp(),
  ];
  for n in 1..=10 {
    footprints.push(header_fp(n));
  }

  let mut devices = vec![
    dev("R_0603", "Resistor, 0603", "R", "10k", "R", "R_0603_1608Metric"),
    dev("R_0805", "Resistor, 0805", "R", "10k", "R", "R_0805_2012Metric"),
    dev("C_0603", "Ceramic capacitor, 0603", "C", "100n", "C", "C_0603_1608Metric"),
    dev("C_0805", "Ceramic capacitor, 0805", "C", "1u", "C", "C_0805_2012Metric"),
    dev("C_1206", "Ceramic capacitor, 1206", "C", "10u", "C", "C_1206_3216Metric"),
    dev("LED_0603", "LED, 0603", "D", "red", "LED", "LED_0603_1608Metric"),
    dev("LED_5mm", "LED, 5 mm through-hole", "D", "red", "LED", "LED_D5.0mm"),
    Device { mpn: "NE555DR".into(), manufacturer: "Texas Instruments".into(), ..dev("NE555D", "555 timer, SOIC-8", "U", "NE555D", "NE555", "SOIC-8_3.9x4.9mm_P1.27mm") },
    Device { mpn: "XC6206P332MR".into(), manufacturer: "Torex".into(), ..dev("XC6206P332MR", "3.3 V 200 mA LDO regulator, SOT-23", "U", "XC6206-3.3", "LDO_GND_OUT_IN", "SOT-23") },
    Device { mpn: "MMBT3904".into(), manufacturer: "onsemi".into(), ..dev("MMBT3904", "NPN transistor, SOT-23", "Q", "MMBT3904", "Q_NPN_BEC", "SOT-23") },
    Device {
      pinmap: map(&[("21", "21"), ("21", "33")]),
      mpn: "ATMEGA328P-MU".into(),
      manufacturer: "Microchip".into(),
      ..dev("ATmega328P-MU", "8-bit AVR MCU, 32 KB flash, QFN-32", "U", "ATmega328P", "ATmega328P-M", "QFN-32-1EP_5x5mm_P0.5mm")
    },
    Device {
      pinmap: map(&[
        ("VBUS", "A4B9"),
        ("VBUS", "B4A9"),
        ("GND", "A1B12"),
        ("GND", "B1A12"),
        ("CC1", "A5"),
        ("CC2", "B5"),
        ("D+", "A6"),
        ("D+", "B6"),
        ("D-", "A7"),
        ("D-", "B7"),
        ("SBU1", "A8"),
        ("SBU2", "B8"),
        ("SHIELD", "S1"),
      ]),
      mpn: "TYPE-C-31-M-12".into(),
      manufacturer: "Korean Hroparts".into(),
      ..dev("USB_C_Receptacle", "USB 2.0 Type-C receptacle, SMD", "J", "USB-C", "USB_C_Receptacle_USB2.0", "USB_C_Receptacle_HRO_TYPE-C-31-M-12")
    },
  ];
  for n in 1..=10 {
    devices.push(dev(&format!("PinHeader_1x{n:02}"), &format!("Pin header, 1x{n}, 2.54 mm"), "J", &format!("Conn_01x{n:02}"), &format!("Conn_01x{n:02}"), &format!("PinHeader_1x{n:02}_P2.54mm_Vertical")));
  }
  for (n, _) in [("GND", 0), ("VCC", 0), ("+5V", 0), ("+3V3", 0), ("VBUS", 0)] {
    devices.push(dev(n, &format!("Power symbol: names its net {n}"), "#PWR", n, n, ""));
  }
  Library { name: NAME.into(), symbols, footprints, devices }
}
