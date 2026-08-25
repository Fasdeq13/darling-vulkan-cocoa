use std::collections::HashMap;
use std::ffi::{c_char, c_void, CString};
use std::sync::{Mutex, OnceLock};

pub type Id = *mut c_void;
pub type Class = *mut c_void;
pub type Sel = *mut c_void;
pub type Imp = *const c_void;

pub const NO: i8 = 0;
pub const YES: i8 = 1;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NSPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NSSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NSRect {
    pub origin: NSPoint,
    pub size: NSSize,
}

extern "C" {
    pub fn objc_allocateClassPair(superclass: Class, name: *const c_char, extra_bytes: usize) -> Class;
    pub fn objc_registerClassPair(cls: Class);
    pub fn objc_getClass(name: *const c_char) -> Class;
    pub fn object_getClass(obj: Id) -> Class;
    pub fn class_addMethod(cls: Class, sel: Sel, imp: Imp, types: *const c_char) -> bool;
    pub fn class_replaceMethod(cls: Class, sel: Sel, imp: Imp, types: *const c_char) -> Imp;
    pub fn sel_registerName(name: *const c_char) -> Sel;
    pub fn objc_msgSend();
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFRunLoopRun();
}

pub fn sel(name: &str) -> Sel {
    let c = CString::new(name).expect("selector name had a NUL byte");
    unsafe { sel_registerName(c.as_ptr()) }
}

pub fn class(name: &str) -> Class {
    let c = CString::new(name).expect("class name had a NUL byte");
    unsafe { objc_getClass(c.as_ptr()) }
}

pub unsafe fn send0_id(recv: Id, s: Sel) -> Id {
    let f: unsafe extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(recv, s)
}

pub unsafe fn send0_void(recv: Id, s: Sel) {
    let f: unsafe extern "C" fn(Id, Sel) = std::mem::transmute(objc_msgSend as *const ());
    f(recv, s)
}

pub unsafe fn send0_bool(recv: Id, s: Sel) -> bool {
    let f: unsafe extern "C" fn(Id, Sel) -> i8 = std::mem::transmute(objc_msgSend as *const ());
    f(recv, s) != 0
}

pub unsafe fn send0_u64(recv: Id, s: Sel) -> u64 {
    let f: unsafe extern "C" fn(Id, Sel) -> u64 = std::mem::transmute(objc_msgSend as *const ());
    f(recv, s)
}

pub unsafe fn send0_rect(recv: Id, s: Sel) -> NSRect {
    let f: unsafe extern "C" fn(Id, Sel) -> NSRect = std::mem::transmute(objc_msgSend as *const ());
    f(recv, s)
}

pub unsafe fn send1_id_id(recv: Id, s: Sel, a: Id) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, Id) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(recv, s, a)
}

pub unsafe fn send1_void_id(recv: Id, s: Sel, a: Id) {
    let f: unsafe extern "C" fn(Id, Sel, Id) = std::mem::transmute(objc_msgSend as *const ());
    f(recv, s, a)
}

pub unsafe fn send1_id_ptr(recv: Id, s: Sel, a: *mut c_void) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, *mut c_void) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(recv, s, a)
}

pub unsafe fn send_str(recv: Id, s: Sel, text: &str) -> Id {
    let nsstr = ns_string(text);
    send1_id_id(recv, s, nsstr)
}

pub fn ns_string(text: &str) -> Id {
    let c = CString::new(text).unwrap_or_else(|_| CString::new("").unwrap());
    unsafe {
        let cls = class("NSString");
        let obj = send0_id(cls, sel("alloc"));
        let f: unsafe extern "C" fn(Id, Sel, *const c_char) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        f(obj, sel("initWithUTF8String:"), c.as_ptr())
    }
}

pub fn ns_string_to_rust(obj: Id) -> String {
    if obj.is_null() {
        return String::new();
    }
    unsafe {
        let f: unsafe extern "C" fn(Id, Sel) -> *const c_char = std::mem::transmute(objc_msgSend as *const ());
        let ptr = f(obj, sel("UTF8String"));
        if ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

#[derive(Debug, Default)]
pub struct WindowState {
    pub frame: NSRect,
    pub title: String,
    pub is_key: bool,
    pub wants_layer: bool,
    pub layer: Id,
    pub delegate: Id,
}

unsafe impl Send for WindowState {}

fn window_states() -> &'static Mutex<HashMap<usize, WindowState>> {
    static STATES: OnceLock<Mutex<HashMap<usize, WindowState>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn with_window_state<R>(obj: Id, f: impl FnOnce(&mut WindowState) -> R) -> R {
    let key = obj as usize;
    let mut map = window_states().lock().expect("window state mutex poisoned");
    let state = map.entry(key).or_default();
    f(state)
}

pub fn remove_window_state(obj: Id) {
    let key = obj as usize;
    let mut map = window_states().lock().expect("window state mutex poisoned");
    map.remove(&key);
}

fn monitor_table() -> &'static Mutex<Vec<NSRect>> {
    static MONITORS: OnceLock<Mutex<Vec<NSRect>>> = OnceLock::new();
    MONITORS.get_or_init(|| {
        Mutex::new(vec![NSRect {
            origin: NSPoint::default(),
            size: NSSize {
                width: 1920.0,
                height: 1080.0,
            },
        }])
    })
}

pub fn set_monitors(monitors: Vec<NSRect>) {
    let mut table = monitor_table().lock().expect("monitor table mutex poisoned");
    *table = monitors;
}

pub fn with_monitors<R>(f: impl FnOnce(&[NSRect]) -> R) -> R {
    let table = monitor_table().lock().expect("monitor table mutex poisoned");
    f(&table)
}
