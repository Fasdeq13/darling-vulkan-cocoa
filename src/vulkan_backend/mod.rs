pub mod resource;
pub mod state_machine;
pub mod swapchain;

use ash::khr::{surface, swapchain as khr_swapchain};
use ash::vk;
use ash::{Device, Instance};
use state_machine::SharedDevice;
use std::collections::HashSet;
use std::ffi::CStr;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DeviceSelectionError {
    #[error("vulkan error: {0}")]
    Vk(#[from] vk::Result),
    #[error("no Vulkan-capable physical devices found on this system")]
    NoDevices,
    #[error("no physical device satisfies the required extensions, queues, and presentation support")]
    NoSuitableDevice,
}

const EXT_SWAPCHAIN: &CStr = c"VK_KHR_swapchain";
const EXT_EXTERNAL_MEMORY: &CStr = c"VK_KHR_external_memory";
const EXT_EXTERNAL_MEMORY_FD: &CStr = c"VK_KHR_external_memory_fd";
const EXT_EXTERNAL_MEMORY_HOST: &CStr = c"VK_EXT_external_memory_host";
const EXT_METAL_OBJECTS: &CStr = c"VK_EXT_metal_objects";

const REQUIRED_DEVICE_EXTENSIONS: &[&CStr] = &[EXT_SWAPCHAIN];

const PREFERRED_DEVICE_EXTENSIONS: &[&CStr] = &[
    EXT_EXTERNAL_MEMORY,
    EXT_EXTERNAL_MEMORY_FD,
    EXT_EXTERNAL_MEMORY_HOST,
    EXT_METAL_OBJECTS,
];

#[derive(Debug, Clone, Copy)]
pub struct QueueFamilyIndices {
    pub graphics: u32,
    pub transfer: u32,
    pub graphics_supports_present: bool,
}

#[derive(Debug)]
struct Candidate {
    physical_device: vk::PhysicalDevice,
    score: i64,
    name: String,
    queues: QueueFamilyIndices,
    available_extensions: HashSet<String>,
}

fn cstr_to_string(raw: &[std::ffi::c_char]) -> String {
    let cstr = unsafe { CStr::from_ptr(raw.as_ptr()) };
    cstr.to_string_lossy().into_owned()
}

fn device_name(instance: &Instance, pdevice: vk::PhysicalDevice) -> String {
    let props = unsafe { instance.get_physical_device_properties(pdevice) };
    cstr_to_string(&props.device_name)
}

fn enumerate_device_extensions(
    instance: &Instance,
    pdevice: vk::PhysicalDevice,
) -> Result<HashSet<String>, vk::Result> {
    let props = unsafe { instance.enumerate_device_extension_properties(pdevice)? };
    Ok(props.iter().map(|e| cstr_to_string(&e.extension_name)).collect())
}

fn pick_queue_families(
    instance: &Instance,
    pdevice: vk::PhysicalDevice,
    surface_ext: &surface::Instance,
    surface: Option<vk::SurfaceKHR>,
) -> Option<QueueFamilyIndices> {
    let families = unsafe { instance.get_physical_device_queue_family_properties(pdevice) };

    let mut best_graphics: Option<(u32, bool)> = None;
    let mut dedicated_transfer: Option<u32> = None;

    for (i, fam) in families.iter().enumerate() {
        let idx = i as u32;
        let has_graphics = fam.queue_flags.contains(vk::QueueFlags::GRAPHICS);
        let has_compute = fam.queue_flags.contains(vk::QueueFlags::COMPUTE);
        let has_transfer = fam.queue_flags.contains(vk::QueueFlags::TRANSFER);

        if has_graphics {
            let supports_present = match surface {
                Some(surf) => unsafe {
                    surface_ext
                        .get_physical_device_surface_support(pdevice, idx, surf)
                        .unwrap_or(false)
                },
                None => true,
            };

            let is_better = match best_graphics {
                None => true,
                Some((_, prev_present)) => supports_present && !prev_present,
            };
            if is_better {
                best_graphics = Some((idx, supports_present));
            }
        }

        if has_transfer && !has_graphics && !has_compute && dedicated_transfer.is_none() {
            dedicated_transfer = Some(idx);
        }
    }

    let (graphics, graphics_supports_present) = best_graphics?;
    let transfer = dedicated_transfer.unwrap_or(graphics);

    Some(QueueFamilyIndices {
        graphics,
        transfer,
        graphics_supports_present,
    })
}

fn score_candidate(instance: &Instance, pdevice: vk::PhysicalDevice, extensions: &HashSet<String>) -> i64 {
    let props = unsafe { instance.get_physical_device_properties(pdevice) };
    let features = unsafe { instance.get_physical_device_features(pdevice) };
    let mem_props = unsafe { instance.get_physical_device_memory_properties(pdevice) };

    let mut score: i64 = match props.device_type {
        vk::PhysicalDeviceType::DISCRETE_GPU => 100_000,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 10_000,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 1_000,
        vk::PhysicalDeviceType::CPU => 100,
        _ => 0,
    };

    let device_local_bytes: u64 = mem_props
        .memory_heaps
        .iter()
        .take(mem_props.memory_heap_count as usize)
        .filter(|h| h.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|h| h.size)
        .sum();
    score += (device_local_bytes / (64 * 1024 * 1024)) as i64;

    score += props.limits.max_image_dimension2_d as i64 / 64;

    if features.sampler_anisotropy == vk::TRUE {
        score += 500;
    }
    if features.shader_storage_image_extended_formats == vk::TRUE {
        score += 200;
    }
    if features.geometry_shader == vk::TRUE {
        score += 100;
    }
    if features.tessellation_shader == vk::TRUE {
        score += 100;
    }

    for ext in PREFERRED_DEVICE_EXTENSIONS {
        if extensions.contains(ext.to_str().unwrap_or("")) {
            score += 1_500;
        }
    }

    score
}

pub fn select_physical_device(
    instance: &Instance,
    surface_ext: &surface::Instance,
    surface: Option<vk::SurfaceKHR>,
) -> Result<(vk::PhysicalDevice, QueueFamilyIndices, HashSet<String>, String), DeviceSelectionError> {
    let pdevices = unsafe { instance.enumerate_physical_devices()? };
    if pdevices.is_empty() {
        return Err(DeviceSelectionError::NoDevices);
    }

    let mut candidates: Vec<Candidate> = Vec::new();

    for &pdevice in &pdevices {
        let name = device_name(instance, pdevice);

        let extensions = match enumerate_device_extensions(instance, pdevice) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("[ravynOS] GPU '{name}' rejected: failed to enumerate extensions ({e})");
                continue;
            }
        };

        let missing_required = REQUIRED_DEVICE_EXTENSIONS
            .iter()
            .any(|req| !extensions.contains(req.to_str().unwrap_or("")));
        if missing_required {
            log::warn!("[ravynOS] GPU '{name}' rejected: missing VK_KHR_swapchain");
            continue;
        }

        let queues = match pick_queue_families(instance, pdevice, surface_ext, surface) {
            Some(q) => q,
            None => {
                log::warn!("[ravynOS] GPU '{name}' rejected: no graphics-capable queue family");
                continue;
            }
        };

        if surface.is_some() && !queues.graphics_supports_present {
            log::warn!("[ravynOS] GPU '{name}' rejected: cannot present to the given surface");
            continue;
        }

        let score = score_candidate(instance, pdevice, &extensions);
        log::info!(
            "[ravynOS] GPU candidate '{name}' scored {score} (graphics family {}, transfer family {})",
            queues.graphics, queues.transfer
        );

        candidates.push(Candidate {
            physical_device: pdevice,
            score,
            name,
            queues,
            available_extensions: extensions,
        });
    }

    candidates
        .into_iter()
        .max_by_key(|c| c.score)
        .map(|c| {
            log::info!("[ravynOS] Selected GPU: {} (score {})", c.name, c.score);
            (c.physical_device, c.queues, c.available_extensions, c.name)
        })
        .ok_or(DeviceSelectionError::NoSuitableDevice)
}

