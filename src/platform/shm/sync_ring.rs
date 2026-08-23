use libc::{
    c_void, close, ftruncate, mmap, munmap, off_t, shm_open, shm_unlink, MAP_FAILED,
    MAP_SHARED, O_CREAT, O_EXCL, O_RDWR, PROT_READ, PROT_WRITE,
};
use std::ffi::CString;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const MAGIC: u32 = 0x514D_5653;
const NAME_MAX: usize = 96;
const MAX_SLOTS: u32 = 8;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameState {
    Free = 0,
    Writing = 1,
    Ready = 2,
    Reading = 3,
}

#[repr(C)]
struct ShmSlot {
    state: AtomicU32,
    generation: AtomicU64,
    frame_index: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
struct ShmSyncRegion {
    magic: u32,
    slot_count: u32,
    write_cursor: AtomicU32,
    read_cursor: AtomicU32,
    slots: [ShmSlot; MAX_SLOTS as usize],
}

pub struct ShmSync {
    fd: i32,
    name: String,
    base: *mut c_void,
    mapped_size: usize,
    region: *mut ShmSyncRegion,
    owner: bool,
}

unsafe impl Send for ShmSync {}
unsafe impl Sync for ShmSync {}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

impl ShmSync {
    pub fn create(name: &str, slot_count: u32) -> Option<Self> {
        if name.is_empty() || slot_count == 0 || slot_count > MAX_SLOTS {
            return None;
        }

        let mut c_name = name.to_string();
        c_name.truncate(NAME_MAX - 1);
        let cstr = CString::new(c_name.clone()).ok()?;

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

        let total_size = align_up(std::mem::size_of::<ShmSyncRegion>(), 4096);
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

        let region = base as *mut ShmSyncRegion;
        unsafe {
            (*region).magic = MAGIC;
            (*region).slot_count = slot_count;
            (*region).write_cursor.store(0, Ordering::Relaxed);
            (*region).read_cursor.store(0, Ordering::Relaxed);
            for i in 0..slot_count as usize {
                (*region).slots[i].state.store(FrameState::Free as u32, Ordering::Relaxed);
                (*region).slots[i].generation.store(0, Ordering::Relaxed);
                (*region).slots[i].frame_index = 0;
                (*region).slots[i].width = 0;
                (*region).slots[i].height = 0;
            }
        }

        Some(ShmSync {
            fd,
            name: c_name,
            base,
            mapped_size: total_size,
            region,
            owner: true,
        })
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

        let total_size = align_up(std::mem::size_of::<ShmSyncRegion>(), 4096);
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

        let region = base as *mut ShmSyncRegion;
        if unsafe { (*region).magic } != MAGIC {
            unsafe { munmap(base, total_size) };
            unsafe { close(fd) };
            return None;
        }

        Some(ShmSync {
            fd,
            name: c_name,
            base,
            mapped_size: total_size,
            region,
            owner: false,
        })
    }

    fn try_transition(slot: &ShmSlot, expected: FrameState, desired: FrameState) -> bool {
        slot.state
            .compare_exchange(
                expected as u32,
                desired as u32,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub fn acquire_write_slot(&self) -> Option<u32> {
        let region = unsafe { &*self.region };
        for _ in 0..region.slot_count {
            let idx = region.write_cursor.fetch_add(1, Ordering::AcqRel) % region.slot_count;
            let slot = &region.slots[idx as usize];
            if Self::try_transition(slot, FrameState::Free, FrameState::Writing) {
                return Some(idx);
            }
        }
        None
    }

    pub fn publish_slot(&self, slot_index: u32, frame_index: u32, width: u32, height: u32) -> bool {
        let region = unsafe { &mut *self.region };
        if slot_index >= region.slot_count {
            return false;
        }
        let slot = &mut region.slots[slot_index as usize];
        if !Self::try_transition(slot, FrameState::Writing, FrameState::Ready) {
            return false;
        }
        slot.frame_index = frame_index;
        slot.width = width;
        slot.height = height;
        slot.generation.fetch_add(1, Ordering::AcqRel);
        true
    }

    pub fn acquire_read_slot(&self) -> Option<u32> {
        let region = unsafe { &*self.region };
        for i in 0..region.slot_count {
            let slot = &region.slots[i as usize];
            if Self::try_transition(slot, FrameState::Ready, FrameState::Reading) {
                return Some(i);
            }
        }
        None
    }

    pub fn release_read_slot(&self, slot_index: u32) -> bool {
        let region = unsafe { &*self.region };
        if slot_index >= region.slot_count {
            return false;
        }
        let slot = &region.slots[slot_index as usize];
        Self::try_transition(slot, FrameState::Reading, FrameState::Free)
    }

    pub fn slot_info(&self, slot_index: u32) -> Option<(u32, u32, u32)> {
        let region = unsafe { &*self.region };
        if slot_index >= region.slot_count {
            return None;
        }
        let slot = &region.slots[slot_index as usize];
        Some((slot.frame_index, slot.width, slot.height))
    }

    pub fn wait_for_ready(&self, timeout: std::time::Duration) -> Option<u32> {
        let start = std::time::Instant::now();
        loop {
            if let Some(idx) = self.acquire_read_slot() {
                return Some(idx);
            }
            if start.elapsed() >= timeout {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_micros(500));
        }
    }

    pub fn fd(&self) -> i32 {
        self.fd
    }
}

impl Drop for ShmSync {
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
    }
}
