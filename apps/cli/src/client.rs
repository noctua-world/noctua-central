//! The CLI's use of the shared client, and what a run that ends at it exits
//! with (`NOCTUA.md §11.1`, `§20.2`).
//!
//! **The client is `nc_ipc::client`**, shared with every other program that
//! asks a daemon: the endpoint reached, the token read, the handshake, the
//! welcome held to the install, and one request at a time. It is named here
//! so the rest of this crate reaches it by one path, and nothing of it is
//! written twice. What is the CLI's own is the exit code a refusal carries,
//! which a program with no exit code — the desktop interface — has no use
//! for: [`exit_of`].

pub use nc_ipc::client::{Changed, Connection, Dispatched, Listed, Unasked, connect, session};

use nc_catalog::ErrorCode;
use nc_ipc::IpcError;

use crate::exit::Exit;

/// The outcome a run that ends at `unasked` carries.
///
/// **Nothing listening is a daemon that is not running**, and so is one
/// that stopped answering a request; everything refused on the way to the
/// first request is a connection refused; a request too long to send is
/// arguments refused; and an answer the CLI cannot read — a frame that is
/// not one message, or one past the bound — from a daemon the handshake
/// proved is the daemon answering what the CLI does not read.
#[must_use]
pub fn exit_of(unasked: &Unasked) -> Exit {
    match unasked {
        Unasked::Connecting(error) if error.code() == ErrorCode::IpcUnreachable => {
            Exit::Unreachable
        }
        Unasked::Asking(IpcError::Unsendable { .. }) => Exit::Invalid,
        Unasked::Asking(IpcError::Malformed(_) | IpcError::TooLarge { .. })
        | Unasked::Unread(_) => Exit::Internal,
        Unasked::Asking(
            IpcError::Lost(_) | IpcError::Silent | IpcError::Unread | IpcError::TurnedAway(_),
        ) => Exit::Unreachable,
        Unasked::Connecting(_)
        | Unasked::TokenNotPrivate(_)
        | Unasked::TokenUnreadable(_)
        | Unasked::Handshake(_)
        | Unasked::Misnamed(_)
        | Unasked::Asking(_) => Exit::Refused,
        Unasked::Refused(refusal) => Exit::of_refusal(refusal),
        Unasked::Runtime(_) => Exit::Failed,
    }
}

#[cfg(test)]
mod tests {
    use nc_catalog::ErrorCode;
    use nc_ipc::testing::Scratch;
    use nc_ipc::{IpcError, Refusal, RefusalKind};

    use super::{Unasked, exit_of};
    use crate::exit::Exit;

    /// A request is answered or lost by how the connection ended: a daemon
    /// gone is not running, an answer the CLI cannot read is the daemon's
    /// failure, and a request too long to send is arguments refused.
    ///
    /// **Found by a review**: an answer refused as a frame after a proven
    /// handshake ended in 10, whose meaning is a connection that did not show
    /// both ends to be this install's, rather than in 8.
    #[test]
    fn a_request_not_answered_ends_by_how_the_connection_ended() {
        for (error, exit) in [
            (IpcError::Lost(String::new()), Exit::Unreachable),
            (IpcError::Silent, Exit::Unreachable),
            (IpcError::Unread, Exit::Unreachable),
            (
                IpcError::TurnedAway(ErrorCode::IpcSilent),
                Exit::Unreachable,
            ),
            (IpcError::Malformed(String::new()), Exit::Internal),
            (IpcError::TooLarge { length: 1 }, Exit::Internal),
            (IpcError::Protocol { theirs: 9 }, Exit::Refused),
            (IpcError::Unsendable { length: 1 }, Exit::Invalid),
        ] {
            assert_eq!(exit_of(&Unasked::Asking(error)), exit);
        }
    }

    /// Everything refused on the way to the first request is a connection
    /// refused, exit 10 — but nothing listening, which is a daemon not
    /// running, exit 9.
    ///
    /// **Nothing listening is asked of a real endpoint**: a scratch install
    /// no daemon serves is connected to, and the refusal that comes back
    /// ends in 9. A daemon of another protocol — what a CLI built before a
    /// change to the protocol sees of a daemon built after it — a proof that
    /// failed, a token not private or unreadable, a welcome misnamed and an
    /// endpoint whose other end is nobody's are each a connection refused.
    #[test]
    fn what_is_refused_before_the_first_request_is_a_connection_refused() {
        let installed = Scratch::new("exit");
        let nobody = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("a runtime")
            .block_on(super::connect(&installed.found))
            .expect_err("nothing listens there");
        assert_eq!(nobody.code(), ErrorCode::IpcUnreachable, "{nobody:?}");
        assert_eq!(exit_of(&nobody), Exit::Unreachable);
        for refused in [
            Unasked::Connecting(IpcError::Endpoint(nc_platform::ipc::Refused::NoProcess)),
            Unasked::Handshake(IpcError::Protocol { theirs: 4 }),
            Unasked::Handshake(IpcError::DaemonUnproven),
            Unasked::Handshake(IpcError::Slow),
            Unasked::TokenNotPrivate(String::new()),
            Unasked::TokenUnreadable(String::new()),
            Unasked::Misnamed(String::new()),
        ] {
            assert_eq!(exit_of(&refused), Exit::Refused, "{refused:?}");
        }
    }

    /// An answer of another kind than was asked is the daemon answering what
    /// the CLI does not read, exit 8; a refusal ends as its kind says; and a
    /// runtime that did not start is a run that failed.
    #[test]
    fn an_answer_unread_a_refusal_and_a_runtime_that_did_not_start_each_end_their_own_way() {
        assert_eq!(exit_of(&Unasked::Unread(String::new())), Exit::Internal);
        for (kind, exit) in [
            (RefusalKind::Unavailable, Exit::Unavailable),
            (RefusalKind::Denied, Exit::Denied),
        ] {
            let refusal = Refusal {
                code: ErrorCode::DaemonNotRunning,
                kind,
                sentence: String::new(),
            };
            assert_eq!(exit_of(&Unasked::Refused(refusal)), exit);
        }
        assert_eq!(
            exit_of(&Unasked::Runtime(std::io::Error::other("no"))),
            Exit::Failed
        );
    }
}
