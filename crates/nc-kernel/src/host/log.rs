//! `log`: a line a plugin writes, kept in its own bounded ring.
//!
//! **Given to every plugin, and reaching nothing but that plugin's ring**
//! (`catalog/capabilities.toml`, `NOCTUA.md §15.4`). A plugin's text is
//! whatever its author or its input put there, so it never reaches the central
//! log, whose type wall exists to keep text out: it stays here, in a ring of
//! `log_ring_lines` lines per plugin, each cut at a character boundary to
//! `log_line_bytes`, the oldest overwritten and counted. A character a reader
//! sees nothing of — a control, a separator, a bidirectional override, a
//! variation selector or a filler (`nc_schema::unseen`) — is replaced by
//! `U+FFFD`, so a line read back later cannot rearrange the lines around it,
//! nor carry what its reader cannot see.
//!
//! **The ring is the plugin's slot's, not its instance's.** The kernel keeps
//! one per plugin it loaded and hands it to every instance of that plugin, so
//! the lines a plugin wrote before it was killed are still there to read once
//! it is started again, through `crate::Kernel::plugin_log`.
//!
//! What this answers: a plugin cannot fill the kernel's memory with log text,
//! and cannot write into the central log at all. What it does not answer:
//! what is in a line, which the plugin chose.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use nc_catalog::KERNEL;

use super::HostState;
use crate::Environment;
use crate::bindings::noctua::kernel::log as world;

/// How much a line says about how bad it is, as the world's `level` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Very fine detail.
    Trace,
    /// Detail a developer wants while diagnosing.
    Debug,
    /// Worth a line in normal operation.
    Info,
    /// Degraded, but working.
    Warn,
    /// Failed.
    Error,
}

impl LogLevel {
    /// The world's own word for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// One line a plugin wrote to its own log, as the kernel kept it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    level: LogLevel,
    text: String,
}

impl LogLine {
    /// How bad the plugin said it was.
    #[must_use]
    pub const fn level(&self) -> LogLevel {
        self.level
    }

    /// What it said, cut to `log_line_bytes` and every character a reader
    /// sees nothing of replaced.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// What the kernel keeps of one plugin's log: its newest lines, oldest first,
/// and how many older ones were let go to make room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLog {
    lines: Vec<LogLine>,
    dropped: u64,
}

impl PluginLog {
    /// The lines kept, oldest first.
    #[must_use]
    pub fn lines(&self) -> &[LogLine] {
        &self.lines
    }

    /// How many lines were let go to make room for newer ones.
    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }
}

/// One plugin's ring, as the kernel holds it for every instance of the
/// plugin.
pub(crate) type SharedRing = Arc<Mutex<Ring>>;

/// One plugin's log: its newest lines, and how many older ones were let go.
#[derive(Debug, Default)]
pub(crate) struct Ring {
    lines: VecDeque<LogLine>,
    dropped: u64,
}

