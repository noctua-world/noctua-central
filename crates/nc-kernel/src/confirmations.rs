//! Calls held for a person's confirmation in the desktop interface
//! (`NOCTUA.md §11.1`, `§11.7`).
//!
//! **A command that cannot be undone runs only with a confirmation the kernel
//! issued for that principal, that command and those arguments** (the decision
//! point's precondition, `crate::dispatch`) — **and so does MCP switched on**
//! (`crate::settings::switches_mcp_on`), which can be undone but lets an agent
//! in, under codes of its own: `[NC-KERN-0032]`, `[NC-KERN-0033]` and
//! `[NC-KERN-0034]` where this module names `[NC-KERN-0006]`,
//! `[NC-KERN-0029]` and `[NC-KERN-0030]`, recorded as
//! `kernel.mcp_on_needs_confirmation`. Through
//! [`crate::Kernel::dispatch`] such a call is refused with `[NC-KERN-0006]`, as
//! it always was. Through [`crate::Kernel::dispatch_held`] — what the daemon
//! asks when the desktop interface it started is attached — the call is
//! **held** instead, in this order:
//!
//! 1. **Decided as every dispatch is**, up to the precondition: a call refused
//!    before it — its name, its arguments, the decision point — is refused and
//!    journaled there, and never put before the person. One the decision point
//!    allowed and nothing confirmed is journaled nowhere and run nowhere yet.
//! 2. **Held**, within `confirmations_pending_max` — one past it is decided
//!    again at once and refused with `[NC-KERN-0006]`, as with no desktop —
//!    under a serial of its own, which [`crate::Kernel::pending_moves`] tells
//!    whoever watches and `confirmations.list` and `confirmations.read` show:
//!    the command, its arguments, the person whose confirmation it waits for,
//!    the agent that asked when an agent did, and its deadline. It waits for
//!    the person's answer, [`crate::Kernel::settle`], up to
//!    `confirmation_ttl_ms`, holding none of the kernel's locks, so every other
//!    call runs meanwhile.
//! 3. **Decided again from the start**: confirmed, the kernel issues the
//!    person's confirmation of that very call — bound to the person the tree
//!    acts for, the command by its name and by the plugin holding it, and the
//!    arguments' canonical text, redeemed by that call alone and never handed
//!    to anyone — and dispatches it once more, so a grant ended, a plugin
//!    unloaded or MCP switched off while it waited refuses it as it would any
//!    call; declined, it is refused with `[NC-KERN-0030]`; and with no answer
//!    before the wait lapses — or once whoever asked it stops waiting, the
//!    daemon stopping — with `[NC-KERN-0029]`, both recorded as
//!    `kernel.irreversible_needs_confirmation`. **Each call is journaled once,
//!    and spends once**: a session's budget is spent when the call is first
//!    decided, and its decision again is made on what it had spent then.
//!
//! **An agent's call is held the same way, and the agent is never
//! confirmed** (`NOCTUA.md §14.6`): what the person confirms is the call the
//! kernel holds, bound to the person and redeemed by the kernel for that call
//! — no session holds a token, none is bound to an agent, and the plugin's
//! `context` still answers that no person confirmed a call in an agent's tree.
//! The decision point is told the person confirmed it, which is what
//! `core.irreversible_needs_a_human` asks of an action through an agent.
//!
//! **Nothing here answers a held call but [`crate::Kernel::settle`], and no
//! command reaches it**: `confirmations.list` and `confirmations.read` are
//! reads on `Feature::"interface"`, and there is no command that confirms —
//! the one act of the interface that is not a command (`NOCTUA.md §12.1`),
//! since a dispatched command, a CLI call and an agent's call would each be a
//! confirmation nobody gave. Who may call `settle` is the daemon's to decide:
//! the desktop interface it started, proven, on its own connection
//! (`apps/daemon/src/desktop.rs`).
//!
//! What this answers (`NOCTUA.md §2.4`): a call that cannot be undone run
//! without the person's answer to that call — the answer settles one serial,
//! the confirmation it issues is bound to the call's person, command, holder
//! and arguments and redeemed by that call alone within `confirmation_ttl_ms`;
//! a call held forever, or calls held without bound; and a call held while it
//! is decided again, journaled twice or spent twice. What it does not answer:
//! who answered — the kernel takes [`crate::Kernel::settle`] as the person's
//! answer, and whoever holds the kernel is trusted to call it for nobody but
//! the person.

