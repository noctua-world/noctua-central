//! The shape of every registry, as the compiler holds it after parsing.
//!
//! The enums here are the first line of validation and the cheapest: a severity
//! that is not one of four, a reversibility that is not one of three, a field
//! type that is not one of the seven a log event may carry — each of those is a
//! `serde` error with a span, which becomes a located diagnostic for free. Only
//! what a type cannot express is checked by hand afterwards.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Everything `catalog/` declares, parsed and validated.
#[derive(Debug, Clone)]
pub struct Catalog {
    /// The fixed area list, area to what it covers.
    pub areas: BTreeMap<String, String>,
    /// Every error code, in declaration order.
    pub errors: Vec<ErrorEntry>,
    /// Every notification a user can receive.
    pub notifications: Vec<Notification>,
    /// Every transactional email.
    pub emails: Vec<Email>,
    /// Every safety limit and tunable.
    pub limits: Limits,
    /// Every command, with the metadata a dispatcher needs.
    pub commands: Vec<CommandEntry>,
    /// Every event the central log can receive.
    pub events: Vec<EventEntry>,
    /// Every capability a plugin can be granted, in declaration order.
    pub capabilities: Vec<CapabilityEntry>,
    /// Every interface of the plugin world granted to every plugin, and why.
    pub ungated: Vec<UngatedEntry>,
    /// Every locale, source locale first.
    pub locales: Vec<Locale>,
}

// --- errors.toml ---

/// The parsed file, before the area cross-check.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ErrorFile {
    pub(crate) areas: BTreeMap<String, String>,
    #[serde(default, rename = "error")]
    pub(crate) errors: Vec<ErrorEntry>,
}

/// One error code (`NOCTUA.md §15.1`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ErrorEntry {
    /// `NC-<AREA>-<NNNN>`.
    pub code: String,
    /// One line, English, for a developer. Never shown to a user.
    pub description: String,
    /// The i18n key of what a user is shown.
    pub message: String,
    /// How bad it is.
    pub severity: Severity,
    /// Whether trying the same thing again could succeed.
    pub retryable: bool,
    /// The placeholders the user-facing message interpolates.
    ///
    /// Checked against the sentence rather than trusted: a message asking for a
    /// name the caller does not pass reaches a real person with the braces still
    /// in it, and `NOCTUA.md §10.9` makes a response a key **plus parameters**
    /// rather than a rendered sentence.
    #[serde(default)]
    pub params: Vec<String>,
    /// What this code is today: something the platform produces, or a number
    /// held for a stage that is not built.
    ///
    /// **A page saying "every error the platform can return" must not say it
    /// over codes nothing raises.** Measured by looking for `ErrorCode::…` outside the
    /// generated crate and outside test modules: `NC-DB-0002`, `NC-LOG-0001`,
    /// `NC-LOG-0002`, `NC-POL-0001` and `NC-POL-0002` have no producer at all,
    /// and `NC-IDS-0002` is recorded rather than returned — a fact that could
    /// only live in a TOML comment, where the generated page could not read it.
    ///
    /// Defaults to [`Raising::Raised`], because that is what an error is unless
    /// somebody says otherwise, and a default that had to be written on every
    /// entry would be a field nobody maintains. `catalog-raising` is the gate
    /// that keeps each value true.
    #[serde(default = "raised")]
    pub raising: Raising,
}

/// The default for [`ErrorEntry::raising`], which serde needs as a function
/// rather than a literal.
const fn raised() -> Raising {
    Raising::Raised
}

/// Whether anything in this tree produces a code.
///
/// **Two states, not three.** A third, "produced, and only a log carries it",
/// is not a thing a gate can check: the gate looks for the code's variant in
/// `crates/`, which says *produced* and says nothing about who reads the
/// result. So it would collapse with `Raised` in the check, and declaring a
/// `critical` chain-broken error as that third state would pass with a
/// byte-identical line while `docs/errors.md` told readers it never reaches a
/// caller.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Raising {
    /// The platform produces it and a caller receives it.
    Raised,
    /// Nothing produces it yet. The number is held for the stage that will, and
    /// reserving is append-only, so it cannot be given back.
    Reserved,
}

impl Raising {
    /// The sentence a page publishes for it.
    #[must_use]
    pub const fn written(self) -> &'static str {
        match self {
            Self::Raised => "yes",
            Self::Reserved => "not yet — the number is held",
        }
    }
}

/// How much attention something deserves.
///
/// One scale for errors and for notifications — see the generated crate's own
/// doc for why.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Something a person should be woken for.
    Critical,
    /// The operation failed.
    Error,
    /// The operation continued, degraded.
    Warning,
    /// Worth recording, nothing failed.
    Info,
}

impl Severity {
    /// The generated Rust variant name.
    pub(crate) const fn variant(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::Error => "Error",
            Self::Warning => "Warning",
            Self::Info => "Info",
        }
    }
}

// --- notifications.toml and emails.toml ---

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NotificationFile {
    #[serde(default, rename = "notification")]
    pub(crate) notifications: Vec<Notification>,
}

/// One notification a user can receive.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    /// Stable, dotted, and what the generated handle is named after.
    pub id: String,
    /// The i18n key of the title.
    pub title: String,
    /// The i18n key of the body.
    pub body: String,
    /// The placeholders the body may interpolate.
    #[serde(default)]
    pub params: Vec<String>,
    /// How prominently it is shown.
    pub severity: Severity,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmailFile {
    #[serde(default, rename = "email")]
    pub(crate) emails: Vec<Email>,
}

/// One transactional email.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Email {
    /// Stable, dotted, and what the generated handle is named after.
    pub id: String,
    /// The i18n key of the subject.
    pub subject: String,
    /// The i18n key of the body.
    pub body: String,
    /// The placeholders the body may interpolate.
    #[serde(default)]
    pub params: Vec<String>,
}

// --- limits.toml ---

