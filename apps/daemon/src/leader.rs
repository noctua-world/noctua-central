//! The Leader, where the daemon is the process that asks the system for it
//! (`NOCTUA.md §12.8`, `nc_ipc::leader`): on Windows, for an install that is
//! not a test's.
//!
//! **Asked on a thread of its own, which holds the registration**: the
//! registration is not `Send` and is given back where it was made, so the
//! thread asks for the Leader in force (`nc_channel::keys::effective`) in the
//! session the daemon's environment names, holds what it was granted until
//! the daemon stops, and then drops it. **Each press is a tap**: the thread
//! that hears it sends it to a task of the runtime, which dispatches
//! `leader.tap` as the person ([`Server::leader_pressed`]) — decided and
//! journaled as the CLI's `noctua leader` is, so a hotkey and a compositor's
//! binding do the same.
//!
//! **What came of asking is the server's to show** ([`Server::leader_asked`]):
//! held, or refused with the code of what refused it — logged, held in the
//! watched state the interface's info line says it from, and in the status —
//! and a registration lost while it was held is told the same way. Where the
//! session's compositor delivers the Leader by its own binding nothing is
//! asked, and nothing is said.
//!
//! **A Leader a setting changes is asked for before the setting is written**
//! ([`Asking`], `NOCTUA.md §12.15`): the thread that holds the registration
//! asks the system for the chord chosen while the Leader in force is still
//! held ([`Asking::tried`]) — so a chord the system refuses, another program's
//! among them, refuses the setting with nothing changed and the old chord
//! still this daemon's — and, once the configuration is written, lets the old
//! chord go and holds the new one ([`Asking::kept`]); a write that failed
//! gives the new chord back instead ([`Asking::dropped`]). For as long as
//! the write takes both chords are held, and each press of either is a tap.
//!
//! **A test's install never asks** ([`asks`]): the Leader is the machine's,
//! one chord for the whole desktop, as lingering is (`cargo xtask dev
//! --install`), so an install relocated by its namespace, which every suite
//! makes, asks the system for nothing.
//!
//! # What this does not answer
//!
//! A Leader configured by anything but a setting of this daemon's — the file
//! edited by hand — which is asked for at the daemon's next start; and a
//! Windows daemon started outside the person's interactive session — over SSH — whose registration the system may grant and never
//! deliver, which was not measured.

use std::sync::Arc;

use nc_ipc::Refusal;
use nc_ipc::leader::{Listener, Listening};
use nc_platform::leader::Refused;
use nc_schema::keys::Leader;

use crate::server::Server;

/// What a press of the Leader is handed to, on the thread that hears it.
type OnPress = Box<dyn Fn() + Send + 'static>;

/// What is told, once, that a registration held was lost.
type OnLost = Box<dyn FnOnce(Refused) + Send + 'static>;

/// Whether the daemon of `install` asks the system for the Leader, where
/// `listener` is the process that does on this system.
///
/// **Only the daemon's system, and only a real install**: one relocated by
/// its namespace is a test's, which asks for nothing the whole machine shares.
#[must_use]
pub fn asks(install: &nc_channel::Install, listener: Listener) -> bool {
    listener == Listener::Daemon && install.namespace.is_none()
}

/// What the thread that holds the registration is asked.
enum Asked {
    /// To ask the system for this Leader beside the one held, and answer what
    /// came of it.
    Tried(Leader, std::sync::mpsc::SyncSender<Result<(), Refusal>>),
    /// To let the Leader held go and hold the one tried last in its place.
    Kept,
    /// To give the Leader tried last back.
    Dropped,
    /// To give every registration back, and end.
    Stopped,
}

/// The Leader the daemon holds, until it is given back.
#[derive(Debug)]
pub struct Held {
    /// What the thread is asked through.
    asks: std::sync::mpsc::Sender<Asked>,
    /// The thread that holds the registration, where one could be started.
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Held {
    /// Gives the Leader back: the thread drops the registration where it made
    /// it, and is waited for.
    pub fn given_back(self) {
        let Self { asks, thread } = self;
        // A thread that ended already has nothing left to give back.
        let _ = asks.send(Asked::Stopped);
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }

    /// What a change of the Leader is asked through, for as long as the
    /// Leader is held.
    #[must_use]
    pub fn asking(&self) -> Asking {
        Asking(self.asks.clone())
    }
}

/// What a change of the channel's Leader is asked through, of the thread
/// that holds the registration: the new chord tried beside the one held,
/// then kept in its place or given back.
#[derive(Debug, Clone)]
pub struct Asking(std::sync::mpsc::Sender<Asked>);

impl Asking {
    /// Asks the system for `leader` while the Leader held stays held, and
    /// waits for what came of it. The Leader held already is granted as it
    /// is, asked of nobody; one tried and never kept is given back first.
    ///
    /// # Errors
    ///
    /// The refusal of the registration, by the code of what refused it
    /// (`nc_ipc::leader::refusal`): nothing was changed, and the Leader held
    /// is held still. A thread that has ended — the daemon stopping — is a
    /// refusal too, `[NC-CHAN-0022]`.
    pub fn tried(&self, leader: Leader) -> Result<(), Refusal> {
        let ended = || {
            nc_ipc::leader::refusal(&Refused::Failed(
                "the daemon no longer holds a registration to change".to_owned(),
            ))
        };
        let (answer, answered) = std::sync::mpsc::sync_channel(1);
        self.0
            .send(Asked::Tried(leader, answer))
            .map_err(|_| ended())?;
        answered.recv().map_err(|_| ended())?
    }

