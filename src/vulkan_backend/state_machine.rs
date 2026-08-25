use ash::khr::{surface, swapchain};
use ash::vk;
use ash::{Device, Instance};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use thiserror::Error;

pub const MAX_FRAMES_IN_FLIGHT: usize = 3;

#[derive(Error, Debug)]
pub enum SwapchainError {
    #[error("vulkan error: {0}")]
    Vk(#[from] vk::Result),
    #[error("no compatible surface formats")]
    NoSurfaceFormats,
    #[error("no compatible present modes")]
    NoPresentModes,
    #[error("zero-sized swapchain extent requested")]
    ZeroExtent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowRenderState {
    Uninitialized,
    Ready,
    Recreating,
    Lost,
}

pub trait MonitorProvider: Send + Sync {
    fn refresh_rate_for_window(&self, window_x: f64, window_y: f64, window_w: f64, window_h: f64) -> f32;
}

pub struct PendingResize {
    flag: AtomicBool,
    width: AtomicU32,
    height: AtomicU32,
}

impl Default for PendingResize {
    fn default() -> Self {
        Self {
            flag: AtomicBool::new(false),
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
        }
    }
}

impl PendingResize {
    pub fn request(&self, width: u32, height: u32) {
        self.width.store(width, Ordering::Release);
        self.height.store(height, Ordering::Release);
        self.flag.store(true, Ordering::Release);
    }

    pub fn take(&self) -> Option<(u32, u32)> {
        if self.flag.swap(false, Ordering::AcqRel) {
            Some((
                self.width.load(Ordering::Acquire),
                self.height.load(Ordering::Acquire),
            ))
        } else {
            None
        }
    }
}

pub struct SharedDevice {
    pub instance: Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: Device,
    pub graphics_queue: vk::Queue,
    pub graphics_queue_family: u32,
    pub transfer_queue: vk::Queue,
    pub transfer_queue_family: u32,
    pub surface_ext: surface::Instance,
    pub swapchain_ext: swapchain::Device,
    pub command_pool: vk::CommandPool,
}

impl Drop for SharedDevice {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

struct FrameSync {
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    in_flight: vk::Fence,
}

pub enum TickOutcome {
    Recreated,
    Acquired {
        image_index: u32,
        command_buffer: vk::CommandBuffer,
        framebuffer: vk::Framebuffer,
        render_pass: vk::RenderPass,
        extent: vk::Extent2D,
    },
}

pub struct WindowRenderContext {
    shared: Arc<SharedDevice>,
    surface: vk::SurfaceKHR,
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    render_pass: vk::RenderPass,
    command_buffers: Vec<vk::CommandBuffer>,
    frames: Vec<FrameSync>,
    extent: vk::Extent2D,
    format: vk::Format,
    current_frame: usize,
    state: WindowRenderState,
    pub pending_resize: PendingResize,
    monitor: Option<Arc<dyn MonitorProvider>>,
}

impl WindowRenderContext {
    pub fn new(
        shared: Arc<SharedDevice>,
        surface: vk::SurfaceKHR,
        initial_width: u32,
        initial_height: u32,
        monitor: Option<Arc<dyn MonitorProvider>>,
    ) -> Result<Self, SwapchainError> {
        let mut ctx = Self {
            shared,
            surface,
            swapchain: vk::SwapchainKHR::null(),
            images: Vec::new(),
            image_views: Vec::new(),
            framebuffers: Vec::new(),
            render_pass: vk::RenderPass::null(),
            command_buffers: Vec::new(),
            frames: Vec::new(),
            extent: vk::Extent2D {
                width: initial_width,
                height: initial_height,
            },
            format: vk::Format::UNDEFINED,
            current_frame: 0,
            state: WindowRenderState::Uninitialized,
            pending_resize: PendingResize::default(),
            monitor,
        };
        ctx.build_swapchain(initial_width, initial_height)?;
        ctx.state = WindowRenderState::Ready;
        Ok(ctx)
    }

    fn build_swapchain(&mut self, want_w: u32, want_h: u32) -> Result<(), SwapchainError> {
        unsafe {
            self.destroy_swapchain_resources();
        }

        let caps = unsafe {
            self.shared
                .surface_ext
                .get_physical_device_surface_capabilities(self.shared.physical_device, self.surface)?
        };
        let formats = unsafe {
            self.shared
                .surface_ext
                .get_physical_device_surface_formats(self.shared.physical_device, self.surface)?
        };
        if formats.is_empty() {
            return Err(SwapchainError::NoSurfaceFormats);
        }
        let chosen = formats
            .iter()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_SRGB
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .or_else(|| formats.iter().find(|f| f.format == vk::Format::B8G8R8A8_UNORM))
            .copied()
            .unwrap_or(formats[0]);

        let present_modes = unsafe {
            self.shared
                .surface_ext
                .get_physical_device_surface_present_modes(self.shared.physical_device, self.surface)?
        };
        if present_modes.is_empty() {
            return Err(SwapchainError::NoPresentModes);
        }
        let present_mode = present_modes
            .iter()
            .copied()
            .find(|&m| m == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            vk::Extent2D {
                width: want_w.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: want_h.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        if extent.width == 0 || extent.height == 0 {
            return Err(SwapchainError::ZeroExtent);
        }

        let mut image_count = caps.min_image_count + 1;
        if caps.max_image_count > 0 {
            image_count = image_count.min(caps.max_image_count);
        }
        image_count = image_count.min(MAX_FRAMES_IN_FLIGHT as u32);

        let old_swapchain = self.swapchain;
        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(chosen.format)
            .image_color_space(chosen.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old_swapchain);

        let new_swapchain = unsafe { self.shared.swapchain_ext.create_swapchain(&create_info, None)? };
        if old_swapchain != vk::SwapchainKHR::null() {
            unsafe {
                self.shared.swapchain_ext.destroy_swapchain(old_swapchain, None);
            }
        }
        self.swapchain = new_swapchain;
        self.extent = extent;
        self.format = chosen.format;

        self.images = unsafe { self.shared.swapchain_ext.get_swapchain_images(self.swapchain)? };

        self.image_views = self
            .images
            .iter()
            .map(|&image| {
                let view_info = vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(self.format)
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
                unsafe { self.shared.device.create_image_view(&view_info, None) }
            })
            .collect::<Result<Vec<_>, _>>()?;

        let color_attachment = vk::AttachmentDescription::default()
            .format(self.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
        let color_ref = vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        let color_refs = [color_ref];
        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs);
        let dependency = vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
        let attachments = [color_attachment];
        let subpasses = [subpass];
        let dependencies = [dependency];
        let rp_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);
        self.render_pass = unsafe { self.shared.device.create_render_pass(&rp_info, None)? };

        self.framebuffers = self
            .image_views
            .iter()
            .map(|&view| {
                let view_arr = [view];
                let fb_info = vk::FramebufferCreateInfo::default()
                    .render_pass(self.render_pass)
                    .attachments(&view_arr)
                    .width(extent.width)
                    .height(extent.height)
                    .layers(1);
                unsafe { self.shared.device.create_framebuffer(&fb_info, None) }
            })
            .collect::<Result<Vec<_>, _>>()?;

        let cb_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.shared.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(self.images.len() as u32);
        self.command_buffers = unsafe { self.shared.device.allocate_command_buffers(&cb_info)? };

        if self.frames.is_empty() {
            let sem_info = vk::SemaphoreCreateInfo::default();
            let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            let mut frames = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
            for _ in 0..MAX_FRAMES_IN_FLIGHT {
                frames.push(FrameSync {
                    image_available: unsafe { self.shared.device.create_semaphore(&sem_info, None)? },
                    render_finished: unsafe { self.shared.device.create_semaphore(&sem_info, None)? },
                    in_flight: unsafe { self.shared.device.create_fence(&fence_info, None)? },
                });
            }
            self.frames = frames;
        }

        Ok(())
    }

    unsafe fn destroy_swapchain_resources(&mut self) {
        let _ = self.shared.device.device_wait_idle();
        if !self.command_buffers.is_empty() {
            self.shared
                .device
                .free_command_buffers(self.shared.command_pool, &self.command_buffers);
            self.command_buffers.clear();
        }
        for &fb in &self.framebuffers {
            self.shared.device.destroy_framebuffer(fb, None);
        }
        self.framebuffers.clear();
        if self.render_pass != vk::RenderPass::null() {
            self.shared.device.destroy_render_pass(self.render_pass, None);
            self.render_pass = vk::RenderPass::null();
        }
        for &view in &self.image_views {
            self.shared.device.destroy_image_view(view, None);
        }
        self.image_views.clear();
        self.images.clear();
    }

    pub fn request_resize(&self, width: u32, height: u32) {
        self.pending_resize.request(width, height);
    }

    pub fn tick(&mut self) -> Result<TickOutcome, SwapchainError> {
        if let Some((w, h)) = self.pending_resize.take() {
            self.state = WindowRenderState::Recreating;
            self.build_swapchain(w, h)?;
            self.state = WindowRenderState::Ready;
            return Ok(TickOutcome::Recreated);
        }

        let frame_idx = self.current_frame;
        let in_flight = self.frames[frame_idx].in_flight;
        let image_available = self.frames[frame_idx].image_available;

        unsafe {
            self.shared.device.wait_for_fences(&[in_flight], true, u64::MAX)?;
        }

        let acquire = unsafe {
            self.shared.swapchain_ext.acquire_next_image(
                self.swapchain,
                u64::MAX,
                image_available,
                vk::Fence::null(),
            )
        };

        let image_index = match acquire {
            Ok((idx, false)) => idx,
            Ok((_, true)) => {
                let (w, h) = (self.extent.width, self.extent.height);
                self.state = WindowRenderState::Recreating;
                self.build_swapchain(w, h)?;
                self.state = WindowRenderState::Ready;
                return Ok(TickOutcome::Recreated);
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                let (w, h) = (self.extent.width, self.extent.height);
                self.state = WindowRenderState::Recreating;
                self.build_swapchain(w, h)?;
                self.state = WindowRenderState::Ready;
                return Ok(TickOutcome::Recreated);
            }
            Err(e) => return Err(e.into()),
        };

        unsafe {
            self.shared.device.reset_fences(&[in_flight])?;
        }

        Ok(TickOutcome::Acquired {
            image_index,
            command_buffer: self.command_buffers[image_index as usize],
            framebuffer: self.framebuffers[image_index as usize],
            render_pass: self.render_pass,
            extent: self.extent,
        })
    }

    pub fn submit_and_present(&mut self, image_index: u32) -> Result<(), SwapchainError> {
        let frame_idx = self.current_frame;
        let image_available = self.frames[frame_idx].image_available;
        let render_finished = self.frames[frame_idx].render_finished;
        let in_flight = self.frames[frame_idx].in_flight;
        let cb = self.command_buffers[image_index as usize];

        let wait_semaphores = [image_available];
        let signal_semaphores = [render_finished];
        let stage_mask = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let cbs = [cb];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&stage_mask)
            .command_buffers(&cbs)
            .signal_semaphores(&signal_semaphores);

        unsafe {
            self.shared
                .device
                .queue_submit(self.shared.graphics_queue, &[submit_info], in_flight)?;
        }

        let swapchains = [self.swapchain];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let present_result = unsafe {
            self.shared
                .swapchain_ext
                .queue_present(self.shared.graphics_queue, &present_info)
        };

        self.current_frame = (self.current_frame + 1) % self.frames.len();

        match present_result {
            Ok(_) => Ok(()),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => {
                let (w, h) = (self.extent.width, self.extent.height);
                self.build_swapchain(w, h)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn state(&self) -> WindowRenderState {
        self.state
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.extent
    }

    pub fn refresh_rate(&self, window_x: f64, window_y: f64, window_w: f64, window_h: f64) -> f32 {
        self.monitor
            .as_ref()
            .map(|m| m.refresh_rate_for_window(window_x, window_y, window_w, window_h))
            .unwrap_or(60.0)
    }
}

impl Drop for WindowRenderContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.shared.device.device_wait_idle();
            self.destroy_swapchain_resources();
            if self.swapchain != vk::SwapchainKHR::null() {
                self.shared.swapchain_ext.destroy_swapchain(self.swapchain, None);
            }
            for frame in &self.frames {
                self.shared.device.destroy_semaphore(frame.image_available, None);
                self.shared.device.destroy_semaphore(frame.render_finished, None);
                self.shared.device.destroy_fence(frame.in_flight, None);
            }
            self.shared.surface_ext.destroy_surface(self.surface, None);
        }
    }
}