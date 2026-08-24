use super::{FrameSubmitMsg, GoodbyeMsg, HelloMsg, FRAME_SUBMIT_MSG_ID, GOODBYE_MSG_ID, HELLO_MSG_ID};
use mach2::bootstrap::{bootstrap_check_in, bootstrap_port, bootstrap_register};
use mach2::kern_return::KERN_SUCCESS;
use mach2::mach_time::mach_absolute_time;
use mach2::message::{
    mach_msg, mach_msg_header_t, mach_msg_return_t, MACH_MSG_SUCCESS, MACH_MSG_TYPE_MAKE_SEND, MACH_RCV_MSG,
    MACH_RCV_TIMEOUT,
};
use mach2::mach_port::{mach_port_allocate, mach_port_deallocate, mach_port_insert_right, mach_port_mod_refs};
use mach2::port::{mach_port_t, MACH_PORT_NULL, MACH_PORT_RIGHT_RECEIVE};
use mach2::traps::mach_task_self;
use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::Mutex;

const HOST_MAX_QUEUE: usize = 256;
const HOST_MAX_CLIENTS: usize = 64;
const HOST_MSG_BUF_SIZE: usize = 2048;

#[derive(Debug, Clone, Copy)]
pub struct IncomingFrame {
    pub client_token: u64,
    pub surface_port: mach_port_t,
    pub width: u32,
    pub height: u32,
    pub frame_index: u32,
    pub received_at_mach_time: u64,
}

#[derive(Debug, Clone, Copy)]
struct ClientRecord {
    client_token: u64,
    pid: i32,
    reply_port: mach_port_t,
    active: bool,
}

impl Default for ClientRecord {
    fn default() -> Self {
        Self {
            client_token: 0,
            pid: -1,
            reply_port: MACH_PORT_NULL,
            active: false,
        }
    }
}

pub trait FrameSink: Send + Sync {
    fn on_frame(&self, frame: &IncomingFrame);
}

pub trait ClientSink: Send + Sync {
    fn on_client_changed(&self, client_token: u64, pid: i32, connected: bool);
}

struct HostInner {
    service_port: mach_port_t,
    queue: VecDeque<IncomingFrame>,
    clients: [ClientRecord; HOST_MAX_CLIENTS],
}

pub struct CompositorHost {
    inner: Mutex<HostInner>,
    frame_sink: Option<Box<dyn FrameSink>>,
    client_sink: Option<Box<dyn ClientSink>>,
}

impl CompositorHost {
    pub fn create(
        service_name: &str,
        frame_sink: Option<Box<dyn FrameSink>>,
        client_sink: Option<Box<dyn ClientSink>>,
    ) -> Result<Self, &'static str> {
        let c_name = CString::new(service_name).map_err(|_| "service name had a NUL byte")?;

