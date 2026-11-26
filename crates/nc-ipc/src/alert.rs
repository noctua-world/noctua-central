//! The daemon alert line (`NOCTUA.md §12.17`): what an interface that lost its
//! daemon says, when it restores it, and what it runs to do so.
//!
//! **The interface's, not the kernel's.** The line shows while the daemon is
//! gone, and the kernel and every state it holds (§12.1) are gone with it, so
//! what the line says is decided here, in the client the desktop interface
//! asks its daemon through, from what an interface still has: when it lost
//! the daemon — its heartbeat stopped, the connection dropped past
//! `ipc_silent_after_ms` ([`crate::Client`]) — the profile's `stopped` mark,
//! read by the one reader every program of an install shares
//! (`nc_channel::stopped`), and the catalog's countdown,
//! `[daemon] interface_restore_countdown_ms`, which no caller hands in.
//!
//! | The profile | The line says | It offers | It counts down |
//! |---|---|---|---|
//! | carries no `stopped` mark | the local server is not running | restore now | from the moment it first says so, and restores the daemon when the countdown ends |
//! | carries the mark | it is stopped | start | nothing |
//!
//! **The interface keeps the clock, as data.** An [`Absence`] is asked at
//! each moment the interface looks ([`Absence::at`]), with the time and the
//! mark as it reads them then, and answers the [`Line`] to draw — what it
//! says, what it offers, and the countdown's deadline as an absolute time in
//! milliseconds since the Unix epoch, which `noctua-design`'s `MessageBar`
//! line draws the seconds left until and acts on nothing — and whether to
//! restore the daemon now. A deadline never moves while it counts: the
//! interface looks again whenever it likes, and the line it draws in between
//! counts to the same moment.
//!
//! **What each act runs** ([`Run`]): a person's — restore now, or start — is
//! the install's own CLI, `noctua start` (`NOCTUA.md §17.1`), which removes
//! the mark and starts the daemon through the service manager's restorer
//! where one is registered, so whatever the line offers, the CLI does too
//! (§2.8); the countdown's is `noctua-daemon --restored`, as every restorer
//! starts the daemon (§17.5). **The interface reads the mark to say what the
//! line says, and the daemon honours it**: a mark written after the
//! interface last read it — a `noctua stop` in a terminal while the line
//! counts — still holds the daemon down, because the restored start reads it
//! before anything else of the profile and ends at once.
//!
//! **One automatic restore in an absence.** The countdown runs from the moment
//! the line first says the server is not running. A mark read before it ends
//! cancels it, nothing counts down while the mark is there, and a mark read
//! gone again counts down afresh from that moment. Once the countdown ended
//! and its restore was asked, or a person acted on the line, nothing counts
//! down again until the daemon returns and is lost once more: a daemon that
//! ends at each start is not started again each countdown, and the tray, once
//! it is built, is the watchdog of last resort (§17.5). **A mark that does
//! not read is a stop** ([`stopped`]): nothing restores the daemon over a
//! stop the interface cannot rule out, which the daemon's own restored start
//! would refuse as well.
//!
//! # What this does not answer
//!
//! A daemon the countdown restored runs outside its service manager's unit,
//! started by the interface rather than by the manager, so the manager does
//! not restore it after a later crash — the interface's next countdown does —
//! until a stop and a start put it back under the manager. And a clock moved
//! back while the line counts makes the countdown longer by as much: the
//! deadline never moves.

use std::path::PathBuf;

use nc_channel::Layout;
use nc_platform::User;

/// The argument every restorer starts the daemon with: started so, it ends at
/// once, and well, while the profile carries the `stopped` mark or another
/// daemon of the channel runs (`NOCTUA.md §17.5`).
pub const RESTORED: &str = "--restored";

/// The CLI's verb that starts the channel's daemon, removing the `stopped`
/// mark first: what a person's act on the line runs.
pub const START: &str = "start";

/// What the line says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// The local server is not running: the daemon was lost, and no person
    /// stopped it.
    NotRunning,
    /// The local server is stopped: a person stopped it on purpose.
    Stopped,
}

