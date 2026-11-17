//! The remapping daemon: grab the physical trackball, apply the configured
//! mappings, and expose the result through a virtual uinput device.

use crate::config::{Config, ResolvedAction, ResolvedConfig};
use crate::device::{InputDeviceInfo, find_device, is_pointer_device, open_event_device};
use anyhow::{Context, Result, bail};
use evdev::uinput::VirtualDevice;
use evdev::{
    AttributeSet, BusType, Device, EventType, InputEvent, InputId, KeyCode, PropType,
    RelativeAxisCode, RelativeAxisEvent, SynchronizationCode,
};
use log::{debug, error, info, warn};
use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const SCROLL_UNITS_PER_TICK: i64 = 120;
const MOUSE_BUTTONS: [KeyCode; 5] = [
    KeyCode::BTN_LEFT,
    KeyCode::BTN_RIGHT,
    KeyCode::BTN_MIDDLE,
    KeyCode::BTN_SIDE,
    KeyCode::BTN_EXTRA,
];

pub fn run(config_path: PathBuf, device_override: Option<PathBuf>) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let reload = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGINT, shutdown.clone())
        .context("cannot install SIGINT handler")?;
    signal_hook::flag::register(SIGQUIT, shutdown.clone())
        .context("cannot install SIGQUIT handler")?;
    signal_hook::flag::register(SIGTERM, shutdown.clone())
        .context("cannot install SIGTERM handler")?;
    signal_hook::flag::register(SIGHUP, reload.clone()).context("cannot install SIGHUP handler")?;

    run_loop(config_path, device_override, shutdown, reload)
}

/// A daemon owned by an embedding application, such as the GUI.
///
/// The daemon runs on a background thread and stops when this handle is
/// dropped. Use [`DaemonHandle::start`] after validating the configuration and
/// [`DaemonHandle::reload`] to request a SIGHUP-like reload.
pub struct DaemonHandle {
    shutdown: Arc<AtomicBool>,
    reload: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
}

impl DaemonHandle {
    /// Validate the configuration and start the daemon on a background thread.
    pub fn start(config_path: PathBuf, device_override: Option<PathBuf>) -> Result<Self> {
        if !config_path.exists() {
            bail!(
                "config {} does not exist; run `slimblade init` to create one",
                config_path.display()
            );
        }
        Config::load(&config_path)?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let reload = Arc::new(AtomicBool::new(false));
        let thread = {
            let shutdown = shutdown.clone();
            let reload = reload.clone();
            std::thread::Builder::new()
                .name("slimblade-daemon".to_string())
                .spawn(move || run_loop(config_path, device_override, shutdown, reload))
                .context("cannot start the daemon thread")?
        };

        Ok(Self {
            shutdown,
            reload,
            thread: Some(thread),
        })
    }

