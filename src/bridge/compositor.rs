use crate::platform::ipc::host::{ClientSink, CompositorHost, FrameSink, IncomingFrame};
use crate::platform::ipc::input_wire;
use crate::platform::shm::sync_ring::ShmSync;
use crate::vulkan_backend::resource::iosurface_import;
use mach2::port::mach_port_t;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct LoggingFrameSink;

impl FrameSink for LoggingFrameSink {
    fn on_frame(&self, frame: &IncomingFrame) {
        match iosurface_import::resolve_from_mach_port(
            frame.surface_port as mach_port_t,
            Some(frame.width),
            Some(frame.height),
        ) {
            Ok(resolved) => {
                println!(
                    "[QMV Compositor] Frame #{} verified. Token: {}. Resolution: {}x{}. Format: 0x{:X}",
                    frame.frame_index, frame.client_token, resolved.width, resolved.height, resolved.pixel_format
                );
            }
            Err(_) => {
                println!(
                    "[QMV Compositor] Frame #{} from token {} could not be resolved as an IOSurface",
                    frame.frame_index, frame.client_token
                );
            }
        }
    }
}

struct LoggingClientSink;

impl ClientSink for LoggingClientSink {
    fn on_client_changed(&self, client_token: u64, pid: i32, connected: bool) {
        let status = if connected { "Connected" } else { "Disconnected" };
        println!(
            "[QMV Compositor] Client process tracking status change -> PID: {pid}, Token: {client_token}, Status: {status}"
        );
    }
}

pub struct QmvServer {
    host: Arc<CompositorHost>,
    sync: ShmSync,
    running: Arc<AtomicBool>,
    poll_thread: Option<std::thread::JoinHandle<()>>,
}

impl QmvServer {
    pub fn start(service_name: &str, shm_name: &str, slots: u32) -> Self {
        let host = CompositorHost::create(service_name, Some(Box::new(LoggingFrameSink)), Some(Box::new(LoggingClientSink)))
            .expect("Failed to initialize Mach IPC host");
        let host = Arc::new(host);

        let sync = ShmSync::create(shm_name, slots).expect("Failed to initialize SHM barrier");

        let running = Arc::new(AtomicBool::new(true));
        let poll_host = Arc::clone(&host);
        let poll_running = Arc::clone(&running);
        let poll_thread = std::thread::spawn(move || {
            while poll_running.load(Ordering::Acquire) {
                let _ = poll_host.run_once(50);
            }
        });

        println!("[QMV Server] Core graphics pipeline and synchronization barriers deployment complete");

        QmvServer {
            host,
            sync,
            running,
            poll_thread: Some(poll_thread),
        }
    }

    pub fn host(&self) -> &Arc<CompositorHost> {
        &self.host
    }

    pub fn notify_input_ready(&self, reply_port: mach_port_t, token: u64, window_id: u64, x: f64, y: f64, dx: f64, dy: f64) -> Result<(), &'static str> {
        input_wire::send_mouse_moved(reply_port, token, window_id, x, y, dx, dy)
    }

    pub fn acquire_ready_slot(&self) -> Option<u32> {
        self.sync.acquire_read_slot()
    }

    pub fn release_slot(&self, slot_index: u32) -> bool {
        self.sync.release_read_slot(slot_index)
    }
}

impl Drop for QmvServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(handle) = self.poll_thread.take() {
            let _ = handle.join();
        }
    }
}
