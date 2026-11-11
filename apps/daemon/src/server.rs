//! What the daemon serves at its endpoint (`NOCTUA.md §11.1`): each
//! connection accepted, its other end held to this user, its handshake proven,
//! and its requests answered one at a time.
//!
//! # In order, for each connection
//!
//! 1. **Accepted** (`nc_platform::ipc::Listener::accept`): a connection whose
//!    other end the system does not show to be this user's, or one past
//!    `ipc_connections_max`, is closed as it is accepted.
//! 2. **Proven** (`nc_ipc::serve`): a client that cannot prove it holds the
//!    channel's token is turned away before a request is read. Each refusal
//!    of either step is logged as `ipc.refused` with its code, and nothing of
//!    the connection reaches the kernel.
//! 3. **Answered**: a dispatch is the kernel's ([`nc_kernel::Kernel::dispatch`])
//!    as the person the profile's world names, and every decision is the
//!    kernel's — this asks nothing on its own. A dispatch or the registry asked
//!    before the kernel runs is refused as not running yet (`NC-DMN-0009`), and
//!    one asked once a stop began as stopping (`NC-DMN-0010`); the status and a
//!    stop are answered from the first moment. A dispatch is counted as
//!    running before the state is read, so a stop either refuses it or waits
//!    for it. A connection that then breaks the protocol, falls silent, takes
//!    nothing or is lost is closed and logged as `ipc.refused` with its code;
//!    one its client closes between requests ends well.
//! 4. **Closed once its last answer is written**: a stop asked for closes
//!    each connection between two requests, its own after the answer to it,
//!    and the daemon ends only once every connection has written its last
//!    answer and closed, `stop_drain_ms` at most ([`Server::wound_down`]).
//!
//! # What each answer holds
//!
//! - **A command of the registry** is one object: `name`, `title` — the
//!   catalog's message key for a command of the kernel's own, the plugin's
//!   label for one of a plugin's — `args_schema` as the text it was declared
//!   in, `mutating`, `action`, `resource_selector`, `reversibility` (`read`,
//!   `write` or `irreversible`) and `owner` (`kernel` or `plugin`).
//! - **The status** is one object: `channel`, `checkout` for a development
//!   install, `version`, `pid`, `instance`, `state`, `since`, `realm`,
//!   `person`, `plugins` — each with its `name`, whether it `loaded`, and the
//!   `code` and `sentence` of what refused one that did not — `schedule`, and
//!   `clock` while a start waits for its clock to pass the mark.
//! - **An answer too long for a frame** is not sent; the request is answered
//!   instead with its refusal (`NC-IPC-0016`), and the connection goes on.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use core::time::Duration;

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_ids::Clock as _;
use nc_ipc::{Answer, Refusal, RefusalKind, Request, State, Welcome};
use nc_kernel::logging::LogSink;
use nc_kernel::registry::{Owner, RegisteredCommand};
use nc_kernel::{Caller, Kernel};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::environment::{Daemon, Sink};
use crate::plugins::Loaded;
use crate::schedule::Done;

/// What the daemon is, as each welcome and its status say it.
#[derive(Debug)]
pub struct Named {
    /// The daemon's process.
    pub pid: u32,
    /// The instance it drew at this start.
    pub instance: [u8; 16],
    /// The channel it serves.
    pub channel: nc_channel::Channel,
    /// The realm the profile decides in.
    pub realm: String,
    /// The person every dispatch is decided as.
    pub person: nc_authz::Principal,
    /// When it started, in milliseconds since the Unix epoch.
    pub since: u64,
}

/// What every connection of one daemon shares.
#[derive(Debug)]
pub struct Server {
    named: Named,
    token: nc_ipc::Token,
    state: tokio::sync::watch::Sender<State>,
    kernel: OnceLock<Arc<Kernel<Daemon>>>,
    plugins: Mutex<Vec<Loaded>>,
    /// How far the clock is behind the mark, while a start waits for it.
    behind: Mutex<Option<Duration>>,
    schedule: Arc<Mutex<Done>>,
    running: AtomicUsize,
    finished: tokio::sync::Notify,
    /// How many connections are open: each counted before it is served, and
    /// until it is closed.
    connections: AtomicUsize,
    all_closed: tokio::sync::Notify,
    log: Sink,
}

