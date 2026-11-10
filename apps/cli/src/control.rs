//! Process control (`NOCTUA.md §17.1`): the four verbs only the CLI has,
//! because a daemon that is down cannot dispatch the command that would start
//! it (`§20.2`) — `start`, `stop`, `restart` and `status`.
//!
//! # A stop, in order
//!
//! 1. **The `stopped` mark**, written first and made durable — the file
//!    synced, and the state directory it is in — in the profile's persistent
//!    state directory, never a temporary one. Every restorer starts the daemon
//!    `--restored`, and a daemon started so ends at once while the mark is
//!    there (`§17.5`), so nothing that restores the daemon can undo the stop.
//! 2. **The PID record**, read once judged this user's alone
//!    (`nc_channel::record`), and **the process it names held to every part
//!    of it** (`nc_platform::process::Verified`) before anything is asked.
//! 3. **The daemon asked through its endpoint**, proven as every request is
//!    ([`crate::client`]); once it answers that it is stopping it is waited
//!    for, `stop_within_ms` at most — through what verified it, or, with
//!    nothing verified, until its endpoint answers no more and its record no
//!    longer names it.
//! 4. **Only when it does not answer, a signal — and only through what was
//!    verified**: asked to end, `kill_within_ms`, then killed, `kill_within_ms`
//!    again. **Nothing unverified is signalled, and no process numbered zero
//!    or below**: a record missing, or naming a process that runs no more, is
//!    a daemon already stopped; a record naming another process — another
//!    executable, another user — or one that cannot be read or asked about is
//!    never signalled, and the stop ends in exit 11 with `NC-CLI-0009`.
//!
//! # A start
//!
//! It removes the mark, answers at once when a daemon already answers, and
//! otherwise starts the install's own daemon, beside this CLI and never found
//! on `PATH`, detached (`nc_platform::process::spawn_detached`) with its
//! standard error written fresh into the profile's `daemon.err`; then it waits
//! `start_within_ms` for that daemon to answer the handshake. One that ends
//! first, or does not answer by then, is a start that failed (`NC-CLI-0007`),
//! and what it wrote to its standard error is shown. **No service manager is
//! asked yet**: a development install registers none.
//!
//! `restart` is a stop and then a start, the mark written and removed between
//! them; `status` is the daemon's own, or, when nothing answers, what the PID
//! record says of the process it names. A stop of a stopped daemon and a start
//! of a running one succeed.
//!
//! # What this does not answer
//!
//! A process running as the same user, which can stop the daemon, remove the
//! mark or rewrite the record; on macOS the moment between the last reading of
//! the process and the signal, which a process id handed out again in it would
//! be sent; and on Windows a terminate that is as hard as a kill, since no
//! signal there is one a process can answer (`AGENTS.md`, What is deliberately
//! not claimed).

use core::time::Duration;
use std::path::Path;

use nc_catalog::ErrorCode;
use nc_channel::Layout;
use nc_channel::found::Found;
use nc_channel::record::{self, Record, Unread};
use nc_ipc::{Answer, Request};
use nc_platform::process::{Unverified, Verified};
use nc_platform::{Judged, Kind, User};

use crate::args::Control;
use crate::client;
use crate::exit::Exit;
use crate::render::Failure;

/// What a verb that controls the daemon answers when it did what it was
/// asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// A sentence saying what was done.
    Said(String),
    /// The daemon's own status, as it answered.
    Status(nc_canon::Value),
}

/// Runs `verb` for the daemon of `found`'s install.
///
/// # Errors
///
/// [`Failure`], with the code and the exit the way it failed ends a run in.
pub async fn controlled(found: &Found, verb: Control) -> Result<Done, Failure> {
    match verb {
        Control::Start => start(found).await.map(Done::Said),
        Control::Stop => stop(found).await.map(Done::Said),
        Control::Restart => {
            let stopped = stop(found).await?;
            let started = start(found).await?;
            Ok(Done::Said(format!("{stopped}{started}")))
        }
        Control::Status => status(found).await.map(Done::Status),
    }
}

/// How long `start` waits for the daemon it started to answer.
fn start_within() -> Duration {
    Duration::from_millis(nc_catalog::DAEMON.start_within_ms)
}

/// How long `stop` waits for a daemon that answered it to end.
fn stop_within() -> Duration {
    Duration::from_millis(nc_catalog::DAEMON.stop_within_ms)
}

