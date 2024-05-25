use std::process::Command;
use std::path::{Path, PathBuf};

fn add_library(location: &Path, target: &str) {
    println!("cargo:rerun-if-changed={}", location.to_string_lossy());
    println!("cargo:rustc-link-search=native={}", location.to_string_lossy());
    println!("cargo:rustc-link-lib=static={}", target);

    Command::new("make")
        .arg(format!("lib{}.a", target))
        .current_dir(location)
        .status()
        .expect("Failed to build native library!");
}

fn main() {
    println!("cargo:rustc-link-lib=chibi-scheme");

    let mut wrapper_location: PathBuf = std::env::current_dir().expect("Unable to access current directory");
    wrapper_location.push("chibi-scheme-wrapper");
    add_library(&wrapper_location, "wrapper");

    let bindings = bindgen::Builder::default()
        .header(wrapper_location.join("wrapper.h").to_string_lossy())
        // Prevents bindgen from adding bindings for a bunch of libc stuff that we don't need.
        .allowlist_file(".*/eval.h")
        .allowlist_file(".*/sexp.h")
        .allowlist_file(".*/wrapper.h")
        .generate()
        .expect("Unable to generate bindings for chibi-scheme!");

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("chibi_scheme.rs"))
        .expect("Couldn't write bindings for chibi-scheme!");
}
