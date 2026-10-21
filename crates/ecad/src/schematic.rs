//! Schematics: placed symbols, wires, net labels, buses and power symbols; the netlist they
//! make, and the electrical rules check.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::geom::{P, Xf, near, point_seg_dist};
use crate::library::{Libs, PinType};
use crate::{Severity, Violation};

/// A part on the schematic: a library device, named by its reference designator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchPart {
  pub refdes: String,
  pub device: String,
  #[serde(default)]
  pub value: String,
  pub at: P,
  #[serde(default)]
  pub rot: f64,
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub mirror: bool,
}

impl SchPart {
  pub fn xf(&self) -> Xf {
    Xf { at: self.at, rot: self.rot, mirror: self.mirror }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wire {
  pub a: P,
  pub b: P,
}

/// A net label: gives the wire it sits on a name; wires with the same name are connected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
  pub name: String,
  pub at: P,
}

/// A bus: a drawn polyline carrying the named signals (`D[0..7]`, `SDA,SCL`). It doesn't
/// connect anything by itself: wires leaving it take a member's name from their label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bus {
  pub name: String,
  pub pts: Vec<P>,
}

impl Bus {
  pub fn members(&self) -> Vec<String> {
    bus_members(&self.name)
  }

  fn touches(&self, p: P) -> bool {
    self.pts.windows(2).any(|w| point_seg_dist(p, w[0], w[1]) < 1e-4)
  }
}

