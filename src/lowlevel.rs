use std::mem;
use std::pin::Pin;
use std::ops::{Deref, DerefMut};
use std::os::raw::{c_void, c_int};
use std::marker::PhantomData;

use crate::error;
use crate::aspawn;
use crate::syscall;
use crate::utility;

pub use error::SyscallError;
use error::toResult;

pub use syscall::sigset_t;
pub use syscall::pid_t;
pub use syscall::Fd;

use utility::to_void_ptr;

pub struct Stack {
    stack_impl: aspawn::Stack_t,
}
impl Default for Stack {
    fn default() -> Stack {
        Stack::new()
    }
}
impl Drop for Stack {
    fn drop(&mut self) {
        toResult(unsafe {
            aspawn::cleanup_stack(&self.stack_impl)
        }).expect("Failed to deallocate the stack");
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
    ///   reserve would unconditionally allocate (32 * 1024) bytes for basic operations.
    /// * `reserved_obj_sz` - the size of all objects you want to put on this stack
    ///   using StackObjectAllocator::alloc_obj
    pub fn reserve(&mut self, reserved_stack_sz: usize, reserved_obj_sz: usize)
        -> Result<StackObjectAllocator, SyscallError>
    {
        unsafe {
            toResult(aspawn::reserve_stack(&mut self.stack_impl,
                                           reserved_stack_sz as u64,
                                           reserved_obj_sz as u64))?;
        }
        Ok(StackObjectAllocator::new(self.stack_impl, reserved_obj_sz))
    }
}

/// StackObjectAllocator is a special class used to ensure that:
///  - any allocation on the stack only stay as long as StackObjectAllocator
///  - prevent reallocation of Stack
pub struct StackObjectAllocator<'a> {
    stack_impl: aspawn::Stack_t,
    reserved_obj_sz: usize,
    alloc_obj_sz: usize,
    phantom: PhantomData<&'a Stack>
}

#[allow(non_camel_case_types)]
type Stack_t = aspawn::Stack_t;

impl<'a> StackObjectAllocator<'a> {
    fn new(stack_impl: Stack_t, reserved_obj_sz: usize)
        -> StackObjectAllocator<'a>
    {
        StackObjectAllocator {
            stack_impl,
            reserved_obj_sz,
            alloc_obj_sz: 0,
            phantom: PhantomData,
        }
    }

    pub fn alloc_obj<T>(&mut self, obj: T) -> Result<StackBox<T>, T> {
        let align = mem::align_of::<T>();
        let size = mem::size_of::<T>();

        let remnant = size % align;
        let size = size + if remnant != 0 { align - remnant } else { 0 };

        if (self.alloc_obj_sz + size) > self.reserved_obj_sz {
            Err(obj)
        } else {
            let addr;
            unsafe {
                let size = size as u64;
                addr = aspawn::allocate_obj_on_stack(&mut self.stack_impl, size);
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
    pub fn pin(&self) -> Pin<&T> {
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

/// AspawnFn takes a Fd and sigset of the parent program, returns a c_int as exit status
/// When this function is called, it is guaranteed that:
///  - all signals are masked,
///  - all signal handlers are cleared,
///
/// **WARNING**: struct implements AspawnFn should not panic or allocate anything on heap
pub trait AvforkFn: Fn(Fd, &mut sigset_t) -> c_int {}

unsafe extern "C"
fn aspawn_fn<Func: AvforkFn>(arg: *mut c_void, write_end_fd: c_int,
                             old_sigset: *mut c_void) -> c_int {
    let func = & *(arg as *const Func);

    func(Fd::from_raw(write_end_fd), &mut *(old_sigset as *mut sigset_t))
}

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
pub fn avfork<Func: AvforkFn>(stack_alloc: &StackObjectAllocator, func: Pin<&Func>)
    -> Result<(Fd, pid_t), SyscallError>
{
    use aspawn::aspawn;

    let stack = stack_alloc.stack_impl;
    let func_ref = func.get_ref();

    let mut pid: pid_t = 0;

    let callback = Option::Some(
        aspawn_fn::<Func> as unsafe extern "C" fn (_, _, _) -> _
    );
    
    let fd = toResult(unsafe {
        aspawn(&mut pid, &stack, callback, to_void_ptr(func_ref))
    })?;

    Ok((Fd::from_raw(fd as i32), pid))
}

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
pub fn avfork_rec<Func: AvforkFn>(
    stack_alloc: &StackObjectAllocator, func: Pin<&Func>, old_sigset: &sigset_t)
    -> Result<(Fd, pid_t), SyscallError>
{
    use aspawn::aspawn_rec;

    let stack = stack_alloc.stack_impl;
    let func_ref = func.get_ref();

    let mut pid: pid_t = 0;

    let callback = Option::Some(
        aspawn_fn::<Func> as unsafe extern "C" fn (_, _, _) -> _
    );
    
    let fd = toResult(unsafe {
        let arg = to_void_ptr(func_ref);
        aspawn_rec(&mut pid, &stack, callback, arg, to_void_ptr(old_sigset))
    })?;

    Ok((Fd::from_raw(fd as i32), pid))
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
