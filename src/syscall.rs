#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub use std::os::raw::c_int;

include!(concat!(env!("OUT_DIR"), "/syscall_binding.rs"));

pub mod wrapper {
    use crate::syscall::*;

    pub struct Fd {
        fd: c_int,
    }
    impl Fd {
        pub fn new(fd: c_int) -> Result<Fd, ()> {
            if fd >= 0 {
                Ok(Fd {fd})
            } else {
                Err(())
            }
        }
    }
    impl Drop for Fd {
        fn drop(&mut self) {
            unsafe {
                psys_close(self.fd);
            }
        }
    }
    pub static AT_FDCWD: Fd = Fd { fd: crate::syscall::AT_FDCWD };
}
