use super::{msgh_bits, FrameSubmitMsg, GoodbyeMsg, HelloMsg, FRAME_SUBMIT_MSG_ID, GOODBYE_MSG_ID, HELLO_MSG_ID};
use mach2::kern_return::KERN_SUCCESS;
use mach2::message::{
    mach_msg, mach_msg_return_t, MACH_MSGH_BITS_COMPLEX, MACH_MSG_PORT_DESCRIPTOR, MACH_MSG_SUCCESS,
    MACH_MSG_TIMEOUT_NONE, MACH_MSG_TYPE_COPY_SEND, MACH_MSG_TYPE_MAKE_SEND, MACH_MSG_TYPE_MOVE_SEND,
    MACH_SEND_MSG,
};
use mach2::port::{
    mach_port_allocate, mach_port_deallocate, mach_port_insert_right, mach_port_mod_refs, mach_port_t,
    MACH_PORT_NULL, MACH_PORT_RIGHT_RECEIVE,
};
use mach2::traps::mach_task_self;
use mach2::bootstrap::{bootstrap_look_up, bootstrap_port};
use std::ffi::CString;
use std::sync::{Mutex, OnceLock};

struct ClientConnection {
    server_port: mach_port_t,
    local_reply_port: mach_port_t,
    token: u64,
    connected: bool,
}

fn client_connection() -> &'static Mutex<ClientConnection> {
    static CONN: OnceLock<Mutex<ClientConnection>> = OnceLock::new();
    CONN.get_or_init(|| {
        Mutex::new(ClientConnection {
            server_port: MACH_PORT_NULL,
            local_reply_port: MACH_PORT_NULL,
            token: 0,
            connected: false,
        })
    })
}

pub fn client_connect(service_name: &str, token: u64) -> Result<(), &'static str> {
    let mut conn = client_connection().lock().expect("client connection mutex poisoned");
    if conn.connected {
        return Err("already connected");
    }

    let c_name = CString::new(service_name).map_err(|_| "service name had a NUL byte")?;
    let mut server_port: mach_port_t = MACH_PORT_NULL;
    let kr = unsafe { bootstrap_look_up(bootstrap_port, c_name.as_ptr(), &mut server_port) };
    if kr != KERN_SUCCESS {
        return Err("bootstrap_look_up failed");
    }

    let mut reply_port: mach_port_t = MACH_PORT_NULL;
    let kr = unsafe { mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &mut reply_port) };
    if kr != KERN_SUCCESS {
        unsafe { mach_port_deallocate(mach_task_self(), server_port) };
        return Err("mach_port_allocate failed");
    }

    let kr = unsafe {
        mach_port_insert_right(mach_task_self(), reply_port, reply_port, MACH_MSG_TYPE_MAKE_SEND)
    };
    if kr != KERN_SUCCESS {
        unsafe {
            mach_port_deallocate(mach_task_self(), reply_port);
            mach_port_deallocate(mach_task_self(), server_port);
        }
        return Err("mach_port_insert_right failed");
    }

    let mut msg: HelloMsg = unsafe { std::mem::zeroed() };
    msg.header.msgh_bits = MACH_MSGH_BITS_COMPLEX | msgh_bits(MACH_MSG_TYPE_COPY_SEND, 0);
    msg.header.msgh_size = std::mem::size_of::<HelloMsg>() as u32;
    msg.header.msgh_remote_port = server_port;
    msg.header.msgh_local_port = MACH_PORT_NULL;
    msg.header.msgh_id = HELLO_MSG_ID as i32;
    msg.body.msgh_descriptor_count = 1;
    msg.reply_port.name = reply_port;
    msg.reply_port.disposition = MACH_MSG_TYPE_MAKE_SEND as u8;
    msg.reply_port.type_ = MACH_MSG_PORT_DESCRIPTOR as u8;
    msg.client_token = token;
    msg.pid = std::process::id() as i32;

    let kr: mach_msg_return_t = unsafe {
        mach_msg(
            &mut msg.header,
            MACH_SEND_MSG,
            std::mem::size_of::<HelloMsg>() as u32,
            0,
            MACH_PORT_NULL,
            MACH_MSG_TIMEOUT_NONE,
            MACH_PORT_NULL,
        )
    };
    if kr != MACH_MSG_SUCCESS {
        unsafe {
            mach_port_deallocate(mach_task_self(), reply_port);
            mach_port_deallocate(mach_task_self(), server_port);
        }
        return Err("mach_msg send failed");
    }

    conn.server_port = server_port;
    conn.local_reply_port = reply_port;
    conn.token = token;
    conn.connected = true;
    Ok(())
}

