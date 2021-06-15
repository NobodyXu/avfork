#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::CStr;
use std::fmt;

include!(concat!(env!("OUT_DIR"), "/errno_msgs_binding.rs"));

pub struct SyscallError {
    errno: u32,
}

/// @param result return value of syscall
pub fn toResult(result: i32) -> Result<u32, SyscallError> {
    if result >= 0 {
        Ok(result as u32)
    } else {
        Err(SyscallError{
            errno: (-result) as u32
        })
    }
}

impl fmt::Display for SyscallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = unsafe { CStr::from_ptr(errno_msgs[self.errno as usize]) }
            .to_str()
            .expect("Internal error: errno_msg defined in C cannot be used in Rust");
        write!(f, "Errno {}: {}", self.errno, msg)
    }
}