    /// Ask the daemon to reload its configuration.
    pub fn reload(&self) -> Result<()> {
        if !self.is_running() {
            bail!("the daemon is not running");
        }
        self.reload.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Stop the daemon and wait for its thread to finish.
    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    pub fn is_running(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
    }
}

impl Drop for DaemonHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_loop(
    config_path: PathBuf,
    device_override: Option<PathBuf>,
    shutdown: Arc<AtomicBool>,
    reload: Arc<AtomicBool>,
) -> Result<()> {
    if !config_path.exists() {
        bail!(
            "config {} does not exist; run `slimblade init` to create one",
            config_path.display()
        );
    }

    let mut config = Config::load(&config_path)?;
    let mut resolved = config
        .resolve()
        .context("cannot resolve the button mappings")?;

    let mut virtual_device: Option<VirtualDevice> = None;
    let mut virtual_keys: BTreeSet<KeyCode> = BTreeSet::new();
    let mut last_problem: Option<String> = None;

    while !shutdown.load(Ordering::Relaxed) {
        if reload.swap(false, Ordering::Relaxed) {
            reload_config(&config_path, &mut config, &mut resolved, &virtual_keys);
        }

        let info = match find_device(&config.device, device_override.as_deref()) {
            Ok(info) => {
                last_problem = None;
                info
            }
            Err(err) => {
                warn_once(
                    &mut last_problem,
                    &format!("waiting for the trackball: {err:#}"),
                );
                sleep_interruptible(&shutdown, Duration::from_millis(700));
                continue;
            }
        };

        let path = choose_event_path(&info);
        let Some(path) = path else {
            warn_once(
                &mut last_problem,
                &format!("\"{}\" has no event device", info.name),
            );
            sleep_interruptible(&shutdown, Duration::from_millis(700));
            continue;
        };

        let mut device = match open_event_device(&path) {
            Ok(device) => device,
            Err(err) => {
                warn_once(&mut last_problem, &format!("{err:#}"));
                sleep_interruptible(&shutdown, Duration::from_millis(700));
                continue;
            }
        };

        if let Err(err) = device.grab() {
            warn_once(
                &mut last_problem,
                &format!(
                    "cannot grab {}: {err}. Another remapper may already own the device",
                    path.display()
                ),
            );
            sleep_interruptible(&shutdown, Duration::from_millis(700));
            continue;
        }
        if let Err(err) = device.set_nonblocking(true) {
            bail!("cannot set {} nonblocking: {err}", path.display());
        }

        if virtual_device.is_none() {
            match create_virtual_device(&info, &device, &resolved) {
                Ok((vdev, keys)) => {
                    virtual_device = Some(vdev);
                    virtual_keys = keys;
                }
                Err(err) => {
                    bail!(
                        "cannot create the virtual input device: {err:#}\n\
                         Check that /dev/uinput is writable by your user, or install \
                         packaging/70-slimblade.rules."
                    );
                }
            }
        }

        info!(
            "managing \"{}\" at {} [{}]",
            info.name,
            path.display(),
            info.id()
        );

        // The grab is released when the device is dropped or the session ends.
        let end = run_session(
            &mut device,
            virtual_device.as_mut().expect("virtual device exists"),
            &resolved,
            &shutdown,
            &reload,
        );

        // `run_session` releases every output key before returning.

        match end {
            Ok(SessionEnd::Shutdown) => break,
            Ok(SessionEnd::Reload) => {
                info!("SIGHUP received, reloading configuration");
                reload_config(&config_path, &mut config, &mut resolved, &virtual_keys);
                // Reopen so a changed device filter/path takes effect.
            }
            Ok(SessionEnd::DeviceLost) => {
                warn!(
                    "lost \"{}\" at {}; waiting for it to come back",
                    info.name,
                    path.display()
                );
                sleep_interruptible(&shutdown, Duration::from_millis(500));
            }
            Err(err) => {
                error!("session ended with an error: {err:#}");
                sleep_interruptible(&shutdown, Duration::from_millis(700));
            }
        }
    }

    info!("stopped");
    Ok(())
}

fn choose_event_path(info: &InputDeviceInfo) -> Option<PathBuf> {
    info.event_paths
        .iter()
        .find(|path| is_pointer_device(path))
        .or_else(|| info.event_paths.first())
        .cloned()
}

fn reload_config(
    path: &Path,
    config: &mut Config,
    resolved: &mut ResolvedConfig,
    virtual_keys: &BTreeSet<KeyCode>,
) {
    let result = Config::load(path).and_then(|new_config| {
        let new_resolved = new_config
            .resolve()
            .context("cannot resolve the new button mappings")?;
        Ok((new_config, new_resolved))
    });

    match result {
        Ok((new_config, new_resolved)) => {
            let missing: Vec<_> = new_resolved
                .output_keys
                .difference(virtual_keys)
                .copied()
                .collect();
            if !missing.is_empty() {
                let names: Vec<String> = missing.iter().map(|code| format!("{code:?}")).collect();
                error!(
                    "config needs key codes that the running virtual device does not expose: {}",
                    names.join(", ")
                );
                error!("restart the daemon to apply this change; keeping the previous config");
                return;
            }
            *config = new_config;
            *resolved = new_resolved;
            info!(
                "configuration reloaded: {} button mappings, scroll speed {:.2}{}",
                resolved.buttons.len(),
                resolved.config.scroll.speed,
                if resolved.config.scroll.natural {
                    ", inverted"
                } else {
                    ""
                }
            );
        }
        Err(err) => {
            error!("cannot reload configuration: {err:#}");
            error!("keeping the previous configuration");
        }
    }
}

fn sleep_interruptible(shutdown: &AtomicBool, duration: Duration) {
    let step = Duration::from_millis(100);
    let mut elapsed = Duration::ZERO;
    while elapsed < duration && !shutdown.load(Ordering::Relaxed) {
        std::thread::sleep(step);
        elapsed += step;
    }
}

fn warn_once(last: &mut Option<String>, message: &str) {
    if last.as_deref() != Some(message) {
        warn!("{message}");
        *last = Some(message.to_string());
    } else {
        debug!("{message}");
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SessionEnd {
    Shutdown,
    Reload,
    DeviceLost,
}

fn run_session(
    device: &mut Device,
    virtual_device: &mut VirtualDevice,
    resolved: &ResolvedConfig,
    shutdown: &AtomicBool,
    reload: &AtomicBool,
) -> Result<SessionEnd> {
    let mut engine = Engine::new(resolved);
    let mut frame = Vec::new();
    let fd = device.as_raw_fd();

    let end = loop {
        if shutdown.load(Ordering::Relaxed) {
            break SessionEnd::Shutdown;
        }
        if reload.load(Ordering::Relaxed) {
            break SessionEnd::Reload;
        }

        let mut poll_fd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll_fd, 1, 250) };
        if ready < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break SessionEnd::DeviceLost;
        }
        if ready == 0 {
            continue;
        }

        if poll_fd.revents & libc::POLLIN == 0 {
            if poll_fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                break SessionEnd::DeviceLost;
            }
            continue;
        }

