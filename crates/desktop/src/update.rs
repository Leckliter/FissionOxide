//! Updates from GitHub releases.
//!
//! The release workflow (`.github/workflows/release.yml`) publishes, for a tag `vX.Y.Z`, the
//! Windows installer `FissionOxide-X.Y.Z-x64-setup.exe` and its SHA-256 next to it
//! (`….exe.sha256`). [`Updater`] asks the releases API for the latest release, and when it is
//! newer than this build downloads the installer into `<config dir>/updates`, checks it against the
//! checksum and runs it. The installer replaces the installed app in place (same AppId), so an
//! update is just a newer installer run over the old one, interactively or silently.
//!
//! `FOX_UPDATE_URL` points the check at another feed (any URL serving the same JSON as
//! `GET /repos/{owner}/{repo}/releases/latest`), for testing or a self-hosted mirror.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::VERSION;
use crate::settings::{Settings, UpdateMode};
use crate::version::Version;

/// The latest published release of FissionOxide.
pub const FEED: &str = "https://api.github.com/repos/leckliter/FissionOxide/releases/latest";

/// Where every release is listed, for platforms without an installer to download.
pub const RELEASES_PAGE: &str = "https://github.com/leckliter/FissionOxide/releases";

/// How long after a check the app looks again on its own.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(20 * 60 * 60);

pub fn feed_url() -> String {
  std::env::var("FOX_UPDATE_URL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| FEED.to_string())
}

/// The end of the installer's file name for this platform, or None where releases ship no
/// installer (the update is then a link to the release page).
pub fn installer_suffix() -> Option<&'static str> {
  if cfg!(all(windows, target_arch = "x86_64")) { Some("-x64-setup.exe") } else { None }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
  pub name: String,
  pub url: String,
  pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
  pub version: Version,
  /// The release's title (often "FissionOxide 0.2.0").
  pub name: String,
  /// Release notes (Markdown).
  pub notes: String,
  /// The release's web page.
  pub page: String,
  /// The installer for this platform, and the file holding its SHA-256.
  pub installer: Option<Asset>,
  pub checksum: Option<Asset>,
}

impl Release {
  /// Reads a release from the GitHub API's JSON, picking the assets for installers ending in
  /// `suffix`.
  pub fn from_json(v: &Value, suffix: Option<&str>) -> Result<Release, String> {
    let tag = v["tag_name"].as_str().ok_or("the release has no tag")?;
    let version = Version::parse(tag).ok_or_else(|| format!("release tag {tag:?} isn't a version"))?;
    let assets: Vec<Asset> = v["assets"]
      .as_array()
      .map(|a| {
        a.iter()
          .filter_map(|a| {
            Some(Asset {
              name: a["name"].as_str()?.to_string(),
              url: a["browser_download_url"].as_str()?.to_string(),
              size: a["size"].as_u64().unwrap_or(0),
            })
          })
          .collect()
      })
      .unwrap_or_default();
    let installer = suffix.and_then(|s| assets.iter().find(|a| a.name.to_ascii_lowercase().ends_with(s)).cloned());
    let checksum = installer.as_ref().and_then(|i| {
      let want = format!("{}.sha256", i.name).to_ascii_lowercase();
      assets.iter().find(|a| a.name.to_ascii_lowercase() == want).cloned()
    });
    let name = v["name"]
      .as_str()
      .filter(|n| !n.trim().is_empty())
      .map_or_else(|| format!("FissionOxide {version}"), str::to_string);
    Ok(Release {
      version,
      name,
      notes: v["body"].as_str().unwrap_or_default().replace("\r\n", "\n"),
      page: v["html_url"].as_str().unwrap_or(RELEASES_PAGE).to_string(),
      installer,
      checksum,
    })
  }

  /// It has an installer for this platform, and its checksum to verify the download.
  pub fn can_install(&self) -> bool {
    self.installer.is_some() && self.checksum.is_some()
  }

  /// Newer than the running build.
  pub fn is_newer(&self) -> bool {
    Version::parse(VERSION).is_none_or(|current| self.version > current)
  }
}

