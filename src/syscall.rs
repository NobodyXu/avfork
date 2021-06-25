#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

mod binding {
    include!(concat!(env!("OUT_DIR"), "/syscall_binding.rs"));
}

use std::ops::Deref;
use std::os::raw::{c_void, c_int};
use std::ffi::CStr;

pub use binding::{sigset_t, pid_t, uid_t, gid_t};

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
bitflags! {
    pub struct Mode: binding::mode_t {
        /// user (file owner) has read, write, and execute permission
        const S_IRWXU = 0x00700;
        /// user has read permission
        const S_IRUSR = 0x00400;
        /// user has write permission
        const S_IWUSR = 0x00200;
        /// user has execute permission
        const S_IXUSR = 0x00100;
        /// group has read, write, and execute permission
        const S_IRWXG = 0x00070;
        /// group has read permission
        const S_IRGRP = 0x00040;
        /// group has write permission
        const S_IWGRP = 0x00020;
        /// group has execute permission
        const S_IXGRP = 0x00010;
        /// others have read, write, and execute permission
        const S_IRWXO = 0x00007;
        /// others have read permission
        const S_IROTH = 0x00004;
        /// others have write permission
        const S_IWOTH = 0x00002;
        /// others have execute permission
        const S_IXOTH = 0x00001;

        // According to POSIX, the effect when other bits are set in mode is unspecified.
        // On Linux, the following bits are also honored in mode:

        /// set-user-ID bit
        const S_ISUID = 0x0004000;
        /// set-group-ID bit (see inode(7)).
        const S_ISGID = 0x0002000;
        /// sticky bit (see inode(7)).
        const S_ISVTX = 0x0001000;
    }
}

#[derive(Debug)]
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
    fn openat_impl(dirfd: Fd, pathname: &CStr, flags: c_int, mode: binding::mode_t)
        -> Result<FdBox, SyscallError>
    {
        let pathname = pathname.as_ptr();

        let result = unsafe {
            binding::psys_openat(dirfd.fd, pathname, flags, mode)
        };
        let fd = toResult(result as i64)?;
        Ok(FdBox::from_raw(fd as c_int))
    }

    /// Open existing file.
    ///
    ///  * `dirfd` - can be `AT_FDCWD`
    ///
    /// Check manpage for openat for more documentation.
    pub fn openat(dirfd: Fd, pathname: &CStr, accMode: AccessMode, flags: FdFlags)
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
    pub fn creatat(
        dirfd: Fd, pathname: &CStr, accMode: AccessMode, flags: FdFlags,
        cflags: FdCreatFlags, exclusive: bool, mode: Mode
    )
        -> Result<FdBox, SyscallError>
    {
        let mut flags = (accMode as i32) | flags.bits | cflags.bits;
        if exclusive {
            flags |= libc::O_EXCL;
        }

        FdBox::openat_impl(dirfd, pathname, flags, mode.bits)
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

#[derive(Copy, Clone, Debug)]
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
pub fn chdir(pathname: &CStr) -> Result<(), SyscallError>
{
    let pathname = pathname.as_ptr();
    toResult(unsafe { binding::psys_chdir(pathname) as i64 })?;
    Ok(())
}

pub fn get_pagesz() -> usize {
    unsafe { binding::psys_get_pagesz() as usize }
}

pub fn setresuid(ruid: uid_t, euid: uid_t, suid: uid_t) -> Result<(), SyscallError> {
    unsafe {
        toResult(binding::psys_setresuid(ruid, euid, suid) as i64)?;
    };
    Ok(())
}

pub fn setresgid(rgid: gid_t, egid: gid_t, sgid: gid_t) -> Result<(), SyscallError> {
    unsafe {
        toResult(binding::psys_setresgid(rgid, egid, sgid) as i64)?;
    };
    Ok(())
}

pub fn setgroups(list: &[gid_t]) -> Result<(), SyscallError> {
    unsafe {
        toResult(binding::psys_setgroups(list.len() as u64, list.as_ptr()) as i64)?;
    };
    Ok(())
}

pub fn getpid() -> pid_t {
    unsafe {
        binding::psys_getpid()
    }
}

pub fn sched_setparam(pid: pid_t, param: &libc::sched_param) -> Result<(), SyscallError> {
    let result = unsafe {
        binding::psys_sched_setparam(pid, param as *const _ as *const c_void)
    };
    toResult(result as i64)?;

    Ok(())
}
