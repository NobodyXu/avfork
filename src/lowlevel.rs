use std::ptr;
use std::mem;

use crate::error;
use crate::aspawn;

pub use error::SyscallError;
use error::toResult;

pub struct Stack {
    stack_impl: aspawn::Stack_t,
    allocated_obj_sz: usize,
    reserved_obj_sz: usize,
    reserved_stack_sz: usize,
}
impl Default for Stack {
    fn default() -> Stack {
        Stack::new()
    }
}
impl Stack {
    pub fn new() -> Stack {
        let mut stack = Stack {
            stack_impl: aspawn::Stack_t {
                addr: ptr::null_mut(),
                size: 0
            },
            allocated_obj_sz: 0,
            reserved_obj_sz: 0,
            reserved_stack_sz: 0,
        };
        unsafe {
            aspawn::init_cached_stack(&mut stack.stack_impl);
        }
        stack
    }

    /// * `stack_sz` - the length of stack to reserve
    /// * `obj_on_stack_len` - the size of all objects you want to put on this stack
    fn reserve(&mut self,
               reserved_stack_sz: usize, reserved_obj_sz: usize) -> Result<(), SyscallError> {
        unsafe {
            toResult(aspawn::reserve_stack(&mut self.stack_impl,
                                           reserved_stack_sz as u64,
                                           reserved_obj_sz as u64))?;
        }
        self.reserved_obj_sz = reserved_obj_sz;
        self.reserved_stack_sz = reserved_stack_sz;
        Ok(())
    }

    fn alloc_obj<T>(&mut self, obj: T) -> Result<&mut T, T> {
        let size = mem::size_of::<T>();
        if (self.allocated_obj_sz + size) > self.reserved_obj_sz {
            Err(obj)
        } else {
            let addr;
            unsafe {
                addr = aspawn::allocate_obj_on_stack(&mut self.stack_impl, size as u64);
            }

            let addr = addr as *mut T;
            unsafe {
                addr.write(obj);
                Ok(&mut (*addr))
            }
        }
    }
}
