//! `clfusion-cli mcp`: a Model Context Protocol server on stdio, so AI agents can drive CLFusion.
//!
//! Every engine command is a tool (dots become underscores: `sheet.edge_flange` →
//! `sheet_edge_flange`) taking the command's JSON parameters; `view_render` returns a PNG of the
//! model. One design session lives for the life of the server.

use std::io::{BufRead, Write};

use clf_engine::render::{self, Camera, StandardView};
use clf_engine::scene::{SceneBuilder, ViewOptions};
use clf_engine::{COMMANDS, Session};
use serde_json::{Value, json};

const PROTOCOL: &str = "2025-06-18";

fn tool_name(cmd: &str) -> String {
  cmd.replace('.', "_")
}

fn tools() -> Vec<Value> {
  let mut out: Vec<Value> = COMMANDS
    .iter()
    .map(|c| {
      json!({
        "name": tool_name(c.id),
        "description": format!("{} Parameters: {}", c.help, c.params),
        "inputSchema": {"type": "object", "additionalProperties": true},
      })
    })
    .collect();
  out.push(json!({
    "name": "view_render",
    "description": "Render the model to a PNG image. Parameters: {view?: home|front|back|top|bottom|left|right, width?, height?}",
    "inputSchema": {"type": "object", "properties": {
      "view": {"type": "string"}, "width": {"type": "integer"}, "height": {"type": "integer"}
    }},
  }));
  out
}

fn render_png(s: &Session, p: &Value) -> Result<Vec<u8>, String> {
  let w = p.get("width").and_then(Value::as_u64).unwrap_or(1000).clamp(64, 4096) as u32;
  let h = p.get("height").and_then(Value::as_u64).unwrap_or(700).clamp(64, 4096) as u32;
  let mut cam = Camera::default();
  if let Some(v) = p.get("view").and_then(Value::as_str) {
    cam.set_view(StandardView::parse(v).ok_or_else(|| format!("unknown view '{v}'"))?);
  }
  let (lo, hi) = s
    .model
    .bounds()
    .unwrap_or((clf_engine::kernel::glam::DVec3::splat(-50.0), clf_engine::kernel::glam::DVec3::splat(50.0)));
  cam.fit(lo, hi, w as f64 / h as f64);
  let (scene, _) = SceneBuilder::default().build(s, &ViewOptions::default());
  render::render(&scene, &cam, w, h).to_png()
}

fn base64(data: &[u8]) -> String {
  const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
  for c in data.chunks(3) {
    let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
    for k in 0..4 {
      if k <= c.len() {
        out.push(char::from(T[((n >> (18 - 6 * k)) & 63) as usize]));
      } else {
        out.push('=');
      }
    }
  }
  out
}

/// Handles one JSON-RPC message; `None` for notifications.
pub fn handle(s: &mut Session, msg: &Value) -> Option<Value> {
  let id = msg.get("id").cloned()?;
  let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
  let params = msg.get("params").cloned().unwrap_or(json!({}));
  let result = match method {
    "initialize" => Ok(json!({
      "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL),
      "capabilities": {"tools": {}},
      "serverInfo": {"name": "clfusion", "version": env!("CARGO_PKG_VERSION")},
      "instructions": "CLFusion parametric CAD. Units are mm and degrees unless an expression says otherwise. \
        Faces and edges can be picked by a point on them ({\"point\": [x, y, z]}). Call 'info' to see the \
        design, 'help' for every command, and 'view_render' to look at the model.",
    })),
    "ping" => Ok(json!({})),
    "tools/list" => Ok(json!({"tools": tools()})),
    "tools/call" => {
      let name = params.get("name").and_then(Value::as_str).unwrap_or("");
      let args = params.get("arguments").cloned().unwrap_or(json!({}));
      Ok(if name == "view_render" {
        match render_png(s, &args) {
          Ok(png) => json!({"content": [{"type": "image", "data": base64(&png), "mimeType": "image/png"}]}),
          Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
        }
      } else {
        match COMMANDS.iter().find(|c| tool_name(c.id) == name) {
          None => json!({"content": [{"type": "text", "text": format!("unknown tool '{name}'")}], "isError": true}),
          Some(c) => match s.execute(c.id, &args) {
            Ok(v) => {
              let failed = v.get("error").is_some_and(|e| !e.is_null());
              json!({"content": [{"type": "text", "text": v.to_string()}], "isError": failed})
            }
            Err(e) => json!({"content": [{"type": "text", "text": e.0}], "isError": true}),
          },
        }
      })
    }
    _ => Err(json!({"code": -32601, "message": format!("method '{method}' not found")})),
  };
  Some(match result {
    Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
    Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
  })
}

/// Serves newline-delimited JSON-RPC on stdin/stdout until stdin closes.
pub fn serve(mut s: Session) -> Result<(), String> {
  let stdin = std::io::stdin();
  let mut stdout = std::io::stdout();
  for line in stdin.lock().lines() {
    let line = line.map_err(|e| e.to_string())?;
    if line.trim().is_empty() {
      continue;
    }
    let reply = match serde_json::from_str::<Value>(&line) {
      Ok(msg) => handle(&mut s, &msg),
      Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}})),
    };
    if let Some(r) = reply {
      writeln!(stdout, "{r}").map_err(|e| e.to_string())?;
      stdout.flush().map_err(|e| e.to_string())?;
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lists_and_calls_tools() {
    let mut s = Session::default();
    let init = handle(&mut s, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})).unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "clfusion");
    assert!(handle(&mut s, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
    let list = handle(&mut s, &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    let names: Vec<&str> =
      list["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"sheet_edge_flange") && names.contains(&"view_render"));
    let call = |s: &mut Session, name: &str, args: Value| {
      handle(
        s,
        &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": name, "arguments": args}}),
      )
      .unwrap()
    };
    let r = call(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
    assert_eq!(r["result"]["isError"], false, "{r}");
    let r = call(&mut s, "view_render", json!({"width": 64, "height": 64}));
    assert_eq!(r["result"]["content"][0]["type"], "image");
    let r = call(&mut s, "nope", json!({}));
    assert_eq!(r["result"]["isError"], true);
    assert_eq!(base64(b"Man"), "TWFu");
    assert_eq!(base64(b"Ma"), "TWE=");
  }
}
