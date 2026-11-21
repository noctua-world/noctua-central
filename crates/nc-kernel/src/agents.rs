//! The agents a person makes: `agents.create`, `agents.list`, `agents.grant`
//! and `agents.revoke` (`NOCTUA.md §14.6`).
//!
//! **A person's delegation, as four commands of the kernel's own.** An agent
//! is an identity of its own, minted by the machine's one minter
//! (`nc_journal::Journal::mint_identifier`), acting for the person who made it
//! and for no one else, with a label, a budget its sessions spend, and grants
//! each scoped to one feature of the realm, given for a reason and for at most
//! `[kernel] agent_grant_minutes_max`. All four are decided on
//! `Feature::"agents"` — `Write`, `Read`, `Grant` and `Revoke` — which the
//! family pack gives every person of the realm and no agent or device
//! (`family.a_member_keeps_their_agents`). What the directory holds is
//! `nc_authz::LocalDirectory`'s, which a daemon opens over the profile's own
//! database: this module asks it, and never holds an agent itself.
//!
//! **What changes is journaled before it is made.** `agents.create`,
//! `agents.grant` and `agents.revoke` change the profile's database outside
//! every unit, so nothing a journal refuses afterwards could take them back.
//! Each is planned first ([`planned`]): a label or a reason holding a
//! character a reader cannot see is refused before anything is minted, the
//! identifier minted, the change built on the kernel's clock and rehearsed by
//! the directory within what the call tree has left — a refusal there fails
//! the dispatch, journaled with its word and none of the caller's text. Only
//! then does the decision point journal it as allowed, its entry naming what
//! the change made — the agent, the grant, the feature as an entity, the
//! window in milliseconds, the grant it ends or the grants it ended — and
//! never the label or the reason; and only once that entry has landed is the
//! change made ([`made`]). Its unit notes it as having left the machine before
//! it runs, so a step that ran one is one undo stops at (`crate::history`):
//! the reverse of a grant is `agents.revoke`, and no agent is ever deleted.
//!
//! **The three that change run only at the top of a call tree**
//! (`crate::registry::only_at_the_top`): dispatched from inside a command — a
//! plugin run as the person, a timer's or an event's callback — each is refused
//! with `[NC-KERN-0025]` before it is decided. `agents.list` runs anywhere,
//! since it changes nothing, and answers the asker's own agents alone.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! Who may delegate, and what: a person, by a decided and journaled dispatch of
//! their own, never a plugin, a timer or an event in their name, and never an
//! agent or a device, which the permit refuses. The operator is the person who
//! asked, written by the kernel and never an argument, and only the agent's
//! operator grants it or ends a grant (`[NC-AI-0002]`); an agent is named by
//! the identifier `agents.create` answered, matched whole and never parsed, and
//! an identifier no agent is held under is `[NC-AI-0001]`; each table is
//! refused past its bound (`[NC-AI-0003]`). Every refusal of the directory's
//! reaches the caller as its own kind, code and word ([`refused`]). A grant
//! runs for the window it was given and no longer, which the decision point
//! decides against its clock at every call. **What it does not answer:** a
//! process running as the same user, which can run the CLI as the person,
//! name any of their agents or edit the profile's database; an identifier as
//! a secret, which it is not; and a change made with its entry landed whose
//! keeping the database then refused, which the unit's close says did not
//! finish — a new agent or grant is then held nowhere, and an ending holds
//! until the kernel stops.

use std::collections::BTreeMap;
use std::time::Instant;

use nc_authz::{
    Change, ChangeKind, ChangeRefused, Directory as _, EntityRef, HeldAgent, Principal,
};
use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_ids::Clock as _;

use crate::dispatch::{Failure, Refusal, RefusalKind, Waits, within};
use crate::{Environment, Shared};

/// The command that makes an agent.
pub(crate) const CREATE: &str = "agents.create";

/// The command that lists the asker's agents.
pub(crate) const LIST: &str = "agents.list";

/// The command that grants an agent a feature.
pub(crate) const GRANT: &str = "agents.grant";

/// The command that ends an agent's grants.
pub(crate) const REVOKE: &str = "agents.revoke";

/// What a person calls an agent they make.
const LABEL: &str = "label";

/// What an agent's sessions may spend.
const BUDGET: &str = "budget";

/// The agent a grant or an ending names, by the identifier it was made under.
const AGENT: &str = "agent";

/// The feature a grant is for, or an ending ends.
const FEATURE: &str = "feature";

/// For how many minutes a grant runs.
const MINUTES: &str = "minutes";

/// Why a grant is given, in its giver's words.
const REASON: &str = "reason";

/// The entity type every grant to an agent is scoped to.
const SCOPE: &str = "Feature";

/// Milliseconds in a minute.
const MINUTE_MS: u64 = 60_000;

/// How many characters of what refused a change its sentence repeats.
const QUOTED: usize = 256;

/// A change the directory would make now, and what follows from it.
///
/// What its entry notes beside the command and the feature, and what the
/// command answers once it is made.
#[derive(Debug)]
pub(crate) struct Planned {
    /// The change, rehearsed.
    change: Change,
    /// What its entry records of it: identifiers, entities and instants.
    noted: BTreeMap<String, Value>,
    /// What the command answers once the change is made.
    answer: String,
}

impl Planned {
    /// What the change's entry records beside its command and its feature.
    pub(crate) fn noted(&self) -> BTreeMap<String, Value> {
        self.noted.clone()
    }
}

