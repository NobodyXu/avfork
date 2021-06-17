#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

mod binding {
    include!(concat!(env!("OUT_DIR"), "/syscall_binding.rs"));
}

pub use binding::{sigset_t, pid_t};
use std::os::raw::{c_void, c_int};

use crate::error::{toResult, SyscallError};

pub struct Fd {
    fd: c_int,
}
impl Fd {
    pub fn from_raw(fd: c_int) -> Fd {
        Fd { fd }
    }

    pub fn read(&self, buffer: &mut [u8]) -> Result<usize, SyscallError> {
        let buf_ptr = buffer.as_mut_ptr() as *mut c_void;
        let buf_len = buffer.len() as u64;
        Ok(toResult(unsafe { binding::psys_read(self.fd, buf_ptr, buf_len) })? as usize)
    }
}
impl Drop for Fd {
    fn drop(&mut self) {
        unsafe {
            binding::psys_close(self.fd);
        }
    }
}

// Use static here to ensure AT_FDCWD never get dropped
pub static AT_FDCWD: Fd = Fd { fd: binding::AT_FDCWD };
