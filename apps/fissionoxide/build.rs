//! On Windows, gives fissionoxide.exe its icon (Explorer, the taskbar, Settings > Apps), its
//! version and description (Task Manager lists the app as "FissionOxide") and a manifest.

fn main() {
  println!("cargo:rerun-if-changed=../../brand/windows/fissionoxide.ico");
  println!("cargo:rerun-if-changed=fissionoxide.manifest");
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
    return;
  }
  let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
  let mut res = winresource::WindowsResource::new();
  res
    .set_icon("../../brand/windows/fissionoxide.ico")
    .set_manifest_file("fissionoxide.manifest")
    .set("FileDescription", "FissionOxide")
    .set("ProductName", "FissionOxide")
    .set("CompanyName", "FissionOxide")
    .set("LegalCopyright", "FissionOxide contributors. MIT or Apache-2.0.")
    .set("OriginalFilename", "fissionoxide.exe")
    .set("InternalName", "fissionoxide")
    .set("ProductVersion", &version)
    .set("FileVersion", &version);
  if let Err(e) = res.compile() {
    panic!("couldn't compile the Windows resources (icon, version, manifest): {e}");
  }
}
