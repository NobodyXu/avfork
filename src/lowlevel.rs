use std::mem;
use std::marker::PhantomData;

use crate::error;
use crate::aspawn;

pub use error::SyscallError;
use error::toResult;

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
        Ok(StackObjectAllocator::new(self.stack_impl, reserved_obj_sz))
    }
}
pub struct StackObjectAllocator<'a> {
    stack_impl: aspawn::Stack_t,
    reserved_obj_sz: usize,
    alloc_obj_sz: usize,
    phantom: PhantomData<&'a Stack>,
}
impl<'a> StackObjectAllocator<'a> {
    fn new<'b>(stack_impl: aspawn::Stack_t, reserved_obj_sz: usize)
        -> StackObjectAllocator<'b>
    {
        StackObjectAllocator {
            stack_impl,
            reserved_obj_sz,
            alloc_obj_sz: 0,
            phantom: PhantomData,
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
