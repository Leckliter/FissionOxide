//! Builds OpenCASCADE (once, via occt-sys, into `target/OCCT`) and the C++ side of the bridge.
//!
//! Set `FOX_OCCT_DIR` to an existing OCCT install (with `include/` and `lib/`) to skip the
//! from-source build (CI restores a cached one this way); an empty value counts as unset.

const OCCT_LIBS: &[&str] = &[
  "TKDEIGES",
  "TKDESTEP",
  // XCAF documents for STEP with names, colours and assemblies (cpp/xcaf.cpp)
  "TKXCAF",
  "TKVCAF",
  "TKV3d",
  "TKService",
  "TKCAF",
  "TKLCAF",
  "TKCDF",
  "TKDE",
  "TKXSBase",
  "TKHLR",
  "TKOffset",
  "TKFillet",
  "TKBool",
  "TKBO",
  "TKFeat",
  "TKMesh",
  "TKShHealing",
  "TKPrim",
  "TKTopAlgo",
  "TKGeomAlgo",
  "TKBRep",
  "TKGeomBase",
  "TKG3d",
  "TKG2d",
  "TKMath",
  "TKernel",
];

fn main() {
  println!("cargo:rerun-if-env-changed=FOX_OCCT_DIR");
  println!("cargo:rerun-if-changed=cpp/kernel.cpp");
  println!("cargo:rerun-if-changed=cpp/kernel.hxx");
  println!("cargo:rerun-if-changed=src/ffi.rs");
  println!("cargo:rerun-if-changed=cpp/features2.cpp");
  println!("cargo:rerun-if-changed=cpp/surface2.cpp");
  println!("cargo:rerun-if-changed=cpp/sheet2.cpp");
  println!("cargo:rerun-if-changed=cpp/xcaf.cpp");
  println!("cargo:rerun-if-changed=cpp/xcaf.hxx");
  println!("cargo:rerun-if-changed=cpp/refparts.cpp");
  println!("cargo:rerun-if-changed=cpp/refparts.hxx");
  println!("cargo:rerun-if-changed=cpp/form.cpp");
  println!("cargo:rerun-if-changed=cpp/form.hxx");
  println!("cargo:rerun-if-changed=cpp/project.cpp");
  println!("cargo:rerun-if-changed=cpp/project.hxx");
  println!("cargo:rerun-if-changed=cpp/spun.cpp");
  println!("cargo:rerun-if-changed=cpp/spun.hxx");
  println!("cargo:rerun-if-changed=cpp/threads.cpp");
  println!("cargo:rerun-if-changed=cpp/threads.hxx");
  println!("cargo:rerun-if-changed=cpp/curves3d.cpp");
  println!("cargo:rerun-if-changed=cpp/curves3d.hxx");
  println!("cargo:rerun-if-changed=cpp/silhouette.cpp");
  println!("cargo:rerun-if-changed=cpp/silhouette.hxx");
  let root = match std::env::var_os("FOX_OCCT_DIR") {
    Some(d) if !d.is_empty() => {
      let root = std::path::PathBuf::from(d);
      // A prebuilt (e.g. cached) install: never fall back to building OCCT behind the user's back.
      if !root.join("lib").is_dir() || !root.join("include").is_dir() {
        panic!("FOX_OCCT_DIR={} has no include/ and lib/ (an OCCT 7.8 install with static libraries)", root.display());
      }
      root
    }
    _ => {
      let root = occt_sys::occt_path();
      if !root.join("lib").exists() {
        occt_sys::build_occt();
      }
      root
    }
  };
  let include = root.join("include");
  let lib = root.join("lib");
  println!("cargo:rustc-link-search=native={}", lib.display());
  for l in OCCT_LIBS {
    println!("cargo:rustc-link-lib=static={l}");
  }
  let target = std::env::var("TARGET").unwrap_or_default();
  if target.contains("windows") {
    println!("cargo:rustc-link-lib=dylib=user32");
    // OSD_Process (OpenProcessToken, GetUserNameW); newer std no longer links it for us.
    println!("cargo:rustc-link-lib=dylib=advapi32");
  }
  cxx_build::bridge("src/ffi.rs")
    .file("cpp/kernel.cpp")
    .file("cpp/features2.cpp")
    .file("cpp/surface2.cpp")
    .file("cpp/sheet2.cpp")
    .file("cpp/xcaf.cpp")
    .file("cpp/refparts.cpp")
    .file("cpp/form.cpp")
    .file("cpp/project.cpp")
    .file("cpp/spun.cpp")
    .file("cpp/threads.cpp")
    .file("cpp/curves3d.cpp")
    .file("cpp/silhouette.cpp")
    .std("c++17")
    .define("_USE_MATH_DEFINES", "TRUE")
    .include(&include)
    .flag_if_supported("-Wno-deprecated-declarations")
    .flag_if_supported("-Wno-unused-parameter")
    .compile("fox-kernel-cpp");
}