/// Every safety limit and tunable (`NOCTUA.md §15.8`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Tables in either database, which the pruner owns.
    #[serde(default)]
    pub tables: BTreeMap<String, TableLimit>,
    /// Stores outside PostgreSQL, bounded by their own mechanism.
    #[serde(default)]
    pub stores: BTreeMap<String, StoreLimit>,
    /// Key-derivation parameters (`NOCTUA.md §10.5`, `§10.6`).
    pub argon2id: Hashing,
    /// The authentication-surface rate limits (`NOCTUA.md §15.5`).
    #[serde(default, rename = "rate_limit")]
    pub rate_limits: Vec<RateLimit>,
    /// The bounds the local kernel holds every plugin to (`NOCTUA.md §11.3`).
    pub kernel: KernelLimits,
}

/// The bounds the local kernel holds every plugin to (`NOCTUA.md §11.3`).
///
/// **Every one required, and every one a number above zero.** The kernel reads
/// these and nothing else: a plugin's manifest may lower a few of them and
/// raise none, so a limit missing here would be one the kernel had to invent,
/// which is the second source of a number `§15.2` exists to prevent. The
/// relations between them are checked where they are read (`load.rs`).
///
/// Two limits a reader might look for here are not, on purpose: how many keys
/// one plugin may keep and how many writes one unit may make are the
/// `store_entries` and `store_writes` rows of `[tables.*]`, because their
/// writer refuses at them — one number, held where the table is bounded.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(Default))]
#[serde(deny_unknown_fields)]
pub struct KernelLimits {
    /// How large one plugin's linear memory may grow, in bytes.
    pub memory_bytes: u64,
    /// How many instances one plugin's store may hold.
    pub instances: u32,
    /// How many tables one plugin's store may hold.
    pub tables: u32,
    /// How many elements one table may hold.
    pub table_elements: u32,
    /// How deep one plugin's WebAssembly stack may grow, in bytes.
    pub wasm_stack_bytes: u64,
    /// How much one dispatch to a plugin may compute, in units of fuel.
    pub fuel_per_dispatch: u64,
    /// How much one plugin's `activate` may compute, in units of fuel.
    pub fuel_per_activate: u64,
    /// How often the epoch every call's deadline is counted in advances.
    pub epoch_tick_ms: u64,
    /// How many ticks of the epoch one call into a plugin may run for.
    pub epoch_deadline_ticks: u64,
    /// How long one host function may take before its caller is killed.
    pub host_call_deadline_ms: u64,
    /// How long one call tree — a dispatch and all it dispatches — may take.
    pub dispatch_deadline_ms: u64,
    /// How deep the dispatches of one call tree may nest.
    pub dispatch_depth: u32,
    /// How many dispatches one call tree may make.
    pub dispatches_per_tree: u32,
    /// How many crashes inside the window quarantine a plugin; a kill at its
    /// call tree's deadline is not one.
    pub restart_budget: u32,
    /// The window a plugin's crashes are counted in.
    pub restart_window_ms: u64,
    /// How large the arguments or the result of one dispatch may be.
    pub payload_bytes: u64,
    /// How many commands one plugin may declare.
    pub commands_per_plugin: u32,
    /// How long one store key may be.
    pub store_key_bytes: u64,
    /// How large one store value may be.
    pub store_value_bytes: u64,
    /// How much one unit of work may stage, its keys and the values they held
    /// and hold counted together.
    pub unit_bytes: u64,
    /// How many keys one listing of a store may return.
    pub store_list_max: u32,
    /// How large one blob may be.
    pub blob_bytes: u64,
    /// How much one read of a blob may return.
    pub blob_read_chunk_bytes: u64,
    /// How long a blob no unit holds keeps its bytes.
    pub blob_orphan_grace_ms: u64,
    /// How many blob readers and writers one plugin may hold open at once.
    pub blob_handles: u32,
    /// How many units one plugin may hold begun and not yet let go of inside
    /// one command.
    pub unit_handles: u32,
    /// How long one network request may take, from connecting to its last byte.
    pub net_timeout_ms: u64,
    /// How large the URL and the headers of one network request may be,
    /// counted together.
    pub net_request_head_bytes: u64,
    /// How large the body of one network request may be.
    pub net_request_body_bytes: u64,
    /// How large the body of one network response may be.
    pub net_response_body_bytes: u64,
    /// How many headers one network request or response may carry.
    pub net_header_count: u32,
    /// How many timers one plugin may hold at once.
    pub timers_per_plugin: u32,
    /// The shortest interval a repeating timer may be set to.
    pub timer_min_interval_ms: u64,
    /// How far ahead a timer may be set.
    pub timer_max_delay_ms: u64,
    /// How many published events may wait for delivery.
    pub event_queue: u32,
    /// How many bytes the published events waiting for delivery may hold,
    /// their topics and their data counted together.
    pub event_queue_bytes: u64,
    /// How many surfaces one plugin may hold open.
    pub surfaces_per_plugin: u32,
    /// How many nodes one rendered tree may hold.
    pub ui_tree_nodes: u32,
    /// How deep one rendered tree may nest.
    pub ui_tree_depth: u32,
    /// How long one text in a rendered tree may be.
    pub ui_text_bytes: u64,
    /// How many lines of its own log the kernel keeps for one plugin.
    pub log_ring_lines: u32,
    /// How long one line of a plugin's log may be.
    pub log_line_bytes: u64,
    /// How many of a person's newest steps an undo looks through, and of
    /// their newest units a redo does.
    pub undo_history: u32,
    /// How long a confirmation a person gave stays good for.
    pub confirmation_ttl_ms: u64,
}

/// What one kernel limit is counted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Bytes, which a page shows in the largest binary unit that divides them.
    Bytes,
    /// Milliseconds.
    Milliseconds,
    /// The fuel wasmtime counts, which a plugin's instructions consume.
    Fuel,
    /// Ticks of the epoch `epoch_tick_ms` advances.
    Ticks,
    /// Things, counted.
    Count,
}

