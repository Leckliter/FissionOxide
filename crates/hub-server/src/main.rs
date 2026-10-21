//! `fox-hub-server [--root DIR] [--listen ADDR] [--token USER=TOKEN]... [--tokens FILE] [--open]`
//!
//! Serves the hub directory DIR (default `$FOX_HUB_ROOT`, else `./fox-hub`) on ADDR (default
//! `$FOX_HUB_LISTEN`, else `127.0.0.1:7474`). Tokens come from `--token`, `$FOX_HUB_TOKENS`
//! (`user=token,user=token`) and the tokens file (default `DIR/tokens.txt`, a `user=token` per
//! line). With none at all, a token for `admin` is made and written to the tokens file.
//! `--open` serves without tokens, for a trusted network only.

use std::collections::HashMap;
use std::path::PathBuf;

use fox_hub_server::{Config, new_token, parse_tokens};

fn usage() -> String {
  "usage: fox-hub-server [--root DIR] [--listen ADDR] [--token USER=TOKEN]... [--tokens FILE] [--open]".into()
}

fn main() {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
  if let Err(e) = run() {
    eprintln!("fox-hub-server: {e}");
    std::process::exit(2);
  }
}

fn run() -> Result<(), String> {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let mut root = std::env::var_os("FOX_HUB_ROOT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("fox-hub"));
  let mut listen = std::env::var("FOX_HUB_LISTEN").unwrap_or_else(|_| "127.0.0.1:7474".into());
  let mut tokens_file: Option<PathBuf> = None;
  let mut tokens: HashMap<String, String> = HashMap::new();
  let mut open = false;
  let mut pairs = String::new();
  if let Ok(env) = std::env::var("FOX_HUB_TOKENS") {
    pairs.push_str(&env.replace(',', "\n"));
    pairs.push('\n');
  }
  let mut it = args.iter();
  while let Some(a) = it.next() {
    let mut value = || it.next().cloned().ok_or_else(|| format!("{a} needs a value\n{}", usage()));
    match a.as_str() {
      "--root" => root = PathBuf::from(value()?),
      "--listen" => listen = value()?,
      "--tokens" => tokens_file = Some(PathBuf::from(value()?)),
      "--token" => {
        pairs.push_str(&value()?);
        pairs.push('\n');
      }
      "--open" => open = true,
      "-h" | "--help" => {
        println!("{}", usage());
        return Ok(());
      }
      other => return Err(format!("unknown argument '{other}'\n{}", usage())),
    }
  }
  tokens.extend(parse_tokens(&pairs).map_err(|e| format!("--token / $FOX_HUB_TOKENS: {e}"))?);
  std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
  let file = tokens_file.clone().unwrap_or_else(|| root.join("tokens.txt"));
  match std::fs::read_to_string(&file) {
    Ok(text) => tokens.extend(parse_tokens(&text).map_err(|e| format!("{}: {e}", file.display()))?),
    Err(e) if tokens_file.is_some() => return Err(format!("{}: {e}", file.display())),
    Err(_) => {}
  }
  if tokens.is_empty() && !open {
    let token = new_token();
    std::fs::write(&file, format!("# user=token, one per line\nadmin={token}\n"))
      .map_err(|e| format!("{}: {e}", file.display()))?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
    }
    println!("No tokens were given: made one for 'admin' in {}:\n  {token}", file.display());
    tokens.insert(token, "admin".into());
  }
  let cfg = Config { root: root.clone(), tokens, open };
  let users: std::collections::BTreeSet<&String> = cfg.tokens.values().collect();
  let who = if cfg.open { "anyone (--open)".to_string() } else { format!("{users:?}") };
  let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
  rt.block_on(async move {
    let listener = tokio::net::TcpListener::bind(&listen).await.map_err(|e| format!("{listen}: {e}"))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    log::info!("serving the hub at {} on http://{addr} for {who}", root.display());
    let stop = async {
      let _ = tokio::signal::ctrl_c().await;
      log::info!("stopping");
    };
    fox_hub_server::serve(listener, cfg, stop).await.map_err(|e| e.to_string())
  })
}