        let events = match device.fetch_events() {
            Ok(events) => events,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => continue,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => {
                warn!("cannot read events: {err}");
                break SessionEnd::DeviceLost;
            }
        };

        for event in events {
            if event.event_type() == EventType::SYNCHRONIZATION {
                if event.code() == SynchronizationCode::SYN_REPORT.0 && !frame.is_empty() {
                    let output = engine.process_frame(&frame);
                    frame.clear();
                    if !output.is_empty() {
                        virtual_device
                            .emit(&output)
                            .context("cannot write to the virtual device")?;
                    }
                }
                // SYN_DROPPED is handled by the evdev crate; other SYN events
                // carry no useful information for a mouse.
            } else {
                frame.push(event);
            }
        }
    };

    let mut output = Vec::new();
    engine.release_all(&mut output);
    if !output.is_empty() {
        let _ = virtual_device.emit(&output);
    }
    Ok(end)
}

fn create_virtual_device(
    info: &InputDeviceInfo,
    physical: &Device,
    resolved: &ResolvedConfig,
) -> Result<(VirtualDevice, BTreeSet<KeyCode>)> {
    let mut keys = AttributeSet::<KeyCode>::new();
    let mut key_set = BTreeSet::new();

    for code in MOUSE_BUTTONS {
        keys.insert(code);
        key_set.insert(code);
    }
    if let Some(supported) = physical.supported_keys() {
        for code in supported.iter() {
            keys.insert(code);
            key_set.insert(code);
        }
    }
    for code in &resolved.output_keys {
        keys.insert(*code);
        key_set.insert(*code);
    }
    // A pointer needs a left button to be recognized as a mouse.
    keys.insert(KeyCode::BTN_LEFT);
    key_set.insert(KeyCode::BTN_LEFT);

    let mut axes = AttributeSet::<RelativeAxisCode>::new();
    for axis in [
        RelativeAxisCode::REL_X,
        RelativeAxisCode::REL_Y,
        RelativeAxisCode::REL_WHEEL,
        RelativeAxisCode::REL_WHEEL_HI_RES,
        RelativeAxisCode::REL_HWHEEL,
        RelativeAxisCode::REL_HWHEEL_HI_RES,
    ] {
        axes.insert(axis);
    }
    if let Some(supported) = physical.supported_relative_axes() {
        for axis in supported.iter() {
            axes.insert(axis);
        }
    }

    let mut props = AttributeSet::<PropType>::new();
    props.insert(PropType::POINTER);

    let input_id = InputId::new(BusType::BUS_USB, info.vendor, info.product, info.version);

    let device = VirtualDevice::builder()
        .context("cannot open /dev/uinput")?
        .name("SlimBlade (managed)")
        .input_id(input_id)
        .with_keys(&keys)
        .context("cannot set virtual key capabilities")?
        .with_relative_axes(&axes)
        .context("cannot set virtual relative-axis capabilities")?
        .with_properties(&props)
        .context("cannot set virtual device properties")?
        .build()
        .context("cannot register virtual device")?;

    info!(
        "created virtual pointer \"SlimBlade (managed)\" ({} keys, {} axes)",
        key_set.len(),
        axes.iter().count()
    );

    Ok((device, key_set))
}

