//! `net`: fetching from a domain the manifest declares, and nothing else.
//!
//! Gated by `net`. `fetch` asks it first and answers `denied` to a plugin not
//! granted it. To one that is, the request is held to every rule of
//! `crate::net` — the domains are its manifest's, read from the manifest and
//! never from the plugin — before the transport is touched, and a request
//! that breaks one is refused as a value the plugin reads.
//!
//! **Where it runs decides what it may send.** A `GET` or a `HEAD` is sent
//! from wherever the plugin runs — `activate`, a callback, a command that only
//! reads. A request that leaves a mark on the host is sent only from a command
//! that changes state, and its unit records it as having left the machine
//! before it is sent: the note is what undoing the unit is told it cannot
//! reach, and a unit holding as many notes as it may refuses the request with
//! `[NC-PLUG-0017]`, unsent. **A request the transport refused before writing
//! any of it is taken back out** — a name that resolved to no public address,
//! a URL or a header its HTTP cannot write (`TransportError::sent_nothing`) —
//! and every other answer keeps it, one that did not come included, since the
//! request may have reached its host. A reviewer found a request never dialled
//! kept as having left, which made its step a barrier no undo reached past.
//!
//! **The transport is called on the blocking pool, under the host call's
//! deadline**: a transport that does not answer within `host_call_deadline_ms`
//! traps the plugin waiting on it, which kills it, and the thread is left to
//! the transport's own bounds. What the transport answers is held to the
//! catalog's bounds on an answer again, whatever it read.
//!
//! **A refusal is recorded once for its reason**, and the kernel logs each new
//! one as `net.refused` with the plugin's slot once the command it ran in
//! returns: the log grows with the reasons a plugin was refused for, not with
//! how often it asked.
//!
//! What this answers (`NOCTUA.md §2.4`): a request no rule allows reaching a
//! transport — every refusal is made before one is called — and a request that
//! changes something elsewhere going unrecorded in the unit that sent it, or
//! being sent from where nothing may change. What it does not answer: what
//! the host a plugin may reach does with it, and a unit that is let go after
//! its request left the machine, which takes the note with it while the
//! request stays sent.

use std::sync::Arc;

use nc_catalog::{Capability, ErrorCode};

use super::{HostState, refused};
use crate::bindings::KernelError;
use crate::bindings::noctua::kernel::net as world;
use crate::dispatch::{Frame, Refusal};
use crate::net::{self, Request, Transport as _};
use crate::supervisor::in_time;
use crate::{Environment, RefusalKind};

impl<E: Environment> world::Host for HostState<E> {
    async fn fetch(&mut self, req: world::Request) -> Result<world::Response, KernelError> {
        self.require(Capability::Net, "fetch")?;
        let asked = net::Asked {
            method: req.method,
            url: req.url,
            headers: req.headers,
            body: req.body,
        };
        let request = net::judged(asked, &self.domains, self.in_a_change())
            .map_err(|refused| self.refused_fetch(&refused))?;
        let marked = request
            .method()
            .leaves_a_mark()
            .then(|| net::note(&request));
        if marked.is_some() {
            self.noted(&request).map_err(|refusal| refused(&refusal))?;
        }
        let transport = Arc::clone(&self.transport);
        let sending = async move {
            Ok::<_, KernelError>(
                tokio::task::spawn_blocking(move || transport.fetch(&request))
                    .await
                    .unwrap_or_else(|_| {
                        Err(net::TransportError::Unavailable(
                            "the transport stopped before it answered".to_owned(),
                        ))
                    }),
            )
        };
        let sent = in_time("net#fetch", self.left(), sending).await?;
        if let (Err(error), Some(note)) = (&sent, &marked)
            && error.sent_nothing()
        {
            self.unnoted(note);
        }
        let response = sent
            .map_err(|error| net::failed(&error))
            .and_then(net::answered)
            .map_err(|refused| self.refused_fetch(&refused))?;
        Ok(world::Response {
            status: response.status,
            headers: response.headers,
            body: response.body,
        })
    }
}

