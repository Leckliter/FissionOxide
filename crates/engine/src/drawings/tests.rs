//! Broken views, ordinate and angular dimensions, GD&T and balloons: checked on the exported
//! files (DXF entities and their positions, SVG and PDF text).

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

/// One DXF entity: LINE (layer, from, to) or TEXT (text, insertion point, angle).
#[derive(Debug, Clone, PartialEq)]
enum Ent {
  Line(String, [f64; 2], [f64; 2]),
  Text(String, [f64; 2], f64),
}

/// The drawing written as DXF and read back.
fn dxf(s: &mut Session) -> Vec<Ent> {
  let dir = std::env::temp_dir().join(format!("clf-annot-{}-{:?}", std::process::id(), std::thread::current().id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("d.dxf");
  run(s, "drawing.export", json!({"path": path}));
  let text = std::fs::read_to_string(&path).unwrap();
  let _ = std::fs::remove_dir_all(&dir);
  let lines: Vec<&str> = text.lines().collect();
  let mut out = Vec::new();
  let mut i = 0;
  while i + 1 < lines.len() {
    if lines[i] == "0" && (lines[i + 1] == "LINE" || lines[i + 1] == "TEXT") {
      let kind = lines[i + 1];
      let mut j = i + 2;
      let (mut layer, mut t, mut x, mut y, mut x2, mut y2, mut ang) =
        (String::new(), String::new(), 0.0, 0.0, 0.0, 0.0, 0.0);
      while j + 1 < lines.len() && lines[j] != "0" {
        let v = lines[j + 1];
        match lines[j] {
          "8" => layer = v.to_string(),
          "1" => t = v.to_string(),
          "10" => x = v.parse().unwrap(),
          "20" => y = v.parse().unwrap(),
          "11" => x2 = v.parse().unwrap(),
          "21" => y2 = v.parse().unwrap(),
          "50" => ang = v.parse().unwrap(),
          _ => {}
        }
        j += 2;
      }
      out.push(if kind == "LINE" { Ent::Line(layer, [x, y], [x2, y2]) } else { Ent::Text(t, [x, y], ang) });
      i = j;
    } else {
      i += 1;
    }
  }
  out
}

fn texts(e: &[Ent]) -> Vec<String> {
  e.iter().filter_map(|x| if let Ent::Text(t, ..) = x { Some(t.clone()) } else { None }).collect()
}

fn text_at(e: &[Ent], want: &str) -> ([f64; 2], f64) {
  e.iter()
    .find_map(|x| if let Ent::Text(t, p, a) = x { (t == want).then_some((*p, *a)) } else { None })
    .unwrap_or_else(|| panic!("no text {want}: {:?}", texts(e)))
}

fn lines(e: &[Ent], layer: &str) -> Vec<([f64; 2], [f64; 2])> {
  e.iter().filter_map(|x| if let Ent::Line(l, a, b) = x { (l == layer).then_some((*a, *b)) } else { None }).collect()
}

/// Sheet x range of the visible edges.
fn visible_span(e: &[Ent]) -> (f64, f64) {
  lines(e, "VISIBLE")
    .iter()
    .flat_map(|(a, b)| [a[0], b[0]])
    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(x), hi.max(x)))
}

fn info_text(s: &mut Session) -> Vec<String> {
  let info = run(s, "drawing.info", json!({}));
  assert!(info["error"].is_null(), "{info}");
  serde_json::from_value(info["text"].clone()).unwrap()
}

/// A `len` × 20 × 10 bar (x from 0 while `len` is 200), drawn front-on at 1:1.
fn bar(s: &mut Session) {
  run(s, "param.add", json!({"name": "len", "expr": "200 mm"}));
  run(s, "box", json!({"width": "len", "depth": 20, "height": 10, "at": [100, 10]}));
  let r = run(s, "drawing.create", json!({"views": ["front"], "scale": "1:1", "auto_dimensions": false}));
  assert!(r["error"].is_null(), "{r}");
}

