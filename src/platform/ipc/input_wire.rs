use mach2::mach_time::mach_absolute_time;
use mach2::message::{
    mach_msg, mach_msg_header_t, mach_msg_return_t, MACH_MSG_SUCCESS, MACH_MSG_TIMEOUT_NONE,
    MACH_MSG_TYPE_COPY_SEND, MACH_SEND_MSG, MACH_SEND_TIMEOUT,
};
use mach2::port::{mach_port_t, MACH_PORT_NULL};

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEventKind {
    MouseMoved = 1,
    MouseDown = 2,
    MouseUp = 3,
    MouseDragged = 4,
    ScrollWheel = 5,
    KeyDown = 6,
    KeyUp = 7,
    FlagsChanged = 8,
}

pub const INPUT_EVENT_MSG_ID: u32 = 0x5100_4010;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputEvent {
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

#[repr(C)]
struct InputEventMsg {
    header: mach_msg_header_t,
    event: InputEvent,
}

fn msgh_bits(remote: u32, local: u32) -> u32 {
    (remote & 0xff) | ((local & 0xff) << 8)
}

fn base_event(kind: InputEventKind, client_token: u64, window_id: u64) -> InputEvent {
    InputEvent {
        kind: kind as u32,
        client_token,
        window_id,
        timestamp_mach: unsafe { mach_absolute_time() },
        x: 0.0,
        y: 0.0,
        delta_x: 0.0,
        delta_y: 0.0,
        button_number: 0,
        key_code: 0,
        modifier_flags: 0,
        click_count: 0,
        characters_utf16: [0; 16],
        characters_length: 0,
    }
}

fn send_input_event(reply_port: mach_port_t, event: &InputEvent) -> Result<(), &'static str> {
    if reply_port == MACH_PORT_NULL {
        return Err("reply port is null");
    }

    let mut msg = InputEventMsg {
        header: unsafe { std::mem::zeroed() },
        event: *event,
    };
    msg.header.msgh_bits = msgh_bits(MACH_MSG_TYPE_COPY_SEND, 0);
    msg.header.msgh_size = std::mem::size_of::<InputEventMsg>() as u32;
    msg.header.msgh_remote_port = reply_port;
    msg.header.msgh_local_port = MACH_PORT_NULL;
    msg.header.msgh_id = INPUT_EVENT_MSG_ID as i32;

    let kr: mach_msg_return_t = unsafe {
        mach_msg(
            &mut msg.header,
            MACH_SEND_MSG | MACH_SEND_TIMEOUT,
            std::mem::size_of::<InputEventMsg>() as u32,
            0,
            MACH_PORT_NULL,
            50,
            MACH_PORT_NULL,
        )
    };

    if kr == MACH_MSG_SUCCESS {
        Ok(())
    } else {
        Err("mach_msg send failed")
    }
}

pub fn send_mouse_moved(
    reply_port: mach_port_t,
    client_token: u64,
    window_id: u64,
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
) -> Result<(), &'static str> {
    let mut event = base_event(InputEventKind::MouseMoved, client_token, window_id);
    event.x = x;
    event.y = y;
    event.delta_x = delta_x;
    event.delta_y = delta_y;
    send_input_event(reply_port, &event)
}

pub fn send_mouse_button(
    reply_port: mach_port_t,
    client_token: u64,
    window_id: u64,
    is_down: bool,
    button_number: u32,
    x: f64,
    y: f64,
    click_count: u32,
    modifier_flags: u32,
) -> Result<(), &'static str> {
    let kind = if is_down { InputEventKind::MouseDown } else { InputEventKind::MouseUp };
    let mut event = base_event(kind, client_token, window_id);
    event.x = x;
    event.y = y;
    event.button_number = button_number;
    event.click_count = click_count;
    event.modifier_flags = modifier_flags;
    send_input_event(reply_port, &event)
}

