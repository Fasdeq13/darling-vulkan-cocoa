use mach2::message::{mach_msg_body_t, mach_msg_header_t, mach_msg_port_descriptor_t};

pub mod client;
pub mod host;
pub mod input_wire;

pub use client::{client_connect, client_disconnect, client_is_connected, client_send_surface};
pub use host::{ClientSink, CompositorHost, FrameSink, IncomingFrame};

pub const FRAME_SUBMIT_MSG_ID: u32 = 0x51004001;
pub const HELLO_MSG_ID: u32 = 0x51004002;
pub const GOODBYE_MSG_ID: u32 = 0x51004003;

#[repr(C)]
pub(super) struct FrameSubmitMsg {
    pub(super) header: mach_msg_header_t,
    pub(super) body: mach_msg_body_t,
    pub(super) surface_port: mach_msg_port_descriptor_t,
    pub(super) client_token: u64,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) frame_index: u32,
}

#[repr(C)]
pub(super) struct HelloMsg {
    pub(super) header: mach_msg_header_t,
    pub(super) body: mach_msg_body_t,
    pub(super) reply_port: mach_msg_port_descriptor_t,
    pub(super) client_token: u64,
    pub(super) pid: i32,
}

#[repr(C)]
pub(super) struct GoodbyeMsg {
    pub(super) header: mach_msg_header_t,
    pub(super) client_token: u64,
}

pub(super) fn msgh_bits(remote: u32, local: u32) -> u32 {
    (remote & 0xff) | ((local & 0xff) << 8)
}
