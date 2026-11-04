//! The append-only ledger that makes "never reuse or renumber a code" a build
//! failure rather than a sentence in a document (`NOCTUA.md §15.1`).
//!
//! **Why a file and not `git log`.** This repository has no git history
//! (`NOCTUA.md §5.3`) and will not have one for a long time, so there is nothing
//! to diff a registry against. A committed ledger is the history: it records
//! what each code was reserved to mean, and the compiler refuses to repoint a
//! line it already contains.
//!
//! **What a recorded meaning is, exactly.** The message key *and* the developer
//! description: `error.db.unreachable — The database is unreachable`. The key is
//! the semantic identity the rest of the platform resolves, and a code pointed
//! at a different key now means something else — which is exactly the case where
//! "historical log entries start lying". The description is locked beside it
//! because it is what a developer reads to decide what an error *is*, and the
//! cost — a typo fix takes the same four deliberate steps as a repointing — is
//! the right price.
//!
//! **And what it is not: the sentence a user is shown.** That lives in
//! `catalog/i18n/<locale>.toml`, and this file has no opinion about it at all.
//! `NC-JRNL-0001` can keep its number, its key and its description while the
//! English sentence behind that key is rewritten from "The activity history does
//! not add up" to "The printer tray is empty", and every gate here stays green —
//! measured. The key is not locked *because* it is what a user is shown; that is
//! the one thing the mechanism does not protect.
//!
//! That is deliberate rather than an oversight, and the argument is the one this
//! module already makes about descriptions, applied to prose that is edited far
//! more often: translations get reworded, they are reworded by people who are
//! not reading this file, and a ledger that failed on a copy edit would teach
//! everyone to delete the ledger. What *is* checked is that the key exists in
//! every locale, in both directions, with matching placeholders
//! (`messages.rs`) — so a code always resolves to a sentence, and which sentence
//! it resolves to is a translation decision rather than a numbering one.
//!
//! # The holes, and what closes them
//!
//! The first version of this treated an absent ledger as an empty one, so
//! `rm catalog/errors.reserved.toml` freed every code in the repository and the
//! next build blessed the renumbering — with `§5.3` saying, in the same breath,
//! that there is nothing to restore from. **An absent ledger is a failure.**
//!
//! An empty `checksum` accepted as the bootstrap of a hand-created ledger would
//! be a smaller door in the same wall: two lines of editing — clear the
//! checksum, empty the table — would reopen the hole exactly, and
//! `cargo xtask check` would report `check passed` on a catalog whose error
//! code has been silently repointed. So the bootstrap is an **intent** rather
//! than a file state: [`Bootstrap::Allow`] comes only from
//! `cargo xtask build --reserve`, `check` never passes it, and an unverifiable
//! ledger therefore fails the gate whatever the file says.
//!
//! A drift check would look like a gate and not be one. A ledger emitted as
//! generated output and drift-checked against what the compiler produced is
//! checked against something *derived from the ledger on disk*, so a hand edit
//! ratifies itself and `check` reports "in sync". That is `§19.3`'s named trap
//! word for word. The ledger is not generated output: `build` appends to it,
//! `check` compares it against `catalog/errors.toml`, and a `CHECKSUM` over its
//! own contents catches an edit made outside either.
//!
//! Every guard above reads the *checksum*, and a ledger reserving nothing has a
//! correct one — `blake3` over nothing is a constant anybody can compute — so
//! `checksum = "<that constant>"` over an empty `[reserved]` verifies,
//! `reconcile`'s loop cannot iterate, and every code is re-derived as new. A
//! ledger that reserves nothing is refused, however it is vouched for.
//!
//! The same hole with its special case taken off says what a checksum over the
//! reservations alone is worth: *any* subset of a ledger is a ledger. Measured
//! over the reservations alone — one of the twenty-nine reservations kept and
//! the file re-signed by this module's own [`render`], with `NC-CANON-0001`'s
//! description in `catalog/errors.toml` changed to "The printer tray is empty"
//! — an ordinary `cargo xtask build`, no `--reserve` and no warning line,
//! appends the other twenty-eight, the ledger records the new meaning,
//! `docs/errors.md` publishes it and `cargo xtask check` prints
//! `check passed`. Criterion 7 switched off by the edit-build-check loop a
//! developer runs every day, and `§5.3` says there is no git to be the second
//! reader.
//!
//! Two things close it, because a file with no history behind it has to carry
//! the check itself. The ledger states **how many codes it has reserved**,
//! inside the checksummed region, so one holding a different number of lines
//! from the number it states is refused ([`read`]'s `ledger-count`). And
//! [`Reservations`] is opaque: nothing outside this module can build one, take a
//! line out of one, or hand [`render`] a smaller set than [`read`] returned. The
//! renderer is the only thing in the repository that computes a checksum on
//! request, and it does not sign a subset.
//!
//! # The honest limit
//!
//! The checksum defends against an accident — a stray `sed`, a bad merge, a
//! half-applied edit — and the append-only reconciliation defends against a
//! renumbering done the ordinary way. Neither defends against somebody who edits
//! the ledger and recomputes its checksum on purpose; nothing in one file can.
//! What covers that is the repository's history, once it has one, and saying so
//! here is better than a comment implying otherwise.
//!
//! The count has the same limit, and it is worth being exact about where it
//! falls. It refuses a ledger that lost lines while its `[meta]` survived, and
//! it takes the compiler away from anyone who wanted a subset signed for them —
//! but somebody who reimplements the hash above can write `count = 1` over one
//! reservation and be perfectly self-consistent. No number a file states about
//! itself closes that. The reader that would is one *outside* both files: a
//! committed count of the reservations that a person has to raise by hand, or
//! the history `§5.3` says this repository does not have yet.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::model::Catalog;
use crate::problem::{Problem, line_of};

/// Where the ledger lives.
pub const PATH: &str = "catalog/errors.reserved.toml";

/// The domain the ledger's checksum is separated by.
const CHECKSUM: &str = "noctua-central/catalog/errors.reserved/v1";

/// What a code was reserved to mean, keyed by the code.
type Meanings = BTreeMap<String, String>;

/// What a ledger reserves.
///
/// **Opaque, and that is half of what closes the subset hole above.** As
/// `pub type Reservations = BTreeMap<String, String>`, anything holding one
/// could take lines out of it and hand the rest to [`render`], which computes
/// the checksum over whatever it is given. That would make the compiler its own
/// forgery tool: a ledger holding one of the twenty-nine codes, signed by
/// `render`, verifies — and the next ordinary build appends the twenty-eight
/// missing ones with the meanings `catalog/errors.toml` carries today. With the
/// field private, the only reservations that exist are the ones [`read`] parsed
/// and the ones [`reconcile`] grew from them, and neither of those can shrink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservations {
    /// What each code was reserved to mean, keyed by the code.
    meanings: Meanings,
}