impl Unit {
    /// What a reader is told the number is in, or nothing for a count.
    pub(crate) const fn in_words(self) -> Option<&'static str> {
        match self {
            Self::Bytes => Some("bytes"),
            Self::Milliseconds => Some("milliseconds"),
            Self::Fuel => Some("units of fuel"),
            Self::Ticks => Some("ticks of the epoch"),
            Self::Count => None,
        }
    }
}

/// One kernel limit as the generated crate and the page carry it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelLimit {
    /// The key `[kernel]` writes it under, which is also the generated field.
    pub name: &'static str,
    /// Its value.
    pub value: u64,
    /// Whether the generated field is a `u64` rather than a `u32`.
    pub wide: bool,
    /// What the number is counted in.
    pub unit: Unit,
    /// What it bounds, as one sentence.
    pub bounds: &'static str,
}

impl KernelLimits {
    /// Every limit, in the order `[kernel]` is documented in.
    ///
    /// **Destructured, so a field the struct gains is a compile error here**
    /// rather than a limit the generated crate carries and the page does not
    /// publish — or one the emitter writes with no sentence saying what it
    /// bounds. The sentence is the field's own doc above, written once more
    /// here because a doc comment is not a value a program can read.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "one row per limit and nothing else: splitting the table would split \
                  the destructuring that makes a forgotten limit a compile error"
    )]
    pub fn each(&self) -> Vec<KernelLimit> {
        let Self {
            memory_bytes,
            instances,
            tables,
            table_elements,
            wasm_stack_bytes,
            fuel_per_dispatch,
            fuel_per_activate,
            epoch_tick_ms,
            epoch_deadline_ticks,
            host_call_deadline_ms,
            dispatch_deadline_ms,
            dispatch_depth,
            dispatches_per_tree,
            restart_budget,
            restart_window_ms,
            payload_bytes,
            commands_per_plugin,
            store_key_bytes,
            store_value_bytes,
            unit_bytes,
            store_list_max,
            blob_bytes,
            blob_read_chunk_bytes,
            blob_orphan_grace_ms,
            blob_handles,
            unit_handles,
            net_timeout_ms,
            net_request_head_bytes,
            net_request_body_bytes,
            net_response_body_bytes,
            net_header_count,
            timers_per_plugin,
            timer_min_interval_ms,
            timer_max_delay_ms,
            event_queue,
            event_queue_bytes,
            surfaces_per_plugin,
            ui_tree_nodes,
            ui_tree_depth,
            ui_text_bytes,
            log_ring_lines,
            log_line_bytes,
            undo_history,
            confirmation_ttl_ms,
        } = *self;
        let wide = |name, value, unit, bounds| KernelLimit {
            name,
            value,
            wide: true,
            unit,
            bounds,
        };
        let narrow = |name, value: u32, bounds| KernelLimit {
            name,
            value: u64::from(value),
            wide: false,
            unit: Unit::Count,
            bounds,
        };
        vec![
            wide(
                "memory_bytes",
                memory_bytes,
                Unit::Bytes,
                "How large one plugin's linear memory may grow",
            ),
            narrow(
                "instances",
                instances,
                "How many instances one plugin's store may hold",
            ),
            narrow(
                "tables",
                tables,
                "How many tables one plugin's store may hold",
            ),
            narrow(
                "table_elements",
                table_elements,
                "How many elements one table may hold",
            ),
            wide(
                "wasm_stack_bytes",
                wasm_stack_bytes,
                Unit::Bytes,
                "How deep one plugin's WebAssembly stack may grow",
            ),
            wide(
                "fuel_per_dispatch",
                fuel_per_dispatch,
                Unit::Fuel,
                "How much one dispatch to a plugin may compute",
            ),
            wide(
                "fuel_per_activate",
                fuel_per_activate,
                Unit::Fuel,
                "How much one plugin's activate may compute",
            ),
            wide(
                "epoch_tick_ms",
                epoch_tick_ms,
                Unit::Milliseconds,
                "How often the epoch every call's deadline is counted in advances",
            ),
            wide(
                "epoch_deadline_ticks",
                epoch_deadline_ticks,
                Unit::Ticks,
                "How long one call into a plugin may run for",
            ),
            wide(
                "host_call_deadline_ms",
                host_call_deadline_ms,
                Unit::Milliseconds,
                "How long one host function may take before the plugin that called it is \
                 killed",
            ),
            wide(
                "dispatch_deadline_ms",
                dispatch_deadline_ms,
                Unit::Milliseconds,
                "How long one call tree — a dispatch and all it dispatches — may take",
            ),
            narrow(
                "dispatch_depth",
                dispatch_depth,
                "How deep the dispatches of one call tree may nest",
            ),
            narrow(
                "dispatches_per_tree",
                dispatches_per_tree,
                "How many dispatches one call tree may make",
            ),
            narrow(
                "restart_budget",
                restart_budget,
                "How many crashes inside the restart window quarantine a plugin; a kill at its \
                 call tree's deadline is not one",
            ),
            wide(
                "restart_window_ms",
                restart_window_ms,
                Unit::Milliseconds,
                "The window a plugin's crashes are counted in",
            ),
            wide(
                "payload_bytes",
                payload_bytes,
                Unit::Bytes,
                "How large the arguments or the result of one dispatch may be",
            ),
            narrow(
                "commands_per_plugin",
                commands_per_plugin,
                "How many commands one plugin may declare",
            ),
            wide(
                "store_key_bytes",
                store_key_bytes,
                Unit::Bytes,
                "How long one store key may be",
            ),
            wide(
                "store_value_bytes",
                store_value_bytes,
                Unit::Bytes,
                "How large one store value may be",
            ),
            wide(
                "unit_bytes",
                unit_bytes,
                Unit::Bytes,
                "How much one unit of work may stage, its keys and the values they held and \
                 hold counted together",
            ),
            narrow(
                "store_list_max",
                store_list_max,
                "How many keys one listing of a store may return",
            ),
            wide(
                "blob_bytes",
                blob_bytes,
                Unit::Bytes,
                "How large one blob may be",
            ),
            wide(
                "blob_read_chunk_bytes",
                blob_read_chunk_bytes,
                Unit::Bytes,
                "How much one read of a blob may return",
            ),
            wide(
                "blob_orphan_grace_ms",
                blob_orphan_grace_ms,
                Unit::Milliseconds,
                "How long a blob no unit holds keeps its bytes, so that a redo can hold it \
                 again",
            ),
            narrow(
                "blob_handles",
                blob_handles,
                "How many blob readers and writers one plugin may hold open at once",
            ),
            narrow(
                "unit_handles",
                unit_handles,
                "How many units one plugin may hold begun and not yet let go of inside one \
                 command",
            ),
            wide(
                "net_timeout_ms",
                net_timeout_ms,
                Unit::Milliseconds,
                "How long one network request may take, from connecting to its last byte",
            ),
            wide(
                "net_request_head_bytes",
                net_request_head_bytes,
                Unit::Bytes,
                "How large the URL and the headers of one network request may be, counted \
                 together",
            ),
            wide(
                "net_request_body_bytes",
                net_request_body_bytes,
                Unit::Bytes,
                "How large the body of one network request may be",
            ),
            wide(
                "net_response_body_bytes",
                net_response_body_bytes,
                Unit::Bytes,
                "How large the body of one network response may be",
            ),
            narrow(
                "net_header_count",
                net_header_count,
                "How many headers one network request or response may carry",
            ),
            narrow(
                "timers_per_plugin",
                timers_per_plugin,
                "How many timers one plugin may hold at once",
            ),
            wide(
                "timer_min_interval_ms",
                timer_min_interval_ms,
                Unit::Milliseconds,
                "The shortest interval a repeating timer may be set to",
            ),
            wide(
                "timer_max_delay_ms",
                timer_max_delay_ms,
                Unit::Milliseconds,
                "How far ahead a timer may be set",
            ),
            narrow(
                "event_queue",
                event_queue,
                "How many published events may wait for delivery",
            ),
            wide(
                "event_queue_bytes",
                event_queue_bytes,
                Unit::Bytes,
                "How many bytes the published events waiting for delivery may hold, their topics and their data counted together",
            ),
            narrow(
                "surfaces_per_plugin",
                surfaces_per_plugin,
                "How many surfaces one plugin may hold open",
            ),
            narrow(
                "ui_tree_nodes",
                ui_tree_nodes,
                "How many nodes one rendered tree may hold",
            ),
            narrow(
                "ui_tree_depth",
                ui_tree_depth,
                "How deep one rendered tree may nest",
            ),
            wide(
                "ui_text_bytes",
                ui_text_bytes,
                Unit::Bytes,
                "How long one text in a rendered tree may be",
            ),
            narrow(
                "log_ring_lines",
                log_ring_lines,
                "How many lines of its own log the kernel keeps for one plugin",
            ),
            wide(
                "log_line_bytes",
                log_line_bytes,
                Unit::Bytes,
                "How long one line of a plugin's log may be",
            ),
            narrow(
                "undo_history",
                undo_history,
                "How many of a person's newest steps an undo looks through, and of their newest units a redo does",
            ),
            wide(
                "confirmation_ttl_ms",
                confirmation_ttl_ms,
                Unit::Milliseconds,
                "How long a confirmation a person gave stays good for",
            ),
        ]
    }
}