impl Server {
    /// A server for the daemon `named`, proving connections by `token` and
    /// logging to `log`, in state `starting`.
    #[must_use]
    pub fn new(named: Named, token: nc_ipc::Token, log: Sink) -> Self {
        Self {
            named,
            token,
            state: tokio::sync::watch::Sender::new(State::Starting),
            kernel: OnceLock::new(),
            plugins: Mutex::new(Vec::new()),
            behind: Mutex::new(None),
            schedule: Arc::default(),
            running: AtomicUsize::new(0),
            finished: tokio::sync::Notify::new(),
            connections: AtomicUsize::new(0),
            all_closed: tokio::sync::Notify::new(),
            log,
        }
    }

    /// Where the daemon's start is.
    #[must_use]
    pub fn state(&self) -> State {
        *self.state.borrow()
    }

    /// Moves the start on to `state` — never out of `stopping`, which a stop
    /// asked for has the last word on.
    pub fn reach(&self, state: State) {
        self.state.send_if_modified(|now| {
            let moved = *now != State::Stopping && *now != state;
            if moved {
                *now = state;
            }
            moved
        });
    }

    /// Asks the daemon to stop: new dispatches are refused from now on.
    pub fn stop(&self) {
        self.state.send_replace(State::Stopping);
    }

    /// Waits until a stop is asked for.
    pub async fn stopping(&self) {
        let mut watching = self.state.subscribe();
        let _ = watching.wait_for(|state| *state == State::Stopping).await;
    }

    /// The clock is `behind` the mark, and the start waits for it.
    pub fn waiting_for_the_clock(&self, behind: Duration) {
        *self.behind.lock().unwrap_or_else(PoisonError::into_inner) = Some(behind);
        self.reach(State::WaitingForClock);
    }

    /// The kernel is open, and serves from now on.
    pub fn serving(&self, kernel: Arc<Kernel<Daemon>>, plugins: Vec<Loaded>) {
        *self.behind.lock().unwrap_or_else(PoisonError::into_inner) = None;
        *self.plugins.lock().unwrap_or_else(PoisonError::into_inner) = plugins;
        let _ = self.kernel.set(kernel);
        self.reach(State::Running);
    }

    /// The central log the daemon's events go to.
    #[must_use]
    pub fn log(&self) -> Sink {
        Arc::clone(&self.log)
    }

    /// What the schedule notes what it did in.
    #[must_use]
    pub fn schedule(&self) -> Arc<Mutex<Done>> {
        Arc::clone(&self.schedule)
    }

