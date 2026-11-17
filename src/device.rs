//! Linux input-device discovery and inspection.

use crate::config::DeviceConfig;
use anyhow::{Context, Result, bail};
use evdev::{Device, KeyCode, RelativeAxisCode};
use std::fs;
use std::path::{Path, PathBuf};

pub const PERMISSION_HINT: &str = "grant access with the udev rule from packaging/70-slimblade.rules \
(or add your user to the input group), then replug the trackball or run \
`sudo udevadm control --reload-rules && sudo udevadm trigger`";

/// Name suffix used by the virtual device created by the daemon.
pub const MANAGED_MARKER: &str = "(managed)";

pub fn is_managed(info: &InputDeviceInfo) -> bool {
    info.name.contains(MANAGED_MARKER)
}

#[derive(Debug, Clone)]
pub struct InputDeviceInfo {
    pub bus: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    pub name: String,
    pub phys: Option<String>,
    #[allow(dead_code)]
    pub sysfs: Option<String>,
    pub event_paths: Vec<PathBuf>,
}

impl InputDeviceInfo {
    pub fn id(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor, self.product)
    }

    pub fn bus_name(&self) -> &'static str {
        match self.bus {
            0x03 => "usb",
            0x05 => "bluetooth",
            0x06 => "virtual",
            0x11 => "i8042",
            _ => "other",
        }
    }
}

/// Parse `/proc/bus/input/devices`.
pub fn list_input_devices() -> Result<Vec<InputDeviceInfo>> {
    let text = fs::read_to_string("/proc/bus/input/devices")
        .context("cannot read /proc/bus/input/devices")?;
    Ok(parse_proc_input_devices(&text))
}

