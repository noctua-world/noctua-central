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
//! **A test's install never asks** ([`asks`]): the Leader is the machine's,
//! one chord for the whole desktop, as lingering is (`cargo xtask dev
//! --install`), so an install relocated by its namespace, which every suite
//! makes, asks the system for nothing.
//!
//! # What this does not answer
//!
//! A Leader configured after the daemon started, which is asked for at its
//! next start; and a Windows daemon started outside the person's interactive
//! session — over SSH — whose registration the system may grant and never
//! deliver, which was not measured.

use std::sync::Arc;

use nc_ipc::Refusal;
use nc_ipc::leader::{Listener, Listening};
use nc_platform::leader::Refused;

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

/// The Leader the daemon holds, until it is given back.
#[derive(Debug)]
pub struct Held {
    /// Dropped to tell the thread to give the registration back.
    stop: std::sync::mpsc::Sender<()>,
    /// The thread that holds the registration, where one could be started.
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Held {
    /// Gives the Leader back: the thread drops the registration where it made
    /// it, and is waited for.
    pub fn given_back(self) {
        let Self { stop, thread } = self;
        drop(stop);
        if let Some(thread) = thread {
            let _ = thread.join();
        }
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
        move |on_press, on_lost| {
            let variable = |name: &str| std::env::var_os(name);
            nc_ipc::leader::listen(nc_platform::leader::delivery(&variable), || {
                nc_platform::leader::register(leader, &variable, on_press, on_lost)
            })
        },
    ))
}

/// `register` asked on a thread of its own, which holds what it grants until
/// the [`Held`] answered is given back; each press it hears tapped on
/// `runtime`, and what came of it, and of a loss, told to `server`.
///
/// **What `register` is handed** is a press that does little and returns — a
/// send on a channel — and a loss told to `server` as a refusal of its own
/// code, as `nc_platform::leader` asks of a caller.
pub fn listening<H: 'static>(
    server: Arc<Server>,
    runtime: &tokio::runtime::Handle,
    register: impl FnOnce(OnPress, OnLost) -> Result<Listening<H>, Refusal> + Send + 'static,
) -> Held {
    let (presses, mut heard) = tokio::sync::mpsc::unbounded_channel::<()>();
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let told = Arc::clone(&server);
    let asking = move || {
        let lost = Arc::clone(&told);
        let asked = register(
            Box::new(move || {
                let _ = presses.send(());
            }),
            Box::new(move |why| lost.leader_asked(Err(&nc_ipc::leader::refusal(&why)))),
        );
        match &asked {
            Ok(Listening::Held(_)) => told.leader_asked(Ok(())),
            Ok(Listening::ByBinding) => {}
            Err(refusal) => told.leader_asked(Err(refusal)),
        }
        let _ = stopped.recv();
        drop(asked);
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
    Held { stop, thread }
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
