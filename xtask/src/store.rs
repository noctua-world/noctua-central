//! What the *running* log store says it was started with.
//!
//! **The bounds were certified against the file that starts the store, and
//! never against the store.** `xtask/tests/the_stack_enforces_the_declared_
//! bounds.rs` reads `docker-compose.yml`, compares its two flags with
//! `catalog/limits.toml`, and that is the whole of it — so the claim the gate
//! makes is "the file agrees with the catalog", while the sentence
//! `catalog/limits.toml` actually writes down is that the log store keeps
//! thirty days and eight gibibytes. Between the two sits everything that can
//! start a container: an override file compose reads without being asked, a `-f` on the command
//! line, an image whose entrypoint appends its own arguments, or simply a
//! container still running from before the file was edited. Each of those
//! leaves every gate green with a store bounded by something else.
//!
//! VictoriaLogs answers `GET /flags` with the flags it is actually running
//! with, which is the one place the answer is not a claim about a file:
//!
//! ```text
//! -retention.maxDiskSpaceUsageBytes="8GiB"
//! -retentionPeriod="30d"
//! -storageDataPath="/victoria-logs-data"
//! ```
//!
//! Read from here rather than from inside the container, for the reason
//! `dev.rs` gives about `/health`: the image has no shell, so a `CMD` in the
//! compose file cannot ask it anything.

use std::io::{Read as _, Write as _};

/// How long to wait for the store to answer, in seconds.
const PATIENCE: u64 = 5;

/// What the running store answered, in a shape only this module can build.
///
/// **A type, because four rounds of text pins did not hold the hand-over.**
/// The comparison in `check.rs` that asks whether the store writes where the
/// container mounts has been made a tautology four times: first inline, then
/// through a closure, then through a rebinding, then through the argument
/// itself. Round 52 pinned the callee's whole body, both of its argument lines
/// and every call site — and round 53 walked past all of it by changing what
/// the *caller hands in*, deriving the store's side of the comparison from the
/// container it is compared against. `check passed`, and the only lines that
/// moved in the transcript were line counts.
///
/// A pin over text is a description of one defect. This is the room itself:
/// the flags are a `Reported`, its one field is private to this module, and
/// `check.rs` cannot construct one from anything at all. The only way to fake
/// the first side of that comparison is now to fake the store's answer, which
/// is the step's own subject and read from the socket a few lines below.
pub(crate) struct Reported(Vec<(String, String)>);

impl Reported {
    /// The value the store reports for one flag, or `None` if it reports none.
    pub(crate) fn named(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(flag, _)| flag == name)
            .map(|(_, value)| value.clone())
    }

    /// Every flag's name, in the order the store answered.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(name, _)| name.as_str())
    }

    /// Whether the store answered with no flag at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many flags the store reported.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

