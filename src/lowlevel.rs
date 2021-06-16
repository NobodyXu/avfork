use std::ptr;

use crate::error;
use crate::aspawn;

pub use error::SyscallError;
use error::toResult;

pub struct Stack {
    stack_impl: aspawn::Stack_t
}
impl Stack {
    pub fn new() -> Stack {
        let mut stack = Stack {
            stack_impl: aspawn::Stack_t {
                addr: ptr::null_mut(),
                size: 0
            }
        };
        unsafe {
            aspawn::init_cached_stack(&mut stack.stack_impl);
        }
        stack
    }

    /// * `stack_sz` - the length of stack to reserve
    /// * `obj_on_stack_len` - the size of all objects you want to put on this stack
    pub fn reserve(&mut self,
                   stack_sz: usize, obj_on_stack_len: usize) -> Result<(), SyscallError> {
        unsafe {
            toResult(aspawn::reserve_stack(&mut self.stack_impl,
                                           stack_sz as u64,
                                           obj_on_stack_len as u64))?;
        }
        Ok(())
    }
}
impl Default for Stack {
    fn default() -> Stack {
        Stack::new()
    }
}