use core::future::Future;
use core::time::Duration;
use std::collections::BTreeMap;
use std::sync::Arc;

use nc_authz::Principal;
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::Clock as _;
use nc_schema::descriptor::Reversibility;

use crate::dispatch::{self, Asker, Came, Failure, Refusal, RefusalKind, Unconfirmed};
use crate::session::Acting;
use crate::{Environment, Shared};

/// The command that lists the calls held for a confirmation.
pub(crate) const LIST: &str = "confirmations.list";

/// The command that reads one call held for a confirmation.
pub(crate) const READ: &str = "confirmations.read";

/// The serial `confirmations.read` reads.
const SERIAL: &str = "serial";

/// How a person answered a call held for their confirmation, in the desktop
/// interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settled {
    /// Confirmed: the call is decided again, and runs if it is allowed.
    Confirmed,
    /// Declined: the call is refused with `[NC-KERN-0030]`.
    Declined,
}

/// A call the decision point allowed and nothing confirmed, which the kernel
/// holds for the person's confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeldCall {
    /// The command's name.
    name: String,
    /// The identity of the plugin that held the command when the call was
    /// decided: none for the kernel's own.
    holder: Option<String>,
    /// The arguments' canonical text.
    canonical: String,
    /// The person whose confirmation it waits for: who acts, or the person an
    /// agent's session acts for.
    person: Principal,
    /// The agent that asked, when an agent's session did.
    agent: Option<Principal>,
    /// What the session had spent when the call was first decided.
    spent: u64,
    /// What the command is declared as: `irreversible`, or `write` for MCP
    /// switched on (`crate::settings::switches_mcp_on`).
    reversibility: Reversibility,
}

impl HeldCall {
    /// The call `name`, held by `holder`, with `canonical` arguments, as
    /// `acting` asked it, its session having spent `spent`.
    pub(crate) fn of(
        name: String,
        holder: Option<String>,
        canonical: String,
        acting: &Acting,
        spent: u64,
    ) -> Self {
        Self {
            name,
            holder,
            canonical,
            person: acting.whose_history().clone(),
            agent: acting.via_agent().then(|| acting.principal().clone()),
            spent,
            reversibility: Reversibility::Irreversible,
        }
    }

    /// The same call, its command declared as `reversibility`.
    pub(crate) const fn declared(mut self, reversibility: Reversibility) -> Self {
        self.reversibility = reversibility;
        self
    }
}

/// Every call held now, by its serial.
///
/// **Bounded by `confirmations_pending_max`**, and each for
/// `confirmation_ttl_ms` at most, so what a kernel holds is bounded however
/// many ask.
#[derive(Debug, Default)]
pub(crate) struct Pending {
    /// The last serial given.
    next: u64,
    /// Each call held, waiting for its answer.
    held: BTreeMap<u64, Waiting>,
}

/// One call held: the call, when its wait ends, and where its answer goes.
#[derive(Debug)]
struct Waiting {
    call: HeldCall,
    /// When its wait ends, in milliseconds since the Unix epoch on the
    /// kernel's clock.
    deadline_ms: u64,
    answer: tokio::sync::oneshot::Sender<Settled>,
}

