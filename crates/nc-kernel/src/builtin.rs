//! The kernel's own commands: what runs a command the catalog gives the kernel.
//!
//! **One handler for each command the catalog declares in a namespace the
//! kernel keeps, and none for any other.** `catalog/commands.toml` declares
//! `journal.verify`, `limits.prune`, `kernel.undo`, `kernel.redo`,
//! `naming.propose` and `naming.read`; the
//! registry gives every one to the kernel (`crate::registry`), the decision
//! point decides each as it decides a plugin's command — journaled, run in a
//! unit when it changes state — and [`Builtin`] is what runs it once it is
//! allowed. This module's test holds the catalog's list, the registry's and
//! this one to each other both ways, so a command declared with nothing to run
//! it, or a handler for a command nothing declares, fails the tests rather than
//! reaching a caller.
//!
//! **`journal.verify` reads the realm's chain** from `from`, or from its
//! start, and answers `{"verified":<entries>}`, or
//! `{"broken":{"reason":"<code>","sequence":<n>}}` at the first entry that
//! does not verify, the reason being the code of what broke it — logging
//! `journal.verified`, with the entry it began at, or `journal.broken` either
//! way. A journal that cannot be
//! read is `unavailable` with its own code, never a verdict. It is `Read` on the
//! realm, which the family pack gives a guardian
//! (`family.a_guardian_reads_the_realm`).
//!
//! **`limits.prune` runs the one pruner over the local database** (`NOCTUA.md
//! §15.8`): every table the catalog bounds there, or the one `table` names,
//! for real or — with `dry_run` — counting what a real run would remove and
//! removing nothing. It answers `{"pruned":[…]}`, one row per table the pruner
//! considered, each with `table`, `removed` and `would_remove`, and, for a
//! table it left alone, `left_alone` saying why and `code` when that is a
//! refusal; a run that pruned nothing because every table it was asked for
//! refused is refused itself, in the pruner's code. It is `Delete` on the
//! realm, which the pack gives a guardian (`family.a_guardian_prunes_the_realm`)
//! and the core holds to a quorum (`core.destructive_needs_quorum`), so it runs
//! only for a guardian holding an approval the kernel issued for it. What it
//! deletes is outside every unit, so the step that ran it is one undo stops at:
//! its unit notes it as having left the machine before it runs
//! (`crate::history`), a dry run as well, since that note is taken before
//! anything the command does is known — **and for the same reason it deletes
//! only once its entry has landed**: a run is rehearsed first, as a dry run,
//! and journaled as failed when that is refused; otherwise the decision point
//! journals it as allowed before the pruner deletes anything, so a journal
//! that does not take that entry leaves every row where it was. A prune that would reach the units
//! table first reverts each unit this kernel left committed in the store and
//! open in the journal (`crate::units`), and is refused, with the code of what
//! refused the reversal, while one stays: the pruner deletes the oldest units'
//! records, and one of those is what the recovery on open reverts it from.
//! `kernel.undo` and `kernel.redo` are `crate::history`'s.
//!
//! **`naming.propose` and `naming.read` are the one file-naming scheme as
//! commands** (`NOCTUA.md §16.3`), so every surface that dispatches names a
//! file the same way. `propose` builds a name from `title`, `signature`,
//! `keywords` and `extension` through `nc_ids::naming::propose`, under the
//! identifier `id` names — kept, so a renamed file is still the same file — or,
//! when there is none, one the journal's minter mints, the machine's one
//! (`NOCTUA.md §16.2`). `read` reads `name` back through
//! `nc_ids::naming::read`. Each answers the name and its fields as canonical
//! JSON — `id`, `keywords`, `name`, `signature`, and `title` and `extension`
//! when there are — and `read` says with `canonical` whether `name` was already
//! written as the scheme writes it. Both are `Read` on `Feature::"naming"`,
//! which the family pack gives every person of the realm
//! (`family.a_member_names_files`): they read nothing of the realm's, so they
//! open no unit and change nothing, and the journal's entry names the command
//! and the feature, never the arguments. **The kernel is handed text only**:
//! a kept identifier is read in `nc-ids`, which cannot log, and comes back as
//! text, so nothing here turns a caller's text into an identifier the log
//! could carry.
//!
//! What this answers (`NOCTUA.md §2.4`): a declared command that answers that
//! it is not built, a pruner run by a caller the pack does not let delete on
//! the realm — every run is decided, journaled and counted against the quorum
//! the core asks for — an undo reaching past what a prune took, and a prune
//! taking the record of a unit whose reversal is still owed. What it
//! does not answer: when the pruner runs on its own, which is the pump's, and
//! how often, which is the daemon's (Stage 13); and a store held in memory,
//! which has no database to prune, so `limits.prune` answers a table the
//! catalog bounds, or every table, with no table at all — a name the catalog
//! bounds nowhere is refused first, as over any store.

use nc_canon::Value;
use nc_catalog::ErrorCode;

use crate::backend::StoreBackend as _;
use nc_ids::naming::{Fields, Identifier, NameError};

use crate::dispatch::{Failure, Refusal, RefusalKind, Waits};
use crate::logging::LogSink as _;
use crate::{Environment, Shared};

/// The command that verifies the realm's chain.
pub(crate) const VERIFY: &str = "journal.verify";

/// The command that holds the local tables to their bounds.
pub(crate) const PRUNE: &str = "limits.prune";

/// The command that proposes a file's name.
pub(crate) const PROPOSE: &str = "naming.propose";

/// The command that reads a file's name back.
pub(crate) const READ: &str = "naming.read";

/// Where `journal.verify` starts: the sequence of the first entry it verifies.
const FROM: &str = "from";

