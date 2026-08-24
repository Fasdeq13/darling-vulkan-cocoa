use libc::{kqueue, timespec};
use mach2::kern_return::KERN_SUCCESS;
use mach2::mach_port::{mach_port_allocate, mach_port_deallocate};
use mach2::message::{mach_msg, mach_msg_header_t, MACH_MSG_SUCCESS, MACH_RCV_MSG, MACH_RCV_TIMEOUT};
use mach2::port::{mach_port_t, MACH_PORT_NULL, MACH_PORT_RIGHT_PORT_SET, MACH_PORT_RIGHT_RECEIVE};
use mach2::traps::mach_task_self;
use mach2::kern_return::kern_return_t;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

extern "C" {
    fn mach_port_insert_member(task: mach2::port::mach_port_t, member: mach2::port::mach_port_name_t, after: mach2::port::mach_port_name_t) -> kern_return_t;
}

const MAX_EVENTS: usize = 64;
const MAX_TIMERS: usize = 32;

const EVFILT_MACHPORT: i16 = -8;
const EVFILT_TIMER: i16 = -7;
const EV_ADD: u16 = 0x0001;
const EV_DELETE: u16 = 0x0002;
const EV_ENABLE: u16 = 0x0004;
const EV_CLEAR: u16 = 0x0020;
const NOTE_MSECONDS: u32 = 0x0000_0001;
const MACH_RCV_MSG_FLAG: i64 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
struct KEvent64 {
    ident: u64,
    filter: i16,
    flags: u16,
    fflags: u32,
    data: i64,
    udata: u64,
    ext0: u64,
    ext1: u64,
}

extern "C" {
    fn kevent64(
        kq: c_int,
        changelist: *const KEvent64,
        nchanges: c_int,
        eventlist: *mut KEvent64,
        nevents: c_int,
        flags: c_uint,
        timeout: *const timespec,
    ) -> c_int;
}

use std::os::raw::c_uint;

fn ev_set64(ident: u64, filter: i16, flags: u16, fflags: u32, data: i64, udata: u64) -> KEvent64 {
    KEvent64 {
        ident,
        filter,
        flags,
        fflags,
        data,
        udata,
        ext0: 0,
        ext1: 0,
    }
}

pub type FdCallback = Box<dyn Fn(i32, i16, isize) + Send + Sync>;
pub type TimerCallback = Arc<dyn Fn(u64) + Send + Sync>;
pub type MachCallback = Box<dyn Fn(mach_port_t, &[u8]) + Send + Sync>;

struct TimerEntry {
    id: u64,
    active: bool,
    callback: Option<TimerCallback>,
}

pub struct EventLoop {
    kq: i32,
    mach_recv_port: mach_port_t,
    mach_port_set: mach_port_t,
    running: AtomicBool,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    fd_cb: Mutex<Option<FdCallback>>,
    mach_cb: Mutex<Option<MachCallback>>,
    timers: Mutex<Vec<TimerEntry>>,
    next_timer_id: AtomicU64,
}

unsafe impl Send for EventLoop {}
unsafe impl Sync for EventLoop {}

impl EventLoop {
    pub fn create() -> Option<Self> {
        let kq = unsafe { kqueue() };
        if kq < 0 {
            return None;
        }

        let mut mach_recv_port: mach_port_t = MACH_PORT_NULL;
        let kr = unsafe { mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &mut mach_recv_port) };
        if kr != KERN_SUCCESS {
            unsafe { libc::close(kq) };
            return None;
        }