#[derive(Debug)]
enum Active {
    Emit(Vec<KeyCode>),
    DragLock,
    Exec,
    None,
}

#[derive(Debug, Default)]
struct OutputState {
    counts: BTreeMap<KeyCode, u32>,
}

impl OutputState {
    fn press(&mut self, code: KeyCode, out: &mut Vec<InputEvent>) {
        let count = self.counts.entry(code).or_insert(0);
        if *count == 0 {
            out.push(evdev::KeyEvent::new_now(code, 1).into());
        }
        *count += 1;
    }

    fn release(&mut self, code: KeyCode, out: &mut Vec<InputEvent>) {
        let Some(count) = self.counts.get_mut(&code) else {
            return;
        };
        if *count <= 1 {
            self.counts.remove(&code);
            out.push(evdev::KeyEvent::new_now(code, 0).into());
        } else {
            *count -= 1;
        }
    }

    fn release_all(&mut self, out: &mut Vec<InputEvent>) {
        for code in std::mem::take(&mut self.counts).into_keys() {
            out.push(evdev::KeyEvent::new_now(code, 0).into());
        }
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }
}

pub struct Engine {
    buttons: BTreeMap<KeyCode, ResolvedAction>,
    scroll_natural: bool,
    scroll_speed: f64,
    active: BTreeMap<KeyCode, Active>,
    /// Physical drag-lock button -> output button currently held.
    drag_locked: BTreeMap<KeyCode, KeyCode>,
    output: OutputState,
    scroll_accumulator: f64,
    scroll_total_hi_res: i64,
    scroll_low_res_emitted: i64,
}

impl Engine {
    pub fn new(resolved: &ResolvedConfig) -> Self {
        Self {
            buttons: resolved.buttons.clone(),
            scroll_natural: resolved.config.scroll.natural,
            scroll_speed: resolved.config.scroll.speed,
            active: BTreeMap::new(),
            drag_locked: BTreeMap::new(),
            output: OutputState::default(),
            scroll_accumulator: 0.0,
            scroll_total_hi_res: 0,
            scroll_low_res_emitted: 0,
        }
    }

    /// Transform one evdev frame into the virtual device's event stream.
    pub fn process_frame(&mut self, frame: &[InputEvent]) -> Vec<InputEvent> {
        let mut out = Vec::new();
        let mut wheel_lo: i64 = 0;
        let mut wheel_hi: i64 = 0;
        let mut have_wheel_lo = false;
        let mut have_wheel_hi = false;

        for &event in frame {
            match event.event_type() {
                EventType::KEY => {
                    self.handle_key(KeyCode(event.code()), event.value(), &mut out);
                }
                EventType::RELATIVE => match RelativeAxisCode(event.code()) {
                    RelativeAxisCode::REL_WHEEL => {
                        wheel_lo += event.value() as i64;
                        have_wheel_lo = true;
                    }
                    RelativeAxisCode::REL_WHEEL_HI_RES => {
                        wheel_hi += event.value() as i64;
                        have_wheel_hi = true;
                    }
                    _ => out.push(event),
                },
                // The virtual device does not advertise MSC/ABS/etc.
                _ => {}
            }
        }

        let scroll = if have_wheel_hi {
            wheel_hi
        } else if have_wheel_lo {
            wheel_lo * SCROLL_UNITS_PER_TICK
        } else {
            0
        };
        if scroll != 0 {
            self.apply_scroll(scroll, &mut out);
        }

        out
    }