    /// Accepts connections on `listener` until a stop is asked for, each served
    /// on a task of its own, and none once it was.
    pub async fn accepting(self: Arc<Self>, mut listener: nc_platform::ipc::Listener) {
        loop {
            let accepted = tokio::select! {
                biased;
                () = self.stopping() => return,
                accepted = listener.accept() => accepted,
            };
            match accepted {
                Ok(accepted) => self.serve_one(accepted.stream),
                Err(refused) => {
                    let code = nc_ipc::IpcError::from(refused).code();
                    self.log.record(&nc_log::events::IpcRefused { code });
                    // A system that does not answer an accept is given a
                    // moment before it is asked again, rather than a loop.
                    if code == ErrorCode::IpcEndpointUnavailable {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            }
        }
    }

    /// Serves `stream` on a task of its own, counted as open until it is
    /// closed.
    fn serve_one<S>(self: &Arc<Self>, stream: S)
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        self.serve_one_after(stream, || core::future::ready(()));
    }

    /// What [`Server::serve_one`] does, with `answering` awaited before each
    /// answer is handed to the writer: where an answer slow to work out — a
    /// dispatch's — would be.
    ///
    /// **Counted before its task is spawned**, so a stop that has stopped
    /// accepting counts every connection there is.
    fn serve_one_after<S, Slow>(
        self: &Arc<Self>,
        stream: S,
        answering: impl Fn() -> Slow + Send + 'static,
    ) where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
        Slow: Future<Output = ()> + Send + 'static,
    {
        self.connections.fetch_add(1, Ordering::SeqCst);
        let open = Open(Arc::clone(self));
        tokio::spawn(Arc::clone(self).connection(stream, open, answering));
    }

    /// One connection: its handshake, then each request in turn until the
    /// client closes it, it fails, or a stop asked for finds it between two
    /// requests; then closed once its last answer is written.
    async fn connection<S, Slow>(
        self: Arc<Self>,
        stream: S,
        open: Open,
        answering: impl Fn() -> Slow,
    ) where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
        Slow: Future<Output = ()>,
    {
        let welcome = Welcome {
            pid: self.named.pid,
            instance: self.named.instance,
            channel: self.named.channel,
            state: self.state(),
        };
        let mut served = match nc_ipc::serve(stream, &self.token, &welcome).await {
            Ok(served) => served,
            Err(refused) => {
                self.log.record(&nc_log::events::IpcRefused {
                    code: refused.code(),
                });
                return;
            }
        };
        let ended = loop {
            // A request that already arrived is answered first; between two
            // requests, a stop asked for ends the connection.
            let next = tokio::select! {
                biased;
                next = served.next() => next,
                () = self.stopping() => break None,
            };
            let request = match next {
                Ok(Some(request)) => request,
                Ok(None) => break None,
                Err(broken) => break Some(broken),
            };
            let answer = self.answer(request).await;
            let slow = answering();
            slow.await;
            let sent = match served.answer(&answer).await {
                Err(unsendable @ nc_ipc::IpcError::Unsendable { .. }) => {
                    served
                        .answer(&Answer::Refused(Refusal {
                            code: unsendable.code(),
                            kind: RefusalKind::Internal,
                            sentence: unsendable.to_string(),
                        }))
                        .await
                }
                sent => sent,
            };
            if let Err(lost) = sent {
                break Some(lost);
            }
        };
        // A client that closed the connection between requests ended it
        // well; one that broke the protocol, fell silent, took nothing or was
        // lost is closed by the daemon and logged as a refusal of its own.
        if let Some(why) = ended {
            self.log
                .record(&nc_log::events::IpcRefused { code: why.code() });
        }
        // Its last answer written before it is let go, which a stop waits
        // for: dropped instead, an answer still on its way would go with the
        // runtime once the run is over.
        served.close().await;
        drop(open);
    }

    /// The answer to `request`.
    async fn answer(&self, request: Request) -> Answer {
        match request {
            Request::Status => Answer::Status(self.status()),
            Request::Stop => {
                self.stop();
                Answer::Stopping
            }
            Request::Registry => match self.kernel_if_running() {
                Ok(kernel) => Answer::Registry(kernel.registry().iter().map(listed).collect()),
                Err(refusal) => Answer::Refused(refusal),
            },
            Request::Dispatch { command, args } => match self.admitted() {
                Ok((kernel, admitted)) => {
                    let answer = self.dispatched(&kernel, &command, &args).await;
                    drop(admitted);
                    answer
                }
                Err(refusal) => Answer::Refused(refusal),
            },
        }
    }

    /// The kernel, for a dispatch admitted while it runs, and the dispatch
    /// counted as running until what this answers is dropped.
    fn admitted(&self) -> Result<(Arc<Kernel<Daemon>>, Admitted<'_>), Refusal> {
        self.admitted_after(|| {})
    }

    /// What [`Server::admitted`] answers, with `meanwhile` run between the
    /// count and the reading of the state: where a stop asked on another
    /// thread may land.
    ///
    /// **Counted before the state is read**, so a stop either is seen here
    /// and the dispatch refused, or sees the dispatch running and waits for
    /// it: the state's lock orders the stop's write before the read here, or
    /// the count here before the drain's read.
    fn admitted_after(
        &self,
        meanwhile: impl FnOnce(),
    ) -> Result<(Arc<Kernel<Daemon>>, Admitted<'_>), Refusal> {
        self.running.fetch_add(1, Ordering::SeqCst);
        let admitted = Admitted(self);
        meanwhile();
        let kernel = self.kernel_if_running()?;
        Ok((kernel, admitted))
    }

    /// The kernel, while it runs: refused as not running yet before it does,
    /// and as stopping once a stop was asked for.
    fn kernel_if_running(&self) -> Result<Arc<Kernel<Daemon>>, Refusal> {
        match (self.state(), self.kernel.get()) {
            (State::Running, Some(kernel)) => Ok(Arc::clone(kernel)),
            (State::Stopping, _) => Err(Refusal {
                code: ErrorCode::DaemonStopping,
                kind: RefusalKind::Unavailable,
                sentence: "the daemon is stopping, and takes no new dispatch".to_owned(),
            }),
            (state, _) => Err(Refusal {
                code: ErrorCode::DaemonNotRunning,
                kind: RefusalKind::Unavailable,
                sentence: format!(
                    "the daemon is {} and does not run its kernel yet; ask again once it runs",
                    state.word()
                ),
            }),
        }
    }

    /// `command` with `args`, dispatched by the kernel as the profile's
    /// person.
    async fn dispatched(&self, kernel: &Kernel<Daemon>, command: &str, args: &Value) -> Answer {
        let caller = Caller::new(self.named.person.clone());
        let answered = kernel
            .dispatch(&caller, command, &args.to_canonical())
            .await;
        match answered {
            Ok(payload) => nc_canon::parse(&payload).map_or_else(
                |error| {
                    Answer::Refused(Refusal {
                        code: ErrorCode::IpcMalformed,
                        kind: RefusalKind::Internal,
                        sentence: format!("the kernel answered what is not one value: {error}"),
                    })
                },
                Answer::Result,
            ),
            Err(refusal) => Answer::Refused(Refusal {
                code: refusal.code(),
                kind: kind_of(refusal.kind()),
                sentence: refusal.sentence().to_owned(),
            }),
        }
    }

    /// Waits for the dispatches running to end, until `deadline` at most,
    /// and answers how many were still running when it gave up.
    pub async fn drained(&self, deadline: tokio::time::Instant) -> u32 {
        Self::none_left(&self.running, &self.finished, deadline).await
    }

    /// Waits for every connection to close, until `deadline` at most, and
    /// answers how many were still open when it gave up.
    pub async fn closed(&self, deadline: tokio::time::Instant) -> u32 {
        Self::none_left(&self.connections, &self.all_closed, deadline).await
    }

    /// Waits until `counted` falls to none, woken by `changed` when it does,
    /// until `deadline` at most, and answers how many were still counted
    /// when it gave up.
    async fn none_left(
        counted: &AtomicUsize,
        changed: &tokio::sync::Notify,
        deadline: tokio::time::Instant,
    ) -> u32 {
        loop {
            let finished = changed.notified();
            let running = counted.load(Ordering::SeqCst);
            if running == 0 {
                return 0;
            }
            if tokio::time::timeout_at(deadline, finished).await.is_err() {
                let left = counted.load(Ordering::SeqCst);
                return u32::try_from(left).unwrap_or(u32::MAX);
            }
        }
    }

    /// Once a stop was asked for: takes no connection more, and waits for the
    /// dispatches running, for the schedule's tasks in `schedule` — each
    /// inside a run or between two — and for every connection to write its
    /// last answer and close, until `deadline` at most; answers how many
    /// dispatches and runs it gave up on.
    ///
    /// **Every connection there is**: `accepting` has ended before the count
    /// is read, so none is taken after it. One still open at the bound has a
    /// client that takes nothing, or a dispatch the drain counts already, and
    /// goes with the runtime.
    pub async fn wound_down(
        &self,
        accepting: tokio::task::JoinHandle<()>,
        schedule: Vec<tokio::task::JoinHandle<()>>,
        deadline: tokio::time::Instant,
    ) -> u32 {
        accepting.abort();
        let _ = accepting.await;
        let runs = async {
            let mut given_up = 0_u32;
            for running in schedule {
                given_up = given_up.saturating_add(crate::schedule::ended(running, deadline).await);
            }
            given_up
        };
        let (dispatches, runs, _still_open) =
            tokio::join!(self.drained(deadline), runs, self.closed(deadline));
        dispatches.saturating_add(runs)
    }

    /// The daemon's status, as one object.
    #[must_use]
    pub fn status(&self) -> Value {
        let named = &self.named;
        let mut fields = vec![
            ("channel", Value::text(named.channel.word())),
            ("version", Value::text(env!("CARGO_PKG_VERSION"))),
            ("pid", Value::Integer(i64::from(named.pid))),
            ("instance", Value::text(nc_canon::to_hex(&named.instance))),
            ("state", Value::text(self.state().word())),
            ("since", number(named.since)),
            ("realm", Value::text(named.realm.as_str())),
            ("person", Value::text(named.person.id().as_str())),
            (
                "plugins",
                Value::Array(
                    self.plugins
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .iter()
                        .map(plugin)
                        .collect(),
                ),
            ),
            (
                "schedule",
                schedule(&self.schedule.lock().unwrap_or_else(PoisonError::into_inner)),
            ),
        ];
        if let nc_channel::Channel::Development(checkout) = named.channel {
            fields.push(("checkout", Value::text(checkout.to_string())));
        }
        if let Some(behind) = *self.behind.lock().unwrap_or_else(PoisonError::into_inner) {
            fields.push((
                "clock",
                Value::object([(
                    "behind_ms",
                    number(u64::try_from(behind.as_millis()).unwrap_or(u64::MAX)),
                )]),
            ));
        }
        Value::object(fields)
    }
}

/// A dispatch admitted, counted as running until it is dropped.
#[derive(Debug)]
struct Admitted<'a>(&'a Server);

