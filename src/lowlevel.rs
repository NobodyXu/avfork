use std::mem;
use std::pin::Pin;
use std::os::raw::c_void;

use crate::error;
use crate::aspawn;
use crate::syscall;

pub use error::SyscallError;
use error::toResult;

pub use syscall::sigset_t;
pub use syscall::pid_t;
pub use syscall::c_int;
pub use syscall::wrapper::Fd;

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
        unsafe {
            aspawn::cleanup_stack(&self.stack_impl);
        }
    }
}
impl Stack {
    pub fn new() -> Stack {
        Stack {
            stack_impl: aspawn::new_stack_t(),
        }
    }

    /// * `reserved_stack_sz` - the length of stack to reserve
    /// * `reserved_obj_sz` - the size of all objects you want to put on this stack
    pub fn reserve(&mut self, reserved_stack_sz: usize, reserved_obj_sz: usize)
        -> Result<StackObjectAllocator, SyscallError>
    {
        unsafe {
            toResult(aspawn::reserve_stack(&mut self.stack_impl,
                                           reserved_stack_sz as u64,
                                           reserved_obj_sz as u64))?;
        }
        Ok(StackObjectAllocator::new(&self.stack_impl, self.stack_impl, reserved_obj_sz))
    }
}

/// StackObjectAllocator is a special class used to ensure that:
///  - any allocation on the stack only stay as long as StackObjectAllocator
///  - prevent reallocation of Stack
pub struct StackObjectAllocator<'a> {
    #[allow(dead_code)]
    reference: &'a aspawn::Stack_t,
    stack_impl: aspawn::Stack_t,
    reserved_obj_sz: usize,
    alloc_obj_sz: usize,
}

#[allow(non_camel_case_types)]
type Stack_t = aspawn::Stack_t;

impl<'a> StackObjectAllocator<'a> {
    fn new(reference: &'a Stack_t, stack_impl: Stack_t, reserved_obj_sz: usize)
        -> StackObjectAllocator<'a>
    {
        StackObjectAllocator {
            reference,
            stack_impl,
            reserved_obj_sz,
            alloc_obj_sz: 0,
        }
    }

    pub fn alloc_obj<T>(&mut self, obj: T) -> Result<&mut T, T> {
        let size = mem::size_of::<T>();
        if (self.alloc_obj_sz + size) > self.reserved_obj_sz {
            Err(obj)
        } else {
            let addr;
            unsafe {
                addr = aspawn::allocate_obj_on_stack(&mut self.stack_impl, size as u64);
            }

            let addr = addr as *mut T;
            unsafe {
                // overwrite addr without dropping
                addr.write(obj);
                Ok(&mut (*addr))
            }
        }
    }
}

/// AspawnFn takes a Fd and sigset and returns a c_int as exit status
pub trait AvforkFn: Fn(Fd, &mut sigset_t) -> c_int {}

unsafe extern "C" fn aspawn_fn<Func: AvforkFn>(arg: *mut c_void, write_end_fd: c_int,
                             old_sigset: *mut c_void) -> c_int {
    let func = & *(arg as *const Func);

    let fd = match Fd::new(write_end_fd) {
        Ok(fd) => fd,
        Err(_) => return 1
    };

    func(fd, &mut *(old_sigset as *mut sigset_t))
}

pub fn avfork<Func: AvforkFn>(stack_alloc: &StackObjectAllocator, func: Pin<&Func>)
    -> Result<pid_t, SyscallError>
{
    use aspawn::aspawn;

    let mut stack = stack_alloc.stack_impl;
    let func_ref = func.get_ref();

    let mut pid: pid_t = 0;

    let callback = Option::Some(
        aspawn_fn::<Func> as unsafe extern "C" fn (_, _, _) -> _
    );
    
    toResult(unsafe {
        aspawn(&mut pid, &mut stack, callback, func_ref as *const _ as *mut c_void)
    })?;

    Ok(pid)
}
