use ash::vk;
use ash::{Entry, Instance};
use std::ffi::CStr;

#[derive(Debug, Clone, Copy, Default)]
pub struct DarwinInstanceExtensions {
    pub has_metal_surface: bool,
    pub has_portability_enumeration: bool,
    pub has_external_memory_capabilities: bool,
    pub has_debug_utils: bool,
    pub total_extension_count: u32,
}

pub fn check_darwin_instance_extensions(entry: &Entry) -> DarwinInstanceExtensions {
    let mut result = DarwinInstanceExtensions::default();

    let extensions = match unsafe { entry.enumerate_instance_extension_properties(None) } {
        Ok(e) => e,
        Err(_) => return result,
    };

    result.total_extension_count = extensions.len() as u32;

    for ext in &extensions {
        let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) }.to_string_lossy();
        match name.as_ref() {
            "VK_EXT_metal_surface" => result.has_metal_surface = true,
            "VK_KHR_portability_enumeration" => result.has_portability_enumeration = true,
            "VK_KHR_external_memory_capabilities" => result.has_external_memory_capabilities = true,
            "VK_EXT_debug_utils" => result.has_debug_utils = true,
            _ => {}
        }
    }

    result
}

pub fn is_extension_available(entry: &Entry, extension_name: &str) -> bool {
    let extensions = match unsafe { entry.enumerate_instance_extension_properties(None) } {
        Ok(e) => e,
        Err(_) => return false,
    };
    extensions.iter().any(|ext| {
        let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
        name.to_string_lossy() == extension_name
    })
}

pub fn instance_api_version(entry: &Entry) -> u32 {
    match unsafe { entry.try_enumerate_instance_version() } {
        Ok(Some(v)) => v,
        _ => vk::API_VERSION_1_0,
    }
}

#[cfg(target_os = "macos")]
pub mod metal_surface {
    use super::*;
    use std::os::raw::c_void;

    #[repr(C)]
    pub struct MetalSurfaceCreateInfoExt {
        pub s_type: vk::StructureType,
        pub p_next: *const c_void,
        pub flags: u32,
        pub p_layer: *const c_void,
    }

    pub const STRUCTURE_TYPE_METAL_SURFACE_CREATE_INFO_EXT: vk::StructureType =
        vk::StructureType::from_raw(1000217000);

    type PfnCreateMetalSurfaceExt = unsafe extern "system" fn(
        instance: vk::Instance,
        create_info: *const MetalSurfaceCreateInfoExt,
        allocator: *const c_void,
        surface: *mut vk::SurfaceKHR,
    ) -> vk::Result;

    pub fn create_metal_surface(
        entry: &Entry,
        instance: &Instance,
        metal_layer: *const c_void,
    ) -> Result<vk::SurfaceKHR, vk::Result> {
        if metal_layer.is_null() {
            return Err(vk::Result::ERROR_INITIALIZATION_FAILED);
        }

        let create_info = MetalSurfaceCreateInfoExt {
            s_type: STRUCTURE_TYPE_METAL_SURFACE_CREATE_INFO_EXT,
            p_next: std::ptr::null(),
            flags: 0,
            p_layer: metal_layer,
        };

        let name = c"vkCreateMetalSurfaceEXT";
        let create_fn = unsafe { entry.get_instance_proc_addr(instance.handle(), name.as_ptr()) };
        let create_fn = match create_fn {
            Some(f) => unsafe { std::mem::transmute::<_, PfnCreateMetalSurfaceExt>(f) },
            None => return Err(vk::Result::ERROR_EXTENSION_NOT_PRESENT),
        };

        let mut surface = vk::SurfaceKHR::null();
        let result = unsafe { create_fn(instance.handle(), &create_info, std::ptr::null(), &mut surface) };
        if result == vk::Result::SUCCESS {
            Ok(surface)
        } else {
            Err(result)
        }
    }
}
