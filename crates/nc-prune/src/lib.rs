//! One pruner, for every table.
//!
//! `NOCTUA.md §15.8`. **One pruner enforces all of them, on a schedule. Not a
//! bespoke cleanup per table** — a cleanup written per table is a cleanup that
//! is written for some of them.
//!
//! Three rules, and each is in the section for a reason:
//!
//! **Prune in large batches.** The worked example's shape is deliberate:
//! deleting twenty thousand rows once is cheap, and deleting one row twenty
//! thousand times is a load problem that arrives exactly when the system is
//! busiest.
//!
//! **At or below the limit, do nothing.** Not "delete a few anyway" — a pruner that
//! ran on every schedule tick would keep a table permanently at its ceiling and
//! make every measurement of headroom meaningless.
//!
//! **At exactly `max_rows`, nothing happens**, and the boundary is worth stating
//! because `NOCTUA.md`'s plan says "pruning at the limit removes exactly
//! `prune_batch` oldest rows". `max_rows` is read here as *how many rows the
//! table may hold* rather than as *the count at which pruning starts*, so the
//! first run is at `max_rows + 1`. Either reading is defensible; this one is the
//! one the code implements, and a table sitting at its stated maximum is not
//! over it.
//!
//! **The journal is the exception.** A table whose strategy is `policy-pack` is
//! bounded per realm by the pack, and this **must not touch it**: a global
//! constant would one day prune below what a pack promised, and audit retention
//! is a promise (`§14.9`).
//!
//! # Why the target is a trait
//!
//! The arithmetic — when to run, how many to take, what to report — is the part
//! that can be wrong, and it is testable without a database. What needs
//! PostgreSQL is one `DELETE`.
//!
//! **The trait is asynchronous because sqlx is.** A synchronous trait can only
//! be implemented over PostgreSQL by blocking a thread on a runtime, which
//! deadlocks when the caller is already inside one — so that shape guarantees
//! the half of this crate that runs against a real database can never be
//! written. [`postgres::Postgres`] is that half,
//! and `crates/nc-prune/tests/in_postgres.rs` runs it against the store —
//! asserting which rows survive rather than how many, because a pruner that
//! takes the newest satisfies every in-memory assertion there is.
//!
//! # Two databases, one pruner
//!
//! **A target prunes one database, and says which.** `catalog/limits.toml`
//! bounds tables in PostgreSQL and in the SQLite file on the user's machine
//! (`database = "local"`), and [`Target::database`] is how a run knows which of
//! them it was handed: [`run`] considers only the tables of that database, and a
//! table of the other one handed over by name is refused rather than asked
//! about. [`sqlite::Sqlite`] is the second target, over the pool
//! `crates/nc-local` opens; the kernel's `pump` and its `limits.prune` run it
//! for the local tables, so `§15.8`'s "one pruner enforces all of them" stays
//! one pruner.
//!
//! **A table that refuses at its limit is left alone, and says why.** Its
//! strategy is `refuse-at-limit` — user state, bounded by its writer refusing a
//! new row — and deleting the oldest of it would be losing somebody's data to
//! make room. It is reported with [`Untouched::ItsWriterRefusesAtTheLimit`].

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]
use core::future::Future;

use nc_catalog::{Strategy, TableLimit};

pub mod postgres;
pub mod sqlite;

/// Where rows are counted and removed.
pub trait Target {
    /// The database this target prunes.
    ///
    /// **Required, not defaulted**: a target that did not say would be read as
    /// PostgreSQL's, and a local table's name handed to the server — or the
    /// other way round — is a table that is not there.
    fn database(&self) -> nc_catalog::Database;

    /// How many rows the table holds.
    ///
    /// # Errors
    ///
    /// Returns why the table could not be counted.
    fn count(&mut self, table: &str) -> impl Future<Output = Result<u64, Failure>> + Send;

    /// Removes the `batch` oldest rows, ordered by `order_by`, and says how many
    /// went.
    ///
    /// # Errors
    ///
    /// Returns why the rows could not be removed.
    fn delete_oldest(
        &mut self,
        table: &str,
        order_by: &str,
        batch: u64,
    ) -> impl Future<Output = Result<u64, Failure>> + Send;
}

/// Why one table could not be counted or pruned, and whether to try again.
///
/// **`waited` is a fact the database stated, not a phrase in a sentence.**
/// Decided by `why.contains("gave up waiting for a lock")` — over a message
/// *the database* composes, which carries text the application wrote — the
/// skip that lets a locked table not stop the run reads an ordinary
/// `BEFORE DELETE` trigger raising
/// `'this table is archived: the exporter gave up waiting for a lock file'`
/// as contention:
///
/// ```text
/// outcome        Ok([Pruned { …, left_alone: Some(SomethingElseHeldALock) }])
/// still holding  9 rows, against a limit of 6
/// ```
///
/// A permanent failure reported as a benign skip that "the next tick will try
/// again" — and every tick after. The SQLSTATE is already in hand when the
/// sentence is built; it is carried instead of re-derived from prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// What went wrong, for a person.
    pub why: String,
    /// Whether the statement gave up waiting rather than being refused.
    ///
    /// **A SQLSTATE is chosen by whoever raised it, and that is the
    /// residual.** PL/pgSQL's `RAISE … USING ERRCODE = '55P03'` lets
    /// a `BEFORE DELETE` trigger — the exact scenario this field exists for —
    /// say "lock timeout" about a permanent refusal. Measured through the real
    /// target:
    ///
    /// ```text
    /// RAISE EXCEPTION 'this table is archived' USING ERRCODE = '55P03';
    ///   -> Ok([Pruned { …, left_alone: Some(SomethingElseHeldALock) }])
    ///      still holding 6 rows against a limit of 2
    /// ```
    ///
    /// Reading the code rather than the message raises the cost of forging it
    /// from "write a sentence" to "write a trigger that lies about its own error
    /// class", and does not remove it. What removes it is not available here: at
    /// the protocol level a forged `55P03` and a real one are the same answer.
    ///
    /// So a skip is a **hint, not a verdict**, and the scheduler that calls this
    /// on a timer — the daemon's, Stage 13, which calls `Kernel::pump` — has to
    /// escalate a table that is skipped every tick. A wait that never ends is a
    /// refusal wearing a wait's clothes, and the number of consecutive skips is
    /// the only thing that can tell them apart. `AGENTS.md` records it beside
    /// the pruner's other limits.
    ///
    /// `lock_timeout` (`55P03`), `statement_timeout` (`57014`) and a deadlock
    /// (`40P01`) are all of them: the work was not done, nothing says it cannot
    /// be, and the next tick is a better answer than ending this one. Every
    /// other failure ends the run, because a bad `order_by` is a defect in the
    /// catalog and no tick will fix it.
    pub waited: bool,
    /// Whether the database answered at all.
    ///
    /// **A store that is not there is not a table that refuses.** `waited` is
    /// read from a SQLSTATE and a transport failure carries none, so without
    /// this a pool timeout, a refused connection and a DNS failure all reach
    /// [`Failure::code`] as `waited: false` — a permanent refusal — and a
    /// scheduler keyed on `retryable` gives up for good on a database restart.
    /// Measured against a closed port: `NC-DB-0005`, `retryable = false`,
    /// *"trying again will not help"*.
    pub answered: bool,
}

impl Failure {
    /// The registry code this failure carries.
    ///
    /// **It carries one.** `NC-DB-0003` — *"Pruning a table failed"* — is
    /// reserved for exactly this, and answering with a `String` would leave it
    /// produced by nothing. `§15.1` makes an error's identity data: without a
    /// code the failure cannot be journalled, cannot be a `LogValue` and cannot
    /// be what the central log is queried by, which is the whole reason a code
    /// exists.
    ///
    /// **One code, and `waited` is what says whether to try again.** A wait
    /// carrying `DbUnreachable` is a sentence about something that did not
    /// happen: measured against a real `ACCESS EXCLUSIVE` lock, the database
    /// answers in 5.0006 s with `55P03`, and the failure says *"The database
    /// did not answer"* at severity **critical**. Both of those are wrong about a benign skip, and `§15.1`
    /// makes the code the identity a responder is paged by.
    ///
    /// Pruning a table failed. Whether it failed because something held a lock
    /// or because the table refuses to be pruned is a different question with a
    /// different field, and the field is [`Self::waited`].
    ///
    /// **And that field chooses the code, because it chooses what to do next.**
    /// With one code for both, and `NC-DB-0003` `retryable`, measured: three
    /// ticks against a table whose `order_by` column does not exist give three
    /// identical retryable failures, both locales promising "this will be tried
    /// again", and twenty rows still held against a limit of five. A lock this
    /// run did not get clears by itself; a statement that will not parse does
    /// not, and a scheduler keyed on `retryable` goes round that loop while the
    /// table grows.
    #[must_use]
    pub const fn code(&self) -> nc_catalog::ErrorCode {
        // **A database that did not answer is not a table that refused.** Read
        // from a SQLSTATE a transport failure does not carry, both would come
        // back `waited: false` — so a pool timeout would answer
        // `DbPruneRefused`, `retryable = false`, *"trying again will not
        // help"*, for a database restarting. `DbUnreachable` exists for exactly
        // this, and this is where it is reached.
        if !self.answered {
            nc_catalog::ErrorCode::DbUnreachable
        } else if self.waited {
            nc_catalog::ErrorCode::DbPruneFailed
        } else {
            nc_catalog::ErrorCode::DbPruneRefused
        }
    }
}

impl Failure {
    /// A failure nothing should retry.
    #[must_use]
    pub fn refused(why: impl Into<String>) -> Self {
        Self {
            why: why.into(),
            waited: false,
            // This crate's own refusals: the store is not what said no.
            answered: true,
        }
    }
}

impl core::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.why)
    }
}

/// A pruning failure is an error, as `NOCTUA.md §15.1` says every error type
/// here is.
///
/// A `Display` alone is not an error type; this is the `Error` beside
/// it.
impl core::error::Error for Failure {}

impl From<String> for Failure {
    fn from(why: String) -> Self {
        Self::refused(why)
    }
}

/// What one pruning run did to one table.
// Not `Copy`: `left_alone` carries the sentence that says why a table was
// refused, and a run that reports what it did has to carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pruned {
    /// Which table.
    pub table: &'static str,
    /// How many rows it held before, or `None` when nothing counted them.
    ///
    /// `None` is what a table bounded by the policy pack reports: this pruner
    /// deliberately does not touch it, so it never asked how large it was. A
    /// table something else held a lock on reports it too, for the same reason.
    ///
    /// What it held *when this run counted it*. Another pruner or another writer
    /// may have changed it between the count and the delete — see
    /// [`Pruned::remaining`].
    pub before: Option<u64>,
    /// How many were removed. Zero when the table was within its limit, and zero
    /// on a dry run.
    pub removed: u64,
    /// How many a real run would remove now.
    ///
    /// **Separate from `removed`, rather than `removed` meaning two things.** A
    /// dry run reports `removed: 0` and this, so a reader never has to know
    /// which kind of run produced the report to know what it says.
    pub would_remove: u64,
    /// Why it was left alone, or `None` when it was this pruner's to bound.
    pub left_alone: Option<Untouched>,
}