impl Said {
    /// Its word, which names it to whatever draws the line.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::NotRunning => "not-running",
            Self::Stopped => "stopped",
        }
    }
}

/// What the line offers a person to do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// Restore now: the daemon started at once, before the countdown ends.
    RestoreNow,
    /// Start: the daemon a person stopped, started again.
    Start,
}

impl Offer {
    /// Its word: the name the line's control is found by, as `noctua-design`'s
    /// line writes it on the control (`data-nd-action`).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::RestoreNow => "restore",
            Self::Start => "start",
        }
    }
}

/// The line an interface draws at one moment of an absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    /// What it says.
    pub said: Said,
    /// What it offers.
    pub offer: Offer,
    /// When the countdown ends and the interface restores the daemon, in
    /// milliseconds since the Unix epoch; nothing while nothing counts down.
    pub deadline_ms: Option<u64>,
}

/// What an interface does at one moment of an absence: the line it draws, and
/// whether it restores the daemon now, by [`Run::restorer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    /// The line to draw.
    pub line: Line,
    /// Whether the countdown ended now: answered once in an absence.
    pub restore: bool,
}

/// An interface's absence of its daemon: from the moment it lost it until the
/// daemon returns, when the interface lets it go and the line disappears.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Absence {
    /// When the line began saying the server is not running, while it counts
    /// down.
    counting_from: Option<u64>,
    /// Whether a restore was asked in it, by the countdown or by a person:
    /// nothing counts down after one.
    asked: bool,
}

impl Absence {
    /// An absence that has just begun: nothing counted, nothing asked.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            counting_from: None,
            asked: false,
        }
    }

    /// The line at `now_ms`, in milliseconds since the Unix epoch, with the
    /// profile carrying the `stopped` mark or not as [`stopped`] reads it
    /// then; and whether to restore the daemon now.
    pub fn at(&mut self, now_ms: u64, stopped: bool) -> Seen {
        if stopped {
            self.counting_from = None;
            return Seen {
                line: Line {
                    said: Said::Stopped,
                    offer: Offer::Start,
                    deadline_ms: None,
                },
                restore: false,
            };
        }
        let not_running = |deadline_ms| Line {
            said: Said::NotRunning,
            offer: Offer::RestoreNow,
            deadline_ms,
        };
        if self.asked {
            return Seen {
                line: not_running(None),
                restore: false,
            };
        }
        let from = *self.counting_from.get_or_insert(now_ms);
        let deadline = from.saturating_add(nc_catalog::DAEMON.interface_restore_countdown_ms);
        if now_ms >= deadline {
            self.asked = true;
            self.counting_from = None;
            return Seen {
                line: not_running(None),
                restore: true,
            };
        }
        Seen {
            line: not_running(Some(deadline)),
            restore: false,
        }
    }

    /// A person acted on the line — restore now, or start — so the countdown,
    /// if one was running, ends without restoring anything: the person's start
    /// is the one under way.
    pub const fn acted(&mut self) {
        self.asked = true;
        self.counting_from = None;
    }
}

/// Whether the profile of `layout` carries the `stopped` mark, as the line
/// reads it: the shared reader's answer, and **a stop for a mark that does
/// not read** — one another account can reach, or one the system could not
/// judge — so nothing restores the daemon over a stop the interface cannot
/// rule out.
#[must_use]
pub fn stopped(layout: &Layout, user: &User) -> bool {
    nc_channel::stopped::on_purpose(layout, user).unwrap_or(true)
}

/// A program of the install, and the arguments the line runs it with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The executable: the install's own, never one found on `PATH`.
    pub program: PathBuf,
    /// What it is run with.
    pub arguments: &'static [&'static str],
}

impl Run {
    /// The countdown's restore: the install's daemon, [`RESTORED`].
    #[must_use]
    pub fn restorer(layout: &Layout) -> Self {
        Self {
            program: layout.install.join(format!(
                "{}{}",
                layout.daemon,
                nc_platform::EXECUTABLE_SUFFIX
            )),
            arguments: &[RESTORED],
        }
    }

