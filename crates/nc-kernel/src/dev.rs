//! What the dev pages read of this machine (`NOCTUA.md §12.16`, `§15.3`).
//!
//! **Seven reads of the kernel's own, each decided as every command is**, so
//! every dev page is authorized and none is reachable only from a window:
//!
//! | Command | Answers | Decided on |
//! |---|---|---|
//! | `dev.limits.read` | every bound with a use to show, and the use | `Feature::"interface"` |
//! | `dev.plugins.read` | every loaded plugin, what it was granted and what it uses | `Feature::"interface"` |
//! | `dev.sessions.read` | the agents' sessions the kernel's host holds open | `Feature::"interface"` |
//! | `dev.agents.read` | the two tables that keep agents, against their bounds | `Feature::"interface"` |
//! | `dev.policy.read` | the realm's policy pack, its version and its epoch | `Feature::"interface"` |
//! | `dev.logs.read` | the newest lines of the host's own log | the realm |
//! | `dev.journal.read` | the realm's journal, a page at a time | the realm |
//!
//! The family pack gives the interface's feature to every person of the
//! realm (`family.a_member_drives_the_interface`) and a read of the realm to
//! a guardian alone (`family.a_guardian_reads_the_realm`), and neither to an
//! agent or a device: the first five answer counts, names and versions, as
//! the interface shows its person what waits for them and what their plugins
//! show, and the last two say who did what, as `journal.verify` reads the
//! realm's audit log. Each is a read — it opens no unit, makes no step and
//! changes nothing — and is journaled once, its entry naming the command and
//! what it was decided on and never the arguments.
//!
//! **No feature of their own, and the reason is measured.** An agent may be
//! granted every feature its realm holds, so each feature the shipped world
//! gains widens the widest answer `agents.list` can give, and one more than
//! it holds takes that answer past a frame
//! (`the_widest_list_of_agents_fits_one_frame` in `crate::agents`). So the
//! dev pages are decided on what the world already holds, by the same
//! people, until that bound is moved.
//!
//! **Every answer is bounded.** One whose rows can grow — the plugins, the
//! log, the journal — is read a page at a time: `from`, where the page
//! starts, and `max`, at most `[kernel] dev_page_max` rows, the maximum each
//! schema declares; it says in `next` where the next page starts. A page also
//! ends before its rows' text would pass half of `payload_bytes`
//! ([`fitting`]), its first row apart, so a page of long rows is shorter and
//! never refused for its size. The other four answer as many rows as the
//! catalog declares tables and this module names counts, or as many sessions
//! as the host holds, which its own bound holds.
//!
//! **The journal is read through the store's own page**
//! (`nc_journal::Store::page`), so a page costs what it answers and not what
//! the history holds, and holds the entries of this kernel's realm alone: an
//! entry another realm's kernel wrote into the same store is left out, and
//! `next` still moves past it. An entry the store could not read is named in
//! `unreadable` and never skipped in silence.
//!
//! **The log is the host's own ring, and nothing else's.** What
//! `dev.logs.read` answers is what the kernel's log sink kept
//! (`crate::logging::LogSink::kept`): in a daemon, the newest
//! `[daemon] dev_logs_ring_lines` lines `nc_log` recorded, each the one line
//! the central log is sent. So the wall that keeps user data and secrets out
//! of that store — an event's fields are typed, and no string type is one of
//! them (`NOCTUA.md §15.4`) — is what keeps them out of this answer: the
//! kernel copies each line as it was written and adds nothing to it. A sink
//! that keeps no line answers none. The central log is read from its own
//! store, never through here.
//!
//! **What the kernel's host holds is the host's to say** ([`Hosting`],
//! `crate::Kernel::hosted_by`): the agents' sessions open and the connections
//! open are a daemon's, counted against `[daemon]`'s bounds. A kernel told of
//! no host answers no session and says the two counts as `null`, never as
//! zero.
//!
//! What this answers (`NOCTUA.md §2.4`): a dev page read by somebody the pack
//! does not let read it — each is decided and journaled, a denial included —
//! an answer that grows with the machine's history until a frame cannot
//! carry it, and a log line that says what a person wrote. What it does not
//! answer: the central log, which is its store's; the limits that are a size
//! or a time, which have no use to show and are `docs/limits.md`'s; and a
//! bound counted for each plugin or each unit, which is answered beside the
//! whole table's rows and is not what bounds them.

use std::sync::Arc;

use nc_canon::Value;
use nc_catalog::{DAEMON, Database, KERNEL, Strategy};
use nc_ids::Snowflake;
use nc_journal::Store as _;

use crate::backend::StoreBackend as _;
use crate::dispatch::{self, Failure, Refusal, RefusalKind, Waits};
use crate::logging::LogSink as _;
use crate::{Environment, PluginSlot, Shared};

/// The command that answers every bound with a use to show.
pub(crate) const LIMITS: &str = "dev.limits.read";

/// The command that answers every loaded plugin.
pub(crate) const PLUGINS: &str = "dev.plugins.read";

