//! The Leader heard by a process of an install and handed to its daemon
//! (`NOCTUA.md §12.8`): which process asks the system for it, what a press is
//! sent as, and how a refusal is told.
//!
//! **One press, one command.** A press of the Leader, whichever process hears
//! it, is [`TAP`] dispatched as the person for the system's own seat, with
//! [`tap`] — what the CLI's `noctua leader` dispatches too, which a Wayland
//! compositor's binding runs — so a hotkey and a binding do the same, and the
//! kernel decides what a tap does (`leader.tap`).
//!
//! **Which process asks, by system** ([`listener_on`]), from what each
//! system's daemon is as `cargo xtask dev --install` registers it
//! (`NOCTUA.md §17.3`) and where `nc_platform::leader` delivers a press:
//!
//! | System | Listener | Why |
//! |---|---|---|
//! | Windows | the daemon | Task Scheduler starts it with the person's interactive token, inside their logon session, which it ends with; `RegisterHotKey` asks for a thread that reads its own message queue, which `nc_platform::leader` makes |
//! | Linux | the desktop | the daemon is a user service that lingering starts at boot, before any graphical session, so its environment names no display; it outlives the X server a grab is held on, and a later session's variables never reach a service already running |
//! | macOS | the desktop | Carbon delivers a press on the main thread's run loop, which the daemon's main thread — its runtime's — never turns, and the desktop's event loop does |
//!
//! Where the delivery is a compositor's binding (Wayland), nothing is asked
//! of the system by anyone: [`listen`] answers [`Listening::ByBinding`].
//!
//! **A refusal is told, never dropped** ([`refusal`]): each kind of
//! `nc_platform::leader::Refused` is a code of the catalog's, so the process
//! that was refused hands it on rather than running on in silence — the
//! daemon in its status and its log, and the desktop interface, once it is
//! built, on its info line in the person's language.
//!
//! **Which process asks is decided here for every system, and only the
//! daemon asks yet**: the desktop interface, the listener on Linux and macOS,
//! is not built, so on those systems a press reaches the kernel only through
//! `noctua leader` until it is.
//!
//! # What this does not answer
//!
//! Which process listens where both could, measured: the table above is read
//! from each system's documentation and from how this repository registers
//! its daemon, and the Windows row was not run on a person's desktop here.

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_platform::clipboard::System;
use nc_platform::leader::{Delivery, Refused};

use crate::{Refusal, RefusalKind};

/// The command a press of the Leader is: the kernel's own, which follows the
/// three rules of a tap (`NOCTUA.md §12.8`).
pub const TAP: &str = "leader.tap";

/// The arguments a press is dispatched with: none, so the tap is the system's
/// own seat's.
#[must_use]
pub fn tap() -> Value {
    Value::object(Vec::<(&str, Value)>::new())
}

/// The process of an install that asks the system for the channel's Leader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listener {
    /// The daemon, on a thread of its own.
    Daemon,
    /// The desktop interface, on the thread its event loop runs on.
    Desktop,
}

impl Listener {
    /// Its word, as the daemon's status writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Daemon => "daemon",
            Self::Desktop => "desktop",
        }
    }
}

/// The process that asks `system` for the Leader, as this module's header
/// says why.
#[must_use]
pub const fn listener_on(system: System) -> Listener {
    match system {
        System::Windows => Listener::Daemon,
        System::Linux | System::MacOs => Listener::Desktop,
    }
}

/// The process that asks this system for the Leader.
#[must_use]
pub const fn listener() -> Listener {
    listener_on(nc_platform::clipboard::SYSTEM)
}

/// The refusal `refused` is told as: its code, its kind, and what the system
/// said.
///
/// **One code for each thing that refused it**: another program holding the
/// chord is `[NC-CHAN-0020]`, a conflict; a session that registers no global
/// key — none named, or one whose compositor delivers the Leader by its own
/// binding — `[NC-CHAN-0021]`; and a system that could not be asked, or a
/// registration that stopped delivering, `[NC-CHAN-0022]`.
#[must_use]
pub fn refusal(refused: &Refused) -> Refusal {
    let (code, kind) = match refused {
        Refused::Taken => (ErrorCode::ChannelLeaderTaken, RefusalKind::Conflict),
        Refused::NoSession | Refused::ByBinding => {
            (ErrorCode::ChannelLeaderNoSession, RefusalKind::Unavailable)
        }
        Refused::Failed(_) => (
            ErrorCode::ChannelLeaderUnregistered,
            RefusalKind::Unavailable,
        ),
    };
    Refusal {
        code,
        kind,
        sentence: format!("the channel's Leader is not registered: {refused}"),
    }
}

