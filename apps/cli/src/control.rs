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
//!    longer names it — and **never signalled**: one still there by then is a
//!    stop that did not finish (`NC-CLI-0008`, exit 11).
//! 4. **Only when it does not answer, a signal — and only through what was
//!    verified**: asked to end, `kill_within_ms`, then killed, `kill_within_ms`
//!    again, once its record, read again, still names it. A daemon removes its
//!    record at the end of its own stop and only then lets its endpoint go, so
//!    one whose record went since step 2 is ending a stop asked before this
//!    one, and is waited for as in step 3, never signalled. **Nothing
//!    unverified is signalled, and no process numbered zero or below**: a
//!    record missing, or naming a process that runs no more — or one this
//!    user may not read, which is not its daemon — is a daemon already
//!    stopped while nothing holds its endpoint; a record naming another
//!    process — another executable, another user — or one that does not
//!    read, and an endpoint something still holds with no record to hold it
//!    to a process, are never signalled, and the stop ends in exit 11 with
//!    `NC-CLI-0009`.
//!
//! # A start
//!
//! It removes the mark and answers at once when a daemon already answers — but
//! for one that says it is stopping, which it waits for as a stop does and then
//! starts again. Otherwise, **where this system's service manager holds the
//! install's restorer** (`cargo xtask dev --install` registers one), it asks
//! that manager to start it — `noctua-daemon --restored`, as every restorer
//! starts it — so the daemon it starts is one the manager restores; where the
//! manager holds none, or there is none to ask, it starts the install's own
//! daemon, beside this CLI and never found on `PATH`, detached
//! (`nc_platform::process::spawn_detached`) with its standard error written
//! fresh into the profile's `daemon.err`. Either way it waits `start_within_ms`
//! for that daemon to answer the handshake. One that does not answer by then,
//! or that a detached start sees end first, is a start that failed
//! (`NC-CLI-0007`), with what it wrote to its standard error, or what its
//! manager says of it.
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
use nc_ipc::{Answer, IpcError, Request};
use nc_platform::ipc::Refused;
use nc_platform::process::{Unverified, Verified};
use nc_platform::service::Registrar;
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

/// Whether the PID record of `found`'s install still names what `record` named.
///
/// Read again once the daemon did not answer, so a record the daemon removed
/// meanwhile, or one another start wrote over it, is told from one that stayed.
fn still_names(found: &Found, record: &Record) -> bool {
    record::read(&found.layout.pid_file, &found.user).is_ok_and(|now| now == *record)
}

/// What a stop does once the daemon did not answer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unanswered {
    /// Signal it, through what verified it.
    Signal,
    /// Wait for it through what verified it, and never signal it: it is ending
    /// a stop.
    Wait,
    /// Nothing: no daemon runs.
    Stopped,
    /// Nothing, and say so: what the record names is not a process this may
    /// signal.
    Refuse,
}

/// What a stop does with what the record said, once the daemon did not
/// answer: **only a process verified is signalled**, and only while its
/// record still names it.
///
/// With no record, or one naming a process that runs no more, the daemon is
/// stopped only while nothing at all `listening` holds its endpoint; one that
/// does and did not answer is a daemon hung with its record lost, which
/// nothing here can hold to a process, and so is refused.
///
/// **A record gone once it was read is a stop ending**, whatever holds the
/// endpoint: a daemon removes its record at the end of its own stop, while
/// its endpoint still answers that it is stopping, and lets the endpoint go
/// only after — so a process verified whose record no longer names it when
/// read again (`still_recorded`) is one a stop asked before this one is
/// ending, waited for as one that answered and never signalled.
const fn when_unanswered(recorded: &Recorded, listening: bool, still_recorded: bool) -> Unanswered {
    match recorded {
        Recorded::Verified(..) if still_recorded => Unanswered::Signal,
        Recorded::Verified(..) => Unanswered::Wait,
        Recorded::Nothing | Recorded::Gone(_) if listening => Unanswered::Refuse,
        Recorded::Nothing | Recorded::Gone(_) => Unanswered::Stopped,
        Recorded::Unverified(_) => Unanswered::Refuse,
    }
}

/// Whether anything holds the endpoint that a connection which failed with
/// the refusal `unasked` was made to.
///
/// Nothing does where nothing answers, or where the endpoint's name is longer
/// than this system lets a socket have, so none can be bound there; anything
/// else on the way to a request is something there, or may be, and is taken
/// for one.
fn listening_after(unasked: &client::Unasked) -> bool {
    match unasked {
        client::Unasked::Connecting(IpcError::Endpoint(refused)) => {
            !matches!(refused, Refused::Nobody(_) | Refused::TooLong { .. })
        }
        _ => true,
    }
}

