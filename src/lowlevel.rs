use std::mem;
use std::pin::Pin;
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::os::raw::{c_void, c_int};
use std::marker::PhantomData;

use crate::expect;
use crate::error;
use crate::aspawn;
use crate::syscall;
use crate::utility;

pub use error::SyscallError;
use error::toResult;

pub use syscall::sigset_t;
pub use syscall::pid_t;
pub use syscall::{Fd, FdBox};
use syscall::FromRaw;

use utility::to_void_ptr;

#[derive(Debug)]
pub struct Stack {
    stack_impl: aspawn::Stack_t,
}
unsafe impl Send for Stack {}
impl Default for Stack {
    fn default() -> Stack {
        Stack::new()
    }
}
impl Drop for Stack {
    fn drop(&mut self) {
        let ret = unsafe {
            aspawn::cleanup_stack(&self.stack_impl) as i64
        };
        if cfg!(debug_assertions) {
            let result = toResult(ret);
            expect!(result, "Failed to deallocate the stack {:#?}", &self.stack_impl);
        }
    }
}
impl Stack {
    pub fn new() -> Stack {
        Stack {
            stack_impl: aspawn::new_stack_t(),
        }
    }

    /// * `reserved_stack_sz` - the length of stack to reserve. Only required
    ///   if you are doing recursive call or have a lot of local objects.
    ///   reserve would unconditionally allocate (32 * 1024) bytes for basic operations
    /// * `reserved_obj_sz` - the size of all objects you want to put on this stack
    ///   using StackObjectAllocator::alloc_obj
    ///
    /// **This API is safe to be used inside avfork callback.**
    pub fn reserve(&mut self, reserved_stack_sz: usize, reserved_obj_sz: usize)
        -> Result<StackObjectAllocator, SyscallError>
    {
        unsafe {
            toResult(aspawn::reserve_stack(&mut self.stack_impl,
                                           reserved_stack_sz as u64,
                                           reserved_obj_sz as u64) as i64)?;
        }
        Ok(StackObjectAllocator::new(self.stack_impl, reserved_obj_sz))
    }
}

/// StackObjectAllocator is a special class used to ensure that:
///  - any allocation on the stack only stay as long as StackObjectAllocator
///  - prevent reallocation of Stack
///
/// **All APIs of this struct are safe to be used inside avfork callback.**
pub struct StackObjectAllocator<'a> {
    /// Inferior Mutability is required since all StackBox created
    /// borrows StackObjectAllocator.
    ///
    /// So in order for multiple `StackBox`es to be allocated, StackObjectAllocator
    /// must be able to allocate using a immutable reference.
    ///
    /// cell stores a copy of Stack_t and alloc_obj_sz
    cell: UnsafeCell<(aspawn::Stack_t, usize)>,
    reserved_obj_sz: usize,
    phantom: PhantomData<&'a Stack>,
}

#[allow(non_camel_case_types)]
type Stack_t = aspawn::Stack_t;

impl<'a> StackObjectAllocator<'a> {
    fn new(stack_impl: Stack_t, reserved_obj_sz: usize)
        -> StackObjectAllocator<'a>
    {
        StackObjectAllocator {
            cell: UnsafeCell::new((stack_impl, 0)),
            reserved_obj_sz,
            phantom: PhantomData,
        }
    }

    pub fn alloc_obj<T>(&self, obj: T) -> Result<StackBox<T>, T> {
        let align = mem::align_of::<T>();
        let size = mem::size_of::<T>();

        let remnant = size % align;
        let size = size + if remnant != 0 { align - remnant } else { 0 };

        let alloc_obj_sz;
        let stack_impl;

        unsafe {
            let cell = &mut *self.cell.get();
            stack_impl = &mut cell.0;
            alloc_obj_sz = &mut cell.1;
        }

        if (*alloc_obj_sz + size) > self.reserved_obj_sz {
            Err(obj)
        } else {
            (*alloc_obj_sz) += size;

            let addr;
            unsafe {
                let size = size as u64;
                addr = aspawn::allocate_obj_on_stack(stack_impl, size);
            }

            let addr = addr as *mut T;
            unsafe {
                // overwrite addr without dropping
                addr.write(obj);
            }
            Ok(StackBox::new(addr))
        }
    }
}
impl<'a> std::fmt::Debug for StackObjectAllocator<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reserved_obj_sz = &self.reserved_obj_sz;

        let alloc_obj_sz;
        let stack_impl;

        unsafe {
            let cell = & *self.cell.get();
            stack_impl = &cell.0;
            alloc_obj_sz = &cell.1;
        }

        write!(f, "{{stack = {:#?}, alloc_obj_sz = {:#?}, reserved_obj_sz = {:#?}}}",
                  stack_impl, alloc_obj_sz, reserved_obj_sz)
    }
}

