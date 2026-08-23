use std::os::raw::c_void;

pub type CgDirectDisplayId = u32;

#[repr(C)]
struct CgError(i32);
const CG_ERROR_SUCCESS: i32 = 0;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGGetActiveDisplayList(max_displays: u32, active_displays: *mut CgDirectDisplayId, display_count: *mut u32) -> i32;
    fn CGMainDisplayID() -> CgDirectDisplayId;
    fn CGDisplayPixelsWide(display: CgDirectDisplayId) -> usize;
    fn CGDisplayPixelsHigh(display: CgDirectDisplayId) -> usize;
    fn CGDisplayIsMain(display: CgDirectDisplayId) -> u32;
    fn CGDisplayIsBuiltin(display: CgDirectDisplayId) -> u32;
    fn CGDisplayCopyDisplayMode(display: CgDirectDisplayId) -> *mut c_void;
    fn CGDisplayModeGetRefreshRate(mode: *mut c_void) -> f64;
    fn CGDisplayModeGetPixelWidth(mode: *mut c_void) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: *mut c_void) -> usize;
    fn CGDisplayModeRelease(mode: *mut c_void);
    fn CGDisplayCopyAllDisplayModes(display: CgDirectDisplayId, options: *const c_void) -> *mut c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFArrayGetCount(array: *mut c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *mut c_void, index: isize) -> *const c_void;
    fn CFRelease(obj: *mut c_void);
}

#[derive(Debug, Clone, Copy)]
pub struct DisplayInfo {
    pub display_id: CgDirectDisplayId,
    pub width: usize,
    pub height: usize,
    pub refresh_rate_hz: f64,
    pub is_main: bool,
    pub is_builtin: bool,
    pub pixel_width: usize,
    pub pixel_height: usize,
}

fn build_info(display_id: CgDirectDisplayId) -> DisplayInfo {
    let width = unsafe { CGDisplayPixelsWide(display_id) };
    let height = unsafe { CGDisplayPixelsHigh(display_id) };
    let is_main = unsafe { CGDisplayIsMain(display_id) } != 0;
    let is_builtin = unsafe { CGDisplayIsBuiltin(display_id) } != 0;

    let mode = unsafe { CGDisplayCopyDisplayMode(display_id) };
    if mode.is_null() {
        return DisplayInfo {
            display_id,
            width,
            height,
            refresh_rate_hz: 60.0,
            is_main,
            is_builtin,
            pixel_width: width,
            pixel_height: height,
        };
    }

    let rate = unsafe { CGDisplayModeGetRefreshRate(mode) };
    let refresh_rate_hz = if rate > 0.0 { rate } else { 60.0 };
    let pixel_width = unsafe { CGDisplayModeGetPixelWidth(mode) };
    let pixel_height = unsafe { CGDisplayModeGetPixelHeight(mode) };
    unsafe { CGDisplayModeRelease(mode) };

    DisplayInfo {
        display_id,
        width,
        height,
        refresh_rate_hz,
        is_main,
        is_builtin,
        pixel_width,
        pixel_height,
    }
}

pub fn enumerate_all() -> Vec<DisplayInfo> {
    let mut display_count: u32 = 0;
    let rc = unsafe { CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut display_count) };
    if rc != CG_ERROR_SUCCESS || display_count == 0 {
        return Vec::new();
    }

    let mut ids = vec![0 as CgDirectDisplayId; display_count as usize];
    let rc = unsafe { CGGetActiveDisplayList(display_count, ids.as_mut_ptr(), &mut display_count) };
    if rc != CG_ERROR_SUCCESS {
        return Vec::new();
    }
    ids.truncate(display_count as usize);

    ids.into_iter().map(build_info).collect()
}

pub fn main_display() -> DisplayInfo {
    let main_id = unsafe { CGMainDisplayID() };
    build_info(main_id)
}

pub fn list_available_refresh_rates(display_id: CgDirectDisplayId) -> Vec<f64> {
    let modes = unsafe { CGDisplayCopyAllDisplayModes(display_id, std::ptr::null()) };
    if modes.is_null() {
        return Vec::new();
    }

    let count = unsafe { CFArrayGetCount(modes) };
    let mut rates: Vec<f64> = Vec::with_capacity(count as usize);
    for i in 0..count {
        let mode_ptr = unsafe { CFArrayGetValueAtIndex(modes, i) } as *mut c_void;
        if mode_ptr.is_null() {
            continue;
        }
        let rate = unsafe { CGDisplayModeGetRefreshRate(mode_ptr) };
        if rate > 0.0 {
            rates.push(rate);
        }
    }
    unsafe { CFRelease(modes) };

    rates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    rates.dedup();
    rates
}