/// The one table `limits.prune` is asked to prune, when it is asked for one.
const TABLE: &str = "table";

/// Whether `limits.prune` only counts what a real run would remove.
const DRY_RUN: &str = "dry_run";

/// The title a name is proposed under, shortened to fit by the builder.
const TITLE: &str = "title";

/// The parts of the signature a name is proposed under.
const SIGNATURE: &str = "signature";

/// The keywords a name is proposed under.
const KEYWORDS: &str = "keywords";

/// The extension a name is proposed with.
const EXTENSION: &str = "extension";

/// The identifier a name keeps: the text of one a file already carries.
const ID: &str = "id";

/// The name `naming.read` reads back.
const NAME: &str = "name";

/// How many characters of a table's name a prune's refusal repeats.
///
/// The name of a table no catalog bounds is the caller's text, so it is cut
/// and escaped as everything a refusal repeats is; every table the catalog
/// bounds is named in fewer.
const NAMED: usize = 64;

/// A command the kernel runs itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builtin {
    /// `journal.verify`: the realm's chain, verified.
    Verify,
    /// `limits.prune`: the local tables held to their bounds.
    Prune,
    /// `kernel.undo`: the person's newest step, taken back.
    Undo,
    /// `kernel.redo`: the person's newest undo, put back.
    Redo,
    /// `naming.propose`: a file's name, built from its parts.
    Propose,
    /// `naming.read`: a file's name, read back into its parts.
    ReadName,
}

impl Builtin {
    /// Every one: the only place a handler is made.
    ///
    /// So a variant left out of it is one the build never constructs, which
    /// the workspace's lints refuse, and one in it is found by its name.
    const ALL: [Self; 6] = [
        Self::Verify,
        Self::Prune,
        Self::Undo,
        Self::Redo,
        Self::Propose,
        Self::ReadName,
    ];

    /// The name of the command it runs, as the catalog declares it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Verify => VERIFY,
            Self::Prune => PRUNE,
            Self::Undo => crate::history::UNDO,
            Self::Redo => crate::history::REDO,
            Self::Propose => PROPOSE,
            Self::ReadName => READ,
        }
    }

    /// The handler of the command called `name`, if the kernel runs one.
    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|builtin| builtin.name() == name)
    }
}

/// `journal.verify`: the chain from `from`, verified.
///
/// It answers `{"verified":<entries>}`, or
/// `{"broken":{"reason":"<code>","sequence":<n>}}` where the chain breaks —
/// the reason being the code of what broke it — and logs which.
///
/// # Errors
///
/// `unavailable`, with the journal's own code, when the journal could not be
/// read: no verdict is answered then.
pub(crate) async fn verified<E: Environment>(
    shared: &Shared<E>,
    args: &serde_json::Value,
    waits: Waits,
) -> Result<String, Failure> {
    let from = args
        .get(FROM)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let started = std::time::Instant::now();
    let verdict = waits
        .on(async { shared.journal.lock().await.verify(from).await })
        .await;
    match verdict {
        Ok(Ok(entries)) => {
            shared.log.record(&nc_log::events::JournalVerified {
                entries,
                from,
                took: started.elapsed(),
            });
            Ok(Value::object([("verified", count(entries))]).to_canonical())
        }
        Ok(Err(broken)) => {
            shared.log.record(&nc_log::events::JournalBroken {
                sequence: broken.sequence,
                code: broken.code(),
            });
            Ok(Value::object([(
                "broken",
                Value::object([
                    ("sequence", count(broken.sequence)),
                    ("reason", Value::text(broken.code().code())),
                ]),
            )])
            .to_canonical())
        }
        Err(error) => Err(Failure::new(
            Refusal::new(
                RefusalKind::Unavailable,
                error.code(),
                "the journal could not be read, so it was not verified".to_owned(),
            ),
            "journal-unreadable",
        )),
    }
}

/// `naming.propose`: a file's name, from the parts `args` holds.
///
/// Under the identifier `id` holds, kept, or — when it holds none — one the
/// journal's minter mints, the machine's one: minted only here, once the
/// decision point has allowed the dispatch, so a denied proposal spends none.
///
/// # Errors
///
/// `invalid` with the code [`unnamed`] gives for a kept identifier that is not
/// one or a name that does not fit, and `unavailable` with the journal's code
/// when its minter mints nothing ([`unminted`]).
pub(crate) async fn proposed<E: Environment>(
    shared: &Shared<E>,
    args: &serde_json::Value,
    waits: Waits,
) -> Result<String, Failure> {
    if let Some(kept) = args.get(ID).and_then(serde_json::Value::as_str) {
        return proposal(args, Identifier::Kept(kept));
    }
    let minted = waits
        .on(async { shared.journal.lock().await.mint_identifier() })
        .await
        .map_err(|error| unminted(&error))?;
    proposal(args, Identifier::Minted(minted))
}

/// The name `args` proposes under `id`, answered as its fields.
fn proposal(args: &serde_json::Value, id: Identifier<'_>) -> Result<String, Failure> {
    let signature = texts(args, SIGNATURE);
    let keywords = texts(args, KEYWORDS);
    let fields = nc_ids::naming::propose(
        &signature,
        args.get(TITLE).and_then(serde_json::Value::as_str),
        &keywords,
        id,
        args.get(EXTENSION).and_then(serde_json::Value::as_str),
    )
    .map_err(|error| unnamed(PROPOSE, &error))?;
    Ok(answer(&fields, None))
}

