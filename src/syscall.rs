#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

/// TODO: Add autorestart

mod binding {
    use super::{CStr, FdPath, c_int, FdBasicOp, FdFlags};
    use crate::error::{toResult, SyscallError};

    include!(concat!(env!("OUT_DIR"), "/syscall_binding.rs"));

    pub fn openat_wrapper(
        dirfd: FdPath,
        pathname: &CStr,
        flags: c_int,
        mode: mode_t
    ) -> Result<c_int, SyscallError>
    {
        let pathname = pathname.as_ptr();

        let result = unsafe {
            psys_openat(dirfd.get_fd(), pathname, flags, mode)
        };
        let fd = toResult(result as i64)?;
        Ok(fd as c_int)
    }

    pub fn dup(oldfd: c_int) -> Result<c_int, SyscallError> {
        let fd = toResult(unsafe { psys_dup(oldfd) } as i64)?;
        Ok(fd as c_int)
    }
    pub fn dup3(oldfd: c_int, newfd: c_int, flags: FdFlags)
        -> Result<c_int, SyscallError>
    {
        let fd = toResult(unsafe { psys_dup3(oldfd, newfd, flags.bits) } as i64)?;
        Ok(fd as c_int)
    }
}

use std::ops::Deref;
pub use std::os::raw::{c_void, c_int, c_long, c_char};
pub use std::ffi::CStr;
use std::io::{Write, Read};
use std::hint::unreachable_unchecked;
use std::os::unix::io::{AsRawFd, RawFd};

pub use binding::{sigset_t, pid_t, uid_t, gid_t};

use crate::expect;
use crate::error::{toResult, SyscallError};
use crate::utility::to_void_ptr;

pub fn autorestart<T, F>(mut f: F)
    -> Result<T, SyscallError>
    where F: FnMut() -> Result<T, SyscallError>
{
    loop {
        let ret = f();

        if let Err(err) = &ret {
            if err.get_errno() == libc::EINTR {
                continue;
            }
        }

        break ret
    }
}
#[macro_export]
macro_rules! autorestart {
    ( { $( $tt:tt )* } ) => {
        $crate::syscall::autorestart(
            || { $( $tt )* }
        )
    };
}

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

        const O_LARGEFILE = libc::O_LARGEFILE;
        const O_NOATIME = libc::O_NOATIME;
        const O_NOCTTY = libc::O_NOCTTY;
        const O_NOFOLLOW = libc::O_NOFOLLOW;
        const O_NONBLOCK = libc::O_NONBLOCK;
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

macro_rules! impl_AsRawFd_for {
    ($t: ident) => (
        impl AsRawFd for $t {
            fn as_raw_fd(&self) -> RawFd {
                self.get_fd()
            }
        }
    )
}

#[derive(Debug)]
pub struct FdBox {
    fd: Fd,
}
impl_AsRawFd_for!(FdBox);
impl FromRaw for FdBox {
    /// # Safety
    ///  * `fd` - must be a valid fd that isn't opened with `O_PATH` or `O_DIRECTORY`
    unsafe fn from_raw(fd: c_int) -> FdBox {
        FdBox { fd: Fd::from_raw(fd) }
    }
}
impl FdBox {
    ///  * `dirfd` - can be `AT_FDCWD`
    ///  * `mode` - ignored if O_CREAT is not passed
    ///
    /// Check manpage for openat for more documentation.
    fn openat_impl(dirfd: FdPath, pathname: &CStr, flags: c_int, mode: binding::mode_t)
        -> Result<FdBox, SyscallError>
    {
        Ok(unsafe {
            FdBox::from_raw(binding::openat_wrapper(dirfd, pathname, flags, mode)?)
        })
    }

    /// Open existing file.
    ///
    ///  * `dirfd` - can be `AT_FDCWD`
    ///
    /// Check manpage for openat for more documentation.
    pub fn openat(dirfd: FdPath, pathname: &CStr, accMode: AccessMode, flags: FdFlags)
        -> Result<FdBox, SyscallError>
    {
        FdBox::openat_impl(dirfd, pathname, (accMode as i32) | flags.bits, 0)
    }

