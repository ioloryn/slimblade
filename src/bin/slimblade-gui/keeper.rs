//! Background tray process.
//!
//! Wayland does not allow a client to hide a window: `Visible(false)` is a
//! no-op and `set_minimized` is often ignored. The only reliable way to
//! "minimize to tray" is to destroy the window while keeping the tray icon
//! alive in another process. This module is that process: it owns the SNI
//! icon, the daemon, and re-launches the settings window on demand.

use crate::signals::GuiSignals;
use crate::tray;
use anyhow::{Context, Result};
use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM, SIGUSR1, SIGUSR2};
use signal_hook::flag;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const SYSTEMD_UNIT: &str = "slimblade.service";

/// Signals the window process sends to the keeper to control the daemon.
pub const SIG_START: i32 = signal_hook::consts::SIGUSR1;
pub const SIG_STOP: i32 = signal_hook::consts::SIGUSR2;
pub const SIG_RELOAD: i32 = signal_hook::consts::SIGHUP;
pub const SIG_QUIT: i32 = signal_hook::consts::SIGTERM;

fn pid_path() -> PathBuf {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime).join("slimblade-gui.pid")
    } else {
        std::env::temp_dir().join(format!("slimblade-gui-{}.pid", unsafe { libc::getuid() }))
    }
}

/// PID of a running keeper, if the pidfile points to a live process.
pub fn running_pid() -> Option<i32> {
    let raw = fs::read_to_string(pid_path()).ok()?;
    let pid: i32 = raw.trim().parse().ok()?;
    if pid <= 0 {
        return None;
    }
    // signal 0 does not deliver anything, it only checks for existence.
    let ok = unsafe { libc::kill(pid, 0) } == 0;
    ok.then_some(pid)
}

/// Launch the keeper as a detached background process.
///
/// The executable path is passed explicitly: a keeper that outlives a binary
/// replacement would otherwise see `current_exe()` as "<path> (deleted)" and
/// fail to re-open the settings window.
pub fn spawn_detached(config_path: &PathBuf, exe: &PathBuf) -> Result<()> {
    Command::new(exe)
        .arg("--tray-only")
        .arg("--config")
        .arg(config_path)
        .arg("--exe")
        .arg(exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .context("не удалось запустить фоновый процесс трея")?;
    Ok(())
}

fn send_signal(pid: i32, sig: i32) -> bool {
    let result = unsafe { libc::kill(pid, sig) };
    result == 0
}

/// Ask the keeper to start/stop/reload the daemon or quit.
pub fn notify(sig: i32) -> bool {
    running_pid().is_some_and(|pid| send_signal(pid, sig))
}

fn systemd_active() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", SYSTEMD_UNIT])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn systemctl(args: &[&str]) {
    let _ = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn open_settings_window(exe: &PathBuf, config_path: &PathBuf) {
    if let Err(err) = Command::new(exe)
        .arg("--window")
        .arg("--config")
        .arg(config_path)
        .spawn()
    {
        log::error!("не удалось открыть окно настроек: {err}");
    }
}

enum Daemon {
    Systemd,
    Embedded {
        child: Option<Child>,
        config: PathBuf,
        exe: PathBuf,
    },
}

impl Daemon {
    fn detect(config: PathBuf, exe: PathBuf) -> Self {
        if Command::new("systemctl")
            .args(["--user", "is-enabled", "--quiet", SYSTEMD_UNIT])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
            || systemd_active()
        {
            Self::Systemd
        } else {
            Self::Embedded {
                child: None,
                config,
                exe,
            }
        }
    }

    fn running(&mut self) -> bool {
        match self {
            Self::Systemd => systemd_active(),
            Self::Embedded { child, .. } => child
                .as_mut()
                .is_some_and(|c| matches!(c.try_wait(), Ok(None))),
        }
    }

    fn start(&mut self) {
        match self {
            Self::Systemd => systemctl(&["start", SYSTEMD_UNIT]),
            Self::Embedded { child, config, exe } => {
                if child
                    .as_mut()
                    .is_some_and(|c| matches!(c.try_wait(), Ok(None)))
                {
                    return;
                }
                let daemon_exe = exe.with_file_name("slimblade");
                match Command::new(daemon_exe)
                    .arg("run")
                    .arg("--config")
                    .arg(config)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(c) => {
                        *child = Some(c);
                        log::info!("демон запущен фоновым процессом");
                    }
                    Err(err) => log::error!("не удалось запустить демон: {err}"),
                }
            }
        }
    }

    fn stop(&mut self) {
        match self {
            Self::Systemd => systemctl(&["stop", SYSTEMD_UNIT]),
            Self::Embedded { child, .. } => {
                if let Some(mut c) = child.take() {
                    let _ = c.kill();
                    let _ = c.wait();
                    log::info!("демон остановлен");
                }
            }
        }
    }

    fn reload(&mut self) {
        match self {
            Self::Systemd => systemctl(&["restart", SYSTEMD_UNIT]),
            Self::Embedded { .. } => {
                self.stop();
                self.start();
            }
        }
    }
}

/// Run the tray keeper forever. Called with `--tray-only`.
pub fn run(config_path: PathBuf, exe_override: Option<PathBuf>) -> Result<()> {
    let exe = exe_override.unwrap_or_else(|| {
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from("slimblade-gui"))
    });
    let pid = std::process::id() as i32;
    fs::write(pid_path(), pid.to_string()).context("не удалось записать pid-файл трея")?;

    let start = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let reload = Arc::new(AtomicBool::new(false));
    let quit = Arc::new(AtomicBool::new(false));
    for (sig, flag) in [
        (SIGUSR1, &start),
        (SIGUSR2, &stop),
        (SIGHUP, &reload),
        (SIGTERM, &quit),
        (SIGINT, &quit),
    ] {
        flag::register(sig, Arc::clone(flag))
            .context("не удалось установить обработчик сигнала")?;
    }

    let running = Arc::new(AtomicBool::new(false));
    let tray_signals = Arc::new(GuiSignals::default());
    let _tray = tray::spawn(tray_signals.clone(), running.clone());

    let mut daemon = Daemon::detect(config_path.clone(), exe.clone());
    daemon.start();

    log::info!("фоновый процесс трея запущен (pid {pid})");

    let result = loop {
        if quit.load(Ordering::Relaxed) || tray_signals.quit.swap(false, Ordering::Relaxed) {
            break Ok(());
        }

        if start.swap(false, Ordering::Relaxed) && !daemon.running() {
            daemon.start();
        }
        if tray_signals.toggle_daemon.swap(false, Ordering::Relaxed) {
            if daemon.running() {
                daemon.stop();
            } else {
                daemon.start();
            }
        }
        if stop.swap(false, Ordering::Relaxed) {
            daemon.stop();
        }
        if reload.swap(false, Ordering::Relaxed)
            || tray_signals.apply.swap(false, Ordering::Relaxed)
        {
            daemon.reload();
        }

        let is_running = daemon.running();
        running.store(is_running, Ordering::Relaxed);

        if tray_signals.show.swap(false, Ordering::Relaxed)
            || tray_signals.toggle.swap(false, Ordering::Relaxed)
        {
            open_settings_window(&exe, &config_path);
        }

        std::thread::sleep(Duration::from_millis(200));
    };

    daemon.stop();
    fs::remove_file(pid_path()).ok();
    result
}