/// The ledger's file shape.
///
/// Declared here rather than inside [`read`], because an item after a statement
/// reads as if it came into being at that point.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    meta: Meta,
    #[serde(default)]
    reserved: Meanings,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    checksum: String,
    /// How many codes the ledger says it has reserved.
    ///
    /// Defaulted rather than required, because the file a brand-new catalog is
    /// started by hand with — `checksum = ""` over an empty `[reserved]`, which
    /// is what [`read`]'s own refusal prescribes — has no count to state, and
    /// calling that malformed would refuse the one documented way to create
    /// this file.
    #[serde(default)]
    count: usize,
}

/// Whether a ledger that cannot be verified may be accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bootstrap {
    /// The ordinary case, and what `cargo xtask check` always passes.
    Refuse,
    /// A ledger with an empty checksum is accepted and rewritten. Reached only
    /// through `cargo xtask build --reserve`, which says loudly what it did.
    Allow,
}
/// Whether the ledger states how many codes it holds.
///
/// **Asked of the document, not of the bytes.** `text.contains("count = ")` is
/// satisfied by any comment and by any `[reserved]` description holding those
/// six characters. `count` is defaulted, and its default is zero, so a forger
/// who deletes the line and re-signs over a smaller set gets a file whose size
/// guard reads `28 < 0` and passes — with one comment left behind to satisfy a
/// test of the bytes. A key is a key; prose that mentions one is not.
fn states_a_count(text: &str) -> bool {
    toml::from_str::<toml::Value>(text)
        .ok()
        .and_then(|document| document.get("meta")?.get("count").map(|_| ()))
        .is_some()
}

/// Reads the ledger, verifies it, and returns what it reserves.
///
/// # Errors
///
/// * `ledger-missing` — there is no file, or it cannot be read. **An absent
///   ledger is a failure, and it is not the same failure as an empty one**:
///   absent means the record of which codes have been used is gone and
///   `NOCTUA.md §5.3` says there is nothing to restore it from, while empty
///   means a file that verifies and says no code was ever reserved.
/// * `ledger-malformed` — it is not TOML, or it holds anything other than
///   `[meta] checksum`, `[meta] count` and a `[reserved]` table.
/// * `ledger-unvouched` — its `checksum` is empty and the caller did not pass
///   [`Bootstrap::Allow`]; or it verifies and reserves nothing at all. Both are
///   ledgers nothing vouches for, and one reserving nothing is written only by
///   `cargo xtask build --reserve`, which says what it reserved.
/// * `ledger-checksum` — the contents do not match the checksum, so the file was
///   written by something other than the compiler.
/// * `ledger-count` — the contents match the checksum and there are fewer of
///   them than the file says: a ledger only ever grows, so one holding fewer
///   lines than its `count` lost some.
///
/// An empty `checksum` with [`Bootstrap::Allow`] returns what the file holds and
/// reaches none of the three refusals after it — that file is the bootstrap
/// `cargo xtask build --reserve` exists for. A ledger that does carry a checksum
/// is read the same way whatever the caller passed.
pub fn read(root: &std::path::Path, bootstrap: Bootstrap) -> Result<Reservations, Problem> {
    let path = root.join(PATH);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Err(Problem::in_file(
            PATH,
            "is missing",
            "this file is the whole history of which error codes have been used, and \
             `NOCTUA.md §5.3` says there is nothing else to restore it from. If a code \
             was ever declared, restore the file. If this is a genuinely new catalog, \
             create it by hand with a `[meta]` table holding `checksum = \"\"` and an \
             empty `[reserved]` table, and the next build will fill both in",
            "ledger-missing",
        ));
    };
    let ledger: Ledger = toml::from_str(&text).map_err(|error| {
        Problem::in_file(
            PATH,
            format!("is not a ledger: {}", error.message()),
            "it holds a `[meta] checksum`, a `[meta] count` of how many codes have \
             been reserved, and a `[reserved]` table of code to message key — and \
             nothing else",
            "ledger-malformed",
        )
    })?;

    // An empty checksum is a ledger nothing vouches for. It is accepted only
    // when the caller said so — `cargo xtask build --reserve` — because a file
    // state anybody can write is not an intent.
    let expected = checksum(&ledger.reserved, ledger.meta.count);
    if ledger.meta.checksum.is_empty() {
        if bootstrap == Bootstrap::Allow {
            // **The count is deliberately not read here.** The remedy
            // `reconcile` prints for a repointed code is to take that code's
            // line out, blank the checksum and run `cargo xtask build
            // --reserve`, which leaves a file holding one line fewer than its
            // `count` says — the exact shape the refusal further down exists to
            // catch. A blank checksum vouches for nothing in the file, its count
            // included, so what is there is what is returned.
            return Ok(Reservations {
                meanings: ledger.reserved,
            });
        }
        return Err(Problem::at(
            PATH,
            line_of(&text, "checksum", 1).unwrap_or(1),
            "carries no checksum, so nothing vouches for what it says has been reserved",
            "an empty checksum is how a brand-new catalog is started, and it is accepted \
             only by `cargo xtask build --reserve` — which says what it reserved. If this \
             file was cleared to free an error code, restore it; if this really is a new \
             catalog, run that command once",
            "ledger-unvouched",
        ));
    }
    if ledger.meta.checksum != expected {
        // **And the fix text says what `--reserve` actually does**, not only
        // that it is the remedy. Measured with a truncating write: an
        // interrupted build leaves the ledger short — `fs::write` truncates
        // before it writes, so under `ulimit -f` it keeps 4096 bytes of 5090 and
        // 22 reservations of 28, with no error line at all — and a diagnostic
        // that blames a person for it and points at `--reserve`, which
        // re-derives every missing line from `catalog/errors.toml` **as it reads
        // now**, followed verbatim, ratifies in silence the meaning change the
        // previous build refused. The write is a rename (`xtask/src/build.rs`),
        // so that state cannot arise from an interruption; this is the other
        // half, because the sentence would send the reader through the bypass.
        return Err(Problem::at(
            PATH,
            line_of(&text, "checksum", 1).unwrap_or(1),
            "does not match its own checksum, so it was not written by the compiler",
            "the ledger records which error codes have been used and cannot be edited by \
             hand: an edited line frees a code for reuse, and every log entry ever \
             written with it would start meaning something else. Restore the file. \
             Clearing the checksum and running `cargo xtask build --reserve` is the last \
             resort and not the first: it re-derives every line from \
             `catalog/errors.toml` as that file reads *today*, so any meaning a code has \
             been given since is accepted without a word",
            "ledger-checksum",
        ));
    }
    // **And a ledger that is a subset of itself, correctly signed.** Every
    // refusal above reads the checksum, and a checksum computed over the lines
    // that are left is a checksum that verifies — so the empty ledger the
    // refusal below catches is only the smallest of these. Measured on a copy
    // of the real `catalog/`, without the count: the ledger cut to one of its
    // twenty-nine reservations and re-signed over what was left, with
    // `NC-CANON-0001`'s description changed to "The printer tray is empty" —
    // `compile` returns `Ok`, the ledger it produces holds twenty-nine
    // reservations again and records the new meaning as though it always had,
    // and no problem is raised anywhere. That call is the one
    // `cargo xtask check` makes, so criterion 7 would be off, one special case
    // wider than the one below.
    //
    // So the file states its own size, the size is hashed with the reservations,
    // and holding fewer lines than that is a refusal. It comes *after* the
    // checksum on purpose: lines taken out by hand leave a checksum that does
    // not verify, and `ledger-checksum` is the truer sentence for that file.
    // **And a ledger that states no count at all.** `count` is defaulted so the
    // hand-written bootstrap file can omit it, and that default is zero — so a
    // forger who deletes the line and re-signs with the count it now implies
    // gets a self-consistent file whose guard below reads `1 < 0` and passes.
    // The bootstrap is the only file entitled to omit it, and a bootstrap is a
    // blank checksum, which has already returned above. Past this point a
    // vouched-for ledger states its size or it is not one.
    // **And the question is asked of the document, not of the bytes.**
    // `text.contains("count = ")` is satisfied by any comment and by any
    // `[reserved]` description holding those six characters — measured, under
    // it the forged count-less ledger is refused, and the identical file plus
    // one comment line above `[meta]` is accepted. A key is a key; prose that
    // mentions one is not.
    if !states_a_count(&text) {
        return Err(Problem::at(
            PATH,
            line_of(&text, "checksum", 1).unwrap_or(1),
            "states no count, so nothing says how many codes it is meant to hold",
            "the compiler writes `count` beside the checksum and hashes the two together; \
             a ledger without it was written by something else. Restore the file",
            "ledger-count",
        ));
    }
    if ledger.reserved.len() < ledger.meta.count {
        return Err(Problem::at(
            PATH,
            line_of(&text, "count = ", 1).unwrap_or(1),
            format!(
                "says {} code(s) have been reserved and holds {}",
                ledger.meta.count,
                ledger.reserved.len()
            ),
            "a ledger only ever grows, so one holding fewer lines than it says it holds \
             lost some — and every code they reserved is free to be re-derived with \
             whatever meaning `catalog/errors.toml` carries today, which is how a \
             renumbering gets blessed by an ordinary build. Restore the file. Blanking \
             the checksum and running `cargo xtask build --reserve` is the last resort \
             and not the first: it re-derives every missing line from \
             `catalog/errors.toml` as that file reads *today*",
            "ledger-count",
        ));
    }
    // **And a ledger that reserves nothing, whose checksum is correct.** The
    // refusals above read the *checksum* and the count, and the empty set
    // satisfies both — `blake3` over nothing is a constant anybody can compute,
    // and nothing is exactly as many reservations as it says it holds — so a
    // file holding only `[meta] checksum = "<that constant>"`, `count = 0` and
    // an empty `[reserved]` verifies here, `reconcile`'s loop cannot iterate, and every
    // code is re-derived as new. Measured without this refusal:
    // `NC-CANON-0001`'s description repointed, the ledger emptied that way, and
    // an ordinary `cargo xtask build` — no `--reserve`, no warning — prints
    // `29 code(s) appended to catalog/errors.reserved.toml, which is permanent`,
    // after which `cargo xtask check` passes and the ledger records the new
    // meaning as though it always had. That is acceptance criterion 7 turned off
    // by the edit-build-check loop, and `§5.3` says there is no git to be the
    // second reader.
    //
    // A test that uses a *wrong* checksum stops one branch earlier and does not
    // cover this.
    //
    // A ledger with no reservations is only ever written by `--reserve`, which
    // announces what it did.
    if ledger.reserved.is_empty() {
        return Err(Problem::at(
            PATH,
            line_of(&text, "reserved", 1).unwrap_or(1),
            "reserves no code at all, so nothing it vouches for can be compared",
            "an empty ledger is how a brand-new catalog starts, and it is written only by \
             `cargo xtask build --reserve`, which says what it reserved. If this file was \
             cleared to free an error code, restore it",
            "ledger-unvouched",
        ));
    }

    Ok(Reservations {
        meanings: ledger.reserved,
    })
}