/// `naming.read`: the name `args` holds, read back into its fields.
///
/// # Errors
///
/// `invalid` with the code [`unnamed`] gives for a name that does not parse,
/// or that parses and does not fit the scheme.
pub(crate) fn read_back(args: &serde_json::Value) -> Result<String, Failure> {
    let name = args
        .get(NAME)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let reading = nc_ids::naming::read(name).map_err(|error| unnamed(READ, &error))?;
    Ok(answer(reading.fields(), Some(reading.canonical())))
}

/// The texts of the list argument `name`, none when it is absent.
fn texts<'a>(args: &'a serde_json::Value, name: &str) -> Vec<&'a str> {
    args.get(name)
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default()
}

/// What a naming command answers for `fields`: every field, and, for a
/// reading, whether the name was already `canonical`.
///
/// The lists are always there and a title or an extension only when the name
/// has one, so an absent field is absent rather than `null`.
fn answer(fields: &Fields, canonical: Option<bool>) -> String {
    let listed = |items: &[String]| {
        Value::Array(
            items
                .iter()
                .map(|item| Value::text(item.as_str()))
                .collect(),
        )
    };
    let mut answer = vec![
        ("id", Value::text(fields.id())),
        ("keywords", listed(fields.keywords())),
        ("name", Value::text(fields.name())),
        ("signature", listed(fields.signature())),
    ];
    if let Some(title) = fields.title() {
        answer.push(("title", Value::text(title)));
    }
    if let Some(extension) = fields.extension() {
        answer.push(("extension", Value::text(extension)));
    }
    if let Some(canonical) = canonical {
        answer.push(("canonical", Value::Bool(canonical)));
    }
    Value::object(answer).to_canonical()
}

/// The failure of `command`, refused `error` by the naming scheme.
///
/// **Each cause its own code and word.** A name that does not fit is
/// `[NC-IDS-0005]`; a kept identifier that is not one is base32's own
/// `[NC-IDS-0006]`, since what was wrong is the identifier and not a name;
/// and a name `naming.read` cannot read is `[NC-IDS-0004]`. The sentence
/// repeats nothing of the caller's text but the one character base32 names,
/// escaped.
fn unnamed(command: &str, error: &NameError) -> Failure {
    let (code, word) = match error {
        NameError::TooLong { .. } => (error.clone().code(), "name-too-long"),
        NameError::BadIdentifier(why) if command == PROPOSE => (why.code(), "not-an-identifier"),
        NameError::NoIdentifier | NameError::BadIdentifier(_) => {
            (error.clone().code(), "name-unparsable")
        }
    };
    Failure::new(
        Refusal::new(
            RefusalKind::Invalid,
            code,
            format!("`{command}` answered no name: {error}"),
        ),
        word,
    )
}

/// The failure of a `naming.propose` the journal's minter minted no
/// identifier for: `unavailable`, in the journal's own code.
fn unminted(error: &nc_journal::Error) -> Failure {
    Failure::new(
        Refusal::new(
            RefusalKind::Unavailable,
            error.code(),
            format!("`{PROPOSE}` answered no name, because no identifier was minted for it"),
        ),
        "no-identifier",
    )
}

/// What `limits.prune` came to before it deletes anything.
#[derive(Debug)]
pub(crate) enum Prune<'a> {
    /// Answered with nothing deleted: a dry run, or a store held in memory,
    /// which has no database.
    Answered(String),
    /// A real run of what was asked, over the local database's own pruner,
    /// which deletes only once the decision point has journaled the prune
    /// ([`deleted`]).
    Deletes(nc_prune::sqlite::Sqlite, nc_prune::Request<'a>),
}

/// `limits.prune`: the pruner, run over the local database as asked — up to
/// what it deletes.
///
/// Every table the catalog bounds there, or the one `table` names, removing
/// rows or, with `dry_run`, only counting them. It answers every table the
/// pruner considered (`answered`): what it removed from each, what a real run
/// would remove now, and why it left one alone. A store held in memory has no
/// database, and is answered with no table once the one asked for, if any, is
/// shown to be a table the catalog bounds.
///
/// **The units this kernel left are reverted first** (`crate::units`): a
/// unit whose close the journal did not take and whose reversal the store
/// refused is committed in the store and open in the journal, and its record
/// is what the recovery on open reverts it from — so a prune that would reach
/// the units table reverts each first, and is refused while one stays: a dry
/// run too, since the run it rehearses would be.
///
/// **Rehearsed, and deleting nothing, until its entry lands.** What deletes is
/// outside every unit, so nothing a journal refuses afterwards could put it
/// back: deleting first, a prune whose entry the journal refused would be
/// answered as withheld, its unit closed as aborted and ten thousand of a
/// person's units gone. So a real run is a dry run first, which every refusal
/// the pruner can give of what was asked meets, and then [`Prune::Deletes`],
/// which the decision point carries out only once it has journaled the prune as
/// allowed.
///
/// # Errors
///
/// `invalid` with `[NC-DB-0004]` for a table the catalog does not bound
/// (`unbounded`); what refused a unit's reversal, in its case and with its
/// code, when a prune that would reach the units table finds one still owed
/// (`held_back`); and what the pruner refused when it pruned nothing
/// (`refused`): a table the local database does not hold, `invalid` with
/// the same code; a database that did not answer or a table another writer
/// held, `unavailable`; and anything else, `internal` with the pruner's code.
pub(crate) async fn pruned<'a, E: Environment>(
    shared: &Shared<E>,
    args: &'a serde_json::Value,
) -> Result<Prune<'a>, Failure> {
    let table = args.get(TABLE).and_then(serde_json::Value::as_str);
    let dry_run = args
        .get(DRY_RUN)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if let Some(refusal) = table.and_then(unbounded) {
        return Err(refusal);
    }
    let Some(mut target) = shared.store.pruner() else {
        return Ok(Prune::Answered(answered(&[])));
    };
    if reaches_the_units(table)
        && let Err(held) = crate::units::reverted_again(shared).await
    {
        return Err(held_back(&held));
    }
    let asked = table.map_or_else(nc_prune::Request::every, nc_prune::Request::just);
    match nc_prune::run(&mut target, asked.dry()).await {
        Ok(tables) if dry_run => Ok(Prune::Answered(answered(&tables))),
        Ok(_) => Ok(Prune::Deletes(target, asked)),
        Err(why) => Err(refused(&why)),
    }
}