#[test]
fn broken_view_keeps_true_lengths() {
  let mut s = Session::default();
  bar(&mut s);
  run(&mut s, "drawing.dimension", json!({"view": 1, "a": [0, 0, 0], "b": [200, 0, 0], "orientation": "horizontal"}));
  let before = dxf(&mut s);
  let (x0, x1) = visible_span(&before);
  assert!((x1 - x0 - 200.0).abs() < 1e-6, "{x0} {x1}");
  let n_visible = lines(&before, "VISIBLE").len();

  run(&mut s, "drawing.break", json!({"view": 1, "from": 50, "to": 150, "gap": 6}));
  let after = dxf(&mut s);
  let (b0, b1) = visible_span(&after);
  // 200 long, 100 broken out, a 6 mm gap
  assert!((b1 - b0 - 106.0).abs() < 1e-6, "{b0} {b1}");
  // the top and bottom edges are cut in two
  assert_eq!(lines(&after, "VISIBLE").len(), n_visible + 2);
  // break lines stand at both sides of the gap
  for x in [b0 + 50.0, b0 + 56.0] {
    let n = lines(&after, "THIN").iter().filter(|(a, b)| (a[0] - x).abs() < 1e-6 && (b[0] - x).abs() < 1e-6).count();
    assert!(n >= 2, "break line at {x}: {n}");
  }
  // the dimension spans the shortened view but reads the true length
  assert!(texts(&after).iter().any(|t| t == "200"), "{:?}", texts(&after));
  let dim_line =
    lines(&after, "THIN").into_iter().find(|(a, b)| (a[1] - b[1]).abs() < 1e-9 && (b[0] - a[0] - 106.0).abs() < 1e-6);
  assert!(dim_line.is_some(), "no 106 mm dimension line");
  let (at, _) = text_at(&after, "200");
  assert!((at[0] - (b0 + 53.0)).abs() < 1e-6, "{at:?}");

  // the break follows a parameter change: 300 long, still 100 out
  run(&mut s, "param.set", json!({"name": "len", "expr": "300"}));
  run(&mut s, "drawing.dimension", json!({"view": 1, "a": [0, 0, 0], "b": [300, 0, 10], "orientation": "horizontal"}));
  let e = dxf(&mut s);
  let (c0, c1) = visible_span(&e);
  assert!((c1 - c0 - 206.0).abs() < 1e-6, "{c0} {c1}");
  assert!(texts(&e).iter().any(|t| t == "300"));

  // auto dimensions read the true size too
  run(&mut s, "drawing.set", json!({"auto_dimensions": true}));
  let t = info_text(&mut s);
  assert!(t.iter().filter(|t| *t == "300").count() >= 2, "{t:?}");

  // vertical breaks, bad input, and removing them
  let bad = s.execute("drawing.break", &json!({"view": 1, "from": 5, "to": 5}));
  assert!(bad.is_err());
  assert!(s.execute("drawing.break", &json!({"view": 9, "from": 1, "to": 5})).is_err());
  run(&mut s, "drawing.break", json!({"view": 1, "direction": "vertical", "from": 2, "to": 8, "gap": 2}));
  let v = dxf(&mut s);
  let ys = lines(&v, "VISIBLE")
    .iter()
    .flat_map(|(a, b)| [a[1], b[1]])
    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), y| (lo.min(y), hi.max(y)));
  assert!((ys.1 - ys.0 - 6.0).abs() < 1e-6, "{ys:?}");
  run(&mut s, "drawing.remove_breaks", json!({"view": 1}));
  let (d0, d1) = visible_span(&dxf(&mut s));
  assert!((d1 - d0 - 300.0).abs() < 1e-6);
}