    /// A person's start: the install's CLI, [`START`].
    #[must_use]
    pub fn start(layout: &Layout) -> Self {
        Self {
            program: layout.install.join(format!(
                "{}{}",
                layout.cli,
                nc_platform::EXECUTABLE_SUFFIX
            )),
            arguments: &[START],
        }
    }

    /// What pressing the line's control runs when it offers `offer`, for the
    /// install of `layout`: a person's start, whichever of the two it is —
    /// what the CLI reaches as well.
    #[must_use]
    pub fn offered(offer: Offer, layout: &Layout) -> Self {
        match offer {
            Offer::RestoreNow | Offer::Start => Self::start(layout),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Absence, Line, Offer, Run, Said, Seen};

    /// The catalog's countdown, which every deadline is that far past the
    /// moment the line began counting.
    const COUNTDOWN: u64 = nc_catalog::DAEMON.interface_restore_countdown_ms;

    /// A moment in milliseconds since the Unix epoch.
    const LOST: u64 = 1_795_000_000_000;

    fn not_running(deadline_ms: Option<u64>, restore: bool) -> Seen {
        Seen {
            line: Line {
                said: Said::NotRunning,
                offer: Offer::RestoreNow,
                deadline_ms,
            },
            restore,
        }
    }

    fn stopped() -> Seen {
        Seen {
            line: Line {
                said: Said::Stopped,
                offer: Offer::Start,
                deadline_ms: None,
            },
            restore: false,
        }
    }

    /// A daemon lost with no stop is said not running, offers restore now and
    /// counts down to one deadline the catalog's countdown away, restores at
    /// the deadline and not a millisecond before, and is restored once: after
    /// that restore nothing counts down again in the same absence.
    #[test]
    fn a_lost_daemon_counts_down_once_and_is_restored_at_the_deadline() {
        const { assert!(COUNTDOWN > 1, "a countdown with no moment inside it") };
        let mut absence = Absence::new();
        let deadline = Some(LOST + COUNTDOWN);
        assert_eq!(absence.at(LOST, false), not_running(deadline, false));
        assert_eq!(
            absence.at(LOST + 1, false),
            not_running(deadline, false),
            "a deadline moved while it counted"
        );
        assert_eq!(
            absence.at(LOST + COUNTDOWN - 1, false),
            not_running(deadline, false)
        );
        assert_eq!(
            absence.at(LOST + COUNTDOWN, false),
            not_running(None, true),
            "the countdown ended and nothing was restored"
        );
        for later in [
            LOST + COUNTDOWN,
            LOST + COUNTDOWN + 1,
            LOST + 10 * COUNTDOWN,
        ] {
            assert_eq!(
                absence.at(later, false),
                not_running(None, false),
                "restored again, or counted down again, at {later}"
            );
        }
    }

    /// A daemon a person stopped is said stopped and offers start, and nothing
    /// counts down or is restored however long the line shows.
    #[test]
    fn a_stop_says_stopped_offers_start_and_counts_nothing_down() {
        let mut absence = Absence::new();
        for moment in [LOST, LOST + COUNTDOWN, LOST + 100 * COUNTDOWN] {
            assert_eq!(absence.at(moment, true), stopped(), "at {moment}");
        }
    }

    /// A mark read while the line counts cancels the countdown, so the moment
    /// it would have ended restores nothing; a mark read gone again counts
    /// down afresh from then, to a deadline of its own.
    #[test]
    fn a_mark_read_while_counting_cancels_it_and_one_gone_counts_afresh() {
        let mut absence = Absence::new();
        assert_eq!(
            absence.at(LOST, false),
            not_running(Some(LOST + COUNTDOWN), false)
        );
        assert_eq!(absence.at(LOST + COUNTDOWN / 2, true), stopped());
        assert_eq!(
            absence.at(LOST + COUNTDOWN, true),
            stopped(),
            "a stop read while counting was restored over"
        );
        let again = LOST + 2 * COUNTDOWN;
        assert_eq!(
            absence.at(again, false),
            not_running(Some(again + COUNTDOWN), false),
            "a countdown cancelled by a stop resumed from its first moment"
        );
        assert_eq!(
            absence.at(again + COUNTDOWN, false),
            not_running(None, true)
        );
    }

    /// A person who acted on the line is not raced by the countdown: the
    /// moment it would have ended restores nothing, and nothing counts down
    /// again in that absence.
    #[test]
    fn a_person_who_acted_is_not_raced_by_the_countdown() {
        let mut absence = Absence::new();
        assert_eq!(
            absence.at(LOST, false),
            not_running(Some(LOST + COUNTDOWN), false)
        );
        absence.acted();
        for moment in [LOST + 1, LOST + COUNTDOWN, LOST + 2 * COUNTDOWN] {
            assert_eq!(
                absence.at(moment, false),
                not_running(None, false),
                "at {moment}"
            );
        }
        let mut stopped_first = Absence::new();
        assert_eq!(stopped_first.at(LOST, true), stopped());
        stopped_first.acted();
        assert_eq!(
            stopped_first.at(LOST + COUNTDOWN, false),
            not_running(None, false),
            "a start that left the daemon down was raced by a countdown"
        );
    }

    /// Each act runs the install's own program, never one found on `PATH`:
    /// the countdown the daemon `--restored`, and either offer the CLI's
    /// `start`; and each word the line is drawn by is its own.
    #[test]
    fn each_act_runs_the_installs_own_program() {
        // Bases nothing reads or makes: a layout is names alone.
        let base = |name: &str| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name);
        let install = nc_channel::Install {
            channel: nc_channel::Channel::Staging,
            bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
                .expect("plain absolute bases"),
            namespace: Some(nc_channel::Namespace::parse("t1").expect("a namespace")),
            log_endpoint: None,
        };
        let layout = nc_channel::layout(&install, "1000");
        let suffix = nc_platform::EXECUTABLE_SUFFIX;
        assert_eq!(
            Run::restorer(&layout),
            Run {
                program: layout.install.join(format!("{}{suffix}", layout.daemon)),
                arguments: &["--restored"],
            }
        );
        let start = Run {
            program: layout.install.join(format!("{}{suffix}", layout.cli)),
            arguments: &["start"],
        };
        assert_eq!(Run::start(&layout), start);
        assert_eq!(Run::offered(Offer::RestoreNow, &layout), start);
        assert_eq!(Run::offered(Offer::Start, &layout), start);
        assert_eq!(
            [Said::NotRunning.word(), Said::Stopped.word()],
            ["not-running", "stopped"]
        );
        assert_eq!(
            [Offer::RestoreNow.word(), Offer::Start.word()],
            ["restore", "start"]
        );
    }

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("nc-alert-line-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }
    }

