//! The one table of the CLI's exit codes (`NOCTUA.md §20.2`), which the verb
//! that asks for help prints and Stage 14's generated reference reads.
//!
//! **One outcome per code, and every way a run ends is one of them**: a
//! script tells a command denied from one the daemon never heard by the code
//! alone. A refusal the kernel makes is mapped by its kind, and a command that
//! needs a person's confirmation by its code, `NC-KERN-0006`, because what
//! fixes it is the Noctua window and not another try.

use core::fmt::Write as _;

use nc_catalog::ErrorCode;
use nc_ipc::{Refusal, RefusalKind};

/// How a run of the CLI ended, by the exit code it carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// The command ran, or the verb did what it was asked.
    Success,
    /// The CLI itself failed.
    Failed,
    /// The command line or the arguments were refused.
    Invalid,
    /// The command was denied.
    Denied,
    /// The command needs the Noctua window.
    NeedsTheInterface,
    /// What the command names is not there.
    NotFound,
    /// The command was written against something that changed.
    Conflict,
    /// Something the command needs did not answer.
    Unavailable,
    /// The daemon or a plugin failed.
    Internal,
    /// No daemon of the channel answered.
    Unreachable,
    /// The connection was refused.
    Refused,
    /// Starting or stopping the daemon did not finish.
    ProcessControl,
    /// No install was found beside this executable.
    NoInstall,
}

impl Exit {
    /// Every outcome, in the order of its code.
    pub const ALL: [Self; 13] = [
        Self::Success,
        Self::Failed,
        Self::Invalid,
        Self::Denied,
        Self::NeedsTheInterface,
        Self::NotFound,
        Self::Conflict,
        Self::Unavailable,
        Self::Internal,
        Self::Unreachable,
        Self::Refused,
        Self::ProcessControl,
        Self::NoInstall,
    ];

    /// The exit code a run that ends this way carries.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failed => 1,
            Self::Invalid => 2,
            Self::Denied => 3,
            Self::NeedsTheInterface => 4,
            Self::NotFound => 5,
            Self::Conflict => 6,
            Self::Unavailable => 7,
            Self::Internal => 8,
            Self::Unreachable => 9,
            Self::Refused => 10,
            Self::ProcessControl => 11,
            Self::NoInstall => 12,
        }
    }

    /// What a run that ends this way means, as `help exit-codes` prints it.
    #[must_use]
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::Success => "the command ran, or the verb did what it was asked",
            Self::Failed => {
                "the CLI itself failed: it could not write its answer, or could not start \
                 what it reaches the daemon with"
            }
            Self::Invalid => {
                "the command line or the arguments were refused: the CLI could not read them, \
                 or the command's schema refused them"
            }
            Self::Denied => "the command was denied",
            Self::NeedsTheInterface => {
                "the command needs a person's confirmation in the Noctua window, which no \
                 command line can give"
            }
            Self::NotFound => "what the command names is not there, the command itself included",
            Self::Conflict => "the command was written against something that changed",
            Self::Unavailable => "something the command needs did not answer; try again",
            Self::Internal => {
                "the daemon or a plugin failed, or answered with something the CLI does not read"
            }
            Self::Unreachable => "no daemon of this channel answered, or it stopped answering",
            Self::Refused => {
                "the connection was refused: the endpoint, the token or the handshake did not \
                 show both ends to be this install's"
            }
            Self::ProcessControl => "starting or stopping the daemon did not finish",
            Self::NoInstall => {
                "no install was found beside this executable, or its manifest was refused"
            }
        }
    }

    /// The outcome a refusal the kernel made ends a run in.
    ///
    /// **By its kind, but for a confirmation**: `NC-KERN-0006` is a denial,
    /// and what fixes it is a person in the Noctua window, never another try
    /// from a command line — so it has a code of its own.
    #[must_use]
    pub const fn of_refusal(refusal: &Refusal) -> Self {
        if matches!(refusal.code, ErrorCode::KernelUnconfirmed) {
            return Self::NeedsTheInterface;
        }
        match refusal.kind {
            RefusalKind::Denied => Self::Denied,
            RefusalKind::NotFound => Self::NotFound,
            RefusalKind::Invalid => Self::Invalid,
            RefusalKind::Conflict => Self::Conflict,
            RefusalKind::Unavailable => Self::Unavailable,
            RefusalKind::Internal => Self::Internal,
        }
    }
}

