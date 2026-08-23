use std::sync::{Mutex, OnceLock};

const KEY_TABLE_SIZE: usize = 512;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MousePosition {
    pub x: f64,
    pub y: f64,
}

pub struct InputState {
    keys: [bool; KEY_TABLE_SIZE],
    mouse_pos: MousePosition,
}

impl InputState {
    fn new() -> Self {
        Self {
            keys: [false; KEY_TABLE_SIZE],
            mouse_pos: MousePosition::default(),
        }
    }

    fn instance() -> &'static Mutex<InputState> {
        static INSTANCE: OnceLock<Mutex<InputState>> = OnceLock::new();
        INSTANCE.get_or_init(|| Mutex::new(InputState::new()))
    }

    pub fn set_key_pressed(key: i32, pressed: bool) {
        if key >= 0 && (key as usize) < KEY_TABLE_SIZE {
            let mut state = Self::instance().lock().expect("input state mutex poisoned");
            state.keys[key as usize] = pressed;
        }
    }

    pub fn is_key_pressed(key: i32) -> bool {
        if key >= 0 && (key as usize) < KEY_TABLE_SIZE {
            let state = Self::instance().lock().expect("input state mutex poisoned");
            state.keys[key as usize]
        } else {
            false
        }
    }

    pub fn set_mouse_position(x: f64, y: f64) {
        let mut state = Self::instance().lock().expect("input state mutex poisoned");
        state.mouse_pos = MousePosition { x, y };
    }

    pub fn mouse_position() -> MousePosition {
        let state = Self::instance().lock().expect("input state mutex poisoned");
        state.mouse_pos
    }
}
