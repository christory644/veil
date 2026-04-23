//! App bootstrap — creates the default workspace and resolves the startup
//! working directory. Extracted from `main.rs` to keep the winit shell thin
//! and enable unit testing without a GPU or window.

use std::path::PathBuf;

use veil_core::focus::FocusManager;
use veil_core::state::AppState;
use veil_core::workspace::SurfaceId;

/// Bootstrap a default workspace and set focus to its root pane.
/// Returns the `SurfaceId` of the root pane for PTY spawning.
pub fn init_default_workspace(app_state: &mut AppState, focus: &mut FocusManager) -> SurfaceId {
    let cwd = resolve_startup_cwd();
    let ws_id = app_state.create_workspace("default".to_string(), cwd);
    let ws = app_state.workspace(ws_id).expect("just created workspace");
    let surface_id = ws.layout.surface_ids()[0];
    focus.focus_surface(surface_id);
    surface_id
}

/// Resolve the working directory for the initial workspace.
///
/// Prefers the process's current directory. Falls back to `$HOME`, then `/`.
fn resolve_startup_cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| {
        std::env::var("HOME").map_or_else(|_| PathBuf::from("/"), PathBuf::from)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use veil_core::lifecycle::ShutdownSignal;
    use veil_core::message::Channels;
    use veil_pty::{PtyConfig, PtyManager, PtySize};

    // ================================================================
    // init_default_workspace
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
    // resolve_startup_cwd
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
    // PTY integration
    // ================================================================

    #[test]
    fn pty_manager_can_spawn_for_surface() {
        let channels = Channels::new(64);
        let shutdown = ShutdownSignal::new();
        let mut pty_manager = PtyManager::new(channels.state_tx.clone(), shutdown.handle());

        let mut app_state = AppState::new();
        let mut focus = FocusManager::new();
        let surface_id = init_default_workspace(&mut app_state, &mut focus);

        let config = PtyConfig {
            command: Some("/bin/sh".to_string()),
            args: vec![],
            working_directory: Some(PathBuf::from("/tmp")),
            env: vec![],
            size: PtySize::default(),
        };
        pty_manager.spawn(surface_id, config).expect("PTY spawn should succeed");

        assert_eq!(pty_manager.active_count(), 1);

        pty_manager.shutdown_all();
        assert_eq!(pty_manager.active_count(), 0);
    }
}
