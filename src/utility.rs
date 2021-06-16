use std::os::raw::c_void;

pub fn to_void_ptr<T>(reference: &T) -> *mut c_void {
    reference as *const _ as *mut c_void
}
