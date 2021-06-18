/* shamelessly copied from https://rust-lang.github.io/rust-bindgen/tutorial-3.html */
extern crate bindgen;
extern crate once_cell;

use std::env;
use std::path::PathBuf;
use std::fs::canonicalize;
use std::process::{Command, exit};

use once_cell::sync::Lazy;

static OUT_DIR: Lazy<String> = Lazy::new(|| {
    env::var("OUT_DIR").unwrap()
});

static OUT_PATH: Lazy<PathBuf> = Lazy::new(|| {
    PathBuf::from((*OUT_DIR).clone())
});

fn gen_binding(header: &str, output: &str) {
    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header(header)
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    bindings
        .write_to_file(OUT_PATH.join(output))
        .expect("Couldn't write bindings!");
}

fn main() {
    let build_dir_path = canonicalize(&(*OUT_PATH))
        .expect("Failed to canonicalize OUT_PATH")
        .join("aspawn_build");

    let build_dir = match build_dir_path.to_str() {
        Some(s) => s.to_owned(),
        None => panic!("Cannot convert canonicalized OUT_PATH to a valid utf-8 str")
    };

    println!("build_dir = {}", build_dir);

    let status = Command::new("sh")
        .current_dir("aspawn")
        .env("BUILD_DIR", &build_dir)
        .args(&["-c", "make", "-j", "$(nproc)"])
        .status()
        .expect("failed to make aspawn/");

    if ! status.success() {
        println!("failed to build submodule aspawn: exit code = {:#?}", status.code());
        exit(1);
    }

    // Tell cargo to where to find library aspawn
    println!("cargo:rustc-link-search=native={}", build_dir);

    // Tell cargo to tell rustc to link the aspawn statically
    println!("cargo:rustc-link-lib=static=aspawn");

    // Tell cargo to invalidate the built crate whenever the submodule changes
    println!("cargo:rerun-if-changed=aspawn");

    gen_binding("aspawn/aspawn.h", "aspawn_binding.rs");
    gen_binding("aspawn/syscall/syscall.h", "syscall_binding.rs");
    gen_binding("aspawn/syscall/errno_msgs.h", "errno_msgs_binding.rs");
}