pub fn send_mouse_dragged(
    reply_port: mach_port_t,
    client_token: u64,
    window_id: u64,
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
    button_number: u32,
) -> Result<(), &'static str> {
    let mut event = base_event(InputEventKind::MouseDragged, client_token, window_id);
    event.x = x;
    event.y = y;
    event.delta_x = delta_x;
    event.delta_y = delta_y;
    event.button_number = button_number;
    send_input_event(reply_port, &event)
}

pub fn send_scroll_wheel(
    reply_port: mach_port_t,
    client_token: u64,
    window_id: u64,
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
) -> Result<(), &'static str> {
    let mut event = base_event(InputEventKind::ScrollWheel, client_token, window_id);
    event.x = x;
    event.y = y;
    event.delta_x = delta_x;
    event.delta_y = delta_y;
    send_input_event(reply_port, &event)
}

pub fn send_key_event(
    reply_port: mach_port_t,
    client_token: u64,
    window_id: u64,
    is_down: bool,
    key_code: u32,
    modifier_flags: u32,
    utf16_chars: &[u16],
) -> Result<(), &'static str> {
    let kind = if is_down { InputEventKind::KeyDown } else { InputEventKind::KeyUp };
    let mut event = base_event(kind, client_token, window_id);
    event.key_code = key_code;
    event.modifier_flags = modifier_flags;

    let copy_len = utf16_chars.len().min(16);
    event.characters_utf16[..copy_len].copy_from_slice(&utf16_chars[..copy_len]);
    event.characters_length = copy_len as u32;

    send_input_event(reply_port, &event)
}

pub fn send_flags_changed(
    reply_port: mach_port_t,
    client_token: u64,
    window_id: u64,
    modifier_flags: u32,
) -> Result<(), &'static str> {
    let mut event = base_event(InputEventKind::FlagsChanged, client_token, window_id);
    event.modifier_flags = modifier_flags;
    send_input_event(reply_port, &event)
}

#[cfg(target_os = "macos")]
pub mod nsevent_translate {
    use super::*;
    use crate::frameworks::appkit::{
        sel, send0_f64, send0_i64, send0_id, send0_point, send0_u64, send_get_characters, Id, NSRange, Sel,
    };

    pub fn translate_nsevent(
        ns_event: Id,
        client_token: u64,
        window_id: u64,
        reply_port: mach_port_t,
    ) -> Result<(), &'static str> {
        if ns_event.is_null() || reply_port == MACH_PORT_NULL {
            return Err("invalid event or reply port");
        }

