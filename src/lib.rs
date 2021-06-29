/// rust bindings for aspawn.h, generated using rust-bindgen
pub mod aspawn;
/// rust bindings for syscall.h, generated using rust-bindgen
/// **ALL FUNCTIONS IN THIS MODULE IS SAFE TO BE USED INSIDE THE CALLBACK OF `avfork`
/// or `avforkrec`**
pub mod syscall;

/// utilty functions used in this library
pub mod utility;

/// wrapper for errno_msg and provide an easy-to-use interface
pub mod error;

/// lowlevel wrapper of aspawn
pub mod lowlevel;

extern crate once_cell;
extern crate libc;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate cstr;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;
