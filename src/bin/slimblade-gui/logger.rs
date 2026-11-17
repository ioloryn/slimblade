//! In-memory log buffer shared by the GUI and the daemon.

use log::{LevelFilter, Log, Metadata, Record};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const MAX_LINES: usize = 400;

#[derive(Clone, Default)]
pub struct LogBuffer {
    lines: Arc<Mutex<VecDeque<String>>>,
}

impl LogBuffer {
    pub fn push(&self, line: String) {
        let mut lines = self.lines.lock().expect("log buffer poisoned");
        if lines.len() == MAX_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines
            .lock()
            .expect("log buffer poisoned")
            .iter()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.lines.lock().expect("log buffer poisoned").clear();
    }
}

struct GuiLogger {
    buffer: LogBuffer,
}

impl Log for GuiLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
            && (metadata.target().starts_with("slimblade")
                || metadata.target().starts_with("slimblade_gui"))
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("[{}] {}", record.level(), record.args());
        self.buffer.push(line.clone());
        eprintln!("{line}");
    }

    fn flush(&self) {}
}

/// Install the GUI logger once and return the shared buffer.
pub fn init() -> LogBuffer {
    let buffer = LogBuffer::default();
    let logger = GuiLogger {
        buffer: buffer.clone(),
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
    buffer
}
