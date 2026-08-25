use crate::vulkan_backend::swapchain::SharedDevice;
use ash::vk;
use objc2::{declare_class, msg_send, mutability, rc::Id, runtime::NSObject, DeclaredClass};
use std::sync::Arc;

declare_class!(
    pub struct MTLDevice;

    unsafe impl ClassType for MTLDevice {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "MTLDevice";
    }

    unsafe impl DeclaredClass for MTLDevice {}

    unsafe impl MTLDevice {
        #[method(newCommandQueue)]
        fn new_command_queue(&self) -> Option<Id<NSObject>> {
            let queue: Option<Id<NSObject>> = unsafe { msg_send![class!(MTLCommandQueue), alloc] };
            if let Some(q) = queue {
                unsafe {
                    let _: () = msg_send![&q, initWithDevice: self];
                }
                return Some(q);
            }
            None
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

    unsafe impl DeclaredClass for MTLCommandQueue {}

    unsafe impl MTLCommandQueue {
        #[method(initWithDevice:)]
        fn init_with_device(&mut self, _device: &MTLDevice) -> Option<Id<Self>> {
            unsafe { msg_send![super(this), init] }
        }

        #[method(commandBuffer)]
        fn command_buffer(&self) -> Option<Id<NSObject>> {
            let cb: Option<Id<NSObject>> = unsafe { msg_send![class!(MTLCommandBuffer), alloc] };
            if let Some(c) = cb {
                unsafe {
                    let _: () = msg_send![&c, initWithQueue: self];
                }
                return Some(c);
            }
            None
        }
    }
);

declare_class!(
    pub struct MTLCommandBuffer;

    unsafe impl ClassType for MTLCommandBuffer {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "MTLCommandBuffer";
    }

    unsafe impl DeclaredClass for MTLCommandBuffer {}

    unsafe impl MTLCommandBuffer {
        #[method(initWithQueue:)]
        fn init_with_queue(&mut self, _queue: &MTLCommandQueue) -> Option<Id<Self>> {
            unsafe { msg_send![super(this), init] }
        }

        #[method(renderCommandEncoderWithDescriptor:)]
        fn render_command_encoder(&self, _descriptor: &NSObject) -> Option<Id<NSObject>> {
            let encoder: Option<Id<NSObject>> = unsafe { msg_send![class!(MTLRenderCommandEncoder), alloc] };
            if let Some(e) = encoder {
                unsafe {
                    let _: () = msg_send![&e, initWithCommandBuffer: self];
                }
                return Some(e);
            }
            None
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

declare_class!(
    pub struct MTLRenderCommandEncoder;

    unsafe impl ClassType for MTLRenderCommandEncoder {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "MTLRenderCommandEncoder";
    }

    unsafe impl DeclaredClass for MTLRenderCommandEncoder {}

    unsafe impl MTLRenderCommandEncoder {
        #[method(initWithCommandBuffer:)]
        fn init_with_command_buffer(&mut self, _cb: &MTLCommandBuffer) -> Option<Id<Self>> {
            unsafe { msg_send![super(this), init] }
        }

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

static mut G_SHARED_DEVICE: Option<Arc<SharedDevice>> = None;

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
