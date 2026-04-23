#![deny(unsafe_code)]

//! Veil — thin winit shell.
//!
//! Creates a window, runs the winit event loop, and wires together the core
//! subsystems (`AppState`, `Channels`, `ShutdownSignal`, `KeybindingRegistry`,
//! `FocusManager`). All real logic lives in veil-core; this file is the minimal
//! platform glue.

mod action_dispatch;
mod aggregator_actor;
mod bootstrap;
mod config_reload;
mod egui_integration;
#[allow(dead_code)]
mod font;
#[allow(dead_code)]
mod font_pipeline;
mod frame;
mod key_translation;
mod quad_builder;
mod renderer;
mod sidebar_wiring;
mod socket_actor;
#[allow(dead_code)]
mod terminal_map;
mod vertex;

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use veil_core::config::{AppConfig, ConfigEvent, ConfigWatcher};
use veil_core::focus::{route_key_event, FocusManager, KeyRoute};
use veil_core::keyboard::{self, KeybindingRegistry};
use veil_core::lifecycle::ShutdownSignal;
use veil_core::message::{Channels, StateUpdate};
use veil_core::state::AppState;

use crate::action_dispatch::ActionEffect;
use crate::bootstrap::init_default_workspace;
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
    channels: Channels,
    /// Shutdown coordinator.
    shutdown: ShutdownSignal,
    /// Keybinding registry with default shortcuts.
    keybindings: KeybindingRegistry,
    /// Keyboard focus tracker.
    focus: FocusManager,
    /// Current modifier state (updated by `ModifiersChanged` events).
    current_modifiers: keyboard::Modifiers,
    /// Current window size in physical pixels.
    window_size: (u32, u32),
    /// PTY manager -- owns all active PTY instances.
    pty_manager: Option<veil_pty::PtyManager>,
    /// The current application config (loaded at startup, updated on reload).
    app_config: AppConfig,
    /// Path to the config file, if one was found.
    config_path: Option<std::path::PathBuf>,
    /// Config file watcher for hot-reload. Started after window creation.
    config_watcher: Option<ConfigWatcher>,
    /// Aggregator background actor handle.
    aggregator_handle: Option<aggregator_actor::AggregatorHandle>,
    /// Socket server background actor handle.
    socket_handle: Option<socket_actor::SocketHandle>,
}

