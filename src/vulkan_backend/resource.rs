use crate::vulkan_backend::state_machine::SharedDevice;
use ash::vk;
use ash::Device;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResourceError {
    #[error("vulkan error: {0}")]
    Vk(#[from] vk::Result),
    #[error("no suitable memory type for type_bits={0:#x} flags={1:?}")]
    NoMemoryType(u32, vk::MemoryPropertyFlags),
    #[error("platform import failed: {0}")]
    ImportFailed(&'static str),
}

pub fn find_memory_type(
    shared: &SharedDevice,
    type_bits: u32,
    properties: vk::MemoryPropertyFlags,
) -> Result<u32, ResourceError> {
    let mem_props = unsafe {
        shared
            .instance
            .get_physical_device_memory_properties(shared.physical_device)
    };
    for i in 0..mem_props.memory_type_count {
        let supported = (type_bits & (1 << i)) != 0;
        let matches = mem_props.memory_types[i as usize]
            .property_flags
            .contains(properties);
        if supported && matches {
            return Ok(i);
        }
    }
    Err(ResourceError::NoMemoryType(type_bits, properties))
}

pub struct GpuImage {
    pub image: vk::Image,
    pub memory: Option<vk::DeviceMemory>,
    pub view: vk::ImageView,
    pub extent: vk::Extent2D,
    pub format: vk::Format,
    owns_memory: bool,
}

impl GpuImage {
    pub fn destroy(&mut self, device: &Device) {
        unsafe {
            if self.view != vk::ImageView::null() {
                device.destroy_image_view(self.view, None);
                self.view = vk::ImageView::null();
            }
            if self.image != vk::Image::null() {
                device.destroy_image(self.image, None);
                self.image = vk::Image::null();
            }
            if self.owns_memory {
                if let Some(mem) = self.memory.take() {
                    device.free_memory(mem, None);
                }
            }
        }
    }
}

fn make_view(
    shared: &SharedDevice,
    image: vk::Image,
    format: vk::Format,
) -> Result<vk::ImageView, ResourceError> {
    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .components(vk::ComponentMapping {
            r: vk::ComponentSwizzle::IDENTITY,
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: vk::ComponentSwizzle::IDENTITY,
        })
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    Ok(unsafe { shared.device.create_image_view(&view_info, None)? })
}

pub fn create_color_attachment(
    shared: &SharedDevice,
    extent: vk::Extent2D,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
) -> Result<GpuImage, ResourceError> {
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);

    let image = unsafe { shared.device.create_image(&image_info, None)? };
    let requirements = unsafe { shared.device.get_image_memory_requirements(image) };
    let type_index = find_memory_type(
        shared,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;

    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(type_index);
    let memory = unsafe { shared.device.allocate_memory(&alloc_info, None)? };
    unsafe {
        shared.device.bind_image_memory(image, memory, 0)?;
    }

    let view = make_view(shared, image, format)?;

    Ok(GpuImage {
        image,
        memory: Some(memory),
        view,
        extent,
        format,
        owns_memory: true,
    })
}

pub fn import_opaque_fd(
    shared: &SharedDevice,
    fd: std::os::fd::RawFd,
    extent: vk::Extent2D,
    format: vk::Format,
) -> Result<GpuImage, ResourceError> {
    let mut ext_image_info =
        vk::ExternalMemoryImageCreateInfo::default().handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);

    let image_info = vk::ImageCreateInfo::default()
        .push_next(&mut ext_image_info)
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(
            vk::ImageUsageFlags::SAMPLED_BIT
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::COLOR_ATTACHMENT,
        )
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);

    let image = unsafe { shared.device.create_image(&image_info, None)? };
    let requirements = unsafe { shared.device.get_image_memory_requirements(image) };

    let mut import_fd_info = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD)
        .fd(fd);

    let type_index = find_memory_type(
        shared,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;

    let alloc_info = vk::MemoryAllocateInfo::default()
        .push_next(&mut import_fd_info)
        .allocation_size(requirements.size)
        .memory_type_index(type_index);

    let memory = unsafe { shared.device.allocate_memory(&alloc_info, None)? };
    unsafe {
        shared.device.bind_image_memory(image, memory, 0)?;
    }

    let view = make_view(shared, image, format)?;

    Ok(GpuImage {
        image,
        memory: Some(memory),
        view,
        extent,
        format,
        owns_memory: true,
    })
}

#[cfg(target_os = "macos")]
pub mod iosurface_import {
    use super::*;
    use mach2::port::mach_port_t;
    use std::os::raw::c_void;

    #[link(name = "IOSurface", kind = "framework")]
    extern "C" {
        fn IOSurfaceLookupFromMachPort(port: mach_port_t) -> *mut c_void;
        fn IOSurfaceGetWidth(surface: *mut c_void) -> usize;
        fn IOSurfaceGetHeight(surface: *mut c_void) -> usize;
        fn IOSurfaceRelease(surface: *mut c_void);
    }

    pub fn import_from_mach_port(
        shared: &SharedDevice,
        port: mach_port_t,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Result<GpuImage, ResourceError> {
        let surface_ref = unsafe { IOSurfaceLookupFromMachPort(port) };
        if surface_ref.is_null() {
            return Err(ResourceError::ImportFailed(
                "IOSurfaceLookupFromMachPort returned null — invalid or expired Mach port",
            ));
        }

        let width = unsafe { IOSurfaceGetWidth(surface_ref) } as u32;
        let height = unsafe { IOSurfaceGetHeight(surface_ref) } as u32;
        if width == 0 || height == 0 {
            unsafe { IOSurfaceRelease(surface_ref) };
            return Err(ResourceError::ImportFailed("IOSurface reported zero dimensions"));
        }

        let mut import_info = vk::ImportMetalIOSurfaceInfoEXT::default().io_surface(surface_ref as *mut _);

        let image_info = vk::ImageCreateInfo::default()
            .push_next(&mut import_info)
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { shared.device.create_image(&image_info, None) };
        unsafe { IOSurfaceRelease(surface_ref) };
        let image = image?;

        let view = make_view(shared, image, format)?;

        Ok(GpuImage {
            image,
            memory: None,
            view,
            extent: vk::Extent2D { width, height },
            format,
            owns_memory: false,
        })
    }
}