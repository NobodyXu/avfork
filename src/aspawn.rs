#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ptr;

include!(concat!(env!("OUT_DIR"), "/aspawn_binding.rs"));

pub fn new_stack_t() -> Stack_t {
    let mut stack_impl = Stack_t {
        addr: ptr::null_mut(),
        size: 0
    };
    unsafe {
        init_cached_stack(&mut stack_impl);
    }
    stack_impl
}
