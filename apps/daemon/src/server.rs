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
//!    as the connection's principal — the person the profile's world names,
//!    or the agent of the session the connection opened — and every decision
//!    is the kernel's: this asks nothing on its own. A dispatch, the registry,
//!    the tools, a session, a watch or a note asked before the kernel runs is
//!    refused as not running yet (`NC-DMN-0009`), and one asked once a stop
//!    began as stopping (`NC-DMN-0010`), as is an answer to a held call; the
//!    status, a stop, a display and an attach are answered from the first
//!    moment. A dispatch is counted as running before the state is read, so a
//!    stop either refuses it or waits for it, a session's as the person's. A
//!    connection that then breaks the protocol, falls silent, takes nothing or
//!    is lost is closed and logged as `ipc.refused` with its code; one its
//!    client closes between requests ends well.
//! 4. **Closed once its last answer is written**: a stop asked for closes
//!    each connection between two requests, its own after the answer to it,
//!    and the daemon ends only once every connection has written its last
//!    answer and closed, `stop_drain_ms` at most ([`Server::wound_down`]).
//!    **The endpoint answers until the drain is over** ([`Server::let_go`]),
//!    and every connection is answered the first request it makes: one made
//!    as a stop began or while it drains — a stop asked again, a status — is
//!    told the daemon is stopping and closed after it, so a stop asked again
//!    finds the daemon answering and waits for it rather than signalling a
//!    process nothing answers for.
//!
//! # What a person's connection watches, notes and announces
//!
//! **The daemon holds one watched state, at a revision** (`Watched`): the
//! graphical environment a person's connection announced last, the
//! interface's state as the kernel holds it (`NOCTUA.md §12.1`), and what
//! came of the daemon's asking the system for the Leader where it is the
//! process that asks (`crate::leader`), as one object — `display`, the
//! environment's variables by name; `shell`, the object `shell.read` answers
//! as `shell`; `leader`, `{"held":true}` or the `refused` code and
//! sentence; `confirmations`, the serials of the calls the kernel holds
//! for the person's confirmation (`nc_kernel::Kernel::pending_moves`); and
//! `keys`, how many times a shortcut setting wrote the channel's keybinding
//! configuration since this daemon started (`crate::environment::Keybindings`),
//! which whoever holds the keys in force reads the file again for, compared
//! only for being another number — each only once there is one. The revision is this process's
//! alone: a daemon that starts again begins at zero, holding no environment
//! and no Leader's outcome of the last one's, and the interface's state is
//! the kernel's, which keeps it between starts.
//!
//! - **A watch** names the revision its client has seen, or none, and how
//!   long it will wait. A state at another revision — smaller or larger, the
//!   two are only compared for being the same — is answered at once, with the
//!   revision and the state; otherwise the first change is, or the wait ends
//!   and the answer is the revision alone. The wait is the shorter of the one
//!   named and `ipc_watch_wait_ms`. A watch that was waiting when a stop
//!   began is refused as stopping at that moment, so a stop waits for no
//!   watch. Any number of connections may watch, and one change answers them
//!   all. **Nothing of a connection is read while its watch waits**, so a
//!   watcher that went away is found only when the answer is written.
//! - **The kernel's state is taken in when a watch is asked and each time the
//!   kernel tells it moved** (`nc_kernel::Kernel::shell_moves`), the move
//!   marked seen before the state is read, so no move is missed: a state the
//!   kernel holds at a revision newer than the one taken in last moves the
//!   watched revision by one, and so answers every watch that waits — and
//!   an older one a slower watch read, taken in after it, moves nothing.
//! - **A note is the kernel's to apply** (`nc_kernel::Kernel::note`): the
//!   facts of the interface's state it says, each decided for the person as
//!   the command it is the equivalent of would be, journaled nowhere and made
//!   whole or not at all. It is admitted as a dispatch is, so a stop drains it
//!   or refuses it, and what the kernel refuses is its answer; what it moved
//!   reaches every watch as any move of the kernel's state does.
//! - **A display** replaces the environment held, moving the revision by one
//!   — and not when it says what is held already. It was read strictly,
//!   within its bounds, by `nc_ipc` before it reached this.
//! - **The revision starts at zero and turns to zero after `i64::MAX`**, the
//!   largest whole number the wire writes, so every revision crosses as it
//!   is.
//!
//! A session is answered none of the three (below).
//!
//! # The desktop the daemon starts, and what it alone answers
//!
//! **The daemon starts its desktop interface** (`crate::desktop`) when its
//! kernel begins to serve, when a person's connection announces a display,
//! and before a press of the Leader is dispatched — wherever none it started
//! runs and a window could be shown. **A person's connection attaches as that
//! desktop** by an attach proving the secret the daemon handed the process it
//! started, over the connection's own handshake, from that very process as the
//! system names it at the other end; one that does not is refused with
//! `NC-DMN-0017`, logged as `ipc.refused`, and closed, and one attached already
//! is refused the same way. **While a connection is attached, a dispatch of a
//! command that cannot be undone, or of MCP switched on, is held for the
//! person's confirmation**
//! (`nc_kernel::Kernel::dispatch_held`) — the person's and an agent's session's
//! alike — its wait ending early once a stop begins; with none attached it is
//! dispatched as ever, and refused at once. **Only an attached connection
//! answers a held call**, by a confirm or a decline naming its serial, which
//! the kernel settles (`nc_kernel::Kernel::settle`); on any other connection
//! either is refused with `NC-DMN-0018`, logged as `ipc.refused`, and nothing
//! is answered. A session is answered none of the three.
//!
//! # An agent's session (`§20.3`)
//!
//! **A connection is the person's until it opens a session, and that
//! session's agent's from then on, never back** (`Bound`): only the person's
//! answerer opens one, and the binding is made once. A session opens, the
//! first refusal the answer, only while the kernel runs, within
//! `sessions_max` — counted before anything else is read, `NC-DMN-0016` past
//! it — while MCP is on (`NC-DMN-0014`, or the store's own refusal when the
//! switch could not be read: never read as on), and for an agent the
//! profile's person made, which the kernel opens from its directory's answer
//! (`NC-AI-0001`, or `NC-POL-0006` when the directory does not answer). **A
//! refused session closes its connection** once the
//! refusal is written, so no client that was refused goes on as the person.
//!
//! **A session asks for its tools and dispatches commands, and nothing
//! else**: before each of its requests, once the kernel runs, MCP is read
//! again, and off — however it came to be, the person's set, undo or redo —
//! or unreadable ends the session with its connection once the refusal is
//! written, whatever it asked; while MCP is on, the registry, the status, a
//! stop, a second session, a watch, a note, a display, an attach, a confirm
//! and a decline are refused (`NC-DMN-0015`) and the session stands — so an agent learns nothing of
//! what an interface shows, and tells the daemon nothing as one. **A held
//! call waits past the request it was read in**, so MCP is read again once
//! the person confirms it, before the kernel decides it again: off, or
//! unreadable, refuses it the same way, run nowhere and journaled nowhere,
//! and ends the session. Its tools are the kernel's probe of what its agent may
//! dispatch ([`nc_kernel::Kernel::dispatchable`]), which spends and journals
//! nothing; its dispatches are the kernel's, decided for the agent and its
//! person and journaled with both.
//!
//! **What is refused here is logged and never journaled**: each refusal of a
//! session's opening, each request no session is answered and each ending for
//! MCP off or unreadable is logged as `daemon.session_refused` by its code,
//! since no command was dispatched; a session opened is logged as
//! `daemon.session_opened`, and its end, with its connection, as
//! `daemon.session_ended`.
//!
//! # What each answer holds
//!
//! - **A command of the registry**, and each of the tools, is one object:
//!   `name`; its entry's texts in the locale the request names —
//!   `display_name`, `line`, `paragraph` and `tags` — the catalog's for a
//!   command it writes and the plugin's own for one it does not, `categories`
//!   by name and `glyph` as `U+XXXX` or `null`; `args_schema` as the text it
//!   was declared in, `mutating`,
//!   `action`, `resource_selector`, `reversibility` (`read`, `write` or
//!   `irreversible`) and `owner` (`kernel` or `plugin`). **Every text is
//!   resolved here**, the catalog's as much as a plugin's, so a client links
//!   no catalog to show a command and escapes every text it was sent alike.
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
use nc_catalog::{ErrorCode, Locale};
use nc_ids::Clock as _;
use nc_ids::Snowflake;
use nc_ipc::{Answer, Drawn, Graphical, Note, Refusal, RefusalKind, Request, State, Welcome};
use nc_kernel::logging::LogSink;
use nc_kernel::registry::{Owner, RegisteredCommand};
use nc_kernel::{Asker, Caller, Kernel, Settled};
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
    /// The person every dispatch on a connection with no session is decided
    /// as, and the one whose agents a session may act as.
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
    /// How many agents' sessions are open: each counted before it opens, and
    /// until its connection ends.
    sessions: AtomicUsize,
    /// The agent of each session open, one for each: held from the moment
    /// the kernel opened it until its connection ends.
    open: Mutex<Vec<Snowflake>>,
    /// What a client may watch, at the revision it is at.
    watched: tokio::sync::watch::Sender<Watched>,
    /// The desktop interface the daemon starts, and the one it started.
    desktop: crate::desktop::Desktop,
    /// How many connections are attached as that desktop: each counted once
    /// it proved it is, and until it closes.
    attached: AtomicUsize,
    /// What a change of the Leader is asked through, once the daemon asks
    /// the system for it (`crate::leader`).
    leader: OnceLock<crate::leader::Asking>,
    log: Sink,
}

impl Server {
    /// A server for the daemon `named`, proving connections by `token`,
    /// starting `desktop` and logging to `log`, in state `starting`.
    #[must_use]
    pub fn new(
        named: Named,
        token: nc_ipc::Token,
        desktop: crate::desktop::Desktop,
        log: Sink,
    ) -> Self {
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
            sessions: AtomicUsize::new(0),
            open: Mutex::new(Vec::new()),
            watched: tokio::sync::watch::Sender::new(Watched::default()),
            desktop,
            attached: AtomicUsize::new(0),
            leader: OnceLock::new(),
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
        let _ = self.desktop_wanted();
    }