impl VeilApp {
    fn new() -> Self {
        let (config, config_path) = veil_core::config::load_or_default();

        let mut app_state = AppState::new();
        app_state.apply_config(&config);

        let mut keybindings = KeybindingRegistry::with_defaults();
        let kb_warnings = keyboard::apply_keybindings_config(&mut keybindings, &config.keybindings);
        for w in &kb_warnings {
            tracing::warn!("keybinding config warning: {w}");
        }

        if let Some(ref path) = config_path {
            tracing::info!("loaded config from {}", path.display());
        } else {
            tracing::info!("no config file found, using defaults");
        }

        Self {
            window: None,
            renderer: None,
            app_state,
            channels: Channels::new(256),
            shutdown: ShutdownSignal::new(),
            keybindings,
            focus: FocusManager::new(),
            current_modifiers: keyboard::Modifiers::default(),
            window_size: (1280, 800),
            pty_manager: None,
            app_config: config,
            config_path,
            config_watcher: None,
            aggregator_handle: None,
            socket_handle: None,
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

        // Bootstrap default workspace and focus.
        let surface_id = init_default_workspace(&mut self.app_state, &mut self.focus);

        // Create PTY manager and spawn shell for the root pane.
        let mut pty_manager =
            veil_pty::PtyManager::new(self.channels.state_tx.clone(), self.shutdown.handle());
        let cwd = self
            .app_state
            .active_workspace()
            .expect("just created workspace")
            .working_directory
            .clone();
        if let Err(e) = pty_manager.spawn(surface_id, default_pty_config(cwd)) {
            tracing::error!("failed to spawn initial PTY: {e}");
        }
        self.pty_manager = Some(pty_manager);

        self.start_config_watcher();

        // Start aggregator actor (session discovery + file watching).
        self.aggregator_handle = Some(aggregator_actor::start_aggregator(
            self.channels.state_tx.clone(),
            self.channels.command_subscriber(),
            self.shutdown.handle(),
        ));

        // Start socket server actor with its own AppState snapshot.
        let socket_state = Arc::new(tokio::sync::Mutex::new(AppState::new()));
        {
            let mut ss = socket_state.blocking_lock();
            ss.apply_config(&self.app_config);
        }
        self.socket_handle =
            Some(socket_actor::start_socket_server(socket_state, self.shutdown.handle()));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Forward events to egui for sidebar input handling.
        if let (Some(renderer), Some(window)) = (&mut self.renderer, self.window.as_ref()) {
            let _ = renderer.egui.on_window_event(window, &event);
        }

        match event {
            WindowEvent::CloseRequested => {
                if let Some(ref mut mgr) = self.pty_manager {
                    mgr.shutdown_all();
                }
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
                self.handle_redraw(event_loop);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.current_modifiers = key_translation::translate_modifiers(new_modifiers);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                if let Some(key_input) =
                    key_translation::translate_key_event(&event, self.current_modifiers)
                {
                    let route = route_key_event(&key_input, &self.keybindings, &self.focus);
                    match route {
                        KeyRoute::Action(action) => {
                            let effects = action_dispatch::dispatch_action(
                                &action,
                                &mut self.app_state,
                                &mut self.focus,
                                self.window_size.0,
                                self.window_size.1,
                            );
                            for effect in effects {
                                self.execute_effect(effect);
                            }
                        }
                        KeyRoute::ForwardToSurface(surface_id) => {
                            if let Some(bytes) =
                                key_translation::encode_key_for_pty(&event, self.current_modifiers)
                            {
                                if let Some(ref mgr) = self.pty_manager {
                                    if let Err(e) = mgr.write(surface_id, bytes) {
                                        tracing::warn!(?surface_id, "PTY write failed: {e}");
                                    }
                                }
                            }
                        }
                        // Sidebar keyboard navigation (j/k, arrows) is a
                        // future enhancement. Mouse interactions are handled
                        // by egui's event system during RedrawRequested.
                        KeyRoute::ForwardToSidebar | KeyRoute::Unhandled => {}
                    }
                }
            }
            _ => {}
        }
    }
}

impl VeilApp {
    /// Start the config file watcher if a config path was discovered at startup.
    ///
    /// Spawns a bridge thread that converts `ConfigEvent`s from the watcher into
    /// `StateUpdate::ConfigReloaded` messages on `Channels.state_tx`.
    fn start_config_watcher(&mut self) {
        let Some(ref config_path) = self.config_path else {
            return;
        };

        let (config_event_tx, mut config_event_rx) = tokio::sync::mpsc::channel::<ConfigEvent>(16);

        match ConfigWatcher::new(config_path.clone(), self.app_config.clone(), config_event_tx) {
            Ok(mut watcher) => {
                if let Err(e) = watcher.start(self.shutdown.handle()) {
                    tracing::warn!("failed to start config watcher: {e}");
                } else {
                    tracing::info!("config watcher started for {}", config_path.display());
                    self.config_watcher = Some(watcher);
                }
            }
            Err(e) => {
                tracing::warn!("failed to create config watcher: {e}");
            }
        }

        // Bridge: forward ConfigEvents to state_tx as StateUpdate::ConfigReloaded.
        let state_tx = self.channels.state_tx.clone();
        std::thread::spawn(move || {
            while let Some(event) = config_event_rx.blocking_recv() {
                let update = match event {
                    ConfigEvent::Reloaded { config, delta, warnings } => {
                        StateUpdate::ConfigReloaded { config, delta, warnings }
                    }
                    ConfigEvent::Error(e) => {
                        tracing::warn!("config reload error: {e}");
                        continue;
                    }
                };
                if state_tx.blocking_send(update).is_err() {
                    break; // receiver dropped
                }
            }
        });
    }

    /// Drain pending state updates from the channel and apply them.
    fn drain_state_updates(&mut self) {
        while let Ok(update) = self.channels.state_rx.try_recv() {
            match update {
                StateUpdate::ConfigReloaded { config, delta, warnings } => {
                    self.handle_config_reloaded(config, &delta, &warnings);
                }
                StateUpdate::ConversationsUpdated(sessions) => {
                    tracing::info!(count = sessions.len(), "conversations updated");
                    self.app_state.update_conversations(sessions);
                    self.request_redraw();
                }
                _ => {
                    // Other StateUpdate variants (PtyOutput, SurfaceExited, etc.)
                    // will be handled in future wiring tasks.
                }
            }
        }
    }