fn create_logical_device(
    instance: &Instance,
    pdevice: vk::PhysicalDevice,
    queues: &QueueFamilyIndices,
    available_extensions: &HashSet<String>,
) -> Result<(Device, vk::Queue, vk::Queue), DeviceSelectionError> {
    let priority = [1.0f32];
    let mut queue_create_infos = vec![vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queues.graphics)
        .queue_priorities(&priority)];

    if queues.transfer != queues.graphics {
        queue_create_infos.push(
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queues.transfer)
                .queue_priorities(&priority),
        );
    }

    let mut enabled_extensions: Vec<*const std::ffi::c_char> =
        REQUIRED_DEVICE_EXTENSIONS.iter().map(|c| c.as_ptr()).collect();
    for ext in PREFERRED_DEVICE_EXTENSIONS {
        if available_extensions.contains(ext.to_str().unwrap_or("")) {
            enabled_extensions.push(ext.as_ptr());
        }
    }

    let supported_features = unsafe { instance.get_physical_device_features(pdevice) };
    let mut features = vk::PhysicalDeviceFeatures::default();
    if supported_features.sampler_anisotropy == vk::TRUE {
        features.sampler_anisotropy = vk::TRUE;
    }
    if supported_features.shader_storage_image_extended_formats == vk::TRUE {
        features.shader_storage_image_extended_formats = vk::TRUE;
    }

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&enabled_extensions)
        .enabled_features(&features);

    let device = unsafe { instance.create_device(pdevice, &device_create_info, None)? };

    let graphics_queue = unsafe { device.get_device_queue(queues.graphics, 0) };
    let transfer_queue = unsafe { device.get_device_queue(queues.transfer, 0) };

    Ok((device, graphics_queue, transfer_queue))
}

pub fn build_shared_device(
    instance: Instance,
    surface_ext: surface::Instance,
    presentation_surface: Option<vk::SurfaceKHR>,
) -> Result<Arc<SharedDevice>, DeviceSelectionError> {
    let (pdevice, queues, available_extensions, name) =
        select_physical_device(&instance, &surface_ext, presentation_surface)?;

    let (device, graphics_queue, transfer_queue) =
        create_logical_device(&instance, pdevice, &queues, &available_extensions)?;

    let swapchain_ext = khr_swapchain::Device::new(&instance, &device);

    let pool_info = vk::CommandPoolCreateInfo::default()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(queues.graphics);
    let command_pool = unsafe { device.create_command_pool(&pool_info, None)? };

    log::info!(
        "[ravynOS] Vulkan device ready: {name} (graphics family {}, transfer family {}, present-capable {})",
        queues.graphics, queues.transfer, queues.graphics_supports_present
    );

    Ok(Arc::new(SharedDevice {
        instance,
        physical_device: pdevice,
        device,
        graphics_queue,
        graphics_queue_family: queues.graphics,
        transfer_queue,
        transfer_queue_family: queues.transfer,
        surface_ext,
        swapchain_ext,
        command_pool,
    }))
}