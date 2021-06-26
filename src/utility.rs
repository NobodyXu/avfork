use std::os::raw::c_void;

pub fn to_void_ptr<T>(reference: &T) -> *const c_void {
    reference as *const _ as *const c_void
}

pub fn to_void_ptr_mut<T>(reference: &mut T) -> *mut c_void {
    reference as *mut _ as *mut c_void
}
