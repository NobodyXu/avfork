/// rust bindings for aspawn.h, generated using rust-bindgen
pub mod aspawn;
/// rust bindings for syscall.h, generated using rust-bindgen
pub mod syscall;

/// utilty functions used in this library
pub mod utility;

/// wrapper for errno_msg and provide an easy-to-use interface
pub mod error;

/// lowlevel wrapper of aspawn and syscall
pub mod lowlevel;

extern crate once_cell;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
