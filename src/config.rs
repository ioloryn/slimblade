//! Configuration file handling and action resolution.

use crate::keys;
use anyhow::{Context, Result, bail};
use evdev::KeyCode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_TEMPLATE: &str = r#"# SlimBlade configuration.
#
# The daemon grabs the physical trackball and exposes a virtual pointer with
# these mappings applied. Button/scroll changes can be reloaded with SIGHUP;
# adding new key names requires a daemon restart.

[device]
# USB IDs of the trackball. The classic SlimBlade is 047d:2041.
vendor = 0x047d
product = 0x2041
# Match a substring of the evdev device name (optional).
# name = "Slimblade"
# Or point at an explicit event device (overrides the filters above).
# path = "/dev/input/event17"

[scroll]
# Invert the wheel direction. System-level natural scrolling still applies
# on top of this because the daemon works on the raw evdev stream.
natural = false
# Scroll speed multiplier. 1.0 = one wheel tick per detent.
speed = 1.0

[buttons.left]
action = "mouse"
button = "left"

[buttons.right]
action = "mouse"
button = "right"

[buttons.middle]
action = "mouse"
button = "middle"

# The small upper-right button. "side" is the browser-back button on Linux.
[buttons.side]
action = "mouse"
button = "side"

# Examples (uncomment to use):
#
# [buttons.middle]
# action = "drag_lock"
# button = "left"
#
# [buttons.side]
# action = "keys"
# keys = ["ctrl", "z"]
#
# [buttons.left]
# action = "exec"
# command = "notify-send 'SlimBlade' 'Hello'"
#
# Disable a button completely:
# [buttons.side]
# action = "none"
"#;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub device: DeviceConfig,
    pub scroll: ScrollConfig,
    pub buttons: BTreeMap<String, Action>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    /// USB vendor ID. `None` disables this filter.
    #[serde(default = "default_vendor")]
    pub vendor: Option<u16>,
    /// USB product ID. `None` disables this filter.
    #[serde(default)]
    pub product: Option<u16>,
    /// Case-insensitive substring of the evdev device name.
    #[serde(default)]
    pub name: Option<String>,
    /// Explicit event device path. Overrides all other filters when set.
    #[serde(default)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollConfig {
    /// Invert the wheel direction.
    pub natural: bool,
    /// Multiplier applied to wheel events.
    pub speed: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Emit a mouse button.
    Mouse { button: String },
    /// Press a keyboard shortcut while the physical button is held.
    Keys { keys: Vec<String> },
    /// Toggle a held mouse button (click to start dragging, click to stop).
    DragLock {
        #[serde(default = "default_drag_button")]
        button: String,
    },
    /// Run a shell command when the button is pressed.
    Exec { command: String },
    /// Ignore the button completely.
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedAction {
    Emit(Vec<KeyCode>),
    DragLock { button: KeyCode },
    Exec(String),
    None,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub config: Config,
    pub buttons: BTreeMap<KeyCode, ResolvedAction>,
    /// Every key code a mapping can emit.
    pub output_keys: BTreeSet<KeyCode>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device: DeviceConfig::default(),
            scroll: ScrollConfig::default(),
            buttons: default_buttons(),
        }
    }
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            vendor: Some(0x047d),
            product: None,
            name: Some("Slimblade".to_string()),
            path: None,
        }
    }
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            natural: false,
            speed: 1.0,
        }
    }
}

fn default_vendor() -> Option<u16> {
    Some(0x047d)
}

fn default_drag_button() -> String {
    "left".to_string()
}

fn default_buttons() -> BTreeMap<String, Action> {
    let mut buttons = BTreeMap::new();
    for (name, button) in [
        ("left", "left"),
        ("right", "right"),
        ("middle", "middle"),
        ("side", "side"),
    ] {
        buttons.insert(
            name.to_string(),
            Action::Mouse {
                button: button.to_string(),
            },
        );
    }
    buttons
}