    fn handle_key(&mut self, code: KeyCode, value: i32, out: &mut Vec<InputEvent>) {
        match value {
            1 => {
                if self.active.contains_key(&code) {
                    // A duplicate press after a resync; the button is already
                    // in the desired state.
                    return;
                }
                match self.buttons.get(&code).cloned() {
                    Some(ResolvedAction::Emit(keys)) => {
                        for key in &keys {
                            self.output.press(*key, out);
                        }
                        self.active.insert(code, Active::Emit(keys));
                    }
                    Some(ResolvedAction::DragLock { button }) => {
                        if self.drag_locked.remove(&code).is_some() {
                            self.output.release(button, out);
                            info!("drag lock released");
                        } else {
                            self.output.press(button, out);
                            self.drag_locked.insert(code, button);
                            info!("drag lock engaged");
                        }
                        self.active.insert(code, Active::DragLock);
                    }
                    Some(ResolvedAction::Exec(command)) => {
                        spawn_command(&command);
                        self.active.insert(code, Active::Exec);
                    }
                    Some(ResolvedAction::None) => {
                        self.active.insert(code, Active::None);
                    }
                    None => self.output.press(code, out),
                }
            }
            0 => {
                if let Some(active) = self.active.remove(&code) {
                    if let Active::Emit(keys) = active {
                        for key in keys.iter().rev() {
                            self.output.release(*key, out);
                        }
                    }
                } else {
                    self.output.release(code, out);
                }
            }
            2 if !self.buttons.contains_key(&code) => {
                out.push(evdev::KeyEvent::new_now(code, 2).into());
            }
            _ => {}
        }
    }

    fn apply_scroll(&mut self, hi_res_delta: i64, out: &mut Vec<InputEvent>) {
        let sign = if self.scroll_natural { -1.0 } else { 1.0 };
        self.scroll_accumulator += hi_res_delta as f64 * self.scroll_speed * sign;

        let emit_hi = self.scroll_accumulator.trunc() as i64;
        self.scroll_accumulator -= emit_hi as f64;
        if emit_hi != 0 {
            self.scroll_total_hi_res += emit_hi;
            out.push(
                RelativeAxisEvent::new_now(RelativeAxisCode::REL_WHEEL_HI_RES, clamp_i32(emit_hi))
                    .into(),
            );
        }

        // Keep the classic wheel in sync with the high-resolution stream.
        let desired_low = self.scroll_total_hi_res / SCROLL_UNITS_PER_TICK;
        let low_delta = desired_low - self.scroll_low_res_emitted;
        if low_delta != 0 {
            self.scroll_low_res_emitted = desired_low;
            out.push(
                RelativeAxisEvent::new_now(RelativeAxisCode::REL_WHEEL, clamp_i32(low_delta))
                    .into(),
            );
        }
    }

    /// Release every button this engine is holding (drag locks included).
    pub fn release_all(&mut self, out: &mut Vec<InputEvent>) {
        for (_, button) in std::mem::take(&mut self.drag_locked) {
            self.output.release(button, out);
        }
        self.output.release_all(out);
        self.active.clear();
    }

    #[cfg(test)]
    fn output_is_empty(&self) -> bool {
        self.output.is_empty() && self.drag_locked.is_empty()
    }
}

