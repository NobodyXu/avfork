use std::io::{Result, Error};
use std::os::raw::{c_void, c_int, c_long, c_char};
use std::mem::{self, size_of, size_of_val, MaybeUninit};
use std::pin::Pin;
use std::sync::Arc;

use libc::{signalfd, signalfd_siginfo, SFD_CLOEXEC, SFD_NONBLOCK, SIGCHLD};
use libc::{sigset_t, SIG_BLOCK, sigemptyset, sigaddset, sigprocmask};

use libc::pid_t;

use tokio::io::unix::AsyncFd;
use tokio::io::Interest;
use tokio::task::JoinHandle;

use waitmap::WaitMap;

use crate::syscall::{FdBox, FromRaw};

const SIGINFO_BUFSIZE: usize = 20;

/// Due to the fact that epoll on signalfd would fail after fork, you cannot use
/// SigChldFd after forked
pub struct SigChldFd {
    inner: AsyncFd<FdBox>,
    map: WaitMap<pid_t, ExitInfo>
}
impl SigChldFd {
    pub fn new() -> Result<(Arc<SigChldFd>, JoinHandle<Result<()>>)> {
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

        let ret = Arc::new(SigChldFd {
            inner: AsyncFd::with_interest(fd, Interest::READABLE)?,
            map: WaitMap::new()
        });

        let sigfd = ret.clone();
        Ok(
            (
                ret,
                tokio::spawn(async move {
                    sigfd.read().await
                })
            )
        )
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
        let mut siginfos: [signalfd_siginfo; SIGINFO_BUFSIZE] = unsafe {
            // signalfd_siginfo does not initialization
            MaybeUninit::zeroed().assume_init()
        };

        let bytes = unsafe {
            std::slice::from_raw_parts_mut(
                siginfos.as_mut_ptr() as *mut u8,
                size_of_val(&siginfos)
            )
        };

        loop {
            let cnt = self.read_bytes(bytes).await?;

            assert_eq!(cnt % size_of::<signalfd_siginfo>(), 0);
            let items = cnt / size_of::<signalfd_siginfo>();

            let recevied_siginfos = &siginfos[0..items];
            for siginfo in recevied_siginfos {
                let wstatus = siginfo.ssi_status;
                if libc::WIFEXITED(wstatus) || libc::WIFSIGNALED(wstatus) {
                    self.map.insert(
                        siginfo.ssi_pid as pid_t,
                        ExitInfo {
                            uid: siginfo.ssi_uid,
                            wstatus,
                            utime: siginfo.ssi_utime as libc::clock_t,
                            stime: siginfo.ssi_stime as libc::clock_t
                        }
                    );
                }
            }
        }
    }

    pub async fn wait(&self, pid: pid_t) -> Result<ExitInfo> {
        ;

        unimplemented!()
    }
}

pub struct ExitInfo {
    /// uid of the child when it exits
    uid: libc::uid_t,
    /// exit status of the child
    wstatus: c_int,
    /// user time consumed
    utime: libc::clock_t,
    /// system time consumed
    stime: libc::clock_t,
}
impl ExitInfo {
    /// uid of the process when it exits
    pub fn get_uid(&self) -> libc::uid_t {
        self.uid
    }

    /// user time consumed by the process
    pub fn get_utime(&self) -> libc::clock_t {
        self.utime
    }

    /// system time consumed by the process
    pub fn get_stime(&self) -> libc::clock_t {
        self.stime
    }

    /// Get exit status if the child terminated normally instead of terminated
    /// by signal
    pub fn get_exit_status(&self) -> Option<c_int> {
        if libc::WIFEXITED(self.wstatus) {
            Some(libc::WEXITSTATUS(self.wstatus))
        } else {
            None
        }
    }

    /// Get the signal that terminated the process if it is killed by signal
    pub fn get_term_sig(&self) -> Option<c_int> {
        if libc::WIFSIGNALED(self.wstatus) {
            Some(libc::WTERMSIG(self.wstatus))
        } else {
            None
        }
    }
}