/// Checks the catalog against what has already been reserved, and returns the
/// ledger the build should write.
///
/// # Errors
///
/// Every violation of the append-only rule, as a `Problem` against
/// `catalog/errors.toml`. There are two:
///
/// * `code-removed` — a code the ledger reserved is no longer declared.
///   Reserving is append-only and every entry ever logged under a code still
///   refers to it, so it cannot be freed. Reported against the file rather than
///   a line, because the declaration that would carry the line is the thing
///   that is gone.
/// * `code-repointed` — a code the ledger reserved is still declared and means
///   something else. The recorded meaning is the message key, the description,
///   the severity, `retryable` and `params` together, so any one of those five
///   moving is this refusal, located at the code's own line in the file.
///
/// A catalog that only adds codes returns the grown ledger and no problem at
/// all.
///
/// **Both are named**, as [`read`]'s own `# Errors` names its five refusals.
/// The two refusals here want opposite things from the reader who hits one —
/// `code-removed` is a declaration to restore, `code-repointed` is a number to
/// leave where it points and a new one to take — and "every violation of the
/// append-only rule" tells them apart from nothing. `AGENTS.md` states
/// the rule: an `# Errors` section names the refusals the function actually
/// returns.
pub fn reconcile(
    catalog: &Catalog,
    reserved: &Reservations,
    errors_text: &str,
) -> Result<Reservations, Vec<Problem>> {
    let mut problems = Vec::new();
    let mut next = reserved.meanings.clone();

    // **The key *and* the description**, because a code's meaning is both. With
    // only the message key recorded, `NC-JRNL-0001` could be rewritten from
    // "the journal's hash chain is broken" to "the printer tray is empty" — and
    // both locale sentences with it — while keeping the key, with
    // `check passed`. `§15.1` says a code identifies one error for as long as
    // the logs live; that is what changing its description undoes.
    //
    // **And `retryable` and `params`, for the same reason.** Outside the
    // recorded meaning, `NC-DB-0002` — "a migration failed to apply",
    // `critical` — could go from `retryable = false` to `true` with
    // `cargo xtask check` green, its ledger line unchanged, `docs/errors.md`
    // republishing it as retryable and `ErrorCode::retryable()` returning `true`.
    // A scheduler keyed on that field then retries a failed migration for ever,
    // and logs from before and after the change are indistinguishable — which is
    // the one property this file exists to protect. `params` is the message's
    // parameter list: change it and the rendered sentence changes, which is what
    // a reader of the log sees.
    //
    // `raising` is deliberately *not* here. It says which crates may raise the
    // code, which is a scope rule the §15.1 walk already pins by name — it is a
    // statement about the tree rather than about what the error is, and putting
    // it here would make a change of scope read as a change of meaning.
    //
    // **And the severity, because that is what a code *does*.** `check_errors`
    // does not read `entry.severity` — `grep -n 'entry.severity'` in `load.rs`
    // is empty — so outside the recorded meaning `NC-JRNL-0002`, "an entry's
    // hash does not match the entry's own contents", could go from `critical`
    // to `info` with `cargo xtask check` green and `docs/errors.md`
    // republishing it as informational. Every consumer pages on
    // `>= Severity::Error`, so that one word decides whether journal tampering
    // wakes anybody, and a severity change repoints neither the message key nor
    // the description.
    //
    // The cost is that fixing a typo in a description takes the same four
    // deliberate steps as repointing a key. That is the right price: the
    // sentence is what a developer reads to decide what an error *is*, and a
    // mechanism that could tell a correction from a repointing would have to
    // understand English.
    let declared: Meanings = catalog
        .errors
        .iter()
        .map(|entry| {
            (
                entry.code.clone(),
                format!(
                    "{} — {} — retryable:{} — params:[{}] — {}",
                    entry.message,
                    crate::docs::severity_word(entry.severity),
                    entry.retryable,
                    entry.params.join(","),
                    entry.description
                ),
            )
        })
        .collect();

    for (code, meaning) in &reserved.meanings {
        match declared.get(code) {
            None => problems.push(Problem::in_file(
                "catalog/errors.toml",
                format!("{code} was reserved for \"{meaning}\" and is no longer declared"),
                "reserving a code is append-only. Every log entry ever written with this \
                 code still refers to it, so it cannot be freed — restore the \
                 declaration, and if the error itself is gone say so in its description \
                 rather than deleting the code. If instead this code was reserved by a \
                 build minutes ago and no released build ever carried it, undoing that \
                 is deliberate and takes three steps: delete its line from \
                 catalog/errors.reserved.toml, blank the `checksum` there, and run \
                 `cargo xtask build --reserve`",
                "code-removed",
            )),
            Some(current) if current != meaning => problems.push(Problem::at(
                "catalog/errors.toml",
                line_of(errors_text, &format!("code = \"{code}\""), 1).unwrap_or(1),
                format!("{code} was reserved for \"{meaning}\" and now means \"{current}\""),
                "a code identifies one error for as long as the logs live. Give the new \
                 error the next unused number in its area and leave this one pointing \
                 where it always pointed. If instead the key was misspelled and no \
                 released build ever used it, rewriting history is deliberate and takes \
                 all four steps: rename it in errors.toml and both locales, delete this \
                 code's line from catalog/errors.reserved.toml, blank the `checksum` \
                 there, and run `cargo xtask build --reserve`. Blanking the checksum \
                 alone will not do it — that forgives a ledger nothing vouches for, not \
                 a code that changed meaning",
                "code-repointed",
            )),
            Some(_) => {}
        }
    }

    for (code, meaning) in declared {
        next.entry(code).or_insert(meaning);
    }

    if problems.is_empty() {
        Ok(Reservations { meanings: next })
    } else {
        Err(problems)
    }
}