    /// Lets the Leader held go and holds the one tried last in its place,
    /// which is said held from then on.
    pub fn kept(&self) {
        // A thread that ended holds nothing to keep.
        let _ = self.0.send(Asked::Kept);
    }

    /// Gives the Leader tried last back: the one held stands.
    pub fn dropped(&self) {
        // A thread that ended gave it back already.
        let _ = self.0.send(Asked::Dropped);
    }
}

/// Asks for the Leader in force for `found`'s install where its daemon is the
/// process that asks ([`asks`]), its presses tapped and what came of it told
/// to `server`; nothing where it does not ask.
#[must_use]
pub fn listened(
    found: &nc_channel::found::Found,
    server: &Arc<Server>,
    runtime: &tokio::runtime::Handle,
) -> Option<Held> {
    if !asks(&found.install, nc_ipc::leader::listener()) {
        return None;
    }
    let leader = nc_channel::keys::effective(found).leader;
    Some(listening(
        Arc::clone(server),
        runtime,
        leader,
        move |leader, on_press, on_lost| {
            let variable = |name: &str| std::env::var_os(name);
            nc_ipc::leader::listen(nc_platform::leader::delivery(&variable), || {
                nc_platform::leader::register(leader, &variable, on_press, on_lost)
            })
        },
    ))
}

/// `register` asked for `leader` on a thread of its own, which holds what
/// it grants until the [`Held`] answered is given back; each press it hears
/// tapped on `runtime`, and what came of it, and of a loss, told to `server`.
///
/// **What `register` is handed** is the Leader to ask for, a press that does
/// little and returns — a send on a channel — and a loss told to `server` as
/// a refusal of its own code, as `nc_platform::leader` asks of a caller.
///
/// **And a Leader a setting changes** ([`Asking`]) is asked for by the same
/// `register`, on the same thread, with the Leader held still held: granted,
/// it is kept beside the old one until the setting says which of the two
/// stands, and the other is dropped where it was made. The Leader held
/// already is never asked for again, since the system would refuse this
/// thread its own chord.
pub fn listening<H: 'static>(
    server: Arc<Server>,
    runtime: &tokio::runtime::Handle,
    leader: Leader,
    mut register: impl FnMut(Leader, OnPress, OnLost) -> Result<Listening<H>, Refusal> + Send + 'static,
) -> Held {
    let (presses, mut heard) = tokio::sync::mpsc::unbounded_channel::<()>();
    let (asks, asked) = std::sync::mpsc::channel::<Asked>();
    let told = Arc::clone(&server);
    let asking = move || {
        let mut registered = |leader: Leader| {
            let (presses, lost) = (presses.clone(), Arc::clone(&told));
            register(
                leader,
                Box::new(move || {
                    let _ = presses.send(());
                }),
                Box::new(move |why| lost.leader_asked(Err(&nc_ipc::leader::refusal(&why)))),
            )
        };
        let mut held = (leader, registered(leader));
        match &held.1 {
            Ok(Listening::Held(_)) => told.leader_asked(Ok(())),
            Ok(Listening::ByBinding) => {}
            Err(refusal) => told.leader_asked(Err(refusal)),
        }
        let mut tried: Option<(Leader, Listening<H>)> = None;
        // Until it is told to stop, or nothing is left that could ask.
        while let Ok(asked) = asked.recv() {
            match asked {
                Asked::Tried(leader, answer) => {
                    drop(tried.take());
                    let granted = if leader == held.0 && held.1.is_ok() {
                        Ok(())
                    } else {
                        registered(leader).map(|listening| tried = Some((leader, listening)))
                    };
                    // An asker that went away is answered by nobody, and what
                    // it tried is given back when the next thing is asked.
                    let _ = answer.send(granted);
                }
                Asked::Kept => {
                    if let Some((leader, listening)) = tried.take() {
                        if matches!(listening, Listening::Held(_)) {
                            told.leader_asked(Ok(()));
                        }
                        // The old registration is dropped here, as the new
                        // one takes its place.
                        held = (leader, Ok(listening));
                    }
                }
                Asked::Dropped => drop(tried.take()),
                Asked::Stopped => break,
            }
        }
        drop((held, tried));
    };
    let thread = std::thread::Builder::new()
        .name("leader".to_owned())
        .spawn(asking)
        .inspect_err(|error| {
            server.leader_asked(Err(&nc_ipc::leader::refusal(&Refused::Failed(format!(
                "the thread that asks for it could not be started: {error}"
            )))));
        })
        .ok();
    runtime.spawn(async move {
        while heard.recv().await.is_some() {
            server.leader_pressed().await;
        }
    });
    Held { asks, thread }
}

#[cfg(test)]
mod tests {
    use nc_ipc::leader::Listener;

    /// The daemon asks for the Leader only where it is the listener, and
    /// never for a test's install.
    #[test]
    fn the_daemon_asks_only_where_it_listens_and_never_for_a_tests_install() {
        let installed =
            crate::testing::Installed::new(&std::env::current_exe().expect("this test binary"));
        let relocated = installed.install.clone();
        assert!(
            relocated.namespace.is_some(),
            "a suite's install is relocated"
        );
        assert!(!super::asks(&relocated, Listener::Daemon));
        assert!(!super::asks(&relocated, Listener::Desktop));
        let real = nc_channel::Install {
            namespace: None,
            ..relocated
        };
        assert!(super::asks(&real, Listener::Daemon));
        assert!(!super::asks(&real, Listener::Desktop));
    }
}