impl<E: Environment> HostState<E> {
    /// Whether the plugin is running a command that changes state, inside
    /// the unit its call tree holds.
    fn in_a_change(&self) -> bool {
        self.frame()
            .is_some_and(|frame| frame.mutating() && frame.tree().unit().is_some())
    }

    /// Records `request` in the unit of the command running as having left
    /// the machine, before it is sent.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` when the unit holds as many notes as it
    /// may, and with `[NC-PLUG-0024]` when no command that changes state is
    /// running in a unit, which the request's own judgement refused first.
    fn noted(&self, request: &Request) -> Result<(), Refusal> {
        let not_in_a_change = || {
            Refusal::new(
                RefusalKind::Denied,
                ErrorCode::PluginReadOnlyFrame,
                format!(
                    "a {} is sent only from a command that changes state, inside its unit",
                    request.method().as_str()
                ),
            )
        };
        let frame = self
            .frame()
            .filter(|frame| Frame::mutating(frame))
            .ok_or_else(not_in_a_change)?;
        let mut tree = frame.tree();
        let unit = tree.unit_mut().ok_or_else(not_in_a_change)?;
        unit.left_the_machine(net::note(request))
    }

    /// Takes `note` back out of the unit of the command running, for a
    /// request the transport refused before it wrote any of it.
    fn unnoted(&self, note: &str) {
        let Some(frame) = self.frame() else {
            return;
        };
        let mut tree = frame.tree();
        if let Some(unit) = tree.unit_mut() {
            unit.not_sent(note);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nc_catalog::{Capability, ErrorCode, KERNEL};

    use crate::bindings::KernelError;
    use crate::bindings::noctua::kernel::net::{Host as _, Request};
    use crate::bindings::noctua::kernel::types;
    use crate::capability::GrantSet;
    use crate::dispatch::Frame;
    use crate::host::HostState;
    use crate::net::{Reason, Response, TransportError};
    use crate::{InMemory, RefusalKind};

    /// A request of `method` to `url`, with nothing else.
    fn request(method: &str, url: &str) -> Request {
        Request {
            method: method.to_owned(),
            url: url.to_owned(),
            headers: Vec::new(),
            body: None,
        }
    }

    /// A state granted `net`, fetching from `example.com`.
    fn granted() -> HostState<InMemory> {
        HostState::in_memory(GrantSet::of([Capability::Net]))
    }

    /// The case of the world's `error` `answer` refused with, and the code its
    /// sentence opens on.
    fn refusal_of(
        answer: &Result<crate::bindings::noctua::kernel::net::Response, KernelError>,
    ) -> Option<(RefusalKind, ErrorCode)> {
        let Err(KernelError::Refused(refused)) = answer else {
            return None;
        };
        let (kind, why) = match refused {
            types::Error::Denied(why) => (RefusalKind::Denied, why),
            types::Error::NotFound(why) => (RefusalKind::NotFound, why),
            types::Error::Invalid(why) => (RefusalKind::Invalid, why),
            types::Error::Conflict(why) => (RefusalKind::Conflict, why),
            types::Error::Unavailable(why) => (RefusalKind::Unavailable, why),
            types::Error::Internal(why) => (RefusalKind::Internal, why),
        };
        let code = why
            .strip_prefix('[')
            .and_then(|rest| rest.split_once(']'))
            .and_then(|(code, _)| ErrorCode::from_code(code))?;
        Some((kind, code))
    }

    #[tokio::test]
    async fn fetching_is_denied_without_the_capability() {
        let mut state = HostState::in_memory(GrantSet::none());
        let answer = state.fetch(request("GET", "https://example.com/")).await;
        assert_eq!(
            refusal_of(&answer),
            Some((RefusalKind::Denied, ErrorCode::PluginCapabilityDenied))
        );
        assert_eq!(state.transport.asked(), [] as [crate::net::Request; 0]);
    }

    #[tokio::test]
    async fn a_request_the_policy_allows_reaches_the_transport_and_its_answer_the_plugin() {
        let mut state = granted();
        state.transport.answering(Ok(Response {
            status: 302,
            headers: vec![("location".to_owned(), "https://example.org/".to_owned())],
            body: b"moved".to_vec(),
        }));
        let answer = state
            .fetch(request("GET", "https://example.com/a#here"))
            .await
            .expect("answered");
        assert_eq!(answer.status, 302, "a redirect is an answer");
        assert_eq!(
            answer.headers,
            [("location".to_owned(), "https://example.org/".to_owned())]
        );
        assert_eq!(answer.body, b"moved");
        let asked = state.transport.asked();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].url().as_str(), "https://example.com/a");
    }

