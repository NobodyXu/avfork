use std::io::{Result, Error};
use std::os::raw::{c_void, c_int, c_long, c_char};

use libc::{signalfd, signalfd_siginfo, SFD_CLOEXEC, SFD_NONBLOCK, SIGCHLD};
use libc::{sigset_t, SIG_BLOCK, sigemptyset, sigaddset, sigprocmask};

use tokio::io::unix::AsyncFd;
use tokio::io::Interest;

use crate::syscall::{FdBox, FromRaw};

/// Due to the fact that epoll on signalfd would fail after fork, you cannot use
/// SigChldFd after forked
pub struct SigChldFd {
    inner: AsyncFd<FdBox>
}
impl SigChldFd {
    pub fn new() -> Result<SigChldFd> {
        let mut mask = std::mem::MaybeUninit::<sigset_t>::uninit();
        unsafe {
            if sigemptyset(mask.as_mut_ptr()) < 0 {
                return Err(Error::last_os_error());
            }
            if sigaddset(mask.as_mut_ptr(), SIGCHLD) < 0 {
                return Err(Error::last_os_error());
            }
        };
        let mask = unsafe { mask.assume_init() };

        if unsafe {
            sigprocmask(SIG_BLOCK, &mask as *const _, std::ptr::null_mut())
        } < 0 {
            return Err(Error::last_os_error());
        }

        let fd = unsafe {
            signalfd(-1, &mask as *const _, SFD_NONBLOCK | SFD_CLOEXEC)
        };
        if fd < 0 {
            return Err(Error::last_os_error());
        }

        let fd = unsafe { FdBox::from_raw(fd) };

        Ok(SigChldFd {
            inner: AsyncFd::with_interest(fd, Interest::READABLE)?
        })
    }

    async fn read_bytes(&self, out: &mut [u8]) -> Result<usize> {
        loop {
            let mut guard = self.inner.readable().await?;

            match guard.try_io(|inner| -> Result<usize> {
                Ok(inner.get_ref().read(out)?)
            }) {
                Ok(result) => break result,
                Err(_would_block) => continue,
            }
        }
    }

    async fn read(&self) -> Result<()> {
        ;

        unimplemented!()
    }
}

pub struct ExitInfo {
    /// pid of the child
    si_pid: libc::pid_t,
    /// uid of the child when it exits
    si_uid: libc::uid_t,
    /// exit status of the child
    si_status: c_int,
    /// user time consumed
    si_utime: libc::clock_t,
    /// system time consumed
    si_stime: libc::clock_t,
}
