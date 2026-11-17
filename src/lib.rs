//! Shared library for the SlimBlade command-line tool and GUI.
//!
//! The library contains configuration handling, device discovery, key-name
//! parsing, the evdev/uinput remapping daemon, and setup diagnostics. Both the
//! `slimblade` and `slimblade-gui` binaries build on these modules.

pub mod config;
pub mod daemon;
pub mod device;
pub mod doctor;
pub mod keys;