impl Drop for Admitted<'_> {
    fn drop(&mut self) {
        if self.0.running.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.finished.notify_waiters();
        }
    }
}

/// A connection served, counted as open until it is dropped.
#[derive(Debug)]
struct Open(Arc<Server>);

impl Drop for Open {
    fn drop(&mut self) {
        if self.0.connections.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.all_closed.notify_waiters();
        }
    }
}

/// A number the status holds, as canonical JSON holds one.
fn number(value: u64) -> Value {
    Value::Integer(i64::try_from(value).unwrap_or(i64::MAX))
}

/// One plugin's load, as the status shows it.
fn plugin(loaded: &Loaded) -> Value {
    let mut fields = vec![
        ("name", Value::text(loaded.name.as_str())),
        ("loaded", Value::Bool(loaded.refused.is_none())),
    ];
    if let Some((code, sentence)) = &loaded.refused {
        fields.push(("code", Value::text(code.code())));
        fields.push(("sentence", Value::text(sentence.as_str())));
    }
    Value::object(fields)
}

/// What the schedule has done, as the status shows it.
fn schedule(done: &Done) -> Value {
    let mut fields = vec![
        ("pumps", number(done.pumps)),
        ("sweeps", number(done.sweeps)),
        ("pump_every_ms", number(nc_catalog::DAEMON.pump_every_ms)),
        ("sweep_every_ms", number(nc_catalog::DAEMON.sweep_every_ms)),
    ];
    if let Some(at) = done.last_pump {
        fields.push(("last_pump", number(at)));
    }
    if let Some(at) = done.last_sweep {
        fields.push(("last_sweep", number(at)));
    }
    if let Some(code) = done.prune.code {
        fields.push((
            "prune_refused",
            Value::object([
                ("code", Value::text(code.code())),
                ("pumps", number(u64::from(done.prune.pumps))),
            ]),
        ));
    }
    if let Some(code) = done.sweep_refused {
        fields.push(("sweep_refused", Value::text(code.code())));
    }
    Value::object(fields)
}