/// What bounds one table, in either database.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TableLimit {
    /// Which database holds it, and so which migrations must create it.
    ///
    /// **`server` when it is not written**, so an entry saying nothing is a
    /// PostgreSQL table. The gate over the migrations reads it both ways: a table created
    /// in `local-migrations/` is bounded only by an entry that says `local`,
    /// and one in `migrations/` only by one that does not.
    #[serde(default)]
    pub database: Database,
    /// How rows are chosen for removal.
    pub strategy: Strategy,
    /// The column that orders the table for pruning. Oldest is smallest.
    pub order_by: String,
    /// The row count above which pruning runs. Absent under `policy-pack`;
    /// under `refuse-at-limit`, the count at which the writer refuses a new row.
    pub max_rows: Option<u64>,
    /// Whose rows `max_rows` counts, when it is not the whole table's.
    ///
    /// **Only under `refuse-at-limit`, where a writer can count one owner's
    /// rows.** The pruner always counts a whole table, and a `policy-pack`
    /// table has no number to count. Absent, the count is the whole table's.
    /// Without it, `docs/limits.md` publishes `store_entries`' quota as the
    /// table's bound while its writer refuses per plugin in each realm, so the
    /// table holds that number for every plugin installed in every realm and
    /// only a comment in `catalog/limits.toml` says so.
    pub per: Option<Owner>,
    /// How many rows one pruning run removes. Absent under `policy-pack`.
    pub prune_batch: Option<u64>,
    /// Why this table is bounded by a policy pack rather than by a number here.
    ///
    /// **Required under `policy-pack`, and refused otherwise.** That strategy is
    /// an exemption from the gate `§15.8` exists to be: the table declares no
    /// bound and the pruner never touches it — so without a required reason an
    /// unbounded, never-pruned table could enter the tree with three lines of
    /// TOML and no reason written anywhere a parser reads.
    ///
    /// Its sibling hatch, `nc:bounded:`, demands a non-empty reason and is
    /// counted with the rest. This is that rule, applied to this hatch.
    pub pack_reason: Option<String>,
}

/// How a table's rows are chosen for removal.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Strategy {
    /// The oldest rows go first, in batches.
    OldestFirst,
    /// **The pruner does not touch this table.** Its retention is a policy-pack
    /// property, per realm, and a global constant here would silently prune
    /// below what a pack promised (`NOCTUA.md §15.8`).
    PolicyPack,
    /// The pruner leaves this table alone too, and it is bounded anyway.
    ///
    /// **Its oldest rows are not the pruner's to take.** It is user state —
    /// what a plugin keeps, a blob a unit pinned — whose oldest rows are
    /// somebody's data, or rows that belong to one row of another table and
    /// go with it — a unit's images and notes, which an undo of that unit
    /// needs whole and which leave by the cascade when the unit does. So
    /// `max_rows` is where its writer refuses a new row instead, and the
    /// refusal is the bound. Which rows the count is taken over is the
    /// writer's, and the entry's `per` says which: absent, the whole table's.
    RefuseAtLimit,
}

