mod syscall;
mod aspawn;

use std::ptr;

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
            aspawn::reserve_stack(&mut stack.stack_impl, 0, 0);
        }
        stack
    }

    pub fn reserve(&mut self,
                   reserved_stack_sz: usize, obj_to_place_on_stack_len: usize) -> i32 {
        let ret;
        unsafe {
            ret = aspawn::reserve_stack(&mut self.stack_impl,
                                        reserved_stack_sz as u64,
                                        obj_to_place_on_stack_len as u64);
        }
        ret
    }
}
impl Default for Stack {
    fn default() -> Stack {
        Stack::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