/// A command of the registry, as one object.
fn listed(command: &RegisteredCommand) -> Value {
    use nc_schema::descriptor::Reversibility;
    Value::object([
        ("name", Value::text(command.name.as_str())),
        ("title", Value::text(command.title.as_str())),
        ("args_schema", Value::text(command.args_schema.as_str())),
        ("mutating", Value::Bool(command.mutating)),
        ("action", Value::text(command.action.as_str())),
        (
            "resource_selector",
            Value::text(command.resource_selector.as_str()),
        ),
        (
            "reversibility",
            Value::text(match command.reversibility {
                Reversibility::Read => "read",
                Reversibility::Write => "write",
                Reversibility::Irreversible => "irreversible",
            }),
        ),
        (
            "owner",
            Value::text(match command.owner {
                Owner::Kernel => "kernel",
                Owner::Plugin(_) => "plugin",
            }),
        ),
    ])
}

/// The kind the protocol tells a kernel's refusal by: the kernel's own word.
const fn kind_of(kind: nc_kernel::RefusalKind) -> RefusalKind {
    match kind {
        nc_kernel::RefusalKind::Denied => RefusalKind::Denied,
        nc_kernel::RefusalKind::NotFound => RefusalKind::NotFound,
        nc_kernel::RefusalKind::Invalid => RefusalKind::Invalid,
        nc_kernel::RefusalKind::Conflict => RefusalKind::Conflict,
        nc_kernel::RefusalKind::Unavailable => RefusalKind::Unavailable,
        nc_kernel::RefusalKind::Internal => RefusalKind::Internal,
    }
}