/// The command that answers the agents' sessions held open.
pub(crate) const SESSIONS: &str = "dev.sessions.read";

/// The command that answers the tables of agents against their bounds.
pub(crate) const AGENTS: &str = "dev.agents.read";

/// The command that answers the realm's policy pack and epoch.
pub(crate) const POLICY: &str = "dev.policy.read";

/// The command that reads the host's own log.
pub(crate) const LOGS: &str = "dev.logs.read";

/// The command that reads the realm's journal.
pub(crate) const JOURNAL: &str = "dev.journal.read";

/// Where a page starts: a plugin's slot, a log line's serial, or a journal
/// entry's sequence.
const FROM: &str = "from";

/// How many rows a page holds at most.
const MAX: &str = "max";

/// The two tables that keep a person's agents, which `dev.agents.read`
/// answers, in the catalog's order.
const TABLES_OF_AGENTS: [&str; 2] = ["agent_grants", "agents"];

/// What the kernel's host holds that the kernel does not: the agents'
/// sessions and the connections a daemon keeps open (`NOCTUA.md §20.6`).
///
/// **Read each time a dev page asks, and never kept by the kernel.** Both
/// answer at once, from what the host counts.
pub trait Hosting: core::fmt::Debug + Send + Sync + 'static {
    /// The agent of each session the host holds open now, one for each
    /// session, in any order.
    fn sessions(&self) -> Vec<Snowflake>;

    /// How many connections the host holds open now.
    fn connections(&self) -> u64;
}

/// A count, as an answer writes it.
fn count(counted: u64) -> Value {
    Value::Integer(i64::try_from(counted).unwrap_or(i64::MAX))
}

/// A count the kernel holds in memory, as an answer writes it.
fn held(counted: usize) -> Value {
    count(u64::try_from(counted).unwrap_or(u64::MAX))
}

/// A text there may be none of, as an answer writes it.
fn text_or_null(text: Option<&str>) -> Value {
    text.map_or(Value::Null, Value::text)
}

/// Where the page `args` asks for starts, and how many rows it holds at
/// most: from the start and `dev_page_max` rows where it says neither.
///
/// The schema holds `max` to `dev_page_max` before this reads it.
fn page(args: &serde_json::Value) -> (u64, u32) {
    let from = args
        .get(FROM)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let most = args
        .get(MAX)
        .and_then(serde_json::Value::as_u64)
        .and_then(|most| u32::try_from(most).ok())
        .unwrap_or(KERNEL.dev_page_max);
    (from, most)
}

/// How many bytes of rows a page holds at most: half a payload, so the
/// answer around them is one the kernel may give.
const fn budget() -> u64 {
    KERNEL.payload_bytes / 2
}

/// The first of `rows` whose text together is within `budget` bytes — the
/// first row whatever its size, so a page is never empty for its size — and
/// the key of the first row left out, when one was.
///
/// Each row comes with its key: where a page that starts at it starts.
fn fitting(rows: Vec<(u64, Value)>, budget: u64) -> (Vec<Value>, Option<u64>) {
    let mut spent = 0_u64;
    let mut kept = Vec::new();
    for (key, row) in rows {
        let bytes = u64::try_from(row.to_canonical().len()).unwrap_or(u64::MAX);
        spent = spent.saturating_add(bytes);
        if spent > budget && !kept.is_empty() {
            return (kept, Some(key));
        }
        kept.push(row);
    }
    (kept, None)
}

/// The word a table's strategy is answered as, the one `catalog/limits.toml`
/// writes.
const fn strategy(strategy: Strategy) -> &'static str {
    match strategy {
        Strategy::OldestFirst => "oldest-first",
        Strategy::PolicyPack => "policy-pack",
        Strategy::RefuseAtLimit => "refuse-at-limit",
    }
}

/// Every table the catalog bounds in the local database, or those of them
/// `only` names, each with the rows it holds now: `table`, `rows`,
/// `max_rows` — `null` where the catalog declares none — and `strategy`.
///
/// **In the catalog's order, and from the catalog alone**, so a table a
/// migration adds is answered with no change here. `rows` is the whole
/// table's count, read and never held to what a delete would reach
/// (`nc_prune::sqlite::Sqlite::rows`), so a table whose trigger refuses every
/// delete is counted as any other; it is `null` for a table that could not
/// be counted. **A table the catalog bounds by no number is counted from
/// the end of the column that orders it** ([`counted_to_the_end`]): its count
/// is read under the bus every dispatch waits on, and reading every row of a
/// table that grows with the whole history would make each read cost more
/// than the last. A store with no local database — one held in memory —
/// answers no table.
async fn tables<E: Environment>(shared: &Shared<E>, only: Option<&[&str]>) -> Value {
    let Some(target) = shared.store.pruner() else {
        return Value::Array(Vec::new());
    };
    let mut rows = Vec::new();
    for table in nc_catalog::TABLES {
        if table.database != Database::LocalFile
            || only.is_some_and(|only| !only.contains(&table.name))
        {
            continue;
        }
        let counted = if counted_to_the_end(table) {
            target.rows_to_the_end(table.name, table.order_by).await
        } else {
            target.rows(table.name).await
        }
        .ok();
        rows.push(Value::object([
            ("table", Value::text(table.name)),
            ("rows", counted.map_or(Value::Null, count)),
            ("max_rows", table.max_rows.map_or(Value::Null, count)),
            ("strategy", Value::text(strategy(table.strategy))),
        ]));
    }
    Value::Array(rows)
}

