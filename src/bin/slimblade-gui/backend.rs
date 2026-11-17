//! Tauri commands backed by the Rust core (config, device status, daemon control).

use crate::keeper;
use crate::logger::LogBuffer;
use serde::Serialize;
use slimblade::config::Config;
use slimblade::{device, doctor};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, State};

const SYSTEMD_UNIT: &str = "slimblade.service";

#[derive(Serialize, Clone)]
pub struct Status {
    pub ok: Option<bool>,
    pub text: String,
}

impl Status {
    fn ok(text: impl Into<String>) -> Self {
        Self {
            ok: Some(true),
            text: text.into(),
        }
    }
    fn bad(text: impl Into<String>) -> Self {
        Self {
            ok: Some(false),
            text: text.into(),
        }
    }
    fn unknown(text: impl Into<String>) -> Self {
        Self {
            ok: None,
            text: text.into(),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct Statuses {
    pub device: Status,
    pub virtual_device: Status,
    pub daemon: Status,
    pub udev: Status,
    pub tray: Status,
}

#[derive(Serialize, Clone)]
pub struct AppStateDto {
    pub config_path: String,
    pub config: Config,
    pub statuses: Statuses,
    pub logs: Vec<String>,
    pub keeper_active: bool,
}

pub struct BackendState {
    pub config_path: PathBuf,
    pub exe_path: PathBuf,
    pub config: Config,
    pub logs: LogBuffer,
    pub systemd_backend: bool,
}

impl BackendState {
    pub fn new(config_path: PathBuf, config: Config, logs: LogBuffer, exe_path: PathBuf) -> Self {
        let systemd_backend = systemd_is_active() || systemd_is_enabled();
        Self {
            config_path,
            exe_path,
            config,
            logs,
            systemd_backend,
        }
    }

    fn statuses(&self) -> Statuses {
        let device_status = match device::find_device(&self.config.device, None) {
            Ok(info) => {
                let path = info
                    .event_paths
                    .first()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "нет event-узла".to_string());
                Status::ok(format!("\"{}\" [{}] {}", info.name, info.id(), path))
            }
            Err(err) => Status::bad(first_line(&format!("{err:#}"))),
        };

        let virtual_status = match device::list_input_devices()
            .ok()
            .and_then(|devices| devices.into_iter().find(device::is_managed))
        {
            Some(info) => Status::ok(format!("\"{}\" [{}]", info.name, info.id())),
            None => Status::unknown("не создан"),
        };

        let daemon_status = if self.systemd_backend {
            if systemd_is_active() {
                Status::ok(format!("работает ({SYSTEMD_UNIT})"))
            } else if systemd_is_enabled() {
                Status::bad(format!("остановлен ({SYSTEMD_UNIT}, enabled)"))
            } else {
                Status::bad(format!("остановлен ({SYSTEMD_UNIT})"))
            }
        } else if other_instance_running() {
            Status::ok("работает (фоновый процесс)")
        } else {
            Status::bad("остановлен")
        };

        let udev_status = match doctor::installed_udev_rule() {
            Some(path) => Status::ok(path.display().to_string()),
            None => Status::bad("не установлено; выполните slimblade doctor"),
        };

        let tray_status = if keeper::running_pid().is_some() {
            Status::ok("фоновый процесс активен")
        } else {
            Status::bad("не запущен")
        };

        Statuses {
            device: device_status,
            virtual_device: virtual_status,
            daemon: daemon_status,
            udev: udev_status,
            tray: tray_status,
        }
    }

    fn dto(&self) -> AppStateDto {
        AppStateDto {
            config_path: self.config_path.display().to_string(),
            config: self.config.clone(),
            statuses: self.statuses(),
            logs: self.logs.lines(),
            keeper_active: keeper::running_pid().is_some(),
        }
    }

    fn persist(&mut self, config: Config) -> Result<(), String> {
        config.validate().map_err(|e| format!("{e:#}"))?;
        config
            .save(&self.config_path)
            .map_err(|e| format!("Не удалось сохранить конфигурацию: {e:#}"))?;
        self.config = config;
        Ok(())
    }
}

#[tauri::command]
pub fn get_state(state: State<'_, Mutex<BackendState>>) -> Result<AppStateDto, String> {
    let guard = state.lock().map_err(|e| e.to_string())?;
    Ok(guard.dto())
}

#[tauri::command]
pub fn refresh(state: State<'_, Mutex<BackendState>>) -> Result<AppStateDto, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    let config = load_config(&guard.config_path);
    guard.config = config;
    Ok(guard.dto())
}

#[tauri::command]
pub fn save_config(
    config: Config,
    state: State<'_, Mutex<BackendState>>,
) -> Result<String, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.persist(config)?;
    log::info!("конфигурация сохранена: {}", guard.config_path.display());
    Ok("Конфигурация сохранена.".to_string())
}

#[tauri::command]
pub fn apply_config(
    config: Config,
    state: State<'_, Mutex<BackendState>>,
) -> Result<String, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.persist(config)?;
    ensure_keeper(&guard);
    if keeper::notify(keeper::SIG_RELOAD) {
        Ok("Конфигурация сохранена и демон перезапущен.".to_string())
    } else {
        Err("Не удалось сообщить фоновому процессу о перезапуске демона.".to_string())
    }
}