/// Whose rows a limit counts when it is not the whole table's.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Owner {
    /// Each plugin's own rows in each realm: a plugin's quota in one realm,
    /// which every plugin installed in every realm has in full.
    Plugin,
    /// Each unit's own rows: what one commit may record, which every unit the
    /// store holds may have in full.
    Unit,
}

impl Owner {
    /// What a reader is shown under `Counted over`.
    pub(crate) const fn counted_over(self) -> &'static str {
        match self {
            Self::Plugin => "each plugin's rows in each realm",
            Self::Unit => "each unit's rows",
        }
    }
}

/// Which database a table lives in.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Database {
    /// PostgreSQL, built by `migrations/`.
    #[default]
    Server,
    /// The SQLite file on the user's machine, built by `local-migrations/`.
    ///
    /// Written `local` in the registry, and not `Local` here, because that name
    /// is `chrono`'s clock and the determinism gate reads it as one.
    #[serde(rename = "local")]
    LocalFile,
}

impl Database {
    /// What a reader is shown, which is the registry's own spelling.
    pub(crate) const fn written(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::LocalFile => "local",
        }
    }
}

impl Strategy {
    /// What a reader is shown, which is the registry's own spelling.
    ///
    /// **Not `{:?}`.** Through `{:?}`, `docs/limits.md` publishes `OldestFirst`
    /// where `catalog/limits.toml` says `strategy = "oldest-first"`, so the one
    /// page a reader is sent to spells the value differently from the file they
    /// have to edit — and a variant rename changes the published page with
    /// nothing to notice, because a test that pins it through `{:?}` moves too.
    pub(crate) const fn written(self) -> &'static str {
        match self {
            Self::OldestFirst => "oldest-first",
            Self::PolicyPack => "policy-pack",
            Self::RefuseAtLimit => "refuse-at-limit",
        }
    }
}

/// What bounds one store that is not a PostgreSQL table.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoreLimit {
    /// How long data is kept.
    pub retention_days: u32,
    /// The disk ceiling, in bytes.
    pub max_disk_bytes: u64,
    /// What actually enforces the two numbers above.
    pub enforced_by: String,
}

/// The two Argon2id configurations this platform derives keys with.
///
/// **Two, not one.** A password is at least twelve characters (`§10.5`); a
/// device PIN is four to eight digits (`§10.6`), which is thirteen to
/// twenty-seven bits of entropy and is only safe because the stored credential
/// is *encrypted* with the derived key and wiped after N failures. Deriving both
/// with one cost either makes every login slow or makes the PIN cheap to attack
/// offline, and a single `ARGON2ID` constant is how the second happens quietly.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Hashing {
    /// What an account password is hashed with.
    pub password: Argon2Id,
    /// What a device PIN's key is derived with.
    pub device_pin: Argon2Id,
}

/// One Argon2id configuration.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Argon2Id {
    /// Memory cost, in kibibytes. The parameter that costs an attacker.
    pub memory_kib: u32,
    /// Time cost.
    pub iterations: u32,
    /// Degree of parallelism.
    pub parallelism: u32,
    /// Length of the derived hash, in bytes.
    pub output_length: u32,
    /// Length of the salt, in bytes.
    pub salt_length: u32,
}

/// One authentication surface's rate limits (`NOCTUA.md §15.5`).
///
/// **Two budgets, not one.** §15.5 requires "per-identifier **and** per-IP rate
/// limits with escalating delay, on every authentication surface", and they are
/// different defences: the per-identifier budget stops somebody grinding one
/// account, and the per-address budget stops somebody spraying one password
/// across many. One number cannot be both, and a gateway handed one number would
/// invent the other — which is the second implementation the catalog exists to
/// prevent (`§10.8`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RateLimit {
    /// Which surface it guards.
    pub surface: String,
    /// What one identifier — an account, a device, an email — may attempt.
    pub per_identifier: Budget,
    /// What one client address may attempt.
    pub per_address: Budget,
}

/// One rate-limit budget.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// The window attempts are counted in.
    pub window_seconds: u32,
    /// Attempts allowed in the window **before the delay curve starts**.
    ///
    /// **Not "attempts allowed", full stop.** Read that way it contradicts the
    /// rule beside it: `load.rs` requires `lockout_after >= max_attempts` because
    /// attempts *continue* past this number under an escalating delay — *"or the
    /// escalating delay never runs and the curve above it is decoration"*. Both
    /// sentences cannot be true, and a page carrying the first makes three of
    /// its own rows unreachable: `after 10 failed attempts` beside
    /// `Attempts 5` reads as a lockout that can never happen.
    ///
    /// What actually happens: the first `max_attempts` are answered at once, the
    /// ones after that are answered more and more slowly, and at `lockout_after`
    /// the surface stops answering.
    pub max_attempts: u32,
    /// The escalating delay after each failure, in milliseconds. The last value
    /// repeats. Checked to be non-decreasing **and to rise**: §15.5 says
    /// *escalating*, and a flat curve satisfies "has a curve" while defending
    /// nothing. `curve-descends` refuses a curve that falls and `curve-is-flat`
    /// refuses one that ends where it began, including a curve of one value,
    /// whose one value repeats for ever.
    pub delay_ms: Vec<u32>,
    /// The failure count at which the surface stops answering at all.
    pub lockout_after: Option<u32>,
    /// Whether the lockout count survives the window.
    ///
    /// `NOCTUA.md §10.6` wipes the device credential "after N failures" —
    /// cumulative. A windowed count resets forever, and a four-digit PIN behind
    /// eight attempts an hour falls in weeks.
    #[serde(default)]
    pub cumulative: bool,
    /// What happens at lockout, when it is more than refusing.
    pub on_lockout: Option<OnLockout>,
}