    #[tokio::test]
    async fn a_refused_request_never_reaches_the_transport_and_is_recorded_once_for_its_reason() {
        let mut state = granted();
        for _ in 0..3 {
            let answer = state.fetch(request("GET", "https://example.org/")).await;
            assert_eq!(
                refusal_of(&answer),
                Some((RefusalKind::Denied, ErrorCode::PluginDomainNotDeclared))
            );
        }
        let answer = state.fetch(request("GET", "https://127.0.0.1/")).await;
        assert_eq!(
            refusal_of(&answer),
            Some((RefusalKind::Denied, ErrorCode::PluginRequestRefused))
        );
        let seen = state.transport.asked();
        assert!(seen.is_empty(), "a refusal reached it: {seen:?}");
        assert_eq!(
            state.fetches_refused(),
            [
                (Reason::Domain, ErrorCode::PluginDomainNotDeclared),
                (Reason::IpLiteral, ErrorCode::PluginRequestRefused)
            ],
            "each reason once, in the order first refused"
        );
        let _again = state.fetch(request("GET", "https://example.org/")).await;
        let seen = state.fetches_refused();
        assert!(
            seen.is_empty(),
            "a reason handed over is not handed over again: {seen:?}"
        );
    }

    #[tokio::test]
    async fn a_request_that_leaves_a_mark_is_noted_in_its_unit_before_it_is_sent() {
        let unit = nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        )
        .mint()
        .expect("a unit");
        for (frame, sent) in [
            (None, false),
            (Some(Frame::for_tests(false, Some(unit))), false),
            (Some(Frame::for_tests(true, None)), false),
            (Some(Frame::for_tests(true, Some(unit))), true),
        ] {
            let mut state = granted();
            let described = format!("{frame:?}");
            if let Some(frame) = frame {
                state.enter(frame);
            }
            let answer = state.fetch(request("POST", "https://example.com/")).await;
            assert_eq!(answer.is_ok(), sent, "{described}: {answer:?}");
            assert_eq!(
                state.transport.asked().len(),
                usize::from(sent),
                "{described}"
            );
            if !sent {
                assert_eq!(
                    refusal_of(&answer),
                    Some((RefusalKind::Denied, ErrorCode::PluginRequestRefused)),
                    "{described}"
                );
                continue;
            }
            let frame = state.frame().expect("entered");
            let tree = frame.tree();
            assert_eq!(
                tree.unit().expect("a unit").left(),
                [r#"{"host":"example.com","method":"POST","note":"left_the_machine"}"#],
                "the unit records what left the machine"
            );
        }
    }

