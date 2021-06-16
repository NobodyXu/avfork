#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

mod binding {
    include!(concat!(env!("OUT_DIR"), "/syscall_binding.rs"));
}

pub use binding::{sigset_t, pid_t};
use std::os::raw::c_int;

pub struct Fd {
    fd: c_int,
}
impl Fd {
    pub fn from_raw(fd: c_int) -> Fd {
        Fd { fd }
    }
}
impl Drop for Fd {
    fn drop(&mut self) {
        unsafe {
            binding::psys_close(self.fd);
        }
    }
}
pub static AT_FDCWD: Fd = Fd { fd: binding::AT_FDCWD };