#[test]
fn ordinate_and_angle_dimensions() {
  let mut s = Session::default();
  bar(&mut s);
  let r = run(
    &mut s,
    "drawing.ordinate",
    json!({"view": 1, "origin": [0, 0, 0], "points": [[20, 0, 0], [200, 0, 10]], "offset": 8}),
  );
  assert_eq!(r["index"], 0);
  let e = dxf(&mut s);
  // values along the view's horizontal, written up the extension lines 20 mm apart
  let (o, a0) = text_at(&e, "0");
  let (p20, a20) = text_at(&e, "20");
  let (p200, _) = text_at(&e, "200");
  assert_eq!((a0, a20), (90.0, 90.0));
  assert!((p20[0] - o[0] - 20.0).abs() < 1e-6 && (p200[0] - o[0] - 200.0).abs() < 1e-6, "{o:?} {p20:?} {p200:?}");
  // the extension lines end at one level, 8 mm above the highest point
  // (vertical thin lines above the title block)
  let ext: Vec<_> =
    lines(&e, "THIN").into_iter().filter(|(a, b)| (a[0] - b[0]).abs() < 1e-9 && a[1].min(b[1]) > 47.0).collect();
  assert_eq!(ext.len(), 3, "{ext:?}");
  let top = ext.iter().map(|(_, b)| b[1]).collect::<Vec<_>>();
  assert!(top.iter().all(|y| (y - top[0]).abs() < 1e-9));
  assert!((top[0] - ext[2].0[1] - 6.5).abs() < 1e-6, "{ext:?}");
  // a broken view moves the far value but not what it reads
  run(&mut s, "drawing.break", json!({"view": 1, "from": 50, "to": 150}));
  let e = dxf(&mut s);
  let (o2, _) = text_at(&e, "0");
  let (q200, _) = text_at(&e, "200");
  assert!((q200[0] - o2[0] - 106.0).abs() < 1e-6);
  run(&mut s, "drawing.remove_breaks", json!({"view": 1}));

  // a string of points (as a dialog sends it), vertical
  let r = run(
    &mut s,
    "drawing.ordinate",
    json!({"view": 1, "origin": [0, 0, 0], "points": "0 0 4; 0 0 10", "orientation": "vertical"}),
  );
  assert!(r.is_object());
  assert!(s.execute("drawing.ordinate", &json!({"view": 1, "origin": [0, 0, 0], "points": "1 2"})).is_err());

  // the angle between the bottom edge and the diagonal to the far top corner
  run(
    &mut s,
    "drawing.angle",
    json!({"view": 1, "vertex": [0, 0, 0], "a": [200, 0, 0], "b": [200, 0, 10], "radius": 30}),
  );
  let want = format!("{}°", clf_model::drawing::fmt_len((10.0f64 / 200.0).atan().to_degrees()));
  assert!(info_text(&mut s).contains(&want), "{want}");
  assert!(texts(&dxf(&mut s)).contains(&want.replace('°', "%%d")));
  // ISO writes a decimal comma
  run(&mut s, "drawing.set", json!({"standard": "iso"}));
  assert!(info_text(&mut s).contains(&want.replace('.', ",")));
  assert!(
    s.execute("drawing.angle", &json!({"view": 1, "vertex": [0, 0, 0], "a": [0, 0, 0], "b": [1, 0, 0]})).is_err()
  );
}

