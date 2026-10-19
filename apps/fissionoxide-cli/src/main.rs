//! Headless FissionOxide.
//!
//! ```text
//! fissionoxide-cli run SCRIPT.jsonl [--open IN.fox] [--save OUT.fox] [--export OUT.step|stl|obj]...
//!                               [--png OUT.png] [--view home|top|front|...] [--size 1200x800]
//! fissionoxide-cli exec IN.fox '{"cmd": "info"}'
//! fissionoxide-cli info IN.fox
//! fissionoxide-cli export IN.fox OUT.step|stl|obj
//! fissionoxide-cli render IN.fox OUT.png [--view V] [--size WxH]
//! fissionoxide-cli commands
//! fissionoxide-cli formats
//! fissionoxide-cli inspect FILE
//! fissionoxide-cli convert IN OUT                 (any readable format to any writable one)
//! fissionoxide-cli hub SUBCOMMAND ['{json}'] [--open FILE]   (hub.SUBCOMMAND on the data hub)
//! fissionoxide-cli mcp [IN.fox]                   (Model Context Protocol server on stdio)
//! ```
//!
//! `--hub DIR` (or `FOX_DATA_DIR`) picks the data hub directory for every subcommand.
//! `--configuration NAME` picks a configuration of the design for `export`, `convert`, `render`,
//! `info` and the `--export`/`--png` outputs of `run`.
//!
//! ```text
//! fissionoxide-cli hub save '{"description": "first cut"}' --open bracket.fox
//! fissionoxide-cli hub list '{"project": "Default Project"}'
//! fissionoxide-cli hub versions '{"item": "i3"}'
//! fissionoxide-cli hub comment '{"item": "i3", "version": 2, "text": "check the fillet"}'
//! fissionoxide-cli hub download '{"item": "i3", "version": 2, "path": "v2.step"}'
//! ```
//!
//! A script is one JSON command per line: `{"cmd": "extrude", "distance": 10}` (the remaining
//! keys are the parameters) or `{"command": "...", "params": {...}}`. Blank lines and lines
//! starting with `#` are ignored. Each result is printed as one JSON line.

mod mcp;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use fox_engine::render::{self, Camera, StandardView};
use fox_engine::scene::{SceneBuilder, ViewOptions};
use fox_engine::{COMMANDS, Session};
use serde_json::{Value, json};

fn main() -> ExitCode {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
  let args: Vec<String> = std::env::args().skip(1).collect();
  match run(&args) {
    Ok(()) => ExitCode::SUCCESS,
    Err(e) => {
      log::error!("{e}");
      let _ = writeln_err(&e);
      ExitCode::FAILURE
    }
  }
}

fn writeln_err(e: &str) -> std::io::Result<()> {
  use std::io::Write;
  writeln!(std::io::stderr(), "fissionoxide-cli: {e}")
}

fn out(v: &Value) {
  use std::io::Write;
  let _ = writeln!(std::io::stdout(), "{v}");
}

fn run(args: &[String]) -> Result<(), String> {
  if let Some(dir) = flag(args, "--hub") {
    HUB.with(|h| *h.borrow_mut() = Some(PathBuf::from(dir)));
  }
  let Some(sub) = args.first() else { return Err(usage()) };
  let rest = &args[1..];
  match sub.as_str() {
    "mcp" => {
      let s = match rest.first() {
        Some(p) => open(p)?,
        None => session(Session::default()),
      };
      mcp::serve(s)
    }
    "formats" => {
      for f in fox_engine::exchange::FORMATS {
        out(&json!(f));
      }
      Ok(())
    }
    "inspect" => {
      let mut s = session(Session::default());
      out(&exec(&mut s, "inspect", &json!({"path": rest.first().ok_or_else(usage)?}))?);
      Ok(())
    }
    "convert" => {
      let mut s = configure(open(rest.first().ok_or_else(usage)?)?, rest)?;
      let target = rest.get(1).ok_or_else(usage)?;
      out(&exec(&mut s, "export", &json!({"path": target}))?);
      Ok(())
    }
    "hub" => {
      let cmd = rest.first().ok_or_else(usage)?;
      let params: Value = match rest.get(1).filter(|a| !a.starts_with("--")) {
        Some(j) => serde_json::from_str(j).map_err(|e| format!("bad JSON parameters: {e}"))?,
        None => json!({}),
      };
      let mut s = match flag(rest, "--open") {
        Some(p) => open(&p)?,
        None => session(Session::default()),
      };
      let name = if cmd.starts_with("hub.") { cmd.clone() } else { format!("hub.{cmd}") };
      out(&exec(&mut s, &name, &params)?);
      Ok(())
    }
    "commands" | "help" => {
      for c in COMMANDS {
        out(&json!({"cmd": c.id, "help": c.help, "params": c.params}));
      }
      Ok(())
    }
    "info" => {
      let mut s = configure(open(rest.first().ok_or_else(usage)?)?, rest)?;
      out(&exec(&mut s, "info", &json!({}))?);
      Ok(())
    }
    "exec" => {
      let mut s = open(rest.first().ok_or_else(usage)?)?;
      let line = rest.get(1).ok_or_else(usage)?;
      let (cmd, params) = parse_line(line)?;
      out(&exec(&mut s, &cmd, &params)?);
      if let Some(p) = flag(rest, "--save") {
        s.save_to(Path::new(&p)).map_err(|e| e.0)?;
      }
      Ok(())
    }
    "export" => {
      let mut s = configure(open(rest.first().ok_or_else(usage)?)?, rest)?;
      let target = rest.get(1).ok_or_else(usage)?;
      out(&exec(&mut s, "export", &json!({"path": target}))?);
      Ok(())
    }
    "render" => {
      let s = configure(open(rest.first().ok_or_else(usage)?)?, rest)?;
      let target = rest.get(1).ok_or_else(usage)?;
      png(&s, Path::new(target), flag(rest, "--view").as_deref(), flag(rest, "--size").as_deref())
    }
    "run" => {
      let script = rest.first().ok_or_else(usage)?;
      let text = if script == "-" {
        let mut t = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut t).map_err(|e| e.to_string())?;
        t
      } else {
        std::fs::read_to_string(script).map_err(|e| format!("{script}: {e}"))?
      };
      let mut s = match flag(rest, "--open") {
        Some(p) => open(&p)?,
        None => session(Session::default()),
      };
      let mut failed = 0;
      for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
          continue;
        }
        let (cmd, params) = parse_line(line).map_err(|e| format!("line {}: {e}", n + 1))?;
        match s.execute(&cmd, &params) {
          Ok(v) => {
            if let Some(e) = v.get("error").and_then(Value::as_str) {
              failed += 1;
              out(&json!({"line": n + 1, "cmd": cmd, "error": e}));
            } else {
              out(&json!({"line": n + 1, "cmd": cmd, "ok": v}));
            }
          }
          Err(e) => {
            failed += 1;
            out(&json!({"line": n + 1, "cmd": cmd, "error": e.0}));
          }
        }
      }
      if let Some(p) = flag(rest, "--save") {
        s.save_to(Path::new(&p)).map_err(|e| e.0)?;
      }
      let mut s = configure(s, rest)?;
      for e in flags(rest, "--export") {
        out(&exec(&mut s, "export", &json!({"path": e}))?);
      }
      if let Some(p) = flag(rest, "--png") {
        png(&s, Path::new(&p), flag(rest, "--view").as_deref(), flag(rest, "--size").as_deref())?;
      }
      if failed > 0 { Err(format!("{failed} command(s) failed")) } else { Ok(()) }
    }
    _ => Err(usage()),
  }
}