/// The checksum over a set of reservations, and over how many of them there are.
///
/// Over the canonical rendering rather than over the file, so that a comment
/// change, a reordering or a trailing newline does not read as tampering.
///
/// **The count is hashed with them because a set cannot say it did not shrink.**
/// Over the reservations alone, every subset of a ledger would itself be a
/// valid ledger — the subset hole in the module header — so a file that lost lines has
/// to lie about how many it has as well as about which ones, and the two lies
/// are one hash apart.
fn checksum(reserved: &Meanings, count: usize) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(CHECKSUM.len() as u64).to_le_bytes());
    hasher.update(CHECKSUM.as_bytes());
    hasher.update(&(count as u64).to_le_bytes());
    for (code, meaning) in reserved {
        hasher.update(&(code.len() as u64).to_le_bytes());
        hasher.update(code.as_bytes());
        hasher.update(&(meaning.len() as u64).to_le_bytes());
        hasher.update(meaning.as_bytes());
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest.as_bytes() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Renders the ledger.
#[must_use]
pub fn render(reserved: &Reservations) -> String {
    let mut out = String::from(
        "# The error codes this repository has used, and what each was reserved to\n\
         # mean. Append-only: the compiler refuses to repoint or remove a line, because\n\
         # every log entry ever written with a code still refers to it (NOCTUA.md\n\
         # §15.1).\n\
         #\n\
         # `cargo xtask build` appends to this file. It is NOT generated output and is\n\
         # not compared against a regeneration — an earlier version was, and a hand edit\n\
         # ratified itself. The checksum below is over the reservations and over how many\n\
         # there are, and it is what catches an edit made outside the compiler.\n\
         #\n\
         # A ledger only ever grows, which is why the count is inside the checksum: a\n\
         # file holding fewer lines than it says it holds lost some, and every code they\n\
         # reserved would otherwise be re-derived with whatever the registry means by it\n\
         # today.\n\
         #\n\
         # Each line records the message key **and** the developer description, because\n\
         # a code's meaning is both. Recording only the key let a code be rewritten from\n\
         # one error to an unrelated one — key intact, description and both locale\n\
         # sentences replaced — with the gate green.\n\
         \n[meta]\n",
    );
    let held = reserved.meanings.len();
    let _ = writeln!(out, "checksum = \"{}\"", checksum(&reserved.meanings, held));
    let _ = writeln!(out, "count = {held}");
    out.push_str("\n[reserved]\n");
    for (code, meaning) in &reserved.meanings {
        let _ = writeln!(out, "{} = {}", as_toml(code), as_toml(meaning));
    }
    out
}

/// The ledger's escaper, so the invisible-character sweep can reach it.
///
/// **Its eight-digit arm is the one to prove reachable.** `\UXXXXXXXX` is
/// written for a character above the basic plane, and a guard set —
/// `is_control() || is_a_direction_mark(…)` — that tops out at `U+FEFF` leaves
/// no input anywhere able to take it. The set reaches the tag block, and this
/// door is how the sweep in `the_gates_fire.rs` proves it.
#[doc(hidden)]
#[must_use]
pub fn as_toml_for_tests(text: &str) -> String {
    as_toml(text)
}

/// A TOML basic string.
///
/// **Not `crate::quoted`, which writes a *Rust* string.** Everything else the
/// compiler emits is Rust or Markdown, and the one escaper in `lib.rs` writes
/// Rust — and the two languages spell an escaped control character
/// differently. Rust writes `\u{7}`; TOML wants four hex digits and no braces.
/// Through that escaper, a BEL in an error's description renders a ledger that
/// TOML cannot read, and rendering it is the write that cannot be taken back:
///
/// ```text
/// "NC-DB-0001" = "… — a bell\u{7}"
///   toml::from_str  ->  too few unicode value digits, expected unicode hexadecimal value
///   cargo xtask check  ->  catalog/errors.reserved.toml is not a ledger
/// ```
///
/// Every subsequent build and check then refuses, on a file `NOCTUA.md §5.3`
/// says there is nothing to restore from — and the fix is a hand edit of the
/// one file the whole append-only rule rests on.
///
/// Control characters are escaped rather than refused, for the reason
/// `crate::quoted` gives: the input is prose somebody writes freely. The
/// direction marks go with them, because a right-to-left override inside a
/// reservation reorders what a reader sees a code was reserved *for*.
fn as_toml(text: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            // `\uXXXX` for the basic plane and `\UXXXXXXXX` above it, which is
            // what the grammar allows — a `\u` with more than four digits is the
            // same parse error as one with fewer.
            other if crate::shows_nothing(other) => {
                let value = other as u32;
                if value <= 0xFFFF {
                    let _ = write!(out, "\\u{value:04X}");
                } else {
                    let _ = write!(out, "\\U{value:08X}");
                }
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::{Meanings, Reservations, read, reconcile, render};
    use crate::model::{Argon2Id, Catalog, ErrorEntry, Hashing, Limits, Severity};

    fn catalog(codes: &[(&str, &str)]) -> Catalog {
        Catalog {
            areas: std::collections::BTreeMap::new(),
            errors: codes
                .iter()
                .enumerate()
                .map(|(at, (code, message))| ErrorEntry {
                    code: (*code).to_owned(),
                    description: "whatever".to_owned(),
                    message: (*message).to_owned(),
                    // **Three fields that vary across the tests.** A recorded
                    // meaning is its key, its severity, its retryability, its
                    // parameters and its description, and a fixture giving every
                    // code `Error`, `false` and `[]` leaves no test in this
                    // module able to tell reading those three from ignoring
                    // them: each neutralised in turn, the suite this file's
                    // mutation operators run stays green.
                    //
                    // They vary by position, so the premise is established
                    // by the fixture rather than borrowed from whatever
                    // `catalog/errors.toml` happens to hold today.
                    severity: [Severity::Error, Severity::Critical, Severity::Warning][at % 3],
                    retryable: at % 2 == 1,
                    // **Two, not one.** A one-element `join` is the element, so
                    // with one parameter `entry.params.join(",")` replaced by
                    // `entry.params.first().cloned().unwrap_or_default()`
                    // would survive the module's own suite and be killed only
                    // by the shipped catalog happening to hold one code with
                    // three parameters. That is the premise borrowed from a
                    // file this test does not own.
                    params: if at % 3 == 2 {
                        vec!["one".to_owned(), "two".to_owned()]
                    } else {
                        Vec::new()
                    },
                    raising: crate::model::Raising::Raised,
                })
                .collect(),
            notifications: Vec::new(),
            emails: Vec::new(),
            limits: Limits {
                tables: std::collections::BTreeMap::new(),
                stores: std::collections::BTreeMap::new(),
                argon2id: Hashing {
                    password: Argon2Id {
                        memory_kib: 19_456,
                        iterations: 2,
                        parallelism: 1,
                        output_length: 32,
                        salt_length: 16,
                    },
                    device_pin: Argon2Id {
                        memory_kib: 65_536,
                        iterations: 3,
                        parallelism: 1,
                        output_length: 32,
                        salt_length: 16,
                    },
                },
                rate_limits: Vec::new(),
                kernel: crate::model::KernelLimits::default(),
            },
            commands: Vec::new(),
            events: Vec::new(),
            capabilities: Vec::new(),
            ungated: Vec::new(),
            locales: Vec::new(),
        }
    }

    /// The ledger's own spelling of a meaning: the message key and the developer
    /// description, joined the way [`reconcile`] joins them.
    ///
    /// Written here rather than shared with the code under test, so a change to
    /// the join is a failure rather than an agreement.
    fn reserved(entries: &[(&str, &str)]) -> Reservations {
        let meanings = entries
            .iter()
            .enumerate()
            // The five fields the fixture's `catalog` gives each code, because a
            // code's recorded meaning is all five — a fixture that writes some
            // of them reads as a repointing of every line. Spelled out here
            // rather than shared with `reconcile`, so a change to the join is a
            // failure rather than an agreement; derived from the same position,
            // so the two sides vary together.
            .map(|(at, (code, meaning))| {
                let severity = ["error", "critical", "warning"][at % 3];
                let retryable = at % 2 == 1;
                let params = if at % 3 == 2 { "one,two" } else { "" };
                (
                    (*code).to_owned(),
                    format!(
                        "{meaning} — {severity} — retryable:{retryable} — params:[{params}] — whatever"
                    ),
                )
            })
            .collect();
        Reservations { meanings }
    }

    fn read_refusing(root: &std::path::Path) -> Result<Reservations, crate::Problem> {
        read(root, super::Bootstrap::Refuse)
    }

    /// A directory holding a ledger of this test's own.
    fn scratch(name: &str, contents: &str) -> std::path::PathBuf {
        let mine = std::process::id(); // allow-nondeterminism: `process::id` names a scratch directory, which reaches no output
        let here = std::env::temp_dir(); // allow-nondeterminism: `env::temp_dir` names a scratch directory, in no output
        let root = here.join(format!("nc-ledger-{mine}-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("catalog")).expect("writable");
        if !contents.is_empty() {
            std::fs::write(root.join(super::PATH), contents).expect("writable");
        }
        root
    }

    #[test]
    fn a_new_code_is_appended() {
        // **Three codes, because two cannot reach the third position.** The
        // fixture varies severity, retryability and parameters by position, and
        // with at most two codes `at % 3 == 2` is unreachable and *every* entry
        // in *every* test has `params: []` and severity `Error` or `Critical`:
        // neutralising `entry.params.join(",")` in `reconcile` then leaves this
        // suite green.
        let next = reconcile(
            &catalog(&[
                ("NC-DB-0001", "error.db.one"),
                ("NC-DB-0002", "error.db.two"),
                ("NC-DB-0003", "error.db.three"),
            ]),
            &reserved(&[("NC-DB-0001", "error.db.one")]),
            "",
        )
        .expect("appending is allowed");
        assert_eq!(next.meanings.len(), 3);
        // Spelled out rather than taken from `reserved`, so the join is asserted
        // rather than agreed with. `NC-DB-0002` is the fixture's second code, and
        // the fixture varies all three of severity, retryability and parameters
        // by position — which is what lets a test here tell a meaning that is
        // read from one that is ignored.
        assert_eq!(
            next.meanings["NC-DB-0002"],
            "error.db.two — critical — retryable:true — params:[] — whatever"
        );
        // And the third, which is the only position that carries parameters and
        // the only one that reaches the third severity.
        assert_eq!(
            next.meanings["NC-DB-0003"],
            "error.db.three — warning — retryable:false — params:[one,two] — whatever"
        );
    }

    #[test]
    fn a_repointed_code_is_refused_and_the_message_names_both_meanings() {
        let problems = reconcile(
            &catalog(&[("NC-DB-0001", "error.db.something-else")]),
            &reserved(&[("NC-DB-0001", "error.db.one")]),
            "",
        )
        .expect_err("repointing is the thing this exists to catch");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].check, "code-repointed");
        assert!(problems[0].what.contains("error.db.one"));
        assert!(problems[0].what.contains("error.db.something-else"));
    }

    #[test]
    fn a_code_whose_description_changed_is_a_repointed_code() {
        // **The half a key-only ledger does not hold.** Recording only the
        // message key, it lets `NC-JRNL-0001` be rewritten from "the journal's
        // hash chain is broken" to "the printer tray is empty" — key intact,
        // description and both locale sentences replaced — with `check passed`.
        // The key is the code's name; the description is what it means.
        let mut moved = catalog(&[("NC-DB-0001", "error.db.one")]);
        moved.errors[0].description = "something else entirely".to_owned();
        let problems = reconcile(&moved, &reserved(&[("NC-DB-0001", "error.db.one")]), "")
            .expect_err("a code that means something else is a repointed code");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].check, "code-repointed");
        assert!(
            problems[0].what.contains("whatever"),
            "{}",
            problems[0].what
        );
        assert!(
            problems[0].what.contains("something else entirely"),
            "{}",
            problems[0].what
        );
    }

    /// A removed code is refused, and the refusal says what it was reserved for.
    ///
    /// **Half of criterion 7's requirement is held here.** The criterion asks
    /// that the error name *what the code was reserved for*, and the other
    /// reader of this arm,
    /// `crates/nc-catalog-compiler/tests/the_gates_fire.rs`'s
    /// `must_fail("code-removed", "NC-DB-0003")`, asserts the check name
    /// and the code. Measured without this test: `format!("{code} was reserved for \"{meaning}\" and
    /// is no longer declared")` rewritten to `format!("{code} is no longer
    /// declared")` leaves every suite green — a reader told a code they have
    /// never seen is missing, and not what it used to mean, which is the one
    /// thing that tells them whether restoring it is right.
    ///
    /// The `code-repointed` half has two tests naming both meanings. This is
    /// the same assertion for the other arm, and it is beside them.
    #[test]
    fn a_removed_code_is_refused_and_the_message_names_what_it_meant() {
        let problems = reconcile(
            &catalog(&[]),
            &reserved(&[("NC-DB-0001", "error.db.one")]),
            "",
        )
        .expect_err("a code cannot be freed");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].check, "code-removed");
        assert!(
            problems[0].what.contains("NC-DB-0001"),
            "the refusal does not name the code: {}",
            problems[0].what
        );
        // The recorded meaning, both halves of it: the message key the ledger
        // reserved and the description it reserved with it. A code is
        // append-only, so what a reader needs is what it used to mean.
        for held in ["error.db.one", "whatever"] {
            assert!(
                problems[0].what.contains(held),
                "the refusal does not say what `NC-DB-0001` was reserved for, which is \
                 the half of §14's criterion 7 that says the error names it: {}",
                problems[0].what
            );
        }
    }

    #[test]
    fn an_unchanged_catalog_reconciles_to_the_same_ledger() {
        let same = reserved(&[("NC-DB-0001", "error.db.one")]);
        let next = reconcile(&catalog(&[("NC-DB-0001", "error.db.one")]), &same, "")
            .expect("nothing changed");
        assert_eq!(next, same);
    }

    #[test]
    fn a_rendered_ledger_reads_back_as_itself() {
        let entries = reserved(&[
            ("NC-DB-0002", "error.db.two"),
            ("NC-DB-0001", "error.db.one"),
        ]);
        let text = render(&entries);
        // The size it states is the size it has. A rendered ledger that said
        // anything else would be one this module's own `read` refuses, and
        // `render` writing a file that cannot be read back is the write
        // `NOCTUA.md §5.3` says there is nothing to restore from.
        assert!(text.contains("\ncount = 2\n"), "{text}");
        let root = scratch("round-trip", &text);
        assert_eq!(read_refusing(&root), Ok(entries));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_absent_ledger_is_a_failure_rather_than_an_empty_one() {
        // Without this refusal, `rm catalog/errors.reserved.toml` frees every
        // code in the repository, and the next build blesses the renumbering.
        let root = scratch("absent", "");
        let problem = read_refusing(&root).expect_err("an absent ledger is not an empty one");
        assert_eq!(problem.check, "ledger-missing");
        assert!(problem.fix.contains("restore"), "{problem:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hand_edited_ledger_does_not_match_its_checksum() {
        // The edit a drift check would ratify: one comparing the ledger against
        // output derived from the ledger.
        let entries = reserved(&[
            ("NC-DB-0001", "error.db.one"),
            ("NC-DB-0002", "error.db.two"),
        ]);
        // Built from the map rather than written out, so the meaning's shape
        // lives in one place: `reserved` joins the key and the description, and
        // a change there must not quietly make this a no-op replace.
        let dropped = format!(
            "{} = {}\n",
            crate::quoted("NC-DB-0002"),
            crate::quoted(&entries.meanings["NC-DB-0002"])
        );
        let rendered = render(&entries);
        assert!(rendered.contains(&dropped), "the line to drop is not there");
        let edited = rendered.replace(&dropped, "");
        let root = scratch("edited", &edited);
        let problem = read_refusing(&root).expect_err("a deleted line frees a code");
        assert_eq!(problem.check, "ledger-checksum");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A vouched-for ledger that states no count is refused.
    ///
    /// **`count` is defaulted so the bootstrap file may omit it, and the default
    /// is zero** — so a forger who deletes the line and re-signs with the count
    /// it now implies gets a self-consistent file whose size guard reads
    /// `1 < 0` and passes, which is the whole hole one special case wider.
    /// The bootstrap is a blank checksum and has already returned before this.
    #[test]
    fn a_ledger_that_states_no_count_is_refused() {
        let mut one = Meanings::new();
        one.insert("NC-POL-0002".to_owned(), "a meaning".to_owned());
        // Signed with the count the file would then imply, which is what makes
        // the forged file self-consistent.
        let signed = super::checksum(&one, 0);
        let root = scratch(
            "no-count",
            &format!(
                "[meta]\nchecksum = \"{signed}\"\n\n[reserved]\n\"NC-POL-0002\" = \"a meaning\"\n"
            ),
        );
        let problem = read_refusing(&root).expect_err("a ledger that states no count");
        assert_eq!(
            problem.check, "ledger-count",
            "a ledger with no count line was accepted, so deleting that line and \
             re-signing frees every code it stopped naming"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A ledger that reserves nothing is refused even with a valid checksum.
    ///
    /// **The empty set has a correct checksum anybody can compute**, so the two
    /// refusals above — both of which read the *checksum* — let it through:
    /// `reconcile`'s loop cannot iterate and every code is re-derived as new.
    /// Measured on the real tree without the refusal: `NC-CANON-0001`'s
    /// description repointed, the ledger replaced by
    /// `[meta] checksum = "<the empty set's own>"` and an empty `[reserved]`,
    /// then an ordinary `cargo xtask build` — no `--reserve`, no warning line —
    /// prints `29 code(s) appended to catalog/errors.reserved.toml, which is permanent`,
    /// and `cargo xtask check` passes with the ledger recording the new meaning
    /// as though it always had. Acceptance criterion 7, turned off by the
    /// edit-build-check loop a developer runs every day.
    ///
    /// The checksum is computed here rather than pasted, because a pasted one
    /// makes a test stop a branch early: a *wrong* checksum never reaches the
    /// state this is about.
    #[test]
    fn a_ledger_that_reserves_nothing_is_refused_however_it_is_vouched_for() {
        let empty = super::checksum(&Meanings::new(), 0);
        let root = scratch(
            "vouched-empty",
            &format!("[meta]\nchecksum = \"{empty}\"\ncount = 0\n\n[reserved]\n"),
        );
        let problem = read_refusing(&root).expect_err("a ledger that reserves nothing");
        assert_eq!(
            problem.check, "ledger-unvouched",
            "an empty ledger with its own valid checksum was accepted, so every error \
             code is free to be re-derived with whatever meaning `catalog/errors.toml` \
             carries today"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_truncated_ledger_does_not_pass_as_an_empty_one() {
        let root = scratch(
            "truncated",
            "[meta]\nchecksum = \"deadbeef\"\n\n[reserved]\n",
        );
        let problem = read_refusing(&root).expect_err("an emptied ledger is not an empty one");
        assert_eq!(problem.check, "ledger-checksum");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_checksum_is_refused_unless_the_caller_asked_to_bootstrap() {
        // The door a blank checksum would open: accepted by everything, it lets
        // two lines of editing free any code with `check` still reporting
        // `check passed`.
        let root = scratch("bootstrap", "[meta]\nchecksum = \"\"\n\n[reserved]\n");
        let problem = read_refusing(&root).expect_err("nothing vouches for it");
        assert_eq!(problem.check, "ledger-unvouched");
        assert!(problem.fix.contains("--reserve"), "{problem:?}");
        assert_eq!(
            read(&root, super::Bootstrap::Allow),
            Ok(Reservations {
                meanings: Meanings::new()
            }),
            "the one intent that accepts it must still work"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_cleared_checksum_over_a_populated_ledger_is_refused_too() {
        // The abuse, exactly: keep the reservations, drop the checksum, repoint a
        // code. Without this it is the same hole one file over.
        let entries = reserved(&[("NC-DB-0001", "error.db.one")]);
        let cleared = render(&entries)
            .lines()
            .map(|line| {
                if line.starts_with("checksum") {
                    "checksum = \"\"".to_owned()
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let root = scratch("cleared", &cleared);
        assert_eq!(
            read_refusing(&root)
                .expect_err("nothing vouches for it")
                .check,
            "ledger-unvouched"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_checksum_covers_the_meaning_and_not_only_the_code() {
        // Repointing a code by hand changes no line count and no key.
        let entries = reserved(&[("NC-DB-0001", "error.db.one")]);
        let repointed = render(&entries).replace("error.db.one", "error.db.other");
        let root = scratch("repointed-by-hand", &repointed);
        let problem = read_refusing(&root).expect_err("the meaning moved");
        assert_eq!(problem.check, "ledger-checksum");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_rendered_ledger_is_ordered_so_a_build_does_not_rewrite_it() {
        let text = render(&reserved(&[("NC-DB-0002", "b"), ("NC-DB-0001", "a")]));
        let one = text.find("NC-DB-0001").expect("declared");
        let two = text.find("NC-DB-0002").expect("declared");
        assert!(
            one < two,
            "the ledger has to be ordered or every build rewrites it"
        );
    }

    /// What `render` writes, `read` reads — whatever the description holds.
    ///
    /// **Not the escaper from `lib.rs`, which writes Rust.** Rust spells
    /// an escaped control character `\u{7}`; TOML wants `\u0007` — four hex
    /// digits and no braces. So a BEL in an error's description — prose a
    /// developer writes freely, refused by nothing on the way in — rendered a
    /// ledger that TOML cannot parse, and rendering it is the write `NOCTUA.md
    /// §5.3` says there is nothing to restore from. Measured through that escaper:
    ///
    /// ```text
    /// "NC-DB-0001" = "... a bell\u{7}"
    ///   toml::from_str              ->  too few unicode value digits
    ///   every later build and check ->  catalog/errors.reserved.toml is not a ledger
    /// ```
    ///
    /// Driven through `render` and `read` rather than asserting the escape,
    /// because the hazard is one language's spelling used for another's, and
    /// only the round trip can tell.
    #[test]
    fn a_reservation_survives_whatever_a_description_holds() {
        // Each of these reaches a different arm: the spelling Rust and TOML
        // disagree on, the two escapes TOML has of its own (`\b`, `\f`), the
        // quote and the backslash, the bidi override that reorders what a reader
        // sees a code was reserved for, and an astral character, which is where
        // four hex digits are not enough.
        let awkward = "a bell\u{7}, a NUL\u{0}, a backspace\u{8}, a form feed\u{c}, \
                       a quote \", a backslash \\, an override \u{202e}, a \u{1d11e} and \u{1f702}";
        let mut meanings = Meanings::new();
        meanings.insert(
            "NC-DB-0001".to_owned(),
            format!("error.db.one — error — retryable:false — params:[] — {awkward}"),
        );
        let reserved = Reservations { meanings };
        let root = scratch("awkward", &render(&reserved));
        let read_back = read_refusing(&root).expect("a rendered ledger is a readable one");
        assert_eq!(
            read_back, reserved,
            "the ledger did not survive its own rendering. Every code in it is permanent \
             and there is no history to restore the file from, so a reservation that \
             cannot be read back is a repository that cannot build"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A ledger that lost lines is refused, though its checksum verifies.
    ///
    /// **A subset of a ledger is not a ledger.** With the checksum over the
    /// reservations alone, the file left behind by keeping one of them and
    /// re-signing it with this module's own `render` verifies: `reconcile` then
    /// finds twenty-eight codes it has never heard of, appends them, and the
    /// meanings it appends are whatever `catalog/errors.toml` says today.
    /// Measured on the real tree with `NC-CANON-0001`'s description changed to
    /// "The printer tray is empty" — `compile` returns `Ok`, the ledger it
    /// produces holds twenty-nine reservations again and records the new
    /// meaning, and no line anywhere says so.
    ///
    /// The checksum here is computed with [`super::checksum`] rather than
    /// pasted, because a pasted one lets a test stop a branch early: a *wrong*
    /// checksum proves `ledger-checksum` fires and never reaches the state the
    /// test is written for.
    #[test]
    fn a_ledger_that_lost_lines_is_refused_though_its_checksum_verifies() {
        let entries = reserved(&[
            ("NC-DB-0001", "error.db.one"),
            ("NC-DB-0002", "error.db.two"),
            ("NC-DB-0003", "error.db.three"),
        ]);
        let kept = "NC-DB-0002";
        let mut left = Meanings::new();
        left.insert(kept.to_owned(), entries.meanings[kept].clone());
        // Two of the three lines gone, the count they were counted in still
        // there, and a checksum that verifies over what is left.
        let text = format!(
            "[meta]\nchecksum = \"{}\"\ncount = {}\n\n[reserved]\n{} = {}\n",
            super::checksum(&left, entries.meanings.len()),
            entries.meanings.len(),
            super::as_toml(kept),
            super::as_toml(&left[kept]),
        );
        let root = scratch("lost-lines", &text);
        let problem = read_refusing(&root).expect_err("a ledger that lost lines");
        assert_eq!(
            problem.check, "ledger-count",
            "a ledger holding one of the three codes it says it has reserved was \
             accepted, so the other two are free to be re-derived with whatever meaning \
             `catalog/errors.toml` carries today"
        );
        assert!(
            problem.what.contains('3') && problem.what.contains('1'),
            "the refusal does not say how many it says it holds and how many it does: {}",
            problem.what
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// One line short is short, and the guard is an exactness rather than a
    /// tolerance.
    ///
    /// **The test above drops two of three, so a shortfall of one is invisible
    /// to it.** Measured without this: `reserved.len() < meta.count` given a
    /// tolerance of one — `reserved.len() + 1 < meta.count` — leaves the rest
    /// of this crate's suites green and `cargo xtask check` at `check passed`.
    /// What that buys a forger is one code: delete a reservation, write `count`
    /// as it was, re-derive the checksum over both — the two are hashed
    /// together — and the file passes `states_a_count`, the checksum, this
    /// guard and the non-empty guard. `reconcile` then never sees that code and
    /// re-derives it with whatever `catalog/errors.toml` means today, which is
    /// criterion 7 off for exactly one code.
    ///
    /// The journal holds the same shape in the same words — *one row short as
    /// well as five*.
    #[test]
    fn a_ledger_one_line_short_is_refused() {
        use core::fmt::Write as _;

        let entries = reserved(&[
            ("NC-DB-0001", "error.db.one"),
            ("NC-DB-0002", "error.db.two"),
            ("NC-DB-0003", "error.db.three"),
        ]);
        let mut left = Meanings::new();
        for kept in ["NC-DB-0001", "NC-DB-0002"] {
            left.insert(kept.to_owned(), entries.meanings[kept].clone());
        }
        let mut lines = String::new();
        for (code, meaning) in &left {
            let _ = writeln!(
                lines,
                "{} = {}",
                super::as_toml(code),
                super::as_toml(meaning)
            );
        }
        // Three counted, two written, and a checksum that verifies over the two.
        let text = format!(
            "[meta]\nchecksum = \"{}\"\ncount = {}\n\n[reserved]\n{lines}",
            super::checksum(&left, entries.meanings.len()),
            entries.meanings.len(),
        );
        let root = scratch("one-short", &text);
        let problem = read_refusing(&root).expect_err("a ledger one line short");
        assert_eq!(
            problem.check, "ledger-count",
            "a ledger holding two of the three codes it says it reserved was accepted, so \
             the third is free to be re-derived with whatever meaning the catalog carries \
             today"
        );
        assert!(
            problem.what.contains('3') && problem.what.contains('2'),
            "the refusal does not say how many it claims and how many it holds: {}",
            problem.what
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The checksum is compared to its last character.
    ///
    /// **Every other test of it plants a digest that differs in its first few
    /// characters.** So without this nothing says the comparison reaches past
    /// them — measured, narrowing it to a shared four-character prefix leaves
    /// the rest of this crate's suites green and `cargo xtask check` at
    /// `check passed`. Under that narrowing a hand-editor needs a digest
    /// agreeing on four hex characters, which is
    /// sixteen bits of work, and this module's own header calls the checksum the
    /// thing that catches an edit made outside the compiler.
    ///
    /// Written as a sweep rather than as one more differing digest, because one
    /// more differing digest leaves the hole: the widest shared prefix
    /// tried has to reach the digest's last character, which is what says the
    /// comparison is over the whole of it. `nc-journal` holds its own hash the
    /// same way, for the same reason.
    #[test]
    fn a_checksum_that_differs_only_in_its_last_character_is_a_break() {
        use core::fmt::Write as _;

        let entries = reserved(&[("NC-DB-0001", "error.db.one")]);
        let real = super::checksum(&entries.meanings, entries.meanings.len());
        let mut lines = String::new();
        for (code, meaning) in &entries.meanings {
            let _ = writeln!(
                lines,
                "{} = {}",
                super::as_toml(code),
                super::as_toml(meaning)
            );
        }
        let mut tried = Vec::new();
        for shared in [1_usize, 2, 4, 8, 16, 32, real.len() - 1] {
            let last = real
                .chars()
                .nth(shared)
                .expect("the digest is longer than the widest prefix tried");
            let swapped = if last == '0' { '1' } else { '0' };
            let forged: String = real
                .chars()
                .enumerate()
                .map(|(at, character)| if at == shared { swapped } else { character })
                .collect();
            let text = format!(
                "[meta]\nchecksum = \"{forged}\"\ncount = {}\n\n[reserved]\n{lines}",
                entries.meanings.len(),
            );
            let root = scratch(&format!("digest-{shared}"), &text);
            let problem = read_refusing(&root)
                .err()
                .unwrap_or_else(|| panic!("a digest differing at {shared} was accepted"));
            assert_eq!(
                problem.check, "ledger-checksum",
                "a ledger whose checksum differs only at character {shared} was accepted, so \
                 the comparison stops before it"
            );
            tried.push(shared);
            let _ = std::fs::remove_dir_all(&root);
        }
        assert_eq!(
            tried.last().copied(),
            Some(real.len() - 1),
            "the widest prefix tried has to reach the digest's last character, or this \
             sweep says nothing about the end of it"
        );
    }

    /// A count in prose is not a count the ledger states.
    ///
    /// The `ledger-count` guard asks whether a vouched ledger says how many
    /// codes it holds. Asked of the file's bytes, as `text.contains("count = ")`,
    /// any comment satisfies it, and so does any `[reserved]` description
    /// holding those six characters — so a forger who deletes the
    /// line, re-signs over the smaller set with the count the file now implies
    /// (zero, because the field is defaulted), and leaves one comment behind
    /// gets a self-consistent ledger the size guard reads as `28 < 0`.
    ///
    /// Asked of the parsed document instead, prose cannot make a key. This
    /// drives the clause directly, because reaching it through `read` needs a
    /// checksum forged over the very file the test is constructing.
    #[test]
    fn a_count_written_in_prose_is_not_a_count() {
        let states_a_count = super::states_a_count;
        assert!(states_a_count("[meta]\ncount = 29\n"));
        assert!(
            !states_a_count("# the meta table carries count = <how many>\n[meta]\n"),
            "a comment holding `count = ` satisfied the guard, which is what it did before"
        );
        assert!(
            !states_a_count("[meta]\nchecksum = \"ab\"\n\n[reserved]\nA = \"count = 3\"\n"),
            "a reservation whose description holds `count = ` satisfied the guard"
        );
        assert!(!states_a_count("[meta]\nchecksum = \"ab\"\n"));
    }

    /// The remedy `reconcile` prints still works: the line out, the checksum
    /// blanked, `--reserve`.
    ///
    /// **The count must not reach it.** That remedy leaves a file whose `count`
    /// says two while its lines are one — exactly the shape the refusal above
    /// exists for — and refusing it here would close the only documented way
    /// back from a code repointed before any build released it. A blank checksum
    /// vouches for nothing in the file, its count included.
    #[test]
    fn the_remedy_for_a_repointed_code_still_works_with_a_count_in_the_file() {
        let entries = reserved(&[
            ("NC-DB-0001", "error.db.one"),
            ("NC-DB-0002", "error.db.two"),
        ]);
        let remedied = render(&entries)
            .lines()
            .filter(|line| !line.starts_with("\"NC-DB-0002\""))
            .map(|line| {
                if line.starts_with("checksum = ") {
                    "checksum = \"\"".to_owned()
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let root = scratch("remedy", &remedied);
        assert_eq!(
            read(&root, super::Bootstrap::Allow),
            Ok(reserved(&[("NC-DB-0001", "error.db.one")])),
            "the remedy the `code-repointed` diagnostic prescribes has to leave a \
             catalog that builds"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