/// When it is now, by the clock every unit is timed by.
#[must_use]
pub fn now_ms() -> u64 {
    nc_ids::SystemClock.now_ms()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use core::time::Duration;

    use nc_canon::Value;
    use nc_catalog::ErrorCode;
    use nc_ipc::{Answer, Refusal, RefusalKind, Request, State};

    use super::{Named, Server};

    /// A world file as an installer writes one, read: the one every suite's
    /// install copies in.
    fn a_world() -> nc_authz::LocalWorld {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        nc_authz::LocalWorld::parse(&crate::testing::a_world(), &pdp).expect("the world reads")
    }

    /// A server for a daemon of `channel` that never bound an endpoint, its
    /// lines sent to a store nothing answers at.
    fn a_server(channel: nc_channel::Channel) -> Server {
        let world = a_world();
        let named = Named {
            pid: 4242,
            instance: [9; 16],
            channel,
            realm: world.realm().as_str().to_owned(),
            person: world.person().clone(),
            since: 1_767_225_600_000,
        };
        let log = nc_log::Log::open(
            super::super::environment::origin(),
            "http://127.0.0.1:0/insert",
        )
        .expect("a store on this machine");
        Server::new(
            named,
            nc_ipc::Token::fresh().expect("a token"),
            Arc::new(std::sync::Mutex::new(log)),
        )
    }

    /// The refusal `request` is answered with.
    async fn refused(server: &Server, request: Request) -> Refusal {
        match server.answer(request).await {
            Answer::Refused(refusal) => refusal,
            other => panic!("answered {other:?}"),
        }
    }

    /// A kernel's refusal is told by the kernel's own word, for every kind.
    #[test]
    fn a_kernels_refusal_is_told_by_the_kernels_own_word() {
        use nc_kernel::RefusalKind as Kernels;
        for kind in [
            Kernels::Denied,
            Kernels::NotFound,
            Kernels::Invalid,
            Kernels::Conflict,
            Kernels::Unavailable,
            Kernels::Internal,
        ] {
            assert_eq!(super::kind_of(kind).word(), kind.word());
        }
    }

    /// A command is listed with every field the registry holds, its
    /// reversibility and its owner by their words.
    #[test]
    fn a_command_is_listed_with_every_field_the_registry_holds() {
        use nc_schema::descriptor::Reversibility;
        let command = nc_kernel::registry::RegisteredCommand {
            name: "journal.verify".to_owned(),
            title: "command.journal.verify.title".to_owned(),
            args_schema: "{\"type\":\"object\"}".to_owned(),
            mutating: false,
            action: "Read".to_owned(),
            resource_selector: "realm".to_owned(),
            reversibility: Reversibility::Irreversible,
            owner: nc_kernel::registry::Owner::Kernel,
        };
        assert_eq!(
            super::listed(&command).to_canonical(),
            "{\"action\":\"Read\",\"args_schema\":\"{\\\"type\\\":\\\"object\\\"}\",\
             \"mutating\":false,\"name\":\"journal.verify\",\"owner\":\"kernel\",\
             \"resource_selector\":\"realm\",\"reversibility\":\"irreversible\",\
             \"title\":\"command.journal.verify.title\"}"
        );
        for (reversibility, word) in [
            (Reversibility::Read, "read"),
            (Reversibility::Write, "write"),
        ] {
            let listed = super::listed(&nc_kernel::registry::RegisteredCommand {
                reversibility,
                mutating: true,
                ..command.clone()
            })
            .to_canonical();
            assert!(
                listed.contains(&format!("\"reversibility\":\"{word}\"")),
                "{listed}"
            );
            assert!(listed.contains("\"mutating\":true"), "{listed}");
        }
    }

    /// Nothing is dispatched, and no registry is read, before the kernel runs
    /// or once a stop began; the status and a stop are answered all along.
    #[tokio::test]
    async fn nothing_is_dispatched_before_the_kernel_runs_or_once_a_stop_began() {
        let server = a_server(nc_channel::Channel::Staging);
        let dispatch = || Request::Dispatch {
            command: "journal.verify".to_owned(),
            args: Value::object([("from", Value::Integer(0))]),
        };
        for state in [State::Starting, State::WaitingForClock, State::Recovering] {
            server.reach(state);
            for request in [dispatch(), Request::Registry] {
                let refusal = refused(&server, request).await;
                assert_eq!(refusal.code, ErrorCode::DaemonNotRunning, "{state:?}");
                assert_eq!(refusal.kind, RefusalKind::Unavailable);
                assert!(refusal.sentence.contains(state.word()), "{refusal:?}");
            }
            assert!(matches!(
                server.answer(Request::Status).await,
                Answer::Status(_)
            ));
        }
        assert_eq!(server.answer(Request::Stop).await, Answer::Stopping);
        assert_eq!(server.state(), State::Stopping);
        for request in [dispatch(), Request::Registry] {
            let refusal = refused(&server, request).await;
            assert_eq!(refusal.code, ErrorCode::DaemonStopping);
            assert_eq!(refusal.kind, RefusalKind::Unavailable);
        }
    }

    /// A stop is the last word on the state: nothing a start reaches after it
    /// moves it back.
    #[tokio::test]
    async fn a_stop_is_the_last_word_on_the_state() {
        let server = a_server(nc_channel::Channel::Staging);
        server.reach(State::Recovering);
        assert_eq!(server.state(), State::Recovering);
        server.stop();
        for state in [State::Running, State::WaitingForClock, State::Starting] {
            server.reach(state);
            assert_eq!(server.state(), State::Stopping);
        }
        tokio::time::timeout(Duration::from_secs(5), server.stopping())
            .await
            .expect("a stop asked for is seen");
    }

    /// The status says what the daemon is: its channel and checkout, its
    /// process and instance, its state, realm and person, its plugins and its
    /// schedule, and how far the clock is behind while it waits.
    #[test]
    fn the_status_says_what_the_daemon_is() {
        let checkout = nc_channel::Checkout::parse("0123abcd").expect("a checkout");
        let server = a_server(nc_channel::Channel::Development(checkout));
        server.waiting_for_the_clock(Duration::from_millis(1_500));
        let status = server.status().to_canonical();
        for said in [
            "\"channel\":\"development\"",
            "\"checkout\":\"0123abcd\"",
            "\"pid\":4242",
            &format!("\"instance\":\"{}\"", nc_canon::to_hex(&[9; 16])),
            "\"state\":\"waiting-for-clock\"",
            "\"realm\":\"home\"",
            "\"person\":\"ana\"",
            "\"clock\":{\"behind_ms\":1500}",
            "\"since\":1767225600000",
            "\"plugins\":[]",
            &format!("\"version\":\"{}\"", env!("CARGO_PKG_VERSION")),
        ] {
            assert!(status.contains(said), "{said}: {status}");
        }
        crate::schedule::pumped(
            &server.schedule,
            10,
            Some(ErrorCode::DbPruneFailed),
            &server.log,
        );
        crate::schedule::swept(&server.schedule, 20, Some(ErrorCode::BlobIo));
        *server.plugins.lock().expect("held") = vec![
            crate::plugins::Loaded {
                name: "reference".to_owned(),
                refused: None,
            },
            crate::plugins::Loaded {
                name: "broken".to_owned(),
                refused: Some((ErrorCode::PluginManifestInvalid, "it is not one".to_owned())),
            },
        ];
        let status = server.status().to_canonical();
        for said in [
            "\"name\":\"reference\"",
            "\"loaded\":true",
            &format!(
                "\"code\":\"{}\",\"loaded\":false,\"name\":\"broken\",\"sentence\":\"it is not one\"",
                ErrorCode::PluginManifestInvalid.code()
            ),
            "\"last_pump\":10",
            "\"last_sweep\":20",
            "\"pumps\":1",
            "\"sweeps\":1",
            &format!(
                "\"prune_refused\":{{\"code\":\"{}\",\"pumps\":1}}",
                ErrorCode::DbPruneFailed.code()
            ),
            &format!("\"sweep_refused\":\"{}\"", ErrorCode::BlobIo.code()),
        ] {
            assert!(status.contains(said), "{said}: {status}");
        }
        let staging = a_server(nc_channel::Channel::Staging)
            .status()
            .to_canonical();
        assert!(!staging.contains("checkout"), "{staging}");
        assert!(!staging.contains("clock"), "{staging}");
    }

    /// A drain waits for the dispatches running to end, and gives up on the
    /// ones still running at its bound, saying how many.
    #[tokio::test]
    async fn a_drain_waits_for_what_runs_and_gives_up_at_its_bound() {
        let after = |bound: Duration| tokio::time::Instant::now() + bound;
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        assert_eq!(server.drained(after(Duration::from_secs(5))).await, 0);
        server.running.fetch_add(2, Ordering::SeqCst);
        assert_eq!(server.drained(after(Duration::from_millis(50))).await, 2);
        let ending = Arc::clone(&server);
        let ended = tokio::spawn(async move {
            for _ in 0..2 {
                tokio::time::sleep(Duration::from_millis(20)).await;
                if ending.running.fetch_sub(1, Ordering::SeqCst) == 1 {
                    ending.finished.notify_waiters();
                }
            }
        });
        assert_eq!(
            server.drained(after(Duration::from_secs(30))).await,
            0,
            "a drain gave up on dispatches that ended inside its bound"
        );
        ended.await.expect("the dispatches ended");
    }

    /// A dispatch is counted as running before the state is read, so a stop
    /// that lands between the two refuses it, and a drain at that moment
    /// would have waited for it; a dispatch refused, or answered, is no longer
    /// counted.
    ///
    /// **Found by a review**: counted only once the state said running, a
    /// dispatch admitted on one thread while a stop drained on another was
    /// seen by neither, and the stop closed the store under it.
    #[test]
    fn a_dispatch_is_counted_before_its_state_is_read() {
        let server = a_server(nc_channel::Channel::Staging);
        server.reach(State::Running);
        let mut seen = None;
        let refused = server
            .admitted_after(|| {
                server.stop();
                seen = Some(server.running.load(Ordering::SeqCst));
            })
            .expect_err("a stop landed before the state was read");
        assert_eq!(
            seen,
            Some(1),
            "a stop landing there saw no dispatch to wait for"
        );
        assert_eq!(refused.code, ErrorCode::DaemonStopping);
        assert_eq!(
            server.running.load(Ordering::SeqCst),
            0,
            "a dispatch refused is still counted as running"
        );
        // Running with no kernel open: refused as not running yet, and not
        // counted either.
        let server = a_server(nc_channel::Channel::Staging);
        server.reach(State::Running);
        let refused = server
            .admitted()
            .expect_err("no kernel is open to admit it to");
        assert_eq!(refused.code, ErrorCode::DaemonNotRunning);
        assert_eq!(server.running.load(Ordering::SeqCst), 0);
    }

    /// A stop's answer is written whole before the connection it was asked
    /// on is let go, and the wind-down waits for that and no longer: however
    /// long the answer takes to work out, and however slowly its client takes
    /// it.
    ///
    /// **Found by a review**: the stop's answer was handed to the
    /// connection's writer, the run ended, and the daemon's runtime dropped the
    /// writer with the answer unwritten, so the client read the daemon as
    /// lost. Here the server runs on a runtime of its own, dropped as soon as
    /// the wind-down returns, as the daemon's is; its client is on another
    /// thread, takes one byte at a time, and holds its connection open until
    /// that runtime is gone, so only the daemon closes it.
    #[test]
    fn a_stop_is_answered_whole_before_its_connection_is_let_go() {
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        let (daemon_end, client_end) = tokio::io::duplex(1);
        let token =
            nc_ipc::Token::from_text(&server.token.to_text()).expect("the token reads back");
        let bound = Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms);
        let runtime = || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime")
        };
        let (let_go, held) = tokio::sync::oneshot::channel::<()>();
        let asking = std::thread::spawn(move || {
            runtime().block_on(async {
                let (mut client, _) =
                    tokio::time::timeout(bound, nc_ipc::connect(client_end, &token))
                        .await
                        .expect("the handshake ended in time")
                        .expect("connected");
                let answered = tokio::time::timeout(bound, client.ask(&Request::Stop))
                    .await
                    .expect("the stop ended in time");
                // Held open, and answering the daemon's pings, until the
                // server's runtime is gone.
                let _ = tokio::time::timeout(bound * 2, held).await;
                drop(client);
                answered
            })
        });
        let serving = runtime();
        let began = std::time::Instant::now();
        let abandoned = serving.block_on(async {
            server.serve_one_after(daemon_end, || {
                tokio::time::sleep(Duration::from_millis(200))
            });
            server.stopping().await;
            let accepting = tokio::spawn(std::future::pending::<()>());
            server
                .wound_down(accepting, Vec::new(), tokio::time::Instant::now() + bound)
                .await
        });
        // What the daemon's runtime does to every task it still holds once
        // its run is over.
        drop(serving);
        let took = began.elapsed();
        let _ = let_go.send(());
        let answered = asking.join().expect("the client did not panic");
        assert_eq!(
            answered.expect("the stop was answered before its connection was let go"),
            Answer::Stopping
        );
        assert_eq!(abandoned, 0, "nothing ran for the stop to give up on");
        assert!(
            took < bound,
            "the stop waited out its bound for a connection with nothing more to say"
        );
    }
}
