use anyhow::{Context, Result, bail};
use clap::{ArgAction, Parser, Subcommand};
use evdev::{EventType, KeyCode, RelativeAxisCode};
use slimblade::config::{self, Config, ResolvedAction};
use slimblade::{daemon, device, doctor, keys};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "slimblade",
    version,
    about = "Manage Kensington SlimBlade buttons and scrolling on Linux",
    long_about = "A userspace remapper for Kensington SlimBlade trackballs.\n\n\
        The daemon grabs the physical device with evdev, applies the mappings from\n\
        a TOML config, and exposes the result as a virtual uinput pointer. The\n\
        classic SlimBlade has no vendor HID configuration interface, so all\n\
        settings are applied in software."
)]
struct Cli {
    /// Configuration file (default: $XDG_CONFIG_HOME/slimblade/config.toml)
    #[arg(short, long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Increase log verbosity (-v: info, -vv: debug)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List compatible input devices
    List {
        /// Show every pointer device, not just devices matching the config
        #[arg(long)]
        all: bool,
    },

    /// Print raw events from the trackball (useful for identifying buttons)
    Monitor {
        /// Explicit /dev/input/eventN path
        #[arg(short, long, value_name = "DEVICE")]
        device: Option<PathBuf>,

        /// Print unmodified evdev events
        #[arg(long)]
        raw: bool,
    },

    /// Grab the trackball and apply the configured mappings
    Run {
        /// Explicit /dev/input/eventN path
        #[arg(short, long, value_name = "DEVICE")]
        device: Option<PathBuf>,
    },

    /// Validate the configuration file
    Check,

    /// Create a commented default configuration file
    Init {
        /// Overwrite an existing file
        #[arg(long)]
        force: bool,

        /// Print the template to stdout instead of writing it
        #[arg(long)]
        stdout: bool,
    },

    /// Check device access, /dev/uinput and setup
    Doctor,

    /// Print the udev rule that grants access to the trackball
    UdevRule,
}