    /// A `GET` and a `HEAD` sent from a command that changes state leave no
    /// note in its unit: a read takes nothing elsewhere that undoing the unit
    /// would have to be told of.
    ///
    /// **What keeps a unit that only read the web one undoing can reach**: a
    /// note of a request that left the machine is what undoing is told it
    /// cannot reach, so a read noted as one would stop undoing at a unit that
    /// changed nothing elsewhere. Found by a reviewer, who noted every request
    /// sent from a change and left every suite green.
    #[tokio::test]
    async fn a_read_sent_from_a_change_leaves_no_note() {
        let unit = nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        )
        .mint()
        .expect("a unit");
        for method in ["GET", "HEAD"] {
            let mut state = granted();
            state.enter(Frame::for_tests(true, Some(unit)));
            let answer = state.fetch(request(method, "https://example.com/")).await;
            assert!(answer.is_ok(), "{method}: {answer:?}");
            assert_eq!(state.transport.asked().len(), 1, "{method} was not sent");
            let frame = state.frame().expect("entered");
            let tree = frame.tree();
            assert!(
                tree.unit().expect("a unit").left().is_empty(),
                "{method} was noted as having left the machine"
            );
        }
    }

    #[tokio::test]
    async fn a_unit_holding_as_many_notes_as_it_may_sends_no_request() {
        let unit = nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        )
        .mint()
        .expect("a unit");
        let mut state = granted();
        state.enter(Frame::for_tests(true, Some(unit)));
        let bound = crate::units::notes_per_step();
        for _ in 0..bound {
            state
                .fetch(request("PUT", "https://example.com/"))
                .await
                .expect("within the bound");
        }
        let answer = state.fetch(request("PUT", "https://example.com/")).await;
        assert_eq!(
            refusal_of(&answer),
            Some((RefusalKind::Unavailable, ErrorCode::PluginBoundReached))
        );
        assert_eq!(
            u64::try_from(state.transport.asked().len()).expect("fits"),
            bound,
            "the request past the bound was sent"
        );
    }

    #[tokio::test]
    async fn what_the_transport_refuses_is_told_by_its_own_code() {
        for (answered, kind, code, reason) in [
            (
                Err(TransportError::PrivateAddress {
                    host: "example.com".to_owned(),
                }),
                RefusalKind::Denied,
                ErrorCode::PluginRequestRefused,
                Some(Reason::PrivateAddress),
            ),
            (
                Err(TransportError::Unavailable("down".to_owned())),
                RefusalKind::Unavailable,
                ErrorCode::PluginNetworkUnavailable,
                None,
            ),
            (
                Ok(Response {
                    status: 200,
                    headers: Vec::new(),
                    body: vec![
                        0;
                        usize::try_from(KERNEL.net_response_body_bytes).expect("fits") + 1
                    ],
                }),
                RefusalKind::Unavailable,
                ErrorCode::PluginResponseTooLarge,
                Some(Reason::TooLarge),
            ),
        ] {
            let mut state = granted();
            state.transport.answering(answered);
            let answer = state.fetch(request("GET", "https://example.com/")).await;
            assert_eq!(refusal_of(&answer), Some((kind, code)), "{code}");
            assert_eq!(
                state.fetches_refused().first().map(|(reason, _)| *reason),
                reason,
                "{code}"
            );
        }
    }

    /// A transport that stops before it answers — it panicked on the blocking
    /// pool — is a request that did not complete, and the plugin lives on.
    #[tokio::test]
    async fn a_transport_that_stops_before_it_answers_is_a_request_that_did_not_complete() {
        /// A transport that panics whatever it is handed.
        #[derive(Debug)]
        struct Breaking;

        impl crate::net::Transport for Breaking {
            fn fetch(&self, _: &crate::net::Request) -> Result<Response, TransportError> {
                panic!("the transport broke, as the test made it");
            }
        }

        /// The kernel over a [`Breaking`] transport.
        #[derive(Debug)]
        struct OverBreaking;

        impl crate::Environment for OverBreaking {
            type Store = crate::backend::MemoryBackend;
            type Blobs = crate::blobs::MemoryBlobs;
            type Journal = nc_journal::InMemory;
            type Clock = nc_ids::SystemClock;
            type Directory = nc_authz::MemoryDirectory;
            type Log = crate::logging::Captured;
            type Transport = Breaking;
        }

        let mut state: HostState<OverBreaking> = HostState::new(
            GrantSet::of([Capability::Net]),
            (
                HostState::<crate::InMemory>::home(),
                "tests:door".to_owned(),
            ),
            vec!["example.com".to_owned()],
            std::sync::Arc::new(crate::backend::MemoryBackend::default()),
            std::sync::Arc::new(crate::blobs::MemoryBlobs::default()),
            std::sync::Arc::new(Breaking),
            crate::limits::MemoryGuard::new(&crate::limits::Limits::catalog()),
            crate::host::log::SharedRing::default(),
        );
        let answer = state.fetch(request("GET", "https://example.com/")).await;
        assert_eq!(
            refusal_of(&answer),
            Some((
                RefusalKind::Unavailable,
                ErrorCode::PluginNetworkUnavailable
            )),
            "{answer:?}"
        );
        let seen = state.fetches_refused();
        assert!(
            seen.is_empty(),
            "a request that did not complete is not a refusal to log: {seen:?}"
        );
    }

    /// Of two requests a unit noted alike, the one the transport refused
    /// unwritten is taken back and the one that left the machine is kept.
    ///
    /// **Written by a reviewer (wave 5, net)**: `Ambient::not_sent` taking
    /// back every note equal to the refused one left the whole gate green —
    /// notes of one method to one host are equal strings — and the step of a
    /// `POST` that reached its host became one undo reaches past.
    #[tokio::test]
    async fn of_two_requests_noted_alike_only_the_one_refused_unwritten_is_taken_back() {
        let unit = nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        )
        .mint()
        .expect("a unit");
        let mut state = granted();
        state.enter(Frame::for_tests(true, Some(unit)));
        state
            .fetch(request("POST", "https://example.com/"))
            .await
            .expect("the first request is answered");
        state
            .transport
            .answering(Err(TransportError::PrivateAddress {
                host: "example.com".to_owned(),
            }));
        let refused = state.fetch(request("POST", "https://example.com/")).await;
        assert_eq!(
            refusal_of(&refused),
            Some((RefusalKind::Denied, ErrorCode::PluginRequestRefused))
        );
        assert_eq!(state.transport.asked().len(), 2);
        let frame = state.frame().expect("entered");
        let tree = frame.tree();
        assert_eq!(
            tree.unit().expect("a unit").left(),
            [r#"{"host":"example.com","method":"POST","note":"left_the_machine"}"#],
            "the request that left the machine lost its note with the one never sent"
        );
    }

    /// A transport that never answers is abandoned at the host call's
    /// deadline, and the call traps.
    ///
    /// **On a paused clock moved by hand**: a task on the blocking pool keeps
    /// tokio's clock from running on by itself, so the test moves it once the
    /// transport is reached, and the deadline is all that ends the call.
    #[tokio::test(start_paused = true)]
    async fn a_transport_that_never_answers_is_abandoned_at_the_host_calls_deadline() {
        let mut state = granted();
        let stall = state.transport.stall();
        let transport = std::sync::Arc::clone(&state.transport);
        let started = tokio::time::Instant::now();
        let fetching = state.fetch(request("GET", "https://example.com/"));
        tokio::pin!(fetching);
        tokio::select! {
            biased;
            answered = &mut fetching => panic!("answered before the transport was reached: {answered:?}"),
            () = transport.reached() => {}
        }
        tokio::time::advance(Duration::from_millis(KERNEL.host_call_deadline_ms)).await;
        let answer = fetching.await;
        assert!(
            matches!(&answer, Err(KernelError::Trap(fault)) if fault.to_string().contains("`net#fetch` did not answer within")),
            "{answer:?}"
        );
        assert_eq!(
            started.elapsed(),
            Duration::from_millis(KERNEL.host_call_deadline_ms)
        );
        drop(stall);
    }
}
