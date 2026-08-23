use ash::ext::debug_utils;
use ash::vk;
use ash::{Entry, Instance};
use std::ffi::CStr;
use std::sync::OnceLock;

pub type LogCallback = fn(severity: i32, message: &str);

static LOG_CALLBACK: OnceLock<LogCallback> = OnceLock::new();

pub fn set_debug_log_callback(callback: LogCallback) {
    let _ = LOG_CALLBACK.set(callback);
}

unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let severity_code = if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        3
    } else if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        2
    } else if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
        1
    } else {
        0
    };

    if callback_data.is_null() {
        return vk::FALSE;
    }
    let data = &*callback_data;
    if data.p_message.is_null() {
        return vk::FALSE;
    }
    let message = CStr::from_ptr(data.p_message).to_string_lossy();

    if let Some(cb) = LOG_CALLBACK.get() {
        cb(severity_code, &message);
    } else {
        eprintln!("[vulkan][{severity_code}] {message}");
    }

    vk::FALSE
}

pub fn create_debug_messenger(
    entry: &Entry,
    instance: &Instance,
) -> Result<(debug_utils::Instance, vk::DebugUtilsMessengerEXT), vk::Result> {
    let debug_utils_loader = debug_utils::Instance::new(entry, instance);

    let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(debug_callback));

    let messenger = unsafe { debug_utils_loader.create_debug_utils_messenger(&create_info, None)? };
    Ok((debug_utils_loader, messenger))
}

pub fn destroy_debug_messenger(loader: &debug_utils::Instance, messenger: vk::DebugUtilsMessengerEXT) {
    if messenger != vk::DebugUtilsMessengerEXT::null() {
        unsafe {
            loader.destroy_debug_utils_messenger(messenger, None);
        }
    }
}

pub fn check_validation_layer_support(entry: &Entry, layer_name: &str) -> bool {
    let layers = match unsafe { entry.enumerate_instance_layer_properties() } {
        Ok(l) => l,
        Err(_) => return false,
    };
    layers.iter().any(|layer| {
        let name = unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) };
        name.to_string_lossy() == layer_name
    })
}
