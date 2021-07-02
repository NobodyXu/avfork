use avfork::lowlevel::*;
use avfork::syscall::*;
use avfork::{CStrArray, errx};
use avfork::utility::unwrap;
use avfork::cstr::cstr;

fn dummy_avfork_callback(_fd: Fd, _old_sigset: &mut sigset_t) -> c_int {
    unwrap(chdir(&cstr!("/tmp")));

    let err = execve(
        &cstr!("/bin/ls"),
        &CStrArray!("/bin/ls"),
        &CStrArray!("A=B")
    );
    errx!(1, "execve failed: {}", err);
}

fn main() {
    let mut stack = Stack::new();

    for _ in 0..10 {
        let allocator = stack.reserve(0, 100).unwrap();

        let f = match allocator.alloc_obj(dummy_avfork_callback) {
            Ok(f) => f,
            Err(_) => panic!("allocation failed"),
        };

        println!("Calling avfork");

        let (fd, _pid) = avfork(&allocator, f.pin()).unwrap();

        println!("avfork is done");

        println!("Wait for child process to exit or exec");

        let mut buf = [1 as u8; 1];
        match fd.read(&mut buf) {
            Ok(cnt) => assert_eq!(0, cnt),
            Err(_) => panic!("There shouldn't be any error")
        };

        println!("Test completed");
    }
}