/// `D[0..3],SDA` → D0 D1 D2 D3 SDA.
pub fn bus_members(name: &str) -> Vec<String> {
  let mut out = Vec::new();
  for part in name.split(',').map(str::trim).filter(|s| !s.is_empty()) {
    if let (Some(o), true) = (part.find('['), part.ends_with(']'))
      && let Some((a, b)) = part[o + 1..part.len() - 1].split_once("..")
      && let (Ok(a), Ok(b)) = (a.trim().parse::<i64>(), b.trim().parse::<i64>())
    {
      let base = &part[..o];
      let (lo, hi) = (a.min(b), a.max(b));
      let mut v: Vec<String> = (lo..=hi).map(|i| format!("{base}{i}")).collect();
      if a > b {
        v.reverse();
      }
      out.extend(v);
      continue;
    }
    out.push(part.to_string());
  }
  out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Schematic {
  #[serde(default)]
  pub parts: Vec<SchPart>,
  #[serde(default)]
  pub wires: Vec<Wire>,
  #[serde(default)]
  pub labels: Vec<Label>,
  #[serde(default)]
  pub buses: Vec<Bus>,
  /// No-connect markers: the pin under one is meant to stay unconnected.
  #[serde(default)]
  pub no_connects: Vec<P>,
}

/// A pin on a net.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NetPin {
  pub refdes: String,
  pub pin: String,
  #[serde(default)]
  pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Net {
  pub name: String,
  pub pins: Vec<NetPin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Netlist {
  pub nets: Vec<Net>,
}

impl Netlist {
  /// The net a part's pin is on.
  pub fn net_of(&self, refdes: &str, pin: &str) -> Option<&str> {
    self.nets.iter().find(|n| n.pins.iter().any(|p| p.refdes == refdes && p.pin == pin)).map(|n| n.name.as_str())
  }

  pub fn net(&self, name: &str) -> Option<&Net> {
    self.nets.iter().find(|n| n.name == name)
  }
}

struct Uf(Vec<usize>);

impl Uf {
  fn new(n: usize) -> Uf {
    Uf((0..n).collect())
  }
  fn find(&mut self, i: usize) -> usize {
    let mut r = i;
    while self.0[r] != r {
      r = self.0[r];
    }
    let mut i = i;
    while self.0[i] != r {
      let n = self.0[i];
      self.0[i] = r;
      i = n;
    }
    r
  }
  fn union(&mut self, a: usize, b: usize) {
    let (a, b) = (self.find(a), self.find(b));
    if a != b {
      self.0[a.max(b)] = a.min(b);
    }
  }
}

/// A pin as placed on the sheet.
#[derive(Debug, Clone)]
pub struct PlacedPin {
  pub part: usize,
  pub refdes: String,
  pub number: String,
  pub name: String,
  pub kind: PinType,
  pub at: P,
  pub hidden: bool,
  /// The part is a power symbol.
  pub power: bool,
}

/// The schematic's pins in sheet coordinates (parts whose device is missing are skipped).
pub fn placed_pins(sch: &Schematic, libs: &Libs) -> Vec<PlacedPin> {
  let mut out = Vec::new();
  for (i, part) in sch.parts.iter().enumerate() {
    let Some(sym) = libs.device(&part.device).and_then(|d| libs.symbol(&d.symbol)) else { continue };
    let xf = part.xf();
    for p in &sym.pins {
      out.push(PlacedPin {
        part: i,
        refdes: part.refdes.clone(),
        number: p.number.clone(),
        name: p.name.clone(),
        kind: p.kind,
        at: xf.apply(p.at),
        hidden: p.hidden,
        power: sym.power,
      });
    }
  }
  out
}

/// Everything connectivity analysis finds.
pub struct Connectivity {
  pub pins: Vec<PlacedPin>,
  /// Net name per group, groups indexed as `group_of_*`.
  pub names: Vec<String>,
  /// Every name given to each group (labels, power symbols).
  pub given: Vec<Vec<String>>,
  pub pin_group: Vec<usize>,
  pub wire_group: Vec<usize>,
  pub label_group: Vec<usize>,
}

pub fn connectivity(sch: &Schematic, libs: &Libs) -> Connectivity {
  let pins = placed_pins(sch, libs);
  let (nw, np, nl) = (sch.wires.len(), pins.len(), sch.labels.len());
  // nodes: wires, pins, labels, then one per hidden-pin net name
  let mut hidden_names: Vec<String> = Vec::new();
  for p in pins.iter().filter(|p| p.hidden) {
    if !hidden_names.contains(&p.name) {
      hidden_names.push(p.name.clone());
    }
  }
  let mut uf = Uf::new(nw + np + nl + hidden_names.len());
  let on_wire = |p: P, w: &Wire| point_seg_dist(p, w.a, w.b) < 1e-4;
  for i in 0..nw {
    for j in 0..nw {
      if i != j && (on_wire(sch.wires[i].a, &sch.wires[j]) || on_wire(sch.wires[i].b, &sch.wires[j])) {
        uf.union(i, j);
      }
    }
  }
  for (k, p) in pins.iter().enumerate() {
    if p.hidden {
      let h = hidden_names.iter().position(|n| *n == p.name).unwrap_or(0);
      uf.union(nw + k, nw + np + nl + h);
      continue;
    }
    for (i, w) in sch.wires.iter().enumerate() {
      if near(p.at, w.a) || near(p.at, w.b) {
        uf.union(nw + k, i);
      }
    }
    for (k2, q) in pins.iter().enumerate().skip(k + 1) {
      if !q.hidden && near(p.at, q.at) {
        uf.union(nw + k, nw + k2);
      }
    }
  }
  for (l, lab) in sch.labels.iter().enumerate() {
    for (i, w) in sch.wires.iter().enumerate() {
      if on_wire(lab.at, w) {
        uf.union(nw + np + l, i);
      }
    }
    for (k, p) in pins.iter().enumerate() {
      if !p.hidden && near(lab.at, p.at) {
        uf.union(nw + np + l, nw + k);
      }
    }
  }
  // names: labels, power symbol values, hidden power pins
  let mut given: HashMap<usize, Vec<String>> = HashMap::new();
  for (l, lab) in sch.labels.iter().enumerate() {
    let r = uf.find(nw + np + l);
    given.entry(r).or_default().push(lab.name.clone());
  }
  for (k, p) in pins.iter().enumerate() {
    if p.power {
      let r = uf.find(nw + k);
      let v = &sch.parts[p.part].value;
      given.entry(r).or_default().push(if v.is_empty() { p.name.clone() } else { v.clone() });
    }
  }
  for (h, n) in hidden_names.iter().enumerate() {
    let r = uf.find(nw + np + nl + h);
    given.entry(r).or_default().push(n.clone());
  }
  // groups with a name in common are one net
  let mut by_name: HashMap<String, usize> = HashMap::new();
  let roots: Vec<usize> = given.keys().copied().collect();
  for r in roots {
    for n in given[&r].clone() {
      if let Some(&o) = by_name.get(&n) {
        uf.union(o, r);
      } else {
        by_name.insert(n, r);
      }
    }
  }
  let total = nw + np + nl + hidden_names.len();
  let mut gid: HashMap<usize, usize> = HashMap::new();
  let mut group = vec![0; total];
  for (i, g) in group.iter_mut().enumerate() {
    let r = uf.find(i);
    let n = gid.len();
    *g = *gid.entry(r).or_insert(n);
  }
  let mut gnames: Vec<Vec<String>> = vec![Vec::new(); gid.len()];
  for (r, names) in &given {
    let g = group[*r];
    for n in names {
      if !gnames[g].contains(n) {
        gnames[g].push(n.clone());
      }
    }
  }
  // name each group: a power name first, then the alphabetically first label; else N$k in order
  // of the first pin on the net (parts sorted by reference designator)
  let mut names = vec![String::new(); gid.len()];
  for (g, ns) in gnames.iter().enumerate() {
    let mut ns = ns.clone();
    ns.sort();
    if let Some(n) = ns.first() {
      names[g] = n.clone();
    }
  }
  let mut order: Vec<usize> = (0..np).collect();
  order.sort_by(|&a, &b| natural(&pins[a].refdes, &pins[b].refdes).then(natural(&pins[a].number, &pins[b].number)));
  let mut k = 1;
  for i in order {
    let g = group[nw + i];
    if names[g].is_empty() {
      names[g] = format!("N${k}");
      k += 1;
    }
  }
  Connectivity {
    pin_group: (0..np).map(|i| group[nw + i]).collect(),
    wire_group: (0..nw).map(|i| group[i]).collect(),
    label_group: (0..nl).map(|i| group[nw + np + i]).collect(),
    names,
    given: gnames,
    pins,
  }
}

/// Orders "R2" before "R10".
pub fn natural(a: &str, b: &str) -> std::cmp::Ordering {
  let split = |s: &str| {
    let i = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let (p, n) = s.split_at(i);
    let num: Option<u64> = n.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok();
    (p.to_string(), num, n.to_string())
  };
  split(a).cmp(&split(b))
}

/// The netlist: every net with at least one real (non-power-symbol) pin.
pub fn netlist(sch: &Schematic, libs: &Libs) -> Netlist {
  let c = connectivity(sch, libs);
  let mut nets: BTreeMap<String, Vec<NetPin>> = BTreeMap::new();
  for (i, p) in c.pins.iter().enumerate() {
    if p.power {
      continue;
    }
    nets.entry(c.names[c.pin_group[i]].clone()).or_default().push(NetPin {
      refdes: p.refdes.clone(),
      pin: p.number.clone(),
      name: p.name.clone(),
    });
  }
  let mut out: Vec<Net> = nets
    .into_iter()
    .map(|(name, mut pins)| {
      pins.sort_by(|a, b| natural(&a.refdes, &b.refdes).then(natural(&a.pin, &b.pin)));
      pins.dedup();
      Net { name, pins }
    })
    .collect();
  out.sort_by(|a, b| natural(&a.name, &b.name));
  Netlist { nets: out }
}

fn v(code: &str, severity: Severity, message: String, at: Option<P>) -> Violation {
  Violation { code: code.into(), severity, message, at, layer: None }
}

/// The electrical rules check.
pub fn erc(sch: &Schematic, libs: &Libs) -> Vec<Violation> {
  use PinType::*;
  let mut out = Vec::new();
  let mut seen: HashMap<&str, usize> = HashMap::new();
  for p in &sch.parts {
    *seen.entry(p.refdes.as_str()).or_default() += 1;
    if libs.device(&p.device).is_none() {
      out.push(v(
        "unknown_device",
        Severity::Error,
        format!("{}: no device '{}' in the libraries", p.refdes, p.device),
        Some(p.at),
      ));
    }
  }
  let mut dups: Vec<_> = seen.into_iter().filter(|(_, n)| *n > 1).collect();
  dups.sort();
  for (r, n) in dups {
    out.push(v("duplicate_refdes", Severity::Error, format!("{r} is used by {n} parts"), None));
  }
  let c = connectivity(sch, libs);
  let ng = c.names.len();
  let mut group_pins: Vec<Vec<usize>> = vec![Vec::new(); ng];
  for (i, g) in c.pin_group.iter().enumerate() {
    group_pins[*g].push(i);
  }
  let mut group_wires = vec![0usize; ng];
  for g in &c.wire_group {
    group_wires[*g] += 1;
  }
  let nc_at = |p: P| sch.no_connects.iter().any(|q| near(*q, p));
  for g in 0..ng {
    let pins: Vec<&PlacedPin> = group_pins[g].iter().map(|&i| &c.pins[i]).collect();
    let real: Vec<&PlacedPin> = pins.iter().copied().filter(|p| !p.power).collect();
    let powered = pins.iter().any(|p| p.power);
    let name = &c.names[g];
    if c.given[g].len() > 1 {
      out.push(v(
        "label_conflict",
        Severity::Warning,
        format!("one net has several names: {}", c.given[g].join(", ")),
        None,
      ));
    }
    if real.len() == 1 && !powered {
      let p = real[0];
      if p.kind != NoConnect && !nc_at(p.at) && !p.hidden {
        let msg = if group_wires[g] == 0 {
          format!("{} pin {} ({}) is not connected", p.refdes, p.number, p.name)
        } else {
          format!("net {name} only reaches {} pin {}", p.refdes, p.number)
        };
        out.push(v("unconnected_pin", Severity::Warning, msg, Some(p.at)));
      }
    }
    if real.len() > 1 {
      for p in &real {
        if p.kind == NoConnect || nc_at(p.at) {
          out.push(v(
            "nc_connected",
            Severity::Warning,
            format!("{} pin {} is marked no-connect but is on net {name}", p.refdes, p.number),
            Some(p.at),
          ));
        }
      }
    }
    let outputs: Vec<&&PlacedPin> = real.iter().filter(|p| matches!(p.kind, Output | PowerOut)).collect();
    if outputs.len() > 1 {
      let who: Vec<String> = outputs.iter().map(|p| format!("{}.{}", p.refdes, p.number)).collect();
      out.push(v(
        "output_conflict",
        Severity::Error,
        format!("net {name}: outputs drive each other ({})", who.join(", ")),
        Some(outputs[0].at),
      ));
    }
    let has_power_out = powered || real.iter().any(|p| p.kind == PowerOut);
    for p in real.iter().filter(|p| p.kind == PowerIn) {
      if !has_power_out {
        out.push(v(
          "power_not_driven",
          Severity::Error,
          format!(
            "{} pin {} ({}) needs power but net {name} has no power source or power symbol",
            p.refdes, p.number, p.name
          ),
          Some(p.at),
        ));
      }
    }
    let driven = powered || real.iter().any(|p| p.kind.drives());
    for p in real.iter().filter(|p| p.kind == Input) {
      if !driven && real.len() > 1 {
        out.push(v(
          "input_not_driven",
          Severity::Error,
          format!("{} pin {} ({}) on net {name} has nothing driving it", p.refdes, p.number, p.name),
          Some(p.at),
        ));
      }
    }
  }
  // dangling wire ends
  for (i, w) in sch.wires.iter().enumerate() {
    for end in [w.a, w.b] {
      let other_wire = sch.wires.iter().enumerate().any(|(j, o)| j != i && point_seg_dist(end, o.a, o.b) < 1e-4);
      let pin = c.pins.iter().any(|p| !p.hidden && near(p.at, end));
      let label = sch.labels.iter().any(|l| near(l.at, end));
      let bus = sch.buses.iter().any(|b| b.touches(end));
      if !(other_wire || pin || label || bus || nc_at(end)) {
        out.push(v(
          "dangling_wire",
          Severity::Warning,
          format!("wire end at ({:.2}, {:.2}) is not connected", end[0], end[1]),
          Some(end),
        ));
      }
    }
  }
  // wires leaving a bus must name a member
  for bus in &sch.buses {
    let members = bus.members();
    for (i, w) in sch.wires.iter().enumerate() {
      if let Some(end) = [w.a, w.b].into_iter().find(|e| bus.touches(*e)) {
        let g = c.wire_group[i];
        if !c.given[g].iter().any(|n| members.contains(n)) {
          out.push(v(
            "bus_entry_unnamed",
            Severity::Warning,
            format!("a wire leaves bus {} without a label naming one of its signals", bus.name),
            Some(end),
          ));
        }
      }
    }
  }
  out
}
