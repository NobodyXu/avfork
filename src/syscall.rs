#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

mod binding {
    include!(concat!(env!("OUT_DIR"), "/syscall_binding.rs"));
}

use std::ops::Deref;
use std::os::raw::{c_void, c_int};
use std::ffi::CStr;

pub use binding::{sigset_t, pid_t};

use crate::error::{toResult, SyscallError};

pub struct FdBox {
    fd: Fd,
}
impl FdBox {
    pub fn from_raw(fd: c_int) -> FdBox {
        FdBox { fd: Fd{fd} }
    }

    /// Check manpage for openat for more documentation.
    /// # Safety
    /// `pathname` - must be a null-terminated utf-8 string
    pub unsafe fn openat(dirfd: Fd, pathname: &str, flags: c_int, mode: binding::mode_t)
        -> Result<FdBox, SyscallError>
    {
        let pathname = CStr::from_bytes_with_nul_unchecked(pathname.as_bytes()).as_ptr();
        let fd = toResult(binding::psys_openat(dirfd.fd, pathname, flags, mode) as i64)?;
        Ok(FdBox::from_raw(fd as c_int))
    }

    /// Returns (read end, write end)
    ///
    /// Check manpage for pipe2 for more documentation.
    pub fn pipe2(flag: c_int) -> Result<(FdBox, FdBox), SyscallError> {
        #[allow(clippy::unnecessary_cast)]
        let mut pipefd = [-1 as c_int; 2];

        toResult(unsafe { binding::psys_pipe2(pipefd.as_mut_ptr(), flag) } as i64)?;

        Ok(( FdBox::from_raw(pipefd[0]), FdBox::from_raw(pipefd[1]) ))
    }

    /// Check manpage for dup3 for more documentation.
    pub fn dup3(&self, newfd: c_int, flags: c_int) -> Result<FdBox, SyscallError> {
        let oldfd = self.fd.fd;
        let fd = toResult(unsafe { binding::psys_dup3(oldfd, newfd, flags) } as i64)?;
        Ok(FdBox::from_raw(fd as c_int))
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

#[derive(Copy, Clone)]
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