/// The failure a refusal of a change is answered with.
///
/// Its own kind — each `nc_authz::ChangeKind` is the `RefusalKind` of the same
/// name — and the code and the word the refusal carries, said in the kernel's
/// words with nothing of the caller's text.
pub(crate) fn refused(refusal: &ChangeRefused) -> Failure {
    let kind = match refusal.kind() {
        ChangeKind::NotFound => RefusalKind::NotFound,
        ChangeKind::Denied => RefusalKind::Denied,
        ChangeKind::Conflict => RefusalKind::Conflict,
        ChangeKind::Invalid => RefusalKind::Invalid,
        ChangeKind::Unavailable => RefusalKind::Unavailable,
    };
    Failure::new(
        Refusal::new(kind, refusal.code(), sentence_of(refusal)),
        refusal.word(),
    )
}

/// What a refusal of a change says, in the kernel's words.
///
/// What another crate said — a store's words, a directory's — is cut and
/// escaped before it is repeated.
fn sentence_of(refusal: &ChangeRefused) -> String {
    let quoted = |said: &str| crate::registry::quoted_up_to(said, QUOTED);
    match refusal {
        ChangeRefused::NoSuchAgent => {
            "no agent the asking person made is held under that identifier, so nothing was \
             changed"
                .to_owned()
        }
        ChangeRefused::NotTheOperator => {
            "an agent is made only for a person, and only the person it acts for grants it a \
             feature or ends a grant, so nothing was changed"
                .to_owned()
        }
        ChangeRefused::Full { table } => format!(
            "the profile holds as many rows of `{table}` as its bound allows, and none is ever \
             deleted to make room, so nothing was changed"
        ),
        ChangeRefused::Malformed(why) => format!(
            "the change is not one an agent or a grant may hold ({}), so nothing was changed",
            quoted(why)
        ),
        ChangeRefused::Unkept(unkept) => format!(
            "the change was not kept where the next start reads it back ({}): a new agent or \
             grant is held nowhere, and an ending holds only until the kernel stops",
            quoted(&unkept.why)
        ),
        ChangeRefused::Directory(error) => format!(
            "the directory the decision point reads could not answer for the change ({}), so \
             nothing was changed",
            quoted(&error.to_string())
        ),
    }
}

/// The failure of a command asked with arguments its schema refuses.
///
/// Reached by no dispatch the schema let through, and refused again here
/// rather than read as a default.
fn invalid(command: &str, said: &str) -> Failure {
    Failure::new(
        Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::ValidationArgumentsInvalid,
            format!("`{command}` was asked {said}, so nothing was changed"),
        ),
        "arguments-invalid",
    )
}

/// The text argument `name` of `args`.
fn text<'a>(command: &str, args: &'a serde_json::Value, name: &str) -> Result<&'a str, Failure> {
    args.get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid(command, &format!("with no text `{name}`")))
}

/// The whole-number argument `name` of `args`.
fn number(command: &str, args: &serde_json::Value, name: &str) -> Result<u64, Failure> {
    args.get(name)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| invalid(command, &format!("with no whole number `{name}`")))
}

/// Refuses a label or a reason holding a character a reader cannot see.
///
/// The schema bounds their length and cannot say this; and the directory
/// refuses them too, once an identifier is minted — so it is refused here
/// first, and nothing is minted for it.
fn judged(command: &str, args: &serde_json::Value) -> Result<(), Failure> {
    for name in [LABEL, REASON] {
        if args
            .get(name)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|said| said.chars().any(nc_schema::unseen))
        {
            return Err(invalid(
                command,
                &format!("with a `{name}` holding a character a reader cannot see"),
            ));
        }
    }
    Ok(())
}

/// The feature `id` names, as a grant's scope.
fn feature(id: &str) -> EntityRef {
    EntityRef {
        type_name: SCOPE.to_owned(),
        id: id.to_owned(),
    }
}

/// An identifier the minter gave, or a count, as an entry or an answer holds it.
fn integer(counted: u64) -> Value {
    Value::Integer(i64::try_from(counted).unwrap_or(i64::MAX))
}

/// A new identifier from the machine's one minter, the journal's.
///
/// # Errors
///
/// `unavailable` in the journal's own code, word `no-identifier`, when the
/// minter mints nothing.
async fn minted<E: Environment>(
    shared: &Shared<E>,
    command: &str,
    waits: Waits,
) -> Result<nc_ids::Snowflake, Failure> {
    waits
        .on(async { shared.journal.lock().await.mint_identifier() })
        .await
        .map_err(|error| {
            Failure::new(
                Refusal::new(
                    RefusalKind::Unavailable,
                    error.code(),
                    format!("`{command}` changed nothing, because no identifier was minted for it"),
                ),
                "no-identifier",
            )
        })
}

/// The agents `asker` made, as the directory holds them now, within what the
/// call tree that began at `began` has left.
async fn held_for<E: Environment>(
    shared: &Shared<E>,
    asker: &Principal,
    began: Instant,
) -> Result<Vec<HeldAgent>, Failure> {
    within(
        began,
        shared
            .directory
            .agents(shared.realm.as_str(), &asker.entity()),
    )
    .await
    .map_err(|error| refused(&ChangeRefused::Directory(error)))
}

/// The agent of `asker`'s the caller named by `agent`, as the directory lists it.
///
/// Matched whole against the identifier each is held under; the directory
/// rehearsed the change already, so an agent it does not list is one it would
/// not change either.
fn the_one<'a>(held: &'a [HeldAgent], agent: &str) -> Result<&'a HeldAgent, Failure> {
    held.iter()
        .find(|one| one.agent.id().as_str() == agent)
        .ok_or_else(|| refused(&ChangeRefused::NoSuchAgent))
}