/// What a surface does when a budget is exhausted.
///
/// An enum and not a string: as a string, `on_lockout = "wipe-credentail"`
/// parses as a value nothing recognises, and the device PIN silently stops
/// wiping.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OnLockout {
    /// Stop answering until the window passes.
    Refuse,
    /// Destroy the stored device credential, forcing a full re-authentication.
    WipeCredential,
}

// --- commands.toml ---

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandFile {
    #[serde(default, rename = "command")]
    pub(crate) commands: Vec<CommandEntry>,
}

/// One command, with everything a dispatcher and a PDP need
/// (`NOCTUA.md §11.4`, `§11.7`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommandEntry {
    /// Dotted, and unique platform-wide.
    pub name: String,
    /// The i18n key of the human name.
    pub title: String,
    /// The i18n key of one line saying what it does.
    pub description: String,
    /// The Cedar action name the PDP evaluates.
    pub action: String,
    /// How the kernel extracts the resource from the arguments.
    pub resource_selector: String,
    /// Whether it can be undone.
    pub reversibility: Reversibility,
    /// Whether it changes state at all.
    pub mutating: bool,
    /// The JSON Schema for its arguments — and its AI tool interface.
    pub args_schema: String,
    /// The default keybinding, where it has one (`NOCTUA.md §12.8`).
    pub keybinding: Option<String>,
}

/// Whether a command can be undone (`NOCTUA.md §14.6`).
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Reversibility {
    /// Changes nothing.
    Read,
    /// Changes state; `kernel.undo` takes back what a step of it kept in the
    /// store and the blobs it made, stops at a step that left the machine or
    /// cannot be undone with no compensation registered, and leaves a timer it
    /// armed or an event it published as it is.
    Write,
    /// Cannot be reversed, and therefore requires `confirmed_by_human`.
    Irreversible,
}

impl Reversibility {
    /// The generated Rust variant name.
    pub(crate) const fn variant(self) -> &'static str {
        match self {
            Self::Read => "Read",
            Self::Write => "Write",
            Self::Irreversible => "Irreversible",
        }
    }
}

// --- capabilities.toml ---

/// The parsed file: every capability, and every interface given without asking.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapabilityFile {
    #[serde(default, rename = "capability")]
    pub(crate) capabilities: Vec<CapabilityEntry>,
    #[serde(default)]
    pub(crate) ungated: Vec<UngatedEntry>,
}

/// One capability a plugin can be granted (`NOCTUA.md §11.3`).
///
/// **The functions it covers are named as the world names them**, so the
/// kernel's coverage test can hold the list to the world without a mapping of
/// its own: `[method]reader.size` is how `wit-parser` names a resource's method,
/// and `list` is `store`'s function whatever escape the WIT writes it with.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityEntry {
    /// What a manifest writes to ask for it: its interface, or its interface,
    /// a dot and a word.
    pub name: String,
    /// The interface of the world whose functions it grants.
    pub interface: String,
    /// Every function of that interface it grants. Empty for a capability that
    /// grants a declaration rather than a call, which `events.subscribe` is.
    pub functions: Vec<String>,
    /// The i18n key of the sentence saying what it lets a plugin do.
    pub description: String,
}

/// One interface of the world every plugin is given without asking.
///
/// **Its functions are written down as a capability's are**, and required:
/// the reason covers the functions it was written beside, so a function the
/// world gains in this interface is one the kernel's coverage test finds in no
/// list until someone writes it here, rather than one every plugin is given
/// without anybody having decided to.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UngatedEntry {
    /// The interface.
    pub interface: String,
    /// Every function of that interface every plugin is given, as the world's
    /// parser names them. Empty for an interface that declares only types.
    pub functions: Vec<String>,
    /// Why giving it to every plugin grants nothing a plugin could misuse.
    pub reason: String,
}

// --- log-events.toml ---

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventFile {
    #[serde(default, rename = "event")]
    pub(crate) events: Vec<EventEntry>,
}

/// One event the central log can receive (`NOCTUA.md §15.4`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventEntry {
    /// Dotted, and what the generated struct is named after.
    pub name: String,
    /// One line, English, for a developer.
    pub description: String,
    /// The level it is emitted at.
    pub level: Level,
    /// What it carries. Every type here implements `LogValue`.
    pub fields: Vec<EventField>,
}

/// One field of a log event.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventField {
    /// The field name, `snake_case`.
    pub name: String,
    /// Its type, which is the whole guarantee.
    #[serde(rename = "type")]
    pub kind: FieldType,
    /// Whether it may be absent.
    #[serde(default)]
    pub optional: bool,
}

/// The level an event is emitted at.
///
/// Not `tracing`'s: nothing in this repository depends on that crate. The names
/// are the five the log store reads, and `nc_log::Level` is what carries them.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Very fine detail.
    Trace,
    /// Detail a developer wants while diagnosing.
    Debug,
    /// Something worth a line in normal operation.
    Info,
    /// Degraded, but working.
    Warn,
    /// Failed.
    Error,
}

