#![deny(unsafe_code)]

//! Veil — thin winit shell.
//!
//! Creates a window, runs the winit event loop, and wires together the core
//! subsystems (`AppState`, `Channels`, `ShutdownSignal`, `KeybindingRegistry`,
//! `FocusManager`). All real logic lives in veil-core; this file is the minimal
//! platform glue.

#[allow(dead_code)]
mod font;
mod frame;
mod quad_builder;
mod renderer;
#[allow(dead_code)]
mod sidebar_wiring;
mod vertex;

use std::sync::Arc;

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

use crate::frame::build_frame_geometry;
use crate::renderer::Renderer;

/// The main application struct that owns all state and implements the winit event loop.
struct VeilApp {
    /// The winit window, created in `resumed`.
    window: Option<Arc<Window>>,
    /// GPU renderer, created in `resumed`.
    renderer: Option<Renderer>,
    /// Central application state (drives frame geometry).
    app_state: AppState,
    /// Channel infrastructure for actor communication.
    _channels: Channels,
    /// Shutdown coordinator.
    shutdown: ShutdownSignal,
    /// Keybinding registry with default shortcuts.
    _keybindings: KeybindingRegistry,
    /// Keyboard focus tracker.
    focus: FocusManager,
    /// Current window size in physical pixels.
    window_size: (u32, u32),
}

impl VeilApp {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            app_state: AppState::new(),
            _channels: Channels::new(256),
            shutdown: ShutdownSignal::new(),
            _keybindings: KeybindingRegistry::with_defaults(),
            focus: FocusManager::new(),
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
        let window = Arc::new(event_loop.create_window(attrs).expect("failed to create window"));
        let renderer =
            pollster::block_on(Renderer::new(window.clone())).expect("failed to create renderer");
        self.renderer = Some(renderer);
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
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size.width, new_size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let frame_geometry = build_frame_geometry(
                    &self.app_state,
                    &self.focus,
                    self.window_size.0,
                    self.window_size.1,
                );
                if let Some(renderer) = &mut self.renderer {
                    match renderer.render(&frame_geometry) {
                        Ok(()) => {}
                        Err(e) => {
                            tracing::error!("render error: {e}");
                            event_loop.exit();
                        }
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() -> anyhow::Result<()> {
    let _tracing_guard = veil_tracing::init();

    tracing::info!("veil v{}", env!("CARGO_PKG_VERSION"));

    let event_loop = EventLoop::new()?;
    let mut app = VeilApp::new();
    event_loop.run_app(&mut app)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Stub functions for VEI-74 (app bootstrap).
// These compile but panic at runtime — the tests below are RED.
// ---------------------------------------------------------------------------

use std::path::PathBuf;
use veil_core::workspace::SurfaceId;

/// Bootstrap a default workspace and set focus to its root pane.
/// Returns the `SurfaceId` of the root pane for PTY spawning.
fn init_default_workspace(_app_state: &mut AppState, _focus: &mut FocusManager) -> SurfaceId {
    todo!("VEI-74: implement init_default_workspace")
}

/// Resolve the working directory for the initial workspace.
///
/// Prefers the process's current directory. Falls back to `$HOME`, then `/`.
fn resolve_startup_cwd() -> PathBuf {
    todo!("VEI-74: implement resolve_startup_cwd")
}

#[cfg(test)]
mod bootstrap_tests {
    use super::*;
    use std::path::PathBuf;

    use veil_core::focus::FocusManager;
    use veil_core::lifecycle::ShutdownSignal;
    use veil_core::message::Channels;
    use veil_core::state::AppState;
    use veil_pty::{PtyConfig, PtyManager, PtySize};

    // ================================================================
    // Unit 1: init_default_workspace
    // ================================================================

    #[test]
    fn init_creates_workspace() {
        let mut app_state = AppState::new();
        let mut focus = FocusManager::new();

        let _surface_id = init_default_workspace(&mut app_state, &mut focus);

        let ws = app_state.active_workspace().expect("active workspace should exist after init");
        assert_eq!(ws.name, "default");
    }

    #[test]
    fn init_sets_focus() {
        let mut app_state = AppState::new();
        let mut focus = FocusManager::new();

        let surface_id = init_default_workspace(&mut app_state, &mut focus);

        let focused = focus.focused_surface().expect("a surface should be focused after init");
        assert_eq!(focused, surface_id);
    }

    #[test]
    fn init_returns_root_surface() {
        let mut app_state = AppState::new();
        let mut focus = FocusManager::new();

        let surface_id = init_default_workspace(&mut app_state, &mut focus);

        let ws = app_state.active_workspace().expect("active workspace should exist");
        let root_surface = ws.layout.surface_ids()[0];
        assert_eq!(surface_id, root_surface);
    }

    #[test]
    fn init_workspace_has_single_pane() {
        let mut app_state = AppState::new();
        let mut focus = FocusManager::new();

        let _surface_id = init_default_workspace(&mut app_state, &mut focus);

        let ws = app_state.active_workspace().expect("active workspace should exist");
        assert_eq!(ws.layout.pane_count(), 1);
    }

    // ================================================================
    // Unit 3: resolve_startup_cwd
    // ================================================================

    #[test]
    fn resolve_cwd_returns_existing_dir() {
        let cwd = resolve_startup_cwd();
        assert!(cwd.exists(), "resolved cwd should exist: {cwd:?}");
        assert!(cwd.is_dir(), "resolved cwd should be a directory: {cwd:?}");
    }

    #[test]
    fn resolve_cwd_returns_non_empty_path() {
        let cwd = resolve_startup_cwd();
        assert_ne!(cwd, PathBuf::new(), "resolved cwd should not be empty");
    }

    // ================================================================
    // Unit 2: PTY integration
    // ================================================================

    #[test]
    fn pty_manager_can_spawn_for_surface() {
        // Create a real PtyManager (no mock factory available cross-crate).
        let channels = Channels::new(64);
        let shutdown = ShutdownSignal::new();
        let mut pty_manager = PtyManager::new(channels.state_tx.clone(), shutdown.handle());

        // Bootstrap workspace to get a valid SurfaceId.
        let mut app_state = AppState::new();
        let mut focus = FocusManager::new();
        let surface_id = init_default_workspace(&mut app_state, &mut focus);

        // Spawn a real shell for that surface.
        let config = PtyConfig {
            command: Some("/bin/sh".to_string()),
            args: vec![],
            working_directory: Some(PathBuf::from("/tmp")),
            env: vec![],
            size: PtySize::default(),
        };
        pty_manager.spawn(surface_id, config).expect("PTY spawn should succeed");

        assert_eq!(pty_manager.active_count(), 1);

        // Clean up: shut down immediately so the child process is reaped.
        pty_manager.shutdown_all();
        assert_eq!(pty_manager.active_count(), 0);
    }
}