/// How long `stop` waits for a daemon it signalled to end.
fn kill_within() -> Duration {
    Duration::from_millis(nc_catalog::DAEMON.kill_within_ms)
}

/// How long a wait for the daemon pauses between two looks.
const BETWEEN_LOOKS: Duration = Duration::from_millis(25);

/// A failure of process control: exit 11, with `code`.
fn failed(code: ErrorCode, detail: String) -> Failure {
    Failure {
        code,
        exit: Exit::ProcessControl,
        detail,
    }
}

/// A path of the profile judged and not taken, as the failure it is.
fn not_taken(judged: Judged) -> Failure {
    match judged {
        Judged::Refused(refused) => failed(
            ErrorCode::DaemonProfileNotPrivate,
            format!("{refused}, so it was left as it was and nothing was done"),
        ),
        Judged::Failed(failed_to) => {
            failed(ErrorCode::DaemonProfileUnavailable, failed_to.to_string())
        }
    }
}

/// `doing`, which the filesystem answered with `error`, as a failure.
fn unavailable(doing: &str, path: &Path, error: &std::io::Error) -> Failure {
    failed(
        ErrorCode::DaemonProfileUnavailable,
        format!("`{}` could not be {doing}: {error}", path.display()),
    )
}

/// Whether `path` is there, as a `kind` this user's alone; nothing where it
/// is not.
fn there(path: &Path, kind: Kind, user: &User) -> Result<bool, Failure> {
    match nc_platform::judge(path, kind, user) {
        Ok(()) => Ok(true),
        Err(Judged::Failed(absent)) if absent.error().kind() == std::io::ErrorKind::NotFound => {
            Ok(false)
        }
        Err(judged) => Err(not_taken(judged)),
    }
}

/// Writes the `stopped` mark in `layout`'s state directory and makes it
/// durable: the file synced, then the directory. A mark already there is
/// judged and kept.
///
/// # Errors
///
/// [`Failure`] with `NC-DMN-0001` for a state directory or a mark another
/// account can reach, and `NC-DMN-0002` when it could not be written or
/// synced.
pub fn mark(layout: &Layout, user: &User) -> Result<(), Failure> {
    if !there(&layout.state, Kind::Directory, user)? {
        nc_platform::create_private_directory(&layout.state)
            .map_err(|error| unavailable("made", &layout.state, &error))?;
    }
    if there(&layout.stopped, Kind::File, user)? {
        return Ok(());
    }
    let file = nc_platform::create_private(&layout.stopped)
        .map_err(|error| unavailable("made", &layout.stopped, &error))?;
    file.sync_all()
        .map_err(|error| unavailable("synced", &layout.stopped, &error))?;
    drop(file);
    nc_platform::sync_directory(&layout.state)
        .map_err(|error| unavailable("synced", &layout.state, &error))
}

/// Removes the `stopped` mark from `layout`'s state directory, once judged
/// this user's alone, and makes its going durable.
///
/// # Errors
///
/// [`Failure`] with `NC-DMN-0001` for a state directory or a mark another
/// account can reach — left as it was — and `NC-DMN-0002` when it could not be
/// removed or the directory synced.
pub fn unmark(layout: &Layout, user: &User) -> Result<(), Failure> {
    if !there(&layout.state, Kind::Directory, user)? || !there(&layout.stopped, Kind::File, user)? {
        return Ok(());
    }
    std::fs::remove_file(&layout.stopped)
        .map_err(|error| unavailable("removed", &layout.stopped, &error))?;
    nc_platform::sync_directory(&layout.state)
        .map_err(|error| unavailable("synced", &layout.state, &error))
}

/// What the PID record and the system say of the daemon's process.
#[derive(Debug)]
enum Recorded {
    /// No record is there.
    Nothing,
    /// The record names a process the system shows to be it: the one a
    /// signal may reach, through this and nothing else.
    Verified(Verified, Record),
    /// The record names a process that runs no more; the text says why.
    Gone(String),
    /// The record names a process that is not the daemon, or could not be
    /// read or asked about; the text says which. Never signalled.
    Unverified(String),
}