fn main() {
    // Rust ignores SIGPIPE, which makes `slimblade ... | head` panic on a
    // closed pipe. Restore the usual Unix behaviour for a CLI tool.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let cli = Cli::parse();
    init_logging(cli.verbose, matches!(cli.command, Command::Run { .. }));

    if let Err(err) = real_main(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn real_main(cli: Cli) -> Result<()> {
    match cli.command {
        Command::List { all } => cmd_list(all, cli.config.as_deref()),
        Command::Monitor { device, raw } => cmd_monitor(device, raw, cli.config.as_deref()),
        Command::Run { device } => {
            let path = config::resolve_config_path(cli.config.as_deref())?;
            daemon::run(path, device)
        }
        Command::Check => cmd_check(cli.config.as_deref()),
        Command::Init { force, stdout } => cmd_init(force, stdout, cli.config.as_deref()),
        Command::Doctor => doctor::run(cli.config.as_deref()),
        Command::UdevRule => {
            print!("{}", doctor::UDEV_RULE);
            Ok(())
        }
    }
}

fn init_logging(verbose: u8, run_command: bool) {
    let default = if verbose >= 2 {
        "debug"
    } else if verbose >= 1 || run_command {
        "info"
    } else {
        "warn"
    };
    let env = env_logger::Env::default().default_filter_or(default);
    let mut builder = env_logger::Builder::from_env(env);
    builder.format_timestamp_secs();
    if builder.try_init().is_err() {
        // A logger is already installed (for example by a test harness).
    }
}

fn load_config_or_default(explicit: Option<&Path>) -> Result<Config> {
    let path = config::resolve_config_path(explicit)?;
    if path.exists() {
        Config::load(&path)
    } else if explicit.is_some() {
        bail!("config {} does not exist", path.display())
    } else {
        Ok(Config::default())
    }
}

fn cmd_list(all: bool, config_override: Option<&Path>) -> Result<()> {
    let config = load_config_or_default(config_override)?;
    let devices = device::list_input_devices()?;

    let mut shown = 0usize;
    for info in devices {
        let selected = if all {
            info.event_paths
                .iter()
                .any(|path| device::is_pointer_device(path))
        } else {
            !device::is_managed(&info) && device::matches(&info, &config.device)
        };
        if !selected {
            continue;
        }
        shown += 1;

        let path = info
            .event_paths
            .iter()
            .find(|path| device::is_pointer_device(path))
            .or_else(|| info.event_paths.first());
        let path_text = path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<no event node>".to_string());

        println!("{path_text}  \"{}\"", info.name);
        println!(
            "    id {} ({})   phys {}",
            info.id(),
            info.bus_name(),
            info.phys.as_deref().unwrap_or("-")
        );
        if let Some(path) = path {
            println!("    {}", device::describe_capabilities(path));
        }
        println!();
    }

    if shown == 0 {
        if all {
            println!("No pointer devices found in /proc/bus/input/devices.");
        } else {
            println!(
                "No device matches configured filters (vendor {:?}, product {:?}, name {:?}).",
                config.device.vendor, config.device.product, config.device.name
            );
            println!("Run `slimblade list --all` to inspect pointer devices.");
        }
    }
    Ok(())
}

fn cmd_monitor(
    device_override: Option<PathBuf>,
    raw: bool,
    config_override: Option<&Path>,
) -> Result<()> {
    let config = load_config_or_default(config_override)?;
    let info = device::find_device(&config.device, device_override.as_deref())?;
    let path = info
        .event_paths
        .iter()
        .find(|path| device::is_pointer_device(path))
        .or_else(|| info.event_paths.first())
        .cloned()
        .with_context(|| format!("\"{}\" has no event device", info.name))?;

    println!(
        "Monitoring \"{}\" [{}] at {}",
        info.name,
        info.id(),
        path.display()
    );
    println!("Press buttons, turn the scroll ring, or move the ball. Ctrl+C to stop.");
    if !raw {
        println!("(high-resolution wheel events are hidden; use --raw to see everything)");
    }
    println!();

    let mut device = device::open_event_device(&path)?;
    loop {
        let events = device
            .fetch_events()
            .with_context(|| format!("cannot read events from {}", path.display()))?;
        for event in events {
            if raw {
                println!("{event:?}");
                continue;
            }
            match event.event_type() {
                EventType::KEY => {
                    let code = KeyCode(event.code());
                    let state = match event.value() {
                        0 => "release",
                        1 => "press",
                        2 => "repeat",
                        _ => "unknown",
                    };
                    let friendly = keys::friendly_name(code);
                    if friendly.starts_with("BTN_") || friendly.starts_with("KEY_") {
                        println!("key {friendly} {state}");
                    } else {
                        println!("button {friendly} ({code:?}) {state}");
                    }
                }
                EventType::RELATIVE => {
                    let code = RelativeAxisCode(event.code());
                    match code {
                        RelativeAxisCode::REL_WHEEL => {
                            let direction = if event.value() > 0 {
                                "up"
                            } else if event.value() < 0 {
                                "down"
                            } else {
                                "zero"
                            };
                            println!("wheel {direction} (REL_WHEEL {:+})", event.value());
                        }
                        RelativeAxisCode::REL_WHEEL_HI_RES => {
                            // The kernel emits this alongside REL_WHEEL on
                            // most devices; keep the default view readable.
                        }
                        RelativeAxisCode::REL_X => {
                            println!("motion x {:+}", event.value());
                        }
                        RelativeAxisCode::REL_Y => {
                            println!("motion y {:+}", event.value());
                        }
                        _ => {
                            println!("relative {:?} {:+}", code, event.value());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn cmd_check(config_override: Option<&Path>) -> Result<()> {
    let path = config::resolve_config_path(config_override)?;
    let config = Config::load(&path)?;
    let resolved = config.resolve()?;

    println!("Config OK: {}", path.display());
    println!(
        "Device filter: vendor={} product={} name={:?} path={:?}",
        config
            .device
            .vendor
            .map(|v| format!("0x{v:04x}"))
            .unwrap_or_else(|| "any".to_string()),
        config
            .device
            .product
            .map(|p| format!("0x{p:04x}"))
            .unwrap_or_else(|| "any".to_string()),
        config.device.name,
        config.device.path
    );
    println!(
        "Scroll: speed={} natural={}",
        config.scroll.speed, config.scroll.natural
    );
    println!("Buttons:");
    for (physical, action) in &resolved.buttons {
        println!(
            "  {:<8} -> {}",
            keys::friendly_name(*physical),
            describe_resolved(action)
        );
    }
    Ok(())
}

fn describe_resolved(action: &ResolvedAction) -> String {
    match action {
        ResolvedAction::Emit(keys) => {
            let names: Vec<String> = keys.iter().map(|k| format!("{k:?}")).collect();
            format!("emit {}", names.join(" + "))
        }
        ResolvedAction::DragLock { button } => {
            format!("toggle drag lock for {button:?}")
        }
        ResolvedAction::Exec(command) => format!("exec {command:?}"),
        ResolvedAction::None => "disabled".to_string(),
    }
}

fn cmd_init(force: bool, stdout: bool, config_override: Option<&Path>) -> Result<()> {
    if stdout {
        print!("{}", config::DEFAULT_TEMPLATE);
        return Ok(());
    }

    let path = config::resolve_config_path(config_override)?;
    Config::write_template(&path, force)?;
    println!("Wrote {}", path.display());
    println!();
    println!("Next steps:");
    println!("  slimblade check          # validate the file");
    println!("  slimblade list           # find the trackball");
    println!("  slimblade monitor        # identify buttons");
    println!("  slimblade run            # apply the mappings");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