pub fn parse_proc_input_devices(text: &str) -> Vec<InputDeviceInfo> {
    let mut out = Vec::new();
    let mut bus = None;
    let mut vendor = None;
    let mut product = None;
    let mut version = None;
    let mut name = None;
    let mut phys = None;
    let mut sysfs = None;
    let mut event_paths = Vec::new();

    let mut flush = |bus: &mut Option<u16>,
                     vendor: &mut Option<u16>,
                     product: &mut Option<u16>,
                     version: &mut Option<u16>,
                     name: &mut Option<String>,
                     phys: &mut Option<String>,
                     sysfs: &mut Option<String>,
                     event_paths: &mut Vec<PathBuf>| {
        if let (Some(bus), Some(vendor), Some(product), Some(name)) =
            (bus.take(), vendor.take(), product.take(), name.take())
        {
            if !event_paths.is_empty() {
                out.push(InputDeviceInfo {
                    bus,
                    vendor,
                    product,
                    version: version.take().unwrap_or_default(),
                    name,
                    phys: phys.take(),
                    sysfs: sysfs.take(),
                    event_paths: std::mem::take(event_paths),
                });
            }
        } else {
            bus.take();
            vendor.take();
            product.take();
            version.take();
            name.take();
            *phys = None;
            *sysfs = None;
            event_paths.clear();
        }
    };

    for line in text.lines() {
        if line.trim().is_empty() {
            flush(
                &mut bus,
                &mut vendor,
                &mut product,
                &mut version,
                &mut name,
                &mut phys,
                &mut sysfs,
                &mut event_paths,
            );
            continue;
        }
        if let Some(value) = line.strip_prefix("I:") {
            for field in value.split_whitespace() {
                if let Some(v) = field.strip_prefix("Bus=") {
                    bus = u16::from_str_radix(v, 16).ok();
                } else if let Some(v) = field.strip_prefix("Vendor=") {
                    vendor = u16::from_str_radix(v, 16).ok();
                } else if let Some(v) = field.strip_prefix("Product=") {
                    product = u16::from_str_radix(v, 16).ok();
                } else if let Some(v) = field.strip_prefix("Version=") {
                    version = u16::from_str_radix(v, 16).ok();
                }
            }
        } else if let Some(value) = line.strip_prefix("N: Name=") {
            name = Some(unquote(value));
        } else if let Some(value) = line.strip_prefix("P: Phys=") {
            phys = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("S: Sysfs=") {
            sysfs = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("H: Handlers=") {
            event_paths = value
                .split_whitespace()
                .filter(|h| h.starts_with("event"))
                .map(|h| PathBuf::from("/dev/input").join(h))
                .collect();
        }
    }
    flush(
        &mut bus,
        &mut vendor,
        &mut product,
        &mut version,
        &mut name,
        &mut phys,
        &mut sysfs,
        &mut event_paths,
    );

    out
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        inner.replace("\\\"", "\"")
    } else {
        value.to_string()
    }
}

pub fn matches(info: &InputDeviceInfo, cfg: &DeviceConfig) -> bool {
    if let Some(vendor) = cfg.vendor
        && info.vendor != vendor
    {
        return false;
    }
    if let Some(product) = cfg.product
        && info.product != product
    {
        return false;
    }
    if let Some(name) = cfg.name.as_deref() {
        let needle = name.trim().to_lowercase();
        if !info.name.to_lowercase().contains(&needle) {
            return false;
        }
    }
    true
}

/// Find the event device described by the config.
pub fn find_device(cfg: &DeviceConfig, explicit: Option<&Path>) -> Result<InputDeviceInfo> {
    if let Some(path) = explicit.or(cfg.path.as_deref()) {
        if let Some(info) = list_input_devices()?
            .into_iter()
            .find(|info| info.event_paths.iter().any(|p| p == path))
        {
            return Ok(info);
        }

        // Allow arbitrary event nodes (for example a test uinput device) even
        // when they are not visible in /proc.
        let device = open_event_device(path)?;
        let input_id = device.input_id();
        let name = device.name().unwrap_or("unknown").to_string();
        return Ok(InputDeviceInfo {
            bus: input_id.bus_type().0,
            vendor: input_id.vendor(),
            product: input_id.product(),
            version: input_id.version(),
            name,
            phys: None,
            sysfs: None,
            event_paths: vec![path.to_path_buf()],
        });
    }

    let all = list_input_devices()?;
    let candidates: Vec<_> = all
        .iter()
        .filter(|info| !is_managed(info) && matches(info, cfg))
        .cloned()
        .collect();

    match candidates.as_slice() {
        [] => {
            let filters = describe_filters(cfg);
            bail!(
                "no matching input device found ({filters}).\n\
                 Run `slimblade list --all` to see pointer devices, or set device.path in the config."
            );
        }
        [one] => Ok(one.clone()),
        many => {
            let mut message = format!(
                "{} devices match {}; select one with --device or device.path:\n",
                many.len(),
                describe_filters(cfg)
            );
            for info in many {
                message.push_str(&format!(
                    "  {}  \"{}\"  [{}]\n",
                    info.event_paths
                        .first()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    info.name,
                    info.id()
                ));
            }
            bail!(message.trim_end().to_string());
        }
    }
}

fn describe_filters(cfg: &DeviceConfig) -> String {
    let mut parts = Vec::new();
    if let Some(vendor) = cfg.vendor {
        parts.push(format!("vendor={vendor:04x}"));
    }
    if let Some(product) = cfg.product {
        parts.push(format!("product={product:04x}"));
    }
    if let Some(name) = &cfg.name {
        parts.push(format!("name contains {name:?}"));
    }
    if parts.is_empty() {
        "any device".to_string()
    } else {
        parts.join(", ")
    }
}

/// Open an event device, adding an actionable permission hint on failure.
pub fn open_event_device(path: &Path) -> Result<Device> {
    Device::open(path).with_context(|| {
        format!(
            "cannot open {}; if this is a permission error, {PERMISSION_HINT}",
            path.display()
        )
    })
}

/// Best effort description of a device's capabilities, without failing when
/// the event node is not readable.
pub fn describe_capabilities(path: &Path) -> String {
    match Device::open(path) {
        Ok(device) => {
            let mut parts = Vec::new();
            if let Some(keys) = device.supported_keys() {
                let names: Vec<String> = keys.iter().map(keys_friendly).collect();
                if !names.is_empty() {
                    parts.push(format!("buttons/keys: {}", names.join(", ")));
                }
            }
            if let Some(axes) = device.supported_relative_axes() {
                let names: Vec<String> = axes.iter().map(|a| format!("{a:?}")).collect();
                if !names.is_empty() {
                    parts.push(format!("relative axes: {}", names.join(", ")));
                }
            }
            if parts.is_empty() {
                "no mouse capabilities reported".to_string()
            } else {
                parts.join("; ")
            }
        }
        Err(err) => format!("capabilities unavailable ({err})"),
    }
}

fn keys_friendly(code: KeyCode) -> String {
    let name = crate::keys::friendly_name(code);
    if name.starts_with("BTN_") || name.starts_with("KEY_") {
        name
    } else {
        format!("{name} ({code:?})")
    }
}

/// List pointer devices using sysfs capability bitmaps. This works without
/// permission to open the event node.
pub fn is_pointer_device(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let rel = format!("/sys/class/input/{file_name}/device/capabilities/rel");
    let Ok(text) = fs::read_to_string(rel) else {
        return false;
    };
    bitmap_has(&text, RelativeAxisCode::REL_X.0 as usize)
        && bitmap_has(&text, RelativeAxisCode::REL_Y.0 as usize)
}

fn bitmap_has(text: &str, bit: usize) -> bool {
    // sysfs prints words most-significant first; the last word holds bits 0..63.
    let words: Vec<u64> = text
        .split_whitespace()
        .filter_map(|w| u64::from_str_radix(w, 16).ok())
        .collect();
    if words.is_empty() {
        return false;
    }
    let word_from_end = bit / 64;
    if word_from_end >= words.len() {
        return false;
    }
    let word = words[words.len() - 1 - word_from_end];
    word & (1u64 << (bit % 64)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROC_SAMPLE: &str = r#"I: Bus=0003 Vendor=047d Product=2041 Version=0110
N: Name="Kensington Slimblade Trackball"
P: Phys=usb-0000:09:00.3-4.2.3/input0
S: Sysfs=/devices/pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.0/0003:047D:2041.0025/input/input42
U: Uniq=
H: Handlers=mouse2 event17
B: PROP=0
B: EV=17
B: KEY=f0000 0 0 0 0
B: REL=903

I: Bus=0011 Vendor=0001 Product=0001 Version=ab41
N: Name="AT Translated Set 2 keyboard"
P: Phys=isa0060/serio0/input0
S: Sysfs=/devices/platform/i8042/serio0/input/input3
U: Uniq=
H: Handlers=sysrq kbd event3 leds
B: PROP=0
B: EV=120013
"#;

    #[test]
    fn parses_proc_devices() {
        let devices = parse_proc_input_devices(PROC_SAMPLE);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].vendor, 0x047d);
        assert_eq!(devices[0].product, 0x2041);
        assert_eq!(devices[0].name, "Kensington Slimblade Trackball");
        assert_eq!(
            devices[0].event_paths,
            vec![PathBuf::from("/dev/input/event17")]
        );
        assert_eq!(devices[1].event_paths[0], Path::new("/dev/input/event3"));
    }

    #[test]
    fn bitmap_parsing() {
        // REL_X (0) and REL_Y (1) in the least-significant word.
        assert!(bitmap_has("3", 0));
        assert!(bitmap_has("3", 1));
        assert!(!bitmap_has("3", 2));
        assert!(bitmap_has("1 0", 64));
    }
}
