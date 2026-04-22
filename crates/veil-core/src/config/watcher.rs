//! Config file watcher — hot-reload via the `notify` crate.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::{RecursiveMode, Watcher};
use tracing::{debug, warn};

use super::diff::ConfigDelta;
use super::parse::{parse_config, validate_config, ConfigWarning};
use super::{AppConfig, ConfigError};
use crate::lifecycle::ShutdownHandle;

/// Event emitted by the config watcher.
#[derive(Debug)]
pub enum ConfigEvent {
    /// Config was successfully reloaded.
    Reloaded {
        /// The new config.
        config: Box<AppConfig>,
        /// What changed.
        delta: ConfigDelta,
        /// Non-fatal validation warnings.
        warnings: Vec<ConfigWarning>,
    },
    /// Config file was modified but had errors; previous config retained.
    Error(ConfigError),
}

/// Watches the config file for changes and emits `ConfigEvent`s.
pub struct ConfigWatcher {
    config_path: PathBuf,
    event_tx: tokio::sync::mpsc::Sender<ConfigEvent>,
    /// Shared config state, updated by both manual `reload()` and background file-triggered reloads.
    bg_config: Arc<Mutex<AppConfig>>,
    /// Holds the notify watcher so it stays alive while watching.
    #[allow(dead_code)]
    notify_watcher: Option<notify::RecommendedWatcher>,
    /// Handle to the background watcher thread.
    #[allow(dead_code)]
    watcher_thread: Option<std::thread::JoinHandle<()>>,
}

impl ConfigWatcher {
    /// Create a new watcher for the given config file path.
    /// `event_tx` is a channel sender for delivering config events.
    /// The watcher does NOT start until `start()` is called.
    pub fn new(
        config_path: PathBuf,
        initial_config: AppConfig,
        event_tx: tokio::sync::mpsc::Sender<ConfigEvent>,
    ) -> Result<Self, ConfigError> {
        let bg_config = Arc::new(Mutex::new(initial_config));
        Ok(Self { config_path, event_tx, bg_config, notify_watcher: None, watcher_thread: None })
    }

    /// Start watching for file changes.
    /// This spawns an internal task that runs until the watcher is dropped
    /// or a shutdown signal is received.
    pub fn start(&mut self, shutdown: ShutdownHandle) -> Result<(), ConfigError> {
        let config_path = self.config_path.clone();
        let bg_config = Arc::clone(&self.bg_config);
        let event_tx = self.event_tx.clone();

        // Channel for notify events to the background thread.
        let (notify_tx, notify_rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();

        // Create the notify watcher, sending events through the std channel.
        let config_path_for_err = config_path.clone();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = notify_tx.send(res);
        })
        .map_err(|e| ConfigError::ReadError {
            path: config_path_for_err,
            source: std::io::Error::other(format!("failed to create file watcher: {e}")),
        })?;

        // Watch the parent directory (not the file itself) to catch delete+recreate.
        let watch_dir = self.config_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        watcher.watch(&watch_dir, RecursiveMode::NonRecursive).map_err(|e| {
            ConfigError::ReadError {
                path: self.config_path.clone(),
                source: std::io::Error::other(format!("failed to watch directory: {e}")),
            }
        })?;

        self.notify_watcher = Some(watcher);

        // Spawn background thread that processes notify events.
        let thread = std::thread::spawn(move || {
            watcher_loop(&notify_rx, &config_path, &bg_config, &event_tx, &shutdown);
            drop(event_tx);
        });

        self.watcher_thread = Some(thread);
        Ok(())
    }

    /// Get the currently active (valid) config.
    ///
    /// This reflects changes from both manual `reload()` calls and
    /// background file-triggered reloads.
    pub fn current_config(&self) -> AppConfig {
        match self.bg_config.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => {
                warn!("bg_config mutex poisoned in current_config(); returning last known config");
                poisoned.into_inner().clone()
            }
        }
    }

    /// Manually trigger a reload (useful for testing or user-initiated reload).
    pub fn reload(&mut self) -> Result<ConfigEvent, ConfigError> {
        let contents = std::fs::read_to_string(&self.config_path)
            .map_err(|e| ConfigError::ReadError { path: self.config_path.clone(), source: e })?;

        match parse_config(&contents, &self.config_path) {
            Ok(parsed) => {
                let (validated, warnings) = validate_config(parsed);

                let mut current = match self.bg_config.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!("bg_config mutex poisoned in reload(); recovering");
                        poisoned.into_inner()
                    }
                };
                let delta = ConfigDelta::diff(&current, &validated);
                *current = validated.clone();

                Ok(ConfigEvent::Reloaded { config: Box::new(validated), delta, warnings })
            }
            Err(e) => Ok(ConfigEvent::Error(e)),
        }
    }
}

