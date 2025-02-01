use std::path::PathBuf;
use std::process::Command;

fn pkg_config(library: &str, config: &str) -> Vec<String> {
    let output = Command::new("pkg-config")
        .arg(library)
        .arg(config)
        .output()
        .expect("Could not retrieve package config for guile")
        .stdout;
    String::from_utf8(output)
        .unwrap()
        .split_ascii_whitespace()
        .map(|s| s.to_owned())
        .collect()
}

fn main() {
    // Declare native dependencies
    pkg_config("guile-3.0", "--libs")
        .iter()
        .for_each(|arg| println!("cargo:rustc-link-lib={}", &arg[2..]));

    // Compile + link the wrapper
    let mut wrapper_location: PathBuf =
        std::env::current_dir().expect("Unable to access current directory");
    wrapper_location.push("guile-wrapper");
    let res = Command::new("make")
        .arg("libwrapper.a")
        .current_dir(&wrapper_location)
        .status()
        .expect("Failed to build wrapper library!");
    assert_eq!(res.code().unwrap(), 0, "Failed to build wrapper library!");
    println!(
        "cargo:rustc-link-search=native={}",
        wrapper_location.to_string_lossy()
    );
    println!("cargo:rustc-link-lib=static=wrapper");
    println!(
        "cargo:rerun-if-changed={}",
        wrapper_location.to_string_lossy()
    );

    // Generate bindings
    let bindings = bindgen::Builder::default()
        .header(wrapper_location.join("wrapper.h").to_string_lossy())
        .clang_args(pkg_config("guile-3.0", "--cflags"))
        // Prevents bindgen from adding bindings for a bunch of libc stuff that we don't need.
        .allowlist_file(".*/libguile.*")
        .allowlist_file(".*/wrapper.h")
        .generate()
        .expect("Unable to generate bindings for guile");

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("guile.rs"))
        .expect("Couldn't write bindings for guile!");
}