    /// A desktop interface, started where none the daemon started runs and
    /// a window could be shown (`crate::desktop`): in the graphical
    /// environment a person's connection announced last, over the daemon's
    /// own.
    pub fn desktop_wanted(&self) -> crate::desktop::Wanted {
        self.desktop
            .wanted(self.announced().as_ref(), &|name| std::env::var_os(name))
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

    /// Accepts connections on `listener` until it is let go
    /// ([`Server::let_go`]), each served on a task of its own — through a
    /// stop's drain too, so a stop asked again, a restart's or a status finds
    /// the daemon answering that it is stopping.
    pub async fn accepting(self: Arc<Self>, mut listener: nc_platform::ipc::Listener) {
        loop {
            let accepted = listener.accept().await;
            match accepted {
                Ok(accepted) => self.serve_one(accepted.stream, Some(accepted.peer.pid)),
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

    /// Serves `stream`, whose other end is the process `peer` as the system
    /// said it, on a task of its own, counted as open until it is closed.
    fn serve_one<S>(self: &Arc<Self>, stream: S, peer: Option<u32>)
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        self.serve_one_after(stream, peer, || core::future::ready(()));
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
        peer: Option<u32>,
        answering: impl Fn() -> Slow + Send + 'static,
    ) where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
        Slow: Future<Output = ()> + Send + 'static,
    {
        self.connections.fetch_add(1, Ordering::SeqCst);
        let open = Open(Arc::clone(self));
        tokio::spawn(Arc::clone(self).connection(stream, open, peer, answering));
    }

    /// One connection: its handshake, then each request in turn until the
    /// client closes it, it fails, or a stop asked for finds it between two
    /// requests; then closed once its last answer is written.
    async fn connection<S, Slow>(
        self: Arc<Self>,
        stream: S,
        open: Open,
        peer: Option<u32>,
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
        // **The first request is owed its answer, a stop or no stop**: a
        // connection made as a stop began, or while it drains — a stop asked
        // again, a status — is told the daemon is stopping, rather than closed
        // unanswered and read as a daemon that does not answer.
        let mut owed = true;
        let mut bound = Bound::Person;
        let mut desk = Desk {
            peer,
            drawn: Some(*served.drawn()),
            attached: None,
        };
        let ended = loop {
            // A request that already arrived is answered first; between two
            // requests, a stop asked for ends the connection.
            let next = if core::mem::take(&mut owed) {
                served.next().await
            } else {
                tokio::select! {
                    biased;
                    next = served.next() => next,
                    () = self.stopping() => break None,
                }
            };
            let request = match next {
                Ok(Some(request)) => request,
                Ok(None) => break None,
                Err(broken) => break Some(broken),
            };
            let answered = match &bound {
                Bound::Person => self.answer(request, &mut desk).await,
                Bound::Session(opened) => self.answer_in_session(opened, request).await,
            };
            let slow = answering();
            slow.await;
            let sent = match served.answer(&answered.answer).await {
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
            // The one assignment: from the person to the session the answer
            // opened, and never from a session, whatever it sent.
            if let (Bound::Person, Some(opened)) = (&bound, answered.opened) {
                bound = Bound::Session(opened);
            }
            // A refusal that ends the connection was logged as a session's;
            // it closes as a client's close does.
            if answered.ends {
                break None;
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
        // The session ends with its connection, and gives its place back
        // before the connection's; and so does the attach.
        drop(bound);
        drop(desk);
        drop(open);
    }

    /// The answer to `request`, on a connection that is the person's, whose
    /// attach as the desktop the daemon started `desk` holds.
    async fn answer(self: &Arc<Self>, request: Request, desk: &mut Desk) -> Answered {
        let person = Caller::new(self.named.person.clone());
        let answer = match request {
            Request::Status => Answer::Status(self.status()),
            Request::Stop => {
                self.stop();
                Answer::Stopping
            }
            Request::Registry { locale } => match self.kernel_if_running() {
                Ok(kernel) => Answer::Registry(
                    kernel
                        .registry()
                        .iter()
                        .map(|command| listed(command, locale))
                        .collect(),
                ),
                Err(refusal) => Answer::Refused(refusal),
            },
            Request::Tools { locale } => match self.kernel_if_running() {
                Ok(kernel) => tools(&kernel, &person, locale).await,
                Err(refusal) => Answer::Refused(refusal),
            },
            Request::Session { agent } => {
                return match self.session(agent).await {
                    Ok(opened) => Answered {
                        answer: Answer::Session,
                        opened: Some(opened),
                        ends: false,
                    },
                    Err(refusal) => Answered::ending(refusal),
                };
            }
            Request::Dispatch { command, args } => {
                // A press of the Leader opens the interface: a desktop is
                // started again first where the one started last is gone.
                if command == nc_ipc::leader::TAP {
                    let _ = self.desktop_wanted();
                }
                self.admitted_dispatch(&person, &command, &args, as_ever)
                    .await
            }
            Request::Watch { seen, wait_ms } => match self.kernel_if_running() {
                Ok(kernel) => self.watch(&*kernel, seen, wait_ms).await,
                Err(refusal) => Answer::Refused(refusal),
            },
            Request::Note(note) => self.noted(&person, &note).await,
            Request::Display(environment) => {
                self.display(environment);
                let _ = self.desktop_wanted();
                Answer::Displayed
            }
            Request::AttachDesktop { proof } => return self.desk_proven(desk, &proof),
            Request::Confirm { pending } => self.settled(desk, pending, Settled::Confirmed),
            Request::Decline { pending } => self.settled(desk, pending, Settled::Declined),
        };
        Answered::going_on(answer)
    }

    /// The answer to an attach proving `proof`, on the connection `desk`
    /// holds: attached when the connection is the desktop the daemon started
    /// — the process it started, proving the secret it handed that process
    /// for this connection — and refused and closed otherwise, logged as
    /// `ipc.refused` with `NC-DMN-0017`. **Once**: a connection attached
    /// already is refused again the same way.
    fn desk_proven(self: &Arc<Self>, desk: &mut Desk, proof: &[u8; 32]) -> Answered {
        let proven = desk.attached.is_none()
            && desk
                .drawn
                .as_ref()
                .is_some_and(|drawn| self.desktop.attaches(desk.peer, proof, drawn));
        if proven {
            self.attached.fetch_add(1, Ordering::SeqCst);
            desk.attached = Some(Attached(Arc::clone(self)));
            return Answered::going_on(Answer::Attached);
        }
        let code = ErrorCode::DaemonAttachRefused;
        self.log.record(&nc_log::events::IpcRefused { code });
        Answered::ending(Refusal {
            code,
            kind: RefusalKind::Denied,
            sentence: "the connection did not prove it is the desktop interface this daemon \
                       started, by the process the daemon started and the secret it handed \
                       that process, so it was closed"
                .to_owned(),
        })
    }

    /// The answer to the person's `settled` answer to the call the kernel
    /// holds as `pending`, on the connection `desk` holds: the kernel's
    /// where the connection is attached as the desktop the daemon started,
    /// and refused with `NC-DMN-0018` — logged as `ipc.refused` — where it is
    /// not, nothing answered.
    fn settled(&self, desk: &Desk, pending: u64, settled: Settled) -> Answer {
        if desk.attached.is_none() {
            let code = ErrorCode::DaemonNotAttached;
            self.log.record(&nc_log::events::IpcRefused { code });
            return Answer::Refused(Refusal {
                code,
                kind: RefusalKind::Denied,
                sentence: "a call held for the person's confirmation is answered by the desktop \
                           interface this daemon started, attached on its own connection, and \
                           this connection is not, so nothing was answered"
                    .to_owned(),
            });
        }
        match self.kernel_if_running() {
            Ok(kernel) => match kernel.settle(pending, settled) {
                Ok(()) => Answer::Settled,
                Err(refusal) => Answer::Refused(refused_by_kernel(&refusal)),
            },
            Err(refusal) => Answer::Refused(refusal),
        }
    }

    /// The answer to a watch that names `seen` and waits `wait_ms`, while the
    /// kernel runs, the interface's state read from `shown`.
    ///
    /// **In order**: the state `shown` holds is taken in, its move marked seen
    /// first; a state at another revision than `seen` is answered at once,
    /// whole; otherwise the first change is, whole — a move `shown` tells is
    /// taken in, and answers once it moved the revision — or a stop asked for
    /// meanwhile refuses the watch as stopping (`NC-DMN-0010`), or the wait
    /// ends — the shorter of `wait_ms` and `ipc_watch_wait_ms` — and the
    /// answer is the revision alone. **Nothing of the connection is read
    /// while this waits**, so a watcher that went away is found only when the
    /// answer is written.
    async fn watch(&self, shown: &impl Shown, seen: Option<u64>, wait_ms: u64) -> Answer {
        let mut watching = self.watched.subscribe();
        let mut moves = shown.moves();
        let mut held = shown.held();
        let wait = Duration::from_millis(wait_ms.min(nc_catalog::DAEMON.ipc_watch_wait_ms));
        let until = tokio::time::Instant::now() + wait;
        let (mut told, mut told_held) = (true, true);
        loop {
            moves.borrow_and_update();
            self.took_in(shown.now());
            let serials = held.borrow_and_update().clone();
            self.held(serials);
            let answer = tokio::select! {
                biased;
                another = watching.wait_for(|watched| Some(watched.revision) != seen) => {
                    another.ok().map(|watched| watched.told(seen))
                }
                () = self.stopping() => Some(Answer::Refused(as_stopping(
                    "the daemon is stopping, and the watch that was waiting ends with it",
                ))),
                heard = moves.changed(), if told => {
                    // A kernel gone tells nothing more, and the watch waits
                    // on what else may change.
                    told = heard.is_ok();
                    continue;
                }
                heard = held.changed(), if told_held => {
                    told_held = heard.is_ok();
                    continue;
                }
                () = tokio::time::sleep_until(until) => None,
            };
            return answer.unwrap_or_else(|| self.watched.borrow().told(seen));
        }
    }

    /// Takes in the interface's state `now` holds, at a new revision — or
    /// changes nothing where the kernel's revision is not newer than the one
    /// taken in last.
    ///
    /// **Newer, never merely another**: two watches read the kernel's state
    /// each outside the channel's lock, so one that read before a move may
    /// take its state in after a watch that read after it; taken in, that
    /// older state would move the watched revision forward with content gone
    /// back, answer every watch waiting with it, and stay until the next
    /// move. The kernel's revision only moves forward in one run, which is
    /// the one kernel this server serves.
    fn took_in(&self, (revision, shell): (u64, Value)) {
        self.watched.send_if_modified(|watched| {
            let another = watched
                .shell
                .as_ref()
                .is_none_or(|(taken, _)| *taken < revision);
            if another {
                watched.shell = Some((revision, shell));
                watched.moved();
            }
            another
        });
    }

    /// Takes in the serials of the calls the kernel holds for the person's
    /// confirmation now, `serials`, at a new revision — or changes nothing
    /// where they are the ones taken in last.
    fn held(&self, serials: Vec<u64>) {
        self.watched.send_if_modified(|watched| {
            let another = watched.held != serials;
            if another {
                watched.held = serials;
                watched.moved();
            }
            another
        });
    }

    /// The answer to `note`, told on a connection that is the person's: the
    /// kernel's to apply, admitted as a dispatch is.
    async fn noted(&self, person: &Caller, note: &Note) -> Answer {
        let (kernel, admitted) = match self.admitted() {
            Ok(admitted) => admitted,
            Err(refusal) => return Answer::Refused(refusal),
        };
        let applied = kernel.note(person, note.detail()).await;
        drop(admitted);
        match applied {
            Ok(()) => Answer::Noted,
            Err(refusal) => Answer::Refused(refused_by_kernel(&refusal)),
        }
    }

    /// Holds `environment` as the graphical environment announced, at a new
    /// revision — or changes nothing where it is the one held.
    fn display(&self, environment: Graphical) {
        self.watched.send_if_modified(|watched| {
            let another = watched.display.as_ref() != Some(&environment);
            if another {
                watched.display = Some(environment);
                watched.moved();
            }
            another
        });
    }

    /// The graphical environment a person's connection announced last, which
    /// a window for the person would be shown in; nothing before one did.
    #[must_use]
    pub fn announced(&self) -> Option<Graphical> {
        self.watched.borrow().display.clone()
    }

    /// The answer to `request`, on a connection bound to the session it
    /// opened, `opened`.
    ///
    /// **In order**: the kernel must run; MCP must be on — off, or a switch
    /// the store could not read, ends the session with its connection once the
    /// refusal is written, whatever the request; what no session asks is
    /// refused (`NC-DMN-0015`) and the session stands; then a dispatch is
    /// admitted as the person's is, so a stop drains it, and decided as the
    /// session's agent, or the tools are the agent's. MCP off or unreadable,
    /// and what no session asks, are logged as `daemon.session_refused` by
    /// their code; a kernel not running yet or stopping (`NC-DMN-0009`,
    /// `NC-DMN-0010`, here or when a dispatch is admitted) is answered as the
    /// person's is, the session standing, and logged by nothing. None of these
    /// refusals reaches the journal.
    async fn answer_in_session(&self, opened: &Opened, request: Request) -> Answered {
        let kernel = match self.kernel_if_running() {
            Ok(kernel) => kernel,
            Err(refusal) => return Answered::going_on(Answer::Refused(refusal)),
        };
        if let Err(refusal) = mcp_is_on(&kernel).await {
            self.refused_session(&refusal);
            return Answered::ending(refusal);
        }
        let asked = match a_sessions(request) {
            Ok(asked) => asked,
            Err(refusal) => {
                self.refused_session(&refusal);
                return Answered::going_on(Answer::Refused(refusal));
            }
        };
        Answered::going_on(match asked {
            InSession::Dispatch { command, args } => {
                // **MCP read again once a held call is confirmed**, before
                // the kernel decides it again: the request was read while
                // MCP was on, and a call held waits past it.
                let off = OnceLock::new();
                let admits = || async {
                    mcp_is_on(&kernel).await.map_err(|refusal| {
                        let answered = nc_kernel::Refusal::new(
                            nc_kernel::RefusalKind::Denied,
                            refusal.code,
                            refusal.sentence.clone(),
                        );
                        let _ = off.set(refusal);
                        answered
                    })
                };
                let answer = self
                    .admitted_dispatch(&opened.session, &command, &args, admits)
                    .await;
                if let Some(refusal) = off.into_inner() {
                    self.refused_session(&refusal);
                    return Answered::ending(refusal);
                }
                answer
            }
            InSession::Tools { locale } => tools(&kernel, &opened.session, locale).await,
        })
    }

    /// The session of the agent `agent` names, opened for the profile's
    /// person, logged as `daemon.session_opened`, or the refusal logged as
    /// `daemon.session_refused` by its code.
    ///
    /// **In order, the first refusal the answer**: the kernel must run
    /// (`NC-DMN-0009`, `NC-DMN-0010`); the session is counted before anything
    /// else is read, and one past `sessions_max` refused (`NC-DMN-0016`); MCP
    /// must be on (`NC-DMN-0014`, or the store's own refusal when the switch
    /// could not be read); and the kernel opens the session from its
    /// directory's answer, for an agent the person made alone (`NC-AI-0001`,
    /// or `NC-POL-0006` when the directory does not answer). A refusal ends
    /// the connection it was asked on, so no refused client goes on as the
    /// person.
    async fn session(self: &Arc<Self>, agent: Snowflake) -> Result<Opened, Refusal> {
        let opened = self.opened(agent).await;
        match &opened {
            Ok(_) => self
                .log
                .record(&nc_log::events::DaemonSessionOpened { agent }),
            Err(refusal) => self.refused_session(refusal),
        }
        opened
    }

    /// What [`Server::session`] opens, before it is logged.
    async fn opened(self: &Arc<Self>, agent: Snowflake) -> Result<Opened, Refusal> {
        let kernel = self.kernel_if_running()?;
        let counted = self.counted_session()?;
        mcp_is_on(&kernel).await?;
        let named = nc_authz::EntityRef {
            type_name: "Agent".to_owned(),
            id: agent.to_crockford(),
        };
        let session = kernel
            .session(&named, &self.named.person)
            .await
            .map_err(|refusal| refused_by_kernel(&refusal))?;
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(agent);
        Ok(Opened {
            agent,
            session,
            counted,
        })
    }

    /// A place among `sessions_max`, counted before the bound is read, so two
    /// sessions racing for the last place cannot both have it.
    fn counted_session(self: &Arc<Self>) -> Result<Counted, Refusal> {
        let before = self.sessions.fetch_add(1, Ordering::SeqCst);
        let counted = Counted(Arc::clone(self));
        let most = usize::try_from(nc_catalog::DAEMON.sessions_max).unwrap_or(0);
        if before < most {
            Ok(counted)
        } else {
            Err(Refusal {
                code: ErrorCode::DaemonTooManySessions,
                kind: RefusalKind::Unavailable,
                sentence: format!(
                    "the daemon holds {most} sessions, as many as `sessions_max` allows, so it \
                     opened no other; one opens once another's connection ends"
                ),
            })
        }
    }

    /// Logs `refusal` as a session's: no command was dispatched, so nothing of
    /// it is journaled.
    fn refused_session(&self, refusal: &Refusal) {
        self.log
            .record(&nc_log::events::DaemonSessionRefused { code: refusal.code });
    }

    /// `command` with `args`, admitted as running until it is answered, and
    /// dispatched by the kernel for `asker`.
    ///
    /// **A command that cannot be undone is held for the person's
    /// confirmation while the desktop the daemon started is attached**
    /// (`nc_kernel::Kernel::dispatch_held`), and its wait ends early when a
    /// stop begins; with none attached it is refused at once
    /// (`NC-KERN-0006`), since nobody could answer it. Held and confirmed, it
    /// is decided again only once `admits` says the asker still may ask it —
    /// a session, while MCP is on.
    async fn admitted_dispatch<'a, Admitted>(
        &self,
        asker: impl Into<Asker<'a>>,
        command: &str,
        args: &Value,
        admits: impl FnOnce() -> Admitted,
    ) -> Answer
    where
        Admitted: Future<Output = Result<(), nc_kernel::Refusal>>,
    {
        match self.admitted() {
            Ok((kernel, admitted)) => {
                let held = self.attached.load(Ordering::SeqCst) > 0;
                let answer = dispatched(
                    &kernel,
                    asker,
                    (command, args),
                    held.then(|| (self.stopping(), admits)),
                )
                .await;
                drop(admitted);
                answer
            }
            Err(refusal) => Answer::Refused(refusal),
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
            (State::Stopping, _) => Err(as_stopping(
                "the daemon is stopping, and takes no new dispatch",
            )),
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

    /// Once a stop was asked for: waits for the dispatches running, for the
    /// schedule's tasks in `schedule` — each inside a run or between two — and
    /// for every connection to write its last answer and close, until
    /// `deadline` at most; answers how many dispatches and runs it gave up on.
    ///
    /// **Still accepting**: a connection made meanwhile is counted with the
    /// rest, answered that the daemon is stopping and closed, so the drain
    /// waits for it as well. One still open at the bound has a client that
    /// takes nothing, one that has asked nothing yet — each connection is owed
    /// an answer to its first request — or a dispatch the drain counts
    /// already, and goes with the runtime.
    pub async fn wound_down(
        &self,
        schedule: Vec<tokio::task::JoinHandle<()>>,
        deadline: tokio::time::Instant,
    ) -> u32 {
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

    /// Lets the endpoint go once the drain is over: `accepting` ends, and every
    /// connection it took as it ended writes its answer and closes, until
    /// `deadline` at most.
    ///
    /// **The last thing a stop does at its endpoint**, after the mark is
    /// settled, the local database closed and the PID record removed, so a stop
    /// asked again before then finds the daemon answering, and one asked after
    /// finds no record naming it.
    pub async fn let_go(
        &self,
        accepting: tokio::task::JoinHandle<()>,
        deadline: tokio::time::Instant,
    ) {
        accepting.abort();
        let _ = accepting.await;
        let _ = self.closed(deadline).await;
    }

    /// The agent of each session open now, one for each session: what the
    /// kernel's dev pages are answered (`crate::environment::Host`).
    #[must_use]
    pub fn open_sessions(&self) -> Vec<Snowflake> {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// How many connections are open now: what the kernel's dev pages are
    /// answered against `ipc_connections_max`.
    #[must_use]
    pub fn open_connections(&self) -> u64 {
        u64::try_from(self.connections.load(Ordering::SeqCst)).unwrap_or(u64::MAX)
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
        if let Some(leader) = self.watched.borrow().leader.clone() {
            fields.push(("leader", leader));
        }
        Value::object(fields)
    }

    /// Holds what came of the daemon's asking the system for the Leader,
    /// `asked`, where it is the process that asks (`crate::leader`): held, or
    /// the refusal — of the registration, or of one held and lost.
    ///
    /// **A refusal is never silent**: it is logged as `daemon.leader_refused`
    /// by its code, and held, with its sentence, in the watched state the
    /// interface's info line says it from, and in the status. What is held
    /// already moves no revision.
    pub fn leader_asked(&self, asked: Result<(), &Refusal>) {
        let said = match asked {
            Ok(()) => Value::object([("held", Value::Bool(true))]),
            Err(refusal) => {
                self.log
                    .record(&nc_log::events::DaemonLeaderRefused { code: refusal.code });
                Value::object([(
                    "refused",
                    Value::object([
                        ("code", Value::text(refusal.code.code())),
                        ("sentence", Value::text(refusal.sentence.as_str())),
                    ]),
                )])
            }
        };
        self.watched.send_if_modified(|watched| {
            let another = watched.leader.as_ref() != Some(&said);
            if another {
                watched.leader = Some(said);
                watched.moved();
            }
            another
        });
    }

    /// Holds `asking`, what a change of the Leader is asked through from
    /// then on: said once, by the run that asked the system for the Leader
    /// (`crate::leader::listened`), and one said already stands.
    pub fn leader_held_by(&self, asking: crate::leader::Asking) {
        // Said once a run; a second is another thread's, which this one
        // never asks.
        let _ = self.leader.set(asking);
    }

    /// What a change of the Leader is asked through, where the daemon is the
    /// process that asks the system for it; nothing where it is not.
    #[must_use]
    pub fn leader_asking(&self) -> Option<crate::leader::Asking> {
        self.leader.get().cloned()
    }

    /// A shortcut setting wrote the channel's keybinding configuration: the
    /// count of those writes moves by one, and the watched revision with it,
    /// so every watch that waits is answered and whoever holds the keys in
    /// force — the desktop interface, which asks the system for the Leader on
    /// Linux and macOS — reads the file again.
    pub fn keys_written(&self) {
        self.watched.send_modify(|watched| {
            watched.keys = Watched::next(watched.keys);
            watched.moved();
        });
    }

    /// A press of the Leader the daemon heard: `leader.tap` dispatched as the
    /// person for the system's own seat (`nc_ipc::leader`), admitted as every
    /// dispatch is, so a stop drains it or refuses it, and journaled as every
    /// dispatch is.
    pub async fn leader_pressed(&self) {
        let _ = self.desktop_wanted();
        let person = Caller::new(self.named.person.clone());
        let _ = self
            .admitted_dispatch(
                &person,
                nc_ipc::leader::TAP,
                &nc_ipc::leader::tap(),
                as_ever,
            )
            .await;
    }
}

/// The refusal of what is asked once a stop began, or was waiting when one
/// did (`NC-DMN-0010`), saying `sentence`.
fn as_stopping(sentence: &str) -> Refusal {
    Refusal {
        code: ErrorCode::DaemonStopping,
        kind: RefusalKind::Unavailable,
        sentence: sentence.to_owned(),
    }
}

/// What a client may watch (`Request::Watch`): what the daemon holds of the
/// interface, and the revision it is at.
///
/// **What it holds at this protocol**: the graphical environment a person's
/// connection announced last, the interface's state the kernel held when it
/// was last taken in, and what came of the daemon's asking for the Leader.
/// The first and the last are this process's alone, journaled and kept
/// nowhere.
///
/// **A revision is one of this start's, and never past what the wire's
/// integer holds**: it starts at zero, each change moves it by one, and after
/// [`Watched::LAST`] it is zero again — a revision is compared for being
/// another one, never for being later, so the turn loses nothing, and no two
/// revisions are ever written as the same number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Watched {
    /// The revision this is at.
    revision: u64,
    /// The graphical environment announced last.
    display: Option<Graphical>,
    /// The interface's state as the kernel held it when it was taken in, and
    /// the kernel's revision of it.
    shell: Option<(u64, Value)>,
    /// What came of the daemon's asking for the Leader, where it asks
    /// (`crate::leader`).
    leader: Option<Value>,
    /// The serials of the calls the kernel holds for the person's
    /// confirmation, as it told them last: none held, none.
    held: Vec<u64>,
    /// How many times a shortcut setting wrote the channel's keybinding
    /// configuration since this daemon started, counted as a revision is.
    keys: u64,
}

impl Watched {
    /// The last revision before the count turns to zero: the largest whole
    /// number the wire writes, `i64::MAX`.
    const LAST: u64 = i64::MAX.unsigned_abs();

    /// The count after `count`, which turns to zero past [`Watched::LAST`].
    const fn next(count: u64) -> u64 {
        if count >= Self::LAST { 0 } else { count + 1 }
    }

    /// Moves to the next revision.
    const fn moved(&mut self) {
        self.revision = Self::next(self.revision);
    }

    /// The count of the writes of the keybinding configuration, as a watch
    /// is answered it: nothing while there is none.
    fn keys_said(&self) -> Option<Value> {
        (self.keys > 0).then(|| Value::Integer(i64::try_from(self.keys).unwrap_or(i64::MAX)))
    }

    /// What is held, as the one object a watch is answered: `display`, the
    /// environment's variables by name; `shell`, the interface's state;
    /// `leader`, what came of asking for it; `confirmations`, the calls held;
    /// and `keys`, how many times a setting wrote the keybinding
    /// configuration — each only once there is one.
    fn state(&self) -> Value {
        let mut fields = Vec::new();
        if let Some(display) = &self.display {
            fields.push((
                "display",
                Value::object(
                    display
                        .pairs()
                        .map(|(name, value)| (name, Value::text(value))),
                ),
            ));
        }
        if let Some((_, shell)) = &self.shell {
            fields.push(("shell", shell.clone()));
        }
        if let Some(leader) = &self.leader {
            fields.push(("leader", leader.clone()));
        }
        if !self.held.is_empty() {
            fields.push((
                "confirmations",
                Value::Array(
                    self.held
                        .iter()
                        .map(|serial| Value::Integer(i64::try_from(*serial).unwrap_or(i64::MAX)))
                        .collect(),
                ),
            ));
        }
        if let Some(keys) = self.keys_said() {
            fields.push(("keys", keys));
        }
        Value::object(fields)
    }

    /// What a watch that names `seen` is answered of this: the revision, and
    /// the state unless `seen` is that revision.
    fn told(&self, seen: Option<u64>) -> Answer {
        Answer::Changed {
            revision: self.revision,
            state: (seen != Some(self.revision)).then(|| self.state()),
        }
    }
}

/// What a watch reads of the interface's state: the kernel's, in the daemon.
trait Shown {
    /// Each move of the state to another revision, from now on.
    fn moves(&self) -> tokio::sync::watch::Receiver<u64>;
    /// The state now, and the revision it is at.
    fn now(&self) -> (u64, Value);
    /// The serials of the calls held for the person's confirmation, told
    /// each time one is held or let go.
    fn held(&self) -> tokio::sync::watch::Receiver<Vec<u64>>;
}

impl Shown for Kernel<Daemon> {
    fn moves(&self) -> tokio::sync::watch::Receiver<u64> {
        self.shell_moves()
    }

    fn now(&self) -> (u64, Value) {
        self.shell()
    }

    fn held(&self) -> tokio::sync::watch::Receiver<Vec<u64>> {
        self.pending_moves()
    }
}

/// Who a connection's requests are answered as.
///
/// **The person until the connection opens a session, and that session's
/// agent from then on, never back**: only the person's answerer opens one, and
/// the loop binds a connection only while it is the person's, so a connection
/// is bound once and nothing it sends rebinds it.
#[derive(Debug)]
enum Bound {
    /// The profile's person.
    Person,
    /// The agent of the session the connection opened.
    Session(Opened),
}

/// A session open on one connection: its agent, the kernel's session, and its
/// place among `sessions_max`.
///
/// Dropping it ends the session: `daemon.session_ended` is logged, the
/// kernel's session goes, and its place is given back after it, in the order
/// the fields are written.
#[derive(Debug)]
struct Opened {
    /// The agent, as the request named it and the kernel holds it.
    agent: Snowflake,
    /// The kernel's session, which every dispatch on the connection is
    /// decided for.
    session: nc_kernel::Session,
    /// Its place among `sessions_max`.
    counted: Counted,
}

impl Drop for Opened {
    fn drop(&mut self) {
        let server = &self.counted.0;
        // One of this agent's sessions is no longer open: an agent may hold
        // several, each listed once.
        let mut open = server.open.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(at) = open.iter().position(|agent| *agent == self.agent) {
            open.swap_remove(at);
        }
        drop(open);
        server
            .log
            .record(&nc_log::events::DaemonSessionEnded { agent: self.agent });
    }
}

#[cfg(test)]
impl Server {
    /// The answer to `request`, on a person's connection that attached
    /// nothing and whose other end and handshake nobody said.
    async fn asked(self: &Arc<Self>, request: Request) -> Answered {
        self.answer(request, &mut Desk::default()).await
    }
}

/// What a person's connection holds of the desktop the daemon started: the
/// process at its other end, what its handshake drew, and its attach once it
/// proved it is that desktop.
#[derive(Debug, Default)]
struct Desk {
    /// The process at the other end, as the system said it when the
    /// connection was accepted.
    peer: Option<u32>,
    /// What the connection's handshake drew.
    drawn: Option<Drawn>,
    /// The attach, once the connection proved it.
    attached: Option<Attached>,
}

/// A connection attached as the desktop the daemon started, counted until it
/// is dropped.
#[derive(Debug)]
struct Attached(Arc<Server>);

impl Drop for Attached {
    fn drop(&mut self) {
        self.0.attached.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A session counted among `sessions_max` until it is dropped.
#[derive(Debug)]
struct Counted(Arc<Server>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.sessions.fetch_sub(1, Ordering::SeqCst);
    }
}

/// What one request comes to: the answer, the session it opened, and whether
/// the connection closes once the answer is written.
#[derive(Debug)]
struct Answered {
    /// What the client is answered.
    answer: Answer,
    /// The session the request opened, which the connection is bound to.
    opened: Option<Opened>,
    /// Whether the connection closes once the answer is written.
    ends: bool,
}

impl Answered {
    /// `answer`, the connection going on as it was.
    const fn going_on(answer: Answer) -> Self {
        Self {
            answer,
            opened: None,
            ends: false,
        }
    }

    /// `refusal`, the connection closed once it is written.
    const fn ending(refusal: Refusal) -> Self {
        Self {
            answer: Answer::Refused(refusal),
            opened: None,
            ends: true,
        }
    }
}

/// What a session may ask: a dispatch, or its tools.
#[derive(Debug)]
enum InSession {
    /// A command with its arguments, decided as the session's agent.
    Dispatch {
        /// The command's registered name.
        command: String,
        /// Its arguments.
        args: Value,
    },
    /// The commands the session's agent may dispatch now.
    Tools {
        /// The locale their texts are resolved in.
        locale: Locale,
    },
}

/// `request`, when a session may ask it, or `NC-DMN-0015`.
///
/// **A session asks for its tools and dispatches commands, and nothing
/// else**: the registry lists what its agent may not dispatch, the status
/// holds the person's identifier, a stop is the person's, a second
/// session would rebind what is bound once, and a watch, a note, a display, an
/// attach and an answer to a held call are the interface's — what the
/// person's window shows, where it is shown, and the person's own answer. The
/// session that asked stands.
fn a_sessions(request: Request) -> Result<InSession, Refusal> {
    let what = match request {
        Request::Dispatch { command, args } => return Ok(InSession::Dispatch { command, args }),
        Request::Tools { locale } => return Ok(InSession::Tools { locale }),
        Request::Registry { .. } => "the registry",
        Request::Session { .. } => "another session",
        Request::Status => "the status",
        Request::Stop => "a stop",
        Request::Watch { .. } => "a watch",
        Request::Note(_) => "a note",
        Request::Display(_) => "a display",
        Request::AttachDesktop { .. } => "a desktop's proof",
        Request::Confirm { .. } | Request::Decline { .. } => "an answer to a held call",
    };
    Err(Refusal {
        code: ErrorCode::DaemonNotForASession,
        kind: RefusalKind::Denied,
        sentence: format!(
            "a session asks for its tools and dispatches commands, and {what} is neither; the \
             session stands as it was"
        ),
    })
}

/// Whether MCP is on, as the kernel's store holds the switch now, and the
/// refusal that ends a session when it is not.
async fn mcp_is_on(kernel: &Kernel<Daemon>) -> Result<(), Refusal> {
    switch_holds(
        kernel
            .settings()
            .await
            .map(|settings| settings.mcp)
            .map_err(|refusal| refused_by_kernel(&refusal)),
    )
}

/// A switch read as `read`: on admits, off is `NC-DMN-0014`, and one the
/// store could not read is the store's refusal — never read as on.
fn switch_holds(read: Result<bool, Refusal>) -> Result<(), Refusal> {
    if read? {
        Ok(())
    } else {
        Err(Refusal {
            code: ErrorCode::DaemonMcpOff,
            kind: RefusalKind::Denied,
            sentence: "MCP is off in this channel, so no session opens and an open one ends; a \
                       person switches it on with `settings.mcp.set`"
                .to_owned(),
        })
    }
}

/// The commands `asker` may dispatch now, their texts in `locale`.
///
/// Each as the registry lists it: the kernel's probe, which decides nothing
/// it journals and spends nothing.
async fn tools<'a>(kernel: &Kernel<Daemon>, asker: impl Into<Asker<'a>>, locale: Locale) -> Answer {
    Answer::Tools(
        kernel
            .dispatchable(asker)
            .await
            .iter()
            .map(|command| listed(command, locale))
            .collect(),
    )
}

/// What a person's own call is, once its person confirmed it: still admitted.
fn as_ever() -> core::future::Ready<Result<(), nc_kernel::Refusal>> {
    core::future::ready(Ok(()))
}

/// `command` with `args`, dispatched by the kernel for `asker`.
///
/// The person, or a session's agent — held for the person's confirmation
/// until the first future of `held` ends, where there is one, and once
/// confirmed decided again while the second still admits it.
async fn dispatched<'a, Admitted>(
    kernel: &Kernel<Daemon>,
    asker: impl Into<Asker<'a>>,
    (command, args): (&str, &Value),
    held: Option<(impl Future<Output = ()>, impl FnOnce() -> Admitted)>,
) -> Answer
where
    Admitted: Future<Output = Result<(), nc_kernel::Refusal>>,
{
    let canonical = args.to_canonical();
    let answered = match held {
        Some((lapsed, admits)) => {
            kernel
                .dispatch_held(asker, command, &canonical, lapsed, admits)
                .await
        }
        None => kernel.dispatch(asker, command, &canonical).await,
    };
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
        Err(refusal) => Answer::Refused(refused_by_kernel(&refusal)),
    }
}

/// The kernel's refusal as the protocol carries it: its code, its kind by
/// the kernel's own word, and its sentence.
fn refused_by_kernel(refusal: &nc_kernel::Refusal) -> Refusal {
    Refusal {
        code: refusal.code(),
        kind: kind_of(refusal.kind()),
        sentence: refusal.sentence().to_owned(),
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

/// A command of the registry, as one object, its texts in `locale`.
fn listed(command: &RegisteredCommand, locale: Locale) -> Value {
    use nc_schema::descriptor::Reversibility;
    let texts = command.entry.in_locale(locale);
    let texts_of =
        |all: &[String]| Value::Array(all.iter().map(|one| Value::text(one.as_str())).collect());
    Value::object([
        ("name", Value::text(command.name.as_str())),
        ("display_name", Value::text(texts.display_name.as_str())),
        ("line", Value::text(texts.line.as_str())),
        ("paragraph", Value::text(texts.paragraph.as_str())),
        ("tags", texts_of(&texts.tags)),
        ("categories", texts_of(&command.entry.categories)),
        (
            "glyph",
            command.entry.glyph.map_or(Value::Null, |glyph| {
                Value::text(format!("U+{:04X}", u32::from(glyph)))
            }),
        ),
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
    use nc_catalog::{ErrorCode, Locale};
    use nc_ipc::{Answer, Refusal, RefusalKind, Request, State};

    /// A request for the registry, in English.
    const REGISTRY: Request = Request::Registry { locale: Locale::En };

    use super::{Named, Server, Settled};

    /// A world file as an installer writes one, read: the one every suite's
    /// install copies in.
    fn a_world() -> nc_authz::LocalWorld {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        nc_authz::LocalWorld::parse(&crate::testing::a_world(), &pdp).expect("the world reads")
    }

    /// A server for a daemon of `channel` that never bound an endpoint, its
    /// lines sent to a store nothing answers at.
    fn a_server(channel: nc_channel::Channel) -> Server {
        a_server_starting(channel, no_desktop())
    }

    /// What [`a_server`] makes, starting `desktop`.
    fn a_server_starting(channel: nc_channel::Channel, desktop: crate::desktop::Desktop) -> Server {
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
            desktop,
            Arc::new(std::sync::Mutex::new(log)),
        )
    }

    /// A desktop whose executable is not there, so none is ever started.
    fn no_desktop() -> crate::desktop::Desktop {
        crate::desktop::Desktop::for_tests(
            std::path::PathBuf::new(),
            Vec::new(),
            std::path::PathBuf::new(),
        )
    }

    /// The refusal `request` is answered with.
    async fn refused(server: &Arc<Server>, request: Request) -> Refusal {
        match server.asked(request).await.answer {
            Answer::Refused(refusal) => refusal,
            other => panic!("answered {other:?}"),
        }
    }

    /// A note setting the prompt's draft to `draft`.
    fn a_note(draft: &str) -> nc_ipc::Note {
        nc_ipc::Note::new(Value::object([(
            "draft",
            Value::object([("text", Value::text(draft))]),
        )]))
        .expect("a note")
    }

    /// An interface's state a test moves, read by a watch as the kernel's is.
    struct Moving {
        /// What a move is told through.
        moves: tokio::sync::watch::Sender<u64>,
        /// The state now, and its revision.
        now: std::sync::Mutex<(u64, Value)>,
        /// The serials of the calls held, told as the kernel tells them.
        held: tokio::sync::watch::Sender<Vec<u64>>,
    }

    impl Moving {
        /// A state holding `draft`, at revision zero, with no call held.
        fn new(draft: &str) -> Self {
            Self {
                moves: tokio::sync::watch::Sender::new(0),
                now: std::sync::Mutex::new((0, shell(draft))),
                held: tokio::sync::watch::Sender::new(Vec::new()),
            }
        }

        /// The state moved to `revision`, holding `draft`, and the move told.
        fn moved(&self, revision: u64, draft: &str) {
            *self.now.lock().expect("nothing panics holding it") = (revision, shell(draft));
            self.moves.send_replace(revision);
        }
    }

    impl super::Shown for Moving {
        fn moves(&self) -> tokio::sync::watch::Receiver<u64> {
            self.moves.subscribe()
        }

        fn now(&self) -> (u64, Value) {
            self.now.lock().expect("nothing panics holding it").clone()
        }

        fn held(&self) -> tokio::sync::watch::Receiver<Vec<u64>> {
            self.held.subscribe()
        }
    }

    /// An interface's state as a test writes one: its draft alone.
    fn shell(draft: &str) -> Value {
        Value::object([("draft", Value::text(draft))])
    }

    /// An announced environment whose display is `display`.
    fn a_display(display: &str) -> nc_ipc::Graphical {
        nc_ipc::Graphical::new([("DISPLAY", display)]).expect("an environment")
    }

    /// An agent's identifier as the profile's minter gives one, of no agent
    /// a person made.
    fn minted() -> nc_ids::Snowflake {
        let flake = nc_ids::MachineFlake::from_value(5).expect("a flake");
        nc_ids::Minter::new(flake, nc_ids::SystemClock)
            .mint()
            .expect("minted")
    }

    /// `request`, answered over `client` within a drain's bound.
    async fn over(
        client: &mut nc_ipc::Client,
        request: Request,
    ) -> Result<Answer, nc_ipc::IpcError> {
        let bound = Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms);
        tokio::time::timeout(bound, client.ask(&request))
            .await
            .expect("answered in time")
    }

    /// `request`, answered over `client` within the suites' patience: a
    /// daemon in another process, which a loaded machine may keep waiting.
    async fn asked(client: &mut nc_ipc::Client, request: Request) -> Answer {
        tokio::time::timeout(crate::testing::PATIENCE, client.ask(&request))
            .await
            .expect("answered in time")
            .expect("answered")
    }

    /// A client of `server` over a connection it serves in memory, proven by
    /// the server's own token.
    async fn a_client_of(server: &Arc<Server>) -> nc_ipc::Client {
        let (daemon_end, client_end) = tokio::io::duplex(1 << 16);
        server.serve_one(daemon_end, None);
        let token =
            nc_ipc::Token::from_text(&server.token.to_text()).expect("the token reads back");
        nc_ipc::connect(client_end, &token)
            .await
            .expect("connected")
            .0
    }

    /// A kernel served as the daemon serves one.
    ///
    /// Opened over a scratch install's profile and served by a running
    /// server, with MCP switched on by the profile's person and an agent of
    /// theirs granted naming.
    struct Serving {
        // Fields drop in the order written: the server and the kernel go
        // before the install they were opened over.
        server: Arc<Server>,
        kernel: Arc<nc_kernel::Kernel<crate::environment::Daemon>>,
        agent: nc_ids::Snowflake,
        store: nc_local::LocalStore,
        installed: crate::testing::Installed,
    }

    /// A permit the family pack does not ship: an agent switches MCP.
    ///
    /// Its switch on is a call held for its person's confirmation, as one
    /// that cannot be undone is, and one no shipped pack lets an agent reach.
    const AN_AGENT_SWITCHES_MCP: (&str, &str) = (
        "policy/packs/family/agents_for_tests.cedar",
        "@id(\"family.an_agent_switches_mcp_for_tests\")\n\
         permit (principal is Agent, action == Action::\"Write\", resource == Feature::\"mcp\");\n",
    );

    /// The core and the family pack as they are on disk, with `extra` beside them.
    ///
    /// A decision point the tree does not ship, built through the same
    /// compiler.
    fn the_family_and(extra: &[(&str, &str)]) -> nc_authz::Pdp {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut texts: Vec<(String, String)> = Vec::new();
        for directory in ["policy/core", "policy/packs/family"] {
            let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(root.join(directory))
                .expect("a policy directory")
                .map(|entry| entry.expect("an entry").path())
                .filter(|path| path.extension().is_some_and(|kind| kind == "cedar"))
                .collect();
            paths.sort();
            for path in paths {
                let name = path.file_name().expect("a file").to_string_lossy();
                texts.push((
                    format!("{directory}/{name}"),
                    std::fs::read_to_string(&path).expect("a readable policy"),
                ));
            }
        }
        let mut files: Vec<(&str, &str)> = texts
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        files.extend_from_slice(extra);
        nc_authz::Pdp::from_sources_for_tests(
            include_str!("../../../policy/schema.cedarschema"),
            &files,
            "family",
            include_str!("../../../policy/packs/family/pack.toml"),
        )
        .expect("the pack compiles with what the test adds")
    }

    impl Serving {
        /// The kernel opened and served, MCP on and the agent granted naming.
        async fn new() -> Self {
            Self::deciding_with(&[]).await
        }

        /// What [`Serving::new`] opens, its kernel deciding with the family
        /// pack and `extra` beside it where there is any.
        async fn deciding_with(extra: &[(&str, &str)]) -> Self {
            let installed =
                crate::testing::Installed::new(&std::env::current_exe().expect("this test binary"));
            let (layout, user) = (&installed.layout, &installed.user);
            let (store, _) = crate::environment::store(layout).await.expect("opened");
            let (world, pdp) =
                crate::profile::world_and_pack(layout, user).expect("the world reads");
            let pdp = if extra.is_empty() {
                pdp
            } else {
                the_family_and(extra)
            };
            let named = Named {
                pid: 4242,
                instance: [9; 16],
                channel: nc_channel::Channel::Staging,
                realm: world.realm().as_str().to_owned(),
                person: world.person().clone(),
                since: 1_767_225_600_000,
            };
            let log = crate::environment::sink(
                nc_log::Log::open(crate::environment::origin(), crate::testing::NOBODY_LISTENS)
                    .expect("a store on this machine"),
            );
            let flake = nc_ids::MachineFlake::from_value(3).expect("a flake");
            let opening = crate::environment::Opening {
                world,
                pdp,
                keeper: nc_ids::OpaqueId::of_device_key(&[7; 32]),
                minter: Arc::new(std::sync::Mutex::new(nc_ids::Minter::new(
                    flake,
                    nc_ids::SystemClock,
                ))),
                log: Arc::clone(&log),
            };
            let kernel = Arc::new(
                crate::environment::kernel(layout, store.clone(), opening)
                    .await
                    .expect("the kernel opens"),
            );
            let server = Arc::new(Server::new(
                named,
                nc_ipc::Token::fresh().expect("a token"),
                no_desktop(),
                log,
            ));
            server.serving(Arc::clone(&kernel), Vec::new());
            let mut serving = Self {
                server,
                kernel,
                agent: minted(),
                store,
                installed,
            };
            serving.switched(true).await;
            let made = serving
                .as_the_person("agents.create", r#"{"budget":100,"label":"a"}"#)
                .await;
            serving.agent = crate::testing::the_agent_made(&made);
            serving
                .as_the_person(
                    "agents.grant",
                    &format!(
                        r#"{{"agent":"{}","feature":"naming","minutes":60,"reason":"a"}}"#,
                        serving.agent.to_crockford()
                    ),
                )
                .await;
            serving
        }

        /// What `command` with `args` answered, dispatched by the profile's
        /// person on the kernel.
        async fn as_the_person(&self, command: &str, args: &str) -> Value {
            let person = nc_kernel::Caller::new(self.server.named.person.clone());
            let answered = self
                .kernel
                .dispatch(&person, command, args)
                .await
                .unwrap_or_else(|refusal| panic!("{command} was refused: {refusal:?}"));
            nc_canon::parse(&answered).expect("one value")
        }

        /// MCP switched `on` by the profile's person — on with a
        /// confirmation the kernel issued, as their answer in the Noctua
        /// window would be.
        async fn switched(&self, on: bool) {
            let args = format!(r#"{{"on":{on}}}"#);
            let person = &self.server.named.person;
            let caller = nc_kernel::Caller::new(person.clone()).confirmed(
                self.kernel
                    .confirmation_for_tests(person, "settings.mcp.set", &args),
            );
            self.kernel
                .dispatch(&caller, "settings.mcp.set", &args)
                .await
                .unwrap_or_else(|refusal| panic!("the switch was refused: {refusal:?}"));
        }

        /// The entries of `command` the journal holds, each by whom it names
        /// and whom it acts for.
        async fn decided(&self, command: &str) -> Vec<(String, Vec<String>)> {
            self.kernel
                .journal_for_tests()
                .await
                .expect("the journal reads")
                .into_iter()
                .map(|entry| entry.record)
                .filter(|record| {
                    record
                        .resource
                        .clone()
                        .into_object()
                        .and_then(|mut fields| fields.remove("command"))
                        .and_then(Value::into_text)
                        .as_deref()
                        == Some(command)
                })
                .map(|record| {
                    (
                        record.principal.as_str().to_owned(),
                        record
                            .actor_chain
                            .iter()
                            .map(|link| link.as_str().to_owned())
                            .collect(),
                    )
                })
                .collect()
        }

        /// Everything let go: the server and its kernel, then the database.
        async fn closed(self) {
            let Self {
                server,
                kernel,
                store,
                installed,
                ..
            } = self;
            drop(server);
            drop(kernel);
            store.close().await;
            drop(installed);
        }
    }

    /// A session asks for its tools and dispatches commands, and is refused
    /// the rest.
    ///
    /// **Over every request the wire reads**, through a match with no
    /// wildcard: a dispatch and the tools pass as they were asked, and the
    /// registry, the status, a stop, a second session, a watch, a note, a
    /// display, an attach and an answer to a held call are each refused with
    /// `NC-DMN-0015`, denied, saying the session stands and naming what was
    /// asked — so no agent attaches as the desktop, nor answers what waits for
    /// its person.
    #[test]
    fn a_session_asks_for_its_tools_and_dispatches_and_is_refused_the_rest() {
        let dispatch = Request::Dispatch {
            command: "naming.read".to_owned(),
            args: Value::object([("name", Value::text("a"))]),
        };
        for request in [
            dispatch,
            REGISTRY,
            Request::Tools {
                locale: Locale::PtBR,
            },
            Request::Session { agent: minted() },
            Request::Status,
            Request::Stop,
            Request::Watch {
                seen: None,
                wait_ms: 0,
            },
            Request::Note(a_note("a")),
            Request::Display(a_display(":0")),
            Request::AttachDesktop { proof: [1; 32] },
            Request::Confirm { pending: 1 },
            Request::Decline { pending: 1 },
        ] {
            let a_sessions_own = match &request {
                Request::Dispatch { .. } | Request::Tools { .. } => None,
                Request::Registry { .. } => Some("the registry"),
                Request::Session { .. } => Some("another session"),
                Request::Status => Some("the status"),
                Request::Stop => Some("a stop"),
                Request::Watch { .. } => Some("a watch"),
                Request::Note(_) => Some("a note"),
                Request::Display(_) => Some("a display"),
                Request::AttachDesktop { .. } => Some("a desktop's proof"),
                Request::Confirm { .. } | Request::Decline { .. } => {
                    Some("an answer to a held call")
                }
            };
            match (super::a_sessions(request.clone()), request) {
                (
                    Ok(super::InSession::Dispatch { command, args }),
                    Request::Dispatch {
                        command: asked,
                        args: with,
                    },
                ) => assert_eq!((command, args), (asked, with)),
                (Ok(super::InSession::Tools { locale }), Request::Tools { locale: asked }) => {
                    assert_eq!(locale, asked);
                }
                (Err(refusal), request) => {
                    let what = a_sessions_own
                        .unwrap_or_else(|| panic!("{request:?} was refused: {refusal:?}"));
                    assert!(
                        refusal.sentence.contains(&format!("and {what} is neither")),
                        "{refusal:?}"
                    );
                    assert_eq!(
                        (refusal.code, refusal.kind),
                        (ErrorCode::DaemonNotForASession, RefusalKind::Denied),
                        "{request:?}"
                    );
                    assert!(refusal.sentence.contains("stands"), "{refusal:?}");
                }
                (passed, request) => panic!("{request:?} passed as {passed:?}"),
            }
        }
    }

    /// No session opens before the kernel runs or once a stop began, and each
    /// refusal ends the connection that asked.
    #[tokio::test]
    async fn no_session_opens_before_the_kernel_runs_or_once_a_stop_began() {
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        let session = Request::Session { agent: minted() };
        // `Running` with no kernel open is a start that has not opened one.
        for state in [
            State::Starting,
            State::WaitingForClock,
            State::Recovering,
            State::Running,
        ] {
            server.reach(state);
            let answered = server.asked(session.clone()).await;
            assert!(
                answered.ends && answered.opened.is_none(),
                "{state:?}: {answered:?}"
            );
            assert!(
                matches!(&answered.answer, Answer::Refused(refusal)
                    if refusal.code == ErrorCode::DaemonNotRunning),
                "{state:?}: {answered:?}"
            );
        }
        server.stop();
        let answered = server.asked(session).await;
        assert!(answered.ends && answered.opened.is_none(), "{answered:?}");
        assert!(
            matches!(&answered.answer, Answer::Refused(refusal)
                if refusal.code == ErrorCode::DaemonStopping),
            "{answered:?}"
        );
        assert_eq!(server.sessions.load(Ordering::SeqCst), 0);
    }

    /// A refused session closes its connection and leaves no person behind.
    ///
    /// **Through a connection the server serves**: a session asked of a
    /// daemon whose kernel does not run yet is refused, and the client's next
    /// request on that connection is lost — while a person's registry refused
    /// the same way leaves the connection going, its status answered.
    #[tokio::test]
    async fn a_refused_session_closes_its_connection_and_leaves_no_person_behind() {
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        server.reach(State::Recovering);
        let mut person = a_client_of(&server).await;
        match over(&mut person, REGISTRY).await {
            Ok(Answer::Refused(refusal)) => {
                assert_eq!(refusal.code, ErrorCode::DaemonNotRunning);
            }
            other => panic!("the registry was answered with {other:?}"),
        }
        assert!(matches!(
            over(&mut person, Request::Status).await,
            Ok(Answer::Status(_))
        ));
        let mut refused = a_client_of(&server).await;
        match over(&mut refused, Request::Session { agent: minted() }).await {
            Ok(Answer::Refused(refusal)) => {
                assert_eq!(refusal.code, ErrorCode::DaemonNotRunning);
            }
            other => panic!("the session was answered with {other:?}"),
        }
        let next = over(&mut refused, Request::Status).await;
        assert!(
            matches!(next, Err(nc_ipc::IpcError::Lost(_))),
            "a connection whose session was refused went on: {next:?}"
        );
        drop((person, refused));
        let bound = Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms);
        assert_eq!(server.closed(tokio::time::Instant::now() + bound).await, 0);
    }

    /// A switch off or one the store could not read refuses, and one on
    /// admits; each refusal ends the session that asked.
    ///
    /// **The switch never read as on but by its own word**: on admits, off is
    /// `NC-DMN-0014`, and a read the store refused is that refusal, as it is.
    /// Then over a kernel the daemon opens: an open session's request once its
    /// person switched MCP off is refused and ends the session, and once on
    /// again its next is answered.
    #[tokio::test]
    async fn a_switch_off_or_unreadable_refuses_and_an_on_one_admits() {
        assert_eq!(super::switch_holds(Ok(true)), Ok(()));
        let off = super::switch_holds(Ok(false)).expect_err("off is refused");
        assert_eq!(
            (off.code, off.kind),
            (ErrorCode::DaemonMcpOff, RefusalKind::Denied)
        );
        assert!(off.sentence.contains("`settings.mcp.set`"), "{off:?}");
        let unreadable = Refusal {
            code: nc_local::LocalError::Unexpected(String::new()).code(),
            kind: RefusalKind::Internal,
            sentence: "the entry is not one the kernel wrote".to_owned(),
        };
        assert_eq!(
            super::switch_holds(Err(unreadable.clone())),
            Err(unreadable)
        );
        let serving = Serving::new().await;
        let opened = serving
            .server
            .session(serving.agent)
            .await
            .expect("the session opens");
        let tools = || Request::Tools { locale: Locale::En };
        serving.switched(false).await;
        let answered = serving.server.answer_in_session(&opened, tools()).await;
        assert!(answered.ends, "MCP off left the session open: {answered:?}");
        assert!(
            matches!(&answered.answer, Answer::Refused(refusal)
                if refusal.code == ErrorCode::DaemonMcpOff),
            "{answered:?}"
        );
        serving.switched(true).await;
        let answered = serving.server.answer_in_session(&opened, tools()).await;
        assert!(
            !answered.ends && matches!(answered.answer, Answer::Tools(_)),
            "{answered:?}"
        );
        drop(opened);
        serving.closed().await;
    }

    /// A session's dispatch is counted as running until it answers, so a
    /// stop's drain waits for it as it waits for the person's.
    ///
    /// **Caught while the kernel runs it**: the dispatch is counted as running
    /// before it is answered, a drain meanwhile gives up on it, and once it is
    /// answered it is counted no more.
    #[tokio::test]
    async fn a_sessions_dispatch_is_counted_as_running_until_it_answers() {
        use core::task::Poll;

        let serving = Serving::new().await;
        let opened = serving
            .server
            .session(serving.agent)
            .await
            .expect("the session opens");
        let dispatch = Request::Dispatch {
            command: "naming.propose".to_owned(),
            args: Value::object([("title", Value::text("a"))]),
        };
        let answered = {
            let mut answering =
                core::pin::pin!(serving.server.answer_in_session(&opened, dispatch));
            let running = &serving.server.running;
            let early = core::future::poll_fn(|context| match answering.as_mut().poll(context) {
                Poll::Ready(answered) => Poll::Ready(Some(answered)),
                Poll::Pending if running.load(Ordering::SeqCst) == 1 => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            })
            .await;
            assert!(
                early.is_none(),
                "the session's dispatch was answered and never counted as running: {early:?}"
            );
            let soon = tokio::time::Instant::now() + Duration::from_millis(200);
            assert_eq!(
                serving.server.drained(soon).await,
                1,
                "a drain did not wait for the session's dispatch"
            );
            answering.await
        };
        assert!(
            !answered.ends && matches!(answered.answer, Answer::Result(_)),
            "{answered:?}"
        );
        let bound = tokio::time::Instant::now() + Duration::from_secs(5);
        assert_eq!(serving.server.drained(bound).await, 0);
        drop(opened);
        serving.closed().await;
    }

    /// Sessions are counted before they open, and one past the bound is
    /// refused.
    ///
    /// `sessions_max` places are given out and the next is `NC-DMN-0016`,
    /// unavailable, saying the bound, its count given back as it is refused;
    /// a place dropped lets one more in, and once every one is dropped none is
    /// counted.
    #[test]
    fn sessions_are_counted_before_they_open_and_refused_past_their_bound() {
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        let most = usize::try_from(nc_catalog::DAEMON.sessions_max).expect("a count");
        let mut held: Vec<super::Counted> = (0..most)
            .map(|_| server.counted_session().expect("a place within the bound"))
            .collect();
        assert_eq!(server.sessions.load(Ordering::SeqCst), most);
        let refused = server.counted_session().expect_err("one past the bound");
        assert_eq!(
            (refused.code, refused.kind),
            (ErrorCode::DaemonTooManySessions, RefusalKind::Unavailable)
        );
        assert!(refused.sentence.contains(&most.to_string()), "{refused:?}");
        assert_eq!(
            server.sessions.load(Ordering::SeqCst),
            most,
            "a session refused is still counted"
        );
        drop(held.pop());
        held.push(server.counted_session().expect("the place given back"));
        drop(held);
        assert_eq!(server.sessions.load(Ordering::SeqCst), 0);
    }

    /// The daemon answers its kernel's dev pages the sessions and the
    /// connections it holds, and the newest lines of its own log.
    ///
    /// **Over a kernel the daemon opens, told of its host as a start tells
    /// it**: no session at first; two sessions of one agent are two rows,
    /// one of them gone when its connection ends, and none once both have;
    /// the connections are the server's own count. The log read answers the
    /// lines the daemon's sink kept, each under its serial and exactly the
    /// line the store is sent — an event's name, its level, the service and
    /// its version, and its declared fields, nothing else.
    #[tokio::test]
    async fn the_dev_pages_read_the_sessions_the_connections_and_the_log_this_daemon_holds() {
        use nc_kernel::logging::LogSink as _;

        let serving = Serving::new().await;
        let server = &serving.server;
        serving
            .kernel
            .hosted_by(Arc::new(crate::environment::Host::new(Arc::downgrade(
                server,
            ))));
        let agent = serving.agent.to_crockford();
        let said = |open: usize, connections: usize| {
            let rows = vec![format!(r#"{{"agent":"{agent}"}}"#); open].join(",");
            format!(
                r#"{{"connections":{connections},"connections_max":{},"max":{},"open":{open},"sessions":[{rows}]}}"#,
                nc_catalog::DAEMON.ipc_connections_max,
                nc_catalog::DAEMON.sessions_max
            )
        };
        let read = || async {
            serving
                .as_the_person("dev.sessions.read", "{}")
                .await
                .to_canonical()
        };
        assert_eq!(read().await, said(0, 0));
        let first = server
            .asked(Request::Session {
                agent: serving.agent,
            })
            .await;
        let second = server
            .asked(Request::Session {
                agent: serving.agent,
            })
            .await;
        assert!(first.opened.is_some() && second.opened.is_some());
        server.connections.fetch_add(2, Ordering::SeqCst);
        assert_eq!(read().await, said(2, 2));
        let limits = serving.as_the_person("dev.limits.read", "{}").await;
        let used = |limit: &str| {
            limits
                .clone()
                .into_object()
                .and_then(|mut fields| fields.remove("counts"))
                .and_then(Value::into_array)
                .expect("the counts")
                .into_iter()
                .filter_map(Value::into_object)
                .find(|row| row.get("limit") == Some(&Value::text(limit)))
                .and_then(|mut row| row.remove("used"))
        };
        assert_eq!(used("daemon.sessions_max"), Some(Value::Integer(2)));
        assert_eq!(used("daemon.ipc_connections_max"), Some(Value::Integer(2)));
        drop(second);
        assert_eq!(read().await, said(1, 2));
        drop(first);
        server.connections.fetch_sub(2, Ordering::SeqCst);
        assert_eq!(read().await, said(0, 0));

        let kept = server.log.kept(0, usize::MAX);
        let answered = serving.as_the_person("dev.logs.read", "{}").await;
        let lines: Vec<(Value, Value)> = answered
            .clone()
            .into_object()
            .and_then(|mut fields| fields.remove("lines"))
            .and_then(Value::into_array)
            .expect("the lines")
            .into_iter()
            .filter_map(Value::into_object)
            .filter_map(|mut row| Some((row.remove("serial")?, row.remove("line")?)))
            .collect();
        assert_eq!(
            lines,
            kept.lines
                .iter()
                .map(|(serial, line)| (
                    Value::Integer(i64::try_from(*serial).expect("fits")),
                    Value::text(line.as_str())
                ))
                .collect::<Vec<_>>()
        );
        let opened = format!(
            r#"{{"_msg":"daemon.session_opened","agent":"{agent}","level":"info","service":"noctua-daemon","version":"{}"}}"#,
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(
            kept.lines
                .iter()
                .filter(|(_, line)| *line == opened)
                .count(),
            2,
            "{kept:?}"
        );
        assert!(
            kept.lines
                .iter()
                .map(|(serial, _)| *serial)
                .eq(1..=kept.held),
            "every line this daemon recorded is kept, in order: {kept:?}"
        );
    }

    /// A session is opened only while MCP is on and for an agent of the
    /// profile's person, and a refusal of either ends the connection.
    ///
    /// **Over a kernel the daemon opens**: an agent no person made is
    /// `NC-AI-0001`; with MCP off the person's own agent is `NC-DMN-0014`;
    /// with it on, the session opens, counted until it is dropped. The
    /// person's own tools are what the person may dispatch, MCP's switch among
    /// them.
    #[tokio::test]
    async fn a_session_is_opened_only_while_mcp_is_on_and_for_an_agent_of_the_persons() {
        let serving = Serving::new().await;
        let server = &serving.server;
        let refused = |answered: &super::Answered, code: ErrorCode| {
            assert!(
                answered.ends && answered.opened.is_none(),
                "{code:?}: {answered:?}"
            );
            assert!(
                matches!(&answered.answer, Answer::Refused(refusal) if refusal.code == code),
                "{code:?}: {answered:?}"
            );
        };
        let stranger = server.asked(Request::Session { agent: minted() }).await;
        refused(&stranger, ErrorCode::AiNoSuchAgent);
        serving.switched(false).await;
        let off = server
            .asked(Request::Session {
                agent: serving.agent,
            })
            .await;
        refused(&off, ErrorCode::DaemonMcpOff);
        assert_eq!(server.sessions.load(Ordering::SeqCst), 0);
        serving.switched(true).await;
        let opened = server
            .asked(Request::Session {
                agent: serving.agent,
            })
            .await;
        assert!(
            !opened.ends && opened.opened.is_some() && opened.answer == Answer::Session,
            "{opened:?}"
        );
        assert_eq!(server.sessions.load(Ordering::SeqCst), 1);
        drop(opened);
        assert_eq!(server.sessions.load(Ordering::SeqCst), 0);
        match server
            .asked(Request::Tools { locale: Locale::En })
            .await
            .answer
        {
            Answer::Tools(tools) => assert!(
                tools.iter().any(|tool| tool
                    .to_canonical()
                    .contains("\"name\":\"settings.mcp.set\"")),
                "{tools:?}"
            ),
            other => panic!("the person's tools were answered with {other:?}"),
        }
        serving.closed().await;
    }

    /// A connection that opened a session is its agent's until it ends, and
    /// its person switching MCP off ends it.
    ///
    /// **Through a connection the server serves**: the session opens, its
    /// tools are its agent's — naming's two, and nothing of what the person
    /// may do — and its dispatches are journaled as the agent acting for the
    /// person; a second session, the registry, the status and a stop are each
    /// `NC-DMN-0015`, the daemon still running and the next dispatch still the
    /// agent's; once the person switches MCP off, the next request is
    /// `NC-DMN-0014`, the connection is lost after it, and its place is given
    /// back.
    #[tokio::test]
    async fn a_connection_is_its_sessions_agents_until_mcp_is_switched_off() {
        let serving = Serving::new().await;
        let server = &serving.server;
        let mut client = a_client_of(server).await;
        let agent = serving.agent;
        assert!(matches!(
            over(&mut client, Request::Session { agent }).await,
            Ok(Answer::Session)
        ));
        match over(&mut client, Request::Tools { locale: Locale::En }).await {
            Ok(Answer::Tools(tools)) => {
                let names: Vec<String> = tools
                    .iter()
                    .filter_map(|tool| {
                        tool.clone()
                            .into_object()
                            .and_then(|mut fields| fields.remove("name"))
                            .and_then(Value::into_text)
                    })
                    .collect();
                assert_eq!(names, ["naming.propose", "naming.read"]);
            }
            other => panic!("the tools were answered with {other:?}"),
        }
        let propose = || Request::Dispatch {
            command: "naming.propose".to_owned(),
            args: Value::object([("title", Value::text("a"))]),
        };
        assert!(matches!(
            over(&mut client, propose()).await,
            Ok(Answer::Result(_))
        ));
        for request in [
            Request::Session { agent: minted() },
            REGISTRY,
            Request::Status,
            Request::Stop,
            Request::Watch {
                seen: None,
                wait_ms: 0,
            },
            Request::Note(a_note("a session's")),
            Request::Display(a_display(":66")),
        ] {
            match over(&mut client, request.clone()).await {
                Ok(Answer::Refused(refusal)) => {
                    assert_eq!(refusal.code, ErrorCode::DaemonNotForASession, "{request:?}");
                }
                other => panic!("{request:?} was answered with {other:?}"),
            }
        }
        assert_eq!(
            server.state(),
            State::Running,
            "a session stopped the daemon"
        );
        assert_eq!(
            *server.watched.borrow(),
            super::Watched::default(),
            "a session's note or display was held"
        );
        assert!(matches!(
            over(&mut client, propose()).await,
            Ok(Answer::Result(_))
        ));
        let acting = (agent.to_crockford(), vec!["ana".to_owned()]);
        assert_eq!(
            serving.decided("naming.propose").await,
            [acting.clone(), acting]
        );
        serving.switched(false).await;
        match over(&mut client, Request::Tools { locale: Locale::En }).await {
            Ok(Answer::Refused(refusal)) => assert_eq!(refusal.code, ErrorCode::DaemonMcpOff),
            other => panic!("a request once MCP was off was answered with {other:?}"),
        }
        let next = over(&mut client, propose()).await;
        assert!(
            matches!(next, Err(nc_ipc::IpcError::Lost(_))),
            "a session went on once MCP was off: {next:?}"
        );
        let bound = Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms);
        assert_eq!(server.closed(tokio::time::Instant::now() + bound).await, 0);
        assert_eq!(server.sessions.load(Ordering::SeqCst), 0);
        drop(client);
        serving.closed().await;
    }

    /// A session's call held while MCP went off is refused once confirmed, and ends it.
    ///
    /// **Over a kernel the daemon opens, deciding with a pack that lets an
    /// agent switch MCP**, which no shipped pack does: MCP switched on is held
    /// as a command that cannot be undone is, so the agent's
    /// `settings.mcp.set` asking for on, with a desktop attached, waits for
    /// its person. Its person switches MCP off meanwhile, then confirms the
    /// call: the session is answered `NC-DMN-0014` and ends, as any request of
    /// it once MCP is off is; MCP stays off; and no entry of the agent's
    /// switch is journaled.
    #[tokio::test]
    async fn a_sessions_call_held_while_mcp_was_switched_off_is_refused_once_confirmed() {
        let serving = Serving::deciding_with(&[AN_AGENT_SWITCHES_MCP]).await;
        let agent = serving.agent.to_crockford();
        serving
            .as_the_person(
                "agents.grant",
                &format!(r#"{{"agent":"{agent}","feature":"mcp","minutes":60,"reason":"a"}}"#),
            )
            .await;
        let opened = serving
            .server
            .session(serving.agent)
            .await
            .expect("the session opens");
        serving.server.attached.fetch_add(1, Ordering::SeqCst);
        let mut moves = serving.kernel.pending_moves();
        let asking = serving.server.answer_in_session(
            &opened,
            Request::Dispatch {
                command: "settings.mcp.set".to_owned(),
                args: Value::object([("on", Value::Bool(true))]),
            },
        );
        let answering = async {
            let serial = tokio::time::timeout(
                crate::testing::PATIENCE,
                moves.wait_for(|held| !held.is_empty()),
            )
            .await
            .expect("the agent's call is held in time")
            .expect("the kernel tells what it holds")[0];
            serving.switched(false).await;
            serving
                .kernel
                .settle(serial, Settled::Confirmed)
                .expect("the held call is answered");
        };
        let (answered, ()) = tokio::join!(asking, answering);
        assert!(
            answered.ends,
            "a session whose call was confirmed once MCP was off went on: {answered:?}"
        );
        assert!(
            matches!(&answered.answer, Answer::Refused(refusal)
                if refusal.code == ErrorCode::DaemonMcpOff),
            "{answered:?}"
        );
        assert!(
            !serving
                .kernel
                .settings()
                .await
                .expect("the switch reads")
                .mcp,
            "MCP stays off"
        );
        assert!(
            serving
                .decided("settings.mcp.set")
                .await
                .iter()
                .all(|(principal, _)| *principal != agent),
            "the agent's switch is journaled nowhere"
        );
        serving.server.attached.fetch_sub(1, Ordering::SeqCst);
        drop(opened);
        serving.closed().await;
    }

    /// A scratch directory of this module's tests, removed however the test
    /// leaves.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        /// A private directory of its own, under the temporary one.
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("nc-ldr-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch");
            Self(root)
        }
    }

    /// A press of the Leader starts the desktop again where the one started
    /// last is gone: the daemon's own press and a person's `leader.tap`
    /// alike.
    ///
    /// **Before anything is dispatched**, so it holds of a daemon whose kernel
    /// does not run yet and refuses the dispatch: with an environment naming a
    /// display announced, the daemon's own press starts a desktop; once it
    /// ended, a person's `leader.tap` starts another; once that ended, the
    /// daemon's own press starts a third.
    #[tokio::test]
    async fn a_press_of_the_leader_starts_the_desktop_again_where_the_one_started_last_is_gone() {
        let scratch = Scratch::new();
        let ends = scratch.0.join("ends");
        let desktop = crate::desktop::waiting_for(&ends, scratch.0.join("desktop.err"));
        let server = Arc::new(a_server_starting(nc_channel::Channel::Staging, desktop));
        server.display(a_display(":99"));
        assert_eq!(server.desktop.running(), None, "nothing started yet");
        let ended = || {
            drop(nc_platform::create_private(&ends).expect("the end is told"));
            let deadline = std::time::Instant::now() + crate::testing::PATIENCE;
            while server.desktop.running().is_some() {
                assert!(std::time::Instant::now() < deadline, "it never ended");
                std::thread::sleep(Duration::from_millis(20));
            }
            std::fs::remove_file(&ends).expect("the end is taken back");
        };
        server.leader_pressed().await;
        assert!(
            server.desktop.running().is_some(),
            "the daemon's own press started no desktop"
        );
        ended();
        let tap = Request::Dispatch {
            command: nc_ipc::leader::TAP.to_owned(),
            args: nc_ipc::leader::tap(),
        };
        let tapped = server.asked(tap).await;
        assert!(
            matches!(&tapped.answer, Answer::Refused(refusal)
                if refusal.code == ErrorCode::DaemonNotRunning),
            "{tapped:?}"
        );
        assert!(
            server.desktop.running().is_some(),
            "a person's `leader.tap` started no desktop again"
        );
        ended();
        server.leader_pressed().await;
        assert!(
            server.desktop.running().is_some(),
            "the daemon's own press started no desktop again"
        );
        ended();
    }

    /// What `answer` is when it is a watch's: its revision and its state.
    fn changed(answer: Answer) -> (u64, Option<Value>) {
        match answer {
            Answer::Changed { revision, state } => (revision, state),
            other => panic!("a watch was answered with {other:?}"),
        }
    }

    /// A watch and a note are refused before the kernel runs and once a stop
    /// began, and a display is taken from the first moment.
    ///
    /// **In every state a start goes through**, `Running` with no kernel open
    /// among them: a watch and a note are `NC-DMN-0009`, the connection going
    /// on, and nothing of the interface is taken in; once a stop was asked for
    /// they are `NC-DMN-0010`. A display is answered in each, and what it
    /// announced is what the daemon holds.
    #[tokio::test]
    async fn a_watch_and_a_note_wait_for_the_kernel_and_a_display_does_not() {
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        let watch = || Request::Watch {
            seen: None,
            wait_ms: 0,
        };
        let states = [
            (State::Starting, ErrorCode::DaemonNotRunning),
            (State::WaitingForClock, ErrorCode::DaemonNotRunning),
            (State::Recovering, ErrorCode::DaemonNotRunning),
            (State::Running, ErrorCode::DaemonNotRunning),
            (State::Stopping, ErrorCode::DaemonStopping),
        ];
        for (number, (state, code)) in states.into_iter().enumerate() {
            if state == State::Stopping {
                server.stop();
            } else {
                server.reach(state);
            }
            for request in [watch(), Request::Note(a_note("a"))] {
                let answered = server.asked(request.clone()).await;
                assert!(!answered.ends, "{state:?}: {answered:?}");
                assert!(
                    matches!(&answered.answer, Answer::Refused(refusal)
                        if refusal.code == code && refusal.kind == RefusalKind::Unavailable),
                    "{state:?}, {request:?}: {answered:?}"
                );
            }
            let display = a_display(&format!(":{number}"));
            let answered = server.asked(Request::Display(display.clone())).await;
            assert!(
                !answered.ends && answered.answer == Answer::Displayed,
                "{state:?}: {answered:?}"
            );
            assert_eq!(server.announced(), Some(display), "{state:?}");
            assert_eq!(server.watched.borrow().shell, None, "{state:?}");
        }
    }

    /// A watch waits the shorter of the wait it names and
    /// `ipc_watch_wait_ms`, and not at all for a state it has not seen.
    ///
    /// **On a paused clock**, each wait measured, once the interface's state
    /// was taken in at revision 1: a watch that has seen nothing, or names
    /// another revision than the state's — a larger one as much as a smaller
    /// — is answered the state at once; one that names the state's revision
    /// waits what it names when that is within the bound, no time for a wait
    /// of none, and the bound itself for every wait past it, and is then
    /// answered the revision alone.
    #[tokio::test(start_paused = true)]
    async fn a_watch_waits_the_shorter_of_its_wait_and_the_bound() {
        let server = a_server(nc_channel::Channel::Staging);
        let still = Moving::new("a");
        let most = nc_catalog::DAEMON.ipc_watch_wait_ms;
        let state = Value::object([("shell", shell("a"))]);
        for (seen, wait_ms, waited, state) in [
            (None, most, 0, Some(state.clone())),
            (None, 0, 0, Some(state.clone())),
            (Some(7), u64::MAX, 0, Some(state.clone())),
            (Some(0), most, 0, Some(state.clone())),
            (Some(super::Watched::LAST), most, 0, Some(state)),
            (Some(1), 0, 0, None),
            (Some(1), 250, 250, None),
            (Some(1), most - 1, most - 1, None),
            (Some(1), most, most, None),
            (Some(1), most + 1, most, None),
            (Some(1), u64::MAX, most, None),
        ] {
            let before = tokio::time::Instant::now();
            let answer = server.watch(&still, seen, wait_ms).await;
            assert_eq!(
                before.elapsed(),
                Duration::from_millis(waited),
                "{seen:?} waiting {wait_ms}"
            );
            assert_eq!(changed(answer), (1, state), "{seen:?} waiting {wait_ms}");
        }
    }

    /// A move of the interface's state or a display answers every watch that
    /// waits, and one that says what is held already answers none.
    ///
    /// **On a paused clock**: two watches waiting on revision 1 are both
    /// answered at the moment the kernel tells its state moved, with revision
    /// 2 and that state; the kernel telling the revision taken in again moves
    /// nothing, so a watch naming 2 waits its whole wait and is answered the
    /// revision alone; a display moves it to 3, the state holding both; the
    /// same display again moves nothing; and the kernel's next move moves it
    /// to 4. **A watch hears every move the kernel tells while it waits**: one
    /// told the revision taken in already, and then a move, is answered at the
    /// second, at revision 5.
    #[tokio::test(start_paused = true)]
    async fn a_move_of_the_interface_or_a_display_answers_every_watch_and_what_is_held_none() {
        let server = a_server(nc_channel::Channel::Staging);
        let moving = Moving::new("a");
        let most = nc_catalog::DAEMON.ipc_watch_wait_ms;
        assert_eq!(changed(server.watch(&moving, None, 0).await).0, 1);
        let before = tokio::time::Instant::now();
        let (first, second, ()) = tokio::join!(
            server.watch(&moving, Some(1), most),
            server.watch(&moving, Some(1), u64::MAX),
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                moving.moved(1, "b");
            }
        );
        assert_eq!(before.elapsed(), Duration::from_millis(100));
        let moved = Value::object([("shell", shell("b"))]);
        assert_eq!(changed(first), (2, Some(moved.clone())));
        assert_eq!(changed(second), (2, Some(moved)));

        moving.moved(1, "b");
        let before = tokio::time::Instant::now();
        assert_eq!(
            changed(server.watch(&moving, Some(2), 300).await),
            (2, None)
        );
        assert_eq!(before.elapsed(), Duration::from_millis(300));

        let before = tokio::time::Instant::now();
        let (watched, ()) = tokio::join!(server.watch(&moving, Some(2), most), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            server.display(a_display(":1"));
        });
        assert_eq!(before.elapsed(), Duration::from_millis(50));
        let both = |draft: &str| {
            Value::object([
                ("display", Value::object([("DISPLAY", Value::text(":1"))])),
                ("shell", shell(draft)),
            ])
        };
        assert_eq!(changed(watched), (3, Some(both("b"))));
        server.display(a_display(":1"));
        assert_eq!(changed(server.watch(&moving, Some(3), 0).await), (3, None));
        moving.moved(2, "c");
        assert_eq!(
            changed(server.watch(&moving, Some(3), most).await),
            (4, Some(both("c")))
        );
        let before = tokio::time::Instant::now();
        let (watched, ()) = tokio::join!(server.watch(&moving, Some(4), most), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            moving.moved(2, "c");
            tokio::time::sleep(Duration::from_millis(50)).await;
            moving.moved(3, "d");
        });
        assert_eq!(before.elapsed(), Duration::from_millis(100));
        assert_eq!(changed(watched), (5, Some(both("d"))));
        assert_eq!(server.announced(), Some(a_display(":1")));
    }

    /// A state of the kernel's read before a newer one was taken in is never
    /// taken in after it.
    ///
    /// **Two watches racing a move** (`NOCTUA.md §11.1`: a state newer than
    /// the revision a watch names answers it): the kernel at revision 3 is
    /// taken in by one watch, and then a slower watch's read of revision 2,
    /// made before the move, reaches the channel. It moves nothing — the
    /// state held and the revision watched are the newer one's — and a watch
    /// naming the revision watched waits it out, answered the revision alone.
    #[tokio::test(start_paused = true)]
    async fn a_state_read_before_a_newer_one_was_taken_in_is_never_taken_in_after_it() {
        let server = a_server(nc_channel::Channel::Staging);
        let moving = Moving::new("a");
        moving.moved(3, "c");
        let fresh = Value::object([("shell", shell("c"))]);
        assert_eq!(
            changed(server.watch(&moving, None, 0).await),
            (1, Some(fresh))
        );
        server.took_in((2, shell("b")));
        assert_eq!(
            server.watched.borrow().shell,
            Some((3, shell("c"))),
            "an older state was taken in"
        );
        assert_eq!(changed(server.watch(&moving, Some(1), 0).await), (1, None));
    }

    /// A watch that was waiting when a stop began is refused as stopping, at
    /// that moment.
    #[tokio::test(start_paused = true)]
    async fn a_watch_waiting_when_a_stop_begins_is_refused_as_stopping() {
        let server = a_server(nc_channel::Channel::Staging);
        let still = Moving::new("a");
        let before = tokio::time::Instant::now();
        let (answer, ()) = tokio::join!(server.watch(&still, Some(1), u64::MAX), async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            server.stop();
        });
        assert_eq!(before.elapsed(), Duration::from_millis(100));
        match answer {
            Answer::Refused(refusal) => {
                assert_eq!(
                    (refusal.code, refusal.kind),
                    (ErrorCode::DaemonStopping, RefusalKind::Unavailable)
                );
                assert!(refusal.sentence.contains("watch"), "{refusal:?}");
            }
            other => panic!("the watch was answered with {other:?}"),
        }
    }

    /// A revision never passes the largest whole number the wire writes: the
    /// one after it is zero, which is another one.
    #[test]
    fn a_revision_turns_to_zero_after_the_wires_largest_integer() {
        let last = u64::try_from(i64::MAX).expect("a count");
        assert_eq!(super::Watched::LAST, last);
        let mut watched = super::Watched::default();
        watched.moved();
        assert_eq!(watched.revision, 1);
        watched.revision = last - 1;
        watched.moved();
        assert_eq!(watched.revision, last);
        watched.moved();
        assert_eq!(watched.revision, 0);
        watched.revision = u64::MAX;
        watched.moved();
        assert_eq!(watched.revision, 0, "a revision past the last was kept");
    }

    /// The interface's state the kernel holds, as a watch answers it.
    fn kernels(serving: &Serving) -> Value {
        serving.kernel.shell().1
    }

    /// A person's connections watch, note and announce through a daemon whose
    /// kernel runs, and a revision crosses the wire as it is at its largest.
    ///
    /// **Through connections the server serves**: a first watch is answered
    /// the kernel's state; a note told on a second connection is applied by
    /// the kernel — its draft is the state's — and answers the watch waiting
    /// on the first with it; a display told then is in the next answer
    /// beside it. With the state at the last revision the wire writes, a watch
    /// reads that revision whole, and the note told after it is answered at
    /// revision zero to a watch naming the last — another revision, though a
    /// smaller one.
    #[tokio::test]
    async fn a_persons_connections_watch_note_and_announce_through_the_daemon() {
        let serving = Serving::new().await;
        let server = &serving.server;
        let mut watcher = a_client_of(server).await;
        let mut teller = a_client_of(server).await;
        let most = nc_catalog::DAEMON.ipc_watch_wait_ms;
        let watch = |seen: Option<u64>| Request::Watch {
            seen,
            wait_ms: most,
        };
        let first = over(&mut watcher, watch(None)).await.expect("answered");
        assert_eq!(
            changed(first),
            (1, Some(Value::object([("shell", kernels(&serving))])))
        );
        let (heard, noted) = tokio::join!(
            over(&mut watcher, watch(Some(1))),
            over(&mut teller, Request::Note(a_note("a")))
        );
        assert_eq!(noted.expect("answered"), Answer::Noted);
        let drafted = ("shell", kernels(&serving));
        assert!(
            drafted.1.to_canonical().contains(r#""draft":"a""#),
            "the note was not applied: {drafted:?}"
        );
        assert_eq!(
            changed(heard.expect("answered")),
            (2, Some(Value::object([drafted.clone()])))
        );
        assert_eq!(
            over(&mut teller, Request::Display(a_display(":7")))
                .await
                .expect("answered"),
            Answer::Displayed
        );
        let display = ("display", Value::object([("DISPLAY", Value::text(":7"))]));
        assert_eq!(
            changed(over(&mut watcher, watch(Some(2))).await.expect("answered")),
            (3, Some(Value::object([display.clone(), drafted.clone()])))
        );
        let last = super::Watched::LAST;
        server
            .watched
            .send_modify(|watched| watched.revision = last);
        assert_eq!(
            changed(over(&mut watcher, watch(Some(3))).await.expect("answered")),
            (last, Some(Value::object([display.clone(), drafted])))
        );
        let (heard, noted) = tokio::join!(
            over(&mut watcher, watch(Some(last))),
            over(&mut teller, Request::Note(a_note("b")))
        );
        assert_eq!(noted.expect("answered"), Answer::Noted);
        assert_eq!(
            changed(heard.expect("answered")),
            (
                0,
                Some(Value::object([display, ("shell", kernels(&serving))]))
            )
        );
        drop((watcher, teller));
        let bound = Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms);
        assert_eq!(server.closed(tokio::time::Instant::now() + bound).await, 0);
        serving.closed().await;
    }

    /// A note the kernel refuses is answered with its refusal, the connection
    /// going on, and moves nothing; and a note is journaled nowhere.
    ///
    /// **Through a connection the server serves**: a note naming no fact is
    /// `[NC-VAL-0002]`, invalid, and one scrolling a page the state does not
    /// hold `[NC-KERN-0027]`, not found; the state and its revision are as
    /// they were, the next note on the same connection is applied, and the
    /// journal holds no entry of any of them.
    #[tokio::test]
    async fn a_note_the_kernel_refuses_is_answered_with_its_refusal_and_none_is_journaled() {
        let serving = Serving::new().await;
        let mut teller = a_client_of(&serving.server).await;
        let journaled = serving
            .kernel
            .journal_for_tests()
            .await
            .expect("the journal reads")
            .len();
        let before = serving.kernel.shell();
        for (detail, code, kind) in [
            (
                Value::object([("zoom", Value::object([("to", Value::Integer(2))]))]),
                ErrorCode::ValidationArgumentsInvalid,
                RefusalKind::Invalid,
            ),
            (
                Value::object([(
                    "scroll",
                    Value::object([("page", Value::Integer(9)), ("to", Value::Integer(1))]),
                )]),
                ErrorCode::KernelShellUnknown,
                RefusalKind::NotFound,
            ),
        ] {
            let note = nc_ipc::Note::new(detail).expect("a note");
            match over(&mut teller, Request::Note(note))
                .await
                .expect("answered")
            {
                Answer::Refused(refusal) => {
                    assert_eq!((refusal.code, refusal.kind), (code, kind), "{refusal:?}");
                }
                other => panic!("a note was answered with {other:?}"),
            }
        }
        assert_eq!(
            serving.kernel.shell(),
            before,
            "a refused note moved the state"
        );
        assert_eq!(
            over(&mut teller, Request::Note(a_note("c")))
                .await
                .expect("answered"),
            Answer::Noted
        );
        assert_eq!(serving.kernel.shell().0, before.0 + 1);
        assert_eq!(
            serving
                .kernel
                .journal_for_tests()
                .await
                .expect("the journal reads")
                .len(),
            journaled,
            "a note was journaled"
        );
        drop(teller);
        serving.closed().await;
    }

    /// The Leader the daemon holds is said held, each press it hears is a tap
    /// the person made, and it is given back when the daemon stops.
    ///
    /// **With a registration a test grants**: once asked, the watched state
    /// and the status say `{"held":true}`, at a new revision; a press is
    /// `leader.tap` dispatched as the profile's person — the window it shows
    /// moves the kernel's state — and journaled as theirs; and giving the
    /// Leader back drops the registration, after which no press is heard.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_leader_the_daemon_holds_is_said_held_and_each_press_is_the_persons_tap() {
        let serving = Serving::new().await;
        let server = &serving.server;
        let pressed: Pressed = Arc::default();
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut watching = server.watched.subscribe();
        let held = {
            let (pressed, dropped) = (Arc::clone(&pressed), Arc::clone(&dropped));
            crate::leader::listening(
                Arc::clone(server),
                &tokio::runtime::Handle::current(),
                nc_channel::Channel::Staging.leader(),
                move |_leader, on_press, _on_lost| {
                    *pressed.lock().expect("nothing panics holding it") = Some(on_press);
                    Ok(nc_ipc::leader::Listening::Held(DroppedOnce(Arc::clone(
                        &dropped,
                    ))))
                },
            )
        };
        let said = Value::object([("held", Value::Bool(true))]);
        tokio::time::timeout(
            crate::testing::PATIENCE,
            watching.wait_for(|watched| watched.leader.as_ref() == Some(&said)),
        )
        .await
        .expect("said in time")
        .expect("held");
        assert!(
            server
                .status()
                .to_canonical()
                .contains(r#""leader":{"held":true}"#),
            "{}",
            server.status().to_canonical()
        );
        let mut moves = serving.kernel.shell_moves();
        let _ = moves.borrow_and_update();
        (pressed
            .lock()
            .expect("nothing panics holding it")
            .as_ref()
            .expect("handed a press"))();
        tokio::time::timeout(crate::testing::PATIENCE, moves.changed())
            .await
            .expect("tapped in time")
            .expect("told");
        let person = server.named.person.id().as_str().to_owned();
        assert_eq!(serving.decided("leader.tap").await, [(person, Vec::new())]);
        assert!(
            !dropped.load(Ordering::SeqCst),
            "the registration was given back before the daemon gave it"
        );
        tokio::task::spawn_blocking(move || held.given_back())
            .await
            .expect("given back");
        assert!(
            dropped.load(Ordering::SeqCst),
            "the registration was not given back"
        );
        serving.closed().await;
    }

    /// Where a test keeps the press a registration was handed.
    type Pressed = Arc<std::sync::Mutex<Option<Box<dyn Fn() + Send>>>>;

    /// A registration a test grants, which says when it is dropped.
    struct DroppedOnce(Arc<std::sync::atomic::AtomicBool>);

    impl Drop for DroppedOnce {
        fn drop(&mut self) {
            assert!(!self.0.swap(true, Ordering::SeqCst), "dropped twice");
        }
    }

    /// The refusal `refused` is told as.
    fn refusal_of(refused: &nc_platform::leader::Refused) -> Refusal {
        nc_ipc::leader::refusal(refused)
    }

    /// What the watched state holds of a Leader refused as `refusal`.
    fn refused_as(refusal: &Refusal) -> Value {
        Value::object([(
            "refused",
            Value::object([
                ("code", Value::text(refusal.code.code())),
                ("sentence", Value::text(refusal.sentence.as_str())),
            ]),
        )])
    }

    /// A Leader the daemon was refused, or lost, is held in the watched state
    /// and the status, said once.
    ///
    /// **With a registration a test refuses, and one it grants and loses**: a
    /// refusal as taken is held as `refused` with `[NC-CHAN-0020]` and the
    /// sentence, at a new revision, and the same refusal again moves nothing;
    /// a registration granted is held, and its loss, the server gone away, is
    /// held with `[NC-CHAN-0022]` in its place and shown in the status.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_leader_refused_or_lost_is_held_in_the_watched_state_and_the_status() {
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        let taken = refusal_of(&nc_platform::leader::Refused::Taken);
        let told = taken.clone();
        crate::leader::listening(
            Arc::clone(&server),
            &tokio::runtime::Handle::current(),
            nc_channel::Channel::Staging.leader(),
            move |_leader, _on_press, _on_lost| {
                Err::<nc_ipc::leader::Listening<()>, _>(told.clone())
            },
        )
        .given_back();
        let held = server.watched.borrow().clone();
        assert_eq!(held.revision, 1);
        assert_eq!(held.leader, Some(refused_as(&taken)));
        assert_eq!(taken.code, ErrorCode::ChannelLeaderTaken);
        server.leader_asked(Err(&taken));
        assert_eq!(server.watched.borrow().revision, 1, "what is held moved");
        let lost = Arc::new(std::sync::Mutex::new(None));
        let losing = Arc::clone(&lost);
        let held = crate::leader::listening(
            Arc::clone(&server),
            &tokio::runtime::Handle::current(),
            nc_channel::Channel::Staging.leader(),
            move |_leader, _on_press, on_lost| {
                *losing.lock().expect("nothing panics holding it") = Some(on_lost);
                Ok(nc_ipc::leader::Listening::Held(()))
            },
        );
        let mut watching = server.watched.subscribe();
        tokio::time::timeout(
            crate::testing::PATIENCE,
            watching.wait_for(|watched| watched.revision == 2),
        )
        .await
        .expect("held in time")
        .expect("held");
        let gone = nc_platform::leader::Refused::Failed("the X server went away".to_owned());
        let lost_as = refusal_of(&gone);
        (lost
            .lock()
            .expect("nothing panics holding it")
            .take()
            .expect("handed a loss"))(gone);
        assert_eq!(server.watched.borrow().leader, Some(refused_as(&lost_as)));
        assert_eq!(lost_as.code, ErrorCode::ChannelLeaderUnregistered);
        assert!(
            server
                .status()
                .to_canonical()
                .contains(&refused_as(&lost_as).to_canonical()),
            "{}",
            server.status().to_canonical()
        );
        tokio::task::spawn_blocking(move || held.given_back())
            .await
            .expect("given back");
    }

    /// What a test's registrations say: each chord asked for, with what it
    /// was asked beside, and each given back, in the order it happened.
    type Said = Arc<std::sync::Mutex<Vec<String>>>;

    /// A registration a test grants, which says when it is given back.
    struct Granted(String, Said);

    impl Drop for Granted {
        fn drop(&mut self) {
            self.1
                .lock()
                .expect("nothing panics holding it")
                .push(format!("back {}", self.0));
        }
    }

    /// The press a registration is handed.
    type Press = Box<dyn Fn() + Send>;

    /// The loss a registration is handed.
    type Loss = Box<dyn FnOnce(nc_platform::leader::Refused) + Send>;

    /// What a test's registration answers.
    type Asked = Result<nc_ipc::leader::Listening<Granted>, Refusal>;

    /// Everything said since the last time this was asked.
    fn said(said: &Said) -> Vec<String> {
        std::mem::take(&mut *said.lock().expect("nothing panics holding it"))
    }

    /// A registration that grants every Leader but `taken`, which it refuses
    /// as another program's, saying each chord asked for and `beside()` then.
    fn granting(
        said: &Said,
        taken: &[&str],
        beside: impl Fn() -> String + Send + 'static,
    ) -> impl FnMut(nc_schema::keys::Leader, Press, Loss) -> Asked + Send + 'static {
        let said = Arc::clone(said);
        let taken: Vec<String> = taken.iter().map(|chord| (*chord).to_owned()).collect();
        move |leader, _on_press, _on_lost| {
            let chord = leader.written();
            said.lock()
                .expect("nothing panics holding it")
                .push(format!("asked {chord}{}", beside()));
            if taken.contains(&chord) {
                Err(refusal_of(&nc_platform::leader::Refused::Taken))
            } else {
                Ok(nc_ipc::leader::Listening::Held(Granted(
                    chord,
                    Arc::clone(&said),
                )))
            }
        }
    }

    /// The Leader the grammar writes as `chord`.
    fn leader(chord: &str) -> nc_schema::keys::Leader {
        nc_schema::keys::Leader::of(nc_schema::keys::Chord::parse(chord).expect("a chord"))
            .expect("a Leader")
    }

    /// A Leader a setting changes is asked for beside the one held, and then
    /// kept in its place or given back.
    ///
    /// **With registrations a test grants and refuses**: the Leader held is
    /// granted again asking nobody; another is asked for with the first still
    /// held, and given back when the setting drops it; one another program
    /// holds is refused by its code, the first held still; one kept takes the
    /// first's place, which is given back only then, and is the Leader held
    /// from then on; giving the Leader back gives back what is held, and
    /// nothing is asked of a thread that ended.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_leader_a_setting_changes_is_asked_for_beside_the_one_held_then_kept_or_given_back() {
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        let heard: Said = Arc::default();
        let held = crate::leader::listening(
            Arc::clone(&server),
            &tokio::runtime::Handle::current(),
            leader("Control+Alt+Space"),
            granting(&heard, &["Alt+KeyT"], String::new),
        );
        let asking = held.asking();
        let tried = |chord: &'static str| {
            let asking = asking.clone();
            async move {
                tokio::task::spawn_blocking(move || asking.tried(leader(chord)))
                    .await
                    .expect("asked")
            }
        };
        assert_eq!(tried("Control+Alt+Space").await, Ok(()));
        assert_eq!(
            said(&heard),
            ["asked Control+Alt+Space"],
            "the Leader held was asked for twice"
        );
        assert_eq!(tried("Alt+KeyB").await, Ok(()));
        assert_eq!(
            said(&heard),
            ["asked Alt+KeyB"],
            "the Leader held was let go for one tried"
        );
        asking.dropped();
        let taken = tried("Alt+KeyT").await.expect_err("another program's");
        assert_eq!(taken.code, ErrorCode::ChannelLeaderTaken);
        assert_eq!(said(&heard), ["back Alt+KeyB", "asked Alt+KeyT"]);
        assert_eq!(tried("Alt+KeyC").await, Ok(()));
        asking.kept();
        assert_eq!(
            tried("Alt+KeyC").await,
            Ok(()),
            "the Leader kept is the one held"
        );
        assert_eq!(said(&heard), ["asked Alt+KeyC", "back Control+Alt+Space"]);
        // One tried and never settled is given back as the next is tried.
        assert_eq!(tried("Alt+KeyD").await, Ok(()));
        assert_eq!(tried("Alt+KeyE").await, Ok(()));
        assert_eq!(
            said(&heard),
            ["asked Alt+KeyD", "back Alt+KeyD", "asked Alt+KeyE"]
        );
        tokio::task::spawn_blocking(move || held.given_back())
            .await
            .expect("given back");
        let mut back = said(&heard);
        back.sort();
        assert_eq!(back, ["back Alt+KeyC", "back Alt+KeyE"]);
        let ended = tried("Alt+KeyF").await.expect_err("the thread ended");
        assert_eq!(ended.code, ErrorCode::ChannelLeaderUnregistered);
        assert_eq!(said(&heard), [] as [&str; 0]);
    }

    /// A Leader refused when the daemon started is asked for again by a
    /// setting, and one a setting keeps is said held.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_leader_refused_at_the_start_is_asked_for_again_and_one_kept_is_said_held() {
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        let heard: Said = Arc::default();
        let held = crate::leader::listening(
            Arc::clone(&server),
            &tokio::runtime::Handle::current(),
            leader("Alt+KeyT"),
            granting(&heard, &["Alt+KeyT"], String::new),
        );
        let asking = held.asking();
        let taken = refusal_of(&nc_platform::leader::Refused::Taken);
        let again = {
            let asking = asking.clone();
            tokio::task::spawn_blocking(move || asking.tried(leader("Alt+KeyT")))
                .await
                .expect("asked")
        };
        assert_eq!(again, Err(taken.clone()));
        assert_eq!(said(&heard), ["asked Alt+KeyT", "asked Alt+KeyT"]);
        assert_eq!(server.watched.borrow().leader, Some(refused_as(&taken)));
        let granted = {
            let asking = asking.clone();
            tokio::task::spawn_blocking(move || asking.tried(leader("Alt+KeyB")))
                .await
                .expect("asked")
        };
        assert_eq!(granted, Ok(()));
        assert_eq!(
            server.watched.borrow().leader,
            Some(refused_as(&taken)),
            "a Leader only tried was said held"
        );
        asking.kept();
        let mut watching = server.watched.subscribe();
        let held_now = Value::object([("held", Value::Bool(true))]);
        tokio::time::timeout(
            crate::testing::PATIENCE,
            watching.wait_for(|watched| watched.leader.as_ref() == Some(&held_now)),
        )
        .await
        .expect("said in time")
        .expect("held");
        tokio::task::spawn_blocking(move || held.given_back())
            .await
            .expect("given back");
        assert_eq!(said(&heard), ["asked Alt+KeyB", "back Alt+KeyB"]);
    }

    /// A count of the writes of the keybinding configuration turns to zero
    /// past the last number the wire writes, as a revision does, and is
    /// answered only once there is one.
    #[test]
    fn the_count_of_the_keys_written_is_answered_once_there_is_one_and_turns_as_a_revision() {
        let server = a_server(nc_channel::Channel::Staging);
        assert!(
            !server
                .watched
                .borrow()
                .state()
                .to_canonical()
                .contains("keys")
        );
        server.keys_written();
        server.keys_written();
        let watched = server.watched.borrow().clone();
        assert_eq!((watched.revision, watched.keys), (2, 2));
        assert_eq!(watched.state().to_canonical(), r#"{"keys":2}"#);
        assert_eq!(
            super::Watched::next(super::Watched::LAST - 1),
            super::Watched::LAST
        );
        assert_eq!(super::Watched::next(super::Watched::LAST), 0);
        assert_eq!(super::Watched::next(u64::MAX), 0);
    }

    /// The install a [`Serving`] was opened over, as its binaries read it.
    fn found_of(serving: &Serving) -> nc_channel::found::Found {
        nc_channel::found::Found {
            install: serving.installed.install.clone(),
            user: serving.installed.user.clone(),
            layout: serving.installed.layout.clone(),
        }
    }

    /// What `command` with `args` came to, dispatched by the profile's
    /// person on the kernel.
    async fn dispatched(
        serving: &Serving,
        command: &str,
        args: &str,
    ) -> Result<String, nc_kernel::Refusal> {
        let person = nc_kernel::Caller::new(serving.server.named.person.clone());
        serving.kernel.dispatch(&person, command, args).await
    }

    /// A shortcut setting writes the install's keybinding configuration
    /// through its judge, and each write is counted in the watched state.
    ///
    /// **Where the daemon asks the system for nothing**, as a test's install
    /// does: each of the four settings is read as the keys in force, set, and
    /// read back; the file holds every choice, as its reader reads it, and
    /// the count of writes and the watched revision move by one for each set
    /// and for no read; and a choice the judge refuses — a Leader holding the
    /// chord modifier — is refused with `[NC-CHAN-0023]`, the file and the
    /// count left as they were.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_shortcut_setting_writes_the_keybinding_configuration_and_each_write_is_counted() {
        let serving = Serving::new().await;
        let found = found_of(&serving);
        let file = found.layout.keybindings.clone();
        serving
            .kernel
            .keep_shortcuts(Arc::new(crate::environment::Keybindings::new(
                found.clone(),
                Arc::downgrade(&serving.server),
            )));
        let default = found.install.channel.leader().written();
        let counted = || {
            let watched = serving.server.watched.borrow();
            (watched.revision, watched.keys)
        };
        let (revision, _) = counted();
        assert_eq!(
            dispatched(&serving, "settings.leader.read", "{}")
                .await
                .as_deref(),
            Ok(format!(r#"{{"leader":"{default}"}}"#).as_str())
        );
        assert_eq!(counted(), (revision, 0), "a read was counted as a write");
        let sets = [
            ("settings.leader", r#"{"leader":"Control+Alt+KeyK"}"#),
            ("settings.chord", r#"{"chord":"Super"}"#),
            (
                "settings.menu-keys",
                r#"{"keys":[{"command":"page.help","key":"KeyH"}]}"#,
            ),
            (
                "settings.chords",
                r#"{"chords":[{"chord":true,"command":"page.close","key":"KeyQ"}]}"#,
            ),
        ];
        for (at, (setting, to)) in (1..).zip(sets) {
            assert_eq!(
                dispatched(&serving, &format!("{setting}.set"), to)
                    .await
                    .as_deref(),
                Ok(to),
                "{setting}"
            );
            assert_eq!(counted(), (revision + at, at), "{setting}");
            assert_eq!(
                dispatched(&serving, &format!("{setting}.read"), "{}")
                    .await
                    .as_deref(),
                Ok(to),
                "{setting}"
            );
            assert_eq!(
                counted(),
                (revision + at, at),
                "{setting}: a read was counted"
            );
        }
        let written = std::fs::read_to_string(&file).expect("the file");
        assert!(
            written.ends_with(r#""chord":"Super","format":1,"leader":"Control+Alt+KeyK"}"#)
                && written.contains(r#""page.close":["Leader+Chord+KeyQ"]"#)
                && !written.contains("page.help"),
            "every choice, and nothing the catalog binds the same: {written}"
        );
        let effective = nc_channel::keys::effective(&found);
        assert!(effective.refused.is_empty(), "{:?}", effective.refused);
        assert_eq!(effective.leader, leader("Control+Alt+KeyK"));
        let refused = dispatched(
            &serving,
            "settings.leader.set",
            r#"{"leader":"Alt+Super+KeyK"}"#,
        )
        .await
        .expect_err("it holds its chord modifier");
        assert_eq!(
            (refused.kind(), refused.code()),
            (
                nc_kernel::RefusalKind::Conflict,
                ErrorCode::ChannelShortcutLeaderCollides
            ),
            "{refused}"
        );
        assert_eq!(std::fs::read_to_string(&file).expect("the file"), written);
        assert_eq!(counted(), (revision + 4, 4), "a refused set was counted");
        serving.closed().await;
    }

    /// Where the daemon asks the system for the Leader, a Leader a setting
    /// changes is granted before the file is written, the old one let go only
    /// after, and one the system refuses refuses the setting.
    ///
    /// **With registrations a test grants and refuses, each saying what the
    /// file held when it was asked**: a new Leader is asked for over the file
    /// as it stood, and the Leader held is given back once the file holds the
    /// choice; a setting that leaves the Leader as it is asks for nothing; a
    /// Leader another program holds refuses the setting with
    /// `[NC-CHAN-0026]`, naming `[NC-CHAN-0020]`, the file and the count of
    /// writes left as they were and the Leader held still held; and a write
    /// the disk refuses gives the Leader just granted back, the one held
    /// still held.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_leader_a_setting_changes_is_granted_before_it_is_written_and_refused_where_the_system_refuses()
     {
        let serving = Serving::new().await;
        let found = found_of(&serving);
        let file = found.layout.keybindings.clone();
        serving
            .kernel
            .keep_shortcuts(Arc::new(crate::environment::Keybindings::new(
                found.clone(),
                Arc::downgrade(&serving.server),
            )));
        let default = found.install.channel.leader();
        let heard: Said = Arc::default();
        let read = file.clone();
        let held = crate::leader::listening(
            Arc::clone(&serving.server),
            &tokio::runtime::Handle::current(),
            default,
            granting(&heard, &["Alt+KeyT"], move || {
                format!(
                    " over {}",
                    std::fs::read_to_string(&read).unwrap_or_default()
                )
            }),
        );
        serving.server.leader_held_by(held.asking());
        let default = default.written();
        let keys = || serving.server.watched.borrow().keys;
        // Set, a new Leader is asked for before the file is written.
        let to = r#"{"leader":"Control+Alt+KeyK"}"#;
        assert_eq!(
            dispatched(&serving, "settings.leader.set", to)
                .await
                .as_deref(),
            Ok(to)
        );
        let first = r#"{"format":1,"leader":"Control+Alt+KeyK"}"#;
        assert_eq!(std::fs::read_to_string(&file).expect("the file"), first);
        // The chord modifier alone leaves the Leader as it is.
        let chord = r#"{"chord":"Super"}"#;
        assert_eq!(
            dispatched(&serving, "settings.chord.set", chord)
                .await
                .as_deref(),
            Ok(chord)
        );
        let second = r#"{"chord":"Super","format":1,"leader":"Control+Alt+KeyK"}"#;
        assert_eq!(std::fs::read_to_string(&file).expect("the file"), second);
        assert_eq!(
            said(&heard),
            [
                format!("asked {default} over "),
                "asked Control+Alt+KeyK over ".to_owned(),
                format!("back {default}"),
            ]
        );
        assert_eq!(keys(), 2);
        // Another program's chord refuses the setting.
        let refused = dispatched(&serving, "settings.leader.set", r#"{"leader":"Alt+KeyT"}"#)
            .await
            .expect_err("the system refuses it");
        assert_eq!(
            (refused.kind(), refused.code()),
            (
                nc_kernel::RefusalKind::Conflict,
                ErrorCode::ChannelShortcutLeaderUndelivered
            ),
            "{refused}"
        );
        assert!(
            refused.sentence().contains("`Alt+KeyT`")
                && refused
                    .sentence()
                    .contains(&format!("[{}]", ErrorCode::ChannelLeaderTaken.code())),
            "{refused}"
        );
        assert_eq!(std::fs::read_to_string(&file).expect("the file"), second);
        assert_eq!(keys(), 2, "a refused set was counted");
        assert_eq!(said(&heard), [format!("asked Alt+KeyT over {second}")]);
        // A write the disk refuses gives the Leader just granted back.
        let beside = std::path::PathBuf::from(format!("{}.new", file.display()));
        drop(nc_platform::create_private(&beside).expect("a file beside it"));
        nc_platform::testing::open_to_another_user(&beside).expect("opened to another user");
        let unsaved = dispatched(&serving, "settings.leader.set", r#"{"leader":"Alt+KeyB"}"#)
            .await
            .expect_err("the write is refused");
        assert_eq!(
            unsaved.code(),
            ErrorCode::ChannelShortcutsUnsaved,
            "{unsaved}"
        );
        assert_eq!(std::fs::read_to_string(&file).expect("the file"), second);
        assert_eq!(keys(), 2, "a write that failed was counted");
        nc_platform::testing::make_private(&beside).expect("private again");
        tokio::task::spawn_blocking(move || held.given_back())
            .await
            .expect("given back");
        assert_eq!(
            said(&heard),
            [
                format!("asked Alt+KeyB over {second}"),
                "back Alt+KeyB".to_owned(),
                "back Control+Alt+KeyK".to_owned(),
            ]
        );
        serving.closed().await;
    }

    /// The variable that tells this test binary, run as a child, to say what
    /// a refused Leader is logged as.
    const TELLS: &str = "NC_DAEMON_SERVER_LEADER_TELLS";

    /// A Leader the daemon was refused, or lost, is logged as
    /// `daemon.leader_refused` with its code, each time.
    ///
    /// **Through the test binary run as a child**, whose log, told of no
    /// store, goes to its standard error: a refusal as taken, the same again
    /// and a loss are three lines, with `[NC-CHAN-0020]` twice and then
    /// `[NC-CHAN-0022]`.
    #[test]
    fn a_leader_refused_or_lost_is_logged_by_its_code() {
        if std::env::var_os(TELLS).is_some() {
            let world = a_world();
            let server = Server::new(
                Named {
                    pid: 4242,
                    instance: [9; 16],
                    channel: nc_channel::Channel::Staging,
                    realm: world.realm().as_str().to_owned(),
                    person: world.person().clone(),
                    since: 1_767_225_600_000,
                },
                nc_ipc::Token::fresh().expect("a token"),
                no_desktop(),
                Arc::new(std::sync::Mutex::new(nc_log::Log::to_standard_error(
                    crate::environment::origin(),
                ))),
            );
            let taken = refusal_of(&nc_platform::leader::Refused::Taken);
            server.leader_asked(Err(&taken));
            server.leader_asked(Err(&taken));
            server.leader_asked(Err(&refusal_of(&nc_platform::leader::Refused::Failed(
                "the X server went away".to_owned(),
            ))));
            return;
        }
        let this = std::env::current_exe().expect("this test binary");
        let installed = crate::testing::Installed::new(&this);
        let mut child = installed.spawn_as(
            &this,
            &[
                "--exact",
                "server::tests::a_leader_refused_or_lost_is_logged_by_its_code",
                "--nocapture",
                "--test-threads=1",
            ],
            &[(TELLS, std::ffi::OsStr::new("1"))],
        );
        let ended = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("a runtime")
            .block_on(child.ended());
        assert_eq!(ended, Some(0), "{}", installed.standard_error());
        let codes: Vec<String> = installed
            .standard_error()
            .lines()
            .filter_map(|line| {
                let mut fields = nc_canon::parse(line.trim_start_matches("nc-log: "))
                    .ok()?
                    .into_object()?;
                (fields.remove("_msg")? == Value::text("daemon.leader_refused"))
                    .then(|| fields.remove("code")?.into_text())?
            })
            .collect();
        assert_eq!(
            codes,
            [
                ErrorCode::ChannelLeaderTaken.code(),
                ErrorCode::ChannelLeaderTaken.code(),
                ErrorCode::ChannelLeaderUnregistered.code(),
            ],
            "{}",
            installed.standard_error()
        );
    }

    /// A watcher that went away is found only when its answer is written, and
    /// a stop ends the watch that waits for nobody.
    ///
    /// **Through a connection the server serves**: a client asks a watch that
    /// waits the whole bound and is let go; its connection is still counted
    /// open while the watch waits, and once a stop refuses the watch, the
    /// answer finds nobody and the connection closes.
    #[tokio::test]
    async fn a_watcher_that_went_away_is_found_only_when_its_answer_is_written() {
        let serving = Serving::new().await;
        let server = &serving.server;
        let mut watcher = a_client_of(server).await;
        let most = nc_catalog::DAEMON.ipc_watch_wait_ms;
        let first = over(
            &mut watcher,
            Request::Watch {
                seen: None,
                wait_ms: 0,
            },
        )
        .await
        .expect("answered");
        assert_eq!(changed(first).0, 1);
        let waiting = Request::Watch {
            seen: Some(1),
            wait_ms: most,
        };
        let gone = tokio::time::timeout(Duration::from_millis(200), watcher.ask(&waiting)).await;
        assert!(gone.is_err(), "a watch of an unchanged state was answered");
        drop(watcher);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            server.connections.load(Ordering::SeqCst),
            1,
            "a connection whose watch still waits was closed"
        );
        server.stop();
        let bound = Duration::from_millis(most);
        assert_eq!(server.closed(tokio::time::Instant::now() + bound).await, 0);
        serving.closed().await;
    }

    /// Once MCP is off, a session's next request ends it, whatever it asks.
    ///
    /// **Through a connection the server serves**: while MCP is on, the status
    /// is `NC-DMN-0015` and the session stands, its next request answered;
    /// once its person switches MCP off, the status is `NC-DMN-0014`, the
    /// connection is lost after it, and its place is given back.
    #[tokio::test]
    async fn once_mcp_is_off_a_sessions_next_request_ends_it_whatever_it_asks() {
        let serving = Serving::new().await;
        let server = &serving.server;
        let mut client = a_client_of(server).await;
        let agent = serving.agent;
        assert!(matches!(
            over(&mut client, Request::Session { agent }).await,
            Ok(Answer::Session)
        ));
        let refused = |answered: Result<Answer, nc_ipc::IpcError>, code: ErrorCode| {
            assert!(
                matches!(&answered, Ok(Answer::Refused(refusal)) if refusal.code == code),
                "{code:?}: the status was answered with {answered:?}"
            );
        };
        refused(
            over(&mut client, Request::Status).await,
            ErrorCode::DaemonNotForASession,
        );
        assert!(
            matches!(
                over(&mut client, Request::Tools { locale: Locale::En }).await,
                Ok(Answer::Tools(_))
            ),
            "the session did not stand once the status was refused"
        );
        assert_eq!(server.sessions.load(Ordering::SeqCst), 1);
        serving.switched(false).await;
        refused(
            over(&mut client, Request::Status).await,
            ErrorCode::DaemonMcpOff,
        );
        let next = over(&mut client, Request::Tools { locale: Locale::En }).await;
        assert!(
            matches!(next, Err(nc_ipc::IpcError::Lost(_))),
            "a session went on once MCP was off: {next:?}"
        );
        let bound = Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms);
        assert_eq!(server.closed(tokio::time::Instant::now() + bound).await, 0);
        assert_eq!(server.sessions.load(Ordering::SeqCst), 0);
        drop(client);
        serving.closed().await;
    }

    /// What tells a child of this test binary to serve an install.
    ///
    /// The install beside the path it holds, as the daemon's binary serves the
    /// one beside itself and as the run's own tests tell one.
    const SERVES: &str = "NC_DAEMON_SERVER_SERVES";

    /// A session is logged as it opens and as it ends, with its agent, and
    /// each refusal of it by its code.
    ///
    /// **Over a daemon a child of this test binary serves**, its lines written
    /// to its standard error alone: a session asked for an agent no person
    /// made, then the person's own agent's session, refused the registry and
    /// ended once its person switches MCP off under it. The daemon logs, in
    /// order, `NC-AI-0001` refused, the agent's session opened, `NC-DMN-0015`
    /// and `NC-DMN-0014` refused, and the agent's session ended, and no line
    /// of it names the stranger.
    #[test]
    fn a_session_is_logged_as_it_opens_and_ends_with_its_agent_and_each_refusal_by_its_code() {
        if let Some(executable) = std::env::var_os(SERVES) {
            crate::run::serve(std::path::Path::new(&executable)).expect("the daemon ran");
            return;
        }
        let this = std::env::current_exe().expect("this test binary");
        let installed = crate::testing::Installed::new(&this);
        installed.without_a_central_log();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let mut daemon = installed.spawn_as(
            &this,
            &[
                "--exact",
                "server::tests::a_session_is_logged_as_it_opens_and_ends_with_its_agent_and_each_refusal_by_its_code",
                "--nocapture",
                "--test-threads=1",
            ],
            &[(SERVES, installed.executable.as_os_str())],
        );
        let refused = |answer: &Answer, code: ErrorCode| {
            assert!(
                matches!(answer, Answer::Refused(refusal) if refusal.code == code),
                "{code:?}: {answer:?}"
            );
        };
        let (agent, stranger) = runtime.block_on(async {
            let (mut person, _) = daemon.connected().await;
            let running = format!("\"state\":\"{}\"", State::Running.word());
            let deadline = tokio::time::Instant::now() + crate::testing::PATIENCE;
            loop {
                match asked(&mut person, Request::Status).await {
                    Answer::Status(status) if status.to_canonical().contains(&running) => break,
                    _ => assert!(tokio::time::Instant::now() < deadline, "it never ran"),
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            daemon.switch_mcp(true).await;
            let agent = daemon.agent("a", 100).await;
            let stranger = minted();
            let (mut unknown, _) = daemon.connected().await;
            refused(
                &asked(&mut unknown, Request::Session { agent: stranger }).await,
                ErrorCode::AiNoSuchAgent,
            );
            let mut session = daemon.session(agent).await;
            refused(
                &asked(&mut session, REGISTRY).await,
                ErrorCode::DaemonNotForASession,
            );
            daemon.switch_mcp(false).await;
            refused(
                &asked(&mut session, Request::Tools { locale: Locale::En }).await,
                ErrorCode::DaemonMcpOff,
            );
            drop((unknown, session));
            assert_eq!(asked(&mut person, Request::Stop).await, Answer::Stopping);
            assert_eq!(
                daemon.ended().await,
                Some(0),
                "{}",
                installed.standard_error()
            );
            (agent.to_crockford(), stranger.to_crockford())
        });
        let said = installed.standard_error();
        let sessions: Vec<(String, String)> = said
            .lines()
            .filter_map(|line| {
                let mut fields = nc_canon::parse(line.trim_start_matches("nc-log: "))
                    .ok()?
                    .into_object()?;
                let event = fields.remove("_msg")?.into_text()?;
                let named = fields.remove("agent").or_else(|| fields.remove("code"))?;
                event
                    .starts_with("daemon.session_")
                    .then(|| named.into_text().map(|named| (event, named)))?
            })
            .collect();
        let line = |event: &str, named: &str| (format!("daemon.session_{event}"), named.to_owned());
        assert_eq!(
            sessions,
            [
                line("refused", ErrorCode::AiNoSuchAgent.code()),
                line("opened", &agent),
                line("refused", ErrorCode::DaemonNotForASession.code()),
                line("refused", ErrorCode::DaemonMcpOff.code()),
                line("ended", &agent),
            ],
            "{said}"
        );
        assert!(
            !said.contains(&stranger),
            "a line names the agent no person made: {said}"
        );
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

    /// A command of the registry, its entry in both locales.
    fn a_command() -> nc_kernel::registry::RegisteredCommand {
        use nc_kernel::registry::{CommandEntry, EntryTexts};
        use nc_schema::descriptor::Reversibility;
        nc_kernel::registry::RegisteredCommand {
            name: "journal.verify".to_owned(),
            entry: CommandEntry::new(
                vec![
                    EntryTexts {
                        display_name: "Verify".to_owned(),
                        line: "Check the history.".to_owned(),
                        paragraph: "Checks \"every\" entry.".to_owned(),
                        tags: vec!["verify".to_owned(), "check".to_owned()],
                    },
                    EntryTexts {
                        display_name: "Verificar".to_owned(),
                        line: "Conferir o histórico.".to_owned(),
                        paragraph: "Confere cada registro.".to_owned(),
                        tags: vec!["verificar".to_owned()],
                    },
                ],
                vec!["noctua.history".to_owned()],
                Some('\u{f058}'),
            ),
            args_schema: "{\"type\":\"object\"}".to_owned(),
            mutating: false,
            action: "Read".to_owned(),
            resource_selector: "realm".to_owned(),
            reversibility: Reversibility::Irreversible,
            owner: nc_kernel::registry::Owner::Kernel,
        }
    }

    /// A command is listed with every field the registry holds.
    ///
    /// Its texts in the locale asked, its glyph by its code point, its
    /// reversibility and its owner by their words.
    #[test]
    fn a_command_is_listed_with_every_field_the_registry_holds() {
        use nc_schema::descriptor::Reversibility;
        let command = a_command();
        assert_eq!(
            super::listed(&command, Locale::En).to_canonical(),
            "{\"action\":\"Read\",\"args_schema\":\"{\\\"type\\\":\\\"object\\\"}\",\
             \"categories\":[\"noctua.history\"],\"display_name\":\"Verify\",\
             \"glyph\":\"U+F058\",\"line\":\"Check the history.\",\"mutating\":false,\
             \"name\":\"journal.verify\",\"owner\":\"kernel\",\
             \"paragraph\":\"Checks \\\"every\\\" entry.\",\"resource_selector\":\"realm\",\
             \"reversibility\":\"irreversible\",\"tags\":[\"verify\",\"check\"]}"
        );
        let unglyphed = nc_kernel::registry::RegisteredCommand {
            entry: nc_kernel::registry::CommandEntry::new(
                vec![command.entry.in_locale(Locale::En).clone()],
                Vec::new(),
                None,
            ),
            ..command.clone()
        };
        let listed = super::listed(&unglyphed, Locale::En).to_canonical();
        assert!(
            listed.contains("\"glyph\":null") && listed.contains("\"categories\":[]"),
            "{listed}"
        );
        for (reversibility, word) in [
            (Reversibility::Read, "read"),
            (Reversibility::Write, "write"),
        ] {
            let listed = super::listed(
                &nc_kernel::registry::RegisteredCommand {
                    reversibility,
                    mutating: true,
                    ..command.clone()
                },
                Locale::En,
            )
            .to_canonical();
            assert!(
                listed.contains(&format!("\"reversibility\":\"{word}\"")),
                "{listed}"
            );
            assert!(listed.contains("\"mutating\":true"), "{listed}");
        }
    }

    /// A command is listed in the locale asked for: every text in it, and
    /// nothing of the other.
    #[test]
    fn a_command_is_listed_in_the_locale_asked_for() {
        let listed = super::listed(&a_command(), Locale::PtBR).to_canonical();
        assert!(
            listed.contains("\"display_name\":\"Verificar\"")
                && listed.contains("\"line\":\"Conferir o histórico.\"")
                && listed.contains("\"paragraph\":\"Confere cada registro.\"")
                && listed.contains("\"tags\":[\"verificar\"]"),
            "{listed}"
        );
        assert!(
            !listed.contains("Verify") && !listed.contains("Check"),
            "{listed}"
        );
    }

    /// A plugin's entries at their bound fit half a frame: every command one
    /// plugin may declare, its texts the heaviest the judge admits in the
    /// locale asked, listed, is no more than half of `ipc_frame_max_bytes`.
    ///
    /// **The answer as the daemon writes it, of an entry the judge admits**,
    /// not a sum. A byte of a sentence or a tag costs the entry one and the
    /// answer at most two, which `"` and `\` reach, every other character
    /// JSON writes longer being one the judge refuses as unseen; every
    /// sentence is at its bound in characters of them. A tag's quotation marks
    /// and comma cost the entry three bytes and the answer only three, so
    /// fewer, longer tags list more: for every count of tags from one to
    /// `TAGS_MAX`, the tags are made as long as the judge still admits the
    /// entry, and the heaviest of those answers is the one held. A listing
    /// that stopped escaping, a judge that stopped counting a tag's
    /// punctuation, or an `entry_bytes` raised past the frame fails here.
    ///
    /// **And the bound the compiler holds**: texts of `entry_bytes`, every
    /// byte one JSON writes as two, fit half a frame exactly.
    #[test]
    fn a_plugins_entries_at_their_bound_fit_half_a_frame() {
        use nc_kernel::registry::{CommandEntry, EntryTexts};
        use nc_schema::entry::{self, Entry, InLocale, Rules, TAGS_MAX, Text, Texts};
        let most = usize::try_from(nc_catalog::KERNEL.entry_bytes).expect("fits");
        let commands = usize::try_from(nc_catalog::KERNEL.commands_per_plugin).expect("fits");
        let frame = usize::try_from(nc_catalog::DAEMON.ipc_frame_max_bytes).expect("fits");
        let texts_alone = |texts: &EntryTexts| {
            let listed = |texts: &EntryTexts| {
                let command = nc_kernel::registry::RegisteredCommand {
                    entry: CommandEntry::new(vec![texts.clone()], Vec::new(), None),
                    ..a_command()
                };
                super::listed(&command, Locale::En).to_canonical().len()
            };
            listed(texts) - listed(&EntryTexts::default())
        };
        let doubled = texts_alone(&EntryTexts {
            paragraph: "\"".repeat(most),
            ..EntryTexts::default()
        });
        assert_eq!(doubled, most * 2, "each `\"` is written as two bytes");
        assert!(
            doubled * commands <= frame / 2,
            "{commands} commands of {doubled} bytes of texts each are past half of a frame of \
             {frame}"
        );

        let admitted = |texts: &EntryTexts| {
            entry::judge(
                &Entry {
                    locales: &[InLocale {
                        locale: Locale::En.tag(),
                        texts: Texts {
                            display_name: Some(&texts.display_name),
                            line: Some(&texts.line),
                            paragraph: Some(&texts.paragraph),
                            tags: Some(&texts.tags),
                        },
                    }],
                    categories: &[],
                    glyph: None,
                    same_in_both: &[],
                },
                &Rules {
                    requires: &Text::ALL,
                    categories: &[],
                    entry_bytes: nc_catalog::KERNEL.entry_bytes,
                },
            )
            .is_ok()
        };
        // Tags no fold of case makes one: each spells its place in `"` and `\`,
        // then `long` more `"`, and one more again for the first `longer`.
        let width = TAGS_MAX.next_power_of_two().trailing_zeros();
        let shaped = |count: usize, long: usize, longer: usize| EntryTexts {
            display_name: "\"".repeat(Text::DisplayName.bound()),
            line: "\\".repeat(Text::Line.bound()),
            paragraph: "\"".repeat(Text::Paragraph.bound()),
            tags: (0..count)
                .map(|at| {
                    let place: String = (0..width)
                        .map(|bit| if (at >> bit) % 2 == 1 { '\\' } else { '"' })
                        .collect();
                    place + &"\"".repeat(long + usize::from(at < longer))
                })
                .collect(),
        };
        // The largest number `admits` holds of below `refused`, found by
        // halving: it holds of nought, and once it fails it fails of every
        // larger one, since a longer tag is never admitted where a shorter one
        // was refused.
        let largest = |admits: &dyn Fn(usize) -> bool, mut refused: usize| {
            let mut held = 0;
            while refused - held > 1 {
                let between = held + (refused - held) / 2;
                if admits(between) {
                    held = between;
                } else {
                    refused = between;
                }
            }
            held
        };
        let listed = (1..=TAGS_MAX)
            .map(|count| {
                assert!(admitted(&shaped(count, 0, 0)), "{count} tag(s)");
                // Every tag as long as the judge admits, a tag `entry_bytes`
                // longer being past the entry's bytes, and then as many of
                // them as it admits a character longer still.
                let long = largest(&|long| admitted(&shaped(count, long, 0)), most);
                let longer = largest(&|longer| admitted(&shaped(count, long, longer)), count + 1);
                texts_alone(&shaped(count, long, longer))
            })
            .max()
            .expect("a count of tags");
        assert!(
            listed > most,
            "every `\"` and `\\` is written as two bytes, and {listed} are not"
        );
        assert!(
            listed * commands <= frame / 2,
            "{commands} commands of the heaviest texts the judge admits, {listed} bytes each \
             listed, are past half of a frame of {frame}"
        );
    }

    /// Nothing is dispatched, and no registry is read, before the kernel runs
    /// or once a stop began; the status and a stop are answered all along.
    #[tokio::test]
    async fn nothing_is_dispatched_before_the_kernel_runs_or_once_a_stop_began() {
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        let dispatch = || Request::Dispatch {
            command: "journal.verify".to_owned(),
            args: Value::object([("from", Value::Integer(0))]),
        };
        for state in [State::Starting, State::WaitingForClock, State::Recovering] {
            server.reach(state);
            for request in [dispatch(), REGISTRY] {
                let refusal = refused(&server, request).await;
                assert_eq!(refusal.code, ErrorCode::DaemonNotRunning, "{state:?}");
                assert_eq!(refusal.kind, RefusalKind::Unavailable);
                assert!(refusal.sentence.contains(state.word()), "{refusal:?}");
            }
            assert!(matches!(
                server.asked(Request::Status).await.answer,
                Answer::Status(_)
            ));
        }
        assert_eq!(server.asked(Request::Stop).await.answer, Answer::Stopping);
        assert_eq!(server.state(), State::Stopping);
        for request in [dispatch(), REGISTRY] {
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
            server.serve_one_after(daemon_end, None, || {
                tokio::time::sleep(Duration::from_millis(200))
            });
            server.stopping().await;
            server
                .wound_down(Vec::new(), tokio::time::Instant::now() + bound)
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

    /// What the daemon at `endpoint` says it is, and its answer to `request`,
    /// over a connection made through the endpoint and proven with `token`.
    async fn asked_at(
        endpoint: &nc_platform::ipc::Endpoint,
        user: &nc_platform::User,
        token: &str,
        request: Request,
    ) -> (State, Answer) {
        let bound = Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms);
        let (stream, _) = nc_platform::ipc::connect(endpoint, user)
            .await
            .expect("the endpoint answers");
        let token = nc_ipc::Token::from_text(token).expect("the token reads back");
        let (mut client, welcome) = tokio::time::timeout(bound, nc_ipc::connect(stream, &token))
            .await
            .expect("the handshake ended in time")
            .expect("proven");
        let answered = tokio::time::timeout(bound, client.ask(&request))
            .await
            .expect("answered in time")
            .expect("answered");
        (welcome.state, answered)
    }

    /// A stop asked again while the daemon drains is answered that it is stopping.
    ///
    /// Through the real endpoint, a dispatch counted as running standing for the
    /// one the drain waits for: the second stop, a status and a dispatch are each
    /// answered, the dispatch refused, and the endpoint goes only once it is let
    /// go after the drain. **Found by a review**: the endpoint went as soon as a
    /// stop began, so a stop asked again found nothing answering and signalled
    /// the daemon through its record, cutting its drain short.
    #[test]
    fn a_stop_asked_again_while_it_drains_is_answered_that_it_is_stopping() {
        let installed =
            crate::testing::Installed::new(&std::env::current_exe().expect("this test binary"));
        let (layout, user) = (&installed.layout, &installed.user);
        crate::profile::prepare(layout, user).expect("the profile");
        let lock = layout
            .instance_lock
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        let held = nc_platform::try_hold(&layout.state, &lock, user)
            .expect("asked")
            .expect("nothing held it");
        let server = Arc::new(a_server(nc_channel::Channel::Staging));
        let token = server.token.to_text();
        let endpoint = nc_platform::ipc::Endpoint::new(
            layout.runtime.clone(),
            layout.socket.clone(),
            layout.pipe.clone(),
        );
        let bound = Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms);
        let asked = |request: Request| asked_at(&endpoint, user, &token, request);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(async {
                let listener = nc_ipc::listen(&endpoint, user, &held).expect("bound");
                let accepting = tokio::spawn(Arc::clone(&server).accepting(listener));
                server.reach(State::Running);
                server.running.fetch_add(1, Ordering::SeqCst);
                let (state, answer) = asked(Request::Stop).await;
                assert_eq!((state, answer), (State::Running, Answer::Stopping));
                let draining = tokio::spawn({
                    let server = Arc::clone(&server);
                    async move {
                        server
                            .wound_down(Vec::new(), tokio::time::Instant::now() + bound)
                            .await
                    }
                });
                let (state, answer) = asked(Request::Stop).await;
                assert_eq!(
                    (state, answer),
                    (State::Stopping, Answer::Stopping),
                    "a stop asked again was not answered that the daemon is stopping"
                );
                match asked(Request::Status).await.1 {
                    Answer::Status(status) => assert!(
                        status.to_canonical().contains("\"state\":\"stopping\""),
                        "{status:?}"
                    ),
                    other => panic!("a status was answered with {other:?}"),
                }
                let dispatch = Request::Dispatch {
                    command: "journal.verify".to_owned(),
                    args: Value::object([("from", Value::Integer(0))]),
                };
                match asked(dispatch).await.1 {
                    Answer::Refused(refusal) => {
                        assert_eq!(refusal.code, ErrorCode::DaemonStopping, "{refusal:?}");
                    }
                    other => panic!("a dispatch while it drains was answered with {other:?}"),
                }
                assert!(
                    !draining.is_finished(),
                    "the drain ended with a dispatch still running"
                );
                if server.running.fetch_sub(1, Ordering::SeqCst) == 1 {
                    server.finished.notify_waiters();
                }
                let abandoned = tokio::time::timeout(bound, draining)
                    .await
                    .expect("the drain ended once the dispatch did")
                    .expect("the drain did not panic");
                assert_eq!(abandoned, 0, "the drain gave up on a dispatch that ended");
                assert_eq!(asked(Request::Stop).await.1, Answer::Stopping);
                server
                    .let_go(accepting, tokio::time::Instant::now() + bound)
                    .await;
                assert!(
                    matches!(
                        nc_platform::ipc::connect(&endpoint, user).await,
                        Err(nc_platform::ipc::Refused::Nobody(_))
                    ),
                    "the endpoint outlived the run that let it go"
                );
            });
        drop(held);
    }
}
