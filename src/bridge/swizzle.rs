use crate::frameworks::appkit::*;
use std::ffi::CString;
use std::sync::OnceLock;

#[derive(Debug)]
pub enum SwizzleError {
    ClassAllocationFailed(String),
    MethodAddFailed(String),
}

impl std::fmt::Display for SwizzleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwizzleError::ClassAllocationFailed(name) => write!(f, "objc_allocateClassPair failed for {name}"),
            SwizzleError::MethodAddFailed(sel) => write!(f, "class_addMethod failed for selector {sel}"),
        }
    }
}

impl std::error::Error for SwizzleError {}

pub trait Compositor: Send + Sync {
    fn window_shown(&self, window_id: usize, width: u32, height: u32, layer: Id);
    fn window_resized(&self, window_id: usize, width: u32, height: u32);
    fn window_closed(&self, window_id: usize);
}

static COMPOSITOR: OnceLock<Box<dyn Compositor>> = OnceLock::new();

pub fn install_compositor(compositor: Box<dyn Compositor>) {
    let _ = COMPOSITOR.set(compositor);
}

unsafe fn ensure_class(name: &str, superclass_name: &str) -> Result<(Class, bool), SwizzleError> {
    let existing = class(name);
    if !existing.is_null() {
        return Ok((existing, false));
    }
    let super_cls = class(superclass_name);
    let cname = CString::new(name).unwrap();
    let cls = objc_allocateClassPair(super_cls, cname.as_ptr(), 0);
    if cls.is_null() {
        return Err(SwizzleError::ClassAllocationFailed(name.to_string()));
    }
    Ok((cls, true))
}

unsafe fn add_or_replace(cls: Class, sel_name: &str, imp: Imp, types: &str, fresh: bool) -> Result<(), SwizzleError> {
    let s = sel(sel_name);
    let t = CString::new(types).unwrap();
    if fresh {
        if class_addMethod(cls, s, imp, t.as_ptr()) {
            Ok(())
        } else {
            Err(SwizzleError::MethodAddFailed(sel_name.to_string()))
        }
    } else {
        class_replaceMethod(cls, s, imp, t.as_ptr());
        Ok(())
    }
}

extern "C" fn shared_application_impl(_cls: Class, _sel: Sel) -> Id {
    static APP: OnceLock<std::sync::Mutex<Id>> = OnceLock::new();
    let cell = APP.get_or_init(|| std::sync::Mutex::new(std::ptr::null_mut()));
    let mut guard = cell.lock().expect("shared application mutex poisoned");
    if guard.is_null() {
        unsafe {
            let cls = class("NSApplication");
            let obj = send0_id(cls, sel("alloc"));
            *guard = send0_id(obj, sel("init"));
        }
    }
    *guard
}

extern "C" fn run_impl(_recv: Id, _sel_: Sel) {
    unsafe {
        CFRunLoopRun();
    }
}

extern "C" fn application_terminate(_recv: Id, _sel_: Sel, _sender: Id) {
    std::process::exit(0);
}

unsafe fn install_nsapplication() -> Result<(), SwizzleError> {
    let (cls, fresh) = ensure_class("NSApplication", "NSObject")?;
    let meta = object_getClass(cls);
    add_or_replace(meta, "sharedApplication", shared_application_impl as Imp, "@@:", fresh)?;
    add_or_replace(cls, "run", run_impl as Imp, "v@:", fresh)?;
    add_or_replace(cls, "terminate:", application_terminate as Imp, "v@:@", fresh)?;
    if fresh {
        objc_registerClassPair(cls);
    }
    Ok(())
}

extern "C" fn window_init(
    recv: Id,
    _sel_: Sel,
    frame: NSRect,
    _style_mask: u64,
    _backing: u64,
    _defer: i8,
) -> Id {
    with_window_state(recv, |ws| {
        ws.frame = frame;
    });
    recv
}

extern "C" fn window_set_title(recv: Id, _sel_: Sel, title: Id) {
    let s = ns_string_to_rust(title);
    with_window_state(recv, |ws| {
        ws.title = s;
    });
}

extern "C" fn window_set_delegate(recv: Id, _sel_: Sel, delegate: Id) {
    with_window_state(recv, |ws| {
        ws.delegate = delegate;
    });
}