/// `limits.prune` for real: `asked`, run over `target` — once the decision
/// point has journaled it ([`pruned`]).
///
/// # Errors
///
/// What the pruner refused when it pruned nothing (`refused`), which a run
/// can meet after its rehearsal did not — a table another writer took since.
pub(crate) async fn deleted(
    mut target: nc_prune::sqlite::Sqlite,
    asked: nc_prune::Request<'_>,
) -> Result<String, Failure> {
    match nc_prune::run(&mut target, asked).await {
        Ok(tables) => Ok(answered(&tables)),
        Err(why) => Err(refused(&why)),
    }
}

/// What `limits.prune` answers for `tables`, the pruner's report, as
/// canonical JSON.
///
/// One row per table, in the pruner's order, each naming the table, what was
/// removed and what a real run would remove — and, for a table left alone,
/// why, with the code of a refusal or of a table another writer held.
fn answered(tables: &[nc_prune::Pruned]) -> String {
    let rows = tables
        .iter()
        .map(|pruned| {
            let mut fields = vec![
                ("table", Value::text(pruned.table)),
                ("removed", count(pruned.removed)),
                ("would_remove", count(pruned.would_remove)),
            ];
            if let Some(why) = &pruned.left_alone {
                fields.push(("left_alone", Value::text(left_alone(why))));
            }
            if let Some(code) = pruned.code() {
                fields.push(("code", Value::text(code.code())));
            }
            Value::object(fields)
        })
        .collect();
    Value::object([("pruned", Value::Array(rows))]).to_canonical()
}

/// The word a table left alone is answered with, by why.
const fn left_alone(why: &nc_prune::Untouched) -> &'static str {
    match why {
        nc_prune::Untouched::ItWasRefused(_) => "refused",
        nc_prune::Untouched::APolicyPackBoundsIt => "bounded_by_its_pack",
        nc_prune::Untouched::SomethingElseHeldALock => "held_by_another_writer",
        nc_prune::Untouched::ItsWriterRefusesAtTheLimit => "bounded_by_its_writer",
        // A reason a later pruner gives that this build does not know.
        _ => "left_alone",
    }
}

/// The failure of a `limits.prune` asked for `table`, when the catalog
/// bounds no table of that name.
///
/// **Before the pruner is asked**, because the pruner's own refusal repeats
/// the name whole and as it is, and the name is the caller's text: here it is
/// repeated cut to [`NAMED`] characters, each one a reader would not see
/// escaped, and every sentence the pruner says after this names only tables
/// the catalog declares.
fn unbounded(table: &str) -> Option<Failure> {
    if nc_catalog::TABLES.iter().any(|known| known.name == table) {
        return None;
    }
    Some(Failure::new(
        Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::DbTableNotBounded,
            format!(
                "`{PRUNE}` was asked for {}, which is not a table catalog/limits.toml \
                 bounds, so there is no limit to hold it to: `grep '^.tables.' \
                 catalog/limits.toml` names the ones there are",
                crate::registry::quoted_up_to(table, NAMED)
            ),
        ),
        "prune-refused",
    ))
}

/// The table the pruner deletes a unit's record from, its images and notes
/// going with it.
const UNITS: &str = "store_units";

/// Whether a prune asked for `table`, or for every table, reaches [`UNITS`].
fn reaches_the_units(table: Option<&str>) -> bool {
    table.is_none_or(|name| name == UNITS)
}

/// The failure of a `limits.prune` that would reach [`UNITS`] while a unit
/// the journal holds open is committed in the store, `held` being why its
/// reversal did not land.
///
/// **The whole run, and not the units table alone.** Every other table of the
/// local database is bounded by its writer, so a run around the units table
/// would prune nothing, and a run that prunes nothing because what it was
/// asked for refused is refused itself — the pruner's own rule for one.
fn held_back(held: &Refusal) -> Failure {
    Failure::new(
        Refusal::new(
            held.kind(),
            held.code(),
            format!(
                "`{PRUNE}` pruned nothing, because a unit whose close the journal did not take \
                 is still to be reverted, and the pruner could take its record: {}",
                held.sentence()
            ),
        ),
        "prune-refused",
    )
}

/// The failure of a `limits.prune` the pruner refused, `why` being its
/// sentence.
///
/// **In the pruner's own code**, read off the sentence it opens on, and in
/// the case that code means to its caller: a table that is not one the local
/// database bounds was asked for, a database that did not answer or a lock
/// that was not let go may pass, and anything else is the store's.
fn refused(why: &str) -> Failure {
    let code = crate::pump::code_of(why);
    let kind = match code {
        ErrorCode::DbTableNotBounded => RefusalKind::Invalid,
        ErrorCode::DbUnreachable | ErrorCode::DbPruneFailed => RefusalKind::Unavailable,
        _ => RefusalKind::Internal,
    };
    Failure::new(
        Refusal::new(kind, code, format!("`{PRUNE}` pruned nothing: {why}")),
        "prune-refused",
    )
}