impl Config {
    /// Load and validate a config file.
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read config {}", path.display()))?;
        let mut config: Config = toml::from_str(&text)
            .with_context(|| format!("cannot parse config {}", path.display()))?;
        config.normalize();
        config
            .validate()
            .with_context(|| format!("invalid config {}", path.display()))?;
        Ok(config)
    }

    /// Fill in default button mappings and normalize values.
    pub(crate) fn normalize(&mut self) {
        // A config that only customizes one button still gets sensible
        // defaults for the others.
        for (name, action) in default_buttons() {
            self.buttons.entry(name).or_insert(action);
        }
        if self
            .device
            .path
            .as_ref()
            .is_some_and(|p| p.as_os_str().is_empty())
        {
            self.device.path = None;
        }
        if self
            .device
            .name
            .as_deref()
            .is_some_and(|n| n.trim().is_empty())
        {
            self.device.name = None;
        }
    }

    /// Validate values that serde cannot check.
    pub fn validate(&self) -> Result<()> {
        if !self.scroll.speed.is_finite() || self.scroll.speed <= 0.0 {
            bail!("scroll.speed must be a positive finite number");
        }
        if self.scroll.speed > 100.0 {
            bail!("scroll.speed is unreasonably large (maximum 100)");
        }
        if self.buttons.is_empty() {
            bail!("at least one button mapping is required");
        }
        self.resolve()
            .map(|_| ())
            .context("button mapping is invalid")
    }

    /// Write a validated configuration to disk.
    ///
    /// The GUI uses this to persist structured changes. The file is rewritten
    /// from the parsed values, so comments are not preserved.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let text = toml::to_string_pretty(self).context("cannot serialize the configuration")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::write(path, text).with_context(|| format!("cannot write config {}", path.display()))?;
        Ok(())
    }

    /// Resolve names to evdev key codes.
    pub fn resolve(&self) -> Result<ResolvedConfig> {
        let mut buttons = BTreeMap::new();
        let mut output_keys = BTreeSet::new();
        let mut source_names: BTreeMap<KeyCode, String> = BTreeMap::new();

        for (name, action) in &self.buttons {
            let physical = keys::parse_physical_button(name)
                .map_err(|e| anyhow::anyhow!("buttons.{name}: {e}"))?;
            if let Some(previous) = source_names.insert(physical, name.clone()) {
                bail!(
                    "buttons.{previous} and buttons.{name} both refer to the same physical button {}",
                    keys::friendly_name(physical)
                );
            }

            let resolved = match action {
                Action::Mouse { button } => {
                    let code = keys::parse_mouse_button(button).map_err(|e| {
                        anyhow::anyhow!("buttons.{name}: invalid output mouse button: {e}")
                    })?;
                    output_keys.insert(code);
                    ResolvedAction::Emit(vec![code])
                }
                Action::Keys { keys: names } => {
                    if names.is_empty() {
                        bail!("buttons.{name}: keys must not be empty");
                    }
                    let mut codes = Vec::with_capacity(names.len());
                    for key in names {
                        let code = keys::parse_key(key).map_err(|e| {
                            anyhow::anyhow!("buttons.{name}: invalid key {key:?}: {e}")
                        })?;
                        output_keys.insert(code);
                        codes.push(code);
                    }
                    ResolvedAction::Emit(codes)
                }
                Action::DragLock { button } => {
                    let code = keys::parse_mouse_button(button).map_err(|e| {
                        anyhow::anyhow!("buttons.{name}: invalid drag-lock button: {e}")
                    })?;
                    if !keys::is_mouse_button(code) {
                        bail!("buttons.{name}: drag_lock needs a mouse button, got {button:?}");
                    }
                    output_keys.insert(code);
                    ResolvedAction::DragLock { button: code }
                }
                Action::Exec { command } => {
                    if command.trim().is_empty() {
                        bail!("buttons.{name}: exec command must not be empty");
                    }
                    ResolvedAction::Exec(command.clone())
                }
                Action::None => ResolvedAction::None,
            };

            buttons.insert(physical, resolved);
        }

        Ok(ResolvedConfig {
            config: self.clone(),
            buttons,
            output_keys,
        })
    }

    /// Write the annotated default template.
    pub fn write_template(path: &Path, force: bool) -> Result<()> {
        if path.exists() && !force {
            bail!(
                "{} already exists (use --force to overwrite)",
                path.display()
            );
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::write(path, DEFAULT_TEMPLATE)
            .with_context(|| format!("cannot write {}", path.display()))?;
        Ok(())
    }
}

/// `$XDG_CONFIG_HOME/slimblade/config.toml`, falling back to `~/.config`.
pub fn default_config_path() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME")
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir).join("slimblade/config.toml"));
    }
    if let Some(home) = env::var_os("HOME")
        && !home.is_empty()
    {
        return Ok(PathBuf::from(home).join(".config/slimblade/config.toml"));
    }
    bail!("cannot determine config directory: set XDG_CONFIG_HOME or HOME")
}

pub fn resolve_config_path(explicit: Option<&Path>) -> Result<PathBuf> {
    match explicit {
        Some(path) => Ok(path.to_path_buf()),
        None => default_config_path(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses() {
        let mut config: Config = toml::from_str(DEFAULT_TEMPLATE).unwrap();
        config.normalize();
        config.validate().unwrap();
    }

    #[test]
    fn defaults_and_partial_override() {
        let mut config: Config = toml::from_str(
            r#"
            [buttons.side]
            action = "keys"
            keys = ["ctrl", "z"]
            "#,
        )
        .unwrap();
        config.normalize();
        let resolved = config.resolve().unwrap();
        assert_eq!(
            resolved.buttons.get(&KeyCode::BTN_LEFT),
            Some(&ResolvedAction::Emit(vec![KeyCode::BTN_LEFT]))
        );
        assert_eq!(
            resolved.buttons.get(&KeyCode::BTN_SIDE),
            Some(&ResolvedAction::Emit(vec![
                KeyCode::KEY_LEFTCTRL,
                KeyCode::KEY_Z
            ]))
        );
        assert!(resolved.output_keys.contains(&KeyCode::KEY_Z));
    }

    #[test]
    fn duplicate_physical_buttons_are_rejected() {
        let mut config: Config = toml::from_str(
            r#"
            [buttons.side]
            action = "none"
            [buttons.back]
            action = "none"
            "#,
        )
        .unwrap();
        config.normalize();
        assert!(config.validate().is_err());
    }

    #[test]
    fn partial_device_table_does_not_inherit_name_filter() {
        let config: Config = toml::from_str(
            r#"
            [device]
            product = 0x2044
            "#,
        )
        .unwrap();
        assert_eq!(config.device.vendor, Some(0x047d));
        assert_eq!(config.device.product, Some(0x2044));
        assert_eq!(config.device.name, None);
    }

    #[test]
    fn bad_scroll_speed_is_rejected() {
        let mut config: Config = toml::from_str("[scroll]\nspeed = 0.0\n").unwrap();
        config.normalize();
        assert!(config.validate().is_err());
    }
}
