//! Every event the registry declares is built, recorded, and read back.
//!
//! **This is what executes the crate's public entry point on every generated
//! event.** `Log::record` is what a caller uses — it is the whole of what this
//! crate is for. The end-to-end line tests
//! drive `Log::line` against hand-written `Probe` and `Impostor` events declared
//! inside `lib.rs`, so without this the types the compiler emits are held as a
//! set by a textual comparison against the catalog, and by nothing that runs
//! each of them.
//!
//! That is `§19.3`'s opening failure in its usual shape — the generated code
//! compiles, a gate says it matches the registry, and whether it *works* is
//! nobody's assertion. `to_field()` on any one of them could return the wrong
//! variant and every gate in this repository would stay green.
//!
//! So this builds one of each, records it through a real `Log` over a real
//! socket, and reads the JSON lines the store receives.

use std::io::{BufRead as _, Read as _, Write as _};
use std::net::TcpListener;

/// A store that accepts connections and hands back every line it was given.
///
/// Not a mock of the shipper: the real `Shipper` opens a real socket to this and
/// speaks the HTTP it speaks to VictoriaLogs. What is faked is the store, which
/// is the one thing a test cannot bring up.
fn a_store_that_keeps_what_it_is_given() -> (String, std::sync::mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
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
                if let Some(said) = header.to_ascii_lowercase().strip_prefix("content-length:") {
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

#[test]
fn every_generated_event_is_built_recorded_and_read_back() {
    use core::time::Duration;

    let (endpoint, received) = a_store_that_keeps_what_it_is_given();
    let log = nc_log::Log::open(
        nc_log::Origin {
            service: nc_log::literal!("nc-log-tests"),
            version: nc_log::literal!("9.9.9"),
        },
        &endpoint,
    )
    .expect("the endpoint is valid");

    record_one_of_each(&log);

    // `Drop` is what flushes, and it is also the path a real process leaves by.
    drop(log);

    // **The registry's own count, not a literal.** A literal count written in
    // three places — here, in the assertion, and as the length of the name list
    // below — catches *removing* an event and not *adding* one. Measured with
    // literals: an event added to `catalog/log-events.toml`,
    // `cargo xtask build`, `cargo xtask docs`, whole workspace green,
    // `check passed`, and the new event's `to_field()` reaching nothing at all.
    // The suite's own header says "every event the registry declares is built,
    // recorded, and read back", and a hand list cannot say that about an event
    // nobody added to it.
    let expected = nc_catalog::EVENTS.len();
    let mut lines = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while lines.len() < expected && std::time::Instant::now() < deadline {
        match received.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => lines.push(line),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert_eq!(
        lines.len(),
        expected,
        "the store received {} of {expected} line(s): {lines:#?}",
        lines.len()
    );

    // Every event's name arrived, so no two collapsed into one line and none was
    // dropped.
    // **And every name the registry declares**, rather than a second hand list
    // that a new event does not appear in either. `record_one_of_each` above is
    // the one place a name still has to be written by hand, which is the point:
    // an event nobody built fails here by name.
    for name in nc_catalog::EVENTS.iter().map(|event| event.name) {
        // **The whole field, not the name as a substring.** Written
        // `line.contains(name)`, an event renamed to `ids.flake_collisionX`
        // satisfies it — the mutated name contains the original — and this test
        // passes with the generated code broken. A placeholder gate written
        // with `contains` has the same shape, and breaking the thing and
        // watching nothing go red is how either shows.
        assert!(
            lines
                .iter()
                .any(|line| line.contains(&format!("\"_msg\":\"{name}\""))),
            "`{name}` did not reach the store: {lines:#?}"
        );
    }

    every_value_is_spelled_as_its_field_says(&lines);

    // The origin is on every line, which is what says the log and not the event
    // decides where a line came from.
    for line in &lines {
        assert!(
            line.contains("\"service\":\"nc-log-tests\""),
            "a line carries no origin: {line}"
        );
    }
}

/// Every value of the lines `record_one_of_each` wrote, spelled the way its
/// field's type says it is spelled: the assertions a wrong `to_field()` fails.
///
/// Its own function because the list grows with the registry, and the test
/// that reads the lines back crossed clippy's line limit when the kernel's
/// events joined it.
fn every_value_is_spelled_as_its_field_says(lines: &[String]) {
    use nc_catalog::ErrorCode;

    // And every value, spelled the way its field's type says it is spelled.
    // These are the assertions a wrong `to_field()` fails.
    let expected_code = |code: ErrorCode| format!("\"code\":\"{}\"", code.code());
    let others = [
        ("db.migrated", "\"applied\":\"7\"".to_owned()),
        ("db.migrated", "\"took\":\"31\"".to_owned()),
        // **The registry's own text, not a code written out.** `§15.1` refuses
        // an `NC-` code as a string literal anywhere outside the catalog, and it
        // caught this line — correctly: a hand-written code is a second place
        // the number lives. Asking `ErrorCode::code()` is also the stronger
        // assertion, because it is what the field is supposed to carry.
        (
            "ids.clock_backwards",
            expected_code(ErrorCode::IdsClockBackwards),
        ),
        ("ids.clock_backwards", "\"drift\":\"41\"".to_owned()),
        ("ids.flake_collision", "\"attempt\":\"3\"".to_owned()),
        ("ids.sequence_exhausted", "\"waited\":\"11\"".to_owned()),
        ("journal.appended", "\"sequence\":\"91\"".to_owned()),
        // The identifier, as the text it is written as — `Field::Id`'s only
        // appearance in this registry, and the arm that blanks every identifier
        // in every line if it is wrong.
        (
            "journal.appended",
            "\"work_unit\":\"01J8XK2M9P51\"".to_owned(),
        ),
        ("journal.broken", "\"sequence\":\"92\"".to_owned()),
        ("journal.verified", "\"entries\":\"93\"".to_owned()),
        ("journal.verified", "\"from\":\"86\"".to_owned()),
        ("log.dropped", "\"dropped\":\"94\"".to_owned()),
        ("log.store_unreachable", "\"status\":\"503\"".to_owned()),
        ("prune.pruned", "\"table\":\"rows_over_time\"".to_owned()),
        ("prune.pruned", "\"removed\":\"95\"".to_owned()),
        ("prune.pruned", "\"remaining\":\"96\"".to_owned()),
        ("prune.within_limit", "\"max_rows\":\"98\"".to_owned()),
        ("prune.within_limit", "\"rows\":\"97\"".to_owned()),
    ];
    for (event, field) in others.into_iter().chain(the_kernels_fields()) {
        let line = lines
            .iter()
            .find(|line| line.contains(&format!("\"_msg\":\"{event}\"")))
            .expect("checked above");
        assert!(
            line.contains(field.as_str()),
            "`{event}` does not carry {field}: {line}"
        );
    }
}

/// The values of the kernel's and the network's lines, as
/// [`every_value_is_spelled_as_its_field_says`] reads them: their own list
/// because that one crossed clippy's line limit again when the kernel's
/// refusals of a host call joined it.
fn the_kernels_fields() -> Vec<(&'static str, String)> {
    use nc_catalog::ErrorCode;

    let expected_code = |code: ErrorCode| format!("\"code\":\"{}\"", code.code());
    vec![
        // `Field::Variant`'s one appearance in this registry: the capability a
        // plugin was refused, spelled as the variant and nothing else.
        ("kernel.capability_denied", "\"slot\":\"99\"".to_owned()),
        (
            "kernel.capability_denied",
            "\"capability\":\"blobs.write\"".to_owned(),
        ),
        (
            "kernel.capability_denied",
            expected_code(ErrorCode::PluginCapabilityDenied),
        ),
        ("kernel.host_call_refused", "\"slot\":\"87\"".to_owned()),
        (
            "kernel.host_call_refused",
            expected_code(ErrorCode::PluginReadOnlyFrame),
        ),
        (
            "kernel.dispatch_refused",
            expected_code(ErrorCode::KernelUnknownCommand),
        ),
        (
            "kernel.dispatch_refused",
            "\"reason\":\"unknown_command\"".to_owned(),
        ),
        ("kernel.dispatch_refused", "\"mutating\":true".to_owned()),
        (
            "kernel.unit_aborted",
            "\"unit\":\"01J8XK2M9P51\"".to_owned(),
        ),
        ("kernel.unit_aborted", "\"cause\":\"nested\"".to_owned()),
        (
            "kernel.revert_failed",
            "\"unit\":\"01J8XK2M9P51\"".to_owned(),
        ),
        (
            "kernel.revert_failed",
            expected_code(ErrorCode::DbUnreachable),
        ),
        ("kernel.recovered", "\"unit\":\"01J8XK2M9P51\"".to_owned()),
        ("kernel.recovered", "\"reverted\":false".to_owned()),
        (
            "kernel.step_reversed",
            "\"unit\":\"01J8XK2M9P51\"".to_owned(),
        ),
        ("kernel.step_reversed", "\"direction\":\"redo\"".to_owned()),
        (
            "kernel.step_reversed",
            "\"compensations\":\"91\"".to_owned(),
        ),
        ("kernel.step_reversed", "\"beyond_reach\":\"92\"".to_owned()),
        ("kernel.plugin_killed", "\"slot\":\"81\"".to_owned()),
        ("kernel.plugin_killed", "\"cause\":\"trap\"".to_owned()),
        (
            "kernel.plugin_killed",
            expected_code(ErrorCode::PluginTrapped),
        ),
        ("kernel.plugin_restarted", "\"slot\":\"82\"".to_owned()),
        ("kernel.plugin_restarted", "\"took\":\"83\"".to_owned()),
        ("kernel.plugin_quarantined", "\"slot\":\"84\"".to_owned()),
        (
            "kernel.plugin_quarantined",
            expected_code(ErrorCode::PluginQuarantined),
        ),
        ("net.refused", "\"slot\":\"85\"".to_owned()),
        ("net.refused", "\"reason\":\"private_address\"".to_owned()),
        (
            "net.refused",
            expected_code(ErrorCode::PluginRequestRefused),
        ),
    ]
}

/// One of each generated event, with a distinguishable value in every field.
///
/// Its own function because building them and judging the lines they become are
/// two halves, and together they cross clippy's line limit. A `to_field()`
/// returning the wrong variant, or a field emitted under the wrong name, changes
/// what the store receives — which is what the assertions in the caller read.
fn record_one_of_each(log: &nc_log::Log) {
    use core::time::Duration;
    use nc_catalog::ErrorCode;
    use nc_log::events;

    // One of each, with a distinguishable value in every field — so a
    // `to_field()` returning the wrong variant, or a field emitted under the
    // wrong name, changes what comes back.
    log.record(&events::DbMigrated {
        applied: 7,
        took: Duration::from_millis(31),
    });
    log.record(&events::IdsClockBackwards {
        drift: Duration::from_millis(41),
        code: ErrorCode::IdsClockBackwards,
    });
    log.record(&events::IdsFlakeCollision { attempt: 3 });
    log.record(&events::IdsSequenceExhausted {
        waited: Duration::from_millis(11),
    });
    // **`Some`, because `None` renders nothing.** `Field::Id` is the arm that
    // carries every identifier into every log line, and `journal.appended` is
    // the only event in the shipped registry that can reach it — built empty
    // here, so the arm ran nowhere in the suite and could return `""` with the
    // whole gate green. `value.rs`'s own comment records fixing exactly this
    // fixture hole one layer up, for `to_field`.
    //
    // Minted here rather than inside the call, because the mint gate reads a
    // logging call site as a window and a mint inside one is what it exists to
    // refuse — the value is a literal of this file's own either way.
    let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P51").expect("a test id"); // allow-logging: `from_crockford` over a literal written in this file, so `Field::Id` has an identifier to render
    log.record(&events::JournalAppended {
        sequence: 91,
        work_unit: Some(unit),
    });
    log.record(&events::JournalBroken {
        sequence: 92,
        code: ErrorCode::JournalChainBroken,
    });
    log.record(&events::JournalVerified {
        entries: 93,
        from: 86,
        took: Duration::from_millis(21),
    });
    log.record(&events::KernelCapabilityDenied {
        slot: 99,
        capability: nc_log::variant!("blobs.write"),
        code: ErrorCode::PluginCapabilityDenied,
    });
    log.record(&events::KernelHostCallRefused {
        slot: 87,
        code: ErrorCode::PluginReadOnlyFrame,
    });
    log.record(&events::KernelDispatchRefused {
        code: ErrorCode::KernelUnknownCommand,
        reason: nc_log::variant!("unknown_command"),
        mutating: true,
    });
    log.record(&events::KernelUnitAborted {
        unit,
        cause: nc_log::variant!("nested"),
    });
    log.record(&events::KernelRevertFailed {
        unit,
        code: ErrorCode::DbUnreachable,
    });
    log.record(&events::KernelRecovered {
        unit,
        reverted: false,
        code: ErrorCode::KernelRecoveredUnit,
    });
    log.record(&events::KernelStepReversed {
        unit,
        direction: nc_log::variant!("redo"),
        compensations: 91,
        beyond_reach: 92,
    });
    log.record(&events::KernelPluginKilled {
        slot: 81,
        cause: nc_log::variant!("trap"),
        code: ErrorCode::PluginTrapped,
    });
    log.record(&events::KernelPluginRestarted {
        slot: 82,
        took: Duration::from_millis(83),
    });
    log.record(&events::KernelPluginQuarantined {
        slot: 84,
        code: ErrorCode::PluginQuarantined,
    });
    log.record(&events::NetRefused {
        slot: 85,
        reason: nc_log::variant!("private_address"),
        code: ErrorCode::PluginRequestRefused,
    });
    log.record(&events::LogDropped {
        dropped: 94,
        code: ErrorCode::LogDropped,
    });
    log.record(&events::LogStoreUnreachable {
        code: ErrorCode::LogUnreachable,
        status: 503,
    });
    log.record(&events::PrunePruned {
        table: nc_log::literal!("rows_over_time"),
        removed: 95,
        remaining: 96,
        took: Duration::from_millis(51),
    });
    log.record(&events::PruneWithinLimit {
        table: nc_log::literal!("journal_entries"),
        rows: 97,
        max_rows: 98,
    });
}
