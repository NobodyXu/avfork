#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

mod binding {
    include!(concat!(env!("OUT_DIR"), "/syscall_binding.rs"));
}

use std::ops::Deref;
use std::os::raw::{c_void, c_int};

pub use binding::{sigset_t, pid_t};

use crate::error::{toResult, SyscallError};

pub struct FdBox {
    fd: Fd,
}
impl FdBox {
    pub fn from_raw(fd: c_int) -> FdBox {
        FdBox { fd: Fd{fd} }
    }
}
impl Drop for FdBox {
    fn drop(&mut self) {
        unsafe {
            binding::psys_close(self.fd.fd);
        }
    }
}
impl Deref for FdBox {
    type Target = Fd;

    fn deref(&self) ->&Self::Target {
        &self.fd
    }
}

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

    pub fn write(&self, buffer: &[u8]) -> Result<usize, SyscallError> {
        let buf_ptr = buffer.as_ptr() as *const c_void;
        let buf_len = buffer.len() as u64;
        Ok(toResult(unsafe { binding::psys_write(self.fd, buf_ptr, buf_len) })? as usize)
    }
}

// Use static here to ensure AT_FDCWD never get dropped
pub static AT_FDCWD: Fd = Fd { fd: binding::AT_FDCWD };
