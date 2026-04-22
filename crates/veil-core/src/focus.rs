//! Focus management — tracks which UI element has keyboard focus.
//!
//! Focus determines where key events are routed: to the keybinding registry
//! (global shortcuts), to the sidebar (navigation keys), or pass-through to
//! a terminal surface.

use crate::keyboard::{KeyAction, KeyInput, KeybindingRegistry};
use crate::workspace::SurfaceId;

/// Where keyboard focus currently lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    /// A terminal surface (keys pass through to PTY).
    Surface(SurfaceId),
    /// The sidebar (keys navigate the sidebar UI).
    Sidebar,
}

/// Manages keyboard focus state.
#[derive(Debug)]
pub struct FocusManager {
    current: Option<FocusTarget>,
}

impl FocusManager {
    /// Create a new `FocusManager` with no focus.
    pub fn new() -> Self {
        Self { current: None }
    }

    /// Get current focus target.
    pub fn current(&self) -> Option<FocusTarget> {
        self.current
    }

    /// Set focus to a terminal surface.
    pub fn focus_surface(&mut self, id: SurfaceId) {
        self.current = Some(FocusTarget::Surface(id));
    }

    /// Set focus to the sidebar.
    pub fn focus_sidebar(&mut self) {
        self.current = Some(FocusTarget::Sidebar);
    }

    /// True if focus is on any surface.
    pub fn is_surface_focused(&self) -> bool {
        matches!(self.current, Some(FocusTarget::Surface(_)))
    }

    /// Get the focused surface ID, if any.
    pub fn focused_surface(&self) -> Option<SurfaceId> {
        match self.current {
            Some(FocusTarget::Surface(id)) => Some(id),
            _ => None,
        }
    }

    /// Clear focus (e.g., during workspace transitions).
    pub fn clear(&mut self) {
        self.current = None;
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

/// How a key event should be handled based on current focus.
#[derive(Debug, PartialEq, Eq)]
pub enum KeyRoute {
    /// Dispatch as a global action.
    Action(KeyAction),
    /// Forward to the focused terminal surface as raw input.
    ForwardToSurface(SurfaceId),
    /// Forward to the sidebar for navigation.
    ForwardToSidebar,
    /// No focus target; drop the event.
    Unhandled,
}

/// Route a key event: check global shortcuts first, then forward to focus target.
pub fn route_key_event(
    input: &KeyInput,
    registry: &KeybindingRegistry,
    focus: &FocusManager,
) -> KeyRoute {
    if let Some(action) = registry.lookup(input) {
        return KeyRoute::Action(action.clone());
    }

    match focus.current() {
        Some(FocusTarget::Surface(id)) => KeyRoute::ForwardToSurface(id),
        Some(FocusTarget::Sidebar) => KeyRoute::ForwardToSidebar,
        None => KeyRoute::Unhandled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::{Key, KeyAction, KeyInput, KeybindingRegistry, Modifiers};
    use crate::workspace::SurfaceId;

    fn plain_key(c: char) -> KeyInput {
        KeyInput { key: Key::Character(c), modifiers: Modifiers::default() }
    }

    fn logo_key(c: char) -> KeyInput {
        KeyInput {
            key: Key::Character(c),
            modifiers: Modifiers { logo: true, ..Default::default() },
        }
    }

    fn registry_with_logo_n() -> KeybindingRegistry {
        let mut reg = KeybindingRegistry::new();
        reg.bind(logo_key('n'), KeyAction::CreateWorkspace);
        reg
    }

    // --- No focus ---

    #[test]
    fn no_focus_non_global_key_is_unhandled() {
        let registry = KeybindingRegistry::new();
        let focus = FocusManager::new();
        let route = route_key_event(&plain_key('a'), &registry, &focus);
        assert_eq!(route, KeyRoute::Unhandled);
    }

    // --- Surface focused ---

    #[test]
    fn surface_focused_non_global_key_forwards_to_surface() {
        let registry = KeybindingRegistry::new();
        let mut focus = FocusManager::new();
        focus.focus_surface(SurfaceId::new(42));
        let route = route_key_event(&plain_key('a'), &registry, &focus);
        assert_eq!(route, KeyRoute::ForwardToSurface(SurfaceId::new(42)));
    }

    // --- Sidebar focused ---

    #[test]
    fn sidebar_focused_non_global_key_forwards_to_sidebar() {
        let registry = KeybindingRegistry::new();
        let mut focus = FocusManager::new();
        focus.focus_sidebar();
        let route = route_key_event(&plain_key('j'), &registry, &focus);
        assert_eq!(route, KeyRoute::ForwardToSidebar);
    }

    // --- Global shortcut priority ---

    #[test]
    fn global_shortcut_takes_priority_over_surface_focus() {
        let registry = registry_with_logo_n();
        let mut focus = FocusManager::new();
        focus.focus_surface(SurfaceId::new(1));
        let route = route_key_event(&logo_key('n'), &registry, &focus);
        assert_eq!(route, KeyRoute::Action(KeyAction::CreateWorkspace));
    }

    #[test]
    fn global_shortcut_takes_priority_over_sidebar_focus() {
        let registry = registry_with_logo_n();
        let mut focus = FocusManager::new();
        focus.focus_sidebar();
        let route = route_key_event(&logo_key('n'), &registry, &focus);
        assert_eq!(route, KeyRoute::Action(KeyAction::CreateWorkspace));
    }

    // --- FocusManager state ---

    #[test]
    fn focus_surface_then_focused_surface_returns_id() {
        let mut focus = FocusManager::new();
        focus.focus_surface(SurfaceId::new(7));
        assert_eq!(focus.focused_surface(), Some(SurfaceId::new(7)));
    }

    #[test]
    fn focus_sidebar_then_is_surface_focused_returns_false() {
        let mut focus = FocusManager::new();
        focus.focus_sidebar();
        assert!(!focus.is_surface_focused());
    }

    #[test]
    fn clear_then_current_returns_none() {
        let mut focus = FocusManager::new();
        focus.focus_surface(SurfaceId::new(1));
        focus.clear();
        assert!(focus.current().is_none());
    }

    // --- Focus transitions ---

    #[test]
    fn focus_transitions_surface_sidebar_surface() {
        let mut focus = FocusManager::new();

        focus.focus_surface(SurfaceId::new(1));
        assert_eq!(focus.current(), Some(FocusTarget::Surface(SurfaceId::new(1))));

        focus.focus_sidebar();
        assert_eq!(focus.current(), Some(FocusTarget::Sidebar));

        focus.focus_surface(SurfaceId::new(2));
        assert_eq!(focus.current(), Some(FocusTarget::Surface(SurfaceId::new(2))));
    }
}