/// `name` with `args` dispatched for `asker` as the top of a call tree,
/// **a call that cannot be undone held for the person's confirmation**, as
/// this module's header says, until `lapsed` ends its wait early.
///
/// # Errors
///
/// What [`crate::Kernel::dispatch`] refuses with, and for a call held:
/// `[NC-KERN-0006]` past `confirmations_pending_max`, `[NC-KERN-0030]`
/// declined, `[NC-KERN-0029]` not answered in time, and what the decision
/// point refuses when it decides the call again.
pub(crate) async fn dispatched<E: Environment>(
    shared: &Arc<Shared<E>>,
    asker: Asker<'_>,
    name: &str,
    args: &str,
    lapsed: impl Future<Output = ()>,
) -> Result<String, Refusal> {
    let asking = asker.asking();
    let call = match dispatch::spawned(shared, asking.clone().holding(), name, args).await {
        Came::Answered(answered) => return answered,
        Came::Held(call) => call,
    };
    let spent = call.spent;
    let Some((serial, answer)) = hold(shared, &call) else {
        let again = asking.again(
            Unconfirmed::Refused(ErrorCode::KernelUnconfirmed),
            None,
            spent,
        );
        return dispatch::answer_of(dispatch::spawned(shared, again, name, args).await, name);
    };
    let wait = Duration::from_millis(KERNEL.confirmation_ttl_ms);
    let settled = tokio::select! {
        settled = answer => settled.ok(),
        () = tokio::time::sleep(wait) => None,
        () = lapsed => None,
    };
    let_go(shared, serial);
    let again = match settled {
        Some(Settled::Confirmed) => {
            let issued = dispatch::lock(&shared.issued).issue_held(
                &call.person,
                (&call.name, call.holder.as_deref()),
                &call.canonical,
                shared.clock.now_ms(),
            );
            asking.again(
                Unconfirmed::Refused(ErrorCode::KernelUnconfirmed),
                Some(issued),
                spent,
            )
        }
        Some(Settled::Declined) => {
            asking.again(Unconfirmed::Refused(ErrorCode::KernelDeclined), None, spent)
        }
        None => asking.again(
            Unconfirmed::Refused(ErrorCode::KernelNotConfirmedInTime),
            None,
            spent,
        ),
    };
    dispatch::answer_of(dispatch::spawned(shared, again, name, args).await, name)
}

/// `call` held under a serial of its own, told to whoever watches, and where
/// its answer arrives; nothing where `confirmations_pending_max` are held.
fn hold<E: Environment>(
    shared: &Shared<E>,
    call: &HeldCall,
) -> Option<(u64, tokio::sync::oneshot::Receiver<Settled>)> {
    let mut pending = dispatch::lock(&shared.pending);
    let most = usize::try_from(KERNEL.confirmations_pending_max).unwrap_or(0);
    if pending.held.len() >= most {
        return None;
    }
    pending.next += 1;
    let serial = pending.next;
    let (answer, answered) = tokio::sync::oneshot::channel();
    pending.held.insert(
        serial,
        Waiting {
            call: call.clone(),
            deadline_ms: shared
                .clock
                .now_ms()
                .saturating_add(KERNEL.confirmation_ttl_ms),
            answer,
        },
    );
    told(shared, &pending);
    Some((serial, answered))
}

/// The call held as `serial` let go, its wait over, told to whoever watches
/// where it was still held.
fn let_go<E: Environment>(shared: &Shared<E>, serial: u64) {
    let mut pending = dispatch::lock(&shared.pending);
    if pending.held.remove(&serial).is_some() {
        told(shared, &pending);
    }
}

/// Tells whoever watches the serials held now.
fn told<E: Environment>(shared: &Shared<E>, pending: &Pending) {
    shared
        .pending_moves
        .send_replace(pending.held.keys().copied().collect());
}

/// The person's answer, `settled`, to the call held as `serial`: it is let go
/// at once, and decided again as the answer says.
///
/// # Errors
///
/// `not-found` with `[NC-KERN-0031]` when no call is held as `serial` —
/// answered already, lapsed, or never held — and nothing is answered.
pub(crate) fn settle<E: Environment>(
    shared: &Shared<E>,
    serial: u64,
    settled: Settled,
) -> Result<(), Refusal> {
    let waiting = {
        let mut pending = dispatch::lock(&shared.pending);
        let waiting = pending.held.remove(&serial);
        if waiting.is_some() {
            told(shared, &pending);
        }
        waiting
    };
    match waiting {
        Some(waiting) => {
            // A call whose asker stopped waiting a moment ago has nobody to
            // hand the answer to, and is decided as lapsed.
            let _ = waiting.answer.send(settled);
            Ok(())
        }
        None => Err(nothing_held(serial)),
    }
}