/// **All APIs of this struct are safe to be used inside avfork callback.**
#[derive(Debug)]
pub struct StackBox<'a, T> {
    ptr: *mut T,
    phantom: PhantomData<&'a T>,
}
impl<'a, T> StackBox<'a, T> {
    fn new(ptr: *mut T) -> StackBox<'a, T> {
        StackBox {
            ptr,
            phantom: PhantomData
        }
    }
    pub fn pin(&'a self) -> Pin<&'a T> {
        unsafe { Pin::new_unchecked(&self) }
    }
}
impl<'a, T> Drop for StackBox<'a, T> {
    fn drop(&mut self) {
        unsafe {
            self.ptr.drop_in_place();
        }
    }
}
impl<'a, T> Deref for StackBox<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { & *self.ptr }
    }
}
impl<'a, T> DerefMut for StackBox<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}

unsafe extern "C"
fn aspawn_fn<Func>(arg: *mut c_void, write_end_fd: c_int, old_sigset: *mut c_void) 
    -> c_int where Func: Fn(Fd, &mut sigset_t) -> c_int 
{
    let func = & *(arg as *const Func);

    func(Fd::from_raw(write_end_fd), &mut *(old_sigset as *mut sigset_t))
}

/// * `func` - takes a Fd and sigset of the parent program, returns a c_int as 
///   exit status.
///   When this function is called, it is guaranteed that:
///    - all signals are masked,
///    - all signal handlers are cleared,
///   
///   **WARNING**: func should not panic or allocate anything on heap
///                It also should not close the fd passed in, otherwise its stack
///                might get invalidated and SIGSEGV.
///
/// Returns fd of read end of CLOEXEC pipe and the pid of the child process.
///
/// avfork would disable thread cancellation, then it would revert it before return.
///
/// It would also mask all signals in parent and reset the signal handler in 
/// the child process.
/// Before aspawn returns in parent, it would revert the signal mask.
///
/// In the function fn, you can only use syscall declared in syscall
/// Use of any glibc function or any function that modifies 
/// global/thread-local variable is undefined behavior.
///
/// # Example
///
/// Check directory `examples/avfork.rs` for example on this function.
pub fn avfork<Func>(stack_alloc: &StackObjectAllocator, func: Pin<&Func>)
    -> Result<(FdBox, pid_t), SyscallError> where Func: Fn(Fd, &mut sigset_t) -> c_int 
{
    use aspawn::aspawn;

    let stack = unsafe { (*stack_alloc.cell.get()).0 };
    let func_ref = func.get_ref();

    let mut pid: pid_t = 0;

    let callback = Option::Some(
        aspawn_fn::<Func> as unsafe extern "C" fn (_, _, _) -> _
    );
    
    let fd = toResult(unsafe {
        aspawn(&mut pid, &stack, callback, to_void_ptr(func_ref) as *mut c_void) as i64
    })?;

    Ok((unsafe { FdBox::from_raw(fd as i32) }, pid))
}

/// **NOT TESTED**
///
/// * `func` - takes a Fd and sigset of the parent program, returns a c_int as 
///   exit status.
///   When this function is called, it is guaranteed that:
///    - all signals are masked,
///    - all signal handlers are cleared,
///   **WARNING**: func should not panic or allocate anything on heap
///                It also should not close the fd passed in, otherwise its stack
///                might get invalidated and SIGSEGV.
///
/// * `old_sigset` - you should pass the sigset argument in your AspawnFn
/// Returns fd of read end of CLOEXEC pipe and the pid of the child process.
///
/// avfork would disable thread cancellation, then it would revert it before return.
///
/// It would also mask all signals in parent and reset the signal handler in 
/// the child process.
/// Before aspawn returns in parent, it would revert the signal mask.
///
/// In the function fn, you can only use syscall declared in syscall
/// Use of any glibc function or any function that modifies 
/// global/thread-local variable is undefined behavior.
pub fn avfork_rec<Func>(
    stack_alloc: &StackObjectAllocator, func: Pin<&Func>, old_sigset: &sigset_t)
    -> Result<(FdBox, pid_t), SyscallError> where Func: Fn(Fd, &mut sigset_t) -> c_int 
{
    use aspawn::aspawn_rec;

    let stack = unsafe { (*stack_alloc.cell.get()).0 };
    let func_ref = func.get_ref();

    let mut pid: pid_t = 0;

    let callback = Option::Some(
        aspawn_fn::<Func> as unsafe extern "C" fn (_, _, _) -> _
    );
    
    let fd = toResult(unsafe {
        let arg = to_void_ptr(func_ref) as *mut c_void;
        aspawn_rec(&mut pid, &stack, callback, arg, to_void_ptr(old_sigset)) as i64
    })?;

    Ok((unsafe { FdBox::from_raw(fd as i32) }, pid))
}