/// A count, as an answer writes it.
fn count(counted: u64) -> Value {
    Value::Integer(i64::try_from(counted).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use nc_catalog::ErrorCode;

    use super::{
        Builtin, DRY_RUN, EXTENSION, FROM, ID, KEYWORDS, NAME, PROPOSE, PRUNE, READ, SIGNATURE,
        TABLE, TITLE, VERIFY, answered, proposal, read_back, refused, unminted, unnamed,
    };
    use crate::dispatch::RefusalKind;
    use crate::registry::{Owner, RESERVED, Registry};

    /// Every argument `journal.verify` reads, by the name the command declares.
    const VERIFY_READS: [&str; 1] = [FROM];

    /// Every argument `limits.prune` reads, by the name the command declares.
    const PRUNE_READS: [&str; 2] = [TABLE, DRY_RUN];

    /// Every argument `naming.propose` reads, by the name the command declares.
    const PROPOSE_READS: [&str; 5] = [TITLE, SIGNATURE, KEYWORDS, EXTENSION, ID];

    /// Every argument `naming.read` reads, by the name the command declares.
    const READ_READS: [&str; 1] = [NAME];

    /// Whether `name` is in a namespace the kernel keeps.
    fn the_kernels(name: &str) -> bool {
        RESERVED
            .iter()
            .any(|kept| name.starts_with(&format!("{kept}.")))
    }

    /// Every command the catalog gives the kernel is one it runs, and it runs
    /// no other — and the registry gives the kernel exactly those.
    ///
    /// **Both ways, and through the name the dispatch is matched by**: a
    /// handler missing for a declared command would answer a caller that the
    /// kernel runs no such command, and one for a command nothing declares is
    /// code no dispatch reaches.
    #[test]
    fn every_command_the_catalog_gives_the_kernel_is_one_it_runs_and_it_runs_no_other() {
        let declared: BTreeSet<&str> = nc_catalog::COMMANDS
            .iter()
            .map(|command| command.name)
            .filter(|name| the_kernels(name))
            .collect();
        let handled: BTreeSet<&str> = Builtin::ALL.iter().map(|builtin| builtin.name()).collect();
        assert_eq!(declared, handled, "the catalog's and the kernel's");
        assert_eq!(
            handled.len(),
            Builtin::ALL.len(),
            "two handlers run one command: {:?}",
            Builtin::ALL
        );
        for name in &declared {
            let builtin = Builtin::named(name)
                .unwrap_or_else(|| panic!("`{name}` is declared, and nothing runs it"));
            assert_eq!(builtin.name(), *name);
        }
        let registered: BTreeSet<String> = Registry::from_catalog()
            .listed()
            .into_iter()
            .filter(|command| command.owner == Owner::Kernel)
            .map(|command| command.name)
            .collect();
        assert_eq!(
            registered,
            handled.iter().map(|name| (*name).to_owned()).collect(),
            "the registry gives the kernel what it runs"
        );
        assert_eq!(Builtin::named("journal.verify.again"), None);
        assert_eq!(Builtin::named("kernel.unknown"), None);
    }

    /// The declared `journal.verify` and `limits.prune` take the arguments
    /// this module reads, and no other.
    ///
    /// **Both ways**, against `Command::arguments`, which the compiler emits
    /// from the parse it validates the schema with — an argument the catalog
    /// gains is one nothing here would read — and the decision each is made
    /// under: `Read` for the verification, which changes nothing, and `Delete`
    /// on the realm for the prune, which changes state and may be undone by
    /// nothing.
    #[test]
    fn the_declared_verify_and_prune_are_the_ones_this_module_implements() {
        for (name, reads) in [(VERIFY, &VERIFY_READS[..]), (PRUNE, &PRUNE_READS[..])] {
            let declared =
                nc_catalog::command(name).unwrap_or_else(|| panic!("`{name}` is declared"));
            let takes: BTreeSet<&str> = declared.arguments.iter().copied().collect();
            let read: BTreeSet<&str> = reads.iter().copied().collect();
            assert_eq!(takes, read, "`{name}` declares {:?}", declared.arguments);
            assert_eq!(declared.arguments.len(), reads.len(), "`{name}`");
        }
        assert_eq!((FROM, TABLE, DRY_RUN), ("from", "table", "dry_run"));
        let verify = nc_catalog::command(VERIFY).expect("declared");
        assert_eq!(
            (verify.action, verify.resource_selector, verify.mutating),
            ("Read", "realm", false)
        );
        assert!(matches!(
            verify.reversibility,
            nc_catalog::Reversibility::Read
        ));
        let prune = nc_catalog::command(PRUNE).expect("declared");
        assert_eq!(
            (prune.action, prune.resource_selector, prune.mutating),
            ("Delete", "realm", true)
        );
        assert!(matches!(
            prune.reversibility,
            nc_catalog::Reversibility::Write
        ));
    }

    /// The declared naming commands are the ones this module implements.
    ///
    /// **Both ways**, against `Command::arguments`, and the decision each is
    /// made under: `Read` on the platform's `naming` feature, never the realm,
    /// changing nothing — so no unit is opened for either, and the permit that
    /// decides them is a person's and not a guardian's.
    #[test]
    fn the_declared_naming_commands_are_the_ones_this_module_implements() {
        for (name, reads) in [(PROPOSE, &PROPOSE_READS[..]), (READ, &READ_READS[..])] {
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
                ("Read", "feature:naming", false),
                "`{name}`"
            );
            assert!(
                matches!(declared.reversibility, nc_catalog::Reversibility::Read),
                "`{name}`"
            );
        }
        assert_eq!(
            (
                PROPOSE, READ, TITLE, SIGNATURE, KEYWORDS, EXTENSION, ID, NAME
            ),
            (
                "naming.propose",
                "naming.read",
                "title",
                "signature",
                "keywords",
                "extension",
                "id",
                "name"
            )
        );
        assert_eq!(Builtin::named(PROPOSE), Some(Builtin::Propose));
        assert_eq!(Builtin::named(READ), Some(Builtin::ReadName));
    }

    /// The bound `property` of `command`'s schema declares under `keyword`,
    /// read off the catalog's own text.
    fn bound(command: &str, path: &[&str], keyword: &str) -> u64 {
        let declared = nc_catalog::command(command).expect("declared");
        let schema: serde_json::Value =
            serde_json::from_str(declared.args_schema).expect("a schema is JSON");
        let mut at = &schema["properties"];
        for step in path {
            at = &at[*step];
        }
        at[keyword]
            .as_u64()
            .unwrap_or_else(|| panic!("`{command}` declares no {keyword} at {path:?}"))
    }

    /// The naming schemas' bounds are the ones a name's byte limit implies.
    ///
    /// A name is at most `MAX_BYTES` bytes and a character a byte at least, so
    /// no part longer than that many characters fits, and a list item costs a
    /// character and its joiner, so no more than half as many fit. Held here,
    /// so a schema edited alone is a failure and not a bound that stopped
    /// meaning anything; the identifier is as wide as base32 writes one.
    #[test]
    fn the_naming_schemas_bounds_are_the_ones_a_names_byte_limit_implies() {
        let most = u64::try_from(nc_ids::naming::MAX_BYTES).expect("fits");
        let items = u64::try_from(nc_ids::naming::MAX_BYTES.div_ceil(2)).expect("fits");
        for list in [SIGNATURE, KEYWORDS] {
            assert_eq!(
                bound(PROPOSE, &[list, "items"], "maxLength"),
                most,
                "{list}"
            );
            assert_eq!(bound(PROPOSE, &[list], "maxItems"), items, "{list}");
        }
        assert_eq!(bound(PROPOSE, &[EXTENSION], "maxLength"), most);
        assert_eq!(bound(READ, &[NAME], "maxLength"), most);
        assert_eq!(bound(READ, &[NAME], "minLength"), 1);
        assert_eq!(
            bound(PROPOSE, &[ID], "maxLength"),
            u64::try_from(nc_ids::base32::MAX_WIDTH).expect("fits")
        );
        assert_eq!(bound(PROPOSE, &[ID], "minLength"), 1);
        assert_eq!(
            bound(PROPOSE, &[TITLE], "maxLength"),
            1_024,
            "the title is shortened to fit, so its bound bounds only the work"
        );
    }

    /// Each naming failure is refused with its own kind, code and word.
    ///
    /// A name that does not fit is the caller's to shorten; a kept identifier
    /// that is not one is the identifier's fault, under base32's own code; a
    /// name that cannot be read is not a name of this scheme; and a minter that
    /// mints nothing is the journal's, unavailable and never the caller's.
    #[test]
    fn each_naming_failure_is_refused_with_its_own_kind_code_and_word() {
        use nc_ids::NameError;
        use nc_ids::base32::Base32Error;

        let bad = NameError::BadIdentifier(Base32Error::Empty);
        for (failure, kind, code, word) in [
            (
                unnamed(PROPOSE, &NameError::TooLong { bytes: 300 }),
                RefusalKind::Invalid,
                ErrorCode::IdsNameTooLong,
                "name-too-long",
            ),
            (
                unnamed(READ, &NameError::TooLong { bytes: 300 }),
                RefusalKind::Invalid,
                ErrorCode::IdsNameTooLong,
                "name-too-long",
            ),
            (
                unnamed(PROPOSE, &bad),
                RefusalKind::Invalid,
                ErrorCode::IdsNotAnIdentifier,
                "not-an-identifier",
            ),
            (
                unnamed(READ, &bad),
                RefusalKind::Invalid,
                ErrorCode::IdsNameUnparsable,
                "name-unparsable",
            ),
            (
                unnamed(READ, &NameError::NoIdentifier),
                RefusalKind::Invalid,
                ErrorCode::IdsNameUnparsable,
                "name-unparsable",
            ),
            (
                unminted(&nc_journal::Error::NoIdentifier(
                    nc_ids::MintError::ClockStuck {
                        at: core::time::Duration::ZERO,
                    },
                )),
                RefusalKind::Unavailable,
                ErrorCode::IdsClockStuck,
                "no-identifier",
            ),
        ] {
            let refusal = failure.refusal();
            assert_eq!(
                (refusal.kind(), refusal.code(), failure.word()),
                (kind, code, word),
                "{}",
                refusal.sentence()
            );
        }
    }

    /// A proposal the handler answers is read back by the handler as its own
    /// fields.
    ///
    /// **Over the handler's JSON, not the crate's types**: each argument shape
    /// the schema admits — every field, none, lists of several, a kept
    /// identifier spelled loosely — answered by `naming.propose` and read back
    /// by `naming.read` from the name it answered, which gives every field
    /// again and says the name is already canonical. An answer that dropped a
    /// field, or a reading that read it from somewhere else, fails here.
    #[test]
    fn a_proposal_the_handler_answers_is_read_back_by_the_handler_as_its_own_fields() {
        use nc_ids::naming::Identifier;

        for (args, id) in [
            (
                serde_json::json!({"title": "Praia de Copacabana", "signature": ["familia", "fotos"],
                                   "keywords": ["brasil"], "extension": "jpg"}),
                Identifier::Kept("01J8XK2M9P51"),
            ),
            (serde_json::json!({}), Identifier::Kept("01J8XK2M9P51")),
            (
                serde_json::json!({"title": "Café à noite", "keywords": ["rio de janeiro", "noite"],
                                   "extension": "TAR.GZ", "id": "01j8xk2m9p5l"}),
                Identifier::Kept("01j8xk2m9p5l"),
            ),
            (
                serde_json::json!({"signature": ["a=b", "c"], "id": "01J8XK2M9P51"}),
                Identifier::Kept("01J8XK2M9P51"),
            ),
        ] {
            let proposed = proposal(&args, id)
                .unwrap_or_else(|failure| panic!("{args}: {:?}", failure.refusal()));
            let fields = nc_canon::parse(&proposed)
                .expect("canonical JSON")
                .into_object()
                .expect("an object");
            let name = fields
                .get("name")
                .cloned()
                .and_then(nc_canon::Value::into_text)
                .expect("a name");
            assert_eq!(
                fields
                    .get("id")
                    .and_then(|id| id.clone().into_text())
                    .as_deref(),
                Some("01J8XK2M9P51"),
                "{args}"
            );
            let read = read_back(&serde_json::json!({ "name": name }))
                .unwrap_or_else(|failure| panic!("{name}: {:?}", failure.refusal()));
            let mut back = nc_canon::parse(&read)
                .expect("canonical JSON")
                .into_object()
                .expect("an object");
            assert_eq!(
                back.remove("canonical"),
                Some(nc_canon::Value::Bool(true)),
                "{read}"
            );
            assert_eq!(back, fields, "{args}: proposed {proposed}, read {read}");
        }
        assert_eq!(
            proposal(
                &serde_json::json!({"title": "Praia de Copacabana", "signature": ["familia", "fotos"],
                                    "keywords": ["brasil"], "extension": "jpg"}),
                Identifier::Kept("01J8XK2M9P51")
            )
            .expect("the example"),
            "{\"extension\":\"jpg\",\"id\":\"01J8XK2M9P51\",\"keywords\":[\"brasil\"],\
             \"name\":\"==familia=fotos--praia-de-copacabana__brasil@@01J8XK2M9P51.jpg\",\
             \"signature\":[\"familia\",\"fotos\"],\"title\":\"praia-de-copacabana\"}"
        );
        let loose = read_back(&serde_json::json!({"name": "--Praia@@01j8xk2m9p51.JPG"}))
            .expect("a loose spelling reads");
        assert_eq!(
            loose,
            "{\"canonical\":false,\"extension\":\"jpg\",\"id\":\"01J8XK2M9P51\",\"keywords\":[],\
             \"name\":\"--praia@@01J8XK2M9P51.jpg\",\"signature\":[],\"title\":\"praia\"}"
        );
    }

    /// A table pruned, one within its bound and one left alone are each
    /// answered with what was done to it, and a refusal with its code.
    #[test]
    fn a_prune_is_answered_table_by_table_with_why_each_was_left_alone() {
        let pruned = nc_prune::Pruned {
            table: "store_units",
            before: Some(12),
            removed: 3,
            would_remove: 3,
            left_alone: None,
        };
        let rehearsed = nc_prune::Pruned {
            removed: 0,
            would_remove: 5,
            ..pruned.clone()
        };
        let user_state = nc_prune::Pruned {
            table: "store_entries",
            before: None,
            removed: 0,
            would_remove: 0,
            left_alone: Some(nc_prune::Untouched::ItsWriterRefusesAtTheLimit),
        };
        let locked = nc_prune::Pruned {
            table: "blobs",
            left_alone: Some(nc_prune::Untouched::SomethingElseHeldALock),
            ..user_state.clone()
        };
        let refusal = nc_prune::Pruned {
            table: "store_notes",
            left_alone: Some(nc_prune::Untouched::ItWasRefused(format!(
                "[{}] the table refused",
                ErrorCode::DbPruneRefused
            ))),
            ..user_state.clone()
        };
        let pack = nc_prune::Pruned {
            table: "journal_entries",
            left_alone: Some(nc_prune::Untouched::APolicyPackBoundsIt),
            ..user_state.clone()
        };
        assert_eq!(
            answered(&[pruned, rehearsed, user_state, locked, refusal, pack]),
            format!(
                "{{\"pruned\":[\
                 {{\"removed\":3,\"table\":\"store_units\",\"would_remove\":3}},\
                 {{\"removed\":0,\"table\":\"store_units\",\"would_remove\":5}},\
                 {{\"left_alone\":\"bounded_by_its_writer\",\"removed\":0,\"table\":\"store_entries\",\"would_remove\":0}},\
                 {{\"code\":\"{}\",\"left_alone\":\"held_by_another_writer\",\"removed\":0,\"table\":\"blobs\",\"would_remove\":0}},\
                 {{\"code\":\"{}\",\"left_alone\":\"refused\",\"removed\":0,\"table\":\"store_notes\",\"would_remove\":0}},\
                 {{\"left_alone\":\"bounded_by_its_pack\",\"removed\":0,\"table\":\"journal_entries\",\"would_remove\":0}}\
                 ]}}",
                ErrorCode::DbPruneFailed,
                ErrorCode::DbPruneRefused
            )
        );
        assert_eq!(answered(&[]), r#"{"pruned":[]}"#);
    }

    /// A prune the pruner refused is refused in the pruner's code, in the
    /// case that code means to its caller, its sentence cut to its bound.
    #[test]
    fn a_refused_prune_is_refused_in_the_pruners_code_and_the_case_it_means() {
        for (code, kind) in [
            (ErrorCode::DbTableNotBounded, RefusalKind::Invalid),
            (ErrorCode::DbUnreachable, RefusalKind::Unavailable),
            (ErrorCode::DbPruneFailed, RefusalKind::Unavailable),
            (ErrorCode::DbPruneRefused, RefusalKind::Internal),
        ] {
            let failure = refused(&format!("[{code}] `store_units` was refused"));
            let refusal = failure.refusal();
            assert_eq!((refusal.kind(), refusal.code()), (kind, code), "{code}");
            assert_eq!(
                refusal.sentence(),
                format!("`{PRUNE}` pruned nothing: [{code}] `store_units` was refused")
            );
        }
        let unnamed = refused("a sentence that names no code");
        assert_eq!(
            (unnamed.refusal().kind(), unnamed.refusal().code()),
            (RefusalKind::Internal, ErrorCode::DbPruneRefused),
            "a refusal is never read as none"
        );
    }

    /// A table the catalog bounds is the pruner's to judge, and any other is
    /// refused first, its name repeated cut to its bound with what a reader
    /// would not see escaped.
    #[test]
    fn a_table_no_catalog_bounds_is_refused_before_the_pruner_repeats_its_name() {
        for table in nc_catalog::TABLES {
            assert!(super::unbounded(table.name).is_none(), "{}", table.name);
            assert!(
                table.name.chars().count() <= super::NAMED,
                "`{}` is longer than a refusal repeats",
                table.name
            );
        }
        let hidden = format!("store_units\u{202e}{}", "t".repeat(1_000_000));
        let refusal = super::unbounded(&hidden).expect("no table is called that");
        let refusal = refusal.refusal();
        assert_eq!(
            (refusal.kind(), refusal.code()),
            (RefusalKind::Invalid, ErrorCode::DbTableNotBounded)
        );
        assert!(
            refusal.sentence().starts_with(&format!(
                "`{PRUNE}` was asked for `store_units\\u{{202e}}tt"
            )),
            "{}",
            refusal.sentence()
        );
        assert!(
            refusal.sentence().chars().count() < 400,
            "what the caller wrote is cut before it is repeated: {}",
            refusal.sentence().len()
        );
        assert!(!refusal.sentence().contains('\u{202e}'));
    }

    /// A clock standing at `2026-08-26T00:00:00Z`, which is inside the
    /// grant `g.current`, moving only past what an identifier asks it to.
    ///
    /// Not the system's, because the grant ends on a day `policy/world.json`
    /// names, and a test on the system's clock would be refused from then on.
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

    /// A kernel whose plugins' data is held in memory, on the [`Standing`]
    /// clock.
    #[derive(Debug)]
    struct HeldInMemory;

    impl crate::Environment for HeldInMemory {
        type Store = crate::backend::MemoryBackend;
        type Blobs = crate::blobs::MemoryBlobs;
        type Journal = nc_journal::InMemory;
        type Clock = Standing;
        type Directory = nc_authz::MemoryDirectory;
        type Log = crate::logging::Captured;
        type Transport = crate::net::Scripted;
    }

    /// A kernel over a store held in memory, deciding with the family pack
    /// over the world with `ana` granted in `home`, and `ana`.
    async fn held_in_memory() -> (crate::Kernel<HeldInMemory>, nc_authz::Principal) {
        let pack = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let world = include_str!("../../../policy/world.json");
        let directory = nc_authz::MemoryDirectory::from_world(world, &pack)
            .expect("the world is valid")
            .with_epoch("home", 7)
            .with_grant(
                "home",
                nc_authz::EntityRef {
                    type_name: "User".to_owned(),
                    id: "ana".to_owned(),
                },
                "g.current",
            )
            .expect("the realm holds the grant");
        let realm = directory.realm_id("home").expect("home is a realm");
        let ana = directory
            .principal(nc_authz::PrincipalKind::User, "ana")
            .expect("ana is a user of the world");
        let clock = Standing(Arc::new(AtomicU64::new(1_787_702_400_000)));
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
        (kernel, ana)
    }

    /// Dispatches `limits.prune` with `args` through `kernel` as `principal`,
    /// holding a quorum's approval of that very dispatch.
    async fn pruned_as(
        kernel: &crate::Kernel<HeldInMemory>,
        principal: &nc_authz::Principal,
        args: &str,
    ) -> Result<String, crate::dispatch::Refusal> {
        let approval = kernel.approval_for_tests(principal, PRUNE, args);
        kernel
            .dispatch(
                &crate::Caller::new(principal.clone()).approved(approval),
                PRUNE,
                args,
            )
            .await
    }

    /// A prune of a store held in memory answers no table, and a name no
    /// catalog bounds is refused first, as over the local database.
    ///
    /// Each prune is dispatched as a guardian holding a quorum's approval of
    /// it, so what answers is the command and not the decision point — for
    /// every table and for the units', each for real and as a dry run. A prune
    /// this store refused outright would fail here.
    #[tokio::test]
    async fn a_prune_of_a_store_held_in_memory_answers_no_table_and_refuses_one_no_catalog_bounds()
    {
        let (kernel, ana) = held_in_memory().await;
        for args in [
            "{}",
            r#"{"table":"store_units"}"#,
            r#"{"dry_run":true}"#,
            r#"{"dry_run":true,"table":"store_units"}"#,
        ] {
            let answered = pruned_as(&kernel, &ana, args)
                .await
                .unwrap_or_else(|refused| panic!("{args}: {refused:?}"));
            assert_eq!(answered, r#"{"pruned":[]}"#, "{args}");
        }
        let refused = pruned_as(&kernel, &ana, r#"{"table":"nothing_here"}"#)
            .await
            .expect_err("no catalog bounds a table of that name");
        assert_eq!(
            (refused.kind(), refused.code()),
            (RefusalKind::Invalid, ErrorCode::DbTableNotBounded),
            "{refused:?}"
        );
        assert!(
            refused
                .sentence()
                .starts_with(&format!("`{PRUNE}` was asked for `nothing_here`")),
            "{}",
            refused.sentence()
        );
    }
}