/// The refusal of a serial no call is held as.
fn nothing_held(serial: u64) -> Refusal {
    Refusal::new(
        RefusalKind::NotFound,
        ErrorCode::KernelNothingHeld,
        format!(
            "no call is held for a person's confirmation as {serial}: it was answered, it \
             lapsed, or it was never held"
        ),
    )
}

/// `confirmations.list`: every call held now, in the order they were held.
///
/// `{"pending":[…]}`, each with its `serial`, its `command`, the `person`
/// whose confirmation it waits for, the `agent` that asked or `null`, its
/// `reversibility` as its command is declared — `irreversible`, or `write`
/// for MCP switched on — and its `deadline_ms`. Never its arguments, which `confirmations.read` answers one
/// call at a time, so the list stays within a frame whatever each holds.
pub(crate) fn listed<E: Environment>(shared: &Shared<E>) -> String {
    let pending = dispatch::lock(&shared.pending);
    Value::object([(
        "pending",
        Value::Array(
            pending
                .held
                .iter()
                .map(|(serial, waiting)| Value::Object(summary(*serial, waiting)))
                .collect(),
        ),
    )])
    .to_canonical()
}

/// `confirmations.read`: the call held as the `serial` `args` names, as
/// `confirmations.list` shows it and with its `args`.
///
/// # Errors
///
/// `not-found` with `[NC-KERN-0031]` when no call is held as that serial.
pub(crate) fn read<E: Environment>(
    shared: &Shared<E>,
    args: &serde_json::Value,
) -> Result<String, Failure> {
    let serial = args
        .get(SERIAL)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let pending = dispatch::lock(&shared.pending);
    let Some(waiting) = pending.held.get(&serial) else {
        return Err(Failure::new(nothing_held(serial), "nothing-held"));
    };
    let mut fields = summary(serial, waiting);
    fields.insert(
        "args".to_owned(),
        nc_canon::parse(&waiting.call.canonical).unwrap_or(Value::Null),
    );
    Ok(Value::Object(fields).to_canonical())
}

/// What a list shows of the call held as `serial`.
fn summary(serial: u64, waiting: &Waiting) -> BTreeMap<String, Value> {
    let count = |value: u64| Value::Integer(i64::try_from(value).unwrap_or(i64::MAX));
    [
        ("serial", count(serial)),
        ("command", Value::text(waiting.call.name.as_str())),
        ("person", Value::text(waiting.call.person.id().as_str())),
        (
            "agent",
            waiting
                .call
                .agent
                .as_ref()
                .map_or(Value::Null, |agent| Value::text(agent.id().as_str())),
        ),
        (
            "reversibility",
            Value::text(match waiting.call.reversibility {
                Reversibility::Read => "read",
                Reversibility::Write => "write",
                Reversibility::Irreversible => "irreversible",
            }),
        ),
        ("deadline_ms", count(waiting.deadline_ms)),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{LIST, READ, SERIAL};

    /// The declared confirmations commands are the ones this module
    /// implements: the arguments each reads, both ways, and the decision each
    /// is made under — `Read` on the interface's feature, changing nothing,
    /// so neither opens a unit and none is a step.
    #[test]
    fn the_declared_confirmations_commands_are_the_ones_this_module_implements() {
        for (name, reads) in [(LIST, &[][..]), (READ, &[SERIAL][..])] {
            let declared =
                nc_catalog::command(name).unwrap_or_else(|| panic!("`{name}` is declared"));
            let takes: BTreeSet<&str> = declared.arguments.iter().copied().collect();
            let read: BTreeSet<&str> = reads.iter().copied().collect();
            assert_eq!(takes, read, "`{name}` declares {:?}", declared.arguments);
            assert_eq!(declared.arguments.len(), reads.len(), "`{name}`");
            assert_eq!(
                (
                    declared.action,
                    declared.resource_selector,
                    declared.mutating
                ),
                ("Read", "feature:interface", false),
                "`{name}`"
            );
            assert!(
                matches!(declared.reversibility, nc_catalog::Reversibility::Read),
                "`{name}`"
            );
        }
        assert_eq!(
            (LIST, READ, SERIAL),
            ("confirmations.list", "confirmations.read", "serial")
        );
    }
}