#[cfg(test)]
mod tests {
    use crate::lowlevel::*;
    use std::fs::File;
    use std::io;

    #[test]
    fn test_stack_init() {
        let _stack = Stack::new();
    }

    #[test]
    fn test_stack_impossible_alloc() {
        let mut stack = Stack::new();

        let allocator = stack.reserve(0, mem::size_of::<u64>()).unwrap();

        let _i1 = allocator.alloc_obj(1 as u64).unwrap();
        
        assert_matches!(allocator.alloc_obj(2333), Result::Err(2333));
    }

    #[test]
    fn test_stack_reserve() {
        let mut stack = Stack::new();

        for _ in 0..3 {
            type T = Box::<i64>;
            let allocator = stack.reserve(0, 200 * mem::size_of::<T>()).unwrap();

            let mut vars = Vec::<StackBox::<T>>::new();

            // simulate allocating 100 variables on the stack
            for i in 0..100 {
                let mut b = allocator.alloc_obj(T::new(i)).unwrap();
                assert_eq!(**b, i);
                *b = T::new(2000);
                assert_eq!(**b, 2000);

                vars.push(b);
            }

            for b in &vars {
                assert_eq!(***b, 2000);
            }
        }
    }

    #[test]
    fn test_stackbox_pin() {
        let mut stack = Stack::new();

        {
            type T = Box::<i64>;
            let allocator = stack.reserve(0, mem::size_of::<T>()).unwrap();

            let b = allocator.alloc_obj(T::new(2333)).unwrap();

            let p1 = b.pin().get_ref() as *const _;
            let p2 = (& *b) as *const _;

            assert_eq!(p1, p2);
        }
    }

    fn print_maps() {
        let mut file = File::open("/proc/self/maps").unwrap();
        io::copy(&mut file, &mut io::stdout()).unwrap();
    }