/// What the record of `found`'s install names, held to the system.
fn recorded(found: &Found) -> Recorded {
    match record::read(&found.layout.pid_file, &found.user) {
        Err(Unread::Missing) => Recorded::Nothing,
        Err(unread) => Recorded::Unverified(unread.to_string()),
        Ok(record) => match Verified::open(&record.identity()) {
            Ok(verified) => Recorded::Verified(verified, record),
            Err(Unverified::Gone(why)) => Recorded::Gone(why),
            Err(unverified) => Recorded::Unverified(format!(
                "the PID record names process {}, and {unverified}",
                record.pid
            )),
        },
    }
}

/// What a stop does once the daemon did not answer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unanswered {
    /// Signal it, through what verified it.
    Signal,
    /// Nothing: no daemon runs.
    Stopped,
    /// Nothing, and say so: what the record names is not a process this may
    /// signal.
    Refuse,
}

/// What a stop does with what the record said, once the daemon did not
/// answer: **only a process verified is signalled**.
const fn when_unanswered(recorded: &Recorded) -> Unanswered {
    match recorded {
        Recorded::Verified(..) => Unanswered::Signal,
        Recorded::Nothing | Recorded::Gone(_) => Unanswered::Stopped,
        Recorded::Unverified(_) => Unanswered::Refuse,
    }
}

/// Stops the daemon of `found`'s install, as this module's header says, and
/// answers what was done.
async fn stop(found: &Found) -> Result<String, Failure> {
    let cli = &found.layout.cli;
    mark(&found.layout, &found.user)?;
    let recorded = recorded(found);
    if let Ok(mut connection) = client::connect(found).await
        && let Ok(Answer::Stopping) = connection.ask(&Request::Stop).await
    {
        let pid = connection.welcome().pid;
        drop(connection);
        return ended_once_asked(found, recorded, pid).await;
    }
    let unanswered = when_unanswered(&recorded);
    match recorded {
        Recorded::Verified(verified, _) if unanswered == Unanswered::Signal => {
            signalled(cli, &verified)
        }
        Recorded::Gone(why) if unanswered == Unanswered::Stopped => Ok(format!(
            "The daemon of {cli} is stopped: nothing answers at its endpoint, and {why}.\n"
        )),
        Recorded::Nothing if unanswered == Unanswered::Stopped => Ok(format!(
            "The daemon of {cli} is stopped: nothing answers at its endpoint.\n"
        )),
        // Refused, and anything the decision did not answer: never signalled.
        Recorded::Unverified(why) | Recorded::Gone(why) => Err(unverified(cli, &why)),
        Recorded::Verified(_, record) => Err(unverified(
            cli,
            &format!(
                "the PID record names process {}, which was not signalled",
                record.pid
            ),
        )),
        Recorded::Nothing => Err(unverified(cli, "no PID record is there")),
    }
}

/// The failure of a stop the daemon did not answer, whose record names
/// nothing this may signal, `why`.
fn unverified(cli: &str, why: &str) -> Failure {
    failed(
        ErrorCode::CliProcessUnverified,
        format!(
            "the daemon of {cli} did not answer, and {why}, so nothing was signalled; the \
             `stopped` mark is written, so no restorer starts it"
        ),
    )
}