/// Whether `table` is counted from the end of the column that orders it,
/// rather than row by row: where the catalog bounds it by no number.
///
/// **True only of a table dense from zero**, which a test holds: the one
/// local table the catalog bounds by no number is the local journal, whose
/// triggers land every entry at the next sequence from zero and refuse every
/// delete (`local-migrations/0004_local_journal.sql`), so its last sequence
/// plus one is its count, read in one step of its primary key.
const fn counted_to_the_end(table: &nc_catalog::TableLimit) -> bool {
    table.max_rows.is_none()
}

/// The host `shared` was told of, if it was told of one.
fn host<E: Environment>(shared: &Shared<E>) -> Option<Arc<dyn Hosting>> {
    dispatch::lock(&shared.host).clone()
}

/// One count held against a limit: `limit`, by its table and name in
/// `catalog/limits.toml`, `max`, and `used`, `null` where nothing says.
fn counted(limit: &str, max: u64, used: Option<u64>) -> Value {
    Value::object([
        ("limit", Value::text(limit)),
        ("max", count(max)),
        ("used", used.map_or(Value::Null, count)),
    ])
}

/// What is in use now of each count held against a limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Used {
    /// The lines the log keeps.
    log_lines: u64,
    /// The connections the host holds open, where a host says.
    connections: Option<u64>,
    /// The agents' sessions the host holds open, where a host says.
    sessions: Option<u64>,
    /// The calls waiting for a person's confirmation.
    pending: u64,
    /// The events waiting for delivery.
    queued: u64,
}

/// Every count held against a limit, in name order, each beside what is
/// `used` of it now.
fn counts(used: Used) -> Vec<Value> {
    vec![
        counted(
            "daemon.dev_logs_ring_lines",
            u64::from(DAEMON.dev_logs_ring_lines),
            Some(used.log_lines),
        ),
        counted(
            "daemon.ipc_connections_max",
            u64::from(DAEMON.ipc_connections_max),
            used.connections,
        ),
        counted(
            "daemon.sessions_max",
            u64::from(DAEMON.sessions_max),
            used.sessions,
        ),
        counted(
            "kernel.confirmations_pending_max",
            u64::from(KERNEL.confirmations_pending_max),
            Some(used.pending),
        ),
        counted(
            "kernel.event_queue",
            u64::from(KERNEL.event_queue),
            Some(used.queued),
        ),
    ]
}

/// `dev.limits.read`: every bound with a use to show, and the use.
///
/// `{"counts":[…],"tables":[…]}`. `counts` are the counts held against a
/// limit, in name order — the lines the log keeps, the connections and the
/// agents' sessions the host holds open, the calls waiting for a person's
/// confirmation and the events waiting for delivery — each with `limit`,
/// `max` and `used`; `tables` is [`tables`], every one.
pub(crate) async fn limits<E: Environment>(shared: &Shared<E>) -> String {
    let host = host(shared);
    let size = |held: usize| u64::try_from(held).unwrap_or(u64::MAX);
    let used = Used {
        log_lines: shared.log.kept(0, 0).held,
        connections: host.as_ref().map(|host| host.connections()),
        sessions: host.as_ref().map(|host| size(host.sessions().len())),
        pending: size(dispatch::lock(&shared.pending).waiting()),
        queued: size(dispatch::lock(&shared.events).len()),
    };
    Value::object([
        ("counts", Value::Array(counts(used))),
        ("tables", tables(shared, None).await),
    ])
    .to_canonical()
}

/// `dev.agents.read`: the two tables that keep a person's agents, against
/// their bounds.
///
/// `{"tables":[…]}`, as [`tables`] answers `agent_grants` and `agents`: rows
/// counted, never an agent shown — `agents.list` is what lists a person's
/// own.
pub(crate) async fn agents<E: Environment>(shared: &Shared<E>) -> String {
    Value::object([("tables", tables(shared, Some(&TABLES_OF_AGENTS)).await)]).to_canonical()
}