extern "C" fn window_make_key_and_order_front(recv: Id, _sel_: Sel, _sender: Id) {
    let (layer, width, height, delegate, was_key) = with_window_state(recv, |ws| {
        let was_key = ws.is_key;
        ws.is_key = true;
        (ws.layer, ws.frame.size.width as u32, ws.frame.size.height as u32, ws.delegate, was_key)
    });

    if let Some(c) = COMPOSITOR.get() {
        c.window_shown(recv as usize, width, height, layer);
    }

    if !was_key {
        unsafe {
            if !delegate.is_null() {
                send1_void_id(delegate, sel("windowDidBecomeKey:"), recv);
            }
        }
    }
}

extern "C" fn window_frame(recv: Id, _sel_: Sel) -> NSRect {
    with_window_state(recv, |ws| ws.frame)
}

extern "C" fn window_set_frame_display(recv: Id, _sel_: Sel, frame: NSRect, _display: i8) {
    let (resized, delegate) = with_window_state(recv, |ws| {
        let changed = ws.frame.size != frame.size;
        ws.frame = frame;
        (changed, ws.delegate)
    });

    if resized {
        let width = frame.size.width as u32;
        let height = frame.size.height as u32;
        if let Some(c) = COMPOSITOR.get() {
            c.window_resized(recv as usize, width, height);
        }
        unsafe {
            if !delegate.is_null() {
                send1_void_id(delegate, sel("windowDidResize:"), recv);
            }
        }
    }
}

extern "C" fn window_order_out(recv: Id, _sel_: Sel, _sender: Id) {
    with_window_state(recv, |ws| {
        ws.is_key = false;
    });
}

extern "C" fn window_close(recv: Id, _sel_: Sel) {
    if let Some(c) = COMPOSITOR.get() {
        c.window_closed(recv as usize);
    }
    remove_window_state(recv);
}

extern "C" fn window_content_view(recv: Id, _sel_: Sel) -> Id {
    recv
}

extern "C" fn window_center(_recv: Id, _sel_: Sel) {}

extern "C" fn window_style_mask(_recv: Id, _sel_: Sel) -> u64 {
    0
}

unsafe fn install_nswindow() -> Result<(), SwizzleError> {
    let (cls, fresh) = ensure_class("NSWindow", "NSResponder")?;
    add_or_replace(
        cls,
        "initWithContentRect:styleMask:backing:defer:",
        window_init as Imp,
        "@@:{CGRect={CGPoint=dd}{CGSize=dd}}LLB",
        fresh,
    )?;
    add_or_replace(cls, "setTitle:", window_set_title as Imp, "v@:@", fresh)?;
    add_or_replace(cls, "setDelegate:", window_set_delegate as Imp, "v@:@", fresh)?;
    add_or_replace(
        cls,
        "makeKeyAndOrderFront:",
        window_make_key_and_order_front as Imp,
        "v@:@",
        fresh,
    )?;
    add_or_replace(
        cls,
        "frame",
        window_frame as Imp,
        "{CGRect={CGPoint=dd}{CGSize=dd}}@:",
        fresh,
    )?;
    add_or_replace(
        cls,
        "setFrame:display:",
        window_set_frame_display as Imp,
        "v@:{CGRect={CGPoint=dd}{CGSize=dd}}B",
        fresh,
    )?;
    add_or_replace(cls, "orderOut:", window_order_out as Imp, "v@:@", fresh)?;
    add_or_replace(cls, "close", window_close as Imp, "v@:", fresh)?;
    add_or_replace(cls, "contentView", window_content_view as Imp, "@@:", fresh)?;
    add_or_replace(cls, "center", window_center as Imp, "v@:", fresh)?;
    add_or_replace(cls, "styleMask", window_style_mask as Imp, "L@:", fresh)?;
    if fresh {
        objc_registerClassPair(cls);
    }
    Ok(())
}

extern "C" fn view_set_wants_layer(recv: Id, _sel_: Sel, wants: i8) {
    with_window_state(recv, |ws| {
        ws.wants_layer = wants != 0;
    });
}

extern "C" fn view_wants_layer(recv: Id, _sel_: Sel) -> i8 {
    with_window_state(recv, |ws| if ws.wants_layer { YES } else { NO })
}

extern "C" fn view_layer(recv: Id, _sel_: Sel) -> Id {
    with_window_state(recv, |ws| {
        if ws.layer.is_null() {
            unsafe {
                if let Ok((cls, fresh)) = ensure_class("CAMetalLayer", "NSObject") {
                    if fresh {
                        let _ = objc_registerClassPair(cls);
                    }
                    let obj = send0_id(cls, sel("alloc"));
                    ws.layer = send0_id(obj, sel("init"));
                }
            }
        }
        ws.layer
    })
}

