pub mod appkit;
pub mod metal_sys;
pub mod quartz_core;
pub mod opengl_sys;

pub fn appkit_objc_msg_send() -> *const std::ffi::c_void {
    appkit::raw_msg_send()
}