/// Why a table this pruner considered was not pruned by it.
// **Not `Copy`**, because one variant carries the sentence that says
// why a table was refused. A code and a reason are what `§15.1` asks a failure
// to carry, and a bare discriminant cannot carry the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Untouched {
    /// It was refused, and the run went on to the tables after it.
    ///
    /// **Carried rather than thrown away.** A refusal that made `run`
    /// return `Err` would discard every `Pruned` already collected, so a run
    /// that pruned one table and was refused on another would tell its caller
    /// about neither — while the rows are gone.
    ItWasRefused(String),
    /// A policy pack decides its retention (`NOCTUA.md §15.8`).
    ///
    /// The journal is the one that matters: audit retention is a promise the
    /// pack makes, and a global constant here would one day prune below it.
    APolicyPackBoundsIt,
    /// The statement gave up waiting rather than being refused.
    ///
    /// **A skip, and the run goes on.** The bounded wait in `postgres.rs` is
    /// right — an unbounded one is a tick that never completes — but returned
    /// through the loop's `?`, a lock on one table would stop every table after
    /// it. Measured that way: two tables both over their limit, an
    /// `ACCESS EXCLUSIVE` lock on the first, and the second still holding ten
    /// rows against a limit of four, never looked at.
    ///
    /// The next tick tries again, which is the whole reason a pruning run has
    /// nowhere to be.
    ///
    /// A lock timeout, a statement timeout and a deadlock are all of them, and
    /// which one is decided by the SQLSTATE the database returned — not by
    /// reading its message, which carries text the application wrote. See
    /// [`Failure::waited`].
    SomethingElseHeldALock,
    /// It is user state, bounded by its writer refusing a new row at its
    /// limit — `strategy = "refuse-at-limit"` — and nothing here deletes from
    /// it.
    ///
    /// **Reported, not skipped**, for the reason [`Self::APolicyPackBoundsIt`]
    /// is: a run that said nothing about a table cannot be told from one that
    /// did not look at it. Not a failure, so it carries no code.
    ItsWriterRefusesAtTheLimit,
}

impl Pruned {
    /// How many rows this run counted, less what this run removed.
    ///
    /// **Not how many the table holds**, which is not something this number
    /// can know. `count` and `delete_oldest` are two statements in two
    /// transactions, so nothing here is a snapshot: two pruners over 2,000 tied
    /// rows with a batch of 500 both report `before: 2000, removed: 500` and
    /// therefore 1,500 remaining, and the table holds 1,000.
    ///
    /// The bounded transaction does not change that: what it bounds is the
    /// wait for a lock, not the two statements into one view. Read this as what
    /// this run saw and did, which is what a pruning report is for — the next
    /// tick counts again.
    #[must_use]
    pub const fn remaining(&self) -> Option<u64> {
        match self.before {
            Some(before) => Some(before.saturating_sub(self.removed)),
            None => None,
        }
    }

    // **One block, not two.** Two inherent `impl` blocks for this type in one
    // file, with an ordinary function between them, would be kept apart by
    // nothing but habit — while a reader that addresses an item by its
    // declaring line, the meta-gate's among them, resolves `impl Pruned` to the
    // first and reports every clause in the second as one it cannot find.
    /// The registry code a `Pruned` carries, when it carries one.
    ///
    /// What went wrong with this table, if anything did.
    ///
    /// **Because otherwise nothing outside this crate could see one.** Every
    /// public entry point — `run`, `prune_these`, `rehearse`,
    /// `prune_these_declared_by` — consumes the `Failure` into a `String` or
    /// into `locked()`, so without this method and the code `refusal` writes
    /// into that line, `Failure::code()` would be called by this crate's own
    /// tests and by nothing else. A code a caller cannot reach is a code the log
    /// cannot be queried by, which is the whole reason `§15.1` asks for one.
    #[must_use]
    pub fn code(&self) -> Option<nc_catalog::ErrorCode> {
        match &self.left_alone {
            Some(Untouched::SomethingElseHeldALock) => Some(nc_catalog::ErrorCode::DbPruneFailed),
            // **And a refusal, which must not answer `None`.** This is the one
            // variant that carries a hard failure, and in the catch-all a
            // scheduler filtering on `code().is_some()` would see every lock
            // skip and miss every refusal. The code is read out of the sentence
            // the refusal already carries, because `refusal()` writes it there
            // and the two must not be able to disagree.
            Some(Untouched::ItWasRefused(why)) => Some(
                nc_catalog::ErrorCode::ALL
                    .iter()
                    .copied()
                    .find(|code| why.starts_with(&format!("[{}]", code.code())))
                    .unwrap_or(nc_catalog::ErrorCode::DbPruneRefused),
            ),
            // A table the policy pack bounds was not pruned on purpose, and a
            // table under its limit needed no pruning. Neither is a failure.
            _ => None,
        }
    }
}

/// Prunes every table the catalog bounds by row count.
///
/// Returns one entry per table it *considered*, including the ones it left
/// alone — a run that reported only what it deleted would be a run whose "did
/// nothing" is indistinguishable from "did not look".
///
/// # Errors
///
/// Returns the first table that could not be counted or pruned, named. Nothing
/// is rolled back: a table that was pruned was over its limit, and the rows it
/// lost were promised to nobody.
pub async fn run(target: &mut impl Target, request: Request<'_>) -> Result<Vec<Pruned>, String> {
    let database = target.database();
    let chosen: Vec<TableLimit> = match request.table {
        // **The target's own tables.** Every table of the other database is
        // one this target cannot reach, and asking it about them would report
        // each as a table that is not there.
        None => nc_catalog::TABLES
            .iter()
            .filter(|table| table.database == database)
            .copied()
            .collect(),
        Some(name) => {
            let found: Vec<TableLimit> = nc_catalog::TABLES
                .iter()
                .filter(|table| table.name == name)
                .copied()
                .collect();
            if found.is_empty() {
                // Named rather than silently pruning nothing: a caller that
                // misspelled a table would otherwise get an empty report and
                // read it as "nothing needed doing".
                return Err(format!(
                    "[{}] `{name}` is not a table catalog/limits.toml bounds, so there is no \
                     limit to enforce on it. `grep '^.tables.' catalog/limits.toml` names the \
                     ones there are — `ls` on that path prints the path",
                    nc_catalog::ErrorCode::DbTableNotBounded.code()
                ));
            }
            found
        }
    };
    prune_these_with(target, &chosen, nc_catalog::TABLES, request.dry_run).await
}

/// What one run of the pruner was asked to do.
///
/// **This is `limits.prune`'s declared argument list.** `catalog/commands.toml`
/// names `table` and `dry_run` in the command's `args_schema`, and a `run`
/// taking neither would leave the command a dispatcher will call and the
/// function that would answer it disagreeing, with nothing saying
/// so. `the_declared_command_is_the_one_this_crate_implements` is what keeps
/// them together.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Request<'a> {
    // **`#[non_exhaustive]` with no constructor makes this unbuildable from
    // outside**: `Request { table: Some(x), dry_run: true }` is
    // `error[E0639]: cannot create non-exhaustive struct using struct
    // expression`, so without one the only expression available to a caller
    // is `Request::default()` — every table, for real. The drift test lives
    // inside this crate and cannot see that, and the declared `limits.prune`
    // surface would not be reachable in the form a dispatcher would write.
    //
    // `every` and `just` below are that form.
    /// One bounded table by name, or every one of them.
    pub table: Option<&'a str>,
    /// Count and report what a real run would remove, and remove nothing.
    ///
    /// The row counts still come from the target, so a dry run is a real
    /// question asked of the real database — what it does not do is delete.
    pub dry_run: bool,
}

impl<'a> Request<'a> {
    /// Every bounded table, for real.
    #[must_use]
    pub const fn every() -> Self {
        Self {
            table: None,
            dry_run: false,
        }
    }

    /// One bounded table by name.
    #[must_use]
    pub const fn just(table: &'a str) -> Self {
        Self {
            table: Some(table),
            dry_run: false,
        }
    }

