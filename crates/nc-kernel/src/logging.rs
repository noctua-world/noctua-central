//! Where the kernel's events go: the central log, or a capture a test reads.
//!
//! **Typed events only.** Every line the kernel writes is one of the structs
//! `catalog/log-events.toml` generates into `nc_log::events`, whose fields are
//! numbers, codes, identifiers the platform minted and the variants of the
//! kernel's own enums — never a command's name, a key, a plugin's text or
//! anything a person wrote, because the central log stays on in production and
//! must hold no user data (`NOCTUA.md §15.4`). A plugin's own words go to its
//! ring (`log.write`), which never reaches here.
//!
//! [`LogSink`] is what a kernel is opened with: `nc_log::Log` behind a mutex in
//! a daemon — the log's shipper holds a channel's receiving end, which no two
//! threads may share, and a kernel's calls arrive on any — and `Captured`,
//! behind `test-support`, in a suite, which keeps each line's name and fields
//! to be asserted.
//!
//! What this answers (`NOCTUA.md §2.4`): user data in the central log, which no
//! event here can carry. What it does not answer: a line lost when the log
//! store is down, which is `nc-log`'s shipper's to count and report.

use nc_catalog::Capability;
use nc_log::Event;

/// Where a kernel writes its events.
pub trait LogSink: core::fmt::Debug + Send + Sync + 'static {
    /// Writes `event`.
    fn record(&self, event: &impl Event);
}

/// A sink behind an `Arc` is a sink: one log, written by the kernel and read
/// or flushed by whoever else holds it.
impl<T: LogSink> LogSink for std::sync::Arc<T> {
    fn record(&self, event: &impl Event) {
        T::record(self, event);
    }
}

impl LogSink for std::sync::Mutex<nc_log::Log> {
    fn record(&self, event: &impl Event) {
        self.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .record(event);
    }
}

/// The variant a capability is logged as: the name the catalog's
/// capabilities give it.
///
/// **Written out, one literal each**, because a log field is a variant the
/// source names and never text read at run time; the test below holds every
/// arm to the catalog's name for it.
pub(crate) fn capability(capability: Capability) -> nc_log::Variant {
    match capability {
        Capability::BlobsRead => nc_log::variant!("blobs.read"),
        Capability::BlobsWrite => nc_log::variant!("blobs.write"),
        Capability::CommandsBegin => nc_log::variant!("commands.begin"),
        Capability::CommandsDispatch => nc_log::variant!("commands.dispatch"),
        Capability::Context => nc_log::variant!("context"),
        Capability::EventsPublish => nc_log::variant!("events.publish"),
        Capability::EventsSubscribe => nc_log::variant!("events.subscribe"),
        Capability::Net => nc_log::variant!("net"),
        Capability::Store => nc_log::variant!("store"),
        Capability::Timers => nc_log::variant!("timers"),
        Capability::Ui => nc_log::variant!("ui"),
    }
}

/// One line a `Captured` sink kept: the event's name, and each field as the
/// canonical value the log would write it as. **For tests only.**
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedLine {
    /// The event's dotted name.
    pub name: String,
    /// Each field, by name, as the log writes it.
    pub fields: std::collections::BTreeMap<String, nc_canon::Value>,
}

#[cfg(any(test, feature = "test-support"))]
impl CapturedLine {
    /// The field `name` as text, when it is written as text.
    #[must_use]
    pub fn text(&self, name: &str) -> Option<String> {
        self.fields
            .get(name)
            .cloned()
            .and_then(nc_canon::Value::into_text)
    }
}

/// A sink that keeps every line in memory, for a test to read. **For tests
/// only.**
///
/// Bounded like nothing else here, because a test is the only thing that
/// writes to one and reads it back before it ends.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct Captured {
    lines: std::sync::Mutex<Vec<CapturedLine>>,
}