impl Level {
    /// What a reader is shown, which is the registry's own spelling.
    pub(crate) const fn written(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// What a log-event field may be.
///
/// **There is no variant for a string that came from anywhere but the source.**
/// That absence is the type wall of `NOCTUA.md §15.4`: a dynamic string is the
/// shape every accidental leak of user data takes, and the registry has no way
/// to ask for one.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// An unsigned 64-bit count.
    U64,
    /// A signed 64-bit number.
    I64,
    /// An unsigned 32-bit count.
    U32,
    /// A flag.
    Bool,
    /// A `std::time::Duration`.
    Duration,
    /// A code from `errors.toml`.
    ErrorCode,
    /// An opaque identifier newtype, written to the log as it is.
    ///
    /// **The identifier reaches the log**, which is what `§15.4`'s own table
    /// says the central log contains — "error codes, static messages, opaque
    /// IDs, counts, durations, enum states". Its contents reach the log too:
    /// `OpaqueId` renders whole.
    ///
    /// What makes that safe is that it is an *identifier* and not free text.
    /// What it does not do is stop a caller minting one **out of** personal
    /// data — `OpaqueId::new` accepts anything in its character set, so
    /// `123.456.789-00` is an identifier as far as the type is concerned. The
    /// shape of an opaque identifier is `NOCTUA.md §5.1`, and it is `[OPEN]`; the
    /// residual risk lives there rather than here, and it is written down so the
    /// stage that closes §5.1 knows this reads it.
    Id,
    /// A literal in the source, and nothing else.
    ///
    /// **Named for what it is, not for a lifetime.** A wall built on the
    /// lifetime — `LogValue` for `&'static str`, on the reasoning that a
    /// borrowed `&'a str` cannot coerce to one — is true and beside the point:
    /// `String::leak` manufactures a real `&'static str` from anything at all.
    /// The wall is `nc_log::Literal`, whose field is private and whose
    /// constructor takes **no value at all**: it reads an associated `const`, and
    /// const evaluation happens before any run-time value exists. A macro
    /// matching `$text:literal` is not enough — the expansion needs something
    /// reachable at the caller's site, and that is a hole.
    Literal,
    /// A Snowflake, written as its Crockford base32 text.
    ///
    /// **So the registry can name the type the journal actually holds.**
    /// `Record::work_unit` is an `Option<Snowflake>`; a `work_unit` field
    /// declared as `id` — a `nc_ids::OpaqueId` — makes a caller logging a work
    /// unit convert one into the other, and the conversion available is
    /// `OpaqueId::new(…)`, which accepts anything in its character set. A
    /// missing kind is how a type wall pushes a caller toward the one field that
    /// takes free text.
    ///
    /// Without this kind, `impl LogValue for Snowflake` in `nc-log` is
    /// reachable from nothing, which is the same gap seen from the other end.
    Snowflake,
    /// A variant of an enum the platform declares.
    ///
    /// `NOCTUA.md §15.4` lists enum variants among what `LogValue` carries and
    /// "enum states" among what the central log holds. Without this kind the
    /// registry could not say either, and the state of a state machine would
    /// have had to travel as text.
    Enum,
}

impl FieldType {
    /// What a reader is told the field holds, in the generated struct's doc.
    ///
    /// Written out rather than derived from the Rust type: a doc saying "A
    /// `nc_ids::OpaqueId`." tells a reader what to type and nothing about why —
    /// and the why is the whole of `§15.4`. An identifier is in the log because
    /// its contents are not.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::U64 => "count",
            Self::I64 => "signed number",
            // **Not "small count".** This label becomes the field's doc comment
            // in the generated crate as well as its cell on the page, and the
            // one `u32` this registry declares is an HTTP status — which is not
            // a count of anything. A label for a *type* has to be true of every
            // field that has it, and the registry has no per-field prose for it
            // to defer to.
            Self::U32 => "whole number, at most 32 bits wide",
            Self::Bool => "flag",
            Self::Duration => "span of time",
            Self::ErrorCode => "code from the error registry",
            Self::Id => "opaque identifier, written to the log exactly as it is",
            Self::Snowflake => "Snowflake, written as its Crockford base32 text",
            // Not `literal!`, which is a Rust macro: this column is read by
            // somebody asking what a field can hold, and the answer is not the
            // name of the thing that builds it.
            Self::Literal => "fixed text chosen when the code was written, never at run time",
            Self::Enum => "variant of an enum the platform declares — a state, never a name",
        }
    }

    /// The Rust type the generated struct's field takes.
    ///
    /// Every one of these implements `LogValue`, and `String` is not among them
    /// — which is the whole wall (`NOCTUA.md §15.4`). `Enum` maps to a newtype
    /// rather than to a literal so that an enum state and a source literal stay
    /// distinguishable in the log; both come from the source, but only one is a
    /// state somebody will want to filter on.
    pub(crate) const fn rust(self) -> &'static str {
        match self {
            Self::U64 => "u64",
            Self::I64 => "i64",
            Self::U32 => "u32",
            Self::Bool => "bool",
            Self::Duration => "core::time::Duration",
            Self::ErrorCode => "nc_catalog::ErrorCode",
            Self::Id => "nc_ids::OpaqueId",
            Self::Snowflake => "nc_ids::Snowflake",
            Self::Literal => "crate::Literal",
            Self::Enum => "crate::Variant",
        }
    }
}

// --- i18n ---

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocaleFile {
    pub(crate) meta: LocaleMeta,
    #[serde(default)]
    pub(crate) message: BTreeMap<String, String>,
}

/// What a locale says about itself.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocaleMeta {
    /// The language's name **written in that language**. A switcher that lists
    /// languages in the language you are trying to leave is useless to the
    /// person who needs it.
    pub name: String,
    /// The BCP 47 tag, for `<html lang>`.
    pub bcp47: String,
    /// `ltr` or `rtl`, for `<html dir>`.
    pub dir: String,
}

/// One locale, with every message it carries.
#[derive(Debug, Clone)]
pub struct Locale {
    /// What the locale says about itself.
    pub meta: LocaleMeta,
    /// Key to sentence.
    pub message: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::{FieldType, Level, Reversibility};

    /// Every spelling this file hands to the generator is the spelling meant.
    ///
    /// **Both of these tables are one-line-per-variant `match`es whose arms are
    /// strings** — the shape where a typo is a valid program.
    /// `Irreversible => "Irreversble"` generates a variant nobody can name and a
    /// `§14.6` confirmation nobody can require; `Trace => "verbose"` publishes a
    /// level the log store does not read. Without this test both leave the
    /// workspace green, because everything downstream copies whatever this
    /// returns and compares it against itself.
    ///
    /// So the expectation is written out here rather than derived, and as an
    /// exhaustive `match` — a variant added to either enum is a compile error in
    /// this file rather than a row that quietly stops being checked.
    #[test]
    fn every_variant_is_spelled_as_the_thing_it_names() {
        for variant in [
            Reversibility::Read,
            Reversibility::Write,
            Reversibility::Irreversible,
        ] {
            let meant = match variant {
                Reversibility::Read => "Read",
                Reversibility::Write => "Write",
                Reversibility::Irreversible => "Irreversible",
            };
            assert_eq!(
                variant.variant(),
                meant,
                "`{variant:?}` generates a Rust variant spelled `{}`, which is not the \
                 name anything downstream can write",
                variant.variant()
            );
        }

        for level in [
            Level::Trace,
            Level::Debug,
            Level::Info,
            Level::Warn,
            Level::Error,
        ] {
            let meant = match level {
                Level::Trace => "trace",
                Level::Debug => "debug",
                Level::Info => "info",
                Level::Warn => "warn",
                Level::Error => "error",
            };
            assert_eq!(
                level.written(),
                meant,
                "`{level:?}` is written `{}`, and the registry's own spelling is what the \
                 log store reads",
                level.written()
            );
        }
    }

