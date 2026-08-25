use crate::vulkan_backend::swapchain::WindowRenderContext;
use ash::vk;
use objc2::{declare_class, msg_send, mutability, rc::Id, runtime::NSObject, DeclaredClass};
use objc2_foundation::NSRect;
use std::sync::{Arc, Mutex};

declare_class!(
    pub struct CAMetalLayer;

    unsafe impl ClassType for CAMetalLayer {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "CAMetalLayer";
    }

    unsafe impl DeclaredClass for CAMetalLayer {}

    unsafe impl CAMetalLayer {
        #[method(init)]
        fn init(this: &mut Self) -> Option<Id<Self>> {
            let this: Option<Id<Self>> = unsafe { msg_send![super(this), init] };
            this
        }

        #[method(setDrawableSize:)]
        fn set_drawable_size(&self, size: vk::Extent2D) {
            if let Some(ctx) = unsafe { RUST_RENDER_CONTEXT.lock().unwrap().as_mut() } {
                ctx.request_resize(size.width, size.height);
            }
        }

        #[method(nextDrawable)]
        fn next_drawable(&self) -> Option<Id<NSObject>> {
            let mut lock = unsafe { RUST_RENDER_CONTEXT.lock().unwrap() };
            if let Some(ctx) = lock.as_mut() {
                if let Ok(outcome) = ctx.tick() {
                    match outcome {
                        crate::vulkan_backend::swapchain::TickOutcome::Acquired { image_index, .. } => {
                            let drawable: Option<Id<NSObject>> = unsafe { msg_send![class!(CAMetalDrawable), alloc] };
                            if let Some(d) = drawable {
                                unsafe {
                                    let _: () = msg_send![&d, initWithLayer: self textureIndex: image_index];
                                }
                                return Some(d);
                            }
                        }
                        _ => {}
                    }
                }
            }
            None
        }
    }
);

declare_class!(
    pub struct CAMetalDrawable;

    unsafe impl ClassType for CAMetalDrawable {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "CAMetalDrawable";
    }

    unsafe impl DeclaredClass for CAMetalDrawable {}

    unsafe impl CAMetalDrawable {
        #[method(initWithLayer:textureIndex:)]
        fn init_with_layer(&mut self, _layer: &CAMetalLayer, texture_index: u32) -> Option<Id<Self>> {
            let this: Option<Id<Self>> = unsafe { msg_send![super(this), init] };
            if this.is_some() {
                unsafe {
                    let ptr = this.as_ref().unwrap() as *const Self as *mut u32;
                    *ptr.add(4) = texture_index;
                }
            }
            this
        }

        #[method(present)]
        fn present(&self) {
            unsafe {
                let ptr = self as *const Self as *mut u32;
                let texture_index = *ptr.add(4);
                if let Some(ctx) = RUST_RENDER_CONTEXT.lock().unwrap().as_mut() {
                    let _ = ctx.submit_and_present(texture_index);
                }
            }
        }
    }
);

static mut RUST_RENDER_CONTEXT: Mutex<Option<WindowRenderContext>> = Mutex::new(None);

pub fn set_global_render_context(ctx: WindowRenderContext) {
    unsafe {
        *RUST_RENDER_CONTEXT.lock().unwrap() = Some(ctx);
    }
}