        unsafe {
            let event_type = send0_i64(ns_event, sel("type"));
            let mods = send0_u64(ns_event, sel("modifierFlags")) as u32;

            const NS_EVENT_TYPE_MOUSE_MOVED: i64 = 5;
            const NS_EVENT_TYPE_LEFT_MOUSE_DOWN: i64 = 1;
            const NS_EVENT_TYPE_LEFT_MOUSE_UP: i64 = 2;
            const NS_EVENT_TYPE_RIGHT_MOUSE_DOWN: i64 = 3;
            const NS_EVENT_TYPE_RIGHT_MOUSE_UP: i64 = 4;
            const NS_EVENT_TYPE_OTHER_MOUSE_DOWN: i64 = 25;
            const NS_EVENT_TYPE_OTHER_MOUSE_UP: i64 = 26;
            const NS_EVENT_TYPE_LEFT_MOUSE_DRAGGED: i64 = 6;
            const NS_EVENT_TYPE_RIGHT_MOUSE_DRAGGED: i64 = 7;
            const NS_EVENT_TYPE_OTHER_MOUSE_DRAGGED: i64 = 27;
            const NS_EVENT_TYPE_SCROLL_WHEEL: i64 = 22;
            const NS_EVENT_TYPE_KEY_DOWN: i64 = 10;
            const NS_EVENT_TYPE_KEY_UP: i64 = 11;
            const NS_EVENT_TYPE_FLAGS_CHANGED: i64 = 12;

            match event_type {
                NS_EVENT_TYPE_MOUSE_MOVED => {
                    let (x, y) = send0_point(ns_event, sel("locationInWindow"));
                    let dx = send0_f64(ns_event, sel("deltaX"));
                    let dy = send0_f64(ns_event, sel("deltaY"));
                    send_mouse_moved(reply_port, client_token, window_id, x, y, dx, dy)
                }
                NS_EVENT_TYPE_LEFT_MOUSE_DOWN | NS_EVENT_TYPE_RIGHT_MOUSE_DOWN | NS_EVENT_TYPE_OTHER_MOUSE_DOWN => {
                    let (x, y) = send0_point(ns_event, sel("locationInWindow"));
                    let button = send0_i64(ns_event, sel("buttonNumber")) as u32;
                    let clicks = send0_i64(ns_event, sel("clickCount")) as u32;
                    send_mouse_button(reply_port, client_token, window_id, true, button, x, y, clicks, mods)
                }
                NS_EVENT_TYPE_LEFT_MOUSE_UP | NS_EVENT_TYPE_RIGHT_MOUSE_UP | NS_EVENT_TYPE_OTHER_MOUSE_UP => {
                    let (x, y) = send0_point(ns_event, sel("locationInWindow"));
                    let button = send0_i64(ns_event, sel("buttonNumber")) as u32;
                    let clicks = send0_i64(ns_event, sel("clickCount")) as u32;
                    send_mouse_button(reply_port, client_token, window_id, false, button, x, y, clicks, mods)
                }
                NS_EVENT_TYPE_LEFT_MOUSE_DRAGGED | NS_EVENT_TYPE_RIGHT_MOUSE_DRAGGED | NS_EVENT_TYPE_OTHER_MOUSE_DRAGGED => {
                    let (x, y) = send0_point(ns_event, sel("locationInWindow"));
                    let dx = send0_f64(ns_event, sel("deltaX"));
                    let dy = send0_f64(ns_event, sel("deltaY"));
                    let button = send0_i64(ns_event, sel("buttonNumber")) as u32;
                    send_mouse_dragged(reply_port, client_token, window_id, x, y, dx, dy, button)
                }
                NS_EVENT_TYPE_SCROLL_WHEEL => {
                    let (x, y) = send0_point(ns_event, sel("locationInWindow"));
                    let dx = send0_f64(ns_event, sel("scrollingDeltaX"));
                    let dy = send0_f64(ns_event, sel("scrollingDeltaY"));
                    send_scroll_wheel(reply_port, client_token, window_id, x, y, dx, dy)
                }
                NS_EVENT_TYPE_KEY_DOWN | NS_EVENT_TYPE_KEY_UP => {
                    let key_code = send0_u64(ns_event, sel("keyCode")) as u32;
                    let chars_obj = send0_id(ns_event, sel("characters"));
                    let utf16 = ns_string_utf16(chars_obj);
                    send_key_event(
                        reply_port,
                        client_token,
                        window_id,
                        event_type == NS_EVENT_TYPE_KEY_DOWN,
                        key_code,
                        mods,
                        &utf16,
                    )
                }
                NS_EVENT_TYPE_FLAGS_CHANGED => send_flags_changed(reply_port, client_token, window_id, mods),
                _ => Err("unhandled NSEvent type"),
            }
        }
    }

    unsafe fn ns_string_utf16(obj: Id) -> Vec<u16> {
        if obj.is_null() {
            return Vec::new();
        }
        let len = send0_u64(obj, sel("length")) as usize;
        let capped = len.min(16);
        let mut buf = vec![0u16; capped];
        if capped > 0 {
            send_get_characters(
                obj,
                sel("getCharacters:range:"),
                buf.as_mut_ptr(),
                NSRange { location: 0, length: capped as u64 },
            );
        }
        buf
    }
}
