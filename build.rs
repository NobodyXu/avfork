/* shamelessly copied from https://rust-lang.github.io/rust-bindgen/tutorial-3.html */
extern crate bindgen;

use std::env;
use std::path::PathBuf;
use std::process::Command;

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
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join(output))
        .expect("Couldn't write bindings!");
}

fn main() {
    let status = Command::new("sh")
        .current_dir("aspawn/")
        .args(&["-c", "make", "-j", "$(nproc)"])
        .status()
        .expect("failed to make aspawn/");

    if ! status.success() {
        println!("failed to make aspawn/: exit code = {:#?}", status.code());
    }

    // Tell cargo to where to find library aspawn
    println!("cargo:rustc-link-search=native=aspawn");

    // Tell cargo to tell rustc to link the aspawn statically
    println!("cargo:rustc-link-lib=static=aspawn");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=aspawn/aspawn.h");
    println!("cargo:rerun-if-changed=aspawn/syscall/syscall.h");

    gen_binding("aspawn/aspawn.h", "aspawn_binding.rs");
    gen_binding("aspawn/syscall/syscall.h", "syscall_binding.rs");
}
