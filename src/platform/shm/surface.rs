use libc::{
    c_void, close, ftruncate, mmap, munmap, off_t, shm_open, shm_unlink, MAP_FAILED,
    MAP_SHARED, O_CREAT, O_EXCL, O_RDWR, PROT_READ, PROT_WRITE,
};
use std::ffi::CString;
use std::sync::Mutex;

const MAGIC: u32 = 0x514D_5631;
const NAME_MAX: usize = 96;
const MAX_SURFACES: usize = 256;

#[repr(C)]
struct SurfaceHeader {
    magic: u32,
    width: u32,
    height: u32,
    stride: u32,
    pixel_format: u32,
    lock_state: std::sync::atomic::AtomicU32,
    frame_counter: std::sync::atomic::AtomicU64,
    data_size: u64,
}

pub struct Surface {
    fd: i32,
    name: String,
    base: *mut c_void,
    mapped_size: usize,
    header: *mut SurfaceHeader,
    pixel_data: *mut c_void,
    owner: bool,
}

unsafe impl Send for Surface {}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

static SLOT_TABLE: Mutex<Option<Vec<usize>>> = Mutex::new(None);

fn register_slot(ptr: usize) {
    let mut table = SLOT_TABLE.lock().expect("shm slot table mutex poisoned");
    let slots = table.get_or_insert_with(|| Vec::with_capacity(MAX_SURFACES));
    if slots.len() < MAX_SURFACES {
        slots.push(ptr);
    }
}

fn unregister_slot(ptr: usize) {
    let mut table = SLOT_TABLE.lock().expect("shm slot table mutex poisoned");
    if let Some(slots) = table.as_mut() {
        slots.retain(|&p| p != ptr);
    }
}

pub fn table_count() -> usize {
    SLOT_TABLE
        .lock()
        .expect("shm slot table mutex poisoned")
        .as_ref()
        .map(|s| s.len())
        .unwrap_or(0)
}

impl Surface {
    pub fn create(name: &str, width: u32, height: u32, stride: u32, pixel_format: u32) -> Option<Self> {
        if name.is_empty() || width == 0 || height == 0 || stride == 0 {
            return None;
        }

        let mut c_name = name.to_string();
        c_name.truncate(NAME_MAX - 1);
        let cstr = CString::new(c_name.clone()).ok()?;

        let payload_size = stride as usize * height as usize;
        let total_size = align_up(std::mem::size_of::<SurfaceHeader>() + payload_size, 4096);

        let mut fd = unsafe { shm_open(cstr.as_ptr(), O_CREAT | O_RDWR | O_EXCL, 0o600) };
        if fd < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST) {
                unsafe { shm_unlink(cstr.as_ptr()) };
                fd = unsafe { shm_open(cstr.as_ptr(), O_CREAT | O_RDWR | O_EXCL, 0o600) };
            }
            if fd < 0 {
                return None;
            }
        }

        if unsafe { ftruncate(fd, total_size as off_t) } != 0 {
            unsafe {
                close(fd);
                shm_unlink(cstr.as_ptr());
            }
            return None;
        }

        let base = unsafe {
            mmap(
                std::ptr::null_mut(),
                total_size,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                fd,
                0,
            )
        };
        if base == MAP_FAILED {
            unsafe {
                close(fd);
                shm_unlink(cstr.as_ptr());
            }
            return None;
        }

        let header = base as *mut SurfaceHeader;
        unsafe {
            (*header).magic = MAGIC;
            (*header).width = width;
            (*header).height = height;
            (*header).stride = stride;
            (*header).pixel_format = pixel_format;
            (*header)
                .lock_state
                .store(0, std::sync::atomic::Ordering::Relaxed);
            (*header)
                .frame_counter
                .store(0, std::sync::atomic::Ordering::Relaxed);
            (*header).data_size = payload_size as u64;
        }

        let pixel_data = unsafe { (base as *mut u8).add(std::mem::size_of::<SurfaceHeader>()) as *mut c_void };

        let surface = Surface {
            fd,
            name: c_name,
            base,
            mapped_size: total_size,
            header,
            pixel_data,
            owner: true,
        };
        register_slot(base as usize);
        Some(surface)
    }

    pub fn open(name: &str) -> Option<Self> {
        if name.is_empty() {
            return None;
        }
        let mut c_name = name.to_string();
        c_name.truncate(NAME_MAX - 1);
        let cstr = CString::new(c_name.clone()).ok()?;

        let fd = unsafe { shm_open(cstr.as_ptr(), O_RDWR, 0o600) };
        if fd < 0 {
            return None;
        }

        let mut probe: SurfaceHeader = unsafe { std::mem::zeroed() };
        let read_bytes = unsafe {
            libc::read(
                fd,
                &mut probe as *mut SurfaceHeader as *mut c_void,
                std::mem::size_of::<SurfaceHeader>(),
            )
        };
        if read_bytes as usize != std::mem::size_of::<SurfaceHeader>() || probe.magic != MAGIC {
            unsafe { close(fd) };
            return None;
        }
        unsafe { libc::lseek(fd, 0, libc::SEEK_SET) };

        let total_size = align_up(std::mem::size_of::<SurfaceHeader>() + probe.data_size as usize, 4096);
        let base = unsafe {
            mmap(
                std::ptr::null_mut(),
                total_size,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                fd,
                0,
            )
        };
        if base == MAP_FAILED {
            unsafe { close(fd) };
            return None;
        }

        let header = base as *mut SurfaceHeader;
        let pixel_data = unsafe { (base as *mut u8).add(std::mem::size_of::<SurfaceHeader>()) as *mut c_void };

        let surface = Surface {
            fd,
            name: c_name,
            base,
            mapped_size: total_size,
            header,
            pixel_data,
            owner: false,
        };
        register_slot(base as usize);
        Some(surface)
    }

    pub fn lock(&self) -> (*mut c_void, u64) {
        unsafe {
            (*self.header)
                .lock_state
                .fetch_or(1, std::sync::atomic::Ordering::AcqRel);
            (self.pixel_data, (*self.header).data_size)
        }
    }

    pub fn unlock(&self) {
        unsafe {
            (*self.header)
                .lock_state
                .fetch_and(!1u32, std::sync::atomic::Ordering::AcqRel);
            (*self.header)
                .frame_counter
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        }
    }

    pub fn frame_counter(&self) -> u64 {
        unsafe { (*self.header).frame_counter.load(std::sync::atomic::Ordering::Acquire) }
    }

    pub fn fd(&self) -> i32 {
        self.fd
    }

    pub fn width(&self) -> u32 {
        unsafe { (*self.header).width }
    }

    pub fn height(&self) -> u32 {
        unsafe { (*self.header).height }
    }

    pub fn stride(&self) -> u32 {
        unsafe { (*self.header).stride }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        if !self.base.is_null() && self.base != MAP_FAILED {
            unsafe { munmap(self.base, self.mapped_size) };
        }
        if self.fd >= 0 {
            unsafe { close(self.fd) };
        }
        if self.owner {
            if let Ok(cstr) = CString::new(self.name.clone()) {
                unsafe { shm_unlink(cstr.as_ptr()) };
            }
        }
        unregister_slot(self.base as usize);
    }
}