#[test]
fn gdt_symbols_follow_the_standard() {
  let mut s = Session::default();
  bar(&mut s);
  let base = dxf(&mut s);
  let count = |e: &[Ent]| (lines(e, "THIN").len(), texts(e).len());
  let (l0, t0) = count(&base);
  // flatness: frame outline (4) + separator (1) + symbol (4) + leader (1) + arrowhead (3)
  run(
    &mut s,
    "drawing.feature_control",
    json!({"view": 1, "point": [100, 0, 10], "characteristic": "flatness", "tolerance": 0.05}),
  );
  let e = dxf(&mut s);
  assert_eq!(count(&e), (l0 + 13, t0 + 1));
  assert!(texts(&e).contains(&"0.05".to_string()));
  // the frame sits 12 mm right and up from the point; the leader starts on the point
  let (p, _) = text_at(&e, "0.05");
  let (x0, _) = visible_span(&base);
  assert!((p[0] - (x0 + 100.0 + 12.0 + 7.0 + 1.5)).abs() < 1e-6, "{p:?}");

  // datums: letters in order, I/O/Q skipped
  let a = run(&mut s, "drawing.datum", json!({"view": 1, "point": [0, 0, 5]}));
  let b = run(&mut s, "drawing.datum", json!({"view": 1, "point": [100, 0, 0], "offset": [0, -12]}));
  assert_eq!((a["label"].as_str(), b["label"].as_str()), (Some("A"), Some("B")));
  assert!(s.execute("drawing.datum", &json!({"view": 1, "point": [0, 0, 0], "label": "O"})).is_err());

  // position, Ø zone at MMC to A and B
  run(
    &mut s,
    "drawing.feature_control",
    json!({"view": 1, "point": [150, 0, 5], "offset": [-10, 15], "characteristic": "position", "tolerance": "0.1", "diameter": true, "modifier": "mmc", "datums": "A B"}),
  );
  // the rules of the standard
  let bad = |s: &mut Session, p: Value| s.execute("drawing.feature_control", &p).is_err();
  assert!(bad(&mut s, json!({"point": [0, 0, 0], "characteristic": "flatness", "tolerance": 0.1, "datums": ["A"]})));
  assert!(bad(&mut s, json!({"point": [0, 0, 0], "characteristic": "perpendicularity", "tolerance": 0.1})));
  assert!(bad(&mut s, json!({"point": [0, 0, 0], "characteristic": "symmetry", "tolerance": 0.1, "datums": ["A"]})));
  assert!(bad(&mut s, json!({"point": [0, 0, 0], "characteristic": "wobble", "tolerance": 0.1})));
  assert!(bad(&mut s, json!({"point": [0, 0, 0], "characteristic": "position", "tolerance": 0})));

  run(
    &mut s,
    "drawing.surface_finish",
    json!({"view": 1, "point": [50, 0, 10], "roughness": 1.6, "process": "removal"}),
  );
  let t = info_text(&mut s);
  for want in ["Ø0.1", "M", "A", "B", "1.6"] {
    assert!(t.iter().any(|x| x == want), "missing {want}: {t:?}");
  }
  assert!(t.iter().any(|x| x.ends_with("ASME")), "{t:?}");

  // ISO: decimal comma, "Ra" on the extension line, and symmetry allowed
  run(&mut s, "drawing.set", json!({"standard": "iso"}));
  run(
    &mut s,
    "drawing.feature_control",
    json!({"view": 1, "point": [100, 0, 5], "characteristic": "symmetry", "tolerance": 0.2, "datums": ["A"]}),
  );
  let e = dxf(&mut s);
  for want in ["%%c0,1", "0,05", "Ra 1,6", "0,2"] {
    assert!(texts(&e).iter().any(|x| x == want), "missing {want}: {:?}", texts(&e));
  }
  let dir = std::env::temp_dir().join(format!("clf-gdt-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let svg = dir.join("d.svg");
  let pdf = dir.join("d.pdf");
  run(&mut s, "drawing.export", json!({"path": svg}));
  run(&mut s, "drawing.export", json!({"path": pdf}));
  let svg = std::fs::read_to_string(&svg).unwrap();
  let pdf = String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).to_string();
  let _ = std::fs::remove_dir_all(&dir);
  assert!(svg.contains(">Ra 1,6<") && svg.contains(">Ø0,1<"));
  assert!(pdf.contains("(Ra 1,6)") && pdf.contains("(\\3300,1)"));

  // annotations go with their view, or one by one
  let n = run(&mut s, "drawing.info", json!({}))["dimensions"].as_u64().unwrap();
  run(&mut s, "drawing.remove_dimension", json!({"index": 0}));
  assert_eq!(run(&mut s, "drawing.info", json!({}))["dimensions"].as_u64().unwrap(), n - 1);
  assert!(s.execute("drawing.remove_dimension", &json!({"index": 99})).is_err());
  run(&mut s, "drawing.remove_view", json!({"view": 1}));
  assert_eq!(run(&mut s, "drawing.info", json!({}))["dimensions"], 0);
}

/// Two boxes as components "Base" and "Arm".
fn assembly(s: &mut Session) {
  run(s, "box", json!({"width": 100, "depth": 60, "height": 10, "at": [50, 30]}));
  run(s, "box", json!({"width": 80, "depth": 20, "height": 5, "at": [40, 200]}));
  let b: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  run(s, "component.create", json!({"name": "Base", "bodies": [b[0]]}));
  run(s, "component.create", json!({"name": "Arm", "bodies": [b[1]]}));
}

#[test]
fn balloons_follow_the_parts_list() {
  let mut s = Session::default();
  assembly(&mut s);
  run(&mut s, "drawing.create", json!({"views": ["top", "iso"], "parts_list": false, "auto_dimensions": false}));
  let r = run(&mut s, "drawing.balloon", json!({"view": 1, "component": "Arm"}));
  assert!(r["index"].is_number(), "{r}");
  let e = dxf(&mut s);
  let numbers = |e: &[Ent]| {
    let mut t: Vec<String> = texts(e).into_iter().filter(|t| t.parse::<u32>().is_ok()).collect();
    t.sort();
    t
  };
  assert_eq!(numbers(&e), vec!["2"]);
  let arm = s.design.components.iter().find(|c| c.name == "Arm").unwrap().id;
  let (at, _) = text_at(&e, "2");
  let item = clf_model::drawing::bom_rows(&s.design).iter().position(|r| r.components.contains(&arm));
  assert_eq!(item, Some(1));

  // the parts list changes, the balloon follows: Base gone, Arm is item 1
  let base = s.design.components.iter().find(|c| c.name == "Base").unwrap().id.0;
  run(&mut s, "component.delete", json!({"component": base}));
  let e2 = dxf(&mut s);
  assert_eq!(numbers(&e2), vec!["1"]);
  assert_eq!(text_at(&e2, "1").0, at);
  s.undo();

  // auto balloons: one per row without a balloon, in the isometric view
  run(&mut s, "drawing.set", json!({"auto_balloons": true}));
  assert_eq!(numbers(&dxf(&mut s)), vec!["1", "2"]);
  let b: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10, "at": [200, 0]}));
  let new = s.model.bodies.iter().map(|x| x.id.0).find(|x| !b.contains(x)).unwrap();
  run(&mut s, "component.create", json!({"name": "Pin", "bodies": [new]}));
  let e3 = dxf(&mut s);
  assert_eq!(numbers(&e3), vec!["1", "2", "3"]);
  // each automatic balloon: a 48-segment ring, a leader and its 8-sided dot
  let thin = lines(&e3, "THIN").len();
  run(&mut s, "drawing.set", json!({"auto_balloons": false}));
  assert_eq!(lines(&dxf(&mut s), "THIN").len(), thin - 2 * (48 + 1 + 8));
  assert!(s.execute("drawing.balloon", &json!({"view": 1, "component": "Nope"})).is_err());

  // everything saves with the design, and older files without the new fields still open
  let back = clf_model::Design::from_json(&s.design.to_json()).unwrap();
  assert_eq!(back.drawings, s.design.drawings);
  let mut old: Value = serde_json::from_str(&s.design.to_json()).unwrap();
  let d = &mut old["drawings"][0];
  d.as_object_mut().unwrap().remove("standard");
  d.as_object_mut().unwrap().remove("auto_balloons");
  d["dimensions"] = json!([]);
  let back = clf_model::Design::from_json(&old.to_string()).unwrap();
  assert_eq!(back.drawings[0].standard, clf_model::drawing::Standard::Asme);
  assert!(!back.drawings[0].auto_balloons);
}
