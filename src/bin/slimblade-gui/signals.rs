//! Cross-thread flags shared by the tray callbacks and the background keeper.

use std::sync::atomic::AtomicBool;

#[derive(Default)]
pub struct GuiSignals {
    /// Explicitly show and focus the settings window.
    pub show: AtomicBool,
    /// Toggle window visibility (tray icon activation).
    pub toggle: AtomicBool,
    pub toggle_daemon: AtomicBool,
    pub apply: AtomicBool,
    pub quit: AtomicBool,
    /// A StatusNotifier host is registered.
    pub tray_ready: AtomicBool,
    /// Registration failed; prevents an optimistic "ready" from overriding it.
    pub tray_offline: AtomicBool,
}