/// `dev.sessions.read`: the agents' sessions the kernel's host holds open.
///
/// `{"connections":…,"connections_max":…,"max":…,"open":…,"sessions":[…]}`:
/// each session by its `agent`, in the order of the agents' identifiers;
/// `open`, how many there are, against `max`, `[daemon] sessions_max`; and
/// `connections` against `connections_max`. A kernel told of no host answers
/// no session, and `open` and `connections` as `null`.
pub(crate) fn open_sessions<E: Environment>(shared: &Shared<E>) -> String {
    let host = host(shared);
    let mut open = host.as_ref().map(|host| host.sessions());
    if let Some(open) = &mut open {
        open.sort_unstable();
    }
    Value::object([
        (
            "sessions",
            Value::Array(
                open.iter()
                    .flatten()
                    .map(|agent| Value::object([("agent", Value::text(agent.to_crockford()))]))
                    .collect(),
            ),
        ),
        (
            "open",
            open.as_ref().map_or(Value::Null, |open| held(open.len())),
        ),
        ("max", count(u64::from(DAEMON.sessions_max))),
        (
            "connections",
            host.map_or(Value::Null, |host| count(host.connections())),
        ),
        (
            "connections_max",
            count(u64::from(DAEMON.ipc_connections_max)),
        ),
    ])
    .to_canonical()
}

/// `dev.policy.read`: the pack that decides in this kernel's realm.
///
/// `{"epoch":…,"pack":…,"realm":…}`: the `realm`, the `pack` and version it
/// is decided by — `family@1` — and its policy `epoch`, each as the decision
/// point read it to decide this very read (`pack` and `epoch`, which its
/// journal entry records too). **What the realm pins and what the kernel
/// carries are one answer here**: a realm pinning a pack the kernel does not
/// carry is denied everything before any command runs, this one included.
pub(crate) fn policy<E: Environment>(shared: &Shared<E>, pack: &str, epoch: u64) -> String {
    Value::object([
        ("realm", Value::text(shared.realm.as_str())),
        ("pack", Value::text(pack)),
        ("epoch", count(epoch)),
    ])
    .to_canonical()
}

/// What `dev.plugins.read` answers of the plugin loaded in `slot`.
struct Seen {
    /// Its place in the order plugins were loaded.
    slot: usize,
    /// Its identity, as its manifest writes it.
    id: String,
    /// The namespace its commands are in.
    namespace: String,
    /// The capabilities it was granted, by the names the catalog gives them.
    capabilities: Vec<&'static str>,
    /// How many commands it declares.
    commands: usize,
    /// How many lines its own log keeps.
    log_lines: usize,
}

/// `dev.plugins.read`: every loaded plugin, what it was granted and what it
/// uses, a page at a time.
///
/// `{"next":…,"plugins":[…]}`: the plugins loaded from the slot `from` on,
/// `max` at most, in the order they were loaded, each with its `slot`, `id`
/// and `namespace`; the `capabilities` it was granted, in the catalog's
/// order; how many `commands` it declares, `surfaces` and `timers` it holds
/// and `log_lines` its own log keeps; and how it stands — the `generation`
/// of the instance that runs, its `crashes` since it was loaded or last
/// released, whether it is `quarantined`, and `last_kill`, why it was last
/// killed, or `null`. A slot whose plugin was unloaded is left out. `next` is
/// the slot the next page starts from.
pub(crate) fn plugins<E: Environment>(shared: &Shared<E>, args: &serde_json::Value) -> String {
    let (from, most) = page(args);
    let from = usize::try_from(from).unwrap_or(usize::MAX);
    let most = usize::try_from(most).unwrap_or(usize::MAX);
    // What the plugins' lock guards is read first and let go, so no other
    // lock is taken under it; how each stands is asked after, slot by slot,
    // and a slot unloaded by then is left out.
    let (seen, loaded) = {
        let plugins = shared.plugins();
        let seen: Vec<Seen> = plugins
            .loaded
            .iter()
            .enumerate()
            .skip(from)
            .map(|(slot, loaded)| Seen {
                slot,
                id: loaded.manifest.id().to_owned(),
                namespace: loaded.manifest.namespace().to_owned(),
                capabilities: loaded
                    .manifest
                    .grants()
                    .iter()
                    .map(|capability| capability.spec().name)
                    .collect(),
                commands: loaded.declaration.commands.len(),
                log_lines: dispatch::lock(&loaded.log).read().lines().len(),
            })
            .collect();
        (seen, plugins.loaded.len())
    };
    let surfaces = dispatch::lock(&shared.surfaces).listed();
    let rows: Vec<(u64, Value)> = seen
        .into_iter()
        .filter_map(|seen| Some((shared.status(PluginSlot(seen.slot))?, seen)))
        .take(most)
        .map(|(status, seen)| {
            let slot = PluginSlot(seen.slot);
            let timers = dispatch::lock(&shared.timers).held_by(slot);
            let shows = surfaces
                .iter()
                .filter(|surface| surface.owner() == slot)
                .count();
            (
                u64::try_from(seen.slot).unwrap_or(u64::MAX),
                Value::object([
                    ("slot", held(seen.slot)),
                    ("id", Value::text(seen.id)),
                    ("namespace", Value::text(seen.namespace)),
                    (
                        "capabilities",
                        Value::Array(seen.capabilities.into_iter().map(Value::text).collect()),
                    ),
                    ("commands", held(seen.commands)),
                    ("surfaces", held(shows)),
                    ("timers", held(timers)),
                    ("log_lines", held(seen.log_lines)),
                    ("generation", count(status.generation())),
                    ("crashes", count(u64::from(status.crashes()))),
                    ("quarantined", Value::Bool(status.quarantined())),
                    (
                        "last_kill",
                        text_or_null(status.last_kill().map(crate::Kill::word)),
                    ),
                ]),
            )
        })
        .collect();
    // A page that is full may have more after its last row; one that is not
    // read every slot there is, and the next plugin loaded takes the next.
    let after_a_full_page = (rows.len() == most)
        .then(|| rows.last().map(|(slot, _)| slot.saturating_add(1)))
        .flatten();
    let (kept, left_out) = fitting(rows, budget());
    let next = left_out
        .or(after_a_full_page)
        .unwrap_or_else(|| u64::try_from(loaded).unwrap_or(u64::MAX));
    Value::object([("plugins", Value::Array(kept)), ("next", count(next))]).to_canonical()
}