    /// The line reads no mark as none, a mark as a stop, and a mark another
    /// account can reach as a stop too, rather than restore over it.
    #[test]
    fn a_mark_that_does_not_read_is_a_stop() {
        let scratch = Scratch::new("stopped");
        let user = nc_platform::user_in(&scratch.0).expect("the scratch is private");
        let base = |name: &str| scratch.0.join(name);
        let install = nc_channel::Install {
            channel: nc_channel::Channel::Staging,
            bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
                .expect("plain absolute bases"),
            namespace: Some(nc_channel::Namespace::parse("t1").expect("a namespace")),
            log_endpoint: None,
        };
        let layout = nc_channel::layout(&install, &user.to_string());
        assert!(!super::stopped(&layout, &user), "no state directory");
        nc_platform::create_private_directory(&base("s")).expect("the base");
        nc_platform::create_private_directory(&layout.state).expect("the state directory");
        assert!(!super::stopped(&layout, &user), "no mark");
        drop(nc_platform::create_private(&layout.stopped).expect("the mark"));
        assert!(super::stopped(&layout, &user), "a mark");
        nc_platform::testing::open_to_another_user(&layout.stopped).expect("opened");
        assert!(
            super::stopped(&layout, &user),
            "a mark another account can reach was read as none"
        );
    }
}