/// What asking for the Leader came to, where it was not refused.
#[derive(Debug)]
pub enum Listening<H> {
    /// The system holds the chord for this process until `H` is dropped.
    Held(H),
    /// The session's compositor delivers the Leader by its own binding, which
    /// runs `noctua leader`: nothing was asked.
    ByBinding,
}

/// Asks for the Leader where `delivery` — the session's, as
/// `nc_platform::leader::delivery` answers it — says the system registers it,
/// by `register`; answers [`Listening::ByBinding`] where a compositor's
/// binding delivers it, and asks nothing.
///
/// `register` is `nc_platform::leader::register` for the channel's Leader in
/// force, called on the thread the presses are to be held on: the module
/// header of `nc_platform::leader` says which.
///
/// # Errors
///
/// The [`refusal`] of what refused it: the session, or the registration.
pub fn listen<H>(
    delivery: Result<Delivery, Refused>,
    register: impl FnOnce() -> Result<H, Refused>,
) -> Result<Listening<H>, Refusal> {
    match delivery.map_err(|refused| refusal(&refused))? {
        Delivery::CompositorBinding => Ok(Listening::ByBinding),
        Delivery::GlobalHotkey => register()
            .map(Listening::Held)
            .map_err(|refused| refusal(&refused)),
    }
}

#[cfg(test)]
mod tests {
    use nc_catalog::ErrorCode;
    use nc_platform::clipboard::System;
    use nc_platform::leader::{Delivery, Refused};

    use super::{Listener, Listening, listen, listener_on, refusal};
    use crate::RefusalKind;

    /// The daemon listens on Windows, and the desktop on Linux and macOS.
    #[test]
    fn the_daemon_listens_on_windows_and_the_desktop_elsewhere() {
        assert_eq!(listener_on(System::Windows), Listener::Daemon);
        assert_eq!(listener_on(System::Linux), Listener::Desktop);
        assert_eq!(listener_on(System::MacOs), Listener::Desktop);
        assert_eq!(
            super::listener(),
            listener_on(nc_platform::clipboard::SYSTEM)
        );
        assert_eq!(
            [Listener::Daemon.word(), Listener::Desktop.word()],
            ["daemon", "desktop"]
        );
    }

    /// A press is `leader.tap` with no argument, the system's own seat's.
    #[test]
    fn a_press_is_a_tap_of_the_systems_own_seat() {
        assert_eq!(super::TAP, "leader.tap");
        assert_eq!(super::tap().to_canonical(), "{}");
        assert!(
            nc_catalog::COMMANDS
                .iter()
                .any(|command| command.name == super::TAP),
            "the catalog declares no `leader.tap`"
        );
    }

    /// Each refusal is told by the code of what refused it, saying what the
    /// system said.
    #[test]
    fn each_refusal_is_told_by_the_code_of_what_refused_it() {
        for (refused, code, kind) in [
            (
                Refused::Taken,
                ErrorCode::ChannelLeaderTaken,
                RefusalKind::Conflict,
            ),
            (
                Refused::NoSession,
                ErrorCode::ChannelLeaderNoSession,
                RefusalKind::Unavailable,
            ),
            (
                Refused::ByBinding,
                ErrorCode::ChannelLeaderNoSession,
                RefusalKind::Unavailable,
            ),
            (
                Refused::Failed("`RegisterHotKey` failed with Windows error 5".to_owned()),
                ErrorCode::ChannelLeaderUnregistered,
                RefusalKind::Unavailable,
            ),
        ] {
            let told = refusal(&refused);
            assert_eq!((told.code, told.kind), (code, kind), "{refused:?}");
            assert!(
                told.sentence.ends_with(&refused.to_string())
                    && told
                        .sentence
                        .starts_with("the channel's Leader is not registered"),
                "{told:?}"
            );
        }
    }

    /// A hotkey is registered, a binding asks nothing, and a refusal of the
    /// session or of the registration is told.
    #[test]
    fn a_hotkey_is_registered_a_binding_asks_nothing_and_a_refusal_is_told() {
        let held = listen(Ok(Delivery::GlobalHotkey), || Ok::<_, Refused>(7));
        assert!(matches!(held, Ok(Listening::Held(7))), "{held:?}");
        let bound = listen(
            Ok(Delivery::CompositorBinding),
            || -> Result<u8, Refused> { panic!("a binding asked the system") },
        );
        assert!(matches!(bound, Ok(Listening::ByBinding)), "{bound:?}");
        let nobody = listen(Err(Refused::NoSession), || -> Result<u8, Refused> {
            panic!("a session that names no display asked the system")
        });
        assert_eq!(
            nobody.expect_err("refused").code,
            ErrorCode::ChannelLeaderNoSession
        );
        let taken = listen(Ok(Delivery::GlobalHotkey), || Err::<u8, _>(Refused::Taken));
        assert_eq!(
            taken.expect_err("refused").code,
            ErrorCode::ChannelLeaderTaken
        );
    }
}