/// Stops the daemon of `found`'s install, as this module's header says, and
/// answers what was done.
async fn stop(found: &Found) -> Result<String, Failure> {
    let cli = &found.layout.cli;
    mark(&found.layout, &found.user)?;
    let recorded = recorded(found);
    let listening = match client::connect(found).await {
        Ok(mut connection) => {
            if let Ok(Answer::Stopping) = connection.ask(&Request::Stop).await {
                let pid = connection.welcome().pid;
                drop(connection);
                return ended_once_asked(found, recorded, pid, stop_within()).await;
            }
            true
        }
        Err(unasked) => listening_after(&unasked),
    };
    let still_recorded =
        matches!(&recorded, Recorded::Verified(_, record) if still_names(found, record));
    let unanswered = when_unanswered(&recorded, listening, still_recorded);
    match recorded {
        Recorded::Verified(verified, _) if unanswered == Unanswered::Signal => {
            signalled(cli, &verified)
        }
        Recorded::Verified(verified, record) if unanswered == Unanswered::Wait => {
            let pid = record.pid;
            ended_once_asked(
                found,
                Recorded::Verified(verified, record),
                pid,
                stop_within(),
            )
            .await
        }
        Recorded::Gone(why) if unanswered == Unanswered::Stopped => Ok(format!(
            "The daemon of {cli} is stopped: nothing answers at its endpoint, and {why}.\n"
        )),
        Recorded::Nothing if unanswered == Unanswered::Stopped => Ok(format!(
            "The daemon of {cli} is stopped: nothing answers at its endpoint.\n"
        )),
        // Refused, and anything the decision did not answer: never signalled.
        Recorded::Unverified(why) => Err(unverified(cli, &why)),
        Recorded::Gone(why) => Err(unverified(
            cli,
            &format!(
                "something still holds its endpoint, and its PID record names a process that \
                 runs no more ({why})"
            ),
        )),
        Recorded::Verified(_, record) => Err(unverified(
            cli,
            &format!(
                "the PID record names process {}, which was not signalled",
                record.pid
            ),
        )),
        Recorded::Nothing => Err(unverified(
            cli,
            "something still holds its endpoint, and no PID record is there to hold it to a \
             process",
        )),
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

/// Waits `within` for a daemon that answered it is stopping, process `pid`, to
/// end, and **never signals it**: a daemon that answers is not silent, and
/// only a silent one is signalled.
async fn ended_once_asked(
    found: &Found,
    recorded: Recorded,
    pid: u32,
    within: Duration,
) -> Result<String, Failure> {
    let cli = &found.layout.cli;
    if let Recorded::Verified(verified, record) = recorded
        && record.pid == pid
    {
        if verified.wait(within).unwrap_or(false) {
            return Ok(format!(
                "The daemon of {cli}, process {pid}, stopped when it was asked to.\n"
            ));
        }
        return Err(still_there(
            cli,
            pid,
            within,
            "since a daemon that answers a stop is never signalled",
        ));
    }
    // Nothing verified the process that answered, so it is waited for until
    // its endpoint answers no more and its record no longer names it, and
    // never signalled.
    let endpoint = nc_platform::ipc::Endpoint::new(
        &found.layout.runtime,
        &found.layout.socket,
        found.layout.pipe.as_str(),
    );
    let deadline = tokio::time::Instant::now() + within;
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
            return Err(still_there(
                cli,
                pid,
                within,
                "since nothing held it to its record",
            ));
        }
        tokio::time::sleep(BETWEEN_LOOKS).await;
    }
}

/// The failure of a stop whose daemon, process `pid`, said it was stopping and
/// was still there `within` later, not signalled for the reason `why`.
fn still_there(cli: &str, pid: u32, within: Duration, why: &str) -> Failure {
    failed(
        ErrorCode::CliDaemonDidNotStop,
        format!(
            "the daemon of {cli}, process {pid}, said it was stopping and was still there after \
             {} ms; it was not signalled, {why}, and a stop asked again waits for it again",
            within.as_millis()
        ),
    )
}

/// What a stop ends a daemon that does not answer through.
///
/// [`Verified`] in a run, and nothing else: a stand-in only in this module's
/// tests, which hold the order of [`signalled`] without a process to end.
trait Ends {
    /// Its process id.
    fn pid(&self) -> u32;
    /// Asks it to end.
    fn terminate(&self) -> std::io::Result<()>;
    /// Ends it outright.
    fn kill(&self) -> std::io::Result<()>;
    /// Whether it has ended within `within`.
    fn wait(&self, within: Duration) -> std::io::Result<bool>;
}