/// Every flag the running store reports, as `(name, value)`.
///
/// The name keeps its leading `-` off and the value its quotes off, so
/// `-retentionPeriod="30d"` reads as `("retentionPeriod", "30d")`.
///
/// # Errors
///
/// Returns why the store did not answer, in the shape a developer can act on:
/// the address that refused, or the status line that was not a 2xx.
pub(crate) fn flags(port: &str) -> Result<Reported, String> {
    let address = format!("127.0.0.1:{port}");
    let mut socket = std::net::TcpStream::connect(&address).map_err(|error| {
        format!("the log store is not accepting connections at {address}: {error}")
    })?;
    let patience = std::time::Duration::from_secs(PATIENCE);
    let _ = socket.set_read_timeout(Some(patience));
    let _ = socket.set_write_timeout(Some(patience));
    socket
        .write_all(b"GET /flags HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .map_err(|error| format!("the log store closed the connection: {error}"))?;
    let mut answer = Vec::new();
    socket
        .read_to_end(&mut answer)
        .map_err(|error| format!("the log store did not answer: {error}"))?;
    let answer = String::from_utf8_lossy(&answer);
    the_answer_read(&answer)
}

/// The flags in a whole `/flags` response, status line and headers included.
///
/// **Lifted so the door opens above the decisions rather than below them.**
/// `as_a_store_would_answer_for_tests` hands a test [`the_flags_in`], the
/// parser — and the two judgements this function makes before the parser ever
/// runs were left with no reader that could drive them. Both survived being
/// removed: `served` answered unconditionally, and the body split taking the
/// whole response, which the paragraph below says would report `Content-Type`
/// as a flag. Reported in round sixty-six. A door written so a type's
/// accessors can be driven has to open above what it is guarding.
fn the_answer_read(answer: &str) -> Result<Reported, String> {
    let status = answer.lines().next().unwrap_or_default();
    if !served(answer) {
        return Err(format!(
            "`GET /flags` on the log store answered `{}`, so what it is running with \
             could not be read",
            status.trim()
        ));
    }
    // **The body, and only the body.** A header line can hold an `=` too, so a
    // reader that took every `name=value` in the response would report
    // `Content-Type` as a flag. The blank line is where the headers stop.
    let Some((_, body)) = answer.split_once("\r\n\r\n") else {
        return Err("the log store's answer has no body".to_owned());
    };
    Ok(the_flags_in(body))
}

/// The flags in a `/flags` body, as the step reads them.
///
/// **A door, because the type is the point.** `Reported`'s one field is private
/// so that `check.rs` cannot build one out of the container it is compared
/// against — and that leaves its three accessors with no reader that can drive
/// them. A reviewer emptied `names()` on 2026-09-12 with `check` passing. This
/// hands a test the parser's own answer without handing it the constructor.
#[cfg(test)]
pub(crate) fn as_a_store_would_answer_for_tests(body: &str) -> Reported {
    the_flags_in(body)
}

/// The whole response a store would send, as the step reads it.
///
/// The door above hands a test the parser; this one hands it
/// [`the_answer_read`], which is where the status and the header boundary are
/// decided. Same argument as that door's, one layer up: without it those two
/// judgements had no reader at all.
#[cfg(test)]
pub(crate) fn as_a_store_answered_for_tests(answer: &str) -> Result<Reported, String> {
    the_answer_read(answer)
}

/// Every `-name="value"` line of a `/flags` body, in the order it answered.
fn the_flags_in(body: &str) -> Reported {
    Reported(
        body.lines()
            .filter_map(|line| line.trim().strip_prefix('-'))
            .filter_map(|line| line.split_once('='))
            .map(|(name, value)| (name.to_owned(), value.trim_matches('"').to_owned()))
            .collect(),
    )
}

/// Posts one line to the store and reads it back.
///
/// **Nothing in this repository ever spoke to the store `cargo xtask dev`
/// brings up.** `crates/nc-log/tests/every_event_reaches_the_store.rs` is the
/// only end-to-end coverage and its store is a hand-written listener that
/// answers `HTTP/1.1 200 OK` to whatever bytes arrive — so it proves the shipper
/// writes what it means to write, and nothing about whether the store keeps it.
/// The step above asks `GET /flags`, which is a question about configuration.
///
/// Measured against the running store: a line whose fields the store reserves is
/// accepted with `200 OK`, the shipper counts it delivered, `lost()` stays 0,
/// nothing reaches stderr — and the record is nowhere, because the value landed
/// it outside the retention window. A `200` from this store is not a receipt.
///
/// One line per `check`, tagged so it is recognisable in the store, and read
/// back through `/select/logsql/query` by that tag. Against 30 days and eight
/// gibibytes that is not a cost worth avoiding, and it is the only thing that
/// would ever notice a format the store takes and discards.
///
/// # Errors
///
/// Returns what happened: the store refused the line, or accepted it and did
/// not give it back.
pub(crate) fn a_line_survives_the_round_trip(
    port: &str,
    tag: &str,
) -> Result<receipt::Receipt, String> {
    let line = format!(
        "{{\"_msg\":\"gate.round_trip\",\"level\":\"debug\",\"service\":\"xtask\",\"probe\":\"{tag}\"}}"
    );
    let answer = ask(
        port,
        &format!(
            "POST /insert/jsonline HTTP/1.1\r\nHost: 127.0.0.1\r\n\
             Content-Type: application/stream+json\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{line}",
            line.len()
        ),
    )?;
    if !served(&answer) {
        return Err(format!(
            "the log store refused a line: `{}`",
            answer.lines().next().unwrap_or_default().trim()
        ));
    }
    // **The store indexes asynchronously**, so a query issued immediately after
    // the insert legitimately finds nothing. The wait is bounded and the failure
    // is the whole budget elapsing, not the first miss.
    // **One value asks and answers.** `Receipt::read(&answer, tag)` took the
    // needle as an ordinary argument beside a query built from the same name, so
    // the two could be made to disagree — and `Receipt::read(&answer, "")`
    // type-checks, `str::contains("")` is always true, and the first served
    // answer then ends the poll whatever it holds. Two reviewers found that
    // independently in round sixty-three: the type closed the *verdict* and left
    // the *question* free, which is a pin on where a value came from and not on
    // where it goes. A probe owns the tag, builds the query from it, and reads
    // the answer against it; there is no second argument to get wrong.
    let probe = receipt::Probe::for_tag(tag).ok_or_else(|| {
        "the round trip was asked to look for an empty tag, which every answer holds".to_owned()
    })?;
    let query = probe.query();
    for _ in 0..PATIENCE_ROUNDS {
        let answer = ask(
            port,
            &format!(
                "POST /select/logsql/query HTTP/1.1\r\nHost: 127.0.0.1\r\n\
                 Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{query}",
                query.len()
            ),
        )?;
        if let Some(receipt) = probe.read(&answer) {
            return Ok(receipt);
        }
        std::thread::sleep(std::time::Duration::from_millis(PATIENCE_STEP_MS));
    }
    Err(format!(
        "the log store accepted a line and did not give it back: `probe:\"{tag}\"` found \
         nothing after {}ms. A 200 from this store is not a receipt — a line whose fields \
         it reserves is taken and dropped in silence",
        u64::from(PATIENCE_ROUNDS) * PATIENCE_STEP_MS
    ))
}

/// How many times the round trip asks before giving up, and how long between.
const PATIENCE_ROUNDS: u32 = 20;
/// Milliseconds between those asks.
const PATIENCE_STEP_MS: u64 = 250;

/// Evidence the store gave back the line it was given.
///
/// **The judgement was pinned and the hand-over was not.** `came_back` had a
/// test of its own; its only call site had none, so `if came_back(&answer, tag)`
/// narrowed to `if served(&answer)` changed no verdict anywhere — and the first
/// empty `200` from `/select/logsql/query` then ends the poll and the run
/// announces that a line came back having checked nothing. Measured in round
/// sixty-two. The step exists because *"a `200` from this store is not a
/// receipt"*, and that is the sentence the narrowing undoes.
///
/// So the round trip hands back a receipt rather than `()`. Nothing outside this
/// module can make one, and the only thing in it that makes one reads both
/// halves — so a call site that decides the poll some other way has nothing to
/// return and does not compile.
mod receipt {
    /// The store served an answer, and that answer holds the line.
    #[derive(Debug)]
    pub(crate) struct Receipt(());

    /// The tag this round trip sent, which is the tag it looks for.
    ///
    /// **The question and the answer are one value.** The needle used to be an
    /// argument beside a query built from the same name, and nothing held the
    /// two together: `Receipt::read(&answer, "")` type-checks and every answer
    /// "contains" the empty string, so the first served response ended the poll
    /// having checked nothing — measured in round sixty-three against the
    /// running store, with a query for a tag nothing inserted answering `200 OK`
    /// and `Content-Length: 0`, and the step announcing that a line came back.
    ///
    /// There is no empty tag: [`Probe::for_tag`] refuses one, so the shape the
    /// reviewers planted cannot be written at all.
    #[derive(Debug)]
    pub(crate) struct Probe {
        tag: String,
    }

    impl Probe {
        /// A probe for this tag, or nothing when there is no tag to look for.
        pub(crate) fn for_tag(tag: &str) -> Option<Self> {
            (!tag.is_empty()).then(|| Self {
                tag: tag.to_owned(),
            })
        }

        /// The query that asks the store for this probe's own tag.
        pub(crate) fn query(&self) -> String {
            format!("query=probe%3A%22{}%22", self.tag)
        }

        /// **The status, and then the tag.** `contains` over the whole response
        /// reads the headers too, and this store echoes the query it could not
        /// parse back into the body of a 400 — so any error path that quotes the
        /// query turns "the line did not come back" into success.
        pub(crate) fn read(&self, answer: &str) -> Option<Receipt> {
            (super::served(answer) && answer.contains(&self.tag)).then_some(Receipt(()))
        }
    }
}

/// Whether an HTTP answer's status line is a 2xx.
///
/// The protocol name at the start and a three-digit 2xx after it, so a body that
/// happens to begin with `200` is not read as a status line.
///
/// **It was written four times and the copies did not agree.** Twice in this
/// file, once in `dev.rs`, once in `nc-log`'s shipper — and only the shipper
/// required the three characters to be digits, so `HTTP/1.1 2ab Invented` was a
/// refusal to the crate that ships logs and a success to both gates that decide
/// whether the store is up. `cargo xtask dev` would have printed
/// `the log store answers /health` for a store answering nonsense.
///
/// One function now, driven by `xtask/canaries/status-lines.tsv` — a table
/// `nc-log`'s own suite reads as well, so the two implementations are held to
/// the same cases rather than to the same sentence in two comments.
pub(crate) fn served(answer: &str) -> bool {
    let status = answer.lines().next().unwrap_or_default();
    let mut words = status.split_whitespace();
    let version = words.next().unwrap_or_default();
    let code = words.next().unwrap_or_default();
    version.starts_with("HTTP/")
        && code.len() == 3
        && code.chars().all(|digit| digit.is_ascii_digit())
        && code.starts_with('2')
}

/// Sends one request and returns the whole answer.
fn ask(port: &str, request: &str) -> Result<String, String> {
    let address = format!("127.0.0.1:{port}");
    let mut socket = std::net::TcpStream::connect(&address).map_err(|error| {
        format!("the log store is not accepting connections at {address}: {error}")
    })?;
    let patience = std::time::Duration::from_secs(PATIENCE);
    let _ = socket.set_read_timeout(Some(patience));
    let _ = socket.set_write_timeout(Some(patience));
    socket
        .write_all(request.as_bytes())
        .map_err(|error| format!("the log store closed the connection: {error}"))?;
    let mut answer = Vec::new();
    socket
        .read_to_end(&mut answer)
        .map_err(|error| format!("the log store did not answer: {error}"))?;
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

/// A `GiB`/`MiB`/`KiB`/`B` size as bytes.
///
/// The same scales `xtask/tests/the_stack_enforces_the_declared_bounds.rs`
/// reads out of `docker-compose.yml`, because the store reports its own bound
/// in the units it was given rather than in bytes.
pub(crate) fn bytes(value: &str) -> Option<u64> {
    for (suffix, scale) in [
        ("GiB", 1024_u64.pow(3)),
        ("MiB", 1024_u64.pow(2)),
        ("KiB", 1024),
        ("B", 1),
    ] {
        if let Some(number) = value.strip_suffix(suffix) {
            return number
                .trim()
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(scale));
        }
    }
    value.parse().ok()
}

#[cfg(test)]
mod tests {
    /// A `/flags` answer is read only when the store served it, and only its
    /// body is read.
    ///
    /// **Both decisions sat below the door and were driven by nothing.** The
    /// door this module had hands a test the parser; these two judgements run
    /// before the parser and each survived being removed with the whole suite
    /// green. The first reads a `500` as an answer. The second reads the
    /// headers as flags, which the code's own comment says would report
    /// `Content-Type` as one — and a gate that believes the store is running
    /// with a `Content-Type` flag is a gate reporting a flag it has no opinion
    /// about, which is a refusal. Round sixty-six.
    #[test]
    fn a_flags_answer_is_read_only_when_it_was_served_and_only_its_body() {
        // The shape this store really answers with: a leading `-` per flag,
        // which is what tells a flag line from the banner above it.
        let body = "-retentionPeriod=\"30d\"\n-storageDataPath=\"/x\"\n";
        let served =
            format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}");
        let read =
            super::as_a_store_answered_for_tests(&served).expect("a 200 with a body is an answer");
        let names: Vec<&str> = read.names().collect();
        assert_eq!(
            names,
            ["retentionPeriod", "storageDataPath"],
            "a header holding an `=` was read as a flag, so the gate reports the store \
             running with a flag it has no opinion about"
        );

        // **A refusal is not an answer.** This store echoes what it could not
        // parse into the body of a `400`, and a `500` carries a body too.
        for refused in [
            "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\n\r\n-retentionPeriod=\"1d\"\n",
            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\n\r\n-retentionPeriod=\"1d\"\n",
        ] {
            assert!(
                super::as_a_store_answered_for_tests(refused).is_err(),
                "an answer the store did not serve was read as what it is running with: \
                 {refused:?}"
            );
        }

        // And an answer with no body at all is not an empty answer.
        assert!(
            super::as_a_store_answered_for_tests("HTTP/1.1 200 OK\r\n").is_err(),
            "a response with no body was read as a store reporting no flags"
        );

        // **`is_empty` in the direction its one reader is a floor against.**
        // That floor catches a store answering with nothing; the accessor was
        // asserted only on an answer that has flags, so any constant satisfied
        // it.
        let nothing = super::as_a_store_answered_for_tests("HTTP/1.1 200 OK\r\n\r\n")
            .expect("a 200 with an empty body is an answer");
        assert!(
            nothing.is_empty(),
            "a store answering with no flag at all was not read as empty, and the floor \
             that catches exactly that reads this accessor: {:?}",
            nothing.0
        );
        assert!(
            !read.is_empty(),
            "an answer holding two flags was read as empty"
        );
    }

    /// A line came back only when the store served the answer it came back in.
    ///
    /// **The status half was a repair proven by nothing.** This store echoes a
    /// query it could not parse back into the body of a `400`, so a search for
    /// the tag alone reads that error as a receipt. Its own comment records the
    /// hole and calls it *"one call to close"* — the call went in and nothing
    /// held it there: measured on 2026-09-08 with the live log store up,
    /// removing `served(&answer) &&` left `cargo test -p xtask --bin xtask`
    /// green at 481.
    #[test]
    fn a_line_came_back_only_in_an_answer_the_store_served() {
        /// A receipt or none, as a `bool` the assertions below can read.
        fn receipt_read(answer: &str, tag: &str) -> bool {
            super::receipt::Probe::for_tag(tag)
                .expect("these fixtures all name a tag")
                .read(answer)
                .is_some()
        }
        let tag = "probe:\"abc123\"";
        let body = format!("{{\"_msg\":\"{tag}\"}}");
        assert!(receipt_read(
            &format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{body}"),
            tag
        ));
        // The case the status check is for: the store quoting back the query it
        // refused. Every character of the tag is present, and the line is not
        // there.
        assert!(
            !receipt_read(
                &format!(
                    "HTTP/1.1 400 Bad Request\r\ncontent-type: text/plain\r\n\r\n\
                     cannot parse query: {tag}"
                ),
                tag
            ),
            "an error quoting the query was read as the line coming back"
        );
        // And the other half, which the status alone cannot answer: a clean
        // response that does not hold the line.
        assert!(!receipt_read(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n",
            tag
        ));
        // **There is no probe for the empty tag**, which is what makes the
        // needle unforgeable rather than merely typed: every answer "holds" the
        // empty string, so a probe for it would end the poll on the first served
        // response whatever it contained.
        assert!(
            super::receipt::Probe::for_tag("").is_none(),
            "a probe was built for the empty tag, which every answer holds"
        );
        // And the query asks for the tag the answer is read against — one value,
        // so the two cannot be made to disagree.
        let asking = super::receipt::Probe::for_tag("abc123").expect("a tag names something");
        assert!(
            asking.query().contains("abc123"),
            "the query does not ask for the tag the probe reads answers against: {}",
            asking.query()
        );
        // A redirect and a server error are not receipts either.
        for status in ["302 Found", "500 Internal Server Error"] {
            assert!(
                !receipt_read(&format!("HTTP/1.1 {status}\r\n\r\n{body}"), tag),
                "`{status}` was read as the store serving the line"
            );
        }
    }

    /// Every scale, and the bare number the store never actually writes.
    #[test]
    fn a_size_reads_as_bytes() {
        assert_eq!(super::bytes("8GiB"), Some(8_589_934_592));
        assert_eq!(super::bytes("100MiB"), Some(104_857_600));
        assert_eq!(super::bytes("512KiB"), Some(524_288));
        assert_eq!(super::bytes("7B"), Some(7));
        assert_eq!(super::bytes("7"), Some(7));
        assert_eq!(super::bytes("what"), None);
        // A size no `u64` holds is not a size, and was a wrapped one.
        assert_eq!(super::bytes("999999999999GiB"), None);
    }

    /// The table in `xtask/canaries/status-lines.tsv`, answered here.
    ///
    /// **`served` was written four times and the copies disagreed.** Twice in
    /// this file, once in `dev.rs`, once in `nc-log`'s shipper, with one
    /// sentence in four comments claiming they were "the same shape" — and only
    /// the shipper required the three characters of the code to be digits. So
    /// `HTTP/1.1 2ab Invented` was a refusal to the crate that ships logs and
    /// `the log store answers /health` to `cargo xtask dev`.
    ///
    /// `the_shared_status_line_table_is_answered_the_same_way` in
    /// `crates/nc-log/src/ship.rs` reads this same file. A case added to it is
    /// answered by both, which is the thing a shared sentence could not do.
    #[test]
    fn the_shared_status_line_table_is_answered_the_same_way() {
        const ROWS: usize = 23;
        let table = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/canaries/status-lines.tsv"
        ))
        .expect("the shared status-line table is readable");
        let mut checked = 0_usize;
        for row in table.lines() {
            if row.starts_with('#') || row.trim().is_empty() {
                continue;
            }
            let mut column = row.split('\t');
            let line = column.next().expect("a status line");
            if line == "line" {
                continue;
            }
            let terminator = column.next().expect("a terminator column");
            let served = column.next().expect("a `served` column");
            let taken = column.next().expect("a `taken` column");
            let _ = taken;
            let expected = match served {
                "2xx" => true,
                "not-2xx" => false,
                other => panic!("`{other}` is not a verdict this table can carry"),
            };
            let written = line.replace("\\r", "\r");
            let answer = match terminator {
                "crlf" => format!("{written}\r\n\r\n"),
                "none" => written,
                other => panic!("`{other}` is not a terminator this table can carry"),
            };
            assert_eq!(
                super::served(&answer),
                expected,
                "`{line}` ({terminator}) is `{served}` in the shared table and `served` \
                 said {}",
                super::served(&answer)
            );
            checked += 1;
        }
        // **The row count, not a floor with room under it.** `checked >= 15` over
        // twenty rows left five to spare, and one of the five is the only row
        // that decides `code.chars().all(is_ascii_digit)` — so deleting
        // `HTTP/1.1 2ab Invented` and then that clause restored exactly the
        // drift this table was built to end, with both suites green. A number
        // that has to move when the file moves is what makes a shared table a
        // mechanism rather than a habit.
        assert_eq!(
            checked, ROWS,
            "the shared table answered {checked} case(s) and holds {ROWS}. Adding or \
             removing a row is a change to what both suites are held to, so the number \
             moves here in the same edit"
        );
    }

    /// Nothing else in the tree spells this rule a third time.
    ///
    /// The four copies were found by reading, and a fifth would be found the
    /// same way — which is not a mechanism. Testing a status code by its first
    /// character is the shape, and it belongs in exactly two places: `served`
    /// here and `Delivery`'s classifier in `nc-log`, which the shared table
    /// holds together.
    ///
    /// **The needle is assembled rather than written**, because a test that
    /// scans for a string is a copy of it. Files, not line numbers: a copy is a
    /// copy wherever in its file it sits, and a line number would make this fail
    /// on every edit above it.
    #[test]
    fn the_two_xx_rule_is_written_twice_and_no_more() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        // **Every spelling of the shape, not one.** The needle was
        // `starts_with('2')`, so a third copy written `starts_with("2")` — or
        // `code.as_bytes()[0] == b'2'`, or `(200..300).contains(&code)` — was
        // invisible, and a reviewer measured a plausible third copy passing.
        // Assembling the needle keeps the scanner from being a copy; it does not
        // widen it.
        let quote = '\'';
        let quotes = '"';
        let needles = [
            format!("starts_with({quote}2{quote})"),
            format!("starts_with({quotes}2{quotes})"),
            format!("[0] == b{quote}2{quote}"),
            format!("first() == Some(&b{quote}2{quote})"),
            // **The ranges without their parentheses**, because `matches!(code,
            // 200..=299)` is how Rust writes an inclusive-range test and carries
            // none. Measured: a third classifier ending in that pattern passed
            // this scan.
            "200..300".to_owned(),
            "200..=299".to_owned(),
            "/ 100 == 2".to_owned(),
            // And the character read rather than matched, which is the most
            // ordinary spelling of all — a third copy written this way accepts
            // `HTTP/1.1 2ab Invented`, the exact drift `status-lines.tsv` exists
            // to end.
            format!("next() == Some({quote}2{quote})"),
            format!("next() == Some(&b{quote}2{quote})"),
            format!("next() == Some(&{quote}2{quote})"),
            // **And the prefix strip, which is how a third reader spelled it.**
            // A reviewer replaced `served`'s call in `dev.rs` with a local whose
            // code test was `code.len() == 3 && code.strip_prefix('2').is_some()`
            // — matching none of the ten needles above — and `cargo xtask dev`
            // printed `the log store answers /health` for `HTTP/1.1 2ab
            // Invented`, the exact invented code `status-lines.tsv` calls
            // `not-2xx`, with the shared table green.
            format!("strip_prefix({quote}2{quote})"),
            format!("starts_with(b{quote}2{quote})"),
            // And the byte read through a slice, which is how a reviewer's
            // third copy spelled it.
            format!("first(), Some(&b{quote}2{quote})"),
            format!("first() == Some(&b{quote}2{quote})"),
        ];
        let mut spelled = Vec::new();
        crate::gates::walk(root, &mut |path| {
            if path.extension().is_none_or(|kind| kind != "rs") {
                return;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            if text.lines().any(|line| {
                let code = crate::gates::line_without_a_comment(line);
                needles.iter().any(|needle| code.contains(needle.as_str()))
            }) {
                spelled.push(crate::relative_to(root, path));
            }
        })
        .expect("the tree walks");
        // **And the shape of reading a status line at all**, which is the rule a
        // needle list cannot be. A reviewer wrote a third classifier whose value
        // test was `code.strip_prefix('2').is_some()` — matching none of the ten
        // needles then in the list — and `cargo xtask dev` printed
        // `the log store answers /health` for `HTTP/1.1 2ab Invented`, the exact
        // invented code `status-lines.tsv` calls `not-2xx`, with the shared
        // table green.
        //
        // A classifier has to establish that the answer *is* HTTP before it can
        // read a code out of it, and there is one way to spell that. The needles
        // above stay as a floor — they catch a value test written in a file that
        // already legitimately parses a status line — but this is the rule.
        let mut parses = Vec::new();
        crate::gates::walk(root, &mut |path| {
            if path.extension().is_none_or(|kind| kind != "rs") {
                return;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            // **`HTTP` without the slash too.** The needle was the whole
            // `starts_with("HTTP/")`, so a third classifier written
            // `version.starts_with("HTTP")` matched neither this nor any of the
            // twelve value spellings — measured, `cargo xtask dev` printing
            // `the log store answers /health` for `HTTP/1.1 2ab Invented`, the
            // one row of `status-lines.tsv` that decides the digit clause.
            let protocols = [
                ["starts_with(", &format!("{quotes}HTTP/{quotes})")].concat(),
                ["starts_with(", &format!("{quotes}HTTP{quotes})")].concat(),
                ["contains(", &format!("{quotes}HTTP/{quotes})")].concat(),
            ];
            if text.lines().any(|line| {
                let code = crate::gates::line_without_a_comment(line);
                protocols.iter().any(|protocol| code.contains(protocol))
            }) {
                parses.push(crate::relative_to(root, path));
            }
        })
        .expect("the tree walks");
        parses.sort();
        assert_eq!(
            parses,
            ["crates/nc-log/src/ship.rs", "xtask/src/store.rs"],
            "{} file(s) take an answer apart as an HTTP status line: {}. Two do, and they \
             are held together by `status-lines.tsv`; a third reads the same bytes by its \
             own rules and no table relates it to either",
            parses.len(),
            parses.join(", ")
        );

        spelled.sort();
        assert_eq!(
            spelled,
            ["crates/nc-log/src/ship.rs", "xtask/src/store.rs"],
            "the 2xx rule is written in {} file(s): {}. It belongs in `served` and in \
             `nc-log`'s `Delivery` classifier, which `status-lines.tsv` holds together. A \
             third copy is a third answer, whichever of the {} spellings above it is \
             written in",
            spelled.len(),
            spelled.join(", "),
            needles.len()
        );
    }
}
