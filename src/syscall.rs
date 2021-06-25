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

// Here it relies on the compiler to check that i32 == c_int
#[repr(i32)]
#[derive(Copy, Clone, Debug)]
pub enum AccessMode {
    O_RDONLY = libc::O_RDONLY,
    O_WRONLY = libc::O_WRONLY,
    O_RDWR   = libc::O_RDWR
}

bitflags! {
    pub struct FdFlags: c_int {
        const O_APPEND = libc::O_APPEND;
        const O_TRUNC = libc::O_TRUNC;
        const O_CLOEXEC = libc::O_CLOEXEC;

        const O_ASYNC = libc::O_ASYNC;
        const O_DSYNC = libc::O_DSYNC;
        const O_SYNC = libc::O_SYNC;
        const O_DIRECT = libc::O_DIRECT;

        const O_DIRECTORY = libc::O_DIRECTORY;
        const O_PATH = libc::O_PATH;

        const O_LARGEFILE = libc::O_LARGEFILE;
        const O_NOATIME = libc::O_NOATIME;
        const O_NOCTTY = libc::O_NOCTTY;
        const O_NOFOLLOW = libc::O_NOFOLLOW;
        const O_NONBLOCK = libc::O_NONBLOCK;
    }
}
bitflags! {
    pub struct FdCreatFlags: c_int {
        const O_CREAT = libc::O_CREAT;
        const O_TMPFILE = libc::O_TMPFILE;
    }
}

pub struct FdBox {
    fd: Fd,
}
impl FdBox {
    pub fn from_raw(fd: c_int) -> FdBox {
        FdBox { fd: Fd{fd} }
    }

    ///  * `dirfd` - can be `AT_FDCWD`
    ///  * `mode` - ignored if O_CREAT is not passed
    ///
    /// Check manpage for openat for more documentation.
    ///
    /// # Safety
    ///  * `pathname` - must be a null-terminated utf-8 string
    unsafe fn openat_impl(dirfd: Fd, pathname: &str, flags: c_int, mode: binding::mode_t)
        -> Result<FdBox, SyscallError>
    {
        let pathname = CStr::from_bytes_with_nul_unchecked(pathname.as_bytes()).as_ptr();
        let fd = toResult(binding::psys_openat(dirfd.fd, pathname, flags, mode) as i64)?;
        Ok(FdBox::from_raw(fd as c_int))
    }

    /// Open existing file.
    ///
    ///  * `dirfd` - can be `AT_FDCWD`
    ///
    /// Check manpage for openat for more documentation.
    ///
    /// # Safety
    ///  * `pathname` - must be a null-terminated utf-8 string
    pub unsafe fn openat(dirfd: Fd, pathname: &str, accMode: AccessMode, flags: FdFlags)
        -> Result<FdBox, SyscallError>
    {
        FdBox::openat_impl(dirfd, pathname, (accMode as i32) | flags.bits, 0)
    }

    /// Open existing file.
    ///
    ///  * `dirfd` - can be `AT_FDCWD`
    ///  * `exclusive` - if yes, then O_EXCL flags is specified when attempting to
    ///    create the file.
    ///
    /// Check manpage for openat for more documentation.
    ///
    /// # Safety
    ///  * `pathname` - must be a null-terminated utf-8 string
    pub unsafe fn creatat(
        dirfd: Fd, pathname: &str, accMode: AccessMode, flags: FdFlags,
        cflags: FdCreatFlags, exclusive: bool, mode: binding::mode_t
    )
        -> Result<FdBox, SyscallError>
    {
        let mut flags = (accMode as i32) | flags.bits | cflags.bits;
        if exclusive {
            flags |= libc::O_EXCL;
        }

        FdBox::openat_impl(dirfd, pathname, flags, mode)
    }

    /// Returns (read end, write end)
    ///
    /// Check manpage for pipe2 for more documentation.
    pub fn pipe2(flag: FdFlags) -> Result<(FdBox, FdBox), SyscallError> {
        #[allow(clippy::unnecessary_cast)]
        let mut pipefd = [-1 as c_int; 2];

        toResult(unsafe { binding::psys_pipe2(pipefd.as_mut_ptr(), flag.bits) } as i64)?;

        Ok(( FdBox::from_raw(pipefd[0]), FdBox::from_raw(pipefd[1]) ))
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

    /// Check manpage for dup3 for more documentation.
    pub fn dup3(&self, newfd: c_int, flags: FdFlags) -> Result<FdBox, SyscallError> {
        let oldfd = self.fd;
        let fd = toResult(unsafe { binding::psys_dup3(oldfd, newfd, flags.bits) } as i64)?;
        Ok(FdBox::from_raw(fd as c_int))
    }

    /// Check manpage for fchdir for more documentation.
    pub fn fchdir(&self) -> Result<(), SyscallError> {
        let fd = self.fd;

        toResult(unsafe { binding::psys_fchdir(fd) } as i64)?;

        Ok(())
    }
}

// Use static here to ensure AT_FDCWD never get dropped
pub static AT_FDCWD: Fd = Fd { fd: binding::AT_FDCWD };


/// Check manpage for chdir for more documentation.
/// # Safety
///  * `pathname` - must be a null-terminated utf-8 string
pub unsafe fn chdir(pathname: &str) -> Result<(), SyscallError>
{
    let pathname = CStr::from_bytes_with_nul_unchecked(pathname.as_bytes()).as_ptr();
    toResult(binding::psys_chdir(pathname) as i64)?;
    Ok(())
}

pub fn get_pagesz() -> usize {
    unsafe { binding::psys_get_pagesz() as usize }
}