extern "C" fn view_set_layer(recv: Id, _sel_: Sel, layer: Id) {
    with_window_state(recv, |ws| {
        ws.layer = layer;
    });
}

extern "C" fn view_frame(recv: Id, _sel_: Sel) -> NSRect {
    with_window_state(recv, |ws| ws.frame)
}

extern "C" fn view_bounds(recv: Id, _sel_: Sel) -> NSRect {
    with_window_state(recv, |ws| NSRect {
        origin: NSPoint::default(),
        size: ws.frame.size,
    })
}

extern "C" fn view_window(recv: Id, _sel_: Sel) -> Id {
    recv
}

unsafe fn install_nsview() -> Result<(), SwizzleError> {
    let (cls, fresh) = ensure_class("NSView", "NSResponder")?;
    add_or_replace(cls, "setWantsLayer:", view_set_wants_layer as Imp, "v@:B", fresh)?;
    add_or_replace(cls, "wantsLayer", view_wants_layer as Imp, "B@:", fresh)?;
    add_or_replace(cls, "layer", view_layer as Imp, "@@:", fresh)?;
    add_or_replace(cls, "setLayer:", view_set_layer as Imp, "v@:@", fresh)?;
    add_or_replace(
        cls,
        "frame",
        view_frame as Imp,
        "{CGRect={CGPoint=dd}{CGSize=dd}}@:",
        fresh,
    )?;
    add_or_replace(
        cls,
        "bounds",
        view_bounds as Imp,
        "{CGRect={CGPoint=dd}{CGSize=dd}}@:",
        fresh,
    )?;
    add_or_replace(cls, "window", view_window as Imp, "@@:", fresh)?;
    if fresh {
        objc_registerClassPair(cls);
    }
    Ok(())
}

extern "C" fn screens_func(_cls: Class, _sel_: Sel) -> Id {
    unsafe {
        let array = send0_id(send0_id(class("NSMutableArray"), sel("alloc")), sel("init"));
        let count = with_monitors(|m| m.len());
        for i in 0..count {
            let nsvalue = send1_id_ptr(class("NSValue"), sel("valueWithPointer:"), i as *mut std::ffi::c_void);
            send1_void_id(array, sel("addObject:"), nsvalue);
        }
        array
    }
}

extern "C" fn main_screen_func(cls: Class, sel_: Sel) -> Id {
    unsafe {
        let array = screens_func(cls, sel_);
        let count = send0_u64(array, sel("count"));
        if count == 0 {
            std::ptr::null_mut()
        } else {
            let f: unsafe extern "C" fn(Id, Sel, u64) -> Id = std::mem::transmute(objc_msgSend as *const ());
            f(array, sel("objectAtIndex:"), 0)
        }
    }
}

extern "C" fn screen_frame(recv: Id, _sel_: Sel) -> NSRect {
    unsafe {
        let idx_ptr = send0_id(recv, sel("pointerValue"));
        let idx = idx_ptr as usize;
        with_monitors(|monitors| {
            monitors.get(idx).copied().unwrap_or(NSRect {
                origin: NSPoint::default(),
                size: NSSize {
                    width: 1920.0,
                    height: 1080.0,
                },
            })
        })
    }
}

extern "C" fn screen_backing_scale_factor(_recv: Id, _sel_: Sel) -> f64 {
    1.0
}

unsafe fn install_nsscreen() -> Result<(), SwizzleError> {
    let (cls, fresh) = ensure_class("NSScreen", "NSObject")?;
    let meta = object_getClass(cls);
    add_or_replace(meta, "screens", screens_func as Imp, "@@:", fresh)?;
    add_or_replace(meta, "mainScreen", main_screen_func as Imp, "@@:", fresh)?;
    add_or_replace(
        cls,
        "frame",
        screen_frame as Imp,
        "{CGRect={CGPoint=dd}{CGSize=dd}}@:",
        fresh,
    )?;
    add_or_replace(
        cls,
        "backingScaleFactor",
        screen_backing_scale_factor as Imp,
        "d@:",
        fresh,
    )?;
    if fresh {
        objc_registerClassPair(cls);
    }
    Ok(())
}

pub fn install() -> Result<(), SwizzleError> {
    unsafe {
        install_nsapplication()?;
        install_nswindow()?;
        install_nsview()?;
        install_nsscreen()?;
    }
    Ok(())
}
