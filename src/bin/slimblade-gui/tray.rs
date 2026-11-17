//! StatusNotifierItem tray integration (ksni 0.3 / zbus).
//!
//! ksni 0.3 is used because it reports registration errors instead of silently
//! panicking in a background thread, and works with modern SNI hosts (Plasma,
//! AppIndicator, ...) without depending on libdbus.

use crate::signals::GuiSignals;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, OfflineReason, Status, Tray};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SlimbladeTray {
    signals: Arc<GuiSignals>,
    running: Arc<AtomicBool>,
}

impl Tray for SlimbladeTray {
    fn id(&self) -> String {
        "slimblade".to_string()
    }

    fn title(&self) -> String {
        "SlimBlade".to_string()
    }

    fn icon_name(&self) -> String {
        "input-mouse".to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![tray_icon(32)]
    }

    fn status(&self) -> Status {
        if self.running.load(Ordering::Relaxed) {
            Status::Active
        } else {
            Status::Passive
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.signals.toggle.store(true, Ordering::Relaxed);
    }

    fn watcher_online(&self) {
        self.signals.tray_offline.store(false, Ordering::Relaxed);
        self.signals.tray_ready.store(true, Ordering::Relaxed);
        log::info!("системный трей подключён");
    }

    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        self.signals.tray_offline.store(true, Ordering::Relaxed);
        self.signals.tray_ready.store(false, Ordering::Relaxed);
        log::warn!("системный трей недоступен: {reason:?}");
        // Keep the service alive: the watcher may come back after a shell restart.
        true
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let running = self.running.load(Ordering::Relaxed);
        vec![
            MenuItem::Standard(StandardItem {
                label: "Показать/скрыть окно".to_string(),
                icon_name: "view-restore".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    tray.signals.toggle.store(true, Ordering::Relaxed);
                }),
                ..Default::default()
            }),
            MenuItem::Standard(StandardItem {
                label: "Открыть настройки".to_string(),
                icon_name: "preferences-system".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    tray.signals.show.store(true, Ordering::Relaxed);
                }),
                ..Default::default()
            }),
            MenuItem::Standard(StandardItem {
                label: if running {
                    "Остановить демон".to_string()
                } else {
                    "Запустить демон".to_string()
                },
                icon_name: if running {
                    "media-playback-stop".to_string()
                } else {
                    "media-playback-start".to_string()
                },
                activate: Box::new(|tray: &mut Self| {
                    tray.signals.toggle_daemon.store(true, Ordering::Relaxed);
                }),
                ..Default::default()
            }),
            MenuItem::Standard(StandardItem {
                label: "Применить конфигурацию".to_string(),
                icon_name: "view-refresh".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    tray.signals.apply.store(true, Ordering::Relaxed);
                }),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Выход".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    tray.signals.quit.store(true, Ordering::Relaxed);
                }),
                ..Default::default()
            }),
        ]
    }
}

/// Start the tray service. A missing StatusNotifierItem host is not fatal.
pub fn spawn(signals: Arc<GuiSignals>, running: Arc<AtomicBool>) -> Option<Handle<SlimbladeTray>> {
    let tray = SlimbladeTray {
        signals: signals.clone(),
        running,
    };

    // `assume_sni_available` lets the service stay alive if the shell starts
    // later; `watcher_offline` then clears the "ready" flag and
    // `watcher_online` sets it again once the host appears.
    match tray.assume_sni_available(true).spawn() {
        Ok(handle) => {
            // Registration normally succeeded here. If it failed with
            // "ServiceUnknown", `watcher_offline` already fired inside
            // `spawn()` and set `tray_offline`, so don't overwrite it.
            if !signals.tray_offline.load(Ordering::Relaxed) {
                signals.tray_ready.store(true, Ordering::Relaxed);
                log::info!("иконка трея зарегистрирована");
            }
            Some(handle)
        }
        Err(err) => {
            log::warn!("не удалось запустить трей: {err:?}");
            None
        }
    }
}

/// A small trackball icon in ARGB32, network byte order.
fn tray_icon(size: i32) -> Icon {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let center = (size as f32 - 1.0) / 2.0;
    let radius = size as f32 * 0.42;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();

            let (a, r, g, b) = if distance <= radius {
                let highlight = (1.0 - distance / radius) * 40.0;
                (
                    255,
                    (50.0 + highlight) as u8,
                    (120.0 + highlight) as u8,
                    (190.0 + highlight) as u8,
                )
            } else if distance <= radius + 1.5 {
                (180, 210, 215, 225)
            } else {
                (0, 0, 0, 0)
            };
            data.extend_from_slice(&[a, r, g, b]);
        }
    }

    Icon {
        width: size,
        height: size,
        data,
    }
}
