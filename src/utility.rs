use std::os::raw::{c_void, c_int, c_char};
use std::ffi::{CStr, FromBytesWithNulError};

use std::io::Write;
use crate::syscall::{STDERR, exit};

pub fn to_void_ptr<T>(reference: &T) -> *const c_void {
    reference as *const _ as *const c_void
}

pub fn to_void_ptr_mut<T>(reference: &mut T) -> *mut c_void {
    reference as *mut _ as *mut c_void
}

pub fn to_cstr(s: &str) -> Result<&CStr, FromBytesWithNulError> {
    CStr::from_bytes_with_nul(s.as_bytes())
}

/// Usage:
/// 
///     [to_cstr("Hello").unwrap()].iter().map(to_cstr_ptr).collect()
pub fn to_cstr_ptr(s: &&CStr) -> *const c_char {
    s.as_ptr()
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
                    $x,
                )*
            )
        )
    };
}

pub fn expect<T, E: std::fmt::Debug>(result: Result<T, E>, msg: &str) -> T {
    match result {
        Ok(val) => val,
        Err(err) => errx_impl(1, std::format_args!("{}: {:#?}", msg, err))
    }
}
pub fn unwrap<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    expect(result, "unwrap failed")
}

pub fn expect_fmt<T, E>(result: Result<T, E>, args: std::fmt::Arguments)
    -> T where E: std::fmt::Debug
{
    match result {
        Ok(val) => val,
        Err(err) => errx_impl(1, std::format_args!("{}: {:#?}", args, err))
    }
}

#[macro_export]
macro_rules! expect {
    ( $result:expr $( , $x:expr )* ) => {
        $crate::utility::expect_fmt($result, 
            std::format_args!(
                $(
                    $x,
                )*
            )
        )
    };
}


#[cfg(test)]
pub mod tests {
    use crate::utility::*;
    use crate::syscall;

    pub fn run<F: FnOnce()>(f: F) -> c_int {
        let pid = unsafe { libc::fork() };

        if pid == 0 {
            f();

            syscall::exit(0);
        } else {
            let mut status = -1 as c_int;
    
            unsafe {
                assert_eq!(pid, libc::waitpid(pid, &mut status as *mut _, 0));
            };

            libc::WEXITSTATUS(status)
        }
    }

    #[test]
    fn test_errx() {
        assert_eq!(run(|| crate::errx!(0, "Hello, world from test_errx!")), 0);
        assert_eq!(run(|| crate::errx!(0, "{}", "Hello, world from test_errx!")), 0);
    }

    const ERR: Result<(), &'static str> = Err("Error");

    #[test]
    fn test_expect() {
        assert_eq!(run(|| expect(ERR, "Expected failure")), 1);
    }

    #[test]
    fn test_expect_fmt() {
        assert_eq!(
            run(|| expect!(ERR, "Expected failure {}", 1)),
            1
        );
    }

    #[test]
    fn test_unwrap() {
        assert_eq!(run(|| unwrap(ERR)), 1);
    }
}