fn agent(read_timeout: Option<Duration>) -> ureq::Agent {
  let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
  ureq::Agent::config_builder()
    .tls_config(tls)
    .user_agent(format!("FissionOxide/{VERSION}"))
    .timeout_connect(Some(Duration::from_secs(15)))
    .timeout_recv_response(Some(Duration::from_secs(30)))
    .timeout_global(read_timeout)
    .build()
    .new_agent()
}

fn http_error(e: ureq::Error) -> String {
  match e {
    ureq::Error::StatusCode(404) => "no release has been published yet (or the release feed isn't public)".into(),
    ureq::Error::StatusCode(403 | 429) => "the release server is limiting requests; try again later".into(),
    ureq::Error::StatusCode(c) => format!("the release server answered {c}"),
    e => format!("couldn't reach the release server ({e})"),
  }
}

/// Asks `feed` for the latest release.
pub fn fetch_latest(feed: &str) -> Result<Release, String> {
  let mut resp = agent(Some(Duration::from_secs(60)))
    .get(feed)
    .header("Accept", "application/vnd.github+json")
    .call()
    .map_err(http_error)?;
  let text = resp.body_mut().read_to_string().map_err(http_error)?;
  let v: Value = serde_json::from_str(&text).map_err(|e| format!("the release feed isn't JSON ({e})"))?;
  Release::from_json(&v, installer_suffix())
}

