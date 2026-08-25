use std::os::raw::{c_char, c_void};
use std::ffi::CString;

#[repr(C)]
pub struct QmvIncomingFrame {
    pub client_token: u64,
    pub surface_port: u32,
    pub width: u32,
    pub height: u32,
    pub frame_index: u32,
    pub received_at_mach_time: u64,
}

#[repr(C)]
pub struct QmvResolvedSurface {
    pub surface_ref: *mut c_void,
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
    pub pixel_format: u32,
}

#[repr(C)]
pub struct QmvInputEvent {
    pub kind: u32,
    pub client_token: u64,
    pub window_id: u64,
    pub timestamp_mach: u64,
    pub x: f64,
    pub y: f64,
    pub delta_x: f64,
    pub delta_y: f64,
    pub button_number: u32,
    pub key_code: u32,
    pub modifier_flags: u32,
    pub click_count: u32,
    pub characters_utf16: [u16; 16],
    pub characters_length: u32,
}

#[link(name = "native_core", kind = "static")]
extern "C" {
    fn qmv_compositor_host_create(service_name: *const c_char) -> *mut c_void;
    fn qmv_compositor_host_set_frame_callback(
        host: *mut c_void,
        cb: extern "C" fn(*const QmvIncomingFrame, *mut c_void),
        user_data: *mut c_void,
    );
    fn qmv_compositor_host_set_client_callback(
        host: *mut c_void,
        cb: extern "C" fn(u64, i32, i32, *mut c_void),
        user_data: *mut c_void,
    );
    fn qmv_compositor_host_start(host: *mut c_void) -> i32;
    fn qmv_compositor_host_destroy(host: *mut c_void);
    
    fn qmv_resolve_surface_from_frame(frame: *const QmvIncomingFrame, out: *mut QmvResolvedSurface) -> i32;
    fn qmv_resolved_surface_release(resolved: *mut QmvResolvedSurface);
    fn qmv_resolved_surface_is_in_use(resolved: *const QmvResolvedSurface) -> i32;

    fn qmv_signals_send_mouse_moved(reply_port: u32, token: u64, win_id: u64, x: f64, y: f64, dx: f64, dy: f64) -> i32;
    fn qmv_signals_send_key_event(reply_port: u32, token: u64, win_id: u64, is_down: i32, code: u32, mods: u32, chars: *const u16, len: u32) -> i32;

    fn qmv_shm_sync_create(name: *const c_char, slot_count: u32) -> *mut c_void;
    fn qmv_shm_sync_acquire_read_slot(sync: *mut c_void, out_idx: *mut u32) -> i32;
    fn qmv_shm_sync_release_read_slot(sync: *mut c_void, idx: u32) -> i32;
    fn qmv_shm_sync_destroy(sync: *mut c_void);
}

extern "C" fn on_frame_received(frame_ptr: *const QmvIncomingFrame, _user_data: *mut c_void) {
    if frame_ptr.is_null() { return; }
    
    unsafe {
        let frame = &*frame_ptr;
        let mut resolved = std::mem::zeroed::<QmvResolvedSurface>();
        
        if qmv_resolve_surface_from_frame(frame_ptr, &mut resolved) == 0 {
            println!(
                "[QMV Compositor] Frame #{} verified. Token: {}. Resolution: {}x{}. Format: 0x{:X}", 
                frame.frame_index, frame.client_token, resolved.width, resolved.height, resolved.pixel_format
            );
            
            qmv_resolved_surface_release(&mut resolved);
        }
    }
}

extern "C" fn on_client_changed(token: u64, pid: i32, connected: i32, _user_data: *mut c_void) {
    let status = if connected == 1 { "Connected" } else { "Disconnected" };
    println!("[QMV Compositor] Client process tracking status change -> PID: {}, Token: {}, Status: {}", pid, token, status);
}

pub struct QmvServer {
    raw_host: *mut c_void,
    raw_sync: *mut c_void,
}

impl QmvServer {
    pub fn start(service_name: &str, shm_name: &str, slots: u32) -> Self {
        let c_service = CString::new(service_name).unwrap();
        let c_shm = CString::new(shm_name).unwrap();
        
        unsafe {
            let host = qmv_compositor_host_create(c_service.as_ptr());
            assert!(!host.is_null(), "Failed to initialize Mach IPC host");
            
            let sync = qmv_shm_sync_create(c_shm.as_ptr(), slots);
            assert!(!sync.is_null(), "Failed to initialize SHM barrier");

            qmv_compositor_host_set_frame_callback(host, on_frame_received, std::ptr::null_mut());
            qmv_compositor_host_set_client_callback(host, on_client_changed, std::ptr::null_mut());
            qmv_compositor_host_start(host);
            
            println!("[QMV Server] Core graphics pipeline and synchronization barriers deployment complete");
            QmvServer { raw_host: host, raw_sync: sync }
        }
    }
}

impl Drop for QmvServer {
    fn drop(&mut self) {
        unsafe {
            qmv_compositor_host_destroy(self.raw_host);
            qmv_shm_sync_destroy(self.raw_sync);
        }
    }
}