/// `dev.logs.read`: the newest lines of the host's own log, a page at a
/// time.
///
/// `{"held":…,"lines":[…],"next":…}`: the lines kept from the serial `from`
/// on, `max` at most, the oldest first, each with its `serial` — its place
/// among every line the log recorded since it opened, from 1 — and its
/// `line`, the text the central log is sent, as it was written; `held`, how
/// many lines are kept now; and `next`, the serial the next read starts from
/// to miss nothing and repeat nothing. A `from` before the oldest line kept
/// starts at that line, so a gap between two reads shows in the serials.
pub(crate) fn logs<E: Environment>(shared: &Shared<E>, args: &serde_json::Value) -> String {
    let (from, most) = page(args);
    let kept = shared
        .log
        .kept(from, usize::try_from(most).unwrap_or(usize::MAX));
    let rows = kept
        .lines
        .into_iter()
        .map(|(serial, line)| {
            (
                serial,
                Value::object([("serial", count(serial)), ("line", Value::text(line))]),
            )
        })
        .collect();
    let (lines, left_out) = fitting(rows, budget());
    Value::object([
        ("lines", Value::Array(lines)),
        ("next", count(left_out.unwrap_or(kept.next))),
        ("held", count(kept.held)),
    ])
    .to_canonical()
}

/// One entry of the journal, as `dev.journal.read` answers it.
fn entry(entry: &nc_journal::Entry) -> Value {
    let record = &entry.record;
    let id = |id: &nc_ids::OpaqueId| Value::text(id.as_str());
    Value::object([
        ("sequence", count(entry.sequence)),
        ("principal", id(&record.principal)),
        (
            "actor_chain",
            Value::Array(record.actor_chain.iter().map(id).collect()),
        ),
        ("action", Value::text(record.action.as_str())),
        ("resource", record.resource.clone()),
        ("result", Value::text(record.result.as_str())),
        (
            "deciding_policy",
            text_or_null(record.deciding_policy.as_deref()),
        ),
        ("policy_pack", text_or_null(record.policy_pack.as_deref())),
        (
            "policy_epoch",
            record.policy_epoch.map_or(Value::Null, count),
        ),
        (
            "grant_used",
            record.grant_used.as_ref().map_or(Value::Null, id),
        ),
        (
            "work_unit",
            record
                .work_unit
                .map_or(Value::Null, |unit| Value::text(unit.to_crockford())),
        ),
        ("recorded_at", count(record.recorded_at)),
        ("hash", Value::text(nc_canon::to_hex(&entry.hash))),
    ])
}