/// A change to `asker`'s agents as `command` asks for it with `args`, rehearsed
/// by the directory — what it makes, what its entry notes and what it answers.
///
/// **In this order**: a label or a reason a reader cannot see all of is
/// refused before anything is minted; the change is built ([`asked_for`]); and
/// the directory rehearses it within what the call tree that began at `began`
/// has left. Nothing is changed here.
///
/// # Errors
///
/// `invalid` with `[NC-VAL-0002]` for a label or a reason holding a character a
/// reader cannot see, and for arguments the schema would have refused;
/// `unavailable` in the journal's code when no identifier is minted; and the
/// directory's refusal of the change, through [`refused`].
pub(crate) async fn planned<E: Environment>(
    shared: &Shared<E>,
    asker: &Principal,
    command: &str,
    args: &serde_json::Value,
    waits: Waits,
    began: Instant,
) -> Result<Planned, Failure> {
    judged(command, args)?;
    let change = asked_for(shared, asker, command, args, waits).await?;
    let rehearsed = within(began, async {
        Ok(shared
            .directory
            .rehearse(shared.realm.as_str(), &change)
            .await)
    })
    .await;
    match rehearsed {
        Ok(Ok(())) => {}
        Ok(Err(refusal)) => return Err(refused(&refusal)),
        Err(error) => return Err(refused(&ChangeRefused::Directory(error))),
    }
    let (noted, answer) = match &change {
        Change::Make {
            agent,
            label,
            budget,
            ..
        } => {
            let agent = Value::text(agent.to_crockford());
            (
                BTreeMap::from([(AGENT.to_owned(), agent.clone())]),
                Value::object([
                    (AGENT, agent),
                    (BUDGET, integer(u64::from(*budget))),
                    (LABEL, Value::text(label.as_str())),
                ])
                .to_canonical(),
            )
        }
        Change::Grant { .. } => granted(&held_for(shared, asker, began).await?, &change)?,
        Change::End { agent, scope, .. } => {
            let held = held_for(shared, asker, began).await?;
            let one = the_one(&held, agent)?;
            let ended = Value::Array(
                one.grants
                    .iter()
                    .filter(|live| scope.as_ref().is_none_or(|scope| live.scope == *scope))
                    .map(|live| Value::text(live.grant.as_str()))
                    .collect(),
            );
            let agent = Value::text(one.agent.id().as_str());
            (
                BTreeMap::from([
                    (AGENT.to_owned(), agent.clone()),
                    ("ended".to_owned(), ended.clone()),
                ]),
                Value::object([(AGENT, agent), ("ended", ended)]).to_canonical(),
            )
        }
    };
    Ok(Planned {
        change,
        noted,
        answer,
    })
}

/// The change `command` asks for with `args`, made by `asker`.
///
/// The agent's or the grant's identifier minted, the kernel's clock read, and
/// the operator or the giver the asker, never an argument.
///
/// # Errors
///
/// `invalid` for arguments the schema would have refused, and `unavailable` in
/// the journal's code when no identifier is minted.
async fn asked_for<E: Environment>(
    shared: &Shared<E>,
    asker: &Principal,
    command: &str,
    args: &serde_json::Value,
    waits: Waits,
) -> Result<Change, Failure> {
    match command {
        CREATE => {
            let budget = u32::try_from(number(command, args, BUDGET)?)
                .map_err(|_| invalid(command, "with a budget no agent holds"))?;
            let label = text(command, args, LABEL)?.to_owned();
            Ok(Change::Make {
                agent: minted(shared, command, waits).await?,
                operator: asker.clone(),
                label,
                budget,
            })
        }
        GRANT => {
            let agent = text(command, args, AGENT)?.to_owned();
            let scope = feature(text(command, args, FEATURE)?);
            let reason = text(command, args, REASON)?.to_owned();
            let minutes = number(command, args, MINUTES)?;
            let grant = minted(shared, command, waits).await?;
            let from_ms = shared.clock.now_ms();
            let until_ms = minutes
                .checked_mul(MINUTE_MS)
                .and_then(|window| from_ms.checked_add(window))
                .ok_or_else(|| invalid(command, "for more minutes than a clock can count"))?;
            Ok(Change::Grant {
                grant,
                agent,
                scope,
                by: asker.clone(),
                reason,
                from_ms,
                until_ms,
            })
        }
        REVOKE => Ok(Change::End {
            agent: text(command, args, AGENT)?.to_owned(),
            scope: args
                .get(FEATURE)
                .and_then(serde_json::Value::as_str)
                .map(feature),
            by: asker.clone(),
            at_ms: shared.clock.now_ms(),
        }),
        _ => Err(invalid(
            command,
            "of the agents' handler, which runs no such command",
        )),
    }
}

/// What a grant's entry notes and what it answers, over the agents its giver
/// holds before it is made.
///
/// **The window as integers**, so the hash-chained journal says for how long a
/// person delegated; and the grant it ends, the live one on its feature.
fn granted(
    held: &[HeldAgent],
    change: &Change,
) -> Result<(BTreeMap<String, Value>, String), Failure> {
    let Change::Grant {
        grant,
        agent,
        scope,
        from_ms,
        until_ms,
        ..
    } = change
    else {
        return Err(refused(&ChangeRefused::NoSuchAgent));
    };
    let one = the_one(held, agent)?;
    let agent = Value::text(one.agent.id().as_str());
    let grant = Value::text(grant.to_crockford());
    let mut noted = BTreeMap::from([
        (AGENT.to_owned(), agent.clone()),
        ("grant".to_owned(), grant.clone()),
        (
            "scope".to_owned(),
            Value::object([
                ("type", Value::text(scope.type_name.as_str())),
                ("id", Value::text(scope.id.as_str())),
            ]),
        ),
        ("valid_from_ms".to_owned(), integer(*from_ms)),
        ("valid_until_ms".to_owned(), integer(*until_ms)),
    ]);
    let mut answer = vec![
        (AGENT, agent),
        (FEATURE, Value::text(scope.id.as_str())),
        ("grant", grant),
        ("valid_from", Value::text(nc_authz::datetime(*from_ms))),
        ("valid_until", Value::text(nc_authz::datetime(*until_ms))),
    ];
    if let Some(ends) = one.grants.iter().find(|live| live.scope == *scope) {
        let ends = Value::text(ends.grant.as_str());
        noted.insert("ends".to_owned(), ends.clone());
        answer.push(("ends", ends));
    }
    Ok((noted, Value::object(answer).to_canonical()))
}

