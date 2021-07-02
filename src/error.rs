#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::CStr;
use std::fmt;
use std::slice::from_raw_parts;

use once_cell::sync::OnceCell;

include!(concat!(env!("OUT_DIR"), "/errno_msgs_binding.rs"));

/// * `result` - return value of syscall
pub const fn toResult(result: i64) -> Result<u64, SyscallError> {
    if result >= 0 {
        Ok(result as u64)
    } else {
        Err(SyscallError{
            errno: (-result) as u32
        })
    }
}

type errno_msgs_t =  [&'static str; errno_msgs_sz as usize];
pub fn get_errno_msgs() -> &'static errno_msgs_t {
    static ERRNO_MSGS: OnceCell<errno_msgs_t> = OnceCell::new();

    ERRNO_MSGS.get_or_init(|| {
        let mut strs  = ["1"; errno_msgs_sz as usize];
        let msgs = unsafe {
            from_raw_parts(get_errno_msgs_cstrs(), errno_msgs_sz as usize)
        };

        for (i, each) in strs.iter_mut().enumerate() {
            *each = unsafe { CStr::from_ptr(msgs[i]) }
                .to_str()
                .expect("Internal error: errno_msg defined in C cannot be used in Rust");
        }

        strs
    })
}

pub struct SyscallError {
    errno: u32,
}
impl SyscallError {
    pub const fn new(errno: u32) -> SyscallError {
        SyscallError { errno }
    }

    pub const fn get_errno(&self) -> i32 {
        self.errno as i32
    }
    pub fn get_msg(&self) -> &'static str {
        /* self.errno should be in range 1..errno_msgs_sz */
        if self.errno <= errno_msgs_sz as u32 {
            get_errno_msgs()[(self.errno as usize) - 1]
        } else {
            "Unknown errno code"
        }
    }
}
impl fmt::Display for SyscallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Errno {}: {}", self.errno, self.get_msg())
    }
}
impl fmt::Debug for SyscallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl From<SyscallError> for std::io::Error {
    fn from(err: SyscallError) -> Self {
        std::io::Error::from_raw_os_error(err.get_errno() as i32)
    }
}

#[cfg(test)]
mod tests {
    use crate::error::*;

    #[test]
    fn test_get_errno_msgs() {
        println!("{:#?}", get_errno_msgs());
    }
}