        let mut service_port: mach_port_t = MACH_PORT_NULL;
        let kr = unsafe { bootstrap_check_in(bootstrap_port, c_name.as_ptr(), &mut service_port) };
        if kr != KERN_SUCCESS {
            let kr = unsafe { mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &mut service_port) };
            if kr != KERN_SUCCESS {
                return Err("mach_port_allocate failed");
            }
            unsafe {
                mach_port_insert_right(mach_task_self(), service_port, service_port, MACH_MSG_TYPE_MAKE_SEND);
                bootstrap_register(bootstrap_port, c_name.as_ptr(), service_port);
            }
        }

        Ok(Self {
            inner: Mutex::new(HostInner {
                service_port,
                queue: VecDeque::with_capacity(HOST_MAX_QUEUE),
                clients: [ClientRecord::default(); HOST_MAX_CLIENTS],
            }),
            frame_sink,
            client_sink,
        })
    }

    fn find_client_index(clients: &[ClientRecord; HOST_MAX_CLIENTS], token: u64) -> Option<usize> {
        clients.iter().position(|c| c.active && c.client_token == token)
    }

    fn alloc_client_index(clients: &[ClientRecord; HOST_MAX_CLIENTS]) -> Option<usize> {
        clients.iter().position(|c| !c.active)
    }

    fn handle_hello(&self, msg: &HelloMsg) {
        let (should_notify, pid) = {
            let mut inner = self.inner.lock().expect("host mutex poisoned");
            if let Some(idx) = Self::find_client_index(&inner.clients, msg.client_token) {
                if inner.clients[idx].reply_port != MACH_PORT_NULL {
                    unsafe { mach_port_deallocate(mach_task_self(), inner.clients[idx].reply_port) };
                }
                inner.clients[idx].reply_port = msg.reply_port.name;
                inner.clients[idx].pid = msg.pid;
                (false, msg.pid)
            } else if let Some(idx) = Self::alloc_client_index(&inner.clients) {
                inner.clients[idx] = ClientRecord {
                    client_token: msg.client_token,
                    pid: msg.pid,
                    reply_port: msg.reply_port.name,
                    active: true,
                };
                (true, msg.pid)
            } else {
                if msg.reply_port.name != MACH_PORT_NULL {
                    unsafe { mach_port_deallocate(mach_task_self(), msg.reply_port.name) };
                }
                (false, msg.pid)
            }
        };

        if should_notify {
            if let Some(sink) = &self.client_sink {
                sink.on_client_changed(msg.client_token, pid, true);
            }
        }
    }

    fn handle_goodbye(&self, client_token: u64) {
        let pid = {
            let mut inner = self.inner.lock().expect("host mutex poisoned");
            match Self::find_client_index(&inner.clients, client_token) {
                Some(idx) => {
                    let pid = inner.clients[idx].pid;
                    if inner.clients[idx].reply_port != MACH_PORT_NULL {
                        unsafe { mach_port_deallocate(mach_task_self(), inner.clients[idx].reply_port) };
                    }
                    inner.clients[idx].active = false;
                    inner.clients[idx].reply_port = MACH_PORT_NULL;
                    Some(pid)
                }
                None => None,
            }
        };

        if let Some(pid) = pid {
            if let Some(sink) = &self.client_sink {
                sink.on_client_changed(client_token, pid, false);
            }
        }
    }

    fn handle_frame_submit(&self, msg: &FrameSubmitMsg) {
        let known = {
            let inner = self.inner.lock().expect("host mutex poisoned");
            Self::find_client_index(&inner.clients, msg.client_token).is_some()
        };

        if !known {
            if msg.surface_port.name != MACH_PORT_NULL {
                unsafe { mach_port_deallocate(mach_task_self(), msg.surface_port.name) };
            }
            return;
        }

        let frame = IncomingFrame {
            client_token: msg.client_token,
            surface_port: msg.surface_port.name,
            width: msg.width,
            height: msg.height,
            frame_index: msg.frame_index,
            received_at_mach_time: unsafe { mach_absolute_time() },
        };

        {
            let mut inner = self.inner.lock().expect("host mutex poisoned");
            if inner.queue.len() >= HOST_MAX_QUEUE {
                if let Some(dropped) = inner.queue.pop_front() {
                    if dropped.surface_port != MACH_PORT_NULL {
                        unsafe { mach_port_deallocate(mach_task_self(), dropped.surface_port) };
                    }
                }
            }
            inner.queue.push_back(frame);
        }

        if let Some(sink) = &self.frame_sink {
            sink.on_frame(&frame);
        }
    }

    pub fn run_once(&self, timeout_ms: i32) -> Result<bool, &'static str> {
        let service_port = self.inner.lock().expect("host mutex poisoned").service_port;

        let mut buf = [0u8; HOST_MSG_BUF_SIZE];
        let header = buf.as_mut_ptr() as *mut mach_msg_header_t;

        let kr: mach_msg_return_t = unsafe {
            mach_msg(
                header,
                MACH_RCV_MSG | MACH_RCV_TIMEOUT,
                0,
                HOST_MSG_BUF_SIZE as u32,
                service_port,
                timeout_ms as u32,
                MACH_PORT_NULL,
            )
        };

        const MACH_RCV_TIMED_OUT: mach_msg_return_t = 0x10004003;
        if kr == MACH_RCV_TIMED_OUT {
            return Ok(false);
        }
        if kr != MACH_MSG_SUCCESS {
            return Err("mach_msg receive failed");
        }

        let msgh_id = unsafe { (*header).msgh_id } as u32;
        match msgh_id {
            HELLO_MSG_ID => self.handle_hello(unsafe { &*(header as *const HelloMsg) }),
            GOODBYE_MSG_ID => {
                let body_ptr = unsafe { (header as *const u8).add(std::mem::size_of::<mach_msg_header_t>()) };
                let token = unsafe { std::ptr::read_unaligned(body_ptr as *const u64) };
                self.handle_goodbye(token);
            }
            FRAME_SUBMIT_MSG_ID => self.handle_frame_submit(unsafe { &*(header as *const FrameSubmitMsg) }),
            _ => {}
        }

        Ok(true)
    }

    pub fn pop_frame(&self) -> Option<IncomingFrame> {
        self.inner.lock().expect("host mutex poisoned").queue.pop_front()
    }

    pub fn release_frame_port(frame: &mut IncomingFrame) {
        if frame.surface_port != MACH_PORT_NULL {
            unsafe { mach_port_deallocate(mach_task_self(), frame.surface_port) };
            frame.surface_port = MACH_PORT_NULL;
        }
    }

    pub fn client_count(&self) -> usize {
        let inner = self.inner.lock().expect("host mutex poisoned");
        inner.clients.iter().filter(|c| c.active).count()
    }

    pub fn service_port(&self) -> mach_port_t {
        self.inner.lock().expect("host mutex poisoned").service_port
    }
}

impl Drop for CompositorHost {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().expect("host mutex poisoned");
        while let Some(frame) = inner.queue.pop_front() {
            if frame.surface_port != MACH_PORT_NULL {
                unsafe { mach_port_deallocate(mach_task_self(), frame.surface_port) };
            }
        }
        for client in inner.clients.iter() {
            if client.active && client.reply_port != MACH_PORT_NULL {
                unsafe { mach_port_deallocate(mach_task_self(), client.reply_port) };
            }
        }
        if inner.service_port != MACH_PORT_NULL {
            unsafe {
                mach_port_mod_refs(mach_task_self(), inner.service_port, MACH_PORT_RIGHT_RECEIVE, -1);
            }
        }
    }
}
