use crate::vulkan_backend::swapchain::SharedDevice;
use ash::vk;
use objc2::{class, declare_class, msg_send, mutability, rc::Id, runtime::NSObject, AnyThread, ClassType, DeclaredClass};
use std::sync::Arc;

declare_class!(
    pub struct MTLDevice;

    unsafe impl ClassType for MTLDevice {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "MTLDevice";
    }

    impl DeclaredClass for MTLDevice {}

    unsafe impl MTLDevice {
        #[method(newCommandQueue)]
        fn new_command_queue(&self) -> Option<Id<NSObject>> {
            MTLCommandQueue::new_with_device(self).map(|q| unsafe { Id::cast(q) })
        }
    }
);

declare_class!(
    pub struct MTLCommandQueue;

    unsafe impl ClassType for MTLCommandQueue {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "MTLCommandQueue";
    }

    impl DeclaredClass for MTLCommandQueue {}

    unsafe impl MTLCommandQueue {
        #[method(commandBuffer)]
        fn command_buffer(&self) -> Option<Id<NSObject>> {
            MTLCommandBuffer::new_with_queue(self).map(|c| unsafe { Id::cast(c) })
        }
    }
);

impl MTLCommandQueue {
    pub fn new_with_device(_device: &MTLDevice) -> Option<Id<Self>> {
        let obj: Option<Id<Self>> = unsafe { msg_send![Self::alloc(), init] };
        obj
    }
}

declare_class!(
    pub struct MTLCommandBuffer;

    unsafe impl ClassType for MTLCommandBuffer {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "MTLCommandBuffer";
    }

    impl DeclaredClass for MTLCommandBuffer {}

    unsafe impl MTLCommandBuffer {
        #[method(renderCommandEncoderWithDescriptor:)]
        fn render_command_encoder(&self, _descriptor: &NSObject) -> Option<Id<NSObject>> {
            MTLRenderCommandEncoder::new_with_command_buffer(self).map(|e| unsafe { Id::cast(e) })
        }

        #[method(commit)]
        fn commit(&self) {
            if let Some(shared) = unsafe { G_SHARED_DEVICE.as_ref() } {
                let fence = vk::Fence::null();
                let submit_info = vk::SubmitInfo::default();
                unsafe {
                    let _ = shared.device.queue_submit(shared.graphics_queue, &[submit_info], fence);
                }
            }
        }
    }
);

impl MTLCommandBuffer {
    pub fn new_with_queue(_queue: &MTLCommandQueue) -> Option<Id<Self>> {
        let obj: Option<Id<Self>> = unsafe { msg_send![Self::alloc(), init] };
        obj
    }
}

declare_class!(
    pub struct MTLRenderCommandEncoder;

    unsafe impl ClassType for MTLRenderCommandEncoder {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "MTLRenderCommandEncoder";
    }

    impl DeclaredClass for MTLRenderCommandEncoder {}

    unsafe impl MTLRenderCommandEncoder {
        #[method(setRenderPipelineState:)]
        fn set_render_pipeline_state(&self, _state: &NSObject) {}

        #[method(setVertexBuffer:offset:atIndex:)]
        fn set_vertex_buffer(&self, _buffer: &NSObject, _offset: usize, _index: usize) {}

        #[method(setFragmentBuffer:offset:atIndex:)]
        fn set_fragment_buffer(&self, _buffer: &NSObject, _offset: usize, _index: usize) {}

        #[method(drawPrimitives:vertexStart:vertexCount:)]
        fn draw_primitives(&self, _type: u32, _start: usize, _count: usize) {
            if let Some(shared) = unsafe { G_SHARED_DEVICE.as_ref() } {
                let cmd_buf = vk::CommandBuffer::null();
                unsafe {
                    shared.device.cmd_draw(cmd_buf, _count as u32, 1, _start as u32, 0);
                }
            }
        }

        #[method(endEncoding)]
        fn end_encoding(&self) {}
    }
);

impl MTLRenderCommandEncoder {
    pub fn new_with_command_buffer(_cb: &MTLCommandBuffer) -> Option<Id<Self>> {
        let obj: Option<Id<Self>> = unsafe { msg_send![Self::alloc(), init] };
        obj
    }
}

pub static mut G_SHARED_DEVICE: Option<Arc<SharedDevice>> = None;

pub fn set_global_shared_device(shared: Arc<SharedDevice>) {
    unsafe {
        G_SHARED_DEVICE = Some(shared);
    }
}

#[no_mangle]
pub extern "C" fn MTLCreateSystemDefaultDevice() -> *mut NSObject {
    let device: Option<Id<MTLDevice>> = unsafe { msg_send![class!(MTLDevice), alloc] };
    if let Some(d) = device {
        let res: Id<MTLDevice> = unsafe { msg_send![d, init] };
        Id::as_ptr(&res) as *mut NSObject
    } else {
        std::ptr::null_mut()
    }
}