pub fn client_send_surface(
    token: u64,
    surface_port: mach_port_t,
    width: u32,
    height: u32,
    index: u32,
) -> Result<(), &'static str> {
    if surface_port == MACH_PORT_NULL {
        return Err("surface port is null");
    }

    let conn = client_connection().lock().expect("client connection mutex poisoned");
    if !conn.connected || conn.token != token {
        return Err("not connected");
    }
    let server_port = conn.server_port;
    drop(conn);

    let mut msg: FrameSubmitMsg = unsafe { std::mem::zeroed() };
    msg.header.msgh_bits = MACH_MSGH_BITS_COMPLEX | msgh_bits(MACH_MSG_TYPE_COPY_SEND, 0);
    msg.header.msgh_size = std::mem::size_of::<FrameSubmitMsg>() as u32;
    msg.header.msgh_remote_port = server_port;
    msg.header.msgh_local_port = MACH_PORT_NULL;
    msg.header.msgh_id = FRAME_SUBMIT_MSG_ID as i32;
    msg.body.msgh_descriptor_count = 1;
    msg.surface_port.name = surface_port;
    msg.surface_port.disposition = MACH_MSG_TYPE_MOVE_SEND as u8;
    msg.surface_port.type_ = MACH_MSG_PORT_DESCRIPTOR as u8;
    msg.client_token = token;
    msg.width = width;
    msg.height = height;
    msg.frame_index = index;

    let kr: mach_msg_return_t = unsafe {
        mach_msg(
            &mut msg.header,
            MACH_SEND_MSG,
            std::mem::size_of::<FrameSubmitMsg>() as u32,
            0,
            MACH_PORT_NULL,
            MACH_MSG_TIMEOUT_NONE,
            MACH_PORT_NULL,
        )
    };
    if kr != MACH_MSG_SUCCESS {
        unsafe { mach_port_deallocate(mach_task_self(), surface_port) };
        return Err("mach_msg send failed");
    }
    Ok(())
}

pub fn client_disconnect(token: u64) -> Result<(), &'static str> {
    let mut conn = client_connection().lock().expect("client connection mutex poisoned");
    if !conn.connected || conn.token != token {
        return Err("not connected");
    }

    let mut msg: GoodbyeMsg = unsafe { std::mem::zeroed() };
    msg.header.msgh_bits = msgh_bits(MACH_MSG_TYPE_COPY_SEND, 0);
    msg.header.msgh_size = std::mem::size_of::<GoodbyeMsg>() as u32;
    msg.header.msgh_remote_port = conn.server_port;
    msg.header.msgh_local_port = MACH_PORT_NULL;
    msg.header.msgh_id = GOODBYE_MSG_ID as i32;
    msg.client_token = token;

    unsafe {
        mach_msg(
            &mut msg.header,
            MACH_SEND_MSG,
            std::mem::size_of::<GoodbyeMsg>() as u32,
            0,
            MACH_PORT_NULL,
            MACH_MSG_TIMEOUT_NONE,
            MACH_PORT_NULL,
        );
        mach_port_deallocate(mach_task_self(), conn.server_port);
        mach_port_mod_refs(mach_task_self(), conn.local_reply_port, MACH_PORT_RIGHT_RECEIVE, -1);
    }

    conn.server_port = MACH_PORT_NULL;
    conn.local_reply_port = MACH_PORT_NULL;
    conn.connected = false;
    Ok(())
}

pub fn client_is_connected(token: u64) -> bool {
    let conn = client_connection().lock().expect("client connection mutex poisoned");
    conn.connected && conn.token == token
}
