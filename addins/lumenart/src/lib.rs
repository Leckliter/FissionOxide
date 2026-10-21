//! LumenArt: the layered sign workflow, ported from the LumenArt Fusion 360 add-in (see
//! docs/LUMENART.md). The general tools of that suite are built into FissionOxide now; this
//! plugin holds the sign-specific ones and lists the built-in ones on its tab in workflow order
//! (plugin.toml).
//!
//! Every length the host hands over is in mm; LumenArt's own numbers are inches (`geo::IN`).

#[allow(dead_code)]
mod fox;
mod geo;
mod layers;
mod output;
mod tabs;

use serde_json::Value;

/// Called when the user presses OK: `inputs` holds the dialog's values.
#[unsafe(no_mangle)]
pub extern "C" fn fox_run(cmd: *const u8, cmd_len: usize, inputs: *const u8, inputs_len: usize) -> i64 {
  let cmd = fox::arg(cmd, cmd_len);
  let v: Value = serde_json::from_str(&fox::arg(inputs, inputs_len)).unwrap_or_default();
  fox::reply(match cmd.as_str() {
    "size_sign" => layers::size_sign(&v),
    "place_part" => layers::place_part(&v),
    "new_layer" => layers::new_layer(&v),
    "quick_tab" => tabs::quick_tab(&v),
    "auto_flange" => tabs::auto_flange(&v),
    "slot_projector" => tabs::slot_projector(&v),
    "tab_builder" => tabs::tab_builder(&v),
    "auto_tab" => tabs::auto_tab(&v),
    "auto_export" => output::auto_export(&v),
    _ => Err(format!("unknown command '{cmd}'")),
  })
}

/// A number input (mm, degrees or plain).
pub(crate) fn num(v: &Value, k: &str, default: f64) -> f64 {
  v[k].as_f64().unwrap_or(default)
}