/// Makes `planned`, once its entry has landed, and answers what it made.
///
/// **Kept before it is held, and an ending held before it is kept**
/// (`nc_authz::LocalDirectory`): a new agent or grant the profile's database
/// refused is held nowhere, and an ending stops the agent at its next call
/// even when the database then refuses to keep it — the refusal says so.
///
/// # Errors
///
/// The directory's refusal of the change, through [`refused`]: a change that
/// went stale since it was rehearsed, the keeper's own code when the database
/// did not keep it, and `[NC-POL-0006]` when the directory could not answer.
pub(crate) async fn made<E: Environment>(
    shared: &Shared<E>,
    planned: Planned,
) -> Result<String, Failure> {
    shared
        .directory
        .change(shared.realm.as_str(), &planned.change)
        .await
        .map(|()| planned.answer)
        .map_err(|refusal| refused(&refusal))
}

/// `agents.list`: the agents `asker` made, in the order they were made, each
/// with its label, its budget and its live grants.
///
/// # Errors
///
/// `unavailable` with `[NC-POL-0006]` when the directory could not answer
/// within what the call tree that began at `began` has left.
pub(crate) async fn listed<E: Environment>(
    shared: &Shared<E>,
    asker: &Principal,
    began: Instant,
) -> Result<String, Failure> {
    Ok(listing(&held_for(shared, asker, began).await?))
}