impl Ends for Verified {
    fn pid(&self) -> u32 {
        Self::pid(self)
    }

    fn terminate(&self) -> std::io::Result<()> {
        Self::terminate(self)
    }

    fn kill(&self) -> std::io::Result<()> {
        Self::kill(self)
    }

    fn wait(&self, within: Duration) -> std::io::Result<bool> {
        Self::wait(self, within)
    }
}

/// Ends `process` — asked first, then killed — and answers what was done.
fn signalled(cli: &str, process: &impl Ends) -> Result<String, Failure> {
    let pid = process.pid();
    let unsent = |error: std::io::Error| {
        failed(
            ErrorCode::CliDaemonDidNotStop,
            format!("the daemon of {cli}, process {pid}, could not be signalled: {error}"),
        )
    };
    process.terminate().map_err(unsent)?;
    if process.wait(kill_within()).unwrap_or(false) {
        return Ok(format!(
            "The daemon of {cli}, process {pid}, did not answer, and was asked to end through \
             the process its record names; it ended.\n"
        ));
    }
    process.kill().map_err(unsent)?;
    if process.wait(kill_within()).unwrap_or(false) {
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
        let pid = connection.welcome().pid;
        if connection.welcome().state != nc_ipc::State::Stopping {
            return Ok(format!(
                "The daemon of {cli} is running already, as process {pid}.\n"
            ));
        }
        // **A daemon winding down a stop is waited for, then started**: told
        // it runs already, the start would leave nothing running once the
        // stop it found ends. Waited for as a stop waits, and never signalled.
        drop(connection);
        ended_once_asked(found, recorded(found), pid, stop_within()).await?;
    }
    if let Some((registrar, name)) = restorer_of(found) {
        return started_by(found, &registrar, &name).await;
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
        let ended = detached
            .ended()
            .unwrap_or(None)
            .map(|status| status.to_string());
        if let Some(how) = given_up(ended, tokio::time::Instant::now() >= deadline) {
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

/// The service manager holding the restorer of `found`'s install.
///
/// This system's manager, and the name it knows the daemon by, when it holds
/// one; nothing otherwise.
fn restorer_of(found: &Found) -> Option<(Registrar, String)> {
    let registrar = Registrar::for_user(&found.user).ok()?;
    let name = crate::install::restorer_name(&found.layout, registrar.manager()).to_owned();
    starts_through(&registrar.is_registered(&name)).then_some((registrar, name))
}

/// Whether a start goes through the service manager, whose answer to whether
/// it holds the install's restorer is `registered`.
///
/// **Only where it says it does.** One that holds none, or does not answer,
/// restores nothing a start could hand it, so the start is a detached one —
/// the one way the daemon then runs — and the instance lock refuses a second
/// daemon whichever way the first was started.
const fn starts_through(registered: &Result<bool, nc_platform::service::Error>) -> bool {
    matches!(registered, Ok(true))
}

/// Starts the daemon of `found`'s install through its service manager.
///
/// `registrar` knows it as `name`; the start waits `start_within_ms` for it to
/// answer.
async fn started_by(found: &Found, registrar: &Registrar, name: &str) -> Result<String, Failure> {
    let cli = &found.layout.cli;
    let manager = registrar.manager();
    registrar.start(name).map_err(|error| {
        failed(
            ErrorCode::CliDaemonDidNotStart,
            format!("{manager} did not start the daemon of {cli}, `{name}`: {error}"),
        )
    })?;
    let deadline = tokio::time::Instant::now() + start_within();
    loop {
        if let Ok(connection) = client::connect(found).await {
            let running = running(connection, cli, deadline).await;
            return Ok(format!(
                "{running}{manager} started it, as `{name}`, and restores it if it dies.\n"
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            let says = registrar
                .counters(name)
                .map_or_else(|error| error.to_string(), |counted| format!("{counted:?}"));
            return Err(failed(
                ErrorCode::CliDaemonDidNotStart,
                format!(
                    "the daemon of {cli}, started by {manager} as `{name}`, did not answer within \
                     {} ms; {manager} says {says}",
                    nc_catalog::DAEMON.start_within_ms
                ),
            ));
        }
        tokio::time::sleep(BETWEEN_LOOKS).await;
    }
}

/// Why a start gives up on the daemon it started, or nothing while it waits.
///
/// At once for one that `ended`, as the system says it did, before it
/// answered; and for one silent still only once `past` the deadline.
fn given_up(ended: Option<String>, past: bool) -> Option<String> {
    match ended {
        Some(status) => Some(format!("ended ({status}) before it answered")),
        None if past => Some(format!(
            "did not answer within {} ms",
            nc_catalog::DAEMON.start_within_ms
        )),
        None => None,
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
    /// stopped while nothing holds its endpoint; and anything else is refused.
    #[test]
    fn only_a_verified_process_is_signalled_when_the_daemon_does_not_answer() {
        let gone = || Recorded::Gone("it ended".to_owned());
        for still in [false, true] {
            assert_eq!(
                when_unanswered(&Recorded::Nothing, false, still),
                Unanswered::Stopped
            );
            assert_eq!(when_unanswered(&gone(), false, still), Unanswered::Stopped);
            // Something that holds the endpoint and did not answer, with no
            // record holding it to a process, is a daemon hung and never stopped.
            assert_eq!(
                when_unanswered(&Recorded::Nothing, true, still),
                Unanswered::Refuse
            );
            assert_eq!(when_unanswered(&gone(), true, still), Unanswered::Refuse);
            for listening in [false, true] {
                assert_eq!(
                    when_unanswered(
                        &Recorded::Unverified("another".to_owned()),
                        listening,
                        still
                    ),
                    Unanswered::Refuse
                );
            }
        }
    }

    /// A verified process is signalled only while its record still names it.
    ///
    /// One whose record went once the stop read it, or was written over, is a
    /// daemon ending a stop asked before — it removes its record before its
    /// endpoint goes — and is waited for, whatever holds the endpoint. **Found
    /// while closing a review's finding**: a stop asked again that read the
    /// record just before a draining daemon removed it, and reached the
    /// endpoint just after it went, signalled the daemon.
    #[test]
    fn a_verified_process_is_signalled_only_while_its_record_still_names_it() {
        let installed = Scratch::new("renamed");
        let (_child, record) = a_sleeper();
        recorded_as(&installed, &record);
        let recorded = super::recorded(&installed.found);
        let Recorded::Verified(_, read) = &recorded else {
            panic!("the child is not held to its record: {recorded:?}");
        };
        assert!(super::still_names(&installed.found, read));
        for listening in [false, true] {
            assert_eq!(
                when_unanswered(&recorded, listening, true),
                Unanswered::Signal
            );
            assert_eq!(
                when_unanswered(&recorded, listening, false),
                Unanswered::Wait
            );
        }
        recorded_as(
            &installed,
            &this_process_but(|other| other.started.push('0')),
        );
        assert!(
            !super::still_names(&installed.found, read),
            "a record written over still names what it named"
        );
        std::fs::remove_file(&installed.found.layout.pid_file).expect("removed");
        assert!(
            !super::still_names(&installed.found, read),
            "a record removed still names what it named"
        );
    }

    /// An endpoint no socket can have is held by nothing, and every other
    /// refusal on the way to a request is something there.
    ///
    /// It is the one that stood for a stopped daemon on macOS, where a
    /// relocated install's socket path is longer than the system holds: taken
    /// for something there, it turned a stop of a stopped daemon into a
    /// refusal.
    #[test]
    fn an_endpoint_no_socket_can_have_is_held_by_nothing() {
        use nc_ipc::IpcError;
        use nc_platform::ipc::Refused;
        let endpoint = |refused| super::client::Unasked::Connecting(IpcError::Endpoint(refused));
        assert!(!super::listening_after(&endpoint(Refused::TooLong {
            name: "s".to_owned(),
            length: 200,
        })));
        for there in [
            endpoint(Refused::NoProcess),
            endpoint(Refused::Busy { connections: 1 }),
            super::client::Unasked::Handshake(IpcError::Slow),
            super::client::Unasked::TokenUnreadable("absent".to_owned()),
        ] {
            assert!(super::listening_after(&there), "{there:?}");
        }
    }

    /// What tells a child of [`a_sleeper`] that it is one: it says its
    /// record, then waits for its input to close.
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

    /// A child of this test binary that says its record and waits for its
    /// input to close, and the record it said, as a daemon writes one.
    #[expect(
        clippy::disallowed_methods,
        reason = "the process a stop is about has to be another one than this test; the child \
                  is this same test binary, its input a pipe the guard closes"
    )]
    fn a_sleeper() -> (Child, String) {
        use std::io::BufRead as _;
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
        loop {
            line.clear();
            assert!(
                lines
                    .read_line(&mut line)
                    .expect("the child's output reads")
                    > 0,
                "the child ended without saying its record"
            );
            if let Some(at) = line.find("record\t") {
                let record = line[at + "record\t".len()..].trim_end().to_owned();
                return (child, record);
            }
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
        use std::io::{Read as _, Write as _};
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
        let (mut child, record) = a_sleeper();
        recorded_as(&installed, &record);
        let done = controlled(&installed, Control::Stop).expect("the child is ended");
        assert!(
            matches!(&done, super::Done::Said(said) if said.contains("asked to end")),
            "{done:?}"
        );
        let status = child.0.wait().expect("reaped");
        assert!(!status.success(), "the child ended on its own: {status:?}");
    }

    /// A daemon that said it is stopping is waited for and never signalled.
    ///
    /// A child of this test held to its record stands in for it, and still
    /// runs once the wait is over.
    #[test]
    fn a_daemon_that_said_it_is_stopping_is_never_signalled() {
        let installed = Scratch::new("said");
        let (mut child, record) = a_sleeper();
        recorded_as(&installed, &record);
        let recorded = super::recorded(&installed.found);
        assert!(matches!(recorded, Recorded::Verified(..)), "{recorded:?}");
        let refused = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(super::ended_once_asked(
                &installed.found,
                recorded,
                child.0.id(),
                core::time::Duration::from_millis(50),
            ))
            .expect_err("the child is still there");
        assert_eq!(refused.code, ErrorCode::CliDaemonDidNotStop, "{refused:?}");
        assert_eq!(refused.exit, Exit::ProcessControl);
        assert!(refused.detail.contains("not signalled"), "{refused:?}");
        assert!(
            child.0.try_wait().expect("asked").is_none(),
            "a daemon that said it is stopping was signalled"
        );
    }

    /// A daemon whose record went while a stop asked it is waited for, and
    /// never signalled.
    ///
    /// It is one ending a stop asked before this one, which removes its record
    /// while its endpoint still answers and lets the endpoint go only after. A
    /// child of this test held to its record stands in for it; what answers at
    /// the endpoint reads the stop, removes the record and goes unanswered, and
    /// a while later lets the child end on its own. The stop, reading the
    /// record again once nothing answered, finds it gone and waits for the
    /// child through what verified it. **Found by a review**: no test went
    /// through that second reading, so a stop that read the record once, and
    /// signalled this daemon, passed every one.
    #[test]
    fn a_daemon_whose_record_went_while_a_stop_asked_it_is_waited_for_and_never_signalled() {
        let installed = Scratch::new("went");
        let token = Token::fresh().expect("a token");
        installed.token(&token);
        let (mut child, record) = a_sleeper();
        recorded_as(&installed, &record);
        let input = child.0.stdin.take().expect("piped");
        let (found, welcome) = (installed.found.clone(), installed.welcome());
        let within = core::time::Duration::from_secs(60);
        let (bound, ready) = std::sync::mpsc::channel();
        let ending = std::thread::spawn(move || {
            let heard = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime")
                .block_on(async {
                    let instance =
                        nc_platform::hold(&found.layout.state, "instance.lock", &found.user)
                            .expect("the lock");
                    let endpoint = nc_platform::ipc::Endpoint::new(
                        &found.layout.runtime,
                        &found.layout.socket,
                        found.layout.pipe.as_str(),
                    );
                    let mut listener =
                        nc_ipc::listen(&endpoint, &found.user, &instance).expect("bound");
                    bound.send(()).expect("the test waits for the bind");
                    let accepted = tokio::time::timeout(within, listener.accept())
                        .await
                        .expect("the stop connects within the wait")
                        .expect("accepted");
                    let mut served = tokio::time::timeout(
                        within,
                        nc_ipc::serve(accepted.stream, &token, &welcome),
                    )
                    .await
                    .expect("a handshake within the wait")
                    .expect("the stop's handshake");
                    let heard = tokio::time::timeout(within, served.next())
                        .await
                        .expect("a request within the wait")
                        .expect("read");
                    // The record goes while the connection is still held, and
                    // the connection goes unanswered.
                    std::fs::remove_file(&found.layout.pid_file).expect("the record is removed");
                    heard
                });
            std::thread::sleep(core::time::Duration::from_millis(200));
            let let_go = std::time::Instant::now();
            drop(input);
            (heard, let_go)
        });
        ready.recv_timeout(within).expect("the endpoint is bound");
        let done = controlled(&installed, Control::Stop).expect("the daemon is waited for");
        let stopped = std::time::Instant::now();
        let (heard, let_go) = ending.join().expect("the stop was read");
        assert_eq!(heard, Some(Request::Stop));
        assert!(
            matches!(&done, super::Done::Said(said) if said.contains("stopped when it was asked")),
            "{done:?}"
        );
        assert!(
            stopped >= let_go,
            "the stop ended before the daemon it asked had"
        );
    }

    /// With nothing verified, a stop waits for the endpoint and the record to let go.
    ///
    /// A daemon that said it is stopping is stopped only once its endpoint
    /// answers no more and its record no longer names it; while either still
    /// holds, the stop does not finish, and nothing is signalled.
    #[test]
    fn with_nothing_verified_a_stop_waits_for_the_endpoint_and_the_record_to_let_go() {
        let installed = Scratch::new("unheld");
        let found = &installed.found;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let ended = |holding: bool| {
            runtime.block_on(async {
                // Something holds the endpoint as a daemon winding down does,
                // every connection accepted and let go.
                let accepting = holding.then(|| {
                    let instance =
                        nc_platform::hold(&found.layout.state, "instance.lock", &found.user)
                            .expect("the lock");
                    let endpoint = nc_platform::ipc::Endpoint::new(
                        &found.layout.runtime,
                        &found.layout.socket,
                        found.layout.pipe.as_str(),
                    );
                    let mut listener =
                        nc_ipc::listen(&endpoint, &found.user, &instance).expect("bound");
                    tokio::spawn(async move {
                        let _instance = instance;
                        loop {
                            let _ = listener.accept().await;
                        }
                    })
                });
                let ended = super::ended_once_asked(
                    found,
                    Recorded::Unverified("another executable".to_owned()),
                    std::process::id(),
                    core::time::Duration::from_millis(50),
                )
                .await;
                if let Some(accepting) = accepting {
                    accepting.abort();
                    let _ = accepting.await;
                }
                ended
            })
        };
        recorded_as(
            &installed,
            &this_process_but(|record| record.started.push('0')),
        );
        let named = ended(false).expect_err("its record still names it");
        std::fs::remove_file(&found.layout.pid_file).expect("removed");
        let held = ended(true).expect_err("its endpoint is still held");
        for refused in [named, held] {
            assert_eq!(refused.code, ErrorCode::CliDaemonDidNotStop, "{refused:?}");
            assert_eq!(refused.exit, Exit::ProcessControl);
            assert!(
                refused
                    .detail
                    .contains("since nothing held it to its record"),
                "{refused:?}"
            );
        }
        let stopped = ended(false).expect("nothing holds it any more");
        assert!(stopped.contains("stopped when it was asked"), "{stopped}");
    }

    /// A start says the daemon it reached runs only once its status says so,
    /// and that it is still starting when the wait ends first.
    #[test]
    fn a_start_says_the_daemon_runs_only_once_its_status_says_so() {
        let installed = Scratch::new("runs");
        let token = Token::fresh().expect("a token");
        installed.token(&token);
        let state = |word: &str| {
            Answer::Status(nc_canon::Value::object([(
                "state",
                nc_canon::Value::text(word),
            )]))
        };
        let daemon = Scripted::serve(
            &installed,
            Script {
                token,
                welcome: installed.welcome(),
                connections: vec![
                    vec![state("starting"), state("running")],
                    vec![state("starting")],
                ],
            },
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let said = |within: core::time::Duration| {
            runtime.block_on(async {
                let connection = crate::client::connect(&installed.found)
                    .await
                    .expect("the daemon answers");
                super::running(connection, "noctua", tokio::time::Instant::now() + within).await
            })
        };
        let running = said(core::time::Duration::from_secs(60));
        assert!(running.contains("is running"), "{running}");
        let starting = said(core::time::Duration::ZERO);
        assert!(starting.contains("still starting"), "{starting}");
        assert_eq!(
            daemon.heard(),
            vec![Request::Status, Request::Status, Request::Status]
        );
    }

    /// A stop something answers otherwise, with no record or a record of a
    /// process gone, signals nothing and says so rather than calling it
    /// stopped.
    #[test]
    fn a_stop_something_answers_otherwise_with_no_record_held_is_refused() {
        let installed = Scratch::new("held");
        let token = Token::fresh().expect("a token");
        installed.token(&token);
        let status = nc_canon::Value::object([("state", nc_canon::Value::text("running"))]);
        let daemon = Scripted::serve(
            &installed,
            Script {
                token,
                welcome: installed.welcome(),
                connections: vec![
                    vec![Answer::Status(status.clone())],
                    vec![Answer::Status(status)],
                ],
            },
        );
        for (record, said) in [
            (None, "no PID record is there"),
            (
                Some(this_process_but(|record| record.started.push('0'))),
                "runs no more",
            ),
        ] {
            if let Some(record) = record {
                recorded_as(&installed, &record);
            }
            let refused = controlled(&installed, Control::Stop).expect_err("something holds it");
            assert_eq!(refused.code, ErrorCode::CliProcessUnverified, "{refused:?}");
            assert_eq!(refused.exit, Exit::ProcessControl);
            assert!(
                refused.detail.contains(said) && refused.detail.contains("nothing was signalled"),
                "{refused:?}"
            );
        }
        assert_eq!(daemon.heard(), vec![Request::Stop, Request::Stop]);
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

    /// A start finding a daemon that says it is stopping waits for it to end.
    ///
    /// Then it starts one: told it runs already, the start would leave nothing
    /// running once the stop it found had ended. The scripted daemon goes once
    /// it has welcomed the start, and its record names this process — though
    /// not as it is, so nothing holds it to one — until a thread removes it a
    /// while after the start began: only then may the start go on, and with no
    /// daemon beside the CLI to start it ends in 11 having asked nothing.
    /// **Found by a review**: with no record to outlive the welcome, a start
    /// that did not wait at all ended the same way, and this test held nothing
    /// of the wait it is named for.
    #[test]
    fn a_start_waits_for_a_daemon_that_is_stopping_and_then_starts_one() {
        let installed = Scratch::new("ending");
        let layout = &installed.found.layout;
        let token = Token::fresh().expect("a token");
        installed.token(&token);
        recorded_as(
            &installed,
            &this_process_but(|record| record.started.push('0')),
        );
        // The stopping daemon's record is removed this long after the start
        // began, and what the start had done by then is read first.
        let ending = std::thread::spawn({
            let (record, started) = (layout.pid_file.clone(), layout.daemon_err.clone());
            move || {
                std::thread::sleep(core::time::Duration::from_millis(400));
                let went_on = started.exists();
                let removed = std::time::Instant::now();
                std::fs::remove_file(&record).expect("the record is removed");
                (went_on, removed)
            }
        });
        let daemon = Scripted::serve(
            &installed,
            Script {
                token,
                welcome: nc_ipc::Welcome {
                    state: nc_ipc::State::Stopping,
                    ..installed.welcome()
                },
                connections: vec![vec![]],
            },
        );
        let refused = controlled(&installed, Control::Start)
            .expect_err("the daemon it found ended, and none is beside it to start");
        let gave_up = std::time::Instant::now();
        let (went_on, removed) = ending.join().expect("the record was removed");
        assert!(
            !went_on,
            "the start went on to start a daemon while the one it found was still stopping"
        );
        assert!(
            gave_up >= removed,
            "the start ended before the daemon it found had"
        );
        assert_eq!(refused.code, ErrorCode::CliDaemonDidNotStart, "{refused:?}");
        assert_eq!(refused.exit, Exit::ProcessControl);
        assert!(
            refused.detail.contains("could not be started"),
            "{refused:?}"
        );
        assert_eq!(daemon.heard(), Vec::<Request>::new());
    }

    /// A stand-in for a process held to its record, which ends on the call
    /// it names, or never, and keeps every call made of it in order.
    struct StandIn {
        /// The call after which it has ended: `terminate`, `kill` or none.
        ends_on: Option<&'static str>,
        /// Every call made of it, in order.
        calls: std::cell::RefCell<Vec<&'static str>>,
    }

    impl super::Ends for StandIn {
        fn pid(&self) -> u32 {
            u32::MAX
        }

        fn terminate(&self) -> std::io::Result<()> {
            self.calls.borrow_mut().push("terminate");
            Ok(())
        }

        fn kill(&self) -> std::io::Result<()> {
            self.calls.borrow_mut().push("kill");
            Ok(())
        }

        fn wait(&self, _within: core::time::Duration) -> std::io::Result<bool> {
            let mut calls = self.calls.borrow_mut();
            let ended = self.ends_on.is_some_and(|call| calls.contains(&call));
            calls.push("wait");
            Ok(ended)
        }
    }

    /// A daemon that does not answer is asked to end, then killed.
    ///
    /// It is killed only when it has not ended by then, and one that ends
    /// neither way is a stop that did not finish.
    #[test]
    fn a_silent_daemon_is_asked_to_end_and_killed_only_when_it_does_not() {
        let all = ["terminate", "wait", "kill", "wait"];
        for (ends_on, calls, said) in [
            (Some("terminate"), &all[..2], Some("it ended")),
            (Some("kill"), &all[..], Some("was killed")),
            (None, &all[..], None),
        ] {
            let process = StandIn {
                ends_on,
                calls: std::cell::RefCell::default(),
            };
            let done = super::signalled("noctua", &process);
            assert_eq!(*process.calls.borrow(), calls, "{ends_on:?}");
            if let Some(said) = said {
                let done = done.expect("it ended");
                assert!(done.contains(said), "{done}");
            } else {
                let refused = done.expect_err("it never ended");
                assert_eq!(refused.code, ErrorCode::CliDaemonDidNotStop, "{refused:?}");
                assert_eq!(refused.exit, Exit::ProcessControl);
                assert!(refused.detail.contains("was killed"), "{refused:?}");
            }
        }
    }

    /// A restart stops the daemon before it starts one, and starts nothing
    /// when its stop did not finish.
    ///
    /// The scripted daemon answers the stop; the start that follows finds
    /// nothing answering and no daemon beside the CLI to start, so it fails,
    /// once it has removed the stop's mark.
    #[test]
    fn a_restart_stops_before_it_starts_and_starts_nothing_after_a_stop_refused() {
        let installed = Scratch::new("restart");
        let stopped = &installed.found.layout.stopped;
        let token = Token::fresh().expect("a token");
        installed.token(&token);
        let daemon = Scripted::serve(
            &installed,
            Script {
                token,
                welcome: installed.welcome(),
                connections: vec![vec![Answer::Stopping]],
            },
        );
        let refused = controlled(&installed, Control::Restart).expect_err("nothing to start");
        assert_eq!(refused.code, ErrorCode::CliDaemonDidNotStart, "{refused:?}");
        assert_eq!(refused.exit, Exit::ProcessControl);
        assert!(!stopped.exists(), "no start followed the stop's mark");
        assert_eq!(daemon.heard(), vec![Request::Stop]);
        // Nothing answers now, and the record does not read: a stop refused.
        recorded_as(&installed, "{\"pid\":0}");
        let refused = controlled(&installed, Control::Restart).expect_err("the stop is refused");
        assert_eq!(refused.code, ErrorCode::CliProcessUnverified, "{refused:?}");
        assert_eq!(refused.exit, Exit::ProcessControl);
        assert!(
            stopped.is_file(),
            "a start followed a stop that did not finish"
        );
    }

    /// A start goes through the manager only where it holds the restorer, and
    /// one that holds none, or does not answer, is a detached start.
    #[test]
    fn a_start_goes_through_the_manager_only_where_it_holds_the_restorer() {
        assert!(super::starts_through(&Ok(true)));
        assert!(!super::starts_through(&Ok(false)));
        for unanswered in [
            nc_platform::service::Error::NoTool("no `systemctl`".to_owned()),
            nc_platform::service::Error::Refused {
                asked: "systemctl --user show".to_owned(),
                said: "Failed to connect to bus".to_owned(),
            },
        ] {
            assert!(
                !super::starts_through(&Err(unanswered)),
                "a manager that did not answer was asked to start the daemon"
            );
        }
    }

    /// A start gives up at once on a daemon that ended before it answered,
    /// and on one that is silent only once the deadline has passed.
    #[test]
    fn a_start_gives_up_at_once_on_a_daemon_that_ended() {
        let ended = super::given_up(Some("exit status: 1".to_owned()), false);
        assert_eq!(
            ended.as_deref(),
            Some("ended (exit status: 1) before it answered")
        );
        assert_eq!(super::given_up(None, false), None);
        let silent = super::given_up(None, true).expect("given up at the deadline");
        assert!(silent.starts_with("did not answer within"), "{silent}");
    }

    /// A failed start shows the last lines its daemon wrote, on one line,
    /// and says so when it wrote nothing.
    #[test]
    fn a_failed_start_shows_the_last_lines_its_daemon_wrote() {
        use std::io::Write as _;
        let installed = Scratch::new("tail");
        let path = installed.path("daemon.err");
        assert_eq!(super::tail_of(&path), "nothing");
        let lines: Vec<String> = (1..=25).map(|line| format!("line {line}")).collect();
        nc_platform::create_private(&path)
            .expect("a standard error")
            .write_all(format!("{}\n", lines.join("\n")).as_bytes())
            .expect("written");
        assert_eq!(super::tail_of(&path), lines[5..].join(" | "));
    }
}