/// The SHA-256 in a `sha256sum`-style file (`<hex>  <name>`, or just the hex).
pub fn parse_checksum(text: &str) -> Option<String> {
  let h = text.split_whitespace().next()?.to_ascii_lowercase();
  (h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).then_some(h)
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
  let mut f = std::fs::File::open(path)?;
  let mut h = Sha256::new();
  let mut buf = vec![0u8; 1 << 16];
  loop {
    let n = f.read(&mut buf)?;
    if n == 0 {
      break;
    }
    h.update(&buf[..n]);
  }
  Ok(hex(&h.finalize()))
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads `installer` into `dir` and checks it against the SHA-256 in `checksum`, calling
/// `progress(done, total)` as it goes. A file already there with the right checksum (from an
/// earlier run) is used as is. Returns the installer's path.
pub fn download(
  installer: &Asset,
  checksum: &Asset,
  dir: &Path,
  progress: &dyn Fn(u64, Option<u64>),
) -> Result<PathBuf, String> {
  // the name comes from the server: keep only a plain file name
  let name = Path::new(&installer.name).file_name().ok_or("the installer has no file name")?;
  let target = dir.join(name);
  let want = {
    let mut resp = agent(Some(Duration::from_secs(60))).get(&checksum.url).call().map_err(http_error)?;
    let text = resp.body_mut().read_to_string().map_err(http_error)?;
    parse_checksum(&text).ok_or("the release's checksum file has no SHA-256")?
  };
  if target.is_file() && sha256_file(&target).is_ok_and(|h| h == want) {
    let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    progress(size, Some(size));
    return Ok(target);
  }
  std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {} ({e})", dir.display()))?;
  let part = target.with_extension("part");
  let result = (|| {
    let resp = agent(None).get(&installer.url).call().map_err(http_error)?;
    let total = resp.body().content_length().or((installer.size > 0).then_some(installer.size));
    let mut body = resp.into_body().into_with_config().limit(u64::MAX).reader();
    let mut file = std::fs::File::create(&part).map_err(|e| format!("couldn't write {} ({e})", part.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    progress(0, total);
    loop {
      let n = body.read(&mut buf).map_err(|e| format!("the download stopped ({e})"))?;
      if n == 0 {
        break;
      }
      h.update(&buf[..n]);
      file.write_all(&buf[..n]).map_err(|e| format!("couldn't write the download ({e})"))?;
      done += n as u64;
      progress(done, total);
    }
    file.sync_all().map_err(|e| format!("couldn't write the download ({e})"))?;
    let got = hex(&h.finalize());
    if got != want {
      return Err("the download is damaged (its checksum doesn't match the release's); try again".to_string());
    }
    std::fs::rename(&part, &target).map_err(|e| format!("couldn't keep the download ({e})"))
  })();
  if result.is_err() {
    let _ = std::fs::remove_file(&part);
  }
  result.map(|()| target)
}

/// How the installer runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStyle {
  /// The installer's wizard, as when the user downloads it by hand.
  Wizard,
  /// Only a progress window; FissionOxide starts again when it's done.
  Restart,
  /// Nothing shown (installing as the app closes).
  Quiet,
}

/// Runs the installer `path` and returns once it has started; the app should then close so its
/// files can be replaced (the installer also closes it, through the Windows Restart Manager).
pub fn run_installer(path: &Path, style: InstallStyle) -> std::io::Result<()> {
  let log = path.with_extension("log");
  let mut cmd = std::process::Command::new(path);
  // flags of Inno Setup installers; /RELAUNCH is read by installer/windows/fissionoxide.iss
  match style {
    InstallStyle::Wizard => {}
    InstallStyle::Restart => {
      cmd.args(["/SILENT", "/SP-", "/SUPPRESSMSGBOXES", "/NOCANCEL", "/CLOSEAPPLICATIONS", "/RELAUNCH=1"]);
    }
    InstallStyle::Quiet => {
      cmd.args(["/VERYSILENT", "/SP-", "/SUPPRESSMSGBOXES", "/NOCANCEL", "/CLOSEAPPLICATIONS"]);
    }
  }
  cmd.arg(format!("/LOG={}", log.display()));
  cmd.spawn().map(|_| ())
}

/// Where the updater is.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
  /// Nothing checked yet (or updates are off).
  Idle,
  Checking,
  /// The last check found nothing newer.
  UpToDate,
  /// A newer release, not downloaded (in Notify mode, or with no installer for this platform).
  Available(Release),
  Downloading {
    release: Release,
    done: u64,
    total: Option<u64>,
  },
  /// Downloaded and verified: installs when asked, or when the app closes.
  Ready {
    release: Release,
    installer: PathBuf,
  },
  Failed(String),
}

impl Status {
  /// The newer release this status is about.
  pub fn release(&self) -> Option<&Release> {
    match self {
      Status::Available(r) | Status::Downloading { release: r, .. } | Status::Ready { release: r, .. } => Some(r),
      _ => None,
    }
  }
}

struct Inner {
  state: Mutex<(Status, Settings)>,
  /// Called from the worker thread when the status changes (the UI repaints).
  wake: Box<dyn Fn() + Send + Sync>,
  feed: String,
  dir: PathBuf,
  settings_path: PathBuf,
}

/// Checks for, downloads and installs updates on a worker thread; the UI polls [`Updater::status`].
#[derive(Clone)]
pub struct Updater {
  inner: Arc<Inner>,
}

fn now() -> u64 {
  std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Updater {
  /// An updater with the user's settings, the default feed and `<config dir>/updates`.
  pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Updater {
    Self::with(feed_url(), crate::config_dir().join("updates"), Settings::path(), wake)
  }

  pub fn with(feed: String, dir: PathBuf, settings_path: PathBuf, wake: impl Fn() + Send + Sync + 'static) -> Updater {
    let settings = Settings::load_from(&settings_path);
    Updater {
      inner: Arc::new(Inner {
        state: Mutex::new((Status::Idle, settings)),
        wake: Box::new(wake),
        feed,
        dir,
        settings_path,
      }),
    }
  }

  pub fn status(&self) -> Status {
    self.lock().0.clone()
  }

  pub fn settings(&self) -> Settings {
    self.lock().1.clone()
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, (Status, Settings)> {
    self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
  }

  fn set(&self, status: Status) {
    self.lock().0 = status;
    (self.inner.wake)();
  }

  fn edit_settings(&self, f: impl FnOnce(&mut Settings)) {
    let mut st = self.lock();
    f(&mut st.1);
    if let Err(e) = st.1.save_to(&self.inner.settings_path) {
      log::warn!("couldn't save {}: {e}", self.inner.settings_path.display());
    }
  }

  pub fn set_mode(&self, mode: UpdateMode) {
    self.edit_settings(|s| s.update_mode = mode);
    if mode == UpdateMode::Auto && self.status().release().is_some_and(Release::can_install) {
      self.download();
    }
  }

  /// Stops offering `release`; a later one is offered again.
  pub fn skip(&self, release: &Release) {
    self.edit_settings(|s| s.skip_version = Some(release.version.to_string()));
    self.set(Status::UpToDate);
  }

  /// At startup: removes installers of versions already installed, then checks if updates are
  /// on and the last check is old enough.
  pub fn start(&self) {
    self.clean_downloads();
    let s = self.settings();
    if s.update_mode != UpdateMode::Off && now().saturating_sub(s.last_check) >= CHECK_INTERVAL.as_secs() {
      self.check(false);
    }
  }

  fn clean_downloads(&self) {
    let Ok(entries) = std::fs::read_dir(&self.inner.dir) else { return };
    let current = Version::parse(VERSION);
    for e in entries.flatten() {
      let name = e.file_name().to_string_lossy().to_string();
      // FissionOxide-0.2.0-x64-setup.exe, .part, .log
      let v = name.strip_prefix("FissionOxide-").and_then(|r| r.split("-x64").next()).and_then(Version::parse);
      if v.is_none_or(|v| current.as_ref().is_some_and(|c| v <= *c)) || name.ends_with(".part") {
        let _ = std::fs::remove_file(e.path());
      }
    }
  }

  /// Looks for a newer release on a worker thread. `manual` (Help → Check for Updates) also
  /// offers a version the user skipped. In Auto mode a newer installer downloads straight away.
  pub fn check(&self, manual: bool) {
    if matches!(self.status(), Status::Checking | Status::Downloading { .. } | Status::Ready { .. }) {
      return;
    }
    self.set(Status::Checking);
    let me = self.clone();
    std::thread::spawn(move || {
      let result = fetch_latest(&me.inner.feed);
      me.edit_settings(|s| s.last_check = now());
      match result {
        Ok(r) if r.is_newer() => {
          let skipped = me.settings().skip_version.is_some_and(|s| Version::parse(&s).as_ref() == Some(&r.version));
          if skipped && !manual {
            me.set(Status::UpToDate);
          } else {
            log::info!("FissionOxide {} is available", r.version);
            let auto = me.settings().update_mode == UpdateMode::Auto && r.can_install();
            me.set(Status::Available(r));
            if auto {
              me.download();
            }
          }
        }
        Ok(_) => me.set(Status::UpToDate),
        Err(e) => {
          log::info!("update check failed: {e}");
          me.set(Status::Failed(e));
        }
      }
    });
  }

  /// Downloads the available release's installer on a worker thread.
  pub fn download(&self) {
    let Status::Available(release) = self.status() else { return };
    let (Some(installer), Some(checksum)) = (release.installer.clone(), release.checksum.clone()) else {
      self.set(Status::Failed("this release has no installer for this computer; download it from its page".into()));
      return;
    };
    self.set(Status::Downloading { release: release.clone(), done: 0, total: Some(installer.size) });
    let me = self.clone();
    std::thread::spawn(move || {
      let progress = |done, total| {
        let mut st = me.lock();
        if let Status::Downloading { done: d, total: t, .. } = &mut st.0 {
          *d = done;
          *t = total;
        }
        drop(st);
        (me.inner.wake)();
      };
      match download(&installer, &checksum, &me.inner.dir, &progress) {
        Ok(path) => me.set(Status::Ready { release, installer: path }),
        Err(e) => {
          log::warn!("update download failed: {e}");
          me.set(Status::Failed(e));
        }
      }
    });
  }

  /// Starts the downloaded installer. The app must close next.
  pub fn install(&self, style: InstallStyle) -> Result<(), String> {
    let Status::Ready { installer, .. } = self.status() else { return Err("no update has been downloaded".into()) };
    run_installer(&installer, style).map_err(|e| format!("couldn't start the installer ({e})"))
  }

  /// As the app closes: installs a downloaded update quietly. Returns whether one was started.
  pub fn install_on_exit(&self) -> bool {
    matches!(self.status(), Status::Ready { .. }) && self.install(InstallStyle::Quiet).is_ok()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::net::TcpListener;

  fn release_json(tag: &str, base: &str) -> Value {
    serde_json::json!({
      "tag_name": tag,
      "name": "",
      "body": "- Faster fillets\r\n- Fixes",
      "html_url": "https://example.com/r",
      "assets": [
        {"name": format!("FissionOxide-{}-x64-setup.exe", &tag[1..]), "browser_download_url": format!("{base}/setup.exe"), "size": 11},
        {"name": format!("FissionOxide-{}-x64-setup.exe.sha256", &tag[1..]), "browser_download_url": format!("{base}/setup.exe.sha256"), "size": 99},
        {"name": "fissionoxide-linux.tar.gz", "browser_download_url": format!("{base}/linux"), "size": 5}
      ]
    })
  }

  #[test]
  fn reads_a_github_release() {
    let r = Release::from_json(&release_json("v99.1.0", "https://x"), Some("-x64-setup.exe")).unwrap();
    assert_eq!(r.version.to_string(), "99.1.0");
    assert_eq!(r.name, "FissionOxide 99.1.0");
    assert_eq!(r.notes, "- Faster fillets\n- Fixes");
    assert_eq!(r.installer.as_ref().unwrap().url, "https://x/setup.exe");
    assert_eq!(r.checksum.as_ref().unwrap().name, "FissionOxide-99.1.0-x64-setup.exe.sha256");
    assert!(r.is_newer());
    let none = Release::from_json(&release_json("v0.0.1", "https://x"), None).unwrap();
    assert!(none.installer.is_none() && none.checksum.is_none() && !none.is_newer());
    assert!(Release::from_json(&serde_json::json!({"tag_name": "nightly"}), None).is_err());
  }

  #[test]
  fn reads_checksum_files() {
    let h = "a".repeat(64);
    assert_eq!(parse_checksum(&format!("{h}  FissionOxide-setup.exe\n")), Some(h.clone()));
    assert_eq!(parse_checksum(&h.to_uppercase()), Some(h));
    assert_eq!(parse_checksum("nope"), None);
  }

  /// Serves `routes` (path → body) over HTTP on a local port, one request per connection.
  fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
      for stream in listener.incoming() {
        let Ok(mut s) = stream else { continue };
        let mut req = Vec::new();
        let mut buf = [0u8; 1024];
        while !req.windows(4).any(|w| w == b"\r\n\r\n") {
          let Ok(n) = s.read(&mut buf) else { break };
          if n == 0 {
            break;
          }
          req.extend_from_slice(&buf[..n]);
        }
        let line = String::from_utf8_lossy(&req).lines().next().unwrap_or_default().to_string();
        let path = line.split(' ').nth(1).unwrap_or("/").to_string();
        let found = routes.iter().find(|(p, _)| *p == path);
        let (status, body) = found.map_or(("404 Not Found", Vec::new()), |(_, b)| ("200 OK", b.clone()));
        let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
        let _ = s.write_all(head.as_bytes());
        let _ = s.write_all(&body);
      }
    });
    base
  }

  fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fox-update-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
  }

  #[test]
  fn downloads_and_verifies() {
    let payload = b"hello setup".to_vec();
    let sum = hex(&Sha256::digest(&payload));
    let base = serve(vec![
      ("/setup.exe", payload.clone()),
      ("/setup.exe.sha256", format!("{sum}  FissionOxide-9.0.0-x64-setup.exe\n").into_bytes()),
      ("/bad.sha256", format!("{}  x\n", "0".repeat(64)).into_bytes()),
    ]);
    let dir = scratch("dl");
    let asset = |name: &str, path: &str| Asset { name: name.into(), url: format!("{base}{path}"), size: 0 };
    let installer = asset("FissionOxide-9.0.0-x64-setup.exe", "/setup.exe");
    let seen = Mutex::new(Vec::new());
    let path =
      download(&installer, &asset("x.sha256", "/setup.exe.sha256"), &dir, &|d, t| seen.lock().unwrap().push((d, t)))
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), payload);
    assert_eq!(seen.lock().unwrap().last(), Some(&(11, Some(11))));
    // a second time the file already there is used
    let again = download(&installer, &asset("x.sha256", "/setup.exe.sha256"), &dir, &|_, _| {}).unwrap();
    assert_eq!(again, path);
    // a checksum mismatch keeps nothing
    let other = asset("FissionOxide-9.0.1-x64-setup.exe", "/setup.exe");
    let e = download(&other, &asset("bad.sha256", "/bad.sha256"), &dir, &|_, _| {}).unwrap_err();
    assert!(e.contains("damaged"), "{e}");
    assert!(!dir.join("FissionOxide-9.0.1-x64-setup.exe").exists());
    assert!(!dir.join("FissionOxide-9.0.1-x64-setup.part").exists());
    std::fs::remove_dir_all(dir).unwrap();
  }

  fn wait(u: &Updater, done: impl Fn(&Status) -> bool) -> Status {
    for _ in 0..500 {
      let s = u.status();
      if done(&s) {
        return s;
      }
      std::thread::sleep(Duration::from_millis(10));
    }
    panic!("updater stuck at {:?}", u.status());
  }

  #[test]
  fn checks_and_downloads_in_the_background() {
    let payload = b"new installer".to_vec();
    let sum = hex(&Sha256::digest(&payload));
    let base = serve(vec![("/setup.exe", payload), ("/setup.exe.sha256", sum.into_bytes())]);
    // routes need the base URL in the release JSON, so serve that from a second server
    let mut json = release_json("v99.0.0", &base);
    if installer_suffix().is_none() {
      // no installer on this platform: the release is only Available
      json["assets"] = Value::Array(vec![]);
    }
    let feed = serve(vec![("/latest", serde_json::to_vec(&json).unwrap())]);
    let dir = scratch("bg");
    let settings = dir.join("settings.ini");
    let u = Updater::with(format!("{feed}/latest"), dir.join("updates"), settings.clone(), || {});
    u.check(true);
    let s = wait(&u, |s| matches!(s, Status::Ready { .. } | Status::Available(_) | Status::Failed(_)));
    assert_eq!(s.release().map(|r| r.version.to_string()).as_deref(), Some("99.0.0"), "{s:?}");
    if installer_suffix().is_some() {
      assert!(matches!(s, Status::Ready { .. }), "{s:?}");
    }
    assert!(Settings::load_from(&settings).last_check > 0);
    // skipping hides it from automatic checks
    let u = Updater::with(format!("{feed}/latest"), dir.join("updates"), settings.clone(), || {});
    u.skip(s.release().unwrap());
    u.check(false);
    wait(&u, |s| matches!(s, Status::UpToDate));
    u.check(true);
    let s = wait(&u, |s| s.release().is_some());
    assert_eq!(s.release().unwrap().version.to_string(), "99.0.0");
    // a feed with nothing newer
    let old = serve(vec![("/latest", serde_json::to_vec(&release_json("v0.0.1", &base)).unwrap())]);
    let u = Updater::with(format!("{old}/latest"), dir.join("updates"), settings, || {});
    u.check(false);
    wait(&u, |s| matches!(s, Status::UpToDate));
    let missing = Updater::with(format!("{old}/nothing"), dir.join("updates"), dir.join("s2.ini"), || {});
    missing.check(true);
    let Status::Failed(e) = wait(&missing, |s| matches!(s, Status::Failed(_))) else { unreachable!() };
    assert!(e.contains("no release"), "{e}");
    std::fs::remove_dir_all(dir).unwrap();
  }
}