fn clamp_i32(value: i64) -> i32 {
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

fn spawn_command(command: &str) {
    let child = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match child {
        Ok(mut child) => {
            info!("started {:?} (pid {})", command, child.id());
            let command = command.to_string();
            std::thread::spawn(move || {
                if let Err(err) = child.wait() {
                    warn!("command {:?} failed: {err}", command);
                }
            });
        }
        Err(err) => error!("cannot start {:?}: {err}", command),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use evdev::{KeyEvent, RelativeAxisCode};
    use std::str::FromStr;

    fn engine(config_toml: &str) -> Engine {
        let mut config: Config = toml::from_str(config_toml).unwrap();
        config.normalize();
        let resolved = config.resolve().unwrap();
        Engine::new(&resolved)
    }

    fn key(code: KeyCode, value: i32) -> InputEvent {
        KeyEvent::new(code, value).into()
    }

    fn frame(events: &[InputEvent]) -> Vec<InputEvent> {
        events.to_vec()
    }

    #[test]
    fn default_buttons_pass_through() {
        let mut engine = engine("[buttons]\n");
        let out = engine.process_frame(&frame(&[key(KeyCode::BTN_LEFT, 1)]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].event_type(), EventType::KEY);
        assert_eq!(out[0].code(), KeyCode::BTN_LEFT.0);
        assert_eq!(out[0].value(), 1);

        let out = engine.process_frame(&frame(&[key(KeyCode::BTN_LEFT, 0)]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].value(), 0);
        assert!(engine.output_is_empty());
    }

    #[test]
    fn remaps_button_to_key_combo() {
        let mut engine = engine(
            r#"
            [buttons.side]
            action = "keys"
            keys = ["ctrl", "z"]
            "#,
        );
        let out = engine.process_frame(&frame(&[key(KeyCode::BTN_SIDE, 1)]));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].code(), KeyCode::KEY_LEFTCTRL.0);
        assert_eq!(out[1].code(), KeyCode::KEY_Z.0);

        let out = engine.process_frame(&frame(&[key(KeyCode::BTN_SIDE, 0)]));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].code(), KeyCode::KEY_Z.0);
        assert_eq!(out[1].code(), KeyCode::KEY_LEFTCTRL.0);
        assert!(engine.output_is_empty());
    }

    #[test]
    fn disabled_button_is_suppressed() {
        let mut engine = engine(
            r#"
            [buttons.side]
            action = "none"
            "#,
        );
        assert!(
            engine
                .process_frame(&frame(&[key(KeyCode::BTN_SIDE, 1)]))
                .is_empty()
        );
        assert!(
            engine
                .process_frame(&frame(&[key(KeyCode::BTN_SIDE, 0)]))
                .is_empty()
        );
        assert!(engine.output_is_empty());
    }

    #[test]
    fn drag_lock_toggles() {
        let mut engine = engine(
            r#"
            [buttons.side]
            action = "drag_lock"
            button = "left"
            "#,
        );
        let out = engine.process_frame(&frame(&[key(KeyCode::BTN_SIDE, 1)]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].code(), KeyCode::BTN_LEFT.0);
        assert_eq!(out[0].value(), 1);
        // Release of the physical lock button does not release the drag.
        assert!(
            engine
                .process_frame(&frame(&[key(KeyCode::BTN_SIDE, 0)]))
                .is_empty()
        );
        assert!(!engine.output_is_empty());

        let out = engine.process_frame(&frame(&[key(KeyCode::BTN_SIDE, 1)]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].code(), KeyCode::BTN_LEFT.0);
        assert_eq!(out[0].value(), 0);
        assert!(engine.output_is_empty());
    }

    #[test]
    fn scroll_is_inverted_and_scaled() {
        let mut engine = engine(
            r#"
            [scroll]
            natural = true
            speed = 2.0
            "#,
        );
        let event: InputEvent = RelativeAxisEvent::new(RelativeAxisCode::REL_WHEEL, 1).into();
        let out = engine.process_frame(&frame(&[event]));
        assert!(out.iter().any(|e| {
            e.event_type() == EventType::RELATIVE
                && e.code() == RelativeAxisCode::REL_WHEEL_HI_RES.0
                && e.value() == -240
        }));
        assert!(out.iter().any(|e| {
            e.event_type() == EventType::RELATIVE
                && e.code() == RelativeAxisCode::REL_WHEEL.0
                && e.value() == -2
        }));
    }

    #[test]
    fn fractional_scroll_accumulates() {
        let mut engine = engine(
            r#"
            [scroll]
            speed = 0.5
            "#,
        );
        let event: InputEvent = RelativeAxisEvent::new(RelativeAxisCode::REL_WHEEL, 1).into();
        let out = engine.process_frame(&frame(&[event]));
        assert!(out.iter().any(|e| {
            e.event_type() == EventType::RELATIVE
                && e.code() == RelativeAxisCode::REL_WHEEL_HI_RES.0
                && e.value() == 60
        }));
        assert!(
            !out.iter().any(|e| e.event_type() == EventType::RELATIVE
                && e.code() == RelativeAxisCode::REL_WHEEL.0)
        );

        let out = engine.process_frame(&frame(&[event]));
        assert!(out.iter().any(|e| {
            e.event_type() == EventType::RELATIVE
                && e.code() == RelativeAxisCode::REL_WHEEL.0
                && e.value() == 1
        }));
    }

    #[test]
    fn key_names_are_parsed() {
        assert!(KeyCode::from_str("KEY_LEFTCTRL").is_ok());
    }
}