#[cfg(any(test, feature = "test-support"))]
impl Captured {
    /// Every line kept, in the order written.
    #[must_use]
    pub fn lines(&self) -> Vec<CapturedLine> {
        self.lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Every line of the event `name`, in the order written.
    #[must_use]
    pub fn named(&self, name: &str) -> Vec<CapturedLine> {
        self.lines()
            .into_iter()
            .filter(|line| line.name == name)
            .collect()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl LogSink for Captured {
    fn record(&self, event: &impl Event) {
        let line = CapturedLine {
            name: event.name().into(),
            fields: event
                .fields()
                .into_iter()
                .map(|(name, field)| (name.into(), field.to_canonical()))
                .collect(),
        };
        self.lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(line);
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, Read as _, Write as _};
    use std::time::Duration;

    use super::{Captured, LogSink as _, capability};

    /// A log store that accepts connections and hands back every line it was
    /// sent, as `nc-log`'s own suite stands one up: what is faked is the
    /// store, and the shipper speaks its real HTTP to it over a real socket.
    fn a_store_that_keeps_what_it_is_given() -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        let (sending, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(32) {
                let Ok(mut stream) = stream else { break };
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("cloneable"));
                let mut length = 0_usize;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 {
                        break;
                    }
                    let lower = header.to_ascii_lowercase();
                    if let Some(said) = lower.strip_prefix("content-length:") {
                        length = said.trim().parse().unwrap_or(0);
                    }
                    if header.trim().is_empty() {
                        break;
                    }
                }
                let mut body = vec![0_u8; length];
                if reader.read_exact(&mut body).is_ok() {
                    for line in String::from_utf8_lossy(&body).lines() {
                        if !line.trim().is_empty() {
                            let _ = sending.send(line.to_owned());
                        }
                    }
                }
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                let _ = stream.flush();
            }
        });
        (format!("http://127.0.0.1:{port}/insert"), received)
    }

    /// The sink a daemon opens a kernel with ships what the kernel records to
    /// the central log's store, every field of it.
    ///
    /// A verifier found it driven by nothing: every suite of this crate logs
    /// into a capture, and `nc-log`'s own cannot reach this impl, so a sink
    /// that dropped every line passed them all.
    #[test]
    fn the_central_log_sink_ships_what_the_kernel_records() {
        let (endpoint, received) = a_store_that_keeps_what_it_is_given();
        let log = nc_log::Log::open(
            nc_log::Origin {
                service: nc_log::literal!("nc-kernel-tests"),
                version: nc_log::literal!("9.9.9"),
            },
            &endpoint,
        )
        .expect("the endpoint is valid");
        let sink = std::sync::Mutex::new(log);
        sink.record(&nc_log::events::KernelPluginQuarantined {
            slot: 3,
            code: nc_catalog::ErrorCode::PluginQuarantined,
        });
        // `Drop` is what flushes, and it is the path a daemon leaves by.
        drop(sink);
        let line = received
            .recv_timeout(Duration::from_secs(20))
            .expect("the store received the line the kernel recorded");
        for said in [
            "\"_msg\":\"kernel.plugin_quarantined\"".to_owned(),
            "\"slot\":\"3\"".to_owned(),
            format!(
                "\"code\":\"{}\"",
                nc_catalog::ErrorCode::PluginQuarantined.code()
            ),
            "\"service\":\"nc-kernel-tests\"".to_owned(),
        ] {
            assert!(line.contains(&said), "{said}: {line}");
        }
    }

    #[test]
    fn every_capability_is_logged_as_the_name_the_catalog_gives_it() {
        assert_ne!(nc_catalog::CAPABILITIES, []);
        for spec in nc_catalog::CAPABILITIES {
            assert_eq!(capability(spec.capability).get(), spec.name);
        }
    }

    #[test]
    fn a_captured_line_keeps_the_events_name_and_every_field() {
        let captured = Captured::default();
        captured.record(&nc_log::events::KernelPluginQuarantined {
            slot: 3,
            code: nc_catalog::ErrorCode::PluginQuarantined,
        });
        let lines = captured.named("kernel.plugin_quarantined");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text("slot").as_deref(), Some("3"));
        assert_eq!(
            lines[0].text("code"),
            Some(nc_catalog::ErrorCode::PluginQuarantined.code().to_owned())
        );
        assert_eq!(
            captured.named("kernel.unit_aborted"),
            [] as [crate::logging::CapturedLine; 0]
        );
    }
}