    /// Apply a config reload: update state, keybindings, and log notable changes.
    fn handle_config_reloaded(
        &mut self,
        config: Box<veil_core::config::AppConfig>,
        delta: &veil_core::config::ConfigDelta,
        warnings: &[veil_core::config::ConfigWarning],
    ) {
        for w in warnings {
            tracing::warn!("config validation: {}: {}", w.field, w.message);
        }

        let needs_redraw = config_reload::apply_config_reload(
            &config,
            delta,
            &mut self.app_state,
            &mut self.keybindings,
        );

        if delta.font_changed {
            tracing::info!(
                "font config changed (family={:?}, size={}) -- \
                 font re-init will apply when font pipeline is wired",
                config.terminal.font_family,
                config.terminal.font_size,
            );
        }

        if delta.theme_changed {
            tracing::info!("theme changed to {:?}", config.general.theme);
        }

        self.app_config = *config;

        if needs_redraw {
            self.request_redraw();
        }
    }

    /// Request a window redraw if the window exists.
    fn request_redraw(&self) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }

    /// Run a single frame: build geometry, execute sidebar UI, render, request next frame.
    fn handle_redraw(&mut self, event_loop: &ActiveEventLoop) {
        self.drain_state_updates();

        let frame_geometry = build_frame_geometry(
            &self.app_state,
            &self.focus,
            self.window_size.0,
            self.window_size.1,
        );

        // Run egui sidebar frame and collect output for GPU rendering.
        let egui_output = self.run_sidebar_frame();

        if let Some(renderer) = &mut self.renderer {
            match renderer.render(&frame_geometry, egui_output) {
                Ok(()) => {}
                Err(e) => {
                    tracing::error!("render error: {e}");
                    event_loop.exit();
                }
            }
        }
        self.request_redraw();
    }

    /// Execute the egui sidebar frame and apply interactions.
    ///
    /// Returns `Some(FullOutput)` when the sidebar is visible (for GPU rendering),
    /// or `None` when hidden.
    fn run_sidebar_frame(&mut self) -> Option<egui::FullOutput> {
        if !self.app_state.sidebar.visible {
            return None;
        }
        let (Some(renderer), Some(window)) = (&mut self.renderer, self.window.as_ref()) else {
            return None;
        };

        let raw_input = renderer.egui.take_raw_input(window);

        let mut sidebar_response = veil_ui::sidebar::SidebarResponse::default();
        let full_output = renderer.egui.ctx.run_ui(raw_input, |ui| {
            sidebar_response = veil_ui::sidebar::render_sidebar(ui, &self.app_state);
        });

        if let Err(e) = sidebar_wiring::apply_sidebar_response(
            &sidebar_response,
            &mut self.app_state,
            &mut self.focus,
        ) {
            tracing::warn!("sidebar response error: {e}");
        }

        renderer.egui.handle_platform_output(window, full_output.platform_output.clone());

        Some(full_output)
    }

    fn execute_effect(&mut self, effect: ActionEffect) {
        match effect {
            ActionEffect::SpawnPty { surface_id, working_directory } => {
                if let Some(ref mut mgr) = self.pty_manager {
                    if let Err(e) = mgr.spawn(surface_id, default_pty_config(working_directory)) {
                        tracing::error!(?surface_id, "failed to spawn PTY: {e}");
                    }
                }
            }
            ActionEffect::ClosePty { surface_id } => {
                if let Some(ref mut mgr) = self.pty_manager {
                    if let Err(e) = mgr.close(surface_id) {
                        tracing::warn!(?surface_id, "failed to close PTY: {e}");
                    }
                }
            }
            ActionEffect::Redraw => {
                self.request_redraw();
            }
        }
    }
}

/// Build a `PtyConfig` with the default shell and the given working directory.
fn default_pty_config(working_directory: std::path::PathBuf) -> veil_pty::PtyConfig {
    veil_pty::PtyConfig {
        command: None,
        args: vec![],
        working_directory: Some(working_directory),
        env: vec![],
        size: veil_pty::PtySize::default(),
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