    /// The same request, counting and reporting rather than deleting.
    #[must_use]
    pub const fn dry(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

/// A table something other than this pruner bounds, reported rather than
/// passed over: a policy pack, or its own writer refusing at the limit.
///
/// Skipped entirely rather than pruned with a default, which is what `§15.8`
/// means by "never a global constant".
///
/// **Reported, not skipped silently.** A `continue` before pushing anything
/// would make a journal considered-and-left-alone indistinguishable from a
/// journal this pruner never looked at — the exact thing `run`'s own doc says a
/// run must not do.
fn bounded_elsewhere(table: &'static str, why: Untouched) -> Pruned {
    Pruned {
        table,
        before: None,
        removed: 0,
        would_remove: 0,
        left_alone: Some(why),
    }
}

/// A table something else was holding, reported rather than ended on.
fn locked(table: &'static str) -> Pruned {
    Pruned {
        table,
        before: None,
        removed: 0,
        would_remove: 0,
        left_alone: Some(Untouched::SomethingElseHeldALock),
    }
}

/// One refusal line, carrying the code a responder queries the log by.
///
/// **The code reaches the caller through the line.** Every public entry point
/// consumes the `Failure` into a `String`, so without the code written here
/// `Failure::code()` would be called by this crate's own tests and by nothing
/// else. `§15.1` asks for a code so a failure can be found again; a code that
/// stops at the crate boundary is not one.
fn refusal(why: &Failure, table: &str, what: &str) -> String {
    format!(
        "[{}] `{table}` could not be {what}: {why}",
        why.code().code()
    )
}

/// Why a table of one database was refused by a target of the other.
fn in_the_other_database(table: &TableLimit, target: nc_catalog::Database) -> String {
    let named = |database| match database {
        nc_catalog::Database::Server => "server's PostgreSQL",
        nc_catalog::Database::LocalFile => "local SQLite file",
    };
    format!(
        "[{}] `{}` is a table of the {} and this target prunes the {}, where it is not. \
         Hand it to a target of its own database: `nc_prune::postgres` for the server's \
         tables, `nc_prune::sqlite` for the local file's",
        nc_catalog::ErrorCode::DbTableNotBounded.code(),
        table.name,
        named(table.database),
        named(target)
    )
}

/// A table the catalog declares is ordered the way the catalog says.
///
/// **`Target` is public and `the_catalog_disagrees` lives one layer up.** So
/// the method that issues the `DELETE` asks the catalog itself, through this.
/// Without that, measured, `delete_oldest("access_records", "occurred_at", 3)`
/// removes the three **newest** rows, which is the exact inversion
/// `migrations/0006_access_records_orders_by_insertion.sql` exists to prevent,
/// and `delete_oldest("journal_entries", "sequence", 6)` truncates an audit log
/// carrying `0001`'s shape without the trigger migrations — a restored dump, or
/// a second deployment mid-migration. `lib.rs` says of the journal that "the
/// database refuses the `DELETE` as well, and this crate does not rely on
/// that"; through this door it would.
///
/// # Errors
///
/// Returns [`crate::Failure`] when the catalog declares the table and orders it
/// by a different column.
pub(crate) fn the_catalog_orders_its_own_tables(
    table: &str,
    order_by: &str,
    database: nc_catalog::Database,
) -> Result<(), crate::Failure> {
    let bare = table.rsplit('.').next().unwrap_or(table);
    let Some(declared) = nc_catalog::TABLES
        .iter()
        .find(|known| known.name == table || known.name == bare)
    else {
        return Ok(());
    };
    // **The database first**: a table of the other one is not there, and a
    // name both databases happen to hold would be the wrong table's rows.
    if declared.database != database {
        return Err(crate::Failure::refused(in_the_other_database(
            declared, database,
        )));
    }
    // **Not a row count, whichever of the two it is.** A table the policy pack
    // bounds has no number here, and one whose writer refuses at its limit is
    // user state nothing deletes from.
    if declared.strategy != nc_catalog::Strategy::OldestFirst {
        return Err(crate::Failure::refused(format!(
            "[{}] `{table}` is bounded by {} and not by a row count the pruner \
             applies (NOCTUA.md §15.8), so there is no oldest to delete",
            nc_catalog::ErrorCode::DbTableNotBounded.code(),
            match declared.strategy {
                nc_catalog::Strategy::RefuseAtLimit => "its writer refusing at the limit",
                _ => "the policy pack",
            }
        )));
    }
    if declared.order_by != order_by {
        return Err(crate::Failure::refused(format!(
            "[{}] `{table}` is ordered by `{}` in catalog/limits.toml and this asks for \
             `{order_by}`. Which column says *oldest* is the whole of what makes a \
             row-count bound safe: measured, ordering the access log by a column a \
             device's clock writes removed the three newest rows",
            nc_catalog::ErrorCode::DbTableNotBounded.code(),
            declared.order_by
        )));
    }
    Ok(())
}

/// The command journal's table, which no row count may prune.
///
/// `NOCTUA.md §15.8` carves it out by name: its retention is a per-realm
/// policy-pack property, so there is no number that could be right.
const JOURNAL: &str = "journal_entries";

/// The same, over a given set of limits. Split out so the tests can drive it
/// with limits of their own rather than only with the catalog's.
///
/// # Errors
///
/// As [`run`].
pub async fn prune_these(
    target: &mut impl Target,
    tables: &[TableLimit],
) -> Result<Vec<Pruned>, String> {
    prune_these_declared_by(target, tables, nc_catalog::TABLES, false).await
}

/// The same as [`prune_these`], counting and reporting rather than deleting.
///
/// Split out so a test outside this crate can drive a rehearsal over limits of
/// its own — the shape `Request::just(…).dry()` produces for one catalog table,
/// generalised the way `prune_these` generalises [`run`].
///
/// # Errors
///
/// As [`run`]. A rehearsal fails where a real run would: it issues the same
/// statement with nothing to take.
pub async fn rehearse(
    target: &mut impl Target,
    tables: &[TableLimit],
) -> Result<Vec<Pruned>, String> {
    prune_these_declared_by(target, tables, nc_catalog::TABLES, true).await
}

/// The same, judged against a set of limits other than the catalog's.
///
/// **The seam, and it is named so that using it is visible.** `prune_these`
/// and `rehearse` judge every table against `nc_catalog::TABLES`: a name the
/// catalog does not declare is refused, and a name it does declare must be
/// handed over exactly as the catalog wrote it. That is what "the catalog
/// decides what this crate may touch" has to mean, and one field of one
/// comparison is not it — a caller that matched `strategy` could put back
/// any `order_by`, `max_rows` and `prune_batch` it liked, and `access_records`
/// is emptied through such a guard by ordering it by `id`, a column the
/// catalog does not order it by, measured.
///
/// The tests in `crates/nc-prune/tests/in_postgres.rs` prune tables they create themselves, and
/// they are the reason this exists. It is `#[doc(hidden)]` and it says in its
/// name what it does, so a caller who reaches for it has decided to.
///
/// The journal is refused whatever the authority says — `the_catalog_disagrees`
/// is where the carve-out lives.
///
/// **A link naming a function that does not exist goes unseen here**:
/// `RUSTDOCFLAGS=-D warnings` denies `broken_intra_doc_links`, but this item is
/// `#[doc(hidden)]` and rustdoc does not document a hidden item unless asked —
/// so it is outside the only gate that reads documentation, along with every
/// other hidden test door in this workspace. Measured: the same broken link on a
/// public function is `error: unresolved link`, and on a hidden one is silence.
///
/// # Errors
///
/// The same as `run`.
#[doc(hidden)]
pub async fn prune_these_declared_by(
    target: &mut impl Target,
    tables: &[TableLimit],
    authority: &[TableLimit],
    dry_run: bool,
) -> Result<Vec<Pruned>, String> {
    prune_these_with(target, tables, authority, dry_run).await
}

/// Why this table may not be pruned as asked, if it may not.
///
/// Four refusals, all of them about the *request* rather than the database: a
/// name the catalog does not bound, the journal's `§15.8` carve-out, an entry
/// that disagrees with what the catalog declares, and an oldest-first entry with
/// no limit. Split out because inside the loop that asks them they would cross
/// a hundred lines.
fn the_catalog_disagrees(table: &TableLimit, authority: &[TableLimit]) -> Option<String> {
    let mut refused = Vec::new();
    let bare = table.name.rsplit('.').next().unwrap_or(table.name);
    // **The journal, whatever anyone's limits say.** The authority is a
    // parameter so the tests can prune tables they created; the one table
    // that is never prunable by row count is not something an authority may
    // grant, because §15.8 carves it out by name and the crate's header
    // claims that carve-out as its own.
    if bare == JOURNAL && table.strategy == Strategy::OldestFirst {
        refused.push(format!(
            "[{}] `{}` is the command journal, and NOCTUA.md §15.8 carves it out of row-count \
             pruning by name: its retention is a per-realm policy-pack property, so \
             there is no number here that could be right. The database refuses the \
             DELETE as well, and this crate does not rely on that",
            nc_catalog::ErrorCode::DbTableNotBounded.code(),
            table.name
        ));
        return refused.pop();
    }
    // **And the authority for a catalog table is the catalog, not the caller.**
    // Compared only against itself, passing one forged entry as both sides
    // makes the comparison vacuous. Measured through the shipped `pub` seam
    // that way: `access_records` with `order_by = "id"`, `max_rows = 0` and a
    // batch of 500 000 handed in as its own authority emptied all 20 rows,
    // newest-first by a column the access-log migration documents as *not*
    // creation order, against a catalog limit of 500 000, reported as an
    // ordinary success.
    // A caller's own limits may describe a table the catalog does not know; for
    // one it does, the catalog is the authority whatever was handed over.
    // **By the bare name, like the journal's carve-out.** Matched on the whole
    // string, a schema-qualified alias — `<schema>.access_records`, a legal
    // name for the same table — is not "a table the catalog declares", so the
    // caller stays its own authority: measured through the seam, 20 of 20 rows
    // of the access log gone, newest-first, reported as an ordinary success.
    // The journal's carve-out compares `bare` for the same reason.
    let authority: &[TableLimit] = if nc_catalog::TABLES
        .iter()
        .any(|known| known.name == table.name || known.name == bare)
    {
        nc_catalog::TABLES
    } else {
        authority
    };
    if let Some(declared) = authority
        .iter()
        .find(|known| known.name == table.name || known.name == bare)
    {
        if declared != table {
            refused.push(format!(
                "[{}] `{}` was handed to this crate as {table:?}, and catalog/limits.toml \
                 declares it as {declared:?}. What a table's retention is belongs to \
                 the catalog (NOCTUA.md §15.8), and a caller that could change it \
                 here could ask the pruner to empty the journal",
                nc_catalog::ErrorCode::DbTableNotBounded.code(),
                table.name
            ));
            return refused.pop();
        }
    } else {
        refused.push(format!(
            "[{}] `{}` is not a table catalog/limits.toml declares{}. The pruner is driven \
             by the catalog and never by a bespoke list (NOCTUA.md §15.8): declare the \
             table there, or use `prune_these_declared_by` and say which limits you \
             mean",
            nc_catalog::ErrorCode::DbTableNotBounded.code(),
            table.name,
            if bare == table.name || !authority.iter().any(|known| known.name == bare) {
                String::new()
            } else {
                format!(", and `{bare}` is a different name for a table that is")
            }
        ));
        return refused.pop();
    }

    refused.pop()
}

/// The same, saying whether to actually delete.
///
/// # Errors
///
/// As [`run`].
async fn prune_these_with(
    target: &mut impl Target,
    tables: &[TableLimit],
    authority: &[TableLimit],
    dry_run: bool,
) -> Result<Vec<Pruned>, String> {
    let mut done = Vec::new();
    // **Every table that refused for a reason no later tick will get past —
    // carried, not returned.** Returning on the first such table, one
    // `RAISE EXCEPTION 'no'` in a `BEFORE DELETE` trigger on the alphabetically
    // first one stops pruning for every table after it. Measured: two tables at
    // six rows each against a limit of two, one trigger, and the second never
    // looked at — unbounded growth, in the crate whose subject is that records
    // do not grow without bound, from four words of PL/pgSQL.
    //
    // A defect is still a defect and the run still fails: at the end, naming
    // every table it happened to, after the tables that could be pruned have
    // been.
    let mut refused: Vec<(&'static str, String)> = Vec::new();
    for table in tables {
        // **The catalog decides what this crate may touch, not the caller.**
        // `prune_these` takes a `&[TableLimit]` and every field of `TableLimit`
        // is `pub`, so a consumer can hand this a `TableLimit { name:
        // "journal_entries", strategy: OldestFirst, max_rows: Some(0), … }`.
        // Judged by the caller's limits alone, that would ask the crate whose
        // header says of the journal *"this must not touch it"* to delete all
        // of it. The tree's own tests build `TableLimit` that way, so the shape
        // is reachable from outside.
        //
        // What else stops it is one migration's trigger — measured, and it does
        // hold:
        // `ERROR: the journal is append-only: DELETE on journal_entries was refused`.
        // But this crate's header claims `§15.8`'s carve-out as its own, and a
        // guarantee a crate claims is one it holds itself rather than one that
        // lives only in the database.
        //
        // **Three ways past a guard that compares one field, all measured.**
        // Comparing only `strategy` of the caller's entry against the
        // catalog's, and only when the names match exactly, lets these through:
        //
        // ```text
        // public.journal_entries  ->  removed 1000000   (a legal name for the
        //                             same table, and not the string the
        //                             catalog holds)
        // _sqlx_migrations        ->  removed 500       (no catalog entry, so
        //                             nothing to disagree with)
        // access_records, order_by = "id"
        //                         ->  removed 400000    (strategy matched, and
        //                             every other field was the caller's)
        // ```
        //
        // So the whole entry is compared, an unqualified name that resolves to a
        // declared table is not a different table, and a name the authority does
        // not declare is refused rather than trusted.
        if let Some(why) = the_catalog_disagrees(table, authority) {
            refused.push((table.name, why));
            continue;
        }
        // **And the target's own database.** A table of the other one is not
        // there to count, and a name that happens to exist in both would be
        // the wrong table's rows.
        if table.database != target.database() {
            refused.push((table.name, in_the_other_database(table, target.database())));
            continue;
        }
        match table.strategy {
            Strategy::OldestFirst => {}
            Strategy::PolicyPack => {
                done.push(bounded_elsewhere(
                    table.name,
                    Untouched::APolicyPackBoundsIt,
                ));
                continue;
            }
            Strategy::RefuseAtLimit => {
                done.push(bounded_elsewhere(
                    table.name,
                    Untouched::ItsWriterRefusesAtTheLimit,
                ));
                continue;
            }
        }
        let (Some(max_rows), Some(batch)) = (table.max_rows, table.prune_batch) else {
            // The catalog's own gate refuses this shape, so reaching it means
            // something bypassed the compiler. Saying so is better than pruning
            // with a number nobody wrote.
            refused.push((
                table.name,
                format!(
                    "[{}] `{}` prunes oldest-first and names no limit, which catalog/limits.toml \
                 cannot express — something wrote this table's entry without the compiler",
                    nc_catalog::ErrorCode::DbTableNotBounded.code(),
                    table.name
                ),
            ));
            continue;
        };
        let before = match target.count(table.name).await {
            Ok(before) => before,
            Err(why) if why.waited => {
                done.push(locked(table.name));
                continue;
            }
            Err(why) => {
                refused.push((table.name, refusal(&why, table.name, "counted")));
                continue;
            }
        };
        if before <= max_rows {
            done.push(Pruned {
                table: table.name,
                before: Some(before),
                removed: 0,
                would_remove: 0,
                left_alone: None,
            });
            continue;
        }
        // Exactly one batch, even when the table is far over. A run that looped
        // until the table was under its limit would hold a lock for as long as
        // the backlog took, and the schedule is what catches up.
        // What a real run takes: one whole batch, or every row if the table
        // holds fewer than a batch. **Not the excess** — the `DELETE` removes a
        // batch whether or not that takes the table below its limit, and a
        // prediction that said otherwise would be a dry run answering a
        // different question from the one a real run answers.
        let would_remove = batch.min(before);
        // **A dry run issues the real statement with nothing to take.** It used
        // to return before `delete_oldest`, which is the only place the
        // identifier guard and the `ORDER BY` column are exercised — so it
        // predicted `would_remove: 50000` for a run that then failed with
        // `column "id" does not exist`. A rehearsal that cannot fail where the
        // real thing fails is not a rehearsal.
        //
        // `LIMIT 0` is the whole difference. Verified on PostgreSQL 18.6: the
        // statement plans, resolves every identifier, removes no row, and still
        // answers `column "nope" does not exist` for a column that is not there.
        let removed = match target
            .delete_oldest(table.name, table.order_by, if dry_run { 0 } else { batch })
            .await
        {
            Ok(removed) => removed,
            Err(why) if why.waited => {
                done.push(locked(table.name));
                continue;
            }
            Err(why) => {
                refused.push((table.name, refusal(&why, table.name, "pruned")));
                continue;
            }
        };
        done.push(Pruned {
            table: table.name,
            before: Some(before),
            removed,
            would_remove,
            left_alone: None,
        });
    }
    verdict(done, &refused)
}

/// What a run answers when some tables refused and others were pruned.
///
/// Its own function because inside `prune_these_with` it would cross a hundred
/// lines.
fn verdict(done: Vec<Pruned>, refused: &[(&'static str, String)]) -> Result<Vec<Pruned>, String> {
    if refused.is_empty() {
        return Ok(done);
    }
    // **What was done is not discarded because something else failed.**
    // Returning `Err` and throwing the `Pruned` list away, a run that pruned
    // one table and was refused on another would report *no* table — measured,
    // four rows gone from `r24c_b` and a caller that could not know it, against
    // a doc that promises one entry per table considered. A scheduler cannot
    // journal what it is not told.
    //
    // So a refusal is an error only when nothing was done. When something was,
    // the refusals ride along as their own rows: `left_alone` is where a table
    // says why it was not pruned, and this is one more reason.
    // **"Nothing was done" is not "removed nothing".** `removed` is zero on
    // every dry run by construction — the batch is `LIMIT 0` — so judged that
    // way a rehearsal with one refusal throws away every prediction it has
    // made, and a table something else held a lock on, the one `Pruned` a
    // scheduler escalates on, goes with it. What distinguishes a run that did
    // nothing is that it has no rows to report at all.
    //
    // **And a row nothing asked the store for is not a row that was done.** A
    // table the pack bounds is pushed without the database being touched, so
    // `done` is never empty for the whole-catalog request — measured with the
    // store unreachable and `done` alone judged: `run(Request::every())`
    // answered **`Ok`** with two rows, one of them the journal's carve-out and
    // the other the refusal, while `run(Request::just("access_records"))`
    // answered `Err` for the same fault. One outage, two result shapes decided
    // by the shape of the request, and a scheduler checking `is_err()` never
    // learns the store is gone while the one table `§15.8` exists to bound
    // grows.
    if done.iter().all(|pruned| {
        matches!(
            pruned.left_alone,
            Some(Untouched::APolicyPackBoundsIt | Untouched::ItsWriterRefusesAtTheLimit)
        )
    }) {
        return Err(refused
            .iter()
            .map(|(_, why)| why.as_str())
            .collect::<Vec<_>>()
            .join("; "));
    }
    let mut done = done;
    // **And the row says which table.** `table: ""` sends a scheduler the one
    // row it has to act on with nothing to act on it by — measured,
    // `Pruned { table: "", removed: 0, left_alone: Some(ItWasRefused(…)) }`
    // beside a row that had deleted 50 000. The name is carried alongside the
    // sentence rather than read back out of it, so the two cannot disagree.
    for (table, why) in refused {
        done.push(Pruned {
            table,
            before: None,
            removed: 0,
            would_remove: 0,
            left_alone: Some(Untouched::ItWasRefused(why.clone())),
        });
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    /// A pruning failure is an error, and says what its sentence says.
    #[test]
    fn a_pruning_failure_is_an_error() {
        let failure = super::Failure::refused("the catalog does not bound this table".to_owned());
        let boxed: Box<dyn core::error::Error> = Box::new(failure);
        assert_eq!(boxed.to_string(), "the catalog does not bound this table");
    }

    /// The code on a refusal is the one its own sentence carries.
    ///
    /// **A refusal whose code is the fallback decides neither
    /// the `ErrorCode::ALL` lookup nor
    /// the `unwrap_or`**: `in_postgres.rs`'s `a_skip_and_a_refusal_each_carry_the_code_a_scheduler_reads`
    /// asserts only `!code.retryable()`, and its refusal is a view, which
    /// is `NC-DB-0005`. Measured without this: the whole arm replaced
    /// by `Some(ErrorCode::DbPruneRefused)` leaves `check passed`, and
    /// a `the catalog does not bound this table` refusal — `NC-DB-0004`, *the
    /// request names something that is not there* — reaches a scheduler as *old
    /// records could not be cleared, and trying again will not help*. That is
    /// one fault under two identities, which is the failure `§15.1` exists to
    /// prevent. Replacing the fallback with the **retryable** `DbPruneFailed`
    /// stays green too, so without this the arm is decided by nothing in either
    /// direction.
    #[test]
    fn a_refusal_carries_the_code_its_own_sentence_names() {
        let refused = |why: &str| super::Pruned {
            table: "access_records",
            before: None,
            removed: 0,
            would_remove: 0,
            left_alone: Some(super::Untouched::ItWasRefused(why.to_owned())),
        };

        // The code the sentence names, which is not the fallback.
        let unbounded = nc_catalog::ErrorCode::DbTableNotBounded;
        let named = refused(&format!(
            "[{}] `nowhere` is not a table catalog/limits.toml bounds",
            unbounded.code()
        ));
        assert_eq!(
            named.code(),
            Some(unbounded),
            "a refusal was reported under a code its own sentence does not name, so one              fault reaches a log under two identities"
        );

        // A sentence naming no code at all falls back — and the fallback is the
        // one that says *this will not work next time*, not the retryable one.
        let bare = refused("something went wrong and nobody wrote a code");
        assert_eq!(bare.code(), Some(nc_catalog::ErrorCode::DbPruneRefused));
        assert!(
            !nc_catalog::ErrorCode::DbPruneRefused.retryable(),
            "a refusal is not something a scheduler should try again"
        );

        // And the code has to open the sentence: a code named halfway through
        // one is a mention, not this refusal's identity.
        let mentioned = refused(&format!(
            "pruning failed; it was not [{}] this time",
            unbounded.code()
        ));
        assert_eq!(
            mentioned.code(),
            Some(nc_catalog::ErrorCode::DbPruneRefused)
        );
    }

    /// Every refusal a caller can receive names a code.
    ///
    /// **Without a code, five of nine reach a caller as bare text.** Measured against a real
    /// database: an unknown table, the journal carve-out, a table the catalog
    /// does not declare, an entry that disagrees with the catalog, and an
    /// oldest-first entry with no limit — all `Err(String)` with nothing in
    /// them a log could be queried by, and nothing telling a scheduler these are
    /// not the lock it should wait on. They are `NC-DB-0004`, which says the
    /// request is wrong rather than that pruning failed.
    #[tokio::test]
    async fn every_refusal_a_caller_receives_names_a_code() {
        let unbounded = nc_catalog::ErrorCode::DbTableNotBounded.code();
        // A `Target` that would answer if it were ever asked: these refusals all
        // happen before any statement runs, which is the point of them.
        let mut nothing = Rows::default();
        let text = super::run(&mut nothing, super::Request::just("no_such_table"))
            .await
            .expect_err("a table the catalog does not bound is a refusal");
        assert!(text.contains(unbounded), "it does not name a code: {text}");

        // The journal by name, handed straight to `prune_these` — `run` answers
        // this one with `left_alone`, correctly, because the catalog's own entry
        // is what carves it out there.
        let journal = TableLimit {
            name: "journal_entries",
            database: Database::Server,
            strategy: nc_catalog::Strategy::OldestFirst,
            order_by: "sequence",
            max_rows: Some(10),
            prune_batch: Some(1),
        };
        let text = super::prune_these(&mut nothing, &[journal])
            .await
            .expect_err("the journal is not prunable by a row count");
        assert!(text.contains(unbounded), "it does not name a code: {text}");

        // And an entry that disagrees with what the catalog declares.
        let forged = TableLimit {
            max_rows: Some(1),
            ..nc_catalog::TABLES[0]
        };
        let text = super::prune_these(&mut nothing, &[forged])
            .await
            .expect_err("a forged entry is a refusal");
        assert!(text.contains(unbounded), "it does not name a code: {text}");
    }

    /// And the code reaches a caller, which is the only reason it exists.
    ///
    /// Every public entry point consumes the `Failure` into a `String` or into
    /// `locked()`, so without the code in that `String`, `Failure::code()`
    /// would be called by the test above and by nothing else on the planet. A
    /// code that stops at the crate boundary cannot be journalled, cannot be a
    /// `LogValue` and cannot be what the central log is queried by — which is
    /// the whole of `§15.1`'s ask.
    #[test]
    fn the_code_reaches_whoever_reads_the_result() {
        let refused = super::Failure {
            why: "this table is archived".to_owned(),
            waited: false,
            answered: true,
        };
        let line = super::refusal(&refused, "access_records", "pruned");
        assert!(
            line.contains("access_records") && line.contains("archived"),
            "{line}"
        );

        // **The code the reader gets back, rather than a `contains`.**
        // `Pruned::code()` reads the code off the *front* of this sentence, and
        // asking only that it appear somewhere in it lets a prefix as small as
        // `prune: ` leave the writer and the reader disagreeing with the whole
        // workspace green. Measured: with one added, a store that does not
        // answer is written `NC-DB-0001` by `refusal` and read back as
        // `NC-DB-0005` by `code`, whose sentence says trying again will not
        // help. That inverts `retryable` for a database that is merely
        // restarting, which is the one distinction `Failure::answered` exists
        // to draw.
        //
        // Both failures, because the pair is what the inversion is between.
        for why in [
            refused.clone(),
            super::Failure {
                why: "the pool timed out".to_owned(),
                waited: false,
                answered: false,
            },
        ] {
            let line = super::refusal(&why, "access_records", "pruned");
            assert_eq!(
                super::Pruned {
                    table: "access_records",
                    before: None,
                    removed: 0,
                    would_remove: 0,
                    left_alone: Some(super::Untouched::ItWasRefused(line.clone())),
                }
                .code(),
                Some(why.code()),
                "the reader did not get back the code the writer put in: {line}"
            );
        }

        // And a skip carries it through the other channel, the one that reports
        // `Ok`: a table left alone because something held a lock is a table that
        // was not pruned, and the caller has a code to say so with.
        assert_eq!(
            super::locked("access_records").code(),
            Some(nc_catalog::ErrorCode::DbPruneFailed)
        );
        // A table the pack bounds was left alone on purpose, and is not a
        // failure — the floor for the line above, which a rule answering
        // "failed" to everything would satisfy.
        assert_eq!(
            super::Pruned {
                table: "journal_entries",
                before: None,
                removed: 0,
                would_remove: 0,
                left_alone: Some(super::Untouched::APolicyPackBoundsIt),
            }
            .code(),
            None
        );
    }

    /// A failure says which failure it is, in the vocabulary a log is queried by.
    ///
    /// **`NC-DB-0003` is reserved for this crate**, the one whose whole job is
    /// pruning tables. Answering with a `String` alone, it would leave a
    /// reserved code produced by nothing, with its producer sitting right here.
    #[test]
    fn a_failure_carries_the_code_the_registry_reserved_for_it() {
        let refused = super::Failure {
            why: "the table is archived".to_owned(),
            waited: false,
            answered: true,
        };
        assert_eq!(refused.code(), nc_catalog::ErrorCode::DbPruneRefused);
        assert!(
            !refused.code().retryable(),
            "a table that refuses to be pruned refuses again on the next tick"
        );
        // **And a wait is a different code**, because what a caller does with
        // it is different. As `DbPruneFailed` for both, which is `retryable`:
        // measured, three ticks against a table whose `order_by` column does
        // not exist give three identical retryable failures, both locales
        // promising "this will be tried again", and twenty rows still held
        // against a limit of five. As `DbUnreachable` — *"The database did not
        // answer"*, severity critical — a lock timeout the database answered
        // with, in five seconds, sends a responder looking for a store that is
        // down.
        let waited = super::Failure {
            why: "canceling statement due to lock timeout".to_owned(),
            waited: true,
            answered: true,
        };
        assert_eq!(waited.code(), nc_catalog::ErrorCode::DbPruneFailed);
        assert!(
            waited.code().retryable(),
            "a lock this run did not get is exactly what a later run may"
        );
        assert_ne!(
            waited.code(),
            nc_catalog::ErrorCode::DbUnreachable,
            "a lock the database told us about is not the database being unreachable"
        );
        // What tells them apart is the field written for it, and the code is
        // chosen by that field so the two cannot disagree.
        assert!(waited.waited && !refused.waited);

        // **And the store that did not answer at all**, held here: the arm's
        // own paragraph records a pool timeout answering `DbPruneRefused`,
        // *retrying will not help*, for a database that is restarting — and
        // `answered` exists to draw exactly that line. Measured without this,
        // the condition switched off: `cargo test -p nc-prune --lib` green,
        // because the one other test that reads this code compares the reader
        // against the writer and both go through this function. An expectation
        // derived from the thing it tests holds for any answer at all.
        let silent = super::Failure {
            why: "the pool timed out".to_owned(),
            waited: false,
            answered: false,
        };
        assert_eq!(
            silent.code(),
            nc_catalog::ErrorCode::DbUnreachable,
            "a store that never answered is reported as a table that refused"
        );
        assert!(
            silent.code().retryable(),
            "a database that did not answer is one a later run may reach"
        );
    }

    use super::{Pruned, Request, Target, prune_these, run};
    use nc_catalog::{Database, Strategy, TableLimit};
    use std::collections::BTreeMap;

    /// A target that holds row counts and nothing else.
    #[derive(Debug, Default)]
    struct Rows {
        counts: BTreeMap<String, u64>,
        deletions: Vec<(String, String, u64)>,
        /// What `delete_oldest` should fail with, when the test needs it to.
        refuse: Option<String>,
        /// One table that refuses permanently, so the run's shape can be seen.
        refuse_first: Option<String>,
    }

    impl Rows {
        fn with(table: &str, count: u64) -> Self {
            let mut rows = Self::default();
            rows.counts.insert(table.to_owned(), count);
            rows
        }

        /// The whole of `delete_oldest`, with nothing to await.
        fn delete_oldest_now(
            &mut self,
            table: &str,
            order_by: &str,
            batch: u64,
        ) -> Result<u64, super::Failure> {
            if let Some(why) = &self.refuse {
                return Err(super::Failure::refused(why.clone()));
            }
            self.deletions
                .push((table.to_owned(), order_by.to_owned(), batch));
            let held = self.counts.entry(table.to_owned()).or_default();
            let removed = batch.min(*held);
            *held -= removed;
            Ok(removed)
        }
    }

    // Neither method awaits: both answer from memory. The trait says
    // `-> impl Future + Send`, not `async fn`, so they say what they are.
    impl Target for Rows {
        fn database(&self) -> Database {
            Database::Server
        }
        fn count(
            &mut self,
            table: &str,
        ) -> impl Future<Output = Result<u64, super::Failure>> + Send {
            core::future::ready(Ok(self.counts.get(table).copied().unwrap_or(0)))
        }
        fn delete_oldest(
            &mut self,
            table: &str,
            order_by: &str,
            batch: u64,
        ) -> impl Future<Output = Result<u64, super::Failure>> + Send {
            core::future::ready(self.delete_oldest_now(table, order_by, batch))
        }
    }

    /// A table the real catalog does not declare.
    ///
    /// **Not `access_records`, because that would make these tests the
    /// attack.** `the_catalog_disagrees` comparing a caller's entry against the
    /// authority the caller handed over is vacuous for one forged entry passed
    /// as both sides — measured through the shipped `pub`
    /// seam, `access_records` emptied newest-first by a column the catalog had
    /// taken away. The comparison is against `nc_catalog::TABLES` for any name
    /// the catalog declares, so a test that describes its own limits has to
    /// describe a table of its own. That is what the seam is
    /// for: `prune_these_declared_by` exists so a test can prune a table it
    /// created.
    const LIMIT: TableLimit = TableLimit {
        name: "rows_a_test_created",
        database: Database::Server,
        strategy: Strategy::OldestFirst,
        order_by: "id",
        max_rows: Some(100),
        prune_batch: Some(10),
    };

    const PACKED: TableLimit = TableLimit {
        name: "journal_entries",
        database: Database::Server,
        strategy: Strategy::PolicyPack,
        order_by: "sequence",
        max_rows: None,
        prune_batch: None,
    };

    /// A table whose writer refuses at its limit is left alone, and said to be.
    ///
    /// **User state is never pruned**: `store_entries` is what plugins keep,
    /// and taking its oldest rows to make room would be losing somebody's data.
    /// It is reported rather than skipped, and it is not a failure — so a run
    /// that reached nothing else and was refused somewhere is still a run that
    /// did nothing, which is what a caller must be told.
    #[tokio::test]
    async fn a_table_that_refuses_at_its_limit_is_left_alone_and_said_to_be() {
        const REFUSING: TableLimit = TableLimit {
            name: "kept_by_a_plugin",
            database: Database::Server,
            strategy: Strategy::RefuseAtLimit,
            order_by: "id",
            max_rows: Some(3),
            prune_batch: None,
        };
        const STRANGER: TableLimit = TableLimit {
            name: "nobody_declared_this",
            database: Database::Server,
            strategy: Strategy::OldestFirst,
            order_by: "id",
            max_rows: Some(1),
            prune_batch: Some(1),
        };
        let mut rows = Rows::with("kept_by_a_plugin", 50);
        let done = super::prune_these_declared_by(&mut rows, &[REFUSING], &[REFUSING], false)
            .await
            .expect("a table left alone is not a failure");
        assert_eq!(
            done,
            vec![Pruned {
                table: "kept_by_a_plugin",
                before: None,
                removed: 0,
                would_remove: 0,
                left_alone: Some(super::Untouched::ItsWriterRefusesAtTheLimit),
            }]
        );
        assert_eq!(
            done[0].code(),
            None,
            "left alone on purpose is not a failure"
        );
        assert!(
            rows.deletions.is_empty(),
            "user state was deleted from: {:?}",
            rows.deletions
        );

        let text =
            super::prune_these_declared_by(&mut rows, &[REFUSING, STRANGER], &[REFUSING], false)
                .await
                .expect_err("a run that reached only a table left alone did nothing");
        assert!(text.contains(STRANGER.name), "{text}");
    }

    /// A table of the other database is refused by name, before any statement.
    #[tokio::test]
    async fn a_table_of_the_other_database_is_refused_before_it_is_asked_about() {
        const LOCAL: TableLimit = TableLimit {
            name: "rows_of_the_local_file",
            database: Database::LocalFile,
            strategy: Strategy::OldestFirst,
            order_by: "id",
            max_rows: Some(1),
            prune_batch: Some(1),
        };
        let mut rows = Rows::with("rows_of_the_local_file", 50);
        let text = super::prune_these_declared_by(&mut rows, &[LOCAL], &[LOCAL], false)
            .await
            .expect_err("a server target handed a local table");
        assert!(
            text.contains("this target prunes the server's PostgreSQL"),
            "{text}"
        );
        assert!(
            text.starts_with(&format!(
                "[{}]",
                nc_catalog::ErrorCode::DbTableNotBounded.code()
            )),
            "{text}"
        );
        assert_eq!(
            rows.deletions,
            [] as [(std::string::String, std::string::String, u64); 0]
        );

        // `run` for the catalog's whole list: the server target is asked about
        // the server's tables and no other.
        let mut rows = Rows::default();
        let done = run(&mut rows, Request::every()).await.expect("a run");
        let considered: Vec<&str> = done.iter().map(|pruned| pruned.table).collect();
        let servers: Vec<&str> = nc_catalog::TABLES
            .iter()
            .filter(|table| table.database == Database::Server)
            .map(|table| table.name)
            .collect();
        assert_eq!(considered, servers);
        assert!(
            servers.len() < nc_catalog::TABLES.len(),
            "the catalog bounds no local table, so this cannot tell a filter from none"
        );
    }

    /// The door that deletes names the database and the strategy it refuses.
    #[test]
    fn the_delete_door_refuses_a_table_it_is_not_the_pruner_of() {
        for (table, order_by, database, said) in [
            (
                "store_units",
                "unit",
                Database::Server,
                "a table of the local SQLite file",
            ),
            (
                "access_records",
                "recorded",
                Database::LocalFile,
                "a table of the server's PostgreSQL",
            ),
            (
                "store_entries",
                "rev_unit",
                Database::LocalFile,
                "its writer refusing at the limit",
            ),
            (
                "journal_entries",
                "sequence",
                Database::Server,
                "the policy pack",
            ),
        ] {
            let refused = super::the_catalog_orders_its_own_tables(table, order_by, database)
                .expect_err(table);
            assert!(refused.why.contains(said), "{table}: {}", refused.why);
            assert_eq!(refused.code(), nc_catalog::ErrorCode::DbPruneRefused);
        }
        assert_eq!(
            super::the_catalog_orders_its_own_tables("store_units", "unit", Database::LocalFile),
            Ok(())
        );
        assert!(
            super::the_catalog_orders_its_own_tables(
                "store_units",
                "committed_at",
                Database::LocalFile
            )
            .is_err(),
            "a column the catalog does not order by"
        );
    }

    /// `LIMIT` is this module's `access_records`, at numbers small enough to
    /// count by hand — a different entry from the catalog's, which is exactly
    /// what `prune_these` refuses. Driven through the seam that says so.
    async fn ours(rows: &mut Rows, tables: &[TableLimit]) -> Result<Vec<Pruned>, String> {
        super::prune_these_declared_by(rows, tables, &[LIMIT, PACKED], false).await
    }

    /// A refusal beside real work keeps the work it reported.
    ///
    /// **"Nothing was done" is not "removed nothing", and both directions are
    /// held.** The predicate is *every row is a table the policy pack bounds*,
    /// and its own paragraph records what `done.is_empty()` costs: a rehearsal
    /// with one refusal throws away every prediction it has made, and a table
    /// something else held a lock on — the one `Pruned` a scheduler escalates
    /// on — goes with it. Planting `done.is_empty()` there is
    /// caught. Planting `pruned.removed == 0` is what this catches: a **dry run
    /// removes nothing by definition**, so every rehearsal beside a refusal
    /// becomes `Err`, and so does every real run where the tables under their
    /// limits happen to be the ones that answered. Measured without
    /// this: `check passed`.
    #[tokio::test]
    async fn a_rehearsal_beside_a_refusal_still_reports_what_it_would_do() {
        const STRANGER: TableLimit = TableLimit {
            name: "a_table_the_catalog_never_heard_of",
            database: Database::Server,
            strategy: Strategy::OldestFirst,
            order_by: "id",
            max_rows: Some(1),
            prune_batch: Some(1),
        };
        let mut rows = Rows::default();
        rows.counts.insert(LIMIT.name.to_owned(), 140);
        let done = super::prune_these_declared_by(
            &mut rows,
            &[LIMIT, STRANGER],
            &[LIMIT, PACKED],
            // **A rehearsal**, which is the case that makes `removed == 0` true
            // of correct work.
            true,
        )
        .await
        .expect("a refusal beside real work is not a run that did nothing");

        let predicted = done
            .iter()
            .find(|entry| entry.table == LIMIT.name)
            .expect("the table under a real limit was inspected");
        assert_eq!(predicted.removed, 0, "a rehearsal removed rows");
        assert!(
            predicted.would_remove > 0,
            "the rehearsal predicted nothing for a table 40 over its limit, so this test \
             cannot tell a discarded report from an empty one"
        );
        assert!(
            done.iter().any(|entry| matches!(
                &entry.left_alone,
                Some(super::Untouched::ItWasRefused(why))
                    if why.contains(STRANGER.name)
            )),
            "the refusal beside the work is not in the report: {done:?}"
        );
        // A rehearsal issues the same statement with nothing to take, which is
        // what makes it a rehearsal of the real thing rather than of a
        // different path — so what must hold is the batch, not the silence.
        assert!(
            rows.deletions.iter().all(|(_, _, batch)| *batch == 0),
            "a rehearsal issued a statement that takes rows: {:?}",
            rows.deletions
        );

        // And the direction the predicate is really for: a request that reached
        // nothing but the policy pack's own carve-out **is** a run that did
        // nothing, and the refusal is what a caller gets.
        let mut rows = Rows::default();
        let text =
            super::prune_these_declared_by(&mut rows, &[PACKED, STRANGER], &[LIMIT, PACKED], true)
                .await
                .expect_err("a request that only reached the carve-out did nothing");
        assert!(
            text.contains(STRANGER.name),
            "the refusal a caller receives does not name what was refused: {text}"
        );
    }

    /// A batch larger than the table takes the table, not the batch.
    ///
    /// **The `.min(before)`, decided here.** Over a case with
    /// `batch <= before` — the catalog's `access_records`, 500,000 over 50,000;
    /// `LIMIT`, 100 over 10; the dry-run case, 14 over 4 —
    /// `let would_remove = batch;` answers the same, so the clause needs a case
    /// of its own. The clause the comment argues for — "or every row if the
    /// table holds fewer than a batch" — is reachable only when
    /// `prune_batch > max_rows`. A dry run would then predict more rows than
    /// any real run can take.
    ///
    /// **`catalog/limits.toml` cannot express that.** `nc-catalog-compiler`'s
    /// loader refuses the shape outright as
    /// `batch-past-limit`, and pruning only runs when `before > max_rows >=
    /// prune_batch`, so for every catalog-legal entry `batch.min(before)` *is*
    /// `batch` and this clause is reachable only through
    /// `prune_these_declared_by`, where a caller passes the numbers directly; the
    /// test is worth having for exactly that caller.
    #[tokio::test]
    async fn a_batch_larger_than_the_table_takes_the_table() {
        const SMALL: TableLimit = TableLimit {
            name: "rows_a_test_created",
            database: Database::Server,
            strategy: Strategy::OldestFirst,
            order_by: "id",
            max_rows: Some(2),
            prune_batch: Some(500),
        };
        let mut rows = Rows::with("rows_a_test_created", 3);
        let done = super::prune_these_declared_by(&mut rows, &[SMALL], &[SMALL, PACKED], false)
            .await
            .expect("prunes");
        assert_eq!(
            done,
            vec![Pruned {
                table: "rows_a_test_created",
                before: Some(3),
                removed: 3,
                would_remove: 3,
                left_alone: None,
            }],
            "a batch of 500 over a table of 3 must take 3, and a dry run must say 3"
        );
    }

    #[tokio::test]
    async fn a_table_over_its_limit_loses_exactly_one_batch_of_the_oldest() {
        let mut rows = Rows::with("rows_a_test_created", 150);
        let done = ours(&mut rows, &[LIMIT]).await.expect("prunes");
        assert_eq!(
            done,
            vec![Pruned {
                table: "rows_a_test_created",
                before: Some(150),
                removed: 10,
                would_remove: 10,
                left_alone: None,
            }]
        );
        assert_eq!(
            rows.deletions,
            vec![("rows_a_test_created".to_owned(), "id".to_owned(), 10)]
        );
        assert_eq!(done[0].remaining(), Some(140));
    }

    #[tokio::test]
    async fn a_table_within_its_limit_is_not_touched() {
        // Not "delete a few anyway": a pruner that ran every tick would hold
        // every table at its ceiling and make headroom unmeasurable.
        let mut rows = Rows::with("rows_a_test_created", 100);
        let done = ours(&mut rows, &[LIMIT]).await.expect("prunes");
        assert_eq!(done[0].removed, 0);
        assert!(rows.deletions.is_empty(), "{:?}", rows.deletions);
    }

    #[tokio::test]
    async fn one_row_over_the_limit_is_one_batch_and_not_one_row() {
        // The shape §15.8 asks for. Deleting one row at a time is the load
        // problem that arrives exactly when the system is busiest.
        let mut rows = Rows::with("rows_a_test_created", 101);
        ours(&mut rows, &[LIMIT]).await.expect("prunes");
        assert_eq!(rows.deletions[0].2, 10);
    }

    #[tokio::test]
    async fn far_over_the_limit_is_still_one_batch() {
        // The schedule catches up. Looping until the table is under would hold a
        // lock for as long as the backlog takes.
        let mut rows = Rows::with("rows_a_test_created", 10_000);
        ours(&mut rows, &[LIMIT]).await.expect("prunes");
        assert_eq!(rows.deletions.len(), 1);
        assert_eq!(rows.deletions[0].2, 10);
    }

    /// A target that refuses one table the way PostgreSQL does under a lock.
    ///
    /// At module scope because two tests drive it, and because one test holding
    /// both would cross a hundred lines.
    #[derive(Debug, Default)]
    struct OneIsLocked {
        rows: Rows,
    }
    impl Target for OneIsLocked {
        fn database(&self) -> Database {
            Database::Server
        }
        async fn count(&mut self, table: &str) -> Result<u64, super::Failure> {
            if self.rows.refuse_first.as_deref() == Some(table) {
                return Err(super::Failure::refused(format!(
                    "`{table}` is archived and will not be pruned"
                )));
            }
            if table == "a_locked" {
                // The database's own fact, not a phrase: decided by a
                // `contains` over the message, a `BEFORE DELETE` trigger
                // raising "the exporter gave up waiting for a lock file" reads
                // as contention, and a permanent failure is reported as a
                // benign skip on every tick for ever.
                return Err(super::Failure {
                    why: "canceling statement due to lock timeout".to_owned(),
                    waited: true,
                    answered: true,
                });
            }
            self.rows.count(table).await
        }
        async fn delete_oldest(
            &mut self,
            table: &str,
            order_by: &str,
            batch: u64,
        ) -> Result<u64, super::Failure> {
            self.rows.delete_oldest(table, order_by, batch).await
        }
    }

    /// A lock skip beside a refusal is still a lock skip.
    ///
    /// **`verdict`'s predicate names one variant, and this decides that it is
    /// one.** The paragraph above it argues for exactly that wording — "a table
    /// something else held a lock on, the one `Pruned` a scheduler escalates
    /// on, went with it". Measured without this: broadened
    /// to `APolicyPackBoundsIt | SomethingElseHeldALock`, the whole run
    /// answers `Err` and the lock skip is discarded, with `check passed` —
    /// because `catalog/limits.toml` ships two bounded tables and
    /// one of them is the carve-out, so the case that bites cannot be built
    /// from the catalog. Driven through the seam that takes its own limits, it
    /// can.
    #[tokio::test]
    async fn a_lock_skip_beside_a_refusal_survives_the_verdict() {
        const STRANGER: TableLimit = TableLimit {
            name: "a_table_the_catalog_never_heard_of",
            database: Database::Server,
            ..LIMIT
        };
        let locked_limit = TableLimit {
            name: "a_locked",
            database: Database::Server,
            ..LIMIT
        };
        let mut target = OneIsLocked::default();
        let beside = super::prune_these_declared_by(
            &mut target,
            &[locked_limit, PACKED, STRANGER],
            &[locked_limit, PACKED],
            false,
        )
        .await
        .expect("a lock skip beside a refusal is not a run that did nothing");
        assert!(
            beside
                .iter()
                .any(|pruned| pruned.left_alone == Some(super::Untouched::SomethingElseHeldALock)),
            "the row a scheduler escalates on was thrown away with the refusal: {beside:?}"
        );
    }

    /// A table something else is holding does not stop the ones after it.
    ///
    /// **A lock bound that returns through the loop's `?` stops every table
    /// after it.** Measured against a real database, two tables both over their
    /// limit with an `ACCESS EXCLUSIVE` lock held on the first:
    ///
    /// ```text
    /// after    5.0017s
    /// outcome  Err("`a_locked` could not be counted: … lock timeout")
    /// b_free   still holds 10 rows, over its limit of 4, never looked at
    /// ```
    ///
    /// Driven here through a target that refuses one table the way PostgreSQL
    /// does, because what is being asserted is the loop and not the timeout.
    #[tokio::test]
    async fn a_table_something_else_is_holding_is_skipped_and_reported() {
        let locked_limit = TableLimit {
            name: "a_locked",
            database: Database::Server,
            ..LIMIT
        };
        let free = TableLimit {
            name: "b_free",
            database: Database::Server,
            ..LIMIT
        };
        let mut target = OneIsLocked::default();
        target.rows.counts.insert("b_free".to_owned(), 150);
        let done = super::prune_these_declared_by(
            &mut target,
            &[locked_limit, free],
            &[locked_limit, free],
            false,
        )
        .await
        .expect("a lock on one table is not a failed run");

        assert_eq!(done.len(), 2, "{done:?}");
        assert_eq!(
            done[0].left_alone,
            Some(super::Untouched::SomethingElseHeldALock)
        );
        assert_eq!(done[0].before, None, "it was never counted");
        assert_eq!(
            done[1].removed, 10,
            "the table after the locked one was not pruned: {done:?}"
        );

        // **A message is not a fact.** The database's message carries text
        // the application wrote, so with the skip decided by `contains` over
        // it, an ordinary `BEFORE DELETE` trigger — no lock, nothing timed
        // out — raising `'this table is archived: the exporter gave up waiting
        // for a lock file'` reads as contention, and the run reports
        // `Ok([Pruned { …, left_alone: Some(SomethingElseHeldALock) }])` while
        // the table sits at nine rows against a limit of six. On every tick.
        let mut target = OneIsLocked::default();
        target.rows.counts.insert("b_free".to_owned(), 150);
        target.rows.refuse =
            Some("this table is archived: the exporter gave up waiting for a lock file".to_owned());
        let escalated = super::prune_these_declared_by(&mut target, &[free], &[free], false).await;
        assert!(
            escalated.is_err(),
            "a trigger's own words were read as contention: {escalated:?}"
        );

        // **A table that refuses permanently does not stop the ones after it.**
        // The run still fails — at the end — but every table that could be
        // pruned has been, where returning on the first refusal would let one
        // `RAISE EXCEPTION 'no'` on the alphabetically first table stop pruning
        // for all of them.
        let mut target = OneIsLocked::default();
        target.rows.counts.insert("b_free".to_owned(), 150);
        target.rows.refuse_first = Some("a_locked".to_owned());
        let stopped = super::prune_these_declared_by(
            &mut target,
            &[locked_limit, free],
            &[locked_limit, free],
            false,
        )
        .await;
        // **The refusal is reported and the work is not discarded.** An
        // `is_err()` alone would pass a `verdict` that returns `Err` while
        // throwing away every `Pruned` it collected — a run that pruned one
        // table and was refused on another telling its caller about neither,
        // with the rows already gone. The refusal rides along as its own row.
        let done = stopped.expect("a run that pruned something reports what it pruned");
        assert!(
            done.iter()
                .any(|pruned| matches!(pruned.left_alone, Some(super::Untouched::ItWasRefused(_)))),
            "the refusal was not reported: {done:?}"
        );
        assert!(
            done.iter().any(|pruned| pruned.removed > 0),
            "the table after the refusing one was not pruned: {done:?}"
        );
        assert_eq!(
            target.rows.deletions.len(),
            1,
            "the table after the refusing one was not pruned: {:?}",
            target.rows.deletions
        );

        // And an error that is not a lock still ends the run: a bad `order_by`
        // is a defect in the catalog, not something the next tick will fix.
        let mut target = OneIsLocked::default();
        target.rows.counts.insert("b_free".to_owned(), 150);
        target.rows.refuse = Some("column \"nope\" does not exist".to_owned());
        let stopped = super::prune_these_declared_by(&mut target, &[free], &[free], false).await;
        assert!(stopped.is_err(), "{stopped:?}");
    }

    /// The journal is carved out under its schema-qualified name too.
    ///
    /// **The `bare` comparison's own case.** Without it, `if bare == JOURNAL`
    /// can be reverted to `if table.name == JOURNAL`
    /// with `cargo test -p nc-prune` green: `public.journal_entries` is still
    /// refused, because the entry-versus-catalog clause below catches it and
    /// says something else. Only the *sentence* changes — so this is defence in
    /// depth rather than an exploit, and a clause whose only evidence is
    /// another clause's verdict is a clause nothing is holding.
    ///
    /// Called directly rather than through the pool: the refusal is decided
    /// before any statement is prepared, and a case that needs a database to
    /// prove a name comparison is a case that will be skipped.
    /// A table at or below its limit is not pruned, and no database is needed
    /// to say so.
    ///
    /// **The clause that decides whether the pruner runs at all, decided
    /// outside the PostgreSQL suite**, which `--offline` skips: without
    /// this, `before <= max_rows` weakened to `before == max_rows`
    /// leaves `cargo test -p nc-prune --lib` green
    /// and `cargo xtask check --offline` at `check passed` — with a pruner that
    /// removes a batch from every table **below** its
    /// limit. For `access_records` that empties a live table 50 000 rows at a
    /// time, every run answering `Ok`.
    ///
    /// `in_postgres.rs`'s header claims *"the parent module's tests prove the
    /// arithmetic — when to run, how many to take, what to report"*. They proved
    /// two of the three; this is the third, and it is the one that needs no
    /// database, because whether to run is arithmetic over two numbers.
    #[tokio::test]
    async fn a_table_below_or_at_its_limit_is_left_alone() {
        for held in [0, 1, 99, 100] {
            let mut rows = Rows::default();
            rows.counts.insert(LIMIT.name.to_owned(), held);
            let done = ours(&mut rows, &[LIMIT]).await.expect("a legal catalog");
            let pruned = done
                .iter()
                .find(|entry| entry.table == LIMIT.name)
                .expect("the table was inspected");
            assert_eq!(
                pruned.removed, 0,
                "a table holding {held} of a limit of 100 lost {} row(s)",
                pruned.removed
            );
            assert_eq!(
                rows.counts.get(LIMIT.name).copied(),
                Some(held),
                "the table holding {held} was written to"
            );
            assert!(
                rows.deletions.is_empty(),
                "a statement was issued against a table below its limit: {:?}",
                rows.deletions
            );
        }
        // And one row over, so this cannot pass by never pruning at all.
        let mut rows = Rows::default();
        rows.counts.insert(LIMIT.name.to_owned(), 101);
        let done = ours(&mut rows, &[LIMIT]).await.expect("a legal catalog");
        let pruned = done
            .iter()
            .find(|entry| entry.table == LIMIT.name)
            .expect("the table was inspected");
        assert_eq!(pruned.removed, 10, "one row over the limit takes a batch");
    }

    #[test]
    fn the_journal_is_carved_out_under_its_schema_qualified_name() {
        let qualified = TableLimit {
            name: "public.journal_entries",
            database: Database::Server,
            strategy: Strategy::OldestFirst,
            max_rows: Some(0),
            prune_batch: Some(1_000_000),
            order_by: "sequence",
        };
        let said = super::the_catalog_disagrees(&qualified, std::slice::from_ref(&qualified))
            .expect("the journal is never prunable by row count");
        assert!(
            said.contains("is the command journal") && said.contains("§15.8"),
            "`public.journal_entries` was refused, and not as the journal: {said}"
        );
    }

    #[tokio::test]
    /// Three ways past the guard, each one measured against a real database.
    ///
    /// A guard comparing `strategy` and only `strategy`, and only when the
    /// caller's name is character-for-character the catalog's, lets three
    /// tables be emptied through it with the public API alone, measured.
    async fn the_catalog_decides_the_whole_entry_and_not_one_field_of_it() {
        // (a) A legal second name for the same table. `identifier` accepts
        // `schema.table` — deliberately — and the default `search_path` makes
        // this the journal. `removed 1000000`, with the crate's own header
        // claiming it holds §15.8's carve-out.
        let mut rows = Rows::with("public.journal_entries", 1_000_000);
        let refused = prune_these(
            &mut rows,
            &[TableLimit {
                name: "public.journal_entries",
                database: Database::Server,
                strategy: Strategy::OldestFirst,
                order_by: "sequence",
                max_rows: Some(0),
                prune_batch: Some(1_000_000),
            }],
        )
        .await;
        assert!(refused.is_err(), "{refused:?}");
        assert!(rows.deletions.is_empty(), "{:?}", rows.deletions);
        // And the authority cannot grant it either: the journal is carved out
        // by name, so the seam the tests use does not open this door.
        let through_the_seam = super::prune_these_declared_by(
            &mut rows,
            &[TableLimit {
                name: "public.journal_entries",
                database: Database::Server,
                strategy: Strategy::OldestFirst,
                order_by: "sequence",
                max_rows: Some(0),
                prune_batch: Some(1_000_000),
            }],
            &[TableLimit {
                name: "public.journal_entries",
                database: Database::Server,
                strategy: Strategy::OldestFirst,
                order_by: "sequence",
                max_rows: Some(0),
                prune_batch: Some(1_000_000),
            }],
            false,
        )
        .await;
        assert!(through_the_seam.is_err(), "{through_the_seam:?}");
        assert!(rows.deletions.is_empty(), "{:?}", rows.deletions);

        // (b) A table the catalog says nothing about, so there is nothing to
        // disagree with. `_sqlx_migrations` is the one this reaches.
        let mut rows = Rows::with("_sqlx_migrations", 500);
        let refused = prune_these(
            &mut rows,
            &[TableLimit {
                name: "_sqlx_migrations",
                database: Database::Server,
                strategy: Strategy::OldestFirst,
                order_by: "version",
                max_rows: Some(0),
                prune_batch: Some(500),
            }],
        )
        .await;
        assert!(refused.is_err(), "{refused:?}");
        assert!(rows.deletions.is_empty(), "{:?}", rows.deletions);

        // (c) The catalog's own table, its strategy matched, and every other
        // field the caller's — including an `order_by` of `id`, which the
        // catalog does not order it by. `removed 400000`, leaving the table
        // empty.
        let declared = nc_catalog::TABLES
            .iter()
            .find(|table| table.name == "access_records")
            .expect("access_records is a bounded table");
        let mut rows = Rows::with("access_records", 400_000);
        let refused = prune_these(
            &mut rows,
            &[TableLimit {
                order_by: "id",
                max_rows: Some(0),
                prune_batch: Some(400_000),
                ..*declared
            }],
        )
        .await;
        assert!(refused.is_err(), "{refused:?}");
        assert!(rows.deletions.is_empty(), "{:?}", rows.deletions);

        // And the catalog's entry, handed over as the catalog wrote it, is
        // accepted — or the three assertions above would hold against a guard
        // that refuses everything.
        let mut rows = Rows::with("access_records", 600_000);
        let done = prune_these(&mut rows, std::slice::from_ref(declared))
            .await
            .expect("the catalog's own entry is not a forgery");
        assert_eq!(done[0].removed, 50_000);
    }

    #[tokio::test]
    async fn the_journal_this_repository_ships_is_bounded_by_the_pack_and_not_by_a_number() {
        // **Against the catalog that ships**, not against a fixture. Without
        // this, setting `journal_entries` to `oldest-first` with a `max_rows`
        // and a `prune_batch` leaves `cargo xtask check` entirely green, and
        // this pruner would then delete 20,000 journal rows per run — which is
        // what `NOCTUA.md §15.8` forbids in as many words: audit retention is
        // "never a global constant, and never silently pruned below what the
        // pack promises". The only other thing that objects is a fixture whose
        // `replace` needle happens to be the journal's own two lines, which is
        // a drift guard rather than a rule about the journal.
        let journal = nc_catalog::TABLES
            .iter()
            .find(|table| table.name == "journal_entries")
            .expect("the journal is a bounded table");
        assert_eq!(
            journal.strategy,
            Strategy::PolicyPack,
            "the journal is bounded by a number, and this pruner would apply it"
        );
        assert_eq!(journal.max_rows, None, "§15.8: never a global constant");
        assert_eq!(journal.prune_batch, None, "§15.8: never a global constant");

        // And end to end: the real catalog, through the real entry point.
        //
        // **With a second table over its limit**, because without one this
        // assertion ranges over nothing: `Rows::with` leaves every other table
        // at zero and the journal is `PolicyPack`, so `deletions` is empty and
        // an `.all()` over it is true whatever the pruner does. Measured with an
        // iterator probe over the whole suite — without the second table this
        // closure never sees an element, so a `run()` that prunes nothing
        // satisfies the "end to end" half of a `§15.8` test.
        let mut rows = Rows::with("journal_entries", 1_000_000);
        rows.counts.insert("access_records".to_owned(), 1_000_000);
        run(&mut rows, Request::default()).await.expect("prunes");
        assert!(
            rows.deletions
                .iter()
                .any(|(table, _, _)| table == "access_records"),
            "nothing was pruned at all, so what follows is an assertion about an empty \
             list: {:?}",
            rows.deletions
        );
        assert!(
            rows.deletions
                .iter()
                .all(|(table, _, _)| table != "journal_entries"),
            "the journal was pruned by the shipped catalog: {:?}",
            rows.deletions
        );
    }

    #[tokio::test]
    async fn a_table_a_policy_pack_bounds_is_reported_and_never_touched() {
        // §15.8's exception, and the one that matters most: audit retention is a
        // promise, and a global constant here would one day prune below it.
        //
        // **And it is reported.** `done.is_empty()` is also what a pruner that
        // never looked at the table produces, so asserting it would contradict
        // the doc comment's own rule, that a run must be able to tell "did
        // nothing" from "did not look".
        let mut rows = Rows::with("journal_entries", 1_000_000);
        let done = prune_these(&mut rows, &[PACKED]).await.expect("prunes");
        assert_eq!(
            done,
            vec![Pruned {
                table: "journal_entries",
                before: None,
                removed: 0,
                would_remove: 0,
                left_alone: Some(super::Untouched::APolicyPackBoundsIt),
            }],
            "the journal was not reported as considered"
        );
        assert_eq!(done[0].remaining(), None, "nothing counted it");
        assert!(
            rows.deletions.is_empty(),
            "the journal was pruned: {:?}",
            rows.deletions
        );
    }

    /// What `Broken::delete_oldest` says if the run ever reaches it.
    const DELETING_WAS_REACHED: &str = "delete_oldest ran, and counting fails first";

    #[tokio::test]
    async fn a_failure_names_the_table() {
        struct Broken;
        impl Target for Broken {
            fn database(&self) -> Database {
                Database::Server
            }
            fn count(
                &mut self,
                _: &str,
            ) -> impl Future<Output = Result<u64, super::Failure>> + Send {
                core::future::ready(Err(super::Failure::refused("the connection went away")))
            }
            // Not `unreachable!`: a diverging body leaves `ready` with no value
            // to hold, and an `async` block for it is a `manual_async_fn` that
            // clippy asks to be an `async fn` again. So reaching this method is
            // a sentence in the error instead, and the assertion below reads it
            // — which says what a panic would.
            fn delete_oldest(
                &mut self,
                _: &str,
                _: &str,
                _: u64,
            ) -> impl Future<Output = Result<u64, super::Failure>> + Send {
                core::future::ready(Err(super::Failure::refused(DELETING_WAS_REACHED)))
            }
        }
        let error = prune_these(&mut Broken, &[LIMIT])
            .await
            .expect_err("must fail");
        assert!(error.contains("rows_a_test_created"), "{error}");
        assert!(
            !error.contains(DELETING_WAS_REACHED),
            "counting failed and the run deleted anyway: {error}"
        );
    }

    #[test]
    fn the_declared_command_is_the_one_this_crate_implements() {
        // **`limits.prune` is declared, so `run` takes what it declares.**
        // `catalog/commands.toml` gives the command an `args_schema` naming
        // `table` and `dry_run`; a `run` taking neither would leave the command
        // a dispatcher calls and the function that answers it disagreeing,
        // with nothing in the tree saying so. A declared surface nothing
        // implements is worse than an undeclared one: the first reads as done.
        let declared = nc_catalog::command("limits.prune").expect("the command is declared");
        let schema = declared.args_schema;
        for argument in ARGUMENTS {
            assert!(
                declared.arguments.contains(&argument),
                "`Request` has `{argument}` and the declared args_schema does not: {schema}"
            );
        }
        // And the other direction, which is the one that rots: a property added
        // to the declaration with nothing behind it.
        for declared_argument in declared.arguments {
            assert!(
                ARGUMENTS.contains(declared_argument),
                "the declared args_schema has `{declared_argument}` and `Request` does not"
            );
        }
        assert_eq!(declared.arguments.len(), ARGUMENTS.len());

        assert_eq!(declared.action, "Delete");
        assert!(declared.mutating, "pruning removes rows");
    }

    /// Every argument [`Request`] accepts, by the name the command declares.
    ///
    /// Written out rather than derived, because a struct's field names are not
    /// available at runtime — and the point is that the two lists are compared.
    const ARGUMENTS: [&str; 2] = ["table", "dry_run"];

    #[tokio::test]
    async fn a_dry_run_answers_the_question_and_removes_nothing() {
        // **Over the *shipped* limit**, because `run` reads the catalog rather
        // than a fixture: this is the entry point a dispatcher will call, and
        // what it enforces is what `catalog/limits.toml` says.
        let bounded = nc_catalog::TABLES
            .iter()
            .find(|table| table.strategy == Strategy::OldestFirst)
            .expect("the catalog bounds something oldest-first");
        let over = bounded
            .max_rows
            .expect("an oldest-first table names a limit")
            + 1;
        let mut rows = Rows::with(bounded.name, over);
        let done = run(&mut rows, Request::just(bounded.name).dry())
            .await
            .expect("runs");
        assert_eq!(done.len(), 1, "only the table asked for: {done:?}");
        assert_eq!(done[0].before, Some(over));
        assert_eq!(done[0].removed, 0, "a dry run removed rows");
        assert!(
            done[0].would_remove > 0,
            "a dry run over the limit reported nothing to do: {done:?}"
        );
        // **It does issue the statement**, with a batch of zero — that is what
        // makes it a rehearsal rather than a prediction. What it must not do is
        // remove a row.
        assert_eq!(
            rows.deletions,
            vec![(bounded.name.to_owned(), bounded.order_by.to_owned(), 0)],
            "a dry run did not issue the statement it is rehearsing"
        );
        assert_eq!(
            rows.counts.get(bounded.name).copied(),
            Some(over),
            "a dry run removed rows"
        );

        // And the same request without the flag does it.
        let done = run(&mut rows, Request::just(bounded.name))
            .await
            .expect("runs");
        assert_eq!(done[0].removed, done[0].would_remove);
        assert_eq!(rows.deletions.len(), 2, "the rehearsal and then the run");
    }

    #[tokio::test]
    async fn a_table_the_catalog_does_not_bound_is_named_rather_than_ignored() {
        // An empty report reads as "nothing needed doing", which is what a
        // misspelled name would have produced.
        let mut rows = Rows::default();
        let error = run(&mut rows, Request::just("access_recods").dry())
            .await
            .expect_err("must name it");
        assert!(error.contains("access_recods"), "{error}");
    }

    #[tokio::test]
    async fn the_catalog_gives_the_pruner_something_to_do() {
        // The coverage floor. A catalog in which every table is the policy-pack
        // exception would leave this component with no data at all, and every
        // test above passing against limits its own test invented.
        //
        // **Not `!done.is_empty()`.** A policy-pack table reports itself as
        // considered-and-left-alone, so `done` is non-empty in exactly the case
        // this is written to catch — and an assertion on it would not measure
        // the floor it names. What it has to count is the tables this component
        // can actually act on.
        let mut rows = Rows::default();
        let done = run(&mut rows, Request::default()).await.expect("runs");
        let actionable = done
            .iter()
            .filter(|entry| entry.left_alone.is_none())
            .count();
        assert!(
            actionable > 0,
            "no table in catalog/limits.toml is one this can prune, so nothing here is \
             exercised against what ships: {done:?}"
        );
    }
}