    /// Every field kind's label and Rust type are the ones meant, all ten.
    ///
    /// **These two string tables are guarded like the two above.**
    /// `FieldType::label` and `FieldType::rust` are the same
    /// one-line-per-variant shape the module above names as "where a typo is a
    /// valid program", and the registry declares neither `i64` nor `id` — so
    /// the `I64` and `Id` arms are dead, and the drift comparison that holds
    /// the rest cannot see them. Measured without this test, wrong strings on
    /// arms the registry does not reach:
    ///
    /// ```text
    /// cargo xtask check  ->  check passed, byte-identical
    /// ```
    ///
    /// The same corruption on a *used* kind gives
    /// `2 generated file(s) out of sync` and EXIT=1, which is all the drift
    /// comparison holds of these tables: a comparison between the emitter and
    /// its own output, true by construction for anything neither of them
    /// reaches.
    ///
    /// `label` is the field's doc comment in `nc-log`'s generated structs, and
    /// `FieldType::Id`'s is the sentence saying an identifier is in the log
    /// *because its contents are not* — which is `§15.4`'s argument, written
    /// where a reader of the generated crate meets it.
    ///
    /// Written out here and as an exhaustive `match`, so a variant added to
    /// `FieldType` is a compile error in this file rather than a row that
    /// quietly stops being checked.
    #[test]
    fn every_field_kind_is_spelled_as_the_thing_it_names() {
        let mut seen = 0_usize;
        for kind in [
            FieldType::U64,
            FieldType::I64,
            FieldType::U32,
            FieldType::Bool,
            FieldType::Duration,
            FieldType::ErrorCode,
            FieldType::Id,
            FieldType::Snowflake,
            FieldType::Literal,
            FieldType::Enum,
        ] {
            let (label, rust) = match kind {
                FieldType::U64 => ("count", "u64"),
                FieldType::I64 => ("signed number", "i64"),
                FieldType::U32 => ("whole number, at most 32 bits wide", "u32"),
                FieldType::Bool => ("flag", "bool"),
                FieldType::Duration => ("span of time", "core::time::Duration"),
                FieldType::ErrorCode => ("code from the error registry", "nc_catalog::ErrorCode"),
                FieldType::Id => (
                    "opaque identifier, written to the log exactly as it is",
                    "nc_ids::OpaqueId",
                ),
                FieldType::Snowflake => (
                    "Snowflake, written as its Crockford base32 text",
                    "nc_ids::Snowflake",
                ),
                FieldType::Literal => (
                    "fixed text chosen when the code was written, never at run time",
                    "crate::Literal",
                ),
                FieldType::Enum => (
                    "variant of an enum the platform declares — a state, never a name",
                    "crate::Variant",
                ),
            };
            assert_eq!(
                kind.label(),
                label,
                "`{kind:?}` becomes the doc comment `{}` on every field of that kind in \
                 the generated crate, and that is not what this kind holds",
                kind.label()
            );
            assert_eq!(
                kind.rust(),
                rust,
                "`{kind:?}` generates the Rust type `{}`, and every type here is one \
                 that implements `LogValue` — which is the whole wall (§15.4)",
                kind.rust()
            );
            seen += 1;
        }
        assert_eq!(
            seen, 10,
            "the registry declares 6 of these kinds and this file writes 10, so the four \
             the registry does not reach are exactly the ones nothing else reads"
        );
    }

    /// No two variants of either enum are written the same way.
    ///
    /// A table of strings collapses as easily as it mis-spells: two variants
    /// given one spelling is a generated surface that cannot tell them apart,
    /// and every assertion above still passes if the two written-out arms are
    /// changed together.
    #[test]
    fn no_two_variants_share_a_spelling() {
        let names: Vec<&str> = [
            Reversibility::Read,
            Reversibility::Write,
            Reversibility::Irreversible,
        ]
        .into_iter()
        .map(Reversibility::variant)
        .collect();
        let distinct: std::collections::BTreeSet<&&str> = names.iter().collect();
        assert_eq!(distinct.len(), names.len(), "{names:?}");

        let levels: Vec<&str> = [
            Level::Trace,
            Level::Debug,
            Level::Info,
            Level::Warn,
            Level::Error,
        ]
        .into_iter()
        .map(Level::written)
        .collect();
        let distinct: std::collections::BTreeSet<&&str> = levels.iter().collect();
        assert_eq!(distinct.len(), levels.len(), "{levels:?}");

        // And the two tables beside them, which are ten rows each: a label
        // shared by two kinds is a generated page that cannot tell them apart,
        // and a Rust type shared by two is a field that takes the wrong one.
        let kinds = [
            FieldType::U64,
            FieldType::I64,
            FieldType::U32,
            FieldType::Bool,
            FieldType::Duration,
            FieldType::ErrorCode,
            FieldType::Id,
            FieldType::Snowflake,
            FieldType::Literal,
            FieldType::Enum,
        ];
        for written in [
            kinds.into_iter().map(FieldType::label).collect::<Vec<_>>(),
            kinds.into_iter().map(FieldType::rust).collect::<Vec<_>>(),
        ] {
            let distinct: std::collections::BTreeSet<&&str> = written.iter().collect();
            assert_eq!(distinct.len(), written.len(), "{written:?}");
        }
    }
}