    fn test_callback<F: Fn(Fd, &mut sigset_t) -> c_int + Copy >(f: F) {
        let mut stack = Stack::new();

        for _ in 0..10 {
            let allocator = stack.reserve(4096 * 100, 100).unwrap();

            println!("allocator = {:#?}", allocator);
            print_maps();

            let f = match allocator.alloc_obj(f) {
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

    fn dummy_avfork_callback(_fd: Fd, _old_sigset: &mut sigset_t) -> c_int {
        0
    }

    #[test]
    fn test_avfork_naive() {
        test_callback(dummy_avfork_callback);
    }

    fn test_avfork_exec_callback(_fd: Fd, _old_sigset: &mut sigset_t) -> c_int {
        use crate::syscall::*;
        use crate::{CStrArray, errx};

        let err = execve(
            &cstr!("/bin/echo"),
            &CStrArray!("/bin/echo", "Hello", "World!"),
            &CStrArray!("A=B")
        );
        errx!(1, "execve failed: {}", err);
    }

    #[test]
    fn test_avfork_exec() {
        test_callback(test_avfork_exec_callback);
    }

    fn test_avfork_cd_exec_callback(_fd: Fd, _old_sigset: &mut sigset_t) -> c_int {
        use crate::syscall::*;
        use crate::{CStrArray, errx};
        use crate::utility::unwrap;

        unwrap(chdir(&cstr!("/tmp")));

        let err = execve(
            &cstr!("/bin/ls"),
            &CStrArray!("/bin/ls"),
            &CStrArray!("A=B")
        );
        errx!(1, "execve failed: {}", err);
    }

    //#[test]
    //fn test_avfork_cd_exec() {
    //    test_callback(test_avfork_cd_exec_callback);
    //}


    //fn dummy_avfork_rec_callback(fd: Fd, old_sigset: &mut sigset_t) -> c_int {
    //    let mut stack = Stack::new();

    //    let allocator = match stack.reserve(0, 100) {
    //        Ok(alloc) => alloc,
    //        // TODO: Print SyscallError
    //        Err(_) => {
    //            let _ = fd.write("Failed to allocate stack".as_bytes());
    //            return 1
    //        }
    //    };

    //    let f = match allocator.alloc_obj(dummy_avfork_callback) {
    //        Ok(f) => f,
    //        Err(_) => {
    //            let _ = fd.write("allocation failed".as_bytes());
    //            return 1
    //        },
    //    };

    //    let (fd2, _pid) = match avfork_rec(&allocator, f.pin(), old_sigset) {
    //        Ok(ret) => ret,
    //        Err(err) => {
    //            let _ = fd.write(err.get_msg().as_bytes());
    //            return 1
    //        }
    //    };

    //    let mut buf = [1 as u8; 1];
    //    match fd2.read(&mut buf) {
    //        Ok(cnt) => {
    //            if cnt != 0 {
    //                let _ = fd.write("cnt != 0 in dummy_avfork_rec_callback".as_bytes());
    //                1
    //            } else {
    //                0
    //            }
    //        },
    //        Err(_) => {
    //            let _ = fd.write("There shouldn't be any error".as_bytes());
    //            1
    //        }
    //    }
    //}

    //#[test]
    //fn test_avfork_rec_naive() {
    //    let mut stack = Stack::new();
    //    // Allocate 100 pages for dummy_avfork_rec_callback, since under debugging mode
    //    // with asan, memory can be eaten up pretty easily.
    //    let allocator = stack.reserve(100 * 4096, 100).unwrap();

    //    let f = match allocator.alloc_obj(dummy_avfork_rec_callback) {
    //        Ok(f) => f,
    //        Err(_) => panic!("allocation failed"),
    //    };

    //    println!("allocator = {:#?}", allocator);

    //    let mut file = File::open("/proc/self/maps").unwrap();
    //    io::copy(&mut file, &mut io::stdout()).unwrap();

    //    let (fd, _pid) = avfork(&allocator, f.pin()).unwrap();

    //    let mut buf = [200 as u8; 1];
    //    match fd.read(&mut buf) {
    //        Ok(cnt) => {
    //            if cnt != 0 {
    //                let err_msg = std::str::from_utf8(&buf).unwrap();
    //                panic!("dummy_avfork_rec_callback failed: {}", err_msg);
    //            }
    //        },
    //        Err(_) => panic!("There shouldn't be any error")
    //    };
    //}

    //fn get_dummy_avfork_rec_callback<'a, Func>(
    //    alloc: &'a StackObjectAllocator,
    //    f: Pin<&'a Func>
    //) -> impl Fn(Fd, &mut sigset_t) -> c_int + 'a
    //    where Func: Fn(Fd, &mut sigset_t) -> c_int
    //{
    //    move |fd: Fd, old_sigset: &mut sigset_t| -> c_int {
    //        let (fd2, _pid) = match avfork_rec(alloc, f, old_sigset) {
    //            Ok(ret) => ret,
    //            Err(err) => {
    //                let _ = fd.write(err.get_msg().as_bytes());
    //                return 1
    //            }
    //        };

    //        let mut buf = [1 as u8; 1];
    //        match fd2.read(&mut buf) {
    //            Ok(cnt) => {
    //                if cnt != 0 {
    //                    let _ = fd.write("cnt != 0 in dummy_avfork_rec_callback".as_bytes());
    //                    1
    //                } else {
    //                    0
    //                }
    //            },
    //            Err(_) => {
    //                let _ = fd.write("There shouldn't be any error".as_bytes());
    //                1
    //            }
    //        }
    //    }
    //}

    //#[test]
    //fn test_avfork_rec_naive2() {
    //    let mut stack2 = Stack::new();
    //    let alloc2 = stack2.reserve(0, 100).unwrap();

    //    let f2 = match alloc2.alloc_obj(dummy_avfork_callback) {
    //        Ok(f) => f,
    //        Err(_) => panic!("allocation failed"),
    //    };

    //    let dummy_avfork_rec_callback = get_dummy_avfork_rec_callback(&alloc2, f2.pin());

    //    let mut stack = Stack::new();
    //    // Allocate 100 pages for dummy_avfork_rec_callback, since under debugging mode
    //    // with asan, memory can be eaten up pretty easily.
    //    let allocator = stack.reserve(100 * 4096, 100).unwrap();

    //    let f = match allocator.alloc_obj(dummy_avfork_rec_callback) {
    //        Ok(f) => f,
    //        Err(_) => panic!("allocation failed"),
    //    };

    //    println!("allocator = {:#?}", allocator);

    //    let mut file = File::open("/proc/self/maps").unwrap();
    //    io::copy(&mut file, &mut io::stdout()).unwrap();

    //    let (fd, _pid) = avfork(&allocator, f.pin()).unwrap();

    //    let mut buf = [200 as u8; 1];
    //    match fd.read(&mut buf) {
    //        Ok(cnt) => {
    //            if cnt != 0 {
    //                let err_msg = std::str::from_utf8(&buf).unwrap();
    //                panic!("dummy_avfork_rec_callback failed: {}", err_msg);
    //            }
    //        },
    //        Err(_) => panic!("There shouldn't be any error")
    //    };
    //}
}