fn usage() -> String {
  "usage: fissionoxide-cli run SCRIPT [--open F.fox] [--save F.fox] [--export F.step] [--png F.png] | exec F.fox JSON | info F.fox | export F.fox OUT | convert IN OUT | inspect FILE | render F.fox OUT.png | hub CMD [JSON] [--open F] | mcp [F.fox] | formats | commands  (all take --hub DIR; export, convert, render, info and run take --configuration NAME)".into()
}

thread_local! {
  static HUB: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Applies `--hub DIR` to a session.
fn session(mut s: Session) -> Session {
  s.store_root = HUB.with(|h| h.borrow().clone());
  s
}

fn open(p: &str) -> Result<Session, String> {
  Session::open(&PathBuf::from(p)).map(session).map_err(|e| e.0)
}

/// Applies `--configuration NAME` (a configuration of the design) to a session.
fn configure(mut s: Session, args: &[String]) -> Result<Session, String> {
  if let Some(name) = flag(args, "--configuration") {
    exec(&mut s, "config.activate", &json!({"name": name}))?;
  }
  Ok(s)
}

fn exec(s: &mut Session, cmd: &str, params: &Value) -> Result<Value, String> {
  s.execute(cmd, params).map_err(|e| e.0)
}

/// `{"cmd": name, ...params}` or `{"command": name, "params": {...}}`.
fn parse_line(line: &str) -> Result<(String, Value), String> {
  let v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
  let obj = v.as_object().ok_or("expected a JSON object")?;
  if let Some(c) = obj.get("command").and_then(Value::as_str) {
    return Ok((c.to_string(), obj.get("params").cloned().unwrap_or(json!({}))));
  }
  let c = obj.get("cmd").and_then(Value::as_str).ok_or("missing \"cmd\"")?.to_string();
  let mut params = obj.clone();
  params.remove("cmd");
  Ok((c, Value::Object(params)))
}

fn flag(args: &[String], name: &str) -> Option<String> {
  args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn flags(args: &[String], name: &str) -> Vec<String> {
  args.windows(2).filter(|w| w[0] == name).map(|w| w[1].clone()).collect()
}

fn png(s: &Session, path: &Path, view: Option<&str>, size: Option<&str>) -> Result<(), String> {
  let (w, h) = match size.and_then(|s| s.split_once('x')) {
    Some((w, h)) => (w.parse().map_err(|_| "bad --size")?, h.parse().map_err(|_| "bad --size")?),
    None => (1200u32, 800u32),
  };
  let mut cam = Camera::default();
  if let Some(v) = view {
    cam.set_view(StandardView::parse(v).ok_or_else(|| format!("unknown view '{v}'"))?);
  }
  let (lo, hi) = s
    .model
    .bounds()
    .unwrap_or((fox_engine::kernel::glam::DVec3::splat(-50.0), fox_engine::kernel::glam::DVec3::splat(50.0)));
  cam.fit(lo, hi, w as f64 / h.max(1) as f64);
  let (scene, _) = SceneBuilder::default().build(s, &ViewOptions::default());
  let frame = render::render(&scene, &cam, w, h);
  frame.save_png(path)?;
  out(&json!({"png": path}));
  Ok(())
}