/// Waits for a daemon that answered it is stopping, process `pid`, to end.
async fn ended_once_asked(found: &Found, recorded: Recorded, pid: u32) -> Result<String, Failure> {
    let cli = &found.layout.cli;
    if let Recorded::Verified(verified, record) = recorded
        && record.pid == pid
    {
        if verified.wait(stop_within()).unwrap_or(false) {
            return Ok(format!(
                "The daemon of {cli}, process {pid}, stopped when it was asked to.\n"
            ));
        }
        return signalled(cli, &verified);
    }
    // Nothing verified the process that answered, so it is waited for until
    // its endpoint answers no more and its record no longer names it, and
    // never signalled.
    let endpoint = nc_platform::ipc::Endpoint::new(
        &found.layout.runtime,
        &found.layout.socket,
        found.layout.pipe.as_str(),
    );
    let deadline = tokio::time::Instant::now() + stop_within();
    loop {
        let listening = nc_platform::ipc::connect(&endpoint, &found.user)
            .await
            .map_or_else(
                |refused| !matches!(refused, nc_platform::ipc::Refused::Nobody(_)),
                |_| true,
            );
        let named =
            record::read(&found.layout.pid_file, &found.user).is_ok_and(|record| record.pid == pid);
        if !listening && !named {
            return Ok(format!(
                "The daemon of {cli}, process {pid}, stopped when it was asked to.\n"
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(failed(
                ErrorCode::CliDaemonDidNotStop,
                format!(
                    "the daemon of {cli}, process {pid}, said it was stopping and was still \
                     there after {} ms; it was not signalled, since nothing held it to its \
                     record",
                    nc_catalog::DAEMON.stop_within_ms
                ),
            ));
        }
        tokio::time::sleep(BETWEEN_LOOKS).await;
    }
}

/// Ends `verified` — asked first, then killed — and answers what was done.
fn signalled(cli: &str, verified: &Verified) -> Result<String, Failure> {
    let pid = verified.pid();
    let unsent = |error: std::io::Error| {
        failed(
            ErrorCode::CliDaemonDidNotStop,
            format!("the daemon of {cli}, process {pid}, could not be signalled: {error}"),
        )
    };
    verified.terminate().map_err(unsent)?;
    if verified.wait(kill_within()).unwrap_or(false) {
        return Ok(format!(
            "The daemon of {cli}, process {pid}, did not answer, and was asked to end through \
             the process its record names; it ended.\n"
        ));
    }
    verified.kill().map_err(unsent)?;
    if verified.wait(kill_within()).unwrap_or(false) {
        return Ok(format!(
            "The daemon of {cli}, process {pid}, did not answer or end when asked, and was \
             killed through the process its record names.\n"
        ));
    }
    Err(failed(
        ErrorCode::CliDaemonDidNotStop,
        format!(
            "the daemon of {cli}, process {pid}, was killed and had not ended {} ms later",
            nc_catalog::DAEMON.kill_within_ms
        ),
    ))
}

/// Starts the daemon of `found`'s install, as this module's header says, and
/// answers what was done.
async fn start(found: &Found) -> Result<String, Failure> {
    let (layout, cli) = (&found.layout, &found.layout.cli);
    unmark(layout, &found.user)?;
    if let Ok(connection) = client::connect(found).await {
        return Ok(format!(
            "The daemon of {cli} is running already, as process {}.\n",
            connection.welcome().pid
        ));
    }
    let daemon = layout.install.join(format!(
        "{}{}",
        layout.daemon,
        nc_platform::EXECUTABLE_SUFFIX
    ));
    let error = standard_error(layout, &found.user)?;
    let mut detached =
        nc_platform::process::spawn_detached(&daemon, &[], error).map_err(|error| {
            failed(
                ErrorCode::CliDaemonDidNotStart,
                format!("`{}` could not be started: {error}", daemon.display()),
            )
        })?;
    let deadline = tokio::time::Instant::now() + start_within();
    loop {
        if let Ok(connection) = client::connect(found).await {
            return Ok(running(connection, cli, deadline).await);
        }
        let ended = detached.ended().unwrap_or(None);
        if ended.is_some() || tokio::time::Instant::now() >= deadline {
            let how = ended.map_or_else(
                || {
                    format!(
                        "did not answer within {} ms",
                        nc_catalog::DAEMON.start_within_ms
                    )
                },
                |status| format!("ended ({status}) before it answered"),
            );
            return Err(failed(
                ErrorCode::CliDaemonDidNotStart,
                format!(
                    "the daemon of {cli}, process {}, {how}; its standard error ends: {}",
                    detached.pid(),
                    tail_of(&layout.daemon_err)
                ),
            ));
        }
        tokio::time::sleep(BETWEEN_LOOKS).await;
    }
}

/// What a start says of the daemon it reached over `connection`.
///
/// Once its status says it runs — a command asked of it then is served — or
/// the state it is in when `deadline` comes first.
async fn running(
    mut connection: client::Connection,
    cli: &str,
    deadline: tokio::time::Instant,
) -> String {
    let pid = connection.welcome().pid;
    let running = format!("\"state\":\"{}\"", nc_ipc::State::Running.word());
    loop {
        let state = match connection.ask(&Request::Status).await {
            Ok(Answer::Status(status)) => status.to_canonical(),
            _ => String::new(),
        };
        if state.contains(&running) {
            return format!("The daemon of {cli} is running, as process {pid}.\n");
        }
        if tokio::time::Instant::now() >= deadline {
            return format!(
                "The daemon of {cli} answers, as process {pid}, and is still starting; it runs \
                 commands once its status says it runs.\n"
            );
        }
        tokio::time::sleep(BETWEEN_LOOKS).await;
    }
}

/// The daemon's standard error, written fresh in the profile's log
/// directory: the directory made or judged, and a file left by an earlier
/// start judged before it is replaced.
fn standard_error(layout: &Layout, user: &User) -> Result<std::fs::File, Failure> {
    if !there(&layout.log, Kind::Directory, user)? {
        nc_platform::create_private_directory(&layout.log)
            .map_err(|error| unavailable("made", &layout.log, &error))?;
    }
    if there(&layout.daemon_err, Kind::File, user)? {
        std::fs::remove_file(&layout.daemon_err)
            .map_err(|error| unavailable("removed", &layout.daemon_err, &error))?;
    }
    nc_platform::create_private(&layout.daemon_err)
        .map_err(|error| unavailable("made", &layout.daemon_err, &error))
}

/// The last lines a daemon wrote to its standard error, which a failed start
/// shows.
fn tail_of(path: &Path) -> String {
    /// The most lines shown.
    const LINES: usize = 20;
    let written = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = written.lines().collect();
    let shown = lines[lines.len().saturating_sub(LINES)..].join(" | ");
    if shown.is_empty() {
        "nothing".to_owned()
    } else {
        shown
    }
}

/// The daemon's own status, or why there is none and what its record says.
async fn status(found: &Found) -> Result<nc_canon::Value, Failure> {
    let unasked = match client::connect(found).await {
        Ok(mut connection) => {
            return match connection.ask(&Request::Status).await {
                Ok(Answer::Status(status)) => Ok(status),
                Ok(_) => Err(Failure {
                    code: ErrorCode::CliAnswerUnreadable,
                    exit: Exit::Internal,
                    detail: "a status was asked, and the daemon answered with something else"
                        .to_owned(),
                }),
                Err(unasked) => Err(Failure {
                    code: unasked.code(),
                    exit: unasked.exit(),
                    detail: unasked.detail(),
                }),
            };
        }
        Err(unasked) => unasked,
    };
    let said = match recorded(found) {
        Recorded::Nothing => "no PID record is there".to_owned(),
        Recorded::Verified(_, record) => format!(
            "its PID record names process {}, which runs as this channel's daemon and does not \
             answer",
            record.pid
        ),
        Recorded::Gone(why) => format!("its PID record names a process that runs no more: {why}"),
        Recorded::Unverified(why) => why,
    };
    Err(Failure {
        code: unasked.code(),
        exit: unasked.exit(),
        detail: format!("{}; {said}", unasked.detail()),
    })
}

#[cfg(test)]
mod tests {
    use nc_catalog::ErrorCode;
    use nc_ipc::{Answer, Request, Token};

    use super::{Recorded, Unanswered, when_unanswered};
    use crate::args::Control;
    use crate::client::scripted::{Scratch, Script, Scripted};
    use crate::exit::Exit;

    /// Runs `verb` for `installed`'s install on a runtime of its own.
    fn controlled(
        installed: &Scratch,
        verb: Control,
    ) -> Result<super::Done, crate::render::Failure> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(super::controlled(&installed.found, verb))
    }

    /// The PID record of `installed` written as `text`, private.
    fn recorded_as(installed: &Scratch, text: &str) {
        use std::io::Write as _;
        let path = &installed.found.layout.pid_file;
        let _ = std::fs::remove_file(path);
        nc_platform::create_private(path)
            .expect("a record")
            .write_all(text.as_bytes())
            .expect("written");
    }

    /// The record of this test process as the system says it is, changed.
    ///
    /// **Never left whole** by `change`: a record naming this process as it is
    /// would be one a stop may signal.
    fn this_process_but(change: impl FnOnce(&mut nc_channel::record::Record)) -> String {
        let mut record = nc_channel::record::Record::of(
            nc_platform::process::this_process().expect("this process is read"),
            [7; 16],
        );
        let whole = record.clone();
        change(&mut record);
        assert_ne!(
            record, whole,
            "a test must never record this process as it is"
        );
        record.to_canonical().expect("written")
    }

    /// Only a process verified is signalled once the daemon does not answer;
    /// no record, or one naming a process that runs no more, is a daemon
    /// stopped; and anything else is refused.
    #[test]
    fn only_a_verified_process_is_signalled_when_the_daemon_does_not_answer() {
        assert_eq!(when_unanswered(&Recorded::Nothing), Unanswered::Stopped);
        assert_eq!(
            when_unanswered(&Recorded::Gone("it ended".to_owned())),
            Unanswered::Stopped
        );
        assert_eq!(
            when_unanswered(&Recorded::Unverified("another".to_owned())),
            Unanswered::Refuse
        );
    }

    /// What tells a child of
    /// [`a_stop_nothing_answers_ends_the_process_its_record_is_held_to`] that
    /// it is one: it says its record, then waits for its input to close.
    const SLEEPER: &str = "NC_CLI_CONTROL_SLEEPER";

    /// A child of this test binary, killed and waited for however the test
    /// leaves.
    struct Child(std::process::Child);

    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// A stop nothing answers ends the process its record is held to, and
    /// through what held it: a child of this test, recorded as it is.
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the process a stop ends has to be another one than this test; the child is \
                  this same test binary, which says its record on its output and waits"
    )]
    fn a_stop_nothing_answers_ends_the_process_its_record_is_held_to() {
        use std::io::{BufRead as _, Read as _, Write as _};
        if std::env::var_os(SLEEPER).is_some() {
            let record = nc_channel::record::Record::of(
                nc_platform::process::this_process().expect("this process is read"),
                [9; 16],
            );
            let mut out = std::io::stdout().lock();
            writeln!(out, "record\t{}", record.to_canonical().expect("written")).expect("said");
            out.flush().expect("said");
            let mut rest = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut rest);
            return;
        }
        let installed = Scratch::new("verified");
        let mut child = Child(
            std::process::Command::new(std::env::current_exe().expect("this test binary"))
                .args([
                    "--exact",
                    "control::tests::a_stop_nothing_answers_ends_the_process_its_record_is_held_to",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(SLEEPER, "1")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("the child starts"),
        );
        let mut lines = std::io::BufReader::new(child.0.stdout.take().expect("piped"));
        let mut line = String::new();
        let record = loop {
            line.clear();
            assert!(
                lines
                    .read_line(&mut line)
                    .expect("the child's output reads")
                    > 0,
                "the child ended without saying its record"
            );
            if let Some(at) = line.find("record\t") {
                break line[at + "record\t".len()..].trim_end().to_owned();
            }
        };
        recorded_as(&installed, &record);
        let done = controlled(&installed, Control::Stop).expect("the child is ended");
        assert!(
            matches!(&done, super::Done::Said(said) if said.contains("asked to end")),
            "{done:?}"
        );
        let status = child.0.wait().expect("reaped");
        assert!(!status.success(), "the child ended on its own: {status:?}");
    }

    /// The mark is written private and kept when it is there already, and
    /// removed once judged; one another account can reach is refused both
    /// ways and left as it was.
    #[test]
    fn the_mark_is_written_private_and_removed_once_judged() {
        let installed = Scratch::new("mark");
        let (layout, user) = (&installed.found.layout, &installed.found.user);
        super::mark(layout, user).expect("written");
        assert!(nc_platform::testing::is_private(&layout.stopped));
        super::mark(layout, user).expect("kept");
        super::unmark(layout, user).expect("removed");
        assert!(!layout.stopped.exists());
        super::unmark(layout, user).expect("nothing to remove is no error");
        super::mark(layout, user).expect("written again");
        nc_platform::testing::open_to_another_user(&layout.stopped).expect("opened");
        for refused in [super::mark(layout, user), super::unmark(layout, user)] {
            let refused = refused.expect_err("a mark another account can reach");
            assert_eq!(refused.code, ErrorCode::DaemonProfileNotPrivate);
            assert_eq!(refused.exit, Exit::ProcessControl);
        }
        assert!(
            layout.stopped.is_file(),
            "a refused mark was not left as it was"
        );
        nc_platform::testing::make_private(&layout.stopped).expect("private again");
    }

    /// A stop with nothing answering writes the mark, and ends well when no
    /// record names a process or the one it names runs no more; a record
    /// naming another process, or one that does not read, is never signalled
    /// and ends in 11.
    ///
    /// **This test process stands in for the other process**, recorded with
    /// one part changed, so a stop that signalled it would end the test.
    #[test]
    fn a_stop_nothing_answers_signals_nothing_it_did_not_verify() {
        let installed = Scratch::new("silent");
        let layout = &installed.found.layout;
        let done = controlled(&installed, Control::Stop).expect("nothing runs");
        assert!(layout.stopped.is_file(), "the stop wrote no mark");
        assert!(matches!(done, super::Done::Said(said) if said.contains("is stopped")));
        recorded_as(
            &installed,
            &this_process_but(|record| record.started.push('0')),
        );
        let done = controlled(&installed, Control::Stop).expect("its process runs no more");
        assert!(
            matches!(&done, super::Done::Said(said) if said.contains("handed out again")),
            "{done:?}"
        );
        for record in [
            this_process_but(|record| record.executable.set_file_name("noctua-daemon")),
            "{\"pid\":0}".to_owned(),
        ] {
            recorded_as(&installed, &record);
            let refused = controlled(&installed, Control::Stop).expect_err("not verified");
            assert_eq!(refused.code, ErrorCode::CliProcessUnverified, "{refused:?}");
            assert_eq!(refused.exit, Exit::ProcessControl);
            assert!(
                refused.detail.contains("nothing was signalled"),
                "{refused:?}"
            );
        }
    }

    /// A stop the daemon answers is asked through the endpoint and waited
    /// for until the endpoint answers no more, and a status is the daemon's
    /// own.
    #[test]
    fn a_stop_is_asked_of_the_daemon_and_a_status_is_its_own() {
        let installed = Scratch::new("asked");
        let token = Token::fresh().expect("a token");
        installed.token(&token);
        let status = nc_canon::Value::object([("state", nc_canon::Value::text("running"))]);
        let daemon = Scripted::serve(
            &installed,
            Script {
                token,
                welcome: installed.welcome(),
                connections: vec![vec![Answer::Status(status.clone())], vec![Answer::Stopping]],
            },
        );
        assert_eq!(
            controlled(&installed, Control::Status).expect("answered"),
            super::Done::Status(status)
        );
        let done = controlled(&installed, Control::Stop).expect("stopped");
        assert!(
            matches!(&done, super::Done::Said(said) if said.contains("stopped when it was asked")),
            "{done:?}"
        );
        assert_eq!(daemon.heard(), vec![Request::Status, Request::Stop]);
        assert!(installed.found.layout.stopped.is_file());
    }

    /// A status with nothing answering ends as the connection did, and says
    /// what the record names.
    #[test]
    fn a_status_with_nothing_answering_says_what_the_record_names() {
        let installed = Scratch::new("status");
        let refused = controlled(&installed, Control::Status).expect_err("nothing answers");
        assert_eq!(refused.exit, Exit::Unreachable, "{refused:?}");
        assert!(refused.detail.contains("no PID record"), "{refused:?}");
        recorded_as(
            &installed,
            &this_process_but(|record| record.started.push('0')),
        );
        let refused = controlled(&installed, Control::Status).expect_err("nothing answers");
        assert!(refused.detail.contains("runs no more"), "{refused:?}");
    }

    /// A start removes the mark and answers at once when a daemon already
    /// answers; with none, and no daemon beside it to start, it ends in 11.
    #[test]
    fn a_start_removes_the_mark_and_fails_without_a_daemon_to_start() {
        let installed = Scratch::new("start");
        let layout = &installed.found.layout;
        super::mark(layout, &installed.found.user).expect("written");
        let refused = controlled(&installed, Control::Start).expect_err("no daemon is beside it");
        assert_eq!(refused.code, ErrorCode::CliDaemonDidNotStart, "{refused:?}");
        assert_eq!(refused.exit, Exit::ProcessControl);
        assert!(!layout.stopped.exists(), "the start left the mark");
        let token = Token::fresh().expect("a token");
        installed.token(&token);
        let daemon = Scripted::serve(
            &installed,
            Script {
                token,
                welcome: installed.welcome(),
                connections: vec![vec![]],
            },
        );
        super::mark(layout, &installed.found.user).expect("written");
        let done = controlled(&installed, Control::Start).expect("running already");
        assert!(
            matches!(&done, super::Done::Said(said) if said.contains("running already")),
            "{done:?}"
        );
        assert!(!layout.stopped.exists(), "the start left the mark");
        assert_eq!(daemon.heard(), Vec::<Request>::new());
    }
}
