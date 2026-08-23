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

    impl DeclaredClass for MTLDevice {}

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

    impl DeclaredClass for MTLCommandQueue {}

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

    impl DeclaredClass for MTLCommandBuffer {}

    unsafe impl MTLCommandBuffer {
        #[method(initWithQueue:)]
        fn