/// The table, as `help exit-codes` prints it: one line per code, in order.
#[must_use]
pub fn table() -> String {
    let mut table = String::from("Exit codes:\n");
    for exit in Exit::ALL {
        let _ = writeln!(table, "  {:>2}  {}", exit.code(), exit.meaning());
    }
    table
}

#[cfg(test)]
mod tests {
    use nc_catalog::ErrorCode;
    use nc_ipc::{Refusal, RefusalKind};

    use super::Exit;

    /// Every outcome has a code of its own, the codes run from 0 to 12 with
    /// none skipped, and `ALL` holds each outcome once, in that order — the
    /// table the plan writes (`NOCTUA.md §20.2`).
    #[test]
    fn every_outcome_is_one_row_and_the_codes_are_the_tables() {
        let codes: Vec<u8> = Exit::ALL.iter().map(|exit| exit.code()).collect();
        assert_eq!(codes, (0..=12).collect::<Vec<u8>>());
        for (at, exit) in Exit::ALL.iter().enumerate() {
            assert!(
                !Exit::ALL[..at].contains(exit),
                "{exit:?} is in the table twice"
            );
        }
        for (exit, code) in [
            (Exit::Success, 0),
            (Exit::Failed, 1),
            (Exit::Invalid, 2),
            (Exit::Denied, 3),
            (Exit::NeedsTheInterface, 4),
            (Exit::NotFound, 5),
            (Exit::Conflict, 6),
            (Exit::Unavailable, 7),
            (Exit::Internal, 8),
            (Exit::Unreachable, 9),
            (Exit::Refused, 10),
            (Exit::ProcessControl, 11),
            (Exit::NoInstall, 12),
        ] {
            assert_eq!(exit.code(), code, "{exit:?}");
        }
    }

    /// The printed table holds every row once, in order, each code beside its
    /// own meaning, and nothing else but its heading.
    #[test]
    fn the_printed_table_holds_every_row_once_in_order() {
        let table = super::table();
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.first(), Some(&"Exit codes:"));
        assert_eq!(lines.len(), 1 + Exit::ALL.len(), "{table}");
        for (line, exit) in lines[1..].iter().zip(Exit::ALL) {
            assert_eq!(
                *line,
                format!("  {:>2}  {}", exit.code(), exit.meaning()),
                "{table}"
            );
        }
        for exit in Exit::ALL {
            assert_eq!(
                table.matches(exit.meaning()).count(),
                1,
                "{exit:?} is not printed once"
            );
        }
    }

    /// A refusal ends a run by its kind, and a confirmation the kernel asked
    /// for ends it in the outcome of its own, whatever kind it came with.
    #[test]
    fn a_refusal_ends_a_run_by_its_kind_and_a_confirmation_by_its_code() {
        let refusal = |code, kind| Refusal {
            code,
            kind,
            sentence: String::new(),
        };
        for (kind, exit) in [
            (RefusalKind::Denied, Exit::Denied),
            (RefusalKind::NotFound, Exit::NotFound),
            (RefusalKind::Invalid, Exit::Invalid),
            (RefusalKind::Conflict, Exit::Conflict),
            (RefusalKind::Unavailable, Exit::Unavailable),
            (RefusalKind::Internal, Exit::Internal),
        ] {
            assert_eq!(
                Exit::of_refusal(&refusal(ErrorCode::PolicyDenied, kind)),
                exit,
                "{kind:?}"
            );
            assert_eq!(
                Exit::of_refusal(&refusal(ErrorCode::KernelUnconfirmed, kind)),
                Exit::NeedsTheInterface,
                "{kind:?}"
            );
        }
    }
}
