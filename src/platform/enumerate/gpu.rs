use objc2::rc::Id;
use objc2::runtime::AnyObject;
use objc2::msg_send;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia = 0x10DE,
    Amd = 0x1002,
    Intel = 0x8086,
    Apple = 0x106B,
    Unknown = 0x0000,
}

impl GpuVendor {
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            0x10DE => GpuVendor::Nvidia,
            0x1002 => GpuVendor::Amd,
            0x8086 => GpuVendor::Intel,
            0x106B => GpuVendor::Apple,
            _ => GpuVendor::Unknown,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            GpuVendor::Nvidia => "NVIDIA",
            GpuVendor::Amd => "AMD",
            GpuVendor::Intel => "Intel",
            GpuVendor::Apple => "Apple",
            GpuVendor::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub is_low_power: bool,
    pub is_removable: bool,
    pub recommended_max_working_set_size: u64,
    pub supports_raytracing: bool,
    pub vendor_name: &'static str,
}

fn extract_vendor_id(name: &str) -> u32 {
    let lower = name.to_lowercase();
    if lower.contains("nvidia") || lower.contains("geforce") || lower.contains("quadro") || lower.contains("rtx") || lower.contains("gtx") {
        return GpuVendor::Nvidia as u32;
    }
    if lower.contains("amd") || lower.contains("radeon") {
        return GpuVendor::Amd as u32;
    }
    if lower.contains("intel") || lower.contains("iris") || lower.contains("uhd") {
        return GpuVendor::Intel as u32;
    }
    if lower.contains("apple") || lower.contains("m1") || lower.contains("m2") || lower.contains("m3") || lower.contains("m4") {
        return GpuVendor::Apple as u32;
    }
    GpuVendor::Unknown as u32
}

unsafe fn device_name(device: &AnyObject) -> String {
    let ns_name: Option<Id<AnyObject>> = msg_send![device, name];
    let ns_name = match ns_name {
        Some(n) => n,
        None => return String::new(),
    };
    let utf8_ptr: *const std::os::raw::c_char = msg_send![&ns_name, UTF8String];
    if utf8_ptr.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(utf8_ptr).to_string_lossy().into_owned()
}

unsafe fn build_info(device: &AnyObject) -> GpuInfo {
    let name = device_name(device);
    let vendor_id = extract_vendor_id(&name);
    let vendor = GpuVendor::from_raw(vendor_id);
    let is_low_power: bool = msg_send![device, isLowPower];
    let is_removable: bool = msg_send![device, isRemovable];
    let recommended_max_working_set_size: u64 = msg_send![device, recommendedMaxWorkingSetSize];
    let supports_raytracing: bool = msg_send![device, supportsRaytracing];

    GpuInfo {
        name,
        vendor_id,
        device_id: 0,
        is_low_power,
        is_removable,
        recommended_max_working_set_size,
        supports_raytracing,
        vendor_name: vendor.display_name(),
    }
}

pub fn enumerate_all() -> Vec<GpuInfo> {
    unsafe extern "C" {
        fn MTLCopyAllDevices() -> *mut AnyObject;
    }
    let array_ptr = unsafe { MTLCopyAllDevices() };
    if array_ptr.is_null() {
        return Vec::new();
    }
    let array: Id<AnyObject> = unsafe { Id::retain(array_ptr) }.expect("MTLCopyAllDevices returned null");

    let count: usize = unsafe { msg_send![&array, count] };
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let device: *mut AnyObject = unsafe { msg_send![&array, objectAtIndex: i] };
        if device.is_null() {
            continue;
        }
        result.push(unsafe { build_info(&*device) });
    }
    result
}

pub fn system_default() -> Option<GpuInfo> {
    unsafe extern "C" {
        fn MTLCreateSystemDefaultDevice() -> *mut AnyObject;
    }
    let device_ptr = unsafe { MTLCreateSystemDefaultDevice() };
    if device_ptr.is_null() {
        return None;
    }
    let device: Id<AnyObject> = unsafe { Id::retain(device_ptr) }?;
    Some(unsafe { build_info(&device) })
}