#[tauri::command]
pub fn daemon(cmd: String, state: State<'_, Mutex<BackendState>>) -> Result<String, String> {
    let guard = state.lock().map_err(|e| e.to_string())?;
    ensure_keeper(&guard);
    let running = if guard.systemd_backend {
        systemd_is_active()
    } else {
        other_instance_running()
    };
    let signal = match cmd.as_str() {
        "toggle" => {
            if running {
                keeper::SIG_STOP
            } else {
                keeper::SIG_START
            }
        }
        "start" => keeper::SIG_START,
        "stop" => keeper::SIG_STOP,
        other => return Err(format!("Неизвестная команда демона: {other}")),
    };
    if keeper::notify(signal) {
        Ok(if signal == keeper::SIG_STOP {
            "Демон остановлен.".to_string()
        } else {
            "Демон запущен.".to_string()
        })
    } else {
        Err("Фоновый процесс трея не запущен.".to_string())
    }
}

#[tauri::command]
pub fn clear_logs(state: State<'_, Mutex<BackendState>>) -> Result<(), String> {
    let guard = state.lock().map_err(|e| e.to_string())?;
    guard.logs.clear();
    Ok(())
}

#[tauri::command]
pub fn hide_to_tray(app: AppHandle) -> Result<(), String> {
    log::info!("окно закрыто, иконка трея остаётся в фоновом процессе");
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) -> Result<(), String> {
    keeper::notify(keeper::SIG_QUIT);
    log::info!("выход из GUI и фонового процесса трея");
    app.exit(0);
    Ok(())
}

/// Make sure the tray keeper process is alive. It can die (or be killed)
/// while the settings window stays open; commands must still work.
fn ensure_keeper(state: &BackendState) {
    if keeper::running_pid().is_some() {
        return;
    }
    if let Err(err) = keeper::spawn_detached(&state.config_path, &state.exe_path) {
        log::warn!("не удалось перезапустить фоновый процесс трея: {err:#}");
        return;
    }
    for _ in 0..20 {
        if keeper::running_pid().is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    log::warn!("фоновый процесс трея не подтвердил запуск");
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).to_string()
}

pub fn load_config(path: &Path) -> Config {
    match Config::load(path) {
        Ok(config) => config,
        Err(err) => {
            log::warn!("не удалось прочитать конфиг {}: {err:#}", path.display());
            Config::default()
        }
    }
}

pub fn load_or_create_config(path: &Path) -> Config {
    if !path.exists() {
        if let Err(err) = Config::write_template(path, false) {
            log::warn!("не удалось создать конфиг {}: {err:#}", path.display());
        } else {
            log::info!("создан конфиг {}", path.display());
        }
    }
    load_config(path)
}

fn other_instance_running() -> bool {
    device::list_input_devices()
        .map(|devices| devices.iter().any(device::is_managed))
        .unwrap_or(false)
}

fn systemd_is_active() -> bool {
    systemctl(&["is-active", "--quiet", SYSTEMD_UNIT])
}

fn systemd_is_enabled() -> bool {
    systemctl(&["is-enabled", "--quiet", SYSTEMD_UNIT])
}

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