    /// Open existing file.
    ///
    ///  * `dirfd` - can be `AT_FDCWD`
    ///  * `readable` - whether the returned fd will be readable
    ///  * `tmpfile_or_regular_file` - if true, then `O_TMPFILE` will be passed as flags;
    ///    Otherwise, `O_CREAT` will be passed.
    ///  * `exclusive` - if yes, then `O_EXCL` flags is passed when attempting to
    ///    create the file.
    ///
    /// Check manpage for openat for more documentation.
    pub fn creatat(
        dirfd: FdPath, pathname: &CStr, readable: bool, flags: FdFlags,
        tmpfile_or_regular_file: bool, exclusive: bool, mode: Mode
    )
        -> Result<FdBox, SyscallError>
    {
        let mut flags = flags.bits;
        if exclusive {
            flags |= libc::O_EXCL;
        }
        if readable {
            flags |= libc::O_RDWR;
        } else {
            flags |= libc::O_WRONLY;
        }
        if tmpfile_or_regular_file {
            flags |= libc::O_TMPFILE;
        } else {
            flags |= libc::O_CREAT;
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

        Ok(unsafe {( FdBox::from_raw(pipefd[0]), FdBox::from_raw(pipefd[1]) )})
    }
}
impl Drop for FdBox {
    fn drop(&mut self) {
        let ret = unsafe {
            binding::psys_close(self.get_fd()) as i64
        };

        if cfg!(debug_assertions) {
            expect!(toResult(ret), "Failed to release the FdBox {:#?}", self);
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
    fd: c_int
}
impl FromRaw for Fd {
    /// # Safety
    ///  * `fd` - must be a valid fd that isn't opened with `O_PATH` or `O_DIRECTORY`
    unsafe fn from_raw(fd: c_int) -> Fd {
        Fd { fd }
    }
}
impl FdBasicOp for Fd {
    type BoxedFd = FdBox;

    fn get_fd(&self) -> c_int {
        self.fd
    }
}
impl_AsRawFd_for!(Fd);
impl Fd {
    pub fn read(&self, buffer: &mut [u8]) -> Result<usize, SyscallError> {
        let buf_ptr = buffer.as_mut_ptr() as *mut c_void;
        let buf_len = buffer.len() as u64;
        Ok(toResult(unsafe {
            binding::psys_read(self.get_fd(), buf_ptr, buf_len)
        })? as usize)
    }

    pub fn write(&self, buffer: &[u8]) -> Result<usize, SyscallError> {
        let buf_ptr = buffer.as_ptr() as *const c_void;
        let buf_len = buffer.len() as u64;
        Ok(toResult(unsafe {
            binding::psys_write(self.get_fd(), buf_ptr, buf_len)
        })? as usize)
    }
}
/// impl Write for Fd so that write!, writeln! and other methods that
/// requires trait Write can be called upon it.
impl Write for Fd {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(Fd::write(self, buf)?)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// impl Read for Fd so that any method that requires trait Write can be called upon it.
impl Read for Fd {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(Fd::read(self, buf)?)
    }
}

// Here it relies on the compiler to check that i32 == c_int
#[repr(i32)]
#[derive(Copy, Clone, Debug)]
pub enum FdPathMode {
    anyPath   = 0,
    directory = libc::O_DIRECTORY,
    symlink   = libc::O_NOFOLLOW,
}

#[derive(Debug)]
pub struct FdPathBox {
    fd: FdPath,
}
impl FromRaw for FdPathBox {
    /// # Safety
    ///  * `fd` - must be a valid fd that is opened with `O_PATH`
    unsafe fn from_raw(fd: c_int) -> FdPathBox {
        FdPathBox { fd: FdPath::from_raw(fd) }
    }
}
impl_AsRawFd_for!(FdPathBox);
impl FdPathBox {
    pub fn openat(dirfd: FdPath, pathname: &CStr, mode: FdPathMode, cloexec: bool)
        -> Result<FdPathBox, SyscallError>
    {
        let flags = if cloexec {
            libc::O_CLOEXEC
        } else {
            0
        } | libc::O_PATH | mode as c_int;

        Ok(unsafe {
            FdPathBox::from_raw(binding::openat_wrapper(dirfd, pathname, flags, 0)?)
        })
    }
}
impl Deref for FdPathBox {
    type Target = FdPath;

    fn deref(&self) ->&Self::Target {
        &self.fd
    }
}
impl Drop for FdPathBox {
    fn drop(&mut self) {
        let ret = unsafe {
            binding::psys_close(self.get_fd()) as i64
        };

        if cfg!(debug_assertions) {
            expect!(toResult(ret), "Failed to release the FdPathBox {:#?}", self);
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct FdPath {
    fd: c_int,
}
impl FromRaw for FdPath {
    /// # Safety
    ///  * `fd` - must be a valid fd that is opened with `O_PATH`
    unsafe fn from_raw(fd: c_int) -> FdPath {
        FdPath { fd }
    }
}
impl FdBasicOp for FdPath {
    type BoxedFd = FdPathBox;

    fn get_fd(&self) -> c_int {
        self.fd
    }
}
impl_AsRawFd_for!(FdPath);
impl FdPath {
    /// Pre condition: self is opened in dir mode
    /// Check manpage for fchdir for more documentation.
    pub fn fchdir(&self) -> Result<(), SyscallError> {
        let fd = self.get_fd();

        toResult(unsafe { binding::psys_fchdir(fd) } as i64)?;

        Ok(())
    }
}

pub trait FromRaw {
    /// # Safety
    ///  * `fd` - must be a valid fd
    unsafe fn from_raw(fd: c_int) -> Self;
}
pub trait FdBasicOp {
    type BoxedFd: FromRaw;

    fn get_fd(&self) -> c_int;

    /// Check manpage for dup3 for more documentation.
    fn dup3(&self, newfd: c_int, flags: FdFlags) -> Result<Self::BoxedFd, SyscallError> {
        use binding::dup3;

        Ok(unsafe { Self::BoxedFd::from_raw(dup3(self.get_fd(), newfd, flags)?) })
    }

    /// Check manpage for dup for more documentation.
    fn dup(&self) -> Result<Self::BoxedFd, SyscallError> {
        use binding::dup;

        Ok(unsafe { Self::BoxedFd::from_raw(dup(self.get_fd())?) })
    }
}

pub const AT_FDCWD: FdPath = FdPath { fd: binding::AT_FDCWD };
pub const STDOUT: Fd = Fd { fd: 1 };
pub const STDERR: Fd = Fd { fd: 2 };

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

pub fn sched_getparam(pid: pid_t) -> Result<libc::sched_param, SyscallError> {
    let mut param = std::mem::MaybeUninit::<libc::sched_param>::uninit();

    let result = unsafe {
        binding::psys_sched_getparam(pid, param.as_mut_ptr() as *mut c_void)
    };
    toResult(result as i64)?;

    Ok(unsafe { param.assume_init() })
}

#[derive(Copy, Clone)]
pub enum SchedPolicy {
    /// the standard round-robin time-sharing policy;
    SCHED_OTHER,
    /// for "batch" style execution of processes; and
    SCHED_BATCH,
    /// for running very low priority background jobs.
    SCHED_IDLE,

    // real-time policies:

    /// a first-in, first-out policy; and
    SCHED_FIFO(libc::sched_param),
    /// a round-robin policy.
    SCHED_RR(libc::sched_param),
}
impl std::fmt::Debug for SchedPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchedPolicy::SCHED_OTHER => write!(f, "SCHED_OTHER"),
            SchedPolicy::SCHED_BATCH => write!(f, "SCHED_BATCH"),
            SchedPolicy::SCHED_IDLE => write!(f, "SCHED_IDLE"),

            SchedPolicy::SCHED_FIFO(param) =>
                write!(f, "SCHED_FIFO({})", param.sched_priority),
            SchedPolicy::SCHED_RR(param) =>
                write!(f, "SCHED_RR({})", param.sched_priority),
        }
    }
}

/// # Error
///
/// If unexpected scheulder policy is returned from kernel, then this function
/// will terminate the process with a friendly error message.
pub fn sched_getscheduler(pid: pid_t) -> Result<SchedPolicy, SyscallError> {
    let result = unsafe {
        toResult(binding::psys_sched_getscheduler(pid) as i64 )? as i32
    };

    Ok(match result {
        libc::SCHED_OTHER => SchedPolicy::SCHED_OTHER,
        libc::SCHED_BATCH => SchedPolicy::SCHED_BATCH,
        libc::SCHED_IDLE => SchedPolicy::SCHED_IDLE,

        libc::SCHED_FIFO => SchedPolicy::SCHED_FIFO(sched_getparam(pid)?),
        libc::SCHED_RR => SchedPolicy::SCHED_RR(sched_getparam(pid)?),

        _ => {
            crate::errx!(1, "Unexpected scheduler policy in sched_getscheduler")
        }
    })
}

pub fn sched_setscheduler(pid: pid_t, policy: &SchedPolicy) -> Result<(), SyscallError> {
    let nullptr: *const libc::sched_param = std::ptr::null();

    let setter = |policy, param| -> Result<(), SyscallError> {
        let result = unsafe {
            binding::psys_sched_setscheduler(pid, policy, param as *const c_void)
        };

        toResult(result as i64)?;

        Ok(())
    };

    match policy {
        SchedPolicy::SCHED_OTHER => setter(libc::SCHED_OTHER, nullptr),
        SchedPolicy::SCHED_BATCH => setter(libc::SCHED_BATCH, nullptr),
        SchedPolicy::SCHED_IDLE => setter(libc::SCHED_IDLE, nullptr),

        SchedPolicy::SCHED_FIFO(param) => setter(libc::SCHED_FIFO, param as *const _),
        SchedPolicy::SCHED_RR(param) => setter(libc::SCHED_RR, param as *const _),
    }
}

// Here it relies on the compiler to check that i32 == c_int
#[repr(i32)]
#[derive(Copy, Clone, Debug)]
pub enum PrlimitResource {
    /// The maximum size of process's virtual memory (address space)
    /// Specified in bytes, but **rounded down to the system page size**
    /// Affects brk, mmap and mremap.
    RLIMIT_AS   = libc::RLIMIT_AS as i32,
    /// The maximum size of a core file in bytes, 0 to disable process dumpping
    RLIMIT_CORE = libc::RLIMIT_CORE as i32,
    /// Limit in seconds on the CPU time for the process.
    /// Kernel will keep sending `SIGXCPU` (can be caught) once soft limit is reached
    /// and sent SIGKILL when hard limit is reached.
    RLIMIT_CPU = libc::RLIMIT_CPU as i32,
    /// Similar to RLIMIT_AS
    RLIMIT_DATA = libc::RLIMIT_DATA as i32,
    /// The maximum size in bytes of files that the process may create.
    /// Attempts to extend a file beyond this limit results in delivery of SIGXFSZ
    /// or EFBIG if the former signal is catched.
    RLIMIT_FSIZE = libc::RLIMIT_FSIZE as i32,
    /// Limit on the combined number of `flock` locks and `fcntl` leases.
    RLIMIT_LOCKS = libc::RLIMIT_LOCKS as i32,
    /// Maxmimum number of memory in bytes that may be locked in RAM.
    /// Affects `mlock`, `mlockall` and `mmap` with `flags = MAP_LOCKED`.
    RLIMIT_MEMLOCK = libc::RLIMIT_MEMLOCK as i32,
    /// Limit on number of bytes that can be allocated for POSIX message queues for the
    /// ruid of the calling process. Enforced on `mq_open`.
    RLIMIT_MSGQUEUE = libc::RLIMIT_MSGQUEUE as i32,
    /// Ceiling to hich the process's nice value can be raised.
    RLIMIT_NICE = libc::RLIMIT_NICE as i32,
    /// Limit nunmber of fds can opened by process.
    RLIMIT_NOFILE = libc::RLIMIT_NOFILE as i32,
    /// Limit on number of extant process for the ruid of the calling process.
    RLIMIT_NPROC = libc::RLIMIT_NPROC as i32,
    /// Limit on the process's resident set in bytes, only for linux 2.4.x, x < 30.
    RLIMIT_RSS = libc::RLIMIT_RSS as i32,
    /// Ceiling on the rt priority set in `sched_setscheduler` and `sched_setparam`.
    RLIMIT_RTPRIO = libc::RLIMIT_RTPRIO as i32,
    /// Limit on the amount of CPU time (in microseconds) that a process scheduled
    /// under a rt scheduling policy may consume without making a blocking system call.
    RLIMIT_RTTIME = libc::RLIMIT_RTTIME as i32,
    /// Limit on the number of signals that may be queued for the ruid of 
    /// the calling process.
    /// Only affects signal sent via `sigqueue`.
    RLIMIT_SIGPENDING = libc::RLIMIT_SIGPENDING as i32,
    /// The maximum size of the process stack in bytes.
    /// Upon limit, SIGSEGV is sent.
    RLIMIT_STACK = libc::RLIMIT_STACK as i32,
}

///  * `new_limit` - If `Some(limit) = new_limit`, then the `limit` will be set to the
///    new limit for the `resource`.
/// Return old_limit
pub fn prlimit(resource: PrlimitResource, new_limit: Option<&binding::rlimit64>)
    -> Result<binding::rlimit64, SyscallError>
{
    let prlimit_impl = |new_limit_ptr| -> Result<binding::rlimit64, SyscallError> {
        let mut old_limit = std::mem::MaybeUninit::<binding::rlimit64>::uninit();

        toResult(unsafe {
            binding::psys_prlimit(
                resource as c_int,
                new_limit_ptr,
                old_limit.as_mut_ptr()
            )
        } as i64)?;

        Ok(unsafe { old_limit.assume_init() })
    };

    match new_limit {
        Some(new_limit) => {
            // In order to work around a strange behavior of the bindgen, that is 
            // translates psys_prlimit(int, const struct rlimit64*, struct rlimit64*)
            // to psys_prlimit(c_int, *mut rlimit64, *mut rlimit64)
            let mut new_limit = *new_limit;

            prlimit_impl(&mut new_limit as *mut _)
        },
        None => prlimit_impl(std::ptr::null_mut())
    }
}

#[derive(Copy, Clone, Debug)]
pub enum PriorityWhichAndWho {
    PRIO_PROCESS(pid_t),
    PRIO_PGRP(pid_t),
    PRIO_USER(uid_t)
}

#[derive(Copy, Clone, Debug)]
pub struct Priority {
    prio: c_int
}
impl Priority {
    /// * `prio` - should be in range -20..20
    pub const fn new(prio: c_int) -> Option<Priority> {
        if prio >= -20 && prio <= 19 {
            Some(Priority { prio })
        } else {
            None
        }
    }

    pub const fn get_prio(&self) -> c_int {
        self.prio
    }
}

pub fn getpriority(which_and_who: PriorityWhichAndWho) -> Result<Priority, SyscallError> {
    let getpriority_impl = |which, who| -> Result<Priority, SyscallError> {
        let knice = toResult(unsafe { binding::psys_getpriority(which, who) as i64 })?;
        Ok(Priority { prio: (20 - knice) as c_int })
    };

    use PriorityWhichAndWho::*;

    match which_and_who {
        PRIO_PROCESS(pid) => getpriority_impl(libc::PRIO_PROCESS as i32, pid as c_long),
        PRIO_PGRP(pgid) => getpriority_impl(libc::PRIO_PGRP as i32, pgid as c_long),
        PRIO_USER(uid) => getpriority_impl(libc::PRIO_USER as i32, uid as c_long),
    }
}

pub fn setpriority(which_and_who: PriorityWhichAndWho, prio: Priority)
    -> Result<(), SyscallError>
{
    let setpriority_impl = |which, who| -> Result<(), SyscallError> {
        let knice = 20 - prio.get_prio();
        toResult(unsafe { binding::psys_setpriority(which, who, knice) as i64 })?;
        Ok(())
    };

    use PriorityWhichAndWho::*;

    match which_and_who {
        PRIO_PROCESS(pid) => setpriority_impl(libc::PRIO_PROCESS as i32, pid as c_long),
        PRIO_PGRP(pgid) => setpriority_impl(libc::PRIO_PGRP as i32, pgid as c_long),
        PRIO_USER(uid) => setpriority_impl(libc::PRIO_USER as i32, uid as c_long),
    }
}

pub fn sigemptyset() -> sigset_t {
    let mut sigset = std::mem::MaybeUninit::<sigset_t>::uninit();
    
    unsafe {
        binding::pure_sigemptyset(sigset.as_mut_ptr() as *mut c_void);
        sigset.assume_init()
    }
}

pub fn sigfillset() -> sigset_t {
    let mut sigset = std::mem::MaybeUninit::<sigset_t>::uninit();
    
    unsafe {
        binding::pure_sigfillset(sigset.as_mut_ptr() as *mut c_void);
        sigset.assume_init()
    }
}

// Here it relies on the compiler to check that i32 == c_int
#[repr(i32)]
#[derive(Copy, Clone, Debug)]
pub enum SigprocmaskHow {
    SIG_BLOCK = libc::SIG_BLOCK,
    SIG_UNBLOCK = libc::SIG_UNBLOCK,
    SIG_SETMASK = libc::SIG_SETMASK,
}

/// * `new_set` - If `Some(set) new_set`, then the sigmask is set to `set`.
/// Returns the old sigset.
///
/// # Errors
///
/// Only when:
///  - new_set contains an invalid pointer
///  - stack overflow caused by too much stack allocation
///  - Internal implementation error of binding::psys_sigprocmask
pub fn sigprocmask(how: SigprocmaskHow, new_set: Option<&sigset_t>)
    -> Result<sigset_t, SyscallError>
{
    let how = how as c_int;
    let new_set: *const c_void = match new_set {
        Some(set) => to_void_ptr(set),
        None => std::ptr::null()
    };
    let mut old_set = std::mem::MaybeUninit::<sigset_t>::uninit();

    let ret = unsafe {
        binding::psys_sigprocmask(how, new_set, old_set.as_mut_ptr() as *mut c_void)
    };
    toResult(ret as i64)?;

    Ok(unsafe { old_set.assume_init() })
}

pub fn exit(status: c_int) -> ! {
    unsafe {
        binding::psys_exit(status);
    }
    unsafe { unreachable_unchecked() }
}

#[derive(Copy, Clone, Debug)]
pub struct CStrArray<'a> {
    arr: &'a [*const c_char]
}
/// Return a literal of type CStrArray
#[macro_export]
macro_rules! CStrArray {
    ( $( $s:literal ),* ) => {
        unsafe {
            $crate::syscall::CStrArray::from_raw(&[
                $(
                    $crate::cstr::cstr!($s).as_ptr(),
                )*
                std::ptr::null()
            ])
        }
    };
}
impl<'a> CStrArray<'a> {
    /// # Safety
    /// This function call is for macro_rules CStrArray!
    pub const unsafe fn from_raw(arr: &'a [*const c_char]) -> CStrArray<'a> {
        CStrArray { arr }
    }

    pub fn new(arr: &'a [*const c_char]) -> Option<CStrArray<'a>> {
        if let Some(last) = arr.last() {
            if *last == std::ptr::null() {
                return Some(CStrArray { arr });
            }
        }

        None
    }
    
    pub const fn as_ptr(&self) -> *const *const c_char {
        self.arr.as_ptr()
    }
}

pub fn execve(pathname: &CStr, argv: &CStrArray, envp: &CStrArray) -> SyscallError
{
    let ret = unsafe {
        binding::psys_execve(pathname.as_ptr(), argv.as_ptr(), envp.as_ptr())
    };

    match toResult(ret as i64) {
        Ok(_) => unsafe { unreachable_unchecked() },
        Err(err) => err
    }
}

bitflags! {
    pub struct ExecveAtFlags: c_int {
        const NONE = 0;
        const AT_EMPTY_PATH       = libc::AT_EMPTY_PATH;
        const AT_SYMLINK_NOFOLLOW = libc::AT_SYMLINK_NOFOLLOW;
    }
}

/// This syscall is native to linux, but is emulated on any other target
/// Checks `man 2 execveat` for more info.
pub fn execveat(
    dirfd: FdPath,
    pathname: &CStr,
    argv: &CStrArray,
    envp: &CStrArray,
    flags: ExecveAtFlags
) -> SyscallError
{
    let ret = unsafe {
        binding::psys_execveat(
            dirfd.get_fd(),
            pathname.as_ptr(),
            argv.as_ptr(),
            envp.as_ptr(),
            flags.bits()
        )
    };

    match toResult(ret as i64) {
        Ok(_) => unsafe { unreachable_unchecked() },
        Err(err) => err
    }
}

/// linux/limits.h say PATH_MAX is 4096, but it seems that the filesystem on linux
/// does not actually hardcoded this limit
/// 
/// So let's make PSYS_PATH_MAX 5 * 4096 just in case.
///
/// Include the null byte.
pub const PATH_MAX: usize = 5 * 4096;

/// Does not include the null byte.
pub const FILENAME_MAX: usize = 255;

#[derive(Copy, Clone, Debug)]
pub struct Filename<'a> {
    filename: &'a CStr
}
impl<'a> Filename<'a> {
    /// * `filename` - must not contains any slash or be empty,
    ///   must be less than `FILENAME_MAX
    pub fn new(filename: &'a CStr) -> Option<Filename<'a>> {
        let filename_sz = filename.to_bytes().len();
        if filename_sz == 0 || filename_sz > FILENAME_MAX {
            return None;
        }

        for byte in filename.to_bytes() {
            if *byte == b'/' {
                return None;
            }
        }

        Some(Filename { filename })
    }

    pub fn to_bytes(&self) -> &[u8] {
        self.filename.to_bytes()
    }

    pub fn len(&self) -> usize {
        self.to_bytes().len()
    }

    pub fn is_empty(&self) -> bool {
        self.to_bytes().is_empty()
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ExecvelCandidate<'a> {
    filename: Filename<'a>,
    paths: &'a [&'a str]
}
impl<'a> ExecvelCandidate<'a> {
    /// * `paths` - must not be empty and neither should each element in it be empty,
    ///   and len of each element plus len of filename plus 1 must be less than 
    ///   `PATH_MAX`.
    pub fn new(filename: Filename<'a>, paths: &'a [&'a str])
        -> Option<ExecvelCandidate<'a>>
    {
        let filename_sz = filename.len();

        if paths.is_empty() {
            return None;
        }

        for path in paths {
            let path = path.as_bytes();

            // The additional two bytes is for the slash and the null bytes
            let size = filename_sz + path.len() + 2;
            if path.is_empty() || size > PATH_MAX {
                return None;
            }
        }

        Some(ExecvelCandidate { filename, paths })
    }

    pub fn get_filename(&'a self) -> &'a Filename<'a> {
        &self.filename
    }

    pub fn get_paths(&self) -> &'a [&'a str] {
        self.paths
    }
}

/// These functions duplicate the actions of the shell in searching for 
/// an executable file
/// 
/// Certain errors are treated specially:
/// 
/// If permission is denied for a file (the attempted execve(2) failed with 
/// the error `EACCES`), these functions will continue searching the rest of 
/// the search path
/// 
/// If no other file is found, however, they will return with errno set to EACCES
pub fn execvel(
    candidate: &ExecvelCandidate,
    argv: &CStrArray,
    envp: &CStrArray
) -> SyscallError
{
    let argv = argv.as_ptr();
    let envp = envp.as_ptr();

    // Since PATH_MAX is 5 page long, it will be too costy to write it all 
    // to zero and zeroing will also trigger interruption, causing the kernel to 
    // allocate pages for it while it might not be used at all.
    let mut constructed_path = std::mem::MaybeUninit::<[u8; PATH_MAX]>::uninit();
    let constructed_path_ptr = constructed_path.as_mut_ptr() as *mut u8;

    let pmemcpy = |offset, src, size| {
        unsafe {
            binding::pmemcpy(
                constructed_path_ptr.add(offset) as *mut c_void,
                src as *const c_void,
                size as u64
            );
        };
    };
    let write_one_byte = |size, byte| {
        unsafe {
            constructed_path_ptr.add(size).write(byte);
        };
    };

    let filename = candidate.get_filename().to_bytes();
    let filename_sz = filename.len();
    let filename = filename.as_ptr();

    let mut got_eaccess = false;

    for path in candidate.get_paths().iter() {
        let path = path.as_bytes();
        let path_sz = path.len();
        let path = path.as_ptr();

        pmemcpy(0, path, path_sz);
        write_one_byte(path_sz, b'/');
        pmemcpy(path_sz + 1, filename, filename_sz);
        write_one_byte(path_sz + 1 + filename_sz, b'\0');

        let ret = unsafe {
            binding::psys_execve(constructed_path.as_ptr() as *const c_char, argv, envp)
        };
        let err = match toResult(ret as i64) {
            Ok(_) => unsafe { unreachable_unchecked() },
            Err(err) => err
        };

        match err.get_errno() as i32 {
            libc::EACCES => {
                // Record that we got a 'Permission denied' error.  If we end
                // up finding no executable we can use, we want to diagnose
                // that we did find one but were denied access.
                got_eaccess = true;
                continue;
            },
            // Those errors indicate the file is missing or not executable
            // by us, in which case we want to just try the next path
            // directory.
            libc::ENOENT  => continue,
            libc::ESTALE  => continue,
            libc::ENOTDIR => continue,
            // Some strange filesystems like AFS return even
            // stranger error numbers.  They cannot reasonably mean
            // anything else so ignore those, too.
            libc::ENODEV    => continue,
            libc::ETIMEDOUT => continue,
    
            _ => return err,
        };
    }

    if got_eaccess {
        SyscallError::new(libc::EACCES as u32)
    } else {
        SyscallError::new(libc::ENOENT as u32)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct FexecvelCandidate<'a> {
    filename: Filename<'a>,
    paths: &'a [FdPath]
}
impl<'a> FexecvelCandidate<'a> {
    /// * `paths` - must not be empty
    pub fn new(filename: Filename<'a>, paths: &'a [FdPath])
        -> Option<FexecvelCandidate<'a>>
    {
        if paths.is_empty() {
            return None;
        }

        Some(FexecvelCandidate { filename, paths })
    }

    pub fn get_filename(&'a self) -> &'a Filename<'a> {
        &self.filename
    }

    pub fn get_paths(&self) -> &'a [FdPath] {
        self.paths
    }
}

pub fn fexecvel(
    candidate: &FexecvelCandidate,
    argv: &CStrArray,
    envp: &CStrArray
) -> SyscallError
{
    let filename = candidate.get_filename().filename;

    let mut got_eaccess = false;

    for pathfd in candidate.get_paths().iter() {
        let err = execveat(*pathfd, filename, argv, envp, ExecveAtFlags::NONE);

        match err.get_errno() as i32 {
            libc::EACCES => {
                // Record that we got a 'Permission denied' error.  If we end
                // up finding no executable we can use, we want to diagnose
                // that we did find one but were denied access.
                got_eaccess = true;
                continue;
            },
            // Those errors indicate the file is missing or not executable
            // by us, in which case we want to just try the next path
            // directory.
            libc::ENOENT  => continue,
            libc::ESTALE  => continue,
            libc::ENOTDIR => continue,
            // Some strange filesystems like AFS return even
            // stranger error numbers.  They cannot reasonably mean
            // anything else so ignore those, too.
            libc::ENODEV    => continue,
            libc::ETIMEDOUT => continue,
    
            _ => return err,
        };
    }

    if got_eaccess {
        SyscallError::new(libc::EACCES as u32)
    } else {
        SyscallError::new(libc::ENOENT as u32)
    }
}

#[cfg(test)]
mod tests {
    use crate::{errx, autorestart};
    use crate::syscall::*;
    use crate::utility::{to_cstr, to_cstr_ptr};
    use crate::utility::tests::run;
    use std::os::raw::c_char;

