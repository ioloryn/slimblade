//! Graphical settings and tray application for the Kensington SlimBlade.
//!
//! The settings window is a Tauri app with a Material 3 web frontend. The tray
//! icon and daemon live in a separate background process (`--tray-only`),
//! because Wayland compositors do not let a client hide its own window.

mod backend;
mod keeper;
mod logger;
mod signals;
mod tray;

use clap::Parser;
use slimblade::config;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Parser)]
#[command(
    name = "slimblade-gui",
    version,
    about = "Настройка Kensington SlimBlade и управление демоном из трея"
)]
struct Cli {
    /// Конфигурационный файл (по умолчанию: $XDG_CONFIG_HOME/slimblade/config.toml)
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Запустить только фоновый процесс с треем, без окна (для автозапуска).
    #[arg(long)]
    hidden: bool,

    /// Внутренний режим: фоновый процесс с иконкой трея.
    #[arg(long, hide = true)]
    tray_only: bool,

    /// Внутренний режим: окно, запущенное из трея.
    #[arg(long, hide = true)]
    window: bool,

    /// Внутренний: абсолютный путь к текущему бинарнику (передаётся трею).
    #[arg(long, hide = true, value_name = "PATH")]
    exe: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    let config_path = match config::resolve_config_path(cli.config.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("Ошибка: {err:#}");
            std::process::exit(1);
        }
    };

    let exe_path = cli
        .exe
        .clone()
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("slimblade-gui"));

    if cli.tray_only {
        let _logs = logger::init();
        if let Err(err) = keeper::run(config_path, cli.exe) {
            eprintln!("Фоновый процесс трея: {err:#}");
            std::process::exit(1);
        }
        return;
    }

    if cli.hidden {
        if keeper::running_pid().is_none() {
            if let Err(err) = keeper::spawn_detached(&config_path, &exe_path) {
                eprintln!("Не удалось запустить трей: {err:#}");
            }
        }
        return;
    }

    if !cli.window && keeper::running_pid().is_none() {
        if let Err(err) = keeper::spawn_detached(&config_path, &exe_path) {
            eprintln!("Не удалось запустить трей: {err:#}");
        }
    }

    let logs = logger::init();
    let config = backend::load_or_create_config(&config_path);
    let state = Mutex::new(backend::BackendState::new(
        config_path,
        config,
        logs,
        exe_path,
    ));

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            backend::get_state,
            backend::refresh,
            backend::save_config,
            backend::apply_config,
            backend::daemon,
            backend::clear_logs,
            backend::hide_to_tray,
            backend::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить Tauri-приложение");
}