/// `dev.journal.read`: the realm's journal, a page at a time.
///
/// `{"entries":[…],"head":…,"next":…,"unreadable":…}`: the entries of this
/// kernel's realm from the sequence `from` on, among the `max` the store
/// holds there, the oldest first, each as [`entry`] writes it — who asked,
/// for whom, which command, on what, how it was decided and by which policy,
/// in which unit and when, and its hash; `head`, the sequence of the newest
/// entry the store holds, or `null` for an empty one; `next`, the sequence
/// the next page starts from; and `unreadable`, the sequence of an entry the
/// store could not read, which the page stops at and `next` passes, or
/// `null`.
///
/// # Errors
///
/// `unavailable`, with the journal's own code, when the journal could not be
/// read: nothing is answered then.
pub(crate) async fn journal<E: Environment>(
    shared: &Shared<E>,
    args: &serde_json::Value,
    waits: Waits,
) -> Result<String, Failure> {
    let (from, most) = page(args);
    let read = waits
        .on(async {
            let journal = shared.journal.lock().await;
            let head = journal.store().head().await?;
            let read = journal.store().page(from, most).await?;
            Ok((head, read))
        })
        .await;
    let (head, read) = read.map_err(|error| {
        Failure::new(
            Refusal::new(
                RefusalKind::Unavailable,
                error.code(),
                "the journal could not be read, so no entry of it is answered".to_owned(),
            ),
            "journal-unreadable",
        )
    })?;
    // Past the last entry read, whichever realm wrote it.
    let read_to = read
        .entries
        .last()
        .map_or(from, |last| last.sequence.saturating_add(1));
    let rows = read
        .entries
        .iter()
        .filter(|entry| entry.record.realm == shared.realm)
        .map(|read| (read.sequence, entry(read)))
        .collect();
    let (entries, left_out) = fitting(rows, budget());
    // An entry the store could not read is said only by the page that
    // reached it, and that page's `next` is past it.
    let unreadable = match left_out {
        Some(_) => None,
        None => read.stopped_at.map(|(sequence, _)| sequence),
    };
    let next = left_out
        .or_else(|| unreadable.map(|sequence| sequence.saturating_add(1)))
        .unwrap_or(read_to);
    Ok(Value::object([
        ("entries", Value::Array(entries)),
        ("next", count(next)),
        (
            "head",
            head.map_or(Value::Null, |(sequence, _)| count(sequence)),
        ),
        ("unreadable", unreadable.map_or(Value::Null, count)),
    ])
    .to_canonical())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nc_canon::Value;
    use nc_catalog::{DAEMON, Database, KERNEL, Strategy};

    use super::{
        AGENTS, FROM, JOURNAL, LIMITS, LOGS, MAX, PLUGINS, POLICY, SESSIONS, TABLES_OF_AGENTS,
        Used, budget, counted_to_the_end, counts, entry, fitting, page, strategy,
    };

    /// The feature the five reads of how the machine stands are decided on.
    const INTERFACE: &str = "feature:interface";

    /// The selector of the two reads of the realm's records.
    const REALM: &str = "realm";

    /// Every command of this module: its name, the arguments it reads and
    /// what it is decided on.
    const DECLARED: [(&str, &[&str], &str); 7] = [
        (LIMITS, &[], INTERFACE),
        (PLUGINS, &[FROM, MAX], INTERFACE),
        (SESSIONS, &[], INTERFACE),
        (AGENTS, &[], INTERFACE),
        (POLICY, &[], INTERFACE),
        (LOGS, &[FROM, MAX], REALM),
        (JOURNAL, &[FROM, MAX], REALM),
    ];

    /// Only the local journal is counted from the end of its key, and every
    /// other local table row by row.
    ///
    /// **The one table the catalog bounds by no number is the one whose key
    /// is dense from zero**: a table a migration adds with no bound is
    /// counted from its end only once a reader of its triggers has added it
    /// here, since for any other table the end of its key is not its count.
    #[test]
    fn only_the_local_journal_is_counted_from_the_end_of_its_key() {
        let to_the_end: Vec<(&str, &str)> = nc_catalog::TABLES
            .iter()
            .filter(|table| table.database == Database::LocalFile && counted_to_the_end(table))
            .map(|table| (table.name, table.order_by))
            .collect();
        assert_eq!(to_the_end, [("local_journal_entries", "sequence")]);
        assert!(
            nc_catalog::TABLES
                .iter()
                .filter(|table| table.database == Database::LocalFile)
                .filter(|table| !counted_to_the_end(table))
                .all(|table| table.max_rows.is_some()),
            "a bounded table is counted row by row"
        );
    }

    /// The declared dev commands are the ones this module implements: the
    /// arguments each reads, both ways, and the decision each is made under —
    /// a `Read` that changes nothing, on the interface's feature for how the
    /// machine stands and on the realm for its records.
    #[test]
    fn the_declared_dev_commands_are_the_ones_this_module_implements() {
        for (name, reads, selector) in DECLARED {
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
                ("Read", selector, false),
                "`{name}`"
            );
            assert!(
                matches!(declared.reversibility, nc_catalog::Reversibility::Read),
                "`{name}`"
            );
        }
        let ours: BTreeSet<&str> = DECLARED.iter().map(|(name, _, _)| *name).collect();
        let dev: BTreeSet<&str> = nc_catalog::COMMANDS
            .iter()
            .map(|command| command.name)
            .filter(|name| name.starts_with("dev."))
            .collect();
        assert_eq!(ours, dev, "every command of `dev.` is one of this module's");
        assert_eq!(
            (LIMITS, PLUGINS, SESSIONS, AGENTS, POLICY, LOGS, JOURNAL),
            (
                "dev.limits.read",
                "dev.plugins.read",
                "dev.sessions.read",
                "dev.agents.read",
                "dev.policy.read",
                "dev.logs.read",
                "dev.journal.read"
            )
        );
        assert_eq!((FROM, MAX), ("from", "max"));
    }

    /// Every bound a paged command's schema declares is the catalog's: a page
    /// of `dev_page_max` rows at most and one at least, from zero on.
    #[test]
    fn every_bound_the_paged_schemas_declare_is_the_catalogs() {
        let paged: Vec<&str> = DECLARED
            .iter()
            .filter(|(_, reads, _)| !reads.is_empty())
            .map(|(name, _, _)| *name)
            .collect();
        assert_eq!(paged, [PLUGINS, LOGS, JOURNAL]);
        for name in paged {
            let declared = nc_catalog::command(name).expect("declared");
            let schema: serde_json::Value =
                serde_json::from_str(declared.args_schema).expect("a schema is JSON");
            assert_eq!(
                schema["properties"][MAX]["maximum"].as_u64(),
                Some(u64::from(KERNEL.dev_page_max)),
                "`{name}`"
            );
            assert_eq!(
                schema["properties"][MAX]["minimum"].as_u64(),
                Some(1),
                "`{name}`"
            );
            assert_eq!(
                schema["properties"][FROM]["minimum"].as_u64(),
                Some(0),
                "`{name}`"
            );
            assert_eq!(schema["required"], serde_json::Value::Null, "`{name}`");
        }
        assert_eq!(KERNEL.dev_page_max, 256);
        assert_eq!(DAEMON.dev_logs_ring_lines, 4_096);
    }

    /// A page starts where it is asked and holds as many rows as it is asked
    /// for, and from the start and `dev_page_max` rows where it says neither.
    #[test]
    fn a_page_starts_where_it_is_asked_and_holds_what_it_is_asked_for() {
        let asked = |text: &str| page(&serde_json::from_str(text).expect("JSON"));
        assert_eq!(asked("{}"), (0, KERNEL.dev_page_max));
        assert_eq!(asked(r#"{"from":7}"#), (7, KERNEL.dev_page_max));
        assert_eq!(asked(r#"{"max":3}"#), (0, 3));
        assert_eq!(asked(r#"{"from":9,"max":1}"#), (9, 1));
    }

    /// A page ends before its rows' text passes the budget, its first row
    /// apart, and names the first row it left out.
    ///
    /// **At the boundary both ways**: rows that fill the budget exactly are
    /// all kept, and one byte less leaves the last out; a first row longer
    /// than the whole budget is still answered, alone.
    #[test]
    fn a_page_ends_before_its_text_passes_the_budget_and_names_what_it_left_out() {
        // Each row is the five bytes of `"aaa"`.
        let rows =
            || -> Vec<(u64, Value)> { (10..14).map(|key| (key, Value::text("aaa"))).collect() };
        let all: Vec<Value> = rows().into_iter().map(|(_, row)| row).collect();
        assert_eq!(all[0].to_canonical().len(), 5);
        assert_eq!(
            fitting(rows(), 20),
            (all.clone(), None),
            "exactly the budget"
        );
        assert_eq!(
            fitting(rows(), 19),
            (all[..3].to_vec(), Some(13)),
            "a byte under it"
        );
        assert_eq!(fitting(rows(), 5), (all[..1].to_vec(), Some(11)));
        assert_eq!(
            fitting(rows(), 1),
            (all[..1].to_vec(), Some(11)),
            "the first row whatever its size"
        );
        assert_eq!(fitting(Vec::new(), 1), (Vec::new(), None));
        assert_eq!(budget(), KERNEL.payload_bytes / 2);
        assert_eq!(budget(), 524_288);
    }

    /// An entry is answered with every field the journal records of it, and
    /// `null` where it records none.
    #[test]
    fn an_entry_is_answered_with_everything_the_journal_records_of_it() {
        // Identifiers the crate mints from keys, and a unit a minter mints:
        // no text is turned into one here.
        let [agent, person, earlier, grant, realm] =
            [1_u8, 2, 3, 4, 5].map(|key| nc_ids::OpaqueId::of_device_key(&[key; 32]));
        let unit = nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(7).expect("a flake"),
            nc_ids::SystemClock,
        )
        .mint()
        .expect("an identifier");
        let mut record = nc_journal::Record::new(
            agent.clone(),
            "Read",
            Value::object([("command", Value::text("dev.policy.read"))]),
            nc_journal::Outcome::Allowed,
            1_800_000_000_000,
        );
        record.realm = realm;
        let bare = nc_journal::Entry::chain(4, None, record.clone());
        assert_eq!(
            entry(&bare).to_canonical(),
            format!(
                r#"{{"action":"Read","actor_chain":[],"deciding_policy":null,"grant_used":null,"hash":"{}","policy_epoch":null,"policy_pack":null,"principal":"{}","recorded_at":1800000000000,"resource":{{"command":"dev.policy.read"}},"result":"allowed","sequence":4,"work_unit":null}}"#,
                nc_canon::to_hex(&bare.hash),
                agent.as_str()
            )
        );
        record.actor_chain = vec![person.clone(), earlier.clone()];
        record.deciding_policy = Some("family.a_member_drives_the_interface".to_owned());
        record.policy_pack = Some("family@1".to_owned());
        record.policy_epoch = Some(7);
        record.grant_used = Some(grant.clone());
        record.work_unit = Some(unit);
        record.result = nc_journal::Outcome::Denied;
        let full = nc_journal::Entry::chain(5, Some(bare.hash), record);
        assert_eq!(
            entry(&full).to_canonical(),
            format!(
                r#"{{"action":"Read","actor_chain":["{}","{}"],"deciding_policy":"family.a_member_drives_the_interface","grant_used":"{}","hash":"{}","policy_epoch":7,"policy_pack":"family@1","principal":"{}","recorded_at":1800000000000,"resource":{{"command":"dev.policy.read"}},"result":"denied","sequence":5,"work_unit":"{}"}}"#,
                person.as_str(),
                earlier.as_str(),
                grant.as_str(),
                nc_canon::to_hex(&full.hash),
                agent.as_str(),
                unit.to_crockford()
            )
        );
        assert_ne!(bare.hash, full.hash);
        assert_ne!(person, earlier);
    }

    /// A strategy is answered by the word `catalog/limits.toml` writes it
    /// with, for every table the catalog bounds.
    #[test]
    fn a_strategy_is_answered_by_the_word_the_registry_writes() {
        let registry = include_str!("../../../catalog/limits.toml");
        let mut seen = BTreeSet::new();
        for table in nc_catalog::TABLES {
            let section = registry
                .split(&format!("\n[tables.{}]\n", table.name))
                .nth(1)
                .unwrap_or_else(|| panic!("`{}` is a table of the registry", table.name));
            let section = section.split("\n[").next().expect("a section");
            let word = strategy(table.strategy);
            assert!(
                section.contains(&format!("\nstrategy = \"{word}\"\n")),
                "`{}` is not written as `{word}`",
                table.name
            );
            seen.insert(word);
        }
        assert_eq!(
            seen,
            BTreeSet::from(["oldest-first", "policy-pack", "refuse-at-limit"]),
            "every strategy is some table's"
        );
        assert_eq!(strategy(Strategy::OldestFirst), "oldest-first");
        assert_eq!(strategy(Strategy::PolicyPack), "policy-pack");
        assert_eq!(strategy(Strategy::RefuseAtLimit), "refuse-at-limit");
    }

    /// Every count is answered beside its own limit, under the name the
    /// registry gives it, and `null` where nothing says how many are in use.
    #[test]
    fn every_count_is_answered_beside_its_own_limit() {
        let said = |used: Used| {
            counts(used)
                .iter()
                .map(Value::to_canonical)
                .collect::<Vec<String>>()
        };
        let all = said(Used {
            log_lines: 1,
            connections: Some(2),
            sessions: Some(3),
            pending: 4,
            queued: 5,
        });
        assert_eq!(
            all,
            [
                format!(
                    r#"{{"limit":"daemon.dev_logs_ring_lines","max":{},"used":1}}"#,
                    DAEMON.dev_logs_ring_lines
                ),
                format!(
                    r#"{{"limit":"daemon.ipc_connections_max","max":{},"used":2}}"#,
                    DAEMON.ipc_connections_max
                ),
                format!(
                    r#"{{"limit":"daemon.sessions_max","max":{},"used":3}}"#,
                    DAEMON.sessions_max
                ),
                format!(
                    r#"{{"limit":"kernel.confirmations_pending_max","max":{},"used":4}}"#,
                    KERNEL.confirmations_pending_max
                ),
                format!(
                    r#"{{"limit":"kernel.event_queue","max":{},"used":5}}"#,
                    KERNEL.event_queue
                ),
            ]
        );
        let registry = include_str!("../../../catalog/limits.toml");
        for row in counts(Used {
            log_lines: 0,
            connections: None,
            sessions: None,
            pending: 0,
            queued: 0,
        }) {
            let fields = row.into_object().expect("a row");
            let named = fields["limit"].clone().into_text().expect("a name");
            let (table, limit) = named.split_once('.').expect("a table and a limit");
            let section = registry
                .split(&format!("\n[{table}]\n"))
                .nth(1)
                .unwrap_or_else(|| panic!("`[{table}]` is a table of the registry"));
            let section = section.split("\n[").next().expect("a section");
            assert!(
                section.contains(&format!("\n{limit} = ")),
                "`{named}` is no limit of the registry"
            );
        }
        let unhosted = said(Used {
            log_lines: 0,
            connections: None,
            sessions: None,
            pending: 0,
            queued: 0,
        });
        assert!(unhosted[1].ends_with(r#""used":null}"#), "{unhosted:?}");
        assert!(unhosted[2].ends_with(r#""used":null}"#), "{unhosted:?}");
        assert!(unhosted[0].ends_with(r#""used":0}"#), "{unhosted:?}");
    }

    /// The tables of agents are two the catalog bounds in the local
    /// database, in its order.
    #[test]
    fn the_tables_of_agents_are_two_the_catalog_bounds_in_the_local_database() {
        let bounded: Vec<&str> = nc_catalog::TABLES
            .iter()
            .filter(|table| {
                table.database == Database::LocalFile && TABLES_OF_AGENTS.contains(&table.name)
            })
            .map(|table| table.name)
            .collect();
        assert_eq!(bounded, TABLES_OF_AGENTS);
        assert_eq!(TABLES_OF_AGENTS, ["agent_grants", "agents"]);
    }
}