/// What `agents.list` answers for `held`, as canonical JSON.
fn listing(held: &[HeldAgent]) -> String {
    let agents = held
        .iter()
        .map(|one| {
            let grants: Vec<Value> = one
                .grants
                .iter()
                .map(|live| {
                    Value::object([
                        (FEATURE, Value::text(live.scope.id.as_str())),
                        ("grant", Value::text(live.grant.as_str())),
                        (REASON, Value::text(live.reason.as_str())),
                        (
                            "valid_from",
                            Value::text(nc_authz::datetime(live.valid_from_ms)),
                        ),
                        (
                            "valid_until",
                            Value::text(nc_authz::datetime(live.valid_until_ms)),
                        ),
                    ])
                })
                .collect();
            Value::object([
                (AGENT, Value::text(one.agent.id().as_str())),
                (BUDGET, integer(u64::from(one.budget))),
                ("grants", Value::Array(grants)),
                (LABEL, Value::text(one.label.as_str())),
            ])
        })
        .collect();
    Value::object([("agents", Value::Array(agents))]).to_canonical()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use nc_authz::{ChangeKind, ChangeRefused, DirectoryError, Unkept};
    use nc_catalog::{DAEMON, ErrorCode, KERNEL};

    use super::{
        AGENT, BUDGET, CREATE, FEATURE, GRANT, LABEL, LIST, MINUTES, REASON, REVOKE, judged,
        listing, made, planned, refused,
    };
    use crate::builtin::Builtin;
    use crate::dispatch::{RefusalKind, Waits};

    /// Every argument `agents.create` reads, by the name the command declares.
    const CREATE_READS: [&str; 2] = [LABEL, BUDGET];

    /// Every argument `agents.list` reads: none.
    const LIST_READS: [&str; 0] = [];

    /// Every argument `agents.grant` reads, by the name the command declares.
    const GRANT_READS: [&str; 4] = [AGENT, FEATURE, MINUTES, REASON];

    /// Every argument `agents.revoke` reads, by the name the command declares.
    const REVOKE_READS: [&str; 2] = [AGENT, FEATURE];

    /// The declared agents' commands are the ones this module implements.
    ///
    /// **Both ways**, against `Command::arguments`, which the compiler emits
    /// from the parse it validates the schema with, and the decision each is
    /// made under: `Write`, `Read`, `Grant` and `Revoke` on the platform's
    /// `agents` feature, never the realm; the three that change something that
    /// can be undone by no unit's images and the list a read — so a unit is
    /// opened for each change, noted as having left the machine, and the permit
    /// that decides them is a person's.
    #[test]
    fn the_declared_agents_commands_are_the_ones_this_module_implements() {
        for (name, reads, action, mutating, builtin) in [
            (
                CREATE,
                &CREATE_READS[..],
                "Write",
                true,
                Builtin::AgentsCreate,
            ),
            (LIST, &LIST_READS[..], "Read", false, Builtin::AgentsList),
            (GRANT, &GRANT_READS[..], "Grant", true, Builtin::AgentsGrant),
            (
                REVOKE,
                &REVOKE_READS[..],
                "Revoke",
                true,
                Builtin::AgentsRevoke,
            ),
        ] {
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
                (action, "feature:agents", mutating),
                "`{name}`"
            );
            assert!(
                matches!(
                    (declared.reversibility, mutating),
                    (nc_catalog::Reversibility::Write, true)
                        | (nc_catalog::Reversibility::Read, false)
                ),
                "`{name}`"
            );
            assert_eq!(
                declared.categories,
                ["noctua.settings"],
                "`{name}` is not listed among the settings"
            );
            assert_eq!(Builtin::named(name), Some(builtin), "`{name}`");
        }
        assert_eq!(
            (CREATE, LIST, GRANT, REVOKE),
            (
                "agents.create",
                "agents.list",
                "agents.grant",
                "agents.revoke"
            )
        );
        assert_eq!(
            (LABEL, BUDGET, AGENT, FEATURE, MINUTES, REASON),
            ("label", "budget", "agent", "feature", "minutes", "reason")
        );
    }

    /// The bound `keyword` of `command`'s argument `property` declares, read
    /// off the catalog's own text.
    fn bound(command: &str, property: &str, keyword: &str) -> u64 {
        let declared = nc_catalog::command(command).expect("declared");
        let schema: serde_json::Value =
            serde_json::from_str(declared.args_schema).expect("a schema is JSON");
        schema["properties"][property][keyword]
            .as_u64()
            .unwrap_or_else(|| panic!("`{command}` declares no {keyword} for `{property}`"))
    }

    /// The characters the profile's table holds of `column` at most.
    ///
    /// Read off its `CHECK`, `length(<column>) BETWEEN 1 AND <most>`, which
    /// counts a text's characters.
    fn the_tables_most(column: &str) -> u64 {
        let migration = include_str!("../../../local-migrations/0005_agents.sql");
        let check = format!("CHECK (length({column}) BETWEEN 1 AND ");
        let at = migration
            .find(&check)
            .unwrap_or_else(|| panic!("the table checks `{column}`'s length"))
            + check.len();
        migration[at..]
            .split(')')
            .next()
            .and_then(|most| most.trim().parse().ok())
            .unwrap_or_else(|| panic!("`{column}`'s bound is a number"))
    }

    /// The bounds the agents' schemas declare are the catalog's.
    ///
    /// **Each maximum where its number is written once**: a budget is at most
    /// `[kernel] agent_budget_max` and a grant runs at most
    /// `[kernel] agent_grant_minutes_max` minutes, a label and a reason hold
    /// what the profile's table holds, and an agent is named by as many
    /// characters as base32 writes an identifier in — so a schema edited alone
    /// is a failure here, and not a bound that stopped meaning anything.
    #[test]
    fn the_bounds_the_agents_schemas_declare_are_the_catalogs() {
        assert_eq!(
            bound(CREATE, BUDGET, "maximum"),
            u64::from(KERNEL.agent_budget_max)
        );
        assert_eq!(bound(CREATE, BUDGET, "minimum"), 1);
        assert_eq!(
            bound(GRANT, MINUTES, "maximum"),
            u64::from(KERNEL.agent_grant_minutes_max)
        );
        assert_eq!(bound(GRANT, MINUTES, "minimum"), 1);
        assert_eq!(bound(CREATE, LABEL, "maxLength"), the_tables_most("label"));
        assert_eq!(bound(GRANT, REASON, "maxLength"), the_tables_most("reason"));
        for (command, property) in [(CREATE, LABEL), (GRANT, REASON), (GRANT, FEATURE)] {
            assert_eq!(bound(command, property, "minLength"), 1, "{property}");
        }
        for command in [GRANT, REVOKE] {
            assert_eq!(
                bound(command, AGENT, "minLength"),
                u64::try_from(nc_ids::base32::MIN_WIDTH).expect("fits"),
                "{command}"
            );
            assert_eq!(
                bound(command, AGENT, "maxLength"),
                u64::try_from(nc_ids::base32::MAX_WIDTH).expect("fits"),
                "{command}"
            );
        }
        assert_eq!(
            bound(GRANT, FEATURE, "maxLength"),
            bound(REVOKE, FEATURE, "maxLength")
        );
    }

    /// Each refusal of a change reaches the caller as its own kind, code and
    /// word.
    ///
    /// **The kind of the same name**, for every kind a change can be refused
    /// with, and the code and the word the directory gave it; and nothing the
    /// store said is repeated uncut.
    #[test]
    fn each_refusal_of_a_change_reaches_the_caller_as_its_own_kind_code_and_word() {
        let long = "w".repeat(4_096);
        for (refusal, kind, code, word) in [
            (
                ChangeRefused::NoSuchAgent,
                RefusalKind::NotFound,
                ErrorCode::AiNoSuchAgent,
                "no-such-agent",
            ),
            (
                ChangeRefused::NotTheOperator,
                RefusalKind::Denied,
                ErrorCode::AiNotTheOperator,
                "not-the-operator",
            ),
            (
                ChangeRefused::Full { table: "agents" },
                RefusalKind::Conflict,
                ErrorCode::AiAgentsFull,
                "agents-full",
            ),
            (
                ChangeRefused::Malformed("a reason".to_owned()),
                RefusalKind::Invalid,
                ErrorCode::ValidationArgumentsInvalid,
                "arguments-invalid",
            ),
            (
                ChangeRefused::Unkept(Unkept {
                    code: ErrorCode::DbLocalSchemaUnexpected,
                    why: long.clone(),
                }),
                RefusalKind::Unavailable,
                ErrorCode::DbLocalSchemaUnexpected,
                "not-kept",
            ),
            (
                ChangeRefused::Directory(DirectoryError::Unavailable(long.clone())),
                RefusalKind::Unavailable,
                ErrorCode::PolicyDirectoryUnavailable,
                "directory-unavailable",
            ),
        ] {
            let failure = refused(&refusal);
            assert_eq!(
                (
                    failure.refusal().kind(),
                    failure.refusal().code(),
                    failure.word()
                ),
                (kind, code, word),
                "{refusal:?}"
            );
            assert!(
                failure.refusal().sentence().len() < 1_024,
                "{refusal:?} repeated what another crate said uncut"
            );
        }
        // Every kind a change is refused with is one the caller is told.
        for kind in [
            ChangeKind::NotFound,
            ChangeKind::Denied,
            ChangeKind::Conflict,
            ChangeKind::Invalid,
            ChangeKind::Unavailable,
        ] {
            assert!(
                [
                    ChangeRefused::NoSuchAgent,
                    ChangeRefused::NotTheOperator,
                    ChangeRefused::Full { table: "agents" },
                    ChangeRefused::Malformed(String::new()),
                    ChangeRefused::Directory(DirectoryError::Unavailable(String::new())),
                ]
                .iter()
                .any(|refusal| refusal.kind() == kind),
                "{kind:?}"
            );
        }
    }

    /// A clock standing at `2026-08-26T00:00:00Z`, moving only past what an
    /// identifier asks it to, so every identifier minted in a test is its
    /// predecessor's next.
    #[derive(Debug, Clone)]
    struct Standing(Arc<AtomicU64>);

    impl nc_ids::Clock for Standing {
        fn now_ms(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }

        fn wait_past(&self, millis: u64) {
            self.0.fetch_max(millis.saturating_add(1), Ordering::SeqCst);
        }
    }

    /// A keeper that keeps nothing and refuses nothing.
    #[derive(Debug)]
    struct Forgets;

    impl nc_authz::Keeper for Forgets {
        fn keep(
            &self,
            _kept: &nc_authz::Kept,
        ) -> impl Future<Output = Result<(), nc_authz::Unkept>> + Send {
            std::future::ready(Ok(()))
        }
    }

    /// A keeper that refuses nothing and holds on to what it was handed.
    ///
    /// Each change as the directory hands it over, in order, so a test reads
    /// when and by whom the profile's database would have kept it.
    #[derive(Debug, Clone, Default)]
    struct Recording(Arc<std::sync::Mutex<Vec<nc_authz::Kept>>>);

    impl Recording {
        /// What it was handed so far, in order.
        fn handed(&self) -> Vec<nc_authz::Kept> {
            self.0.lock().expect("nothing panics holding it").clone()
        }
    }

    impl nc_authz::Keeper for Recording {
        fn keep(
            &self,
            kept: &nc_authz::Kept,
        ) -> impl Future<Output = Result<(), nc_authz::Unkept>> + Send {
            self.0
                .lock()
                .expect("nothing panics holding it")
                .push(kept.clone());
            std::future::ready(Ok(()))
        }
    }

    /// A kernel over a store held in memory, on the [`Standing`] clock, over a
    /// directory of agents whose keeper is a [`Recording`].
    #[derive(Debug)]
    struct Delegating;

    impl crate::Environment for Delegating {
        type Store = crate::backend::MemoryBackend;
        type Blobs = crate::blobs::MemoryBlobs;
        type Journal = nc_journal::InMemory;
        type Clock = Standing;
        type Directory = nc_authz::LocalDirectory<Recording>;
        type Log = crate::logging::Captured;
        type Transport = crate::net::Scripted;
    }

    /// A kernel deciding with the family pack over the world the policy cases
    /// are about, its agents held by a directory over it, `ana`, and its keeper.
    async fn delegating() -> (crate::Kernel<Delegating>, nc_authz::Principal, Recording) {
        let pack = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let world = nc_authz::MemoryDirectory::from_world(
            include_str!("../../../policy/world.json"),
            &pack,
        )
        .expect("the world is valid")
        .with_epoch("home", 7);
        let realm = world.realm_id("home").expect("home is a realm");
        let ana = world
            .principal(nc_authz::PrincipalKind::User, "ana")
            .expect("ana is a user of the world");
        let clock = Standing(Arc::new(AtomicU64::new(1_787_702_400_000)));
        let handed = Recording::default();
        let directory = nc_authz::LocalDirectory::open(
            realm.clone(),
            world,
            nc_authz::Records::default(),
            handed.clone(),
            nc_authz::Bounds {
                agents: 4,
                grants: 4,
            },
        )
        .expect("a directory holding nothing opens");
        let kernel = crate::Kernel::open(crate::KernelConfig {
            store: crate::backend::MemoryBackend::default(),
            blobs: crate::blobs::MemoryBlobs::default(),
            pdp: Ok(pack),
            journal: nc_journal::Journal::new(
                nc_journal::InMemory::default(),
                realm,
                ana.id().clone(),
                nc_ids::Minter::new(
                    nc_ids::MachineFlake::from_value(3).expect("a flake"),
                    clock.clone(),
                ),
            ),
            directory,
            clock,
            log: crate::logging::Captured::default(),
            transport: crate::net::Scripted::default(),
        })
        .await
        .expect("the kernel opens");
        (kernel, ana, handed)
    }

    /// The next identifier the kernel's one minter gives.
    async fn next(kernel: &crate::Kernel<Delegating>) -> u64 {
        kernel
            .shared
            .journal
            .lock()
            .await
            .mint_identifier()
            .expect("an identifier is minted")
            .value()
    }

    /// A label or a reason holding an unseen character is refused before
    /// anything is minted.
    ///
    /// **Measured on the minter itself**: on a clock that stands still, each
    /// identifier is its predecessor's next, so an identifier the handler
    /// minted is a gap between the two the test mints around it — none around
    /// a refusal, and exactly one around the same change with the character
    /// taken out, which shows the measure sees a mint. The schema bounds the
    /// two texts' length and says nothing of what a reader sees of them.
    #[tokio::test]
    async fn a_label_or_a_reason_holding_an_unseen_character_is_refused_before_anything_is_minted()
    {
        let (kernel, ana, _) = delegating().await;
        let asked = |command: &str, args: serde_json::Value| {
            let shared = Arc::clone(&kernel.shared);
            let ana = ana.clone();
            let command = command.to_owned();
            async move {
                planned(
                    &shared,
                    &ana,
                    &command,
                    &args,
                    Waits::TOP,
                    std::time::Instant::now(),
                )
                .await
            }
        };
        for (command, args) in [
            (
                CREATE,
                serde_json::json!({"label": "ana\u{200b}s", "budget": 1}),
            ),
            (
                GRANT,
                serde_json::json!({
                    "agent": "000000000000", "feature": "naming", "minutes": 1,
                    "reason": "because\u{2066}"
                }),
            ),
        ] {
            let before = next(&kernel).await;
            let refusal = asked(command, args.clone())
                .await
                .expect_err("a text a reader cannot see all of");
            let after = next(&kernel).await;
            assert_eq!(
                (
                    refusal.refusal().kind(),
                    refusal.refusal().code(),
                    refusal.word()
                ),
                (
                    RefusalKind::Invalid,
                    ErrorCode::ValidationArgumentsInvalid,
                    "arguments-invalid"
                ),
                "{command}"
            );
            assert_eq!(after, before + 1, "{command} minted before it refused");
            assert!(
                !refusal.refusal().sentence().contains("ana")
                    && !refusal.refusal().sentence().contains("because"),
                "{command} repeated the caller's text: {}",
                refusal.refusal().sentence()
            );
        }
        let before = next(&kernel).await;
        asked(CREATE, serde_json::json!({"label": "anas", "budget": 1}))
            .await
            .expect("the same label, seen whole, is planned");
        assert_eq!(
            next(&kernel).await,
            before + 2,
            "a change planned mints its identifier, and this measure sees it"
        );
        assert!(judged(GRANT, &serde_json::json!({"reason": "seen"})).is_ok());
        assert!(judged(GRANT, &serde_json::json!({"reason": "\u{feff}"})).is_err());
    }

    /// What `command` answers when `ana` asks it with `args` at `moment`.
    ///
    /// The kernel's clock is moved to `moment` first, and the change is
    /// planned and made as a dispatch makes it once the decision allowed it.
    async fn changed_at(
        kernel: &crate::Kernel<Delegating>,
        ana: &nc_authz::Principal,
        moment: u64,
        command: &str,
        args: serde_json::Value,
    ) -> serde_json::Value {
        kernel.shared.clock.0.store(moment, Ordering::SeqCst);
        let planned = planned(
            &kernel.shared,
            ana,
            command,
            &args,
            Waits::TOP,
            std::time::Instant::now(),
        )
        .await
        .expect("the change is planned");
        let answer = made(&kernel.shared, planned)
            .await
            .expect("the change is made");
        serde_json::from_str(&answer).expect("an answer is JSON")
    }

    /// Each change is handed to the keeper at the kernel's clock, by its asker.
    ///
    /// **What the profile's database would keep**, as a dispatch plans and
    /// makes each change on a clock moved between them: ana makes an agent,
    /// grants it naming, grants it naming again a minute later — which ends
    /// the first grant where the second begins, by her — and half a minute
    /// after that ends what is live on naming. Each grant begins at the
    /// clock's reading, runs for the minutes asked and is given by ana; the
    /// ending is kept at the clock's reading, by ana, of exactly the grant that
    /// was live — so the next start reads back when each grant began and
    /// ended, and not some other moment.
    #[tokio::test]
    async fn each_change_is_handed_to_the_keeper_at_the_kernels_clock_by_its_asker() {
        let (kernel, ana, keeper) = delegating().await;
        let began = kernel.shared.clock.0.load(Ordering::SeqCst);
        let (first_at, second_at, ended_at) = (began + 60_000, began + 120_000, began + 150_000);
        let made_one = changed_at(
            &kernel,
            &ana,
            began,
            CREATE,
            serde_json::json!({"label": "kept", "budget": 3}),
        )
        .await;
        let agent = made_one[AGENT].as_str().expect("an identifier").to_owned();
        let granting = |minutes: u64, reason: &str| {
            serde_json::json!({
                "agent": agent, "feature": "naming", "minutes": minutes, "reason": reason
            })
        };
        let first = changed_at(&kernel, &ana, first_at, GRANT, granting(5, "first")).await;
        let second = changed_at(&kernel, &ana, second_at, GRANT, granting(1, "second")).await;
        let ending = serde_json::json!({"agent": agent, "feature": "naming"});
        let ended = changed_at(&kernel, &ana, ended_at, REVOKE, ending).await;
        assert_eq!(ended["ended"], serde_json::json!([second["grant"]]));
        let handed = keeper.handed();
        let [
            nc_authz::Kept::Agent(kept_agent),
            nc_authz::Kept::Grant {
                record: first_kept,
                ends: None,
            },
            nc_authz::Kept::Grant {
                record: second_kept,
                ends: Some(ends),
            },
            nc_authz::Kept::Ended { grants, at_ms, by },
        ] = handed.as_slice()
        else {
            panic!("the keeper was handed {handed:#?}");
        };
        assert_eq!(
            (
                kept_agent.agent.to_crockford(),
                &kept_agent.operator,
                kept_agent.budget
            ),
            (agent, ana.id(), 3)
        );
        let naming = super::feature("naming");
        for (kept, answered, from_ms, minutes) in [
            (first_kept, &first, first_at, 5),
            (second_kept, &second, second_at, 1),
        ] {
            assert_eq!(
                (
                    kept.grant.to_crockford(),
                    kept.agent,
                    &kept.scope,
                    &kept.granted_by,
                    kept.valid_from_ms,
                    kept.valid_until_ms,
                ),
                (
                    answered["grant"]
                        .as_str()
                        .expect("an identifier")
                        .to_owned(),
                    kept_agent.agent,
                    &naming,
                    ana.id(),
                    from_ms,
                    from_ms + minutes * 60_000,
                ),
                "a grant is kept from the kernel's clock, by its giver"
            );
        }
        assert_eq!(
            *ends, first_kept.grant,
            "the second grant ends the first where it begins"
        );
        assert_eq!(
            (grants.as_slice(), *at_ms, by),
            ([second_kept.grant].as_slice(), ended_at, ana.id()),
            "an ending is kept at the kernel's clock, by its asker"
        );
    }

    /// How many features the realm of the shipped world holds: as many as an
    /// agent may hold a live grant on, one each.
    fn the_realms_features() -> Vec<String> {
        let world: serde_json::Value =
            serde_json::from_str(include_str!("../../../policy/world.json"))
                .expect("the world is JSON");
        world
            .as_array()
            .expect("a list of entities")
            .iter()
            .filter(|entity| {
                entity["uid"]["type"] == "Feature"
                    && entity["parents"]
                        .as_array()
                        .is_some_and(|parents| parents.iter().any(|parent| parent["id"] == "home"))
            })
            .map(|entity| {
                entity["uid"]["id"]
                    .as_str()
                    .expect("an identifier")
                    .to_owned()
            })
            .collect()
    }

    /// What a frame wraps a payload in at most, which the catalog compiler
    /// holds every frame above a payload by (`IPC_ENVELOPE_BYTES` in
    /// `crates/nc-catalog-compiler/src/load.rs`).
    ///
    /// Written again here because the kernel links no compiler; the frame
    /// itself is read from the catalog.
    const A_FRAMES_ENVELOPE: u64 = 65_536;

    /// The widest list of agents fits one frame.
    ///
    /// **The most a profile of the shipped world can be made to answer**: as
    /// many agents as `[tables.agents]` bounds, made by one person, each
    /// holding a live grant on every feature the realm holds — the directory
    /// keeps one live grant per feature — every identifier minted as wide as
    /// base32 writes one, every window as long as a grant may run, and every
    /// label and reason at its bound in a character canonical JSON writes in
    /// four bytes, the widest of any a reader can see. Longer than a payload,
    /// it fits one frame with the daemon's envelope; a realm of more features
    /// may have it refused whole (`NC-IPC-0016`), and never cut.
    #[tokio::test]
    async fn the_widest_list_of_agents_fits_one_frame() {
        use nc_authz::Directory as _;

        let agents = nc_catalog::TABLES
            .iter()
            .find(|table| table.name == "agents")
            .and_then(|table| table.max_rows)
            .expect("the agents' table is bounded");
        let features = the_realms_features();
        assert!(
            features.iter().any(|feature| feature == "agents"),
            "{features:?}"
        );
        let pack = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let world = nc_authz::MemoryDirectory::from_world(
            include_str!("../../../policy/world.json"),
            &pack,
        )
        .expect("the world is valid");
        let realm = world.realm_id("home").expect("home is a realm");
        let ana = world
            .principal(nc_authz::PrincipalKind::User, "ana")
            .expect("ana is a user of the world");
        let grants = agents * u64::try_from(features.len()).expect("fits");
        let directory = nc_authz::LocalDirectory::open(
            realm.clone(),
            world,
            nc_authz::Records::default(),
            Forgets,
            nc_authz::Bounds { agents, grants },
        )
        .expect("a directory holding nothing opens");
        // A moment past which every identifier is written in base32's widest.
        let later = 2_208_988_800_000;
        let mut minter = nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1_023).expect("a flake"),
            Standing(Arc::new(AtomicU64::new(later))),
        );
        let wide = |property: &str, command: &str| -> String {
            let chars = usize::try_from(bound(command, property, "maxLength")).expect("fits");
            std::iter::repeat_n('\u{1F99D}', chars).collect()
        };
        let window = u64::from(KERNEL.agent_grant_minutes_max) * 60_000;
        for _ in 0..agents {
            let agent = minter.mint().expect("an identifier");
            assert_eq!(agent.to_crockford().len(), nc_ids::base32::MAX_WIDTH);
            directory
                .change(
                    realm.as_str(),
                    &nc_authz::Change::Make {
                        agent,
                        operator: ana.clone(),
                        label: wide(LABEL, CREATE),
                        budget: KERNEL.agent_budget_max,
                    },
                )
                .await
                .expect("the agent is made");
            for feature in &features {
                directory
                    .change(
                        realm.as_str(),
                        &nc_authz::Change::Grant {
                            grant: minter.mint().expect("an identifier"),
                            agent: agent.to_crockford(),
                            scope: super::feature(feature),
                            by: ana.clone(),
                            reason: wide(REASON, GRANT),
                            from_ms: later,
                            until_ms: later + window,
                        },
                    )
                    .await
                    .expect("the grant is given");
            }
        }
        let held = directory
            .agents(realm.as_str(), &ana.entity())
            .await
            .expect("the agents are listed");
        assert_eq!(u64::try_from(held.len()).expect("fits"), agents);
        assert!(held.iter().all(|one| one.grants.len() == features.len()));
        let bytes = u64::try_from(listing(&held).len()).expect("fits");
        assert!(
            bytes + A_FRAMES_ENVELOPE <= DAEMON.ipc_frame_max_bytes,
            "the widest list of agents is {bytes} bytes, and a frame holds {} with its \
             envelope",
            DAEMON.ipc_frame_max_bytes
        );
        // What `docs/limits.md` and `AGENTS.md` say of it: once it fits a
        // payload, the bound here is `payload_bytes` and both say so instead.
        assert!(
            bytes > KERNEL.payload_bytes,
            "the widest list of agents is {bytes} bytes, within the {} of a payload, which \
             the documents say it is longer than",
            KERNEL.payload_bytes
        );
    }
}