    #[test]
    fn test_autorestart() {
        let mut cnt = 0;

        let result = autorestart(|| {
            if cnt == 0 {
                cnt = 1;
                Err(SyscallError::new(libc::EINTR as u32))
            } else {
                Ok(1)
            }
        });

        assert_matches!(result, Ok(1));
        assert_eq!(cnt, 1);
    }

    #[test]
    fn test_autorestart_macro() {
        let mut cnt = 0;

        let result = autorestart!({
            if cnt == 0 {
                cnt = 1;
                Err(SyscallError::new(libc::EINTR as u32))
            } else {
                Ok(1)
            }
        });

        assert_matches!(result, Ok(1));
        assert_eq!(cnt, 1);
    }

    #[test]
    fn test_impl_Write_for_Fd() {
        writeln!(STDERR.clone(), "Hello, world from test_impl_Write_for_Fd!");
    }

    #[test]
    fn test_execvel() {
        let paths = ["/bin", "/usr/bin"];

        let mut argvVec: Vec<*const c_char> =
            [to_cstr("echo\0").unwrap(), to_cstr("Hello\0").unwrap()]
            .iter()
            .map(to_cstr_ptr)
            .collect();
        argvVec.push(std::ptr::null());

        let argv = CStrArray::new(&argvVec).unwrap();

        let mut envpVec: Vec<*const c_char> = [to_cstr("A=B\0").unwrap()]
            .iter()
            .map(to_cstr_ptr)
            .collect();
        envpVec.push(std::ptr::null());

        let envp = CStrArray::new(&envpVec).unwrap();

        let run_program = |filename, argv| {
            let filename = Filename::new(filename).unwrap();
            let candidate = ExecvelCandidate::new(filename, &paths)
                .unwrap();
            assert_eq!(run(|| {
                errx!(1, "{}", execvel(&candidate, argv, &envp));
            }), 0);
        };
        run_program(cstr!("echo"), &argv);

        // Test out macro_rules CStrArray!
        const argv2: CStrArray = CStrArray!("env");
        run_program(cstr!("env"), &argv2);
    }

    #[test]
    fn test_fexecvel() {
        let pathBoxs = [
            FdPathBox::openat(AT_FDCWD, cstr!("/bin"), FdPathMode::anyPath, true)
                .unwrap(),
            FdPathBox::openat(AT_FDCWD, cstr!("/usr/bin"), FdPathMode::anyPath, true)
                .unwrap(),
        ];
        let paths = [*pathBoxs[0], *pathBoxs[1]];

        const envp: CStrArray = CStrArray!("A=B");

        let run_program = |filename, argv_var| {
            let filename = Filename::new(filename).unwrap();
            let candidate = FexecvelCandidate::new(filename, &paths)
                .unwrap();
            assert_eq!(run(|| {
                errx!(1, "{}", fexecvel(&candidate, argv_var, &envp));
            }), 0);
        };
        const argv: &'static CStrArray = &CStrArray!("echo", "Hello");
        run_program(cstr!("echo"), argv);

        // Test out macro_rules CStrArray!
        const argv2: &'static CStrArray = &CStrArray!("env");
        run_program(cstr!("env"), argv2);
    }
}
