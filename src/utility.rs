use std::os::raw::{c_void, c_int, c_char};
use std::ffi::CStr;

use std::io::Write;
use crate::syscall::{STDERR, exit};

pub fn to_void_ptr<T>(reference: &T) -> *const c_void {
    reference as *const _ as *const c_void
}

pub fn to_void_ptr_mut<T>(reference: &mut T) -> *mut c_void {
    reference as *mut _ as *mut c_void
}

fn to_cstr_ptr(s: &&CStr) -> *const c_char {
    s.as_ptr()
}

pub fn to_cstr_ptrs<'a>(in_arr: &'a [&CStr]) -> impl std::iter::Iterator + 'a {
    in_arr.iter().map(to_cstr_ptr)
}

pub fn errx_impl(exit_status: c_int, args: std::fmt::Arguments) -> ! {
    let _ = writeln!(STDERR.clone(), "Fatal Error: {}", args);
    exit(exit_status)
}

#[macro_export]
macro_rules! errx {
    ( $status:expr $( , $x:expr )* ) => {
        $crate::utility::errx_impl($status, 
            std::format_args!(
                $(
                    $x
                )*
            )
        )
    };
}

// Implement eprintln, errx

#[cfg(test)]
mod tests {
    #[macro_use]
    use crate::utility::*;

    #[test]
    fn test_errx() {
        let pid = unsafe { libc::fork() };

        if pid == 0 {
            errx!(0, "Hello, world from test_errx!");
        } else {
            let mut status = -1 as c_int;
    
            unsafe {
                assert_eq!(pid, libc::waitpid(pid, &mut status as *mut _, 0));
            };

            assert_eq!(status, 0);
        }
    }
}
