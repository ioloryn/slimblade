//! Setup diagnostics and generated udev rule.

use crate::config::{Config, resolve_config_path};
use crate::device;
use anyhow::{Result, bail};
use evdev::Device;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

pub const UDEV_RULE: &str = include_str!("../packaging/70-slimblade.rules");

pub fn run(config_override: Option<&Path>) -> Result<()> {
    let mut problems = 0usize;
    let config_path = resolve_config_path(config_override)?;

    println!("slimblade doctor");
    println!("================");

    println!("\nConfiguration:");
    let config = if config_path.exists() {
        match Config::load(&config_path) {
            Ok(config) => {
                println!("  [ok] {} is valid", config_path.display());
                Some(config)
            }
            Err(err) => {
                problems += 1;
                println!("  [!!] {}: {err:#}", config_path.display());
                None
            }
        }
    } else {
        println!(
            "  [--] {} does not exist (using built-in defaults)",
            config_path.display()
        );
        println!("       create it with: slimblade init");
        Some(Config::default())
    };

    let config = config.unwrap_or_default();

    println!("\nDevice:");
    let info = match device::find_device(&config.device, None) {
        Ok(info) => {
            println!(
                "  [ok] \"{}\" [{}] at {}",
                info.name,
                info.id(),
                info.event_paths
                    .first()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            );
            Some(info)
        }
        Err(err) => {
            problems += 1;
            println!("  [!!] {err:#}");
            None
        }
    };

    println!("\nEvent device access:");
    if let Some(info) = &info {
        let path = info
            .event_paths
            .iter()
            .find(|path| device::is_pointer_device(path))
            .or_else(|| info.event_paths.first());
        match path {
            Some(path) => match Device::open(path) {
                Ok(device) => {
                    println!("  [ok] {} is readable", path.display());
                    if let Some(keys) = device.supported_keys() {
                        let names: Vec<String> = keys.iter().map(|k| format!("{k:?}")).collect();
                        println!("       keys: {}", names.join(", "));
                    }
                }
                Err(err) => {
                    problems += 1;
                    println!("  [!!] cannot read {}: {err}", path.display());
                    println!("       install the udev rule below, then replug the device");
                }
            },
            None => {
                problems += 1;
                println!("  [!!] the device has no event node");
            }
        }
    } else {
        problems += 1;
        println!("  [!!] skipped (no device)");
    }

    println!("\nVirtual device (/dev/uinput):");
    match OpenOptions::new().write(true).open("/dev/uinput") {
        Ok(_) => println!("  [ok] /dev/uinput is writable"),
        Err(err) => {
            problems += 1;
            println!("  [!!] /dev/uinput is not writable: {err}");
            println!("       most desktop sessions grant this through a uaccess ACL;");
            println!("       otherwise add your user to the input group or create a udev rule");
        }
    }

    println!("\nUdev rule:");
    if let Some(path) = installed_udev_rule() {
        println!("  [ok] installed at {}", path.display());
    } else if let Some(path) = legacy_udev_rule() {
        problems += 1;
        println!(
            "  [!!] {} is installed under a number that is too high",
            path.display()
        );
        println!("       rules adding TAG+=\"uaccess\" must load before 73-seat-late.rules,");
        println!("       otherwise the ACL is never applied");
        println!(
            "       rename it: sudo mv {} /etc/udev/rules.d/70-slimblade.rules",
            path.display()
        );
        println!("       sudo udevadm control --reload-rules && sudo udevadm trigger");
    } else {
        println!("  [--] not installed");
        println!(
            "       slimblade udev-rule | sudo tee /etc/udev/rules.d/70-slimblade.rules >/dev/null"
        );
        println!("       sudo udevadm control --reload-rules && sudo udevadm trigger");
        println!("       then replug the trackball");
    }

    println!("\nSystemd user service:");
    if let Some(path) = user_service_path()
        && path.exists()
    {
        println!("  [ok] {}", path.display());
    } else {
        println!("  [--] not installed (optional)");
        println!("       see packaging/slimblade.service in the source tree");
    }

    println!();
    if problems == 0 {
        println!("Everything looks usable. Start the daemon with: slimblade run");
        Ok(())
    } else {
        bail!("{problems} check(s) failed")
    }
}

pub fn installed_udev_rule() -> Option<PathBuf> {
    [
        PathBuf::from("/etc/udev/rules.d/70-slimblade.rules"),
        PathBuf::from("/run/udev/rules.d/70-slimblade.rules"),
        PathBuf::from("/usr/lib/udev/rules.d/70-slimblade.rules"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn legacy_udev_rule() -> Option<PathBuf> {
    [
        PathBuf::from("/etc/udev/rules.d/99-slimblade.rules"),
        PathBuf::from("/run/udev/rules.d/99-slimblade.rules"),
        PathBuf::from("/usr/lib/udev/rules.d/99-slimblade.rules"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn user_service_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .map(|dir| dir.join("systemd/user/slimblade.service"))
}
