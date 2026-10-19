//! Builds OpenCASCADE (once, via occt-sys, into `target/OCCT`) and the C++ side of the bridge.
//!
//! Set `CLF_OCCT_DIR` to an existing OCCT install (with `include/` and `lib/`) to skip the
//! from-source build.

const OCCT_LIBS: &[&str] = &[
  "TKDEIGES",
  "TKDESTEP",
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
  println!("cargo:rerun-if-env-changed=CLF_OCCT_DIR");
  println!("cargo:rerun-if-changed=cpp/kernel.cpp");
  println!("cargo:rerun-if-changed=cpp/kernel.hxx");
  println!("cargo:rerun-if-changed=src/ffi.rs");
  println!("cargo:rerun-if-changed=cpp/features2.cpp");
  let root = match std::env::var_os("CLF_OCCT_DIR") {
    Some(d) => std::path::PathBuf::from(d),
    None => {
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
  }
  cxx_build::bridge("src/ffi.rs")
    .file("cpp/kernel.cpp")
    .file("cpp/features2.cpp")
    .std("c++17")
    .define("_USE_MATH_DEFINES", "TRUE")
    .include(&include)
    .flag_if_supported("-Wno-deprecated-declarations")
    .flag_if_supported("-Wno-unused-parameter")
    .compile("clf-kernel-cpp");
}