/// Main loop for the background watcher thread: receives notify events,
/// debounces them, reloads config from disk, and sends `ConfigEvent`s.
fn watcher_loop(
    notify_rx: &std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
    config_path: &Path,
    bg_config: &Arc<Mutex<AppConfig>>,
    event_tx: &tokio::sync::mpsc::Sender<ConfigEvent>,
    shutdown: &ShutdownHandle,
) {
    let debounce = std::time::Duration::from_millis(50);

    loop {
        if shutdown.is_triggered() {
            debug!("config watcher shutting down");
            break;
        }

        let event = notify_rx.recv_timeout(std::time::Duration::from_millis(100));

        if shutdown.is_triggered() {
            debug!("config watcher shutting down after recv");
            break;
        }

        match event {
            Ok(Ok(notify_event)) => {
                if !is_relevant_event(&notify_event, config_path) {
                    continue;
                }

                // Debounce: drain additional events arriving within the quiet window.
                drain_debounce_window(notify_rx, debounce);

                if shutdown.is_triggered() {
                    break;
                }

                let config_event = reload_from_disk(config_path, bg_config);
                if event_tx.blocking_send(config_event).is_err() {
                    debug!("config event receiver dropped, stopping watcher");
                    break;
                }
            }
            Ok(Err(e)) => {
                warn!("file watcher error: {e}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                debug!("notify watcher disconnected, stopping");
                break;
            }
        }
    }
}

/// Drain events arriving within the debounce window so we only process
/// the file once after a quiet period.
fn drain_debounce_window(
    notify_rx: &std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
    debounce: std::time::Duration,
) {
    while let Ok(Ok(_) | Err(_)) = notify_rx.recv_timeout(debounce) {}
}

/// Check whether a notify event is relevant (affects our config file and is
/// a Modify or Create event).
fn is_relevant_event(event: &notify::Event, config_path: &Path) -> bool {
    match event.kind {
        notify::EventKind::Modify(_) | notify::EventKind::Create(_) => {}
        _ => return false,
    }
    event.paths.iter().any(|p| {
        if let (Ok(ep), Ok(tp)) = (std::fs::canonicalize(p), std::fs::canonicalize(config_path)) {
            return ep == tp;
        }
        // Fallback: normalize relative paths to absolute before comparing.
        let abs_p = if p.is_relative() {
            std::env::current_dir().map_or_else(|_| p.clone(), |cwd| cwd.join(p))
        } else {
            p.clone()
        };
        let abs_cfg = if config_path.is_relative() {
            std::env::current_dir()
                .map_or_else(|_| config_path.to_path_buf(), |cwd| cwd.join(config_path))
        } else {
            config_path.to_path_buf()
        };
        abs_p.file_name() == abs_cfg.file_name() && abs_p.parent() == abs_cfg.parent()
    })
}

/// Reload the config from disk, diff against the shared current config,
/// and update it if valid.
fn reload_from_disk(config_path: &Path, bg_config: &Arc<Mutex<AppConfig>>) -> ConfigEvent {
    let contents = match std::fs::read_to_string(config_path) {
        Ok(s) => s,
        Err(e) => {
            return ConfigEvent::Error(ConfigError::ReadError {
                path: config_path.to_path_buf(),
                source: e,
            });
        }
    };

    let parsed = match parse_config(&contents, config_path) {
        Ok(c) => c,
        Err(e) => return ConfigEvent::Error(e),
    };

    let (validated, warnings) = validate_config(parsed);

    let mut current = match bg_config.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("bg_config mutex poisoned in reload_from_disk(); recovering");
            poisoned.into_inner()
        }
    };
    let delta = ConfigDelta::diff(&current, &validated);
    *current = validated.clone();

    ConfigEvent::Reloaded { config: Box::new(validated), delta, warnings }
}
