/// TODO: 
///  - Move this code into another independent crate
use std::io::{Result, Error};
use std::os::raw::c_int;
use std::mem::{size_of, size_of_val, MaybeUninit};
use std::sync::Arc;

use libc::{signalfd, signalfd_siginfo, SFD_CLOEXEC, SFD_NONBLOCK, SIGCHLD};
use libc::{sigset_t, SIG_BLOCK, sigemptyset, sigaddset, sigprocmask};

use libc::pid_t;

use tokio::io::unix::AsyncFd;
use tokio::io::Interest;
use tokio::task::JoinHandle;

use waitmap::WaitMap;

use crate::autorestart;
use crate::syscall::{FdBox, FromRaw};

const SIGINFO_BUFSIZE: usize = 20;

fn waitid(idtype: libc::idtype_t, id: libc::id_t, options: c_int)
    -> Result<Option<libc::siginfo_t>>
{
    let mut siginfo = MaybeUninit::<libc::siginfo_t>::zeroed();

    let ret = unsafe {
        libc::waitid(idtype, id, siginfo.as_mut_ptr(), options)
    };
    if ret < 0 {
        return Err(Error::last_os_error());
    }

    let siginfo = unsafe { siginfo.assume_init() };
    if unsafe { siginfo.si_pid() } == 0 {
        Ok(None)
    } else {
        Ok(Some(siginfo))
    }
}

// Workaround for WaitMap's strange requirement in wait
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
struct Pid(libc::pid_t);
impl From<&Pid> for Pid {
    fn from(pid: &Pid) -> Pid {
        *pid
    }
}

/// Due to the fact that epoll on signalfd would fail after fork, you cannot use
/// SigChldFd after forked
pub struct SigChldFd {
    inner: AsyncFd<FdBox>,
    map: WaitMap<Pid, ExitInfo>
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
                let fd = inner.get_ref();

                Ok(
                    autorestart!({
                        fd.read(out)
                    })?
                )
            }) {
                Ok(result) => break result,
                Err(_would_block) => continue,
            }
        }
    }

    async fn read(&self) -> Result<()> {
        use libc::P_ALL;

        let waitid_option = libc::WEXITED | libc::WNOHANG;

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

            // Given that signal is an unreliable way of detecting 
            // SIGCHLD and can cause race condition when using waitid
            // (E.g. after reading all siginfo, some new SIGCHLD is generated
            // but these zombies are already released via watid)
            //
            // Thus it is considered better to just ignore the siginfo at all
            // and just use waitid instead.

            //let items = cnt / size_of::<signalfd_siginfo>();
            //let recevied_siginfos = &siginfos[0..items];
            //for siginfo in recevied_siginfos {
            //    let wstatus = siginfo.ssi_status;
            //    if ! (libc::WIFEXITED(wstatus) || libc::WIFSIGNALED(wstatus)) {
            //        continue;
            //    }

            //    let pid = siginfo.ssi_pid as pid_t;
            //    self.map.insert(
            //        pid,
            //        Ok(ExitInfo {
            //            uid: siginfo.ssi_uid,
            //            wstatus,
            //            utime: siginfo.ssi_utime as libc::clock_t,
            //            stime: siginfo.ssi_stime as libc::clock_t
            //        })
            //    );

            //    // release the zombie
            //    match waitid(P_PID, pid as id_t, waitid_option)? {
            //        Some(_) => (),
            //        None => errx!(1, "waitid cannot find zombie {}", pid)
            //    }
            //}

            // Continue to collect zombies whose SIGCHLD might get coalesced
            while let Some(siginfo) = waitid(P_ALL, 0, waitid_option)? {
                self.map.insert(
                    Pid(unsafe { siginfo.si_pid() }),
                    ExitInfo {
                        uid: unsafe { siginfo.si_uid() },
                        wstatus: unsafe { siginfo.si_status() },
                        utime: unsafe { siginfo.si_utime() },
                        stime: unsafe { siginfo.si_stime() }
                    }
                );
            }
        }
    }

    pub async fn wait(&self, pid: pid_t) -> ExitInfo {
        let pid = Pid(pid);
        loop {
            match self.map.wait(&pid).await {
                Some(val) => break *(val.value()),
                None => continue,
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
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