        let mut mach_port_set: mach_port_t = MACH_PORT_NULL;
        let kr = unsafe { mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_PORT_SET, &mut mach_port_set) };
        if kr == KERN_SUCCESS {
            unsafe { mach_port_insert_member(mach_task_self(), mach_recv_port, mach_port_set) };
        }

        let reg = ev_set64(
            mach_port_set as u64,
            EVFILT_MACHPORT,
            EV_ADD | EV_ENABLE,
            MACH_RCV_MSG_FLAG as u32,
            0,
            0,
        );
        let rc = unsafe { kevent64(kq, &reg, 1, std::ptr::null_mut(), 0, 0, std::ptr::null()) };
        if rc != 0 {
            unsafe {
                mach_port_deallocate(mach_task_self(), mach_port_set);
                mach_port_deallocate(mach_task_self(), mach_recv_port);
                libc::close(kq);
            }
            return None;
        }

        Some(Self {
            kq,
            mach_recv_port,
            mach_port_set,
            running: AtomicBool::new(false),
            thread: Mutex::new(None),
            fd_cb: Mutex::new(None),
            mach_cb: Mutex::new(None),
            timers: Mutex::new(Vec::with_capacity(MAX_TIMERS)),
            next_timer_id: AtomicU64::new(1),
        })
    }

    pub fn watch_fd(&self, fd: i32, filter: i16) -> bool {
        let reg = ev_set64(fd as u64, filter, EV_ADD | EV_ENABLE | EV_CLEAR, 0, 0, 0);
        unsafe { kevent64(self.kq, &reg, 1, std::ptr::null_mut(), 0, 0, std::ptr::null()) == 0 }
    }

    pub fn unwatch_fd(&self, fd: i32, filter: i16) -> bool {
        let reg = ev_set64(fd as u64, filter, EV_DELETE, 0, 0, 0);
        unsafe { kevent64(self.kq, &reg, 1, std::ptr::null_mut(), 0, 0, std::ptr::null()) == 0 }
    }

    pub fn set_fd_callback(&self, cb: FdCallback) {
        *self.fd_cb.lock().expect("fd callback mutex poisoned") = Some(cb);
    }

    pub fn set_mach_callback(&self, cb: MachCallback) {
        *self.mach_cb.lock().expect("mach callback mutex poisoned") = Some(cb);
    }

    pub fn mach_port(&self) -> mach_port_t {
        self.mach_recv_port
    }

    pub fn add_timer(&self, interval_ms: u64, callback: TimerCallback) -> Option<u64> {
        let mut timers = self.timers.lock().expect("timers mutex poisoned");
        if timers.len() >= MAX_TIMERS && timers.iter().all(|t| t.active) {
            return None;
        }

        let id = self.next_timer_id.fetch_add(1, Ordering::AcqRel);
        let entry = TimerEntry {
            id,
            active: true,
            callback: Some(callback),
        };

        if let Some(slot) = timers.iter_mut().find(|t| !t.active) {
            *slot = entry;
        } else {
            timers.push(entry);
        }
        drop(timers);

        let reg = ev_set64(id, EVFILT_TIMER, EV_ADD | EV_ENABLE, NOTE_MSECONDS, interval_ms as i64, 0);
        let rc = unsafe { kevent64(self.kq, &reg, 1, std::ptr::null_mut(), 0, 0, std::ptr::null()) };
        if rc != 0 {
            let mut timers = self.timers.lock().expect("timers mutex poisoned");
            if let Some(t) = timers.iter_mut().find(|t| t.id == id) {
                t.active = false;
            }
            return None;
        }

        Some(id)
    }

    pub fn remove_timer(&self, timer_id: u64) {
        let reg = ev_set64(timer_id, EVFILT_TIMER, EV_DELETE, 0, 0, 0);
        unsafe { kevent64(self.kq, &reg, 1, std::ptr::null_mut(), 0, 0, std::ptr::null()) };

        let mut timers = self.timers.lock().expect("timers mutex poisoned");
        if let Some(t) = timers.iter_mut().find(|t| t.id == timer_id) {
            t.active = false;
        }
    }

    fn dispatch_mach_message(&self) {
        let mut buf = vec![0u8; 4096];
        let header = buf.as_mut_ptr() as *mut mach_msg_header_t;

        let mr = unsafe {
            mach_msg(
                header,
                MACH_RCV_MSG | MACH_RCV_TIMEOUT,
                0,
                4096,
                self.mach_recv_port,
                0,
                MACH_PORT_NULL,
            )
        };

        if mr == MACH_MSG_SUCCESS {
            let size = unsafe { (*header).msgh_size } as usize;
            if let Some(cb) = self.mach_cb.lock().expect("mach callback mutex poisoned").as_ref() {
                cb(self.mach_recv_port, &buf[..size.min(buf.len())]);
            }
        }
    }

    fn dispatch_kevent(&self, ev: &KEvent64) {
        if ev.filter == EVFILT_MACHPORT {
            self.dispatch_mach_message();
            return;
        }

        if ev.filter == EVFILT_TIMER {
            let id = ev.ident;
            let cb: Option<TimerCallback> = {
                let timers = self.timers.lock().expect("timers mutex poisoned");
                timers.iter().find(|t| t.id == id).and_then(|t| t.callback.clone())
            };
            if let Some(cb) = cb {
                cb(id);
            }
            return;
        }

        if let Some(cb) = self.fd_cb.lock().expect("fd callback mutex poisoned").as_ref() {
            cb(ev.ident as i32, ev.filter, ev.data as isize);
        }
    }

    pub fn run_once(&self, timeout_ms: i32) -> i32 {
        let mut events: [KEvent64; MAX_EVENTS] = [ev_set64(0, 0, 0, 0, 0, 0); MAX_EVENTS];

        let ts = timespec {
            tv_sec: (timeout_ms / 1000) as i64,
            tv_nsec: ((timeout_ms % 1000) * 1_000_000) as i64,
        };
        let ts_ptr = if timeout_ms >= 0 { &ts as *const timespec } else { std::ptr::null() };

        let n = unsafe {
            kevent64(
                self.kq,
                std::ptr::null(),
                0,
                events.as_mut_ptr(),
                MAX_EVENTS as c_int,
                0,
                ts_ptr,
            )
        };

        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                return 0;
            }
            return -1;
        }

        for ev in events.iter().take(n as usize) {
            self.dispatch_kevent(ev);
        }

        n
    }

    fn thread_main(this: &EventLoop) {
        while this.running.load(Ordering::Acquire) {
            this.run_once(50);
        }
    }

    pub fn start(self: &'static Self) -> bool {
        if self.running.swap(true, Ordering::AcqRel) {
            return false;
        }
        let handle = std::thread::spawn(move || Self::thread_main(self));
        *self.thread.lock().expect("thread handle mutex poisoned") = Some(handle);
        true
    }

    pub fn stop(&self) {
        if !self.running.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Some(handle) = self.thread.lock().expect("thread handle mutex poisoned").take() {
            let _ = handle.join();
        }
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        if self.mach_port_set != MACH_PORT_NULL {
            unsafe { mach_port_deallocate(mach_task_self(), self.mach_port_set) };
        }
        if self.mach_recv_port != MACH_PORT_NULL {
            unsafe { mach_port_deallocate(mach_task_self(), self.mach_recv_port) };
        }
        if self.kq >= 0 {
            unsafe { libc::close(self.kq) };
        }
    }
}

static GLOBAL_LOOP: OnceLock<EventLoop> = OnceLock::new();
static GLOBAL_LOOP_FAILED: AtomicI32 = AtomicI32::new(0);

pub fn global() -> Option<&'static EventLoop> {
    if GLOBAL_LOOP.get().is_none() && GLOBAL_LOOP_FAILED.load(Ordering::Acquire) == 0 {
        match EventLoop::create() {
            Some(loop_) => {
                let _ = GLOBAL_LOOP.set(loop_);
            }
            None => {
                GLOBAL_LOOP_FAILED.store(1, Ordering::Release);
            }
        }
    }
    GLOBAL_LOOP.get()
}