impl Ring {
    /// Keeps `text` at `level`, cut and cleaned, letting the oldest line go
    /// when the ring is full.
    pub(crate) fn push(&mut self, level: LogLevel, text: &str) {
        let bound = usize::try_from(KERNEL.log_line_bytes).unwrap_or(usize::MAX);
        let mut kept = String::with_capacity(text.len().min(bound));
        for character in text.chars() {
            let shown = if nc_schema::unseen(character) {
                char::REPLACEMENT_CHARACTER
            } else {
                character
            };
            if kept.len() + shown.len_utf8() > bound {
                break;
            }
            kept.push(shown);
        }
        let lines = usize::try_from(KERNEL.log_ring_lines).unwrap_or(usize::MAX);
        while self.lines.len() >= lines {
            self.lines.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.lines.push_back(LogLine { level, text: kept });
    }

    /// What the ring holds, as a reader is handed it.
    pub(crate) fn read(&self) -> PluginLog {
        PluginLog {
            lines: self.lines.iter().cloned().collect(),
            dropped: self.dropped,
        }
    }
}

/// The world's level, as the ring keeps it.
const fn kept_level(level: world::Level) -> LogLevel {
    match level {
        world::Level::Trace => LogLevel::Trace,
        world::Level::Debug => LogLevel::Debug,
        world::Level::Info => LogLevel::Info,
        world::Level::Warn => LogLevel::Warn,
        world::Level::Error => LogLevel::Error,
    }
}

impl<E: Environment> world::Host for HostState<E> {
    fn write(&mut self, lvl: world::Level, msg: String) -> wasmtime::Result<()> {
        self.ungated("log", "write");
        crate::dispatch::lock(&self.ring).push(kept_level(lvl), &msg);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{LogLevel, LogLine, Ring, kept_level, world};
    use crate::bindings::noctua::kernel::log::Host as _;
    use crate::capability::{GrantSet, Linked};
    use crate::host::HostState;
    use nc_catalog::KERNEL;

    #[test]
    fn a_line_is_kept_at_its_level_and_the_write_is_recorded_as_reached() {
        let mut state = HostState::in_memory(GrantSet::none());
        state
            .write(world::Level::Warn, "the disk is full".to_owned())
            .expect("log is given to every plugin");
        let read = state.ring().read();
        assert_eq!(
            read.lines()
                .iter()
                .map(|line| (line.level(), line.text()))
                .collect::<Vec<_>>(),
            [(LogLevel::Warn, "the disk is full")]
        );
        assert_eq!(read.dropped(), 0);
        assert_eq!(
            state.reached(),
            [Linked {
                interface: "log",
                function: "write",
                capability: None,
            }]
        );
    }

    #[test]
    fn every_level_of_the_world_is_kept_as_its_own() {
        for (written, kept, word) in [
            (world::Level::Trace, LogLevel::Trace, "trace"),
            (world::Level::Debug, LogLevel::Debug, "debug"),
            (world::Level::Info, LogLevel::Info, "info"),
            (world::Level::Warn, LogLevel::Warn, "warn"),
            (world::Level::Error, LogLevel::Error, "error"),
        ] {
            assert_eq!(kept_level(written), kept);
            assert_eq!(kept.word(), word);
        }
    }

    #[test]
    fn a_long_line_is_cut_at_a_character_and_at_the_bound() {
        let bound = usize::try_from(KERNEL.log_line_bytes).expect("the bound fits");
        let mut ring = Ring::default();
        // One byte short of the bound, then a character of two bytes: it does
        // not fit, and half of it is not a character.
        ring.push(LogLevel::Info, &format!("{}é", "a".repeat(bound - 1)));
        let read = ring.read();
        assert_eq!(read.lines()[0].text(), "a".repeat(bound - 1));
        ring.push(LogLevel::Info, &"b".repeat(bound));
        assert_eq!(
            ring.read().lines().get(1).map(|line| line.text().len()),
            Some(bound),
            "a line of exactly the bound is kept whole"
        );
    }

    #[test]
    fn a_character_a_reader_sees_nothing_of_is_replaced() {
        let mut ring = Ring::default();
        ring.push(LogLevel::Info, "one\ntwo\u{202e}three\u{fe0f}four");
        assert_eq!(
            ring.read().lines()[0].text(),
            "one\u{fffd}two\u{fffd}three\u{fffd}four"
        );
    }

    #[test]
    fn a_full_ring_lets_its_oldest_line_go_and_counts_it() {
        let lines = usize::try_from(KERNEL.log_ring_lines).expect("the bound fits");
        let mut ring = Ring::default();
        for at in 0..=lines {
            ring.push(LogLevel::Debug, &at.to_string());
        }
        let read = ring.read();
        assert_eq!(read.lines().len(), lines);
        assert_eq!(read.dropped(), 1);
        assert_eq!(read.lines().first().map(LogLine::text), Some("1"));
        assert_eq!(
            read.lines().last().map(LogLine::text),
            Some(lines.to_string().as_str())
        );
    }
}
