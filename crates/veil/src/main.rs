#![deny(unsafe_code)]

//! Veil — thin winit shell.
//!
//! Creates a window, runs the winit event loop, and wires together the core
//! subsystems (`AppState`, `Channels`, `ShutdownSignal`, `KeybindingRegistry`,
//! `FocusManager`). All real logic lives in veil-core; this file is the minimal
//! platform glue.

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use veil_core::focus::FocusManager;
use veil_core::keyboard::KeybindingRegistry;
use veil_core::lifecycle::ShutdownSignal;
use veil_core::message::Channels;
use veil_core::state::AppState;

/// The main application struct that owns all state and implements the winit event loop.
struct VeilApp {
    /// The winit window, created in `resumed`.
    window: Option<Window>,
    /// Central application state (used by future renderer).
    _app_state: AppState,
    /// Channel infrastructure for actor communication.
    _channels: Channels,
    /// Shutdown coordinator.
    shutdown: ShutdownSignal,
    /// Keybinding registry with default shortcuts.
    _keybindings: KeybindingRegistry,
    /// Keyboard focus tracker.
    _focus: FocusManager,
    /// Current window size in physical pixels.
    window_size: (u32, u32),
}

impl VeilApp {
    fn new() -> Self {
        Self {
            window: None,
            _app_state: AppState::new(),
            _channels: Channels::new(256),
            shutdown: ShutdownSignal::new(),
            _keybindings: KeybindingRegistry::with_defaults(),
            _focus: FocusManager::new(),
            window_size: (1280, 800),
        }
    }
}

impl ApplicationHandler for VeilApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let size = LogicalSize::new(1280.0_f64, 800.0_f64);
        let attrs = WindowAttributes::default().with_title("Veil").with_inner_size(size);
        let window = event_loop.create_window(attrs).expect("failed to create window");
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.shutdown.trigger();
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                self.window_size = (new_size.width, new_size.height);
            }
            WindowEvent::RedrawRequested => {
                // Future: wgpu rendering goes here.
                // For now, request another redraw to keep the loop alive.
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() -> anyhow::Result<()> {
    println!("veil v{}", env!("CARGO_PKG_VERSION"));

    let event_loop = EventLoop::new()?;
    let mut app = VeilApp::new();
    event_loop.run_app(&mut app)?;

    Ok(())
}
