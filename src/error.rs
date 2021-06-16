#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::CStr;
use std::fmt;

use once_cell::sync::OnceCell;

include!(concat!(env!("OUT_DIR"), "/errno_msgs_binding.rs"));

pub struct SyscallError {
    errno: u32,
}

/// * `result` - return value of syscall
pub fn toResult(result: i32) -> Result<u32, SyscallError> {
    if result >= 0 {
        Ok(result as u32)
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

        for (i, each) in strs.iter_mut().enumerate() {
            *each = unsafe { CStr::from_ptr(errno_msgs[i]) }
                .to_str()
                .expect("Internal error: errno_msg defined in C cannot be used in Rust");
        }

        strs
    })
}

// TODO: convert errno_msgs to str at compile-time
impl fmt::Display for SyscallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Errno {}: {}", self.errno, get_errno_msgs()[self.errno as usize])
    }
}
