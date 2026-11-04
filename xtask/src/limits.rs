//! The gate that makes record limits a policy instead of a habit.
//!
//! `NOCTUA.md §15.8`: **every table that can grow without bound declares
//! `max_rows` and `prune_batch`. A new unbounded table without an entry fails
//! the build — that gate is what makes this a policy rather than a habit.**
//!
//! It reads the migrations rather than the database, deliberately. A gate that
//! needed a live PostgreSQL would be one CI skipped, and the moment somebody
//! adds a table is the moment the migration is written — not the moment it is
//! applied.
//!
//! **A table may say it cannot grow**, with a marker and a reason in the comment
//! block directly above its `CREATE TABLE`. That is a declaration, not a
//! silence: a single-row configuration table is genuinely bounded, and a gate
//! with no way to say so is a gate people route around by putting the table
//! somewhere else.
//!
//! # Why this reads statements and not lines
//!
//! Matched line by line, **four ordinary shapes of valid PostgreSQL walk
//! straight through acceptance criterion 5**:
//!
//! ```sql
//! CREATE TABLE
//!     login_attempts (id TEXT PRIMARY KEY);           -- the name is on the next line
//!
//! CREATE TABLE a (id INT); CREATE TABLE login_attempts (id INT);  -- the second one
//!
//! /* CREATE TABLE access_records ( … ); */            -- commented out, still "created"
//!
//! /* a prose block mentioning CREATE TABLE widgets */ -- a table nobody wrote, named
//! ```
//!
//! Read that way, every one of them leaves `cargo xtask check` green while
//! creating an unbounded table, or names a table that does not exist. A line is
//! not the unit PostgreSQL reads: `without_comments` removes both comment forms
//! over the whole file, and the text is then split into statements on `;`.

use std::path::Path;

use crate::gates::{Finding, walk};
use crate::relative_to;

/// Where the migrations live.
pub(crate) const MIGRATIONS: &str = "migrations";

/// Where the local database's migrations live: SQLite, never mixed with
/// PostgreSQL's.
pub(crate) const LOCAL_MIGRATIONS: &str = "local-migrations";

/// Which SQL a directory of migrations is written in.
///
/// **One tokeniser, and one rule that differs.** Both dialects quote, comment
/// and name tables the same way for everything this gate reads; what SQLite
/// adds is a trigger whose body is statements of its own — `CREATE TRIGGER …
/// BEGIN …; …; END;` — so a `;` inside the body does not end the statement, and
/// cutting there would read `END` as a statement and the trigger as half of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// `migrations/`, which the server's PostgreSQL applies.
    PostgreSql,
    /// `local-migrations/`, which the SQLite file on the user's machine applies.
    Sqlite,
}

/// Each directory of migrations, the database it builds, and its SQL.
///
/// **The gate reads both**, and holds every table to an entry of its own
/// database: a table `local-migrations/` creates is bounded only by an entry
/// saying `database = "local"`, and one `migrations/` creates only by an entry
/// that does not.
pub(crate) const DIRECTORIES: [(&str, nc_catalog::Database, Dialect); 2] = [
    (
        MIGRATIONS,
        nc_catalog::Database::Server,
        Dialect::PostgreSql,
    ),
    (
        LOCAL_MIGRATIONS,
        nc_catalog::Database::LocalFile,
        Dialect::Sqlite,
    ),
];

/// The marker that says a table is bounded by its own shape.
///
/// It takes a reason, and the reason is never parsed — its job is to put the
/// exception in front of whoever reads the migration next.
pub(crate) const BOUNDED: &str = "nc:bounded:";

/// The `set_config` values this gate cannot evaluate, cleared by hand.
///
/// **Because the question is not decidable, and a rule pretending otherwise is
/// wrong.** Whether a value preserves the first schema on the path depends on
/// what SQL *computes*. `0011` writes
/// `regexp_replace(current_setting('search_path'), ', pg_temp$', '')`, which
/// removes a trailing element and keeps the first; `substr(current_setting(
/// 'search_path'), 1, 0) || 'archive'` names the path, discards it and prepends
/// a constant. The two differ by an *argument*, not by a function, so no list of
/// functions can tell them apart — whitelisting `regexp_replace` would bless a
/// pattern anchored at the start just as readily.
///
/// **And the reason cannot live beside the statement.** A migration's text is
/// immutable once applied: written as a comment above this one, `cargo xtask
/// check` answers `check failed: an applied migration was edited`. So it lives
/// here, keyed on the statement's own words — an edit to those words stops
/// matching and the gate reports again, which is the loud failure a written list
/// is for.
///
/// The one entry, and why it is safe: the pattern is anchored at the end with
/// `$`, so it removes the `, pg_temp` that `0011` itself appended and nothing
/// else. The first schema on the path is whatever it already was. An anchor at
/// the *start* would gut the path with the same function, which is why this
/// names the anchor rather than the function.
pub(crate) const CLEARED_BY_HAND: [&str; 1] = [concat!(
    "SELECT set_config('search_path', ",
    "regexp_replace(current_setting('search_path'), ', pg_temp$', ''), false);"
)];

/// Where a `CREATE FUNCTION`'s body opens, or the start of the statement.
///
/// PostgreSQL writes a body after `AS`, as `$tag$…$tag$` or as `'…'`. Anything
/// quoted before that belongs to the signature — a parameter's `DEFAULT`, most
/// often — and reading it as the body's opener brackets the declaration in the
/// wrong place. Returns zero when there is no `AS` followed by a quote, which
/// leaves the search exactly where it was.
fn the_body_opener_in(statement: &str) -> usize {
    // SQL is case-blind and this tree writes `AS` in capitals, so the keyword is
    // matched against a cased-down copy while the offsets stay the original's —
    // `to_ascii_lowercase` is byte-for-byte, which is what that needs.
    let cased = statement.to_ascii_lowercase();
    // **And the bodies that open with a keyword rather than a quote.**
    // PostgreSQL's SQL-standard forms — `LANGUAGE sql RETURN <expr>` and
    // `BEGIN ATOMIC … END` — put the routine's code in the statement's own
    // tokens with no `AS` and no quote at all. A rule that looks only for `AS`
    // followed by a quote reads the whole of such a function as its header.
    let keyword = ["\nreturn ", " return ", "begin atomic"]
        .iter()
        .filter_map(|opener| cased.find(opener).map(|at| at + 1))
        .min();
    let bytes = cased.as_bytes();
    let mut from = 0_usize;
    while let Some(offset) = cased[from..].find("as") {
        let at = from + offset;
        from = at + "as".len();
        // **A word, not `" as "`.** SQL wraps where it reads best, and this
        // tree writes `LANGUAGE sql\nAS $probe$` — the keyword preceded by a
        // newline rather than a space, which a spaced needle does not match. The
        // boundaries are what make it the keyword rather than the tail of
        // `alias` or the head of `assert`.
        let opens = at == 0 || bytes[at - 1].is_ascii_whitespace();
        let rest = &cased[from..];
        if !opens || !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let head = rest.trim_start();
        if head.starts_with('$') || head.starts_with('\'') {
            let quoted = cased.len() - head.len();
            return keyword.map_or(quoted, |word| word.min(quoted));
        }
    }
    keyword.unwrap_or(0)
}

/// What this gate says about a `set_config` whose name it cannot read.
///
/// Held apart from the walk so the walk stays under a hundred lines, and
/// written as one pair so the verdict and the thing to do about it move
/// together.
const THE_UNREADABLE_NAME: (&str, &str) = (
    "a migration sets a configuration whose name this gate cannot read",
    "the name is built by a function, so nothing here can tell whether it is `search_path` \
     — and a `search_path` this gate cannot see is the §15.8 defect it exists to stop. \
     Write the setting's name as a literal, or, if it genuinely has to be computed, split \
     the statement so that the one naming `search_path` is written out",
);

/// Where a statement names `search_path`, in each of the two ways it can.
///
/// The tokeniser flushes on whitespace, so `search_path` may arrive as its own
/// word or glued to what follows it — `search_path=archive` is one token, and
/// the second position is what finds that. Held apart from the walk so the walk
/// stays under a hundred lines.
fn where_the_path_is_named(tokens: &[Token]) -> (Option<usize>, Option<usize>) {
    let at_the_name = tokens.iter().position(|token| {
        token
            .name()
            .is_some_and(|text| text.eq_ignore_ascii_case("search_path"))
    });
    let joined = tokens.iter().position(|token| {
        token
            .name()
            .is_some_and(|text| text.to_ascii_lowercase().starts_with("search_path="))
    });
    (at_the_name, joined)
}

/// Every custom setting a `SET` in this statement writes.
///
/// Held apart from the walk because a `SET` and a `set_config` are two
/// spellings of one question that read their name and value from different places.
fn the_settings_a_set_writes(
    statement: &Statement,
    index: usize,
) -> Vec<(String, usize, bool, bool)> {
    let mut written = Vec::new();
    // **`set_config` is not the only way to write a setting.** `SET nc.p =
    // 'archive'`, `SET LOCAL`, `ALTER DATABASE … SET` and `ALTER ROLE … SET`
    // are the same write in the spellings PostgreSQL also accepts, and with
    // none of them read a save can be overwritten with a constant
    // between the save and the read-back and the read-back is still
    // cleared. Measured on PostgreSQL 18: `SHOW search_path` → `archive`,
    // and an unqualified `CREATE TABLE` lands there. This is the class
    // three paragraphs in this file already carry, one level down — in the
    // allow-list rather than in the detection.
    //
    // A `SET` names its setting in the token after the keyword, so what it
    // writes is read the same way and its value is judged by the same shape
    // rule. A name with no dot is a PostgreSQL setting rather than a custom
    // one and is not what this list is about.
    for (at, token) in statement.tokens.iter().enumerate() {
        if !token.is("SET") {
            continue;
        }
        let mut name_at = at + 1;
        if statement
            .tokens
            .get(name_at)
            .is_some_and(|next| next.is("LOCAL") || next.is("SESSION"))
        {
            name_at += 1;
        }
        // A qualified setting name arrives in pieces — `nc`, `.`, `p` — so
        // it is joined back before it is compared with what a
        // `current_setting` asks for.
        let mut name = String::new();
        let mut ends = name_at;
        while let Some(token) = statement.tokens.get(ends) {
            match token {
                Token::Punct('.') => name.push('.'),
                // **And the `=` the tokeniser glued on.** `SET nc.p='archive'`
                // yields `nc`, `.`, `p=`, so unstripped the write is recorded
                // under the name `nc.p=` and `holds_the_path`, looking for
                // `nc.p`, never finds the overwrite. Measured on PostgreSQL 18:
                // the read-back puts `probe_a` on the path and a table lands
                // there.
                _ => match token.name() {
                    Some(word) if name.is_empty() || name.ends_with('.') => {
                        let word = word.to_ascii_lowercase();
                        let (word, glued) = word
                            .split_once('=')
                            .map_or((word.as_str(), false), |(before, _)| (before, true));
                        name.push_str(word);
                        if glued {
                            ends += 1;
                            break;
                        }
                    }
                    _ => break,
                },
            }
            ends += 1;
        }
        if !name.contains('.') {
            continue;
        }
        let name_at = ends - 1;
        let assigned = statement.tokens.get(name_at).is_some_and(assigns_here)
            || statement
                .tokens
                .get(name_at)
                .and_then(Token::name)
                .is_some_and(|word| word.contains('='));
        // The value begins after the `=` or `TO`, which is where the shape
        // rule reads a `set_config`'s second argument from.
        // **A `SET`'s value is not an expression, and `current_schema` in one is
        // an identifier.** `the_value_is_the_path` reads a
        // `set_config`'s second argument, where `current_schema` is the function;
        // `assigns_a_new_path` in this same file already records that in a `SET`
        // it names a schema, and a migration may create one called
        // `current_schema` — two fixtures here do. So `SET nc.p = current_schema`
        // stores that constant, and read as the function, the write is recorded
        // as path-bearing and the restore reading it back is cleared. Measured on
        // PostgreSQL 18, with `access_records` landing in `"current_schema"`. That
        // opens the exception in `hold_the_path`, so a four-word edit to a fixture
        // already in the refused list walks through the guard by way of the
        // guard's own escape.
        let value_at = if assigned { name_at + 1 } else { name_at + 2 };
        // **A slice, and the statement may end before it.** `SET` is a
        // non-reserved word, so `CREATE SCHEMA set; CREATE VIEW v AS SELECT c
        // FROM set.t;` is ordinary SQL — and an unchecked index runs past the
        // end, so `cargo xtask check` aborts with a Rust slice message instead of
        // producing a finding, measured. A truncated `SET nc.p` does the same. This is the
        // gate whose whole argument is *refused rather than guessed at*, and a
        // panic is neither.
        let value = statement.tokens.get(value_at..).unwrap_or_default();
        let names_a_schema = value.iter().enumerate().any(|(at, token)| {
            token.is("current_schema")
                && !value
                    .get(at + 1)
                    .is_some_and(|next| matches!(next, Token::Punct('(')))
        });
        let from_the_path = (assigned
            || statement.tokens.get(name_at + 1).is_some_and(assigns_here))
            && !names_a_schema
            && the_value_is_the_path(value, &Settings::none());
        written.push((name, index, from_the_path, statement.nested));
    }
    written
}

/// What this file knows about the custom settings a migration writes.
///
/// The three travel together because they are one question — *does this name
/// hold the real path at this point in the run* — and passing them separately
/// puts eight arguments on one signature.
struct Settings<'a> {
    /// Every write to a custom setting: its name, its place in the running
    /// order, whether its value came from the path, and whether it was written
    /// somewhere the running order cannot be read from.
    written: &'a [(String, usize, bool, bool)],
    /// Where the statement asking sits in that order.
    index: usize,
}

impl Settings<'_> {
    /// Nothing known, for the pre-pass that builds the list itself.
    fn none() -> Self {
        Self {
            written: &[],
            index: 0,
        }
    }

    /// Whether `name` holds the real path at the statement numbered `self.index`.
    ///
    /// **A name is not a value, and a name can be refilled.** A set of *names*
    /// gathered over the whole file with no regard to order counts every
    /// `current_setting` of a custom setting as path-bearing once it has been
    /// filled from the path anywhere — including after it has been overwritten
    /// with a constant, and including *before* the save that fills it.
    /// Measured: three statements, `nc.probe` ← path, `nc.probe` ← `'archive'`,
    /// `search_path` ← `nc.probe`, give `check passed` with the `ok` lines
    /// unchanged.
    ///
    /// So what counts is the most recent assignment *before* the statement asking.
    /// Statements are numbered in the order they run, which is the order a migration
    /// applies them in.
    fn hold_the_path(&self, name: &str) -> bool {
        // A write this walk could not place in the running order — one inside a
        // routine body — is recorded as not path-bearing, and one anywhere in the
        // file is enough to stop the name being trusted at all: the comparison
        // below rests on an order, and there is none to compare against.
        // A write inside a routine body cannot be ordered against a *top-level*
        // statement, because `cut` finishes the top level before it recurses. It can
        // be ordered against another nested one: bodies are appended whole and in
        // order, so within and across them the index is the running order. Poisoning
        // regardless refuses the save-and-restore pair `0011` and `0012` write the
        // moment it is wrapped in the `DO` a conditional migration needs — measured
        // on PostgreSQL 18 as a genuine no-op.
        //
        // **Mixed is unknowable, in both directions.** A guard for a top-level
        // *reader* only misses it. Measured: a save inside one `DO` block, a
        // top-level `SET nc.p = 'archive'` between them, and the restore inside
        // a second `DO` block — the overwrite is numbered *before* both bodies,
        // so the restore trusts a save that has already been replaced.
        // `SHOW search_path` → `r16archive`, and a table lands there. Sharper
        // still: the save written inside a `CREATE FUNCTION` body the migration
        // never calls is trusted too. Within one level the index is the running
        // order and that is kept; a name written at both levels is judged by
        // neither.
        // **And only where the order could change the answer.** Written as "a
        // name written at both levels is judged by neither", this refuses a
        // migration whose *every* write of the name comes straight from
        // `current_setting('search_path')` — a measured no-op, and one edit away
        // from the tree: `0012` already writes `nc.path_before_0012` at the top
        // level and carries a `DO` block, so moving the save inside the guard's
        // branch to make it conditional turns the file red. Where every write is
        // path-bearing there is nothing an ordering could reorder into a
        // redirect.
        let mixed = {
            let at_level = |wanted: bool| {
                self.written
                    .iter()
                    .any(|(setting, _, _, nested)| setting == name && *nested == wanted)
            };
            at_level(true) && at_level(false)
        };
        let all_from_the_path = self
            .written
            .iter()
            .filter(|(setting, _, _, _)| setting == name)
            .all(|(_, _, from_the_path, _)| *from_the_path);
        if mixed && !all_from_the_path {
            return false;
        }
        self.written
            .iter()
            .rfind(|(setting, at, _, _)| setting == name && *at < self.index)
            .is_some_and(|(_, _, from_the_path, _)| *from_the_path)
    }
}

/// Whether a token names the function `name`, quoted or not.
///
/// **`Token::is` matches a bare word only**, and its own paragraph justifies
/// that with `CREATE "TABLE" x` — which is true of a *keyword*: quoting one
/// stops it being one. A function name is an identifier, and quoting a
/// lower-case identifier changes nothing about which function runs. Measured on
/// PostgreSQL 18, `SELECT "set_config"('search_path', 'probe_q', false)` puts
/// `probe_q` on the path and a table lands there, with all three readers of
/// this name silent under a bare-word match.
///
/// A quoted identifier is case-sensitive, so only the exact lower-case spelling
/// counts; a bare word is folded, so it is compared without case.
fn names_the_function(token: &Token, name: &str) -> bool {
    match token {
        Token::Name(quoted) => quoted == name,
        _ => token.is(name),
    }
}

/// One statement's own source lines, joined.
///
/// Two readers need the same span — the hand-clearance and the routine-clause
/// rule — so it is taken once and by one rule.
fn the_source_of(lines: &[&str], statement: &Statement) -> String {
    let from = statement.line.saturating_sub(1).min(lines.len());
    let upto = statement.ends_on.min(lines.len()).max(from);
    lines[from..upto].join("\n")
}

/// Whether the `search_path` this statement names is the routine's own clause.
///
/// **`CREATE FUNCTION … SECURITY DEFINER SET search_path = pg_catalog, pg_temp
/// AS $$…$$` writes `proconfig` and leaves `SHOW search_path` exactly as it
/// was** — measured on PostgreSQL 18, and it is the canonical hardening that
/// migration `0011`'s own comment block records. Reading it as a session
/// redirect refuses the one thing this gate most wants a migration to do.
///
/// Such a statement carries a `SET search_path` of its own, which is a
/// property of the routine rather than of the session — `proconfig`, not the
/// path the next statement runs under. It is judged by the pin gate in this same
/// file, which asks a different and stricter question of it.
fn the_routines_own_clause(tokens: &[Token], source: &str) -> bool {
    if !defines_a_routine(tokens) {
        return false;
    }
    // **The clause, not the statement — and what that is really worth.** A
    // hatch skipping the whole statement is spelling-sensitive:
    // a dollar-quoted body is re-cut and judged, while PostgreSQL's SQL-standard
    // body lives in the statement's own tokens. Measured on PostgreSQL 18, such a
    // redirect runs and a table lands in the wrong schema.
    //
    // **But it is `sets_by_function` that catches it, not this narrowing.**
    // `sets_by_function` is
    // deliberately not gated by this hatch, so a `set_config` anywhere in a
    // routine — header, dollar-quoted body or SQL-standard body — is judged
    // whatever this answers. And `SET` is not legal in a SQL-standard body at
    // all:
    //
    // ```text
    // CREATE FUNCTION f() RETURNS void LANGUAGE sql BEGIN ATOMIC SET search_path = a; END;
    // ERROR:  SET is not yet supported in unquoted SQL function body
    // ```
    //
    // So `sets_the_path`, the only branch this hatch gates, cannot arise from a
    // keyword body, and the false-refusal direction is decided loudly: the
    // attributes-after-body fixture, the block-comment header and the
    // parameter-default header each turn red if `the_body_ends_at`'s bracketing
    // or `strip_sql_comments` goes.
    //
    // **The narrowing itself is decided by what it does, not by a redirect.**
    // No redirect is left in the tree to catch it with, so a test asks the
    // question directly instead:
    // `the_routine_hatch_reads_the_clause_and_not_the_body`. Replaced with
    // `true` — the shape whose paragraph above records such a redirect running
    // and a table landing in the wrong schema — the narrowing fails it, and so
    // does `begin atomic` removed from the body openers.
    //
    // A routine's configuration clause is written among its *attributes*, and
    // PostgreSQL takes those in any order — before the body or after it.
    // Deciding on the first `search_path` and calling everything from the body
    // opener onwards "the routine's code" refuses the identical function with
    // two attributes swapped:
    // `CREATE FUNCTION … AS $$…$$ SECURITY DEFINER SET search_path = pg_catalog,
    // pg_temp;` writes `proconfig` and leaves `SHOW search_path` alone, measured
    // on PostgreSQL 18. The pin rule in this same file already reads the option
    // list after the body for that reason. So what is excluded is the body
    // *span*, not everything after where it starts.
    //
    // **And SQL's comments, with SQL's stripper.** `gates::without_comments`
    // closes `/* … */` only for Rust, so with it a block comment in a header —
    // or a string in a parameter default — is scanned for the body-opener
    // keywords and the function refused. `strip_sql_comments`'s own paragraph in this
    // file records that exact failure one rule over.
    // `strip_sql_comments` blanks a string's contents as well as removing every
    // comment form, and preserves length — which is what both halves need: a
    // `DEFAULT ' return '` must not open a body either, and the offsets have to
    // stay the source's.
    let source = strip_sql_comments(source);
    let cased = source.to_ascii_lowercase();
    let opens = the_body_opener_in(&source);
    let body = if opens == 0 {
        opens..opens
    } else {
        opens..the_body_ends_at(&source, opens)
    };
    cased
        .match_indices("search_path")
        .all(|(at, _)| !body.contains(&at))
}

/// Where the body that opens at `opens` ends.
///
/// A dollar-quoted body ends at its own tag and a quoted one at its quote;
/// a keyword body — `RETURN …`, `BEGIN ATOMIC … END` — runs to the statement's
/// end, and so does anything this cannot bracket, which is the safe direction:
/// an attribute the rule cannot place is not excused.
fn the_body_ends_at(source: &str, opens: usize) -> usize {
    let rest = &source[opens..];
    if let Some(after) = rest.strip_prefix('$') {
        let Some(tag_end) = after.find('$') else {
            return source.len();
        };
        let tag = &rest[..tag_end + 2];
        return rest[tag.len()..]
            .find(tag)
            .map_or(source.len(), |end| opens + tag.len() + end + tag.len());
    }
    if let Some(after) = rest.strip_prefix('\'') {
        return after.find('\'').map_or(source.len(), |end| opens + end + 2);
    }
    source.len()
}

/// Whether a statement defines or alters a routine.
fn defines_a_routine(tokens: &[Token]) -> bool {
    let word = |at: usize| {
        tokens
            .get(at)
            .and_then(Token::name)
            .map(str::to_ascii_lowercase)
    };
    let routine = |at: usize| {
        word(at).is_some_and(|word| matches!(word.as_str(), "function" | "procedure" | "routine"))
    };
    match word(0).as_deref() {
        Some("create") => routine(1) || (word(1).as_deref() == Some("or") && routine(3)),
        Some("alter") => routine(1),
        _ => false,
    }
}

/// Whether a token is the assignment of a `SET`, however it was cut.
///
/// The tokeniser flushes on whitespace, so `= archive`, `=archive` and `TO x`
/// are three shapes for one operator, and a reader of the first alone misses two.
fn assigns_here(token: &Token) -> bool {
    // **And `FROM`, which is the fourth member of the closed set.** The
    // paragraph beside `reset_the_path` names `SET x TO v`, `SET x FROM
    // CURRENT`, `RESET x` and `RESET ALL`, and calls the last omission "one rule
    // written twice, with the second copy narrower"; without `FROM`, this rule
    // is that narrower copy, one member over. Measured on PostgreSQL 18: `ALTER DATABASE
    // noctua SET search_path FROM CURRENT` writes `pg_db_role_setting`, and a
    // brand-new session then answers with the saved path and an unqualified
    // `CREATE TABLE` lands there — the §15.8 harm, made permanent and surviving
    // every future session.
    //
    // In a routine's own attribute list `SET search_path FROM CURRENT` is the
    // *pin* every function in this tree writes, and telling the two apart is
    // what [`the_routines_own_clause`] is for — which is also the first thing
    // that rule decides.
    token.is("=")
        || token.is("TO")
        || token.is("FROM")
        || token.name().is_some_and(|word| word.starts_with('='))
}

/// Whether a `SET search_path` at `at` puts a new path there.
///
/// **And nothing after `current_schema`.** A carve-out reading one token
/// accepts `SET search_path TO current_schema, archive;` — two words longer,
/// and a redirect. Measured on PostgreSQL 18: unquoted
/// `current_schema` in a `SET` is parsed as an *identifier* rather than the
/// function, `SHOW search_path` answers `"current_schema", archive`, and an
/// unqualified `CREATE TABLE` below it lands in `archive`. So no excuse is
/// made for it at all; see the paragraph inside.
fn assigns_a_new_path(tokens: &[Token], at: usize) -> bool {
    // **The operator, however the tokeniser cut it.** `SET search_path =archive;`
    // — a space before the `=` and none after — flushes `search_path` as one
    // word and glues `=archive` into the next, so a reader of a bare `=` and
    // the `search_path=` one beside it see no assignment and the statement
    // produces no finding at all. Measured on PostgreSQL 18: `SHOW search_path` →
    // `archive`, and an unqualified `CREATE TABLE access_records` lands there,
    // which is the §15.8 scenario this gate's own paragraph names.
    if !tokens.get(at + 1).is_some_and(assigns_here) {
        return false;
    }
    // **And there is no carve-out, because there is no spelling to carve out.**
    // `SET search_path TO current_schema` names an *identifier*, and a
    // migration may create a schema called `current_schema`; the function,
    // with its parentheses, cannot be written in a `SET` at all. Measured on
    // PostgreSQL 18, that statement does not exist:
    //
    // ```text
    // SET search_path = current_schema();
    // ERROR:  syntax error at or near "("
    // ```
    //
    // `SET` takes identifiers and literals, not calls, so a carve-out requiring
    // the parentheses is unreachable from any valid migration — an assertion
    // that cannot fire in the direction it is written for. A `SET` that names the path is a
    // redirect, and the way to keep the path is `set_config`, which the value
    // rule reads.
    true
}

/// Whether this exact statement is one somebody has read and written down.
///
/// **The statement's own words, and all of them.** Written as a `contains` over
/// the source *lines*, anything sharing a line with the cleared statement
/// inherits its clearance — a `--` comment holding the cleared words grants
/// it, which is a gate satisfied by a comment, in the file that carries three
/// paragraphs about scanners reading a comment as code. So comments come off
/// first and the match is an equality: a second statement on the same line
/// fails to match and is reported, which is the safe direction.
///
/// [`strip_sql_comments`] spends a string's *contents* as well, and against a
/// text whose literals are blanked this would clear any statement of the same
/// shape whatever settings it named. [`crate::gates::without_comments`] keeps
/// them verbatim, which is what an identity needs.
fn read_by_hand(lines: &[&str], line: usize, ends_on: usize) -> bool {
    let from = line.saturating_sub(1).min(lines.len());
    let upto = ends_on.min(lines.len()).max(from);
    let source = just_the_words(&crate::gates::without_comments(
        &lines[from..upto].join("\n"),
        "--",
    ));
    CLEARED_BY_HAND
        .iter()
        .any(|cleared| source == just_the_words(cleared))
}

/// One statement's words, with every run of whitespace collapsed.
///
/// A migration wraps where it reads best and rustfmt does not touch SQL, so the
/// key has to be the words rather than the layout.
fn just_the_words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every table in the migrations — both directories of them — declares a bound.
///
/// Returns the findings and how many `CREATE TABLE` statements were read, so a
/// migrations directory that lost its files is visible rather than green.
///
/// # Errors
///
/// Returns why the migrations could not be read.
pub(crate) fn every_table_declares_a_bound(
    root: &Path,
) -> Result<(Vec<Finding>, usize, Vec<String>, usize), String> {
    let (mut findings, mut seen, mut excused, mut files) = (Vec::new(), 0, Vec::new(), 0);
    for (directory, database, dialect) in DIRECTORIES {
        let (said, counted, excused_here, read) =
            one_directory_declares_its_bounds(root, directory, database, dialect)?;
        findings.extend(said);
        seen += counted;
        excused.extend(excused_here);
        files += read;
    }
    excused.sort();
    Ok((findings, seen, excused, files))
}

/// Every table one directory of migrations creates declares a bound of its own
/// database.
///
/// # Errors
///
/// Returns why the directory could not be read.
fn one_directory_declares_its_bounds(
    root: &Path,
    migrations: &str,
    database: nc_catalog::Database,
    dialect: Dialect,
) -> Result<(Vec<Finding>, usize, Vec<String>, usize), String> {
    let directory = root.join(migrations);
    // **The local directory may be absent from a tree this reads, and
    // `migrations/` may not.** A scratch tree built to drive one rule of this
    // gate holds `migrations/` alone; in this repository `local-migrations/`
    // going missing is refused twice before this is asked — by
    // `LOCAL_MIGRATIONS_ON_DISK`, which cannot read it, and by the schema gate,
    // which cannot build it — so reading it as holding nothing here reports no
    // table as bounded that is not.
    if !directory.is_dir() && database == nc_catalog::Database::LocalFile {
        return Ok((Vec::new(), 0, Vec::new(), 0));
    }
    if !directory.is_dir() {
        return Err(format!(
            "{migrations}/ is missing, so the unbounded-table gate read nothing there. It \
             is what makes NOCTUA.md §15.8 a policy rather than a habit"
        ));
    }

    // **Its own database's entries, and no other.** A table the other
    // directory creates under the same name is a different table, and an entry
    // saying `local` bounds nothing PostgreSQL holds.
    let declared: Vec<&str> = nc_catalog::TABLES
        .iter()
        .filter(|table| table.database == database)
        .map(|table| table.name)
        .collect();
    let mut findings = Vec::new();
    let mut seen = 0_usize;
    let mut exists: Vec<String> = Vec::new();

    let mut excused_by_a_marker: Vec<String> = Vec::new();
    let mut all: Vec<Created> = Vec::new();
    let mut changed_columns: Vec<ColumnChange> = Vec::new();
    let mut domains: Vec<(String, String)> = Vec::new();
    // How many statements have been read, so `Created.at` and
    // `ColumnChange.at` order the whole set of migrations and not one file.
    let mut stamp = 0_usize;
    // **And how many files the walk actually reads.** A count of
    // `CREATE TABLE`s alone, floored against the number of `[tables.` lines in
    // the catalog, does not move when a migration goes unread.
    // Measured with that count alone: filtering the walk to files below
    // `0007` leaves `cargo test -p xtask --bin xtask` passing, and an unbounded
    // `CREATE TABLE login_attempts` appended to `0012` is then excused.
    // The accidental coverage stops at `0006` only because `access_records`
    // gains its ordering column by `ALTER TABLE` there.
    //
    // Its sibling `every_migration_is_numbered_once` returns a file count and
    // `check.rs` floors it, and this one does the same. The count
    // is taken here, inside the loop that judges, so a filter between the two
    // pays for itself.
    let mut files = 0_usize;
    for path in migrations_in(root, migrations)? {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        files += 1;
        let relative = relative_to(root, &path);
        let lines: Vec<&str> = text.lines().collect();
        let comments = comment_only_lines(&text, dialect);
        let mut changes = changes_in_as(&text, dialect);
        // **Where each statement sits among all of them, across every file.**
        // A column's type is decided by the last statement to decide it, and
        // `created` and `columns` are two lists: without a shared ordering, a
        // table dropped and re-created after an `ALTER COLUMN … TYPE` would be
        // read through the stale alteration.
        for created in &mut changes.created {
            created.at += stamp;
        }
        for column in &mut changes.columns {
            column.at += stamp;
        }
        for rename in &mut changes.renamed {
            rename.at += stamp;
        }
        stamp += changes.statements;

        if changes.statements == 0 {
            findings.push(runs_nothing(&relative));
        }
        domains.extend(changes.domains.iter().cloned());

        // **A shape this scanner cannot read is refused, not guessed at.**
        // `COPY t FROM stdin;` is followed by raw data until a line holding
        // `\.`, and an apostrophe in that data opens a string literal that runs
        // to the next one — measured, it blanks the rest of the file, so every
        // `CREATE TABLE` after it goes unseen while the gate reports a count
        // and passes. It is refused rather than parsed because `cargo xtask dev`
        // applies migrations through sqlx, which cannot feed a statement's
        // stdin: a migration holding one is already broken, and saying so is
        // both simpler and more true than teaching the tokeniser a second
        // grammar.
        // **The reader says what it judged, and a disagreement is an error, not a
        // finding.** With the reader taking the other count and reporting the
        // disagreement as one of its own verdicts, three shapes get past: a
        // guard clause at the top of the reader returning before the comparison
        // runs; a filter here on `files == 1` dropping every verdict from the
        // migrations no fixture writes; and a `retain` on the verdict's own text
        // silencing the comparison itself. A finding can be
        // filtered — this cannot, and it stops the walk where it is.
        let (refused, judged) = unreadable_statements_in(&text, dialect);
        the_readers_agree(
            &relative,
            judged.wrapping_add(a_forced_disagreement()),
            changes.tokens,
        )?;
        for (line, what, fix) in refused {
            findings.push(Finding {
                file: relative.clone(),
                line,
                what,
                fix: fix.to_owned(),
            });
        }

        all.extend(changes.created.iter().cloned());
        changed_columns.extend(changes.columns.iter().cloned());
        // **The counter is the last statement a verdict reaches, not the first
        // statement of the body.** `seen` is what `check.rs` floors against
        // `catalog/limits.toml`'s entry count and what it prints, and counted at
        // the top of this loop, one line below it turns criterion 5 off with the
        // number unmoved:
        //
        // ```text
        // + if table.line > 5 { continue; }
        //   PROBE seen=3 bounded=2 floor_ok=true findings=[]
        //   the suite passes
        // ```
        //
        // A real unbounded table, the count printed over it, and the suite
        // green. `xtask/canaries/README.md` states this rule for every gate,
        // and this is where this one keeps it.
        // **In the order the statements run, not every create and then every
        // rename.** Two loops, the first run to its end before the second
        // begins, make a `DROP` within one file always win over a `CREATE`
        // however they are written:
        //
        // ```text
        // -- 0002, with every catalog table created in 0001
        // DROP TABLE access_records;
        // CREATE TABLE access_records (recorded BIGINT NOT NULL, note TEXT);
        //   -> "`access_records` is bounded and nothing creates it"
        // ```
        //
        // Dropping and re-creating a table inside one migration is ordinary
        // work, and unmerged the gate tells the developer to delete a bound that
        // is correct. That is this rule's *too strong* failure — a refusal of
        // something legitimate — and a tree where no migration does it yet is
        // green regardless. Both lists carry the statement they came from, and
        // they are merged on it.
        let (said, excused_here, counted) = what_one_file_declares(
            &changes,
            &Bounds {
                declared: &declared,
                database,
            },
            &lines,
            &comments,
            &relative,
            &mut exists,
        );
        findings.extend(said);
        excused_by_a_marker.extend(excused_here);
        seen += counted;
    }

    // **Where the entry actually is.** Reported at line 1 of the catalog, a
    // finding about an entry living well down the file sits beside
    // migration-side findings that carry real line numbers, so one run shows
    // both and teaches a reader to distrust the column. The text is read here
    // rather than in the rule so the rule stays a function of the migrations
    // and the catalog.
    let limits = std::fs::read_to_string(root.join("catalog/limits.toml")).unwrap_or_default();
    findings.extend(order_by_names_a_column(
        &all,
        &changed_columns,
        &domains,
        &limits,
        database,
    ));

    findings.extend(bounded_and_never_created(&exists, &limits, database));

    excused_by_a_marker.sort();
    Ok((findings, seen, excused_by_a_marker, files))
}

/// Every entry of one database naming a table its migrations never create.
///
/// **The other direction**: an entry naming a table nothing creates is a limit
/// that will never be applied, which reads as protection and is not. Split out
/// of the walk, which reads two directories.
fn bounded_and_never_created(
    exists: &[String],
    limits: &str,
    database: nc_catalog::Database,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    // `access_records` and `journal_entries` are both declared *and* created, so
    // this has something real to check rather than a rule nothing exercises.
    for table in nc_catalog::TABLES
        .iter()
        .filter(|table| table.database == database)
    {
        if !exists.iter().any(|name| name == table.name) {
            findings.push(Finding {
                file: "catalog/limits.toml".to_owned(),
                // **The entry's own line**, and not `1`, which is the file's
                // header comment, while the migration-side findings in the same
                // run carry real numbers — the mixed transcript `line_of_entry`'s
                // own doc describes, which teaches a reader that the column is
                // decorative.
                line: line_of_entry(limits, table.name),
                what: format!("`{}` is bounded and nothing creates it", table.name),
                fix: "a limit on a table that does not exist is a number the pruner will \
                      never apply, and it reads as protection. Either add the migration \
                      that creates it, or remove the entry until the stage that does"
                    .to_owned(),
            });
        }
    }
    findings
}

/// The finding a table produces whose entry names the other database.
fn in_the_other_database(
    file: &str,
    table: &Declared<'_>,
    database: nc_catalog::Database,
) -> Finding {
    let (here, there) = match database {
        nc_catalog::Database::Server => ("migrations/", "local"),
        nc_catalog::Database::LocalFile => ("local-migrations/", "server"),
    };
    Finding {
        file: file.to_owned(),
        line: table.line,
        what: format!(
            "`{}` is created in {here} and its entry in catalog/limits.toml says \
             `database = \"{there}\"`",
            table.name
        ),
        fix: format!(
            "an entry bounds a table in the database it names, and the pruner looks for \
             `{}` there — where this migration does not put it. Make the entry name the \
             database whose migrations create the table",
            table.name
        ),
    }
}

/// The finding a table with no bound produces.
fn unbounded(file: &str, name: &str, line: usize) -> Finding {
    Finding {
        file: file.to_owned(),
        line,
        what: format!("`{name}` has no entry in catalog/limits.toml"),
        fix: format!(
            "a table that can grow without bound needs `[tables.{}]` with \
             `max_rows`, `prune_batch`, `strategy` and `order_by`, so one pruner \
             can enforce it (NOCTUA.md §15.8). If it genuinely cannot grow, say \
             so in the comment block directly above it, on a line beginning \
             `-- {BOUNDED} <reason>`",
            // **Quoted when it carries a schema.** A bare
            // `[tables.archive.access_records]` is a *nested* table to TOML, and
            // such advice followed literally produces `unknown field
            // 'access_records'` — a gate telling a developer to write something
            // that does not parse.
            toml_key(name)
        ),
    }
}

/// A table's name as TOML writes it under `[tables.…]`.
///
/// A dot means a schema, and TOML reads a bare dot as nesting — so a
/// schema-qualified name has to be quoted or the entry is a different thing
/// entirely.
fn toml_key(name: &str) -> String {
    if name.contains('.') {
        format!("\"{name}\"")
    } else {
        name.to_owned()
    }
}

/// Whether the comment block directly above a statement excuses it.
///
/// **Three rules, and each closes a way the hatch would be free.**
///
/// The marker must *begin* the comment, the block must be contiguous with the
/// statement, and **the statement must be the only one on its line**. Without
/// the third, the hatch is line-scoped while detection is statement-scoped,
/// and one reason excuses two tables:
///
/// ```sql
/// -- nc:bounded: exactly one row, written by the installer
/// CREATE TABLE install_marker (id INT); CREATE TABLE login_attempts (id TEXT);
/// ```
///
/// That is the line-versus-statement mismatch, on the side of the
/// gate that turns it *off*.
///
/// Without the first, a comment that merely *describes* the hatch switches it
/// off for everything beneath — the sentence this gate's own `fix` text tells you to
/// write. That is the self-exemption `references.rs` documents for
/// `is_generated`, and the reason the marker must begin its line.
fn excused(lines: &[&str], comments: &[usize], table: &Declared<'_>) -> bool {
    if !table.alone {
        return false;
    }
    let mut at = table.line;
    while at > 1 {
        at -= 1;
        // **Only a line the scanner called a comment.** Reading raw text, a
        // `-- nc:bounded: …` that is the *second line of a string literal* —
        // house style quoted inside a `COMMENT ON` — excuses the table beneath
        // it, with the gate green and the table created on a live server.
        // Detection is statement-scoped, and the hatch has to be too.
        if !comments.contains(&at) {
            return false;
        }
        if is_the_marker(lines.get(at - 1).copied().unwrap_or_default(), table.name) {
            return true;
        }
    }
    false
}

/// A name one statement brings into existence, and where its marker would be.
///
/// **Two statements create a table, and both can be excused.** With
/// `CREATE TABLE` reaching the hatch and `ALTER TABLE … RENAME TO` not, the
/// remedy printed on *both* paths still says, on both, that a table which
/// genuinely cannot grow may say so in a comment above its statement — the
/// marker `BOUNDED` spells, named here without spelling it, because a census
/// counts a hatch wherever it is written. Measured that way: a marker written
/// above a rename leaves the finding standing, so the gate names a hatch that path
/// does not have — prose stating the inverse of the code, which is a shape this
/// file carries three paragraphs about.
struct Declared<'a> {
    /// The table, as `catalog/limits.toml` would spell it.
    name: &'a str,
    /// The line the statement opens on, one-based.
    line: usize,
    /// Whether a marker above the statement can speak for this name alone.
    ///
    /// A marker excuses one table: two statements on one line cannot both be
    /// the one it meant, and `CREATE SCHEMA … CREATE TABLE a … CREATE TABLE b`
    /// is one statement declaring two, so a marker above it cannot say which.
    alone: bool,
}

/// Whether one comment line *is* the marker, rather than mentioning it.
fn is_the_marker(line: &str, table: &str) -> bool {
    let Some(comment) = line.trim_start().strip_prefix("--") else {
        return false;
    };
    let Some(reason) = comment.trim_start().strip_prefix(BOUNDED) else {
        return false;
    };
    // **And the reason names the table.** Held only to non-empty, `x` is a
    // reason — and the marker sits above a statement, which means the
    // *next* table added above it inherits an excuse written about a different
    // one. Naming the table makes an excuse that drifted onto a new table fail
    // rather than widen, the rule `allow-logging:`, `allow-count:` and
    // `allow-nondeterminism:` all follow, for the same measurement.
    // **And a preceding `.` is a different table, not this one.**
    // `references::names` accepts one, because it reasons about paths: in
    // `docs/limits.md` a preceding `/` is a path's last segment and *does* name
    // the thing. In SQL it is a schema qualifier, so
    // a marker whose reason reads `archive.login_attempts …` would excuse a
    // bare `login_attempts` in `public` — measured through criterion 5's own
    // driver: `the gate found 0 unbounded table(s), and it leaves 1`. That is
    // the excuse drifting onto another table, which is the failure the `names`
    // requirement exists to stop.
    //
    // Decided here rather than in `names`, because a preceding `.` in a Rust
    // path — `Utc::now` beside `now` — is a different question.
    // **One occurrence has to satisfy both halves.** `names` needs a
    // word-bounded occurrence and the clause beside it needs one not preceded
    // by `.` — and asked apart, they need not be the *same* occurrence. So a reason about
    // `archive.login_attempts` plus any mention of the bare name inside a
    // longer identifier would excuse `login_attempts` in `public`: measured
    // through the gate, `unbounded=0 excused=["login_attempts"]` for a migration
    // whose table has no entry at all. And deleting the `names` half re-opens
    // the substring defect it exists for.
    // **And naming it is still not saying anything about it.** Measured through
    // criterion 5's own driver, all three of `nc:bounded: login_attempts`,
    // `nc:bounded: TODO login_attempts` and `nc:bounded: x login_attempts x`
    // give `found 0 unbounded, leaves 1` — the name satisfying the rule that
    // asked for the name. So the second half is the compiler's own
    // `a_reason_names_what_it_exempts`, which the sibling hatch reads as
    // well; one predicate, so the two cannot drift apart in
    // what counts as a reason.
    // **One rule, one place.** With both halves written here as well as in the
    // compiler's predicate, deleting either the word-boundary clause or the
    // schema-qualifier clause from this file leaves every corpus case deciding
    // the same way, and `mutants.py` reports two survivors. Two copies of one
    // rule is the defect this repository catches most often — the second copy
    // narrower than the language it scans — and the cure is not a third copy.
    nc_catalog_compiler::a_reason_names_what_it_exempts(table, reason).is_ok()
}

/// Every SQL file under `migrations/`, in a fixed order.
fn migrations(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    migrations_in(root, MIGRATIONS)
}

/// Every SQL file under one directory of migrations, in a fixed order.
fn migrations_in(root: &Path, directory: &str) -> Result<Vec<std::path::PathBuf>, String> {
    let mut files = Vec::new();
    walk(&root.join(directory), &mut |path| {
        if path.extension().is_some_and(|extension| extension == "sql") {
            files.push(path.to_path_buf());
        }
    })?;
    files.sort();
    Ok(files)
}

/// Where a `CREATE [OR REPLACE] FUNCTION|PROCEDURE` begins, and what follows it.
///
/// **SQL does not require a literal single space between the keywords**: a
/// line break after `CREATE`, or between `OR REPLACE` and `FUNCTION`, is the
/// same statement. A parser and an independent count that floors it, both
/// requiring the space, cannot see one, so the two agree and
/// `found != creates` — the one thing standing between a partial parse and a
/// green run — is satisfied by both halves missing the same statement.
/// Measured over a scratch migrations tree:
///
/// ```text
/// one line                       second count 1, parser 1, findings 1
/// a break between OR REPLACE and FUNCTION      0, 0, 0
/// a break straight after CREATE                0, 0, 0
/// ```
///
/// `CREATE  FUNCTION` with two spaces *is* caught — by the count mismatch —
/// which makes a single-space rule look tested. The live-database half backstops it, but
/// `--offline` and a machine with no stack have only this.
///
/// `lowered` is already lowercased and stripped of line comments. Returns the
/// byte offset of the `create` and the text after the routine keyword, so both
/// callers keep their own offsets.
fn routine_at(lowered: &str, from: usize) -> Option<(usize, &str)> {
    let mut at = from;
    while let Some(offset) = lowered[at..].find("create") {
        let start = at + offset;
        at = start + "create".len();
        // A word, not a substring: `recreate` and `created_at` are neither.
        let before = lowered[..start].chars().next_back();
        if before.is_some_and(|character| character.is_alphanumeric() || character == '_') {
            continue;
        }
        let Some(rest) = after_a_keyword(&lowered[start..], "create") else {
            continue;
        };
        let rest = after_a_keyword(rest, "or")
            .and_then(|rest| after_a_keyword(rest, "replace"))
            .unwrap_or(rest);
        if let Some(rest) =
            after_a_keyword(rest, "function").or_else(|| after_a_keyword(rest, "procedure"))
        {
            return Some((start, rest));
        }
    }
    None
}

/// A routine's name and argument list, from the text that follows its keywords.
///
/// **One spelling for both halves of this gate.** A `CREATE` side walking the
/// characters, counting brackets and dropping the whitespace inside them, and
/// an `ALTER` side taking `words.get(2)` — one whitespace-separated token —
/// disagree where the argument list holds a space, or a space
/// sits before the bracket, and a disagreement here is not a
/// miss: the `ALTER` lands in the map as a *second* function, pinned, and the
/// real one is reported unpinned. Measured, each an ordinary statement:
///
/// ```text
/// CREATE FUNCTION archive.f(a text) …;  ALTER FUNCTION archive.f(a text) SET …
///   -> 2 function(s), and `create function archive.f(a text) …` reported
/// CREATE FUNCTION archive.f() …;        ALTER FUNCTION archive.f () SET …
///   -> 2 function(s), and the same false finding
/// ```
///
/// A false finding is what this module's header calls worse than a miss, and
/// this one names the statement that carries the pin. So the spelling is one
/// function, called from both.
fn the_routine_name_in(after: &str) -> String {
    // **A space before the bracket is not the end of the name.** `f ()` and
    // `f (a text)` are ordinary PostgreSQL, and the walk below ends the name at
    // the first space outside a bracket — so it reads `f`, while the other half
    // reads `f()`. Closed here rather than in the walk so both sides get it.
    after
        .replace(" (", "(")
        .char_indices()
        .scan(0_i32, |depth, (at, character)| {
            match character {
                '(' => *depth += 1,
                ')' => *depth -= 1,
                _ => {}
            }
            Some((at, character, *depth))
        })
        .take_while(|(_, character, depth)| *depth > 0 || *character != ' ')
        .filter(|(_, character, _)| !character.is_whitespace())
        .map(|(_, character, _)| character)
        .collect()
}

/// `text` after `word` and the whitespace that must follow it, or `None`.
///
/// Whitespace, not one space: a newline separates two keywords exactly as well.
fn after_a_keyword<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    let breaks = rest
        .chars()
        .next()
        .is_some_and(|character| character.is_whitespace() || character == '(');
    breaks.then(|| rest.trim_start())
}

/// How many distinct functions the migrations create, counted without the parser.
///
/// **The walk's floor is not its own output.** `every_function_resolves_its_own_names`
/// reports `latest.len()` — the map the verdict loop drains — and with
/// `functions == 0` as the only guard, a parser that reads one function of three
/// passes with `ok 1 function(s) in migrations pin how they resolve a name`. A
/// floor derived from the thing it floors is a mirror.
///
/// **What it shares is the directory and the comment stripper, and nothing
/// else.** Sharing `routine_at` as well would put the *same* reader in both
/// halves, so `found != creates` could not disagree about anything that reader
/// decides. Measured: one `if rest.starts_with("access_") { continue; }` inside
/// a shared `routine_at` takes a real routine out of `creates=3, found=3` and
/// leaves it at `creates=2, found=2, mismatch=false, findings=0`, with both scratch
/// fixtures green. Two counts of the same ground catch a filter in one of them
/// and cannot catch a blind spot both share.
///
/// So this walks tokens: the whole file split on whitespace, looking for the
/// keyword sequence, which is a different way of finding the same statements —
/// and the two agree about whitespace without going through one function.
///
/// The comment stripper *is* shared, deliberately:
/// `line.split("--").next()` does not know a block comment, a dollar
/// quote or a string literal, so with it a `--` inside a literal makes the two
/// disagree and fails the run for a reason that is neither half's rule. A shared
/// normalisation of the input is not the same as a shared judgement of it.
///
/// It keys on the name and the argument list — which is what makes an overload a
/// second function — and counts the distinct set, because `latest` keeps one
/// entry per key however many times it is replaced.
pub(crate) fn functions_the_migrations_create(root: &Path) -> Result<usize, String> {
    let mut found: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    for path in migrations(root)? {
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
        let lowered = strip_sql_comments(&text).to_ascii_lowercase();
        // **Tokens, not `routine_at`.** The name and the argument list are read
        // off a whitespace split — `create`, then optionally `or replace`, then
        // `function` or `procedure`, then the declaration — which finds the same
        // statements by different code. A `(` may be joined to the name or stand
        // apart, so the tail is rejoined and cut at the parentheses.
        let spaced = lowered.replace('(', " ( ").replace(')', " ) ");
        let words: Vec<&str> = spaced.split_whitespace().collect();
        let mut at = 0_usize;
        while at < words.len() {
            if words[at] != "create" {
                at += 1;
                continue;
            }
            let mut next = at + 1;
            if words.get(next) == Some(&"or") && words.get(next + 1) == Some(&"replace") {
                next += 2;
            }
            if !matches!(words.get(next), Some(&"function" | &"procedure")) {
                at += 1;
                continue;
            }
            let Some(name) = words.get(next + 1) else {
                break;
            };
            let opens = next + 2;
            if words.get(opens) != Some(&"(") {
                at = next + 1;
                continue;
            }
            let Some(closes) = words[opens..]
                .iter()
                .position(|word| *word == ")")
                .map(|by| opens + by)
            else {
                break;
            };
            found.insert(((*name).to_owned(), words[opens + 1..closes].join(" ")));
            at = closes;
        }
    }
    Ok(found.len())
}

/// Every function a migration defines pins how it resolves a name.
///
/// **`pg_temp` is searched before `search_path`, and this looks.**
/// PostgreSQL resolves a relation name in the temporary schema first — ahead of
/// `search_path` entirely — and `CREATE TEMP TABLE` is a default privilege. A
/// trigger function reading an unqualified name lets one ordinary statement from
/// the writing service's own connection decide what it sees: a temp
/// `journal_entries` lets a foreign row leap to the top of `0008`'s range and
/// ends the history for ever, and a temp `access_records_recorded_seq` hands a
/// row a number the pruner will never reach. `migrations/0011_a_trigger_names_its_own_schema.sql` closes both and
/// carries the transcripts.
///
/// This is the static half of the gate, because closing two instances of a class
/// leaves the class: every function a migration defines must say `SET
/// search_path` on it.
///
/// **What the text alone cannot settle, and why the other half is elsewhere.**
/// An *empty* path does not take `pg_temp` out of relation resolution — left
/// unlisted, the temporary schema is searched first whatever the path says. What
/// takes it out is listing it, last:
///
/// ```text
/// search_path = ''                  + unqualified body  ->  999   <- the temp table
/// search_path = ''                  + qualified body    ->    7
/// search_path = pg_catalog, pg_temp + qualified body    ->    7
/// search_path = probe2, pg_temp     + unqualified body  ->    7
/// no SET at all                     + qualified body    ->    7
/// ```
///
/// So the question that decides the property is "does this function's path end in
/// `pg_temp`", and the answer is not in the file: `0011_a_trigger_names_its_own_schema.sql` writes `SET
/// search_path FROM CURRENT`, which freezes whatever the migration was applied
/// with — deliberately, because the integration tests migrate into a schema of
/// their own and a written `public` would aim a test's trigger at the developer's
/// real journal. Reading the text can only say that a path was pinned at all.
/// `crate::database` asks the database the other half, where the answer is.
///
/// **Two questions, not one, and neither is sufficient alone** — measured, with
/// a real table holding 7 and a temp one of the same name holding 999:
///
/// ```text
/// search_path = ''                  + unqualified body  ->  999
/// search_path = ''                  + qualified body    ->    7
/// search_path = pg_catalog, pg_temp + qualified body    ->    7
/// no SET at all                     + qualified body    ->    7
/// ```
///
/// An empty path does not take `pg_temp` out of relation resolution, so the
/// qualified name is what saves the table; `SET search_path` is what stops a
/// planted *function* or operator being called from the body, which qualifying a
/// table does not address. A function needs both, so both are asked for.
///
/// # Errors
///
/// Returns why `migrations/` could not be walked.
pub(crate) fn every_function_resolves_its_own_names(
    root: &Path,
    schema: &str,
) -> Result<(Vec<Finding>, usize), String> {
    let files = migrations(root)?;
    // Paths written literally into a migration, judged by the same predicate the
    // database half applies to `proconfig`. Collected here because `findings`
    // below belongs to the pass that reads the `latest` map.
    let mut written_paths = Vec::new();
    // An `ALTER` whose name matches no `CREATE`, gathered beside the paths for
    // the same reason: `findings` below belongs to the pass over `latest`.
    let mut findings_about_alters = Vec::new();
    // **The last definition of a name, not every definition of it.** Migrations
    // run forward only, so an earlier `CREATE FUNCTION` that a later
    // `CREATE OR REPLACE` supersedes is history and not the state of the
    // database — and reporting it would make the fix for this class fail the
    // gate that exists to keep it closed. `migrations()` sorts by name, which is
    // the order they run in, so the last one wins. An `ALTER FUNCTION … SET
    // search_path` changes only the pinning half, for the same reason: what is
    // judged is what the database ends up holding.
    // The state a function ends up in: where it was last defined, how that
    // definition reads, its line, and whether it is pinned. **The pin is a flag
    // and not a substring of the declaration**, because an `ALTER … RESET` has to
    // take it away — editing the declaration text to do that puts a mangled
    // sentence in the finding, which is a message about a statement nobody wrote.
    let mut latest: std::collections::BTreeMap<String, (String, String, usize, bool)> =
        std::collections::BTreeMap::new();
    for path in &files {
        let relative = relative_to(root, path);
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{relative} could not be read: {error}"))?;
        for (name, declared, _, line, is_alter) in functions_in(&text) {
            // **The clause, not the substring.** `"reset search_path"` contains
            // `"set search_path"`, so read as a substring, a migration that
            // *lifts* a pin reads as one that adds it: one `ALTER FUNCTION …
            // RESET search_path` prints
            // `ok 4 function(s) in migrations pin how they resolve a name` while
            // the database holds a null `proconfig` for that function.
            // `SET search_path TO DEFAULT` is the same shape, and
            // `check --offline` never reaches the database half that objects.
            // **And `RESET ALL`, which is the third and the widest.**
            // `ALTER FUNCTION f() RESET ALL` clears `proconfig` entirely, so it
            // lifts the pin without naming the path at all — measured, one such
            // line prints `ok 3 function(s) in migrations pin how they resolve
            // a name` under `check --offline` while the database half objects
            // that the function does not search `pg_temp` last. `--offline` is
            // what `just check-offline` runs.
            //
            // These three are `ALTER FUNCTION`'s whole grammar for a
            // configuration clause — `SET x TO v`, `SET x FROM CURRENT`,
            // `RESET x`, `RESET ALL` — so this is the closed set rather than a
            // list that grows the next time somebody spells it differently.
            let lifts = declared.contains("reset search_path")
                || declared.contains("reset all")
                || declared.contains("set search_path to default");
            // **And what the clause *says*, when it says anything.** Asking only
            // whether a `SET search_path` is present, with `pg_temp_is_last` —
            // the predicate that knows the rule — applied to `proconfig` from
            // the database and never to a path written into a migration, leaves
            // three shapes the class exists to stop unreported, one of them
            // verbatim the defect the twelfth migration exists to remove:
            //
            // ```text
            // ALTER … SET search_path TO pg_temp                 -> count=3 findings=0
            // SET search_path = pg_temp, public                  -> count=3 findings=0
            // SET search_path = "$user", public, pg_temp         -> count=3 findings=0
            // ```
            //
            // and on the database CI provides — empty, nothing applied — the
            // database half takes its announced skip, so `check passed`.
            // `FROM CURRENT` carries no value to judge: that one is the database
            // half's, and it is the shape the migrations here actually write.
            let written = the_path_written_in(&declared);
            let pinned = declared.contains("set search_path") && !lifts;
            match latest.entry(name) {
                // **`is_alter`, not `body.is_empty()`.** A bodyless
                // `CREATE OR REPLACE … LANGUAGE sql … RETURN …;` also has an
                // empty body, so read by `body.is_empty()`, a replacement that
                // *removes* the pin is filed as a no-op `ALTER` and the earlier
                // pinned entry survives in the map: measured, two definitions of
                // one function with the second unpinned give no finding and a
                // null `proconfig`.
                std::collections::btree_map::Entry::Occupied(mut held) if is_alter => {
                    // An `ALTER … SET` adds the pin and an `ALTER … RESET` takes
                    // it away, which is the whole point of writing one; an
                    // `ALTER` about anything else leaves it as it was.
                    if pinned || lifts {
                        held.get_mut().3 = pinned;
                        if lifts {
                            // So the finding says why, rather than quoting a
                            // declaration that plainly holds the clause.
                            held.get_mut().1 =
                                format!("{}, and a later `{declared}`", held.get().1);
                        }
                    }
                }
                std::collections::btree_map::Entry::Occupied(mut held) => {
                    held.insert((relative.clone(), declared, line, pinned));
                }
                // **An `ALTER` that names nothing this tree creates is a
                // finding, not a new function.** Inserting one — pinned, because
                // that is what an `ALTER … SET` says — lets the phantom entry
                // satisfy the gate while the function the statement was *meant*
                // to pin stays in the map unpinned and is reported. The message
                // then quotes the `CREATE`, and the statement that is actually
                // wrong is the `ALTER` two lines below it.
                //
                // Measured, with the two halves spelling the schema differently
                // — which PostgreSQL resolves through `search_path` and this
                // file cannot:
                //
                // ```text
                // CREATE FUNCTION f() …;  ALTER FUNCTION archive.f() SET …
                //   -> 2 function(s), and `create function f() …` reported
                // ```
                //
                // Every `ALTER FUNCTION` in this tree names a function an
                // earlier migration creates, which is the only way one can work:
                // the statement fails outright otherwise, and a gate that stays
                // quiet about a statement the database will refuse is a gate
                // that only ever agrees with a migration nobody has run.
                std::collections::btree_map::Entry::Vacant(empty) if is_alter => {
                    findings_about_alters.push(Finding {
                        file: relative.clone(),
                        line,
                        what: format!(
                            "`{declared}` pins `{}`, which no migration here creates",
                            empty.key()
                        ),
                        fix: "an `ALTER FUNCTION` names a routine that already exists, so \
                              this one either names it differently from the `CREATE` \
                              that made it — a schema written on one and left off the \
                              other — or names nothing at all. Spell the two the same \
                              way; the pin lands on nothing as written, and the \
                              function it was meant for is left resolving names \
                              through the caller's session"
                            .to_owned(),
                    });
                }
                std::collections::btree_map::Entry::Vacant(empty) => {
                    empty.insert((relative.clone(), declared, line, pinned));
                }
            }
            if let Some(written) = written
                && !written.is_empty()
                && !crate::check::pg_temp_is_last(&written, schema)
            {
                written_paths.push(Finding {
                    file: relative.clone(),
                    line,
                    what: format!(
                        "this writes `search_path = {written}`, which is not one real \
                         schema followed by `pg_temp`"
                    ),
                    fix: "PostgreSQL searches the temporary schema first unless it is \
                          listed, and listed last; and an element that stands for a \
                          schema rather than naming one — `$user` — is expanded afresh \
                          on every call. The migrations write `SET search_path FROM \
                          CURRENT` with one resolved schema on the path, which is the \
                          shape this rule is about"
                        .to_owned(),
                });
            }
        }
    }
    let mut findings = written_paths;
    findings.append(&mut findings_about_alters);
    let functions = latest.len();
    {
        for (relative, declared, line, pinned) in latest.into_values() {
            // **The clause, not the substring.** `"reset search_path"` contains
            // `"set search_path"`, so read as a substring, a migration that
            // *lifts* a pin reads as one that adds it: one `ALTER FUNCTION …
            // RESET search_path` gives
            // `ok 4 function(s) in migrations pin how they resolve a name` with
            // the database holding a null `proconfig` for that function.
            // `SET search_path TO DEFAULT` is the same shape, and
            // `check --offline` — which `just check-offline` runs — never reaches
            // the database half that objects.
            if !pinned {
                findings.push(Finding {
                    file: relative.clone(),
                    line,
                    what: format!(
                        "`{declared}` does not pin `search_path`, so a name in its \
                         body resolves through whatever the caller's session says"
                    ),
                    fix: "add `SET search_path FROM CURRENT` to the declaration, with \
                          the migration having put one real schema and then `pg_temp` on \
                          the path. `pg_temp` last is deliberate — left out, it is \
                          searched first — and every other element has to be a schema \
                          name rather than something expanded when the function runs"
                        .to_owned(),
                });
            }
        }
    }
    Ok((findings, functions))
}

/// The `search_path` a declaration writes out, when it writes one.
///
/// `FROM CURRENT` carries no value to judge — that one is the database half's,
/// and it is the shape the migrations here actually write — so it answers
/// `None`, as does a declaration with no `SET search_path` in it at all.
///
/// Its own function because the loop that calls it is at clippy's line limit,
/// the same reason its neighbours are functions.
fn the_path_written_in(declared: &str) -> Option<String> {
    declared
        .split_once("set search_path")
        .map(|(_, rest)| rest.trim_start())
        .filter(|rest| !rest.starts_with("from current"))
        .map(|rest| {
            rest.trim_start_matches(['=', ' '])
                .trim_start_matches("to ")
                .split(" as ")
                .next()
                .unwrap_or_default()
                .trim()
                .trim_end_matches(';')
                .to_owned()
        })
}

/// Each statement of a migration, with the byte it starts at.
///
/// A statement ends at a semicolon that is not inside a dollar-quoted body or a
/// string literal — which the caller has already blanked, so a plain scan is
/// enough here.
fn statements_of(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut from = 0_usize;
    for (at, character) in text.char_indices() {
        if character == ';' {
            found.push((from, text[from..at].to_owned()));
            from = at + 1;
        }
    }
    if from < text.len() {
        found.push((from, text[from..].to_owned()));
    }
    found
}

/// Each `CREATE FUNCTION` in a migration: its declaration, its body, its line.
///
/// The declaration is everything up to the body's opening dollar quote, cased
/// down; the body is what the dollar quote encloses. Written over the whole file
/// rather than line by line because a declaration is spread across lines here and
/// `SET search_path` is on one of its own.
fn functions_in(text: &str) -> Vec<(String, String, String, usize, bool)> {
    let mut found: Vec<(usize, String, String, String, usize, bool)> = Vec::new();
    functions_and_alters(text, &mut found);
    // **Source order.** Gathered in separate passes, the `CREATE` always arrives
    // after the `ALTER` within one file and overwrites its pin — a function
    // pinned two lines below its own definition is reported unpinned, and the
    // class's own fix would pass only because its `ALTER` is in a later file.
    found.sort_by_key(|(at, ..)| *at);
    found
        .into_iter()
        .map(|(_, name, declared, body, line, is_alter)| (name, declared, body, line, is_alter))
        .collect()
}

/// Every function declaration in one migration, with its byte offset.
fn functions_and_alters(text: &str, found: &mut Vec<(usize, String, String, String, usize, bool)>) {
    // **ASCII lowercase, because `to_lowercase` does not preserve byte length.**
    // `strip_sql_comments` blanks rather than removes precisely so offsets found
    // in one string can index the other — and `to_lowercase()` breaks that
    // invariant: `U+0130` is two bytes and lowercases to three. Measured with
    // it, one migration with twenty-one of them in a string literal panics
    // `cargo xtask check` outright (`start byte index 760 is out of bounds for
    // string of length 678`), and with thirty-eight it silently misses an
    // unpinned function. SQL keywords are ASCII, which is all this matching
    // needs.
    let lowered = strip_sql_comments(text).to_ascii_lowercase();
    // An `ALTER FUNCTION … SET search_path` is a declaration with no body: it
    // pins an existing function without redefining it, which is what the
    // eleventh migration does to the one function that reads nothing at all.
    // **Over the statement, not the line.** A line-scoped scan misses
    // `ALTER FUNCTION` with the name on the next line — ordinary formatting —
    // and the pin is lifted with nothing reported. Measured, both shapes:
    //
    // ```text
    // ALTER FUNCTION\n    f()\n    RESET search_path;   -> ok 3 function(s) …
    // ALTER FUNCTION f()\n    RESET search_path;         -> ok 3 function(s) …
    // ALTER FUNCTION f() RESET search_path;              -> reported
    // ```
    //
    // The statement is what a semicolon ends, so that is the unit.
    for (at, statement) in statements_of(&lowered) {
        let trimmed = statement.split_whitespace().collect::<Vec<_>>().join(" ");
        let trimmed = trimmed.as_str();
        let index = lowered[..at].lines().count();
        // **Tokenised, and `ALTER ROUTINE` too.** `strip_prefix("alter function ")`
        // line by line misses `ALTER  FUNCTION` with two spaces, a line break
        // after the keyword, and `ALTER ROUTINE` — which is PostgreSQL's own
        // spelling for the same operation. Measured with that reader, all three
        // lift a pin with the static half reporting nothing; only the database
        // half catches them, and `check --offline` never runs that.
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if matches!(words.first().copied(), Some("alter"))
            && matches!(
                words.get(1).copied(),
                Some("function" | "procedure" | "routine")
            )
        {
            let name = the_routine_name_in(&words[2.min(words.len())..].join(" "));
            found.push((at, name, trimmed.to_owned(), String::new(), index + 1, true));
        }
    }
    let mut at = 0_usize;
    // **The earliest of the two forms, not the first form that matches
    // anywhere.** Trying `create function` first, with `or_else` only when it
    // finds nothing, fails because it is not a substring of `create or replace
    // function`: one bare `CREATE FUNCTION` at the bottom of a file wins the
    // search and every `CREATE OR REPLACE` above it is jumped over. Measured
    // with that order: a migration declaring two functions, one unpinned,
    // reports `ok 4 function(s) …` — three plus one — while the identical
    // unpinned function alone in a file is caught, so under it adding a line
    // below a function turns the gate off for it.
    // **And procedures, which both halves of the gate read.** A
    // `CREATE PROCEDURE` reading an unqualified name is the same defect with a
    // different keyword — measured, one in a migration gives
    // `ok 3 function(s) in migrations pin how they resolve a name`, a null
    // `proconfig`, and a `CALL` that reads a planted temporary table. A
    // database half filtering on `prokind = 'f'` misses it for the same reason.
    // **And through `routine_at`, which the parser above also uses.** Four
    // phrases each requiring a literal single space between their keywords
    // fail where SQL does not — a line break after `CREATE` makes this count
    // and that parser miss the same statement, and `found != creates` is then
    // satisfied by their agreeing about nothing.
    while let Some((start, _)) = routine_at(&lowered, at) {
        // The dollar quote that opens the body, and the matching one that closes
        // it. `$fn$`, `$$` and `$sql$` all appear in this tree.
        let rest = &lowered[start..];
        // A body may also be an ordinary string literal: read for dollar quotes
        // alone, a migration declaring a fourth function with a `'…'` body,
        // reading `journal_entries` unqualified and pinning nothing, leaves the
        // count at three and the verdict `ok`, measured. A parser that silently skips what
        // it does not understand is a walk that narrows itself.
        // **Bounded by the statement, because a function may have no body quote
        // at all.** PostgreSQL 14's `LANGUAGE sql … RETURN …;` is a whole
        // definition with no `$$` and no `'…'`, and an unbounded search runs
        // forward to the *next* function's quote — so the declaration swallows
        // everything in between, including that function's `SET search_path`, and
        // the loop then jumps past both. Measured: a file declaring two
        // functions, the first unpinned and bodyless, reports
        // `ok 4 function(s) …` where there are two, with no finding.
        //
        // A quote after the statement's terminating semicolon belongs to the next
        // statement, so the search stops there.
        let ends = rest.find(';').unwrap_or(rest.len());
        // **The body opens after `AS`, not at the statement's first quote.**
        // With whichever of `$` and `'` comes first deciding, a parameter with a
        // text default — `note TEXT DEFAULT ''`, ordinary SQL — takes the `'`
        // branch and brackets the declaration against a quote in the *signature*.
        // The real `$tag$…$tag$` body is then folded into `declared`, and
        // `declared.contains("set search_path")` is satisfied by a string
        // literal inside the function's own body: measured,
        // `ok 4 function(s) in migrations pin how they resolve a name` while the
        // database holds a null `proconfig` for it. This is the same defect as the
        // one `strip_sql_comments` carries a paragraph about — a pin inside a
        // comment reading as a real pin — one quoting form over.
        //
        // PostgreSQL opens a body with `AS`, and a parameter list is before it.
        let opens = the_body_opener_in(&rest[..ends]);
        let (tag, body_from) = match (
            rest[opens..ends].find('$').map(|at| at + opens),
            rest[opens..ends].find('\'').map(|at| at + opens),
        ) {
            (Some(dollar), quote) if quote.is_none_or(|quote| dollar < quote) => {
                let Some(tag_end) = rest[dollar + 1..].find('$') else {
                    break;
                };
                let tag = &rest[dollar..dollar + tag_end + 2];
                (tag, dollar + tag.len())
            }
            (_, Some(quote)) => ("'", quote + 1),
            // No body quote inside the statement: the definition is the statement,
            // and there is nothing to bracket.
            (_, None) => ("", ends),
        };
        let body = if tag.is_empty() {
            ""
        } else {
            rest[body_from..]
                .find(tag)
                .map_or("", |close| &rest[body_from..body_from + close])
        };
        // **The whole statement minus the body, not the run up to the body.**
        // PostgreSQL allows the option list after `AS …` — `0001` itself writes
        // `LANGUAGE plpgsql` there — so a reader of the run up to the body never
        // reads a pin written after it: measured, a function whose
        // `SET search_path FROM CURRENT` follows its `$$…$$` is reported as
        // unpinned while the database holds
        // the setting. A false finding is what this module's header calls worse
        // than no gate.
        let after_body = if tag.is_empty() {
            String::new()
        } else {
            let from = body_from + body.len() + tag.len();
            let rest = lowered.get(start + from..).unwrap_or_default();
            rest[..rest.find(';').map_or(rest.len(), |end| end + 1)].to_owned()
        };
        let declared = lowered[start..start + body_from.saturating_sub(tag.len())]
            .split_whitespace()
            .chain(after_body.split_whitespace())
            .collect::<Vec<_>>()
            .join(" ");
        // **Name and arguments, or overloads collapse into one entry.** The
        // map is keyed on what this returns, and truncating at `(` makes
        // `f(a INT)` and `f(a TEXT)` one function: measured, two definitions with
        // the second pinned give `count=1 findings=0` while the database holds an
        // unpinned one. A count that is the map it judges cannot see that, which
        // is the shape this module keeps finding.
        // **Name and the whole argument list, with the spaces out.** `declared` is
        // whitespace-joined, so taking one token stops at the first space
        // *inside* the parentheses: `f(a INT)` and `f(a TEXT)` both key to
        // `f(a`, and two overloads collapse into one entry with the pinned one
        // winning. Measured — the migrations' own functions all take `()`, so
        // no migration in the tree exercises it.
        let after = declared
            .split_whitespace()
            .skip(usize::from(declared.starts_with("create or replace")) * 2 + 2)
            .collect::<Vec<_>>()
            .join(" ");
        let name = the_routine_name_in(&after);
        found.push((
            start,
            name,
            declared,
            body.to_owned(),
            text[..start].lines().count() + 1,
            false,
        ));
        at = start + body_from + body.len().max(1);
        // A bodyless definition has an empty body, so `body.is_empty()` would
        // read it as an `ALTER`; the offset is what tells them apart, and the
        // `latest` map below keys on the declaration rather than the body.
    }
}

/// `text` with every `/* … */` blanked, keeping its length and its line breaks.
///
/// Nesting is PostgreSQL's rule: `/*` inside a block comment opens another, and
/// the outer one ends only when every inner one has.
fn blank_block_comments(text: &str) -> String {
    // **Not inside a string, and not inside a body.** A scan for `/*` with no
    // notion of either lets one `/*` in a `RAISE EXCEPTION '…'` open a block
    // comment that never closes, and every declaration below it in the file is
    // invisible. Measured, a pinned function whose message holds `see the note /*
    // above` and an unpinned one below it reading `journal_entries`:
    //
    // ```text
    // ok    4 function(s) in migrations pin how they resolve a name   <- five declared
    // ```
    //
    // and CI cannot catch it: the workflow hands `check` an empty database, so
    // the half that asks `pg_proc` takes its "nothing applied" arm. `check
    // passed` over a migration that unpins the journal's density trigger.
    let bytes: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut depth = 0_usize;
    let mut at = 0_usize;
    while at < bytes.len() {
        // A dollar-quoted body: `$$`, `$fn$`, `$sql$`. Copied through whole,
        // because what is inside one is data however it reads.
        if depth == 0
            && bytes[at] == '$'
            && let Some(tag) = dollar_tag(&bytes[at..])
        {
            // Where the matching close ends, or the end of the text.
            let closing: Vec<char> = tag.chars().collect();
            let mut ends = at + tag.len();
            while ends + closing.len() <= bytes.len()
                && bytes[ends..ends + closing.len()] != closing[..]
            {
                ends += 1;
            }
            let ends = (ends + closing.len()).min(bytes.len());
            for character in &bytes[at..ends.min(bytes.len())] {
                out.push(*character);
            }
            at = ends;
            continue;
        }
        // **A string literal is blanked, not copied through.** Copying it keeps its
        // contents in the text every later pass reads — so a `--` inside one
        // blanks the rest of its line *including the statement's semicolon*, and
        // `statements_of` then merges the next statement into it, hiding an
        // `ALTER … RESET search_path` completely. And a `CREATE OR REPLACE
        // FUNCTION …` written inside a literal overwrites the real entry in the
        // last-wins map, masking a genuinely unpinned definition. Both measured:
        // `count=3 findings=0` where a finding is due.
        //
        // `unreadable_statements` refuses `EXECUTE`-built DDL for this same
        // reason — a name assembled at run time is not in the file. A name
        // *quoted* in the file is not a declaration either.
        // **A quoted identifier is not a string, and a `'` inside one is not a
        // quote.** PostgreSQL's `"…"` names a relation or a column, doubling the
        // quote to escape it — and read as a quote, a `'` inside one opens a
        // literal that runs to the next `'` anywhere in the file, blanking every
        // statement between them. It is copied through because a quoted
        // identifier *is* a declaration's name, unlike a string.
        if depth == 0 && bytes[at] == '"' {
            out.push('"');
            at += 1;
            while at < bytes.len() {
                if bytes[at] == '"' && bytes.get(at + 1) == Some(&'"') {
                    out.push_str("\"\"");
                    at += 2;
                    continue;
                }
                out.push(bytes[at]);
                at += 1;
                if bytes[at - 1] == '"' {
                    break;
                }
            }
            continue;
        }
        if depth == 0 && bytes[at] == '\'' {
            // **`E'…'` honours backslash escapes and `'…'` does not.** With
            // `standard_conforming_strings` `on` — measured on the developer's
            // cluster, PostgreSQL 18.6 — a backslash in an ordinary literal is a
            // backslash, so `''` is the only way out of one. An **escape
            // string** is the other form the standard leaves to the
            // implementation: `SELECT length(E'don\'t')` is 5, one five-
            // character string. A walk that knows only the doubling closes the
            // literal at the backslash-escaped quote, re-opens one at the real
            // closing quote, and blanks the rest of the file — a
            // `CREATE FUNCTION` after such a line is invisible to every later
            // pass. Measured against the running cluster, migration and all.
            let escapes = at > 0
                && matches!(bytes[at - 1], 'E' | 'e')
                && (at < 2 || !(bytes[at - 2].is_alphanumeric() || bytes[at - 2] == '_'));
            out.push('\'');
            at += 1;
            while at < bytes.len() {
                if escapes && bytes[at] == '\\' {
                    out.push_str("  ");
                    at += 2;
                    continue;
                }
                if bytes[at] == '\'' && bytes.get(at + 1) == Some(&'\'') {
                    out.push_str("  ");
                    at += 2;
                    continue;
                }
                if bytes[at] == '\'' {
                    out.push('\'');
                    at += 1;
                    break;
                }
                out.push(if bytes[at] == '\n' { '\n' } else { ' ' });
                at += 1;
            }
            continue;
        }
        // **And `--` here, where the quote state is known.** A `--` pass run
        // after this one over raw lines cannot tell a comment from a
        // dash inside a literal or a body.
        if depth == 0 && bytes[at] == '-' && bytes.get(at + 1) == Some(&'-') {
            while at < bytes.len() && bytes[at] != '\n' {
                out.push(' ');
                at += 1;
            }
            continue;
        }
        let opens = bytes[at] == '/' && bytes.get(at + 1) == Some(&'*');
        let closes = depth > 0 && bytes[at] == '*' && bytes.get(at + 1) == Some(&'/');
        if opens {
            depth += 1;
            out.push_str("  ");
            at += 2;
            continue;
        }
        if closes {
            depth -= 1;
            out.push_str("  ");
            at += 2;
            continue;
        }
        if depth > 0 && bytes[at] != '\n' {
            out.push(' ');
        } else {
            out.push(bytes[at]);
        }
        at += 1;
    }
    out
}

/// `text` with every `--` comment removed, line endings kept.
///
/// **Prose is not SQL.** `0006`'s body carries the sentence "An
/// UPDATE may change anything else about a row and never this", and unstripped,
/// the scan below reports `may` as an unqualified relation — a finding about a comment,
/// which is the shape this module's own header calls worse than no gate, because
/// it teaches a reader to scroll past the output.
fn strip_sql_comments(text: &str) -> String {
    // **Block comments too.** The declaration is built from this text, so
    // unstripped, `/* SET search_path FROM CURRENT */` above a
    // function *reads as a pin*: measured, a migration whose only pin is inside a
    // block comment gives `ok 4 function(s) in migrations pin how they resolve a
    // name` with `proconfig` null, and a temporary table then decides what that
    // function reads. In the other direction a whole `CREATE FUNCTION` commented
    // out as an example is reported as a real one — a false finding, which this
    // module's header calls worse than no gate. The sibling parser in this same
    // file carries `a_block_comment_hides_a_table_and_does_not_invent_one`
    // for exactly these two failures.
    let text = &blank_block_comments(text);
    // **Blanked, not removed, and the difference is a panic.** `functions_in`
    // searches the stripped text and then indexes the original with what it
    // found, so a strip that shortens lines makes every offset past the first
    // comment wrong: measured, `end byte index 2808 is out of bounds for string
    // of length 1513`. Spaces keep the two the same length.
    text.lines()
        .map(|line| {
            line.split_once("--").map_or_else(
                || line.to_owned(),
                |(before, rest)| {
                    let mut kept = before.to_owned();
                    kept.push_str(&" ".repeat(rest.len() + 2));
                    kept
                },
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every migration is numbered once, and the numbers run without a gap.
///
/// **`sqlx`'s migrator does not check this.** Measured against a live
/// database:
///
/// | what was added | what happened |
/// |---|---|
/// | a migration **edited** after being applied | correctly refused |
/// | a number **skipped** | applied, no comment |
/// | a **lower** number added after higher ones ran | **applied out of order, in silence** |
/// | **two files sharing a number**, fresh database | one applied, the other silently never ran, behind an error naming an sqlx primary key |
///
/// The last is the worst: a half-migrated schema, and a message about
/// `_sqlx_migrations_pkey` rather than about the two files that collide. So the
/// numbering is checked here, before anything is applied, where the diagnostic
/// can name both files.
///
/// # Errors
///
/// Returns why the migrations could not be read.
pub(crate) fn every_migration_is_numbered_once(
    root: &Path,
    directory: &str,
) -> Result<(Vec<Finding>, usize), String> {
    let files = migrations_in(root, directory)?;
    let mut findings = Vec::new();
    let mut numbers: Vec<(u64, String)> = Vec::new();

    for path in &files {
        let relative = relative_to(root, path);
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        // **Forward-only, as a gate and not only a claim.** `sqlx` reads a
        // reversal named for the migration it undoes, and `sqlx migrate revert`
        // runs it; `NOCTUA.md §21.7` says migrations here are applied forward
        // and never reversed. Measured without this, one such file holding
        // `DROP TABLE journal_entries CASCADE`: `files=7 findings=0`,
        // `Migrator::new` accepts the set, and `cargo xtask dev` would print
        // `7 migration(s) applied` for six that ran. A reversal of the
        // append-only audit log is exactly the thing that must not be one
        // statement away.
        if name.ends_with(".down.sql") {
            findings.push(Finding {
                file: relative.clone(),
                line: 1,
                what: "this is a reversal, and migrations here are forward-only".to_owned(),
                fix: "`sqlx` reads a reversal named for the migration it undoes, and \
                      `sqlx migrate revert` runs it — so a file named this way is a \
                      reversal whether or not anybody meant one, and the journal's table \
                      is append-only by trigger precisely so that undoing it takes a \
                      decision with a person in it (NOCTUA.md §18.4). Write the change \
                      as a new forward migration"
                    .to_owned(),
            });
            continue;
        }
        let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
        match digits.parse::<u64>() {
            Ok(number) if !digits.is_empty() => numbers.push((number, relative)),
            _ => findings.push(Finding {
                file: relative,
                line: 1,
                what: "this migration does not begin with a number".to_owned(),
                fix: "migrations are applied in numeric order and recorded by number, so \
                      one without a number is one nothing can order or record. Name it \
                      `NNNN_what_it_does.sql` <!-- allow-reference: names `NNNN_what_it_does.sql` — the shape of a name, not a file -->"
                    .to_owned(),
            }),
        }
    }

    numbers.sort();
    for pair in numbers.windows(2) {
        let (before, earlier) = &pair[0];
        let (after, later) = &pair[1];
        if before == after {
            findings.push(Finding {
                file: later.clone(),
                line: 1,
                what: format!("`{earlier}` already has number {before}"),
                fix: "two migrations with one number are recorded as one: the first to run \
                      takes the number and the second never runs at all, leaving a \
                      half-migrated schema behind a message about an sqlx primary key. \
                      Give this one the next free number"
                    .to_owned(),
            });
        } else if *after > before + 1 {
            findings.push(Finding {
                file: later.clone(),
                line: 1,
                what: format!(
                    "number {after} follows {before}, so {} is missing",
                    before + 1
                ),
                fix: "a gap means either a migration was deleted after being applied \
                      somewhere — in which case that database and this tree disagree about \
                      what ran — or the next number was guessed. Neither is something to \
                      discover during a deployment"
                    .to_owned(),
            });
        }
    }

    Ok((findings, files.len()))
}

/// A table a migration creates, and the line its statement begins on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Created {
    /// The table's name, as the catalog would have to spell it.
    pub(crate) name: String,
    /// Every column the statement declares, lowercased.
    ///
    /// **Read here rather than asked of a database.** `order_by` is written into
    /// the pruner's `ORDER BY`. Checked against `information_schema.columns` on
    /// whoever's machine runs the gate, it would ratify drift (a column added by
    /// hand and never written into a migration satisfies such a check), be
    /// satisfiable by a table of the same bare name in an unrelated schema, and
    /// never pass on the empty database CI starts. The migrations are what
    /// builds every database there will ever be, so they are what the catalog
    /// has to agree with.
    ///
    /// Empty when the statement declares no column list — `CREATE TABLE … AS
    /// SELECT` and `SELECT … INTO` name their columns in the query — and the
    /// check below treats that as "cannot say" rather than as "no columns".
    pub(crate) columns: Vec<String>,
    /// Which of them cannot hold NULL.
    ///
    /// The pruner writes `ORDER BY <column> ASC` and PostgreSQL sorts NULLs
    /// *last* under `ASC`, so a nullable ordering column makes every row missing
    /// that value the newest rows in the table and the last ever removed.
    pub(crate) non_null: Vec<String>,
    /// Each column's declared type, lowercased, in declaration order.
    ///
    /// **What a column sorts like, which decides what "oldest" means.**
    /// A `text` column holding a Crockford identifier is variable width as an
    /// ordering column — twelve characters below `32^12` and thirteen above,
    /// from about 2034. A `text` sort is
    /// byte-lexicographic, so from that day ascending order is not creation
    /// order, and the pruner deletes the newest rows while reporting an
    /// ordinary run. Measured over six identifiers minted in order: it removes
    /// the oldest and the third-newest.
    pub(crate) types: Vec<(String, String)>,
    /// One-based, so a finding points where a reader can click.
    pub(crate) line: usize,
    /// Whether another statement shares the line this one starts on.
    ///
    /// The marker that excuses a table names *a* statement, and two statements
    /// on one line cannot both be the one it meant.
    pub(crate) shares_its_line: bool,
    /// Whether its statement declares this table and no other.
    ///
    /// **A marker excuses one table.** `CREATE SCHEMA … CREATE TABLE a …
    /// CREATE TABLE b` is one statement declaring two, so a marker above it
    /// cannot say which it meant — and excusing both lets an
    /// unbounded table ride in beside a genuinely bounded one.
    pub(crate) alone_in_its_statement: bool,
    /// Where this sits among every statement the migrations run.
    pub(crate) at: usize,
    /// Whether the statement that decided this sat inside a routine body.
    ///
    /// **`at` is a running order within one level and not across them.** `cut`
    /// finishes the top level and then re-cuts the routine bodies, so every
    /// nested statement is numbered after every top-level one however the file
    /// is written. `hold_the_path` refuses to judge a name written at both
    /// levels, in a paragraph that says why; the readers of *tables* and
    /// *columns* judge by the same order and need the same guard.
    ///
    /// Measured on PostgreSQL 18.6, both directions from that one root:
    ///
    /// ```text
    /// DO $$ BEGIN ALTER TABLE ar ALTER COLUMN recorded TYPE TEXT; END $$;
    /// ALTER TABLE ar ALTER COLUMN recorded TYPE BIGINT USING recorded::bigint;
    ///   -> the column is bigint; the gate refused the migration for `text`
    ///
    /// DROP TABLE ar;  DO $$ BEGIN CREATE TABLE ar (…); END $$;  DROP TABLE ar;
    ///   -> no such table; the gate found nothing and went on bounding it
    /// ```
    pub(crate) nested: bool,
}

/// A table one statement stops referring to, because it renamed it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Renamed {
    /// What it was called.
    pub(crate) from: String,
    /// What it is called now.
    pub(crate) to: String,
    /// One-based.
    pub(crate) line: usize,
    /// Where this sits among every statement the migrations run.
    ///
    /// **`Created` carries one, and so does this**, or the walk that reconciles
    /// them has nothing to order them by and reads every create in a file before
    /// every drop. A table dropped and re-created in one migration — ordinary
    /// work — would then end the file not existing.
    pub(crate) at: usize,
    /// Whether another statement shares the line this one starts on.
    pub(crate) shares_its_line: bool,
    /// Whether its statement brings this name into existence and no other.
    pub(crate) alone_in_its_statement: bool,
    /// Whether the statement that decided this sat inside a routine body.
    ///
    /// **`at` is a running order within one level and not across them.** `cut`
    /// finishes the top level and then re-cuts the routine bodies, so every
    /// nested statement is numbered after every top-level one however the file
    /// is written. `hold_the_path` refuses to judge a name written at both
    /// levels, in a paragraph that says why; the readers of *tables* and
    /// *columns* judge by the same order and need the same guard.
    ///
    /// Measured on PostgreSQL 18.6, both directions from that one root:
    ///
    /// ```text
    /// DO $$ BEGIN ALTER TABLE ar ALTER COLUMN recorded TYPE TEXT; END $$;
    /// ALTER TABLE ar ALTER COLUMN recorded TYPE BIGINT USING recorded::bigint;
    ///   -> the column is bigint; the gate refused the migration for `text`
    ///
    /// DROP TABLE ar;  DO $$ BEGIN CREATE TABLE ar (…); END $$;  DROP TABLE ar;
    ///   -> no such table; the gate found nothing and went on bounding it
    /// ```
    pub(crate) nested: bool,
}

/// What one migration file does to the set of tables that exist.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Changes {
    /// Every table it creates.
    pub(crate) created: Vec<Created>,
    /// Every table it renames.
    pub(crate) renamed: Vec<Renamed>,
    /// Every column an `ALTER TABLE` adds, drops or renames.
    ///
    /// **Not only `CREATE TABLE`.** Reading only that, a migration holding
    /// `ALTER TABLE access_records RENAME COLUMN id TO record_id` leaves
    /// `cargo xtask check` green, and the pruner's next `ORDER BY "id"` fails
    /// with `column "id" does not exist` — precisely the failure the `order_by`
    /// check exists to prevent. Reading the migrations is right; reading only one of
    /// their statements is not.
    pub(crate) columns: Vec<ColumnChange>,
    /// How many statements the file held, so the caller can keep `at` rising
    /// across files.
    pub(crate) statements: usize,
    /// How many tokens those statements hold.
    ///
    /// A count of statements says how much of a file was *cut*; this says how
    /// much of it was there to read. A rule handed an empty token list judges
    /// nothing while the statement count stands exactly where it was.
    pub(crate) tokens: usize,
    /// Every domain it declares, and the type each is built on.
    ///
    /// `CREATE DOMAIN epoch_millis AS BIGINT` is the idiomatic way to declare an
    /// ordering column, and a type rule knowing only names refuses it for not
    /// being one of them — with no way to permit it from `catalog/limits.toml`, in
    /// a repository whose principle is that limits are data. The migrations
    /// declare the domain and the migrations are what this gate reads, so it
    /// follows the declaration rather than growing a second escape hatch.
    pub(crate) domains: Vec<(String, String)>,
}

/// A column an `ALTER TABLE` adds to, removes from, or renames on a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ColumnChange {
    /// The table it changes, as the catalog would spell it.
    pub(crate) table: String,
    /// The column that is gone afterwards, if one is.
    pub(crate) removed: Option<String>,
    /// The column that exists afterwards, if one does.
    pub(crate) added: Option<String>,
    /// The type it is given, when the statement gives one.
    ///
    /// `ADD COLUMN <name> <type>` and `ALTER COLUMN <name> TYPE <type>` both
    /// decide how a column sorts: a type rule asking the `CREATE TABLE` and
    /// nothing else is turned off by one `ALTER COLUMN occurred_at TYPE
    /// TEXT`. Measured — the same type change written two ways,
    /// same database, same resulting schema:
    ///
    /// ```text
    /// in the CREATE:  error `access_records` orders by `occurred_at`, which is `text`
    /// in an ALTER:    check passed
    /// ```
    pub(crate) kind: Option<String>,
    /// Whether it can hold NULL afterwards, when the statement decides.
    pub(crate) non_null: Option<bool>,
    /// Where this sits among every statement the migrations run, so a column
    /// dropped and re-declared is read in the order it happened.
    pub(crate) at: usize,
    /// Which clause this was, because two of them have the same fields.
    pub(crate) how: How,
    /// Whether the statement that decided this sat inside a routine body.
    ///
    /// **`at` is a running order within one level and not across them.** `cut`
    /// finishes the top level and then re-cuts the routine bodies, so every
    /// nested statement is numbered after every top-level one however the file
    /// is written. `hold_the_path` refuses to judge a name written at both
    /// levels, in a paragraph that says why; the readers of *tables* and
    /// *columns* judge by the same order and need the same guard.
    ///
    /// Measured on PostgreSQL 18.6, both directions from that one root:
    ///
    /// ```text
    /// DO $$ BEGIN ALTER TABLE ar ALTER COLUMN recorded TYPE TEXT; END $$;
    /// ALTER TABLE ar ALTER COLUMN recorded TYPE BIGINT USING recorded::bigint;
    ///   -> the column is bigint; the gate refused the migration for `text`
    ///
    /// DROP TABLE ar;  DO $$ BEGIN CREATE TABLE ar (…); END $$;  DROP TABLE ar;
    ///   -> no such table; the gate found nothing and went on bounding it
    /// ```
    pub(crate) nested: bool,
}

/// What an `ALTER TABLE` clause did to a column.
///
/// **`Added` and `Altered` are the same three fields and not the same event**,
/// and telling them apart by which of `kind`/`non_null` happens to be `Some` is
/// a rule about a coincidence. An `ADD COLUMN` replaces what is known about a
/// name; an `ALTER COLUMN` amends it; a `Renamed` *moves* what is known from one
/// name to another — without that, renaming a different column into the
/// ordering column's name leaves the old column's type standing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum How {
    /// `ADD COLUMN`: the name is new, and this says what it is.
    Added,
    /// `ALTER COLUMN`: the name exists, and this changes one thing about it.
    Altered,
    /// `RENAME COLUMN`: the name changes and everything else moves with it.
    Renamed,
    /// `DROP COLUMN`: the name is gone.
    Dropped,
}

/// Everything one migration file does to the set of tables.
///
/// **Statements, not lines**, and the statements are cut by a single walk that
/// knows every way PostgreSQL quotes text. Four ordinary shapes of valid SQL
/// go through a line-by-line reader with `cargo xtask check` green, and each
/// is a table a live PostgreSQL then creates:
///
/// * an `E'…\'…'` escape string, whose backslash-escaped quote closes the
///   literal early — the `/*` that follows becomes a real block comment, nothing
///   closes it, and **the rest of the file is blanked**, `CREATE TABLE` and
///   all;
/// * a `DO $$ … CREATE TABLE … $$` block, the most idiomatic conditional DDL
///   there is, which a line reader copies verbatim and never looks inside;
/// * `SELECT … INTO login_attempts FROM …`, which creates a table without the
///   word `CREATE` anywhere;
/// * `ALTER TABLE access_records RENAME TO login_attempts`, after which the
///   bounded entry names a table that no longer exists and the new one is
///   unbounded — and neither direction of the gate notices.
///
/// A fifth shape produces a **finding naming a table nobody wrote**:
/// `CREATE TABLE "a;b" (…)` is cut at the `;` inside its quoted identifier and
/// reported as `` `a;` ``. That is criterion 5's second half — *the error names
/// the table* — failing, which is the one thing it must not do.
#[cfg(test)]
pub(crate) fn changes_in(text: &str) -> Changes {
    changes_in_as(text, Dialect::PostgreSql)
}

/// The same, read as `dialect` cuts it.
pub(crate) fn changes_in_as(text: &str, dialect: Dialect) -> Changes {
    let mut changes = Changes::default();
    let all = statements_in(text, dialect);
    changes.statements = all.len();
    for (at, statement) in all.into_iter().enumerate() {
        changes.columns.extend(
            column_changes(&statement)
                .into_iter()
                .map(|change| ColumnChange {
                    at,
                    nested: statement.nested,
                    ..change
                }),
        );
        let names = created_tables(&statement);
        let alone = names.len() == 1;
        // **Only when the statement declares one table.** A column list
        // computed once and cloned to every name gives both tables of
        // `CREATE SCHEMA archive CREATE TABLE access_records (…) CREATE TABLE
        // journal_entries (…)` the first one's columns — and a
        // schema-qualified entry is a shape both `limits.rs` and
        // `nc-prune::identifier` explicitly support.
        let columns = if alone {
            declared_columns(&statement)
        } else {
            Vec::new()
        };
        let non_null = if alone {
            columns_that_cannot_be_null(&statement)
        } else {
            Vec::new()
        };
        let types = if alone {
            declared_types(&statement)
        } else {
            Vec::new()
        };
        for name in names {
            changes.created.push(Created {
                name,
                types: types.clone(),
                non_null: non_null.clone(),
                columns: columns.clone(),
                line: statement.line,
                shares_its_line: statement.shares_its_line,
                alone_in_its_statement: alone,
                at,
                nested: statement.nested,
            });
        }
        if let Some(domain) = declared_domain(&statement) {
            changes.domains.push(domain);
        }
        if let Some((from, to)) = renamed_table(&statement) {
            changes.renamed.push(Renamed {
                from,
                to,
                line: statement.line,
                at,
                shares_its_line: statement.shares_its_line,
                alone_in_its_statement: true,
                nested: statement.nested,
            });
        }
        // A dropped table is retired and nothing takes its place: `to` is empty,
        // which the caller reads as "this name no longer exists".
        let dropped = dropped_tables(&statement);
        let alone = dropped.len() == 1;
        for from in dropped {
            changes.renamed.push(Renamed {
                from,
                to: String::new(),
                line: statement.line,
                at,
                shares_its_line: statement.shares_its_line,
                alone_in_its_statement: alone,
                nested: statement.nested,
            });
        }
        // **The last statement a verdict reaches, and it has to be in *this*
        // loop.** Folded over `all` above the loop, the fingerprint makes both
        // readers of a file measure what the *cutter* produces, and neither
        // can see this walk — the one that finds the tables — stop inside it.
        // Measured with it folded there: `if at >= 8 { continue; }` as the
        // loop's first statement then leaves the whole suite passing and
        // `check passed`, with a real unbounded `CREATE TABLE` sitting past the
        // eighth statement of a migration and the two numbers equal to the
        // byte. The sibling reader
        // `unreadable_statements` is held the same way, for the same defect.
        //
        // Wrapping, because what is carried is a fingerprint and not a quantity.
        changes.tokens = changes
            .tokens
            .wrapping_add(the_weight_of(&statement.tokens));
    }
    changes
}

/// Every table one migration file creates. Kept for the tests that only care
/// about that half.
#[cfg(test)]
pub(crate) fn tables_created_in(text: &str) -> Vec<Created> {
    changes_in(text).created
}

/// One statement, with its comments removed and its quoting intact.
struct Statement {
    /// The statement's tokens, in order.
    tokens: Vec<Token>,
    line: usize,
    shares_its_line: bool,
    /// The last line holding any of its code, so the next statement can tell
    /// whether it begins on a line this one already used.
    ends_on: usize,
    /// Whether it came from inside a dollar-quoted routine body.
    nested: bool,
    /// Where it stands in the file, as a path down through the routine bodies.
    ///
    /// A top-level statement is `[k]`; the statements of a body opened by that
    /// statement are `[k, 0]`, `[k, 1]`, and so on, however deep the nesting
    /// goes. Ordering by this is depth-first, which is the order the statements
    /// run in — and it is a total order, which the line alone is not.
    path: Vec<usize>,
    /// The dollar-quoted bodies inside it, and the line each begins on.
    bodies: Vec<(String, usize)>,
}

/// Whether a statement redirects `search_path`, in either of its two forms.
///
/// Split out because the function that calls it is at a hundred lines, and
/// because the two answers are one question: SQL spells this several ways, and
/// a rule written for one spelling is wrong about the others.
fn the_path_redirects_in(
    tokens: &[Token],
    at_the_name: Option<usize>,
    joined: Option<usize>,
    assigns: &dyn Fn(usize) -> bool,
    folded: &dyn Fn(usize) -> Option<String>,
    settings: &Settings<'_>,
) -> (bool, bool, bool) {
    // **`SET` anywhere in the statement, not only as its first word.**
    // `ALTER DATABASE noctua SET search_path TO archive` and the `ALTER ROLE`
    // form say the same thing and outlive the migration; `RESET search_path`
    // puts the session back to a path this gate has not read. Requiring the
    // statement to *open* with `SET` reads none of the three.
    // **An `UPDATE`'s `SET` names a column, not a setting.** The allowed list
    // already blesses a table with a column called `search_path` read in a
    // `CREATE VIEW` and constrained in an `ALTER TABLE`; *writing* to that
    // column is the third shape of the same thing, and a `SET` anywhere refuses
    // it — `UPDATE paths SET search_path = 'archive';`, which on PostgreSQL 18.6
    // updates one row and leaves `SHOW search_path` exactly as it was.
    //
    // *No statement writes a session setting and rows* is false:
    // `WITH bumped AS (UPDATE paths SET n = n + 1
    // RETURNING n) SELECT set_config('search_path','archive',false) FROM bumped;`
    // does both, measured. It is harmless only because `sets_by_function` is
    // deliberately not gated by this hatch.
    //
    // `ON CONFLICT … DO UPDATE SET` is the same clause reached from an `INSERT`,
    // which is why the token is looked for anywhere rather than at the front.
    //
    // **Where the word stands, not whether it is present.** As "`UPDATE`
    // anywhere", this is a hatch two real statements walk through. `UPDATE`
    // is an *unreserved* keyword on PostgreSQL 18.6, so `CREATE SCHEMA update;
    // SET search_path = update;` names a schema with it and is accepted — the
    // path moves and an unqualified `CREATE TABLE` lands there. And PL/pgSQL
    // puts an `UPDATE` and a later `SET` in one `;`-delimited chunk:
    // `DO $$ BEGIN FOR r IN UPDATE paths SET n = n + 1 RETURNING * LOOP
    // SET search_path = archive; END LOOP; END $$;` leaves the path changed
    // after the block, measured.
    //
    // The word opens a statement or it is a schema name, a column, a row's
    // value. `opens_a_statement` is the same test the cutter already uses.
    //
    // **And `opens_a_statement` is not the whole test, in either direction.**
    // `DO` is on that list because a `DO $$…$$` block opens one — but
    // `ON CONFLICT … DO UPDATE SET` and `MERGE … WHEN MATCHED THEN UPDATE SET`
    // are *clauses* of the statement above them, and inside a `FOR … IN` loop
    // either one turns this hatch on for a chunk that also holds a
    // `SET search_path`. Measured on PostgreSQL 18.6: the path moves and an
    // unqualified `CREATE TABLE` lands in the new schema, with the gate silent.
    // What tells them apart is the word before the opener.
    //
    // The other direction is a `(`: a data-modifying CTE —
    // `WITH bumped AS (UPDATE paths SET search_path = 'a' RETURNING id) …` —
    // opens a statement there, leaves `SHOW search_path` untouched (measured),
    // and without this arm is *refused*. That is the fifth spelling of "a column may be called
    // `search_path`", beside the four the allowed list already carries.
    //
    // **And it clears the segment it owns, not the whole chunk.** A `(` arm
    // answering for every token in a `;`-free block meets PL/pgSQL's
    // `FOR r IN <query> LOOP <stmt>;` header, which holds no `;` — so a CTE's
    // `(UPDATE …)` in the header turns the rule off for a `SET search_path`
    // in the *loop body*. Measured on PostgreSQL 18.6: the path moves and an
    // unqualified `CREATE TABLE` lands in the new schema. Deleting the arm is
    // not the answer — the top-level CTE beside it in the allowed list is a
    // shape this repository writes — so what the arm clears is bounded the way
    // the `DO`/`THEN` arm's is: by where the statement openers fall.
    let segment_of = |upto: usize| -> usize {
        tokens[..upto.min(tokens.len())]
            .iter()
            .filter(|token| opens_a_statement(token))
            .count()
    };
    let opens_an_update = |at: usize, token: &Token| {
        if !token.is("UPDATE") {
            return false;
        }
        let Some(before) = at.checked_sub(1).and_then(|before| tokens.get(before)) else {
            return true;
        };
        if matches!(before, Token::Punct('(')) {
            return true;
        }
        if !opens_a_statement(before) {
            return false;
        }
        // **And a conflict action is a row write like any other.** Refusing an
        // `UPDATE` opened by a `DO` or a `THEN` when a `CONFLICT` or a `MATCHED`
        // stands earlier — on the reasoning that those wear an opener's clothes
        // — refuses two ordinary upserts. Measured on PostgreSQL 18.6, both
        // `INSERT … ON CONFLICT (id) DO UPDATE SET search_path = 'archive'` and
        // `MERGE … WHEN MATCHED THEN UPDATE SET search_path = 'archive'` write
        // the column and leave `SHOW search_path` alone. A fixture blessing one
        // can pass for an unrelated reason: if its first `search_path`
        // token is the one in the `INSERT` column list, it is not followed
        // by an assignment, and with the column dropped from that list the
        // identical statement is refused.
        //
        // What separates a row write from a session write is the *segment*, and
        // that is asked below rather than here.
        true
    };
    let writing_rows: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(at, token)| opens_an_update(*at, token))
        .map(|(at, _)| segment_of(at))
        .collect();
    let assigns_a_column =
        |where_it_writes: usize| -> bool { writing_rows.contains(&segment_of(where_it_writes)) };
    let changes_a_setting = tokens
        .iter()
        .enumerate()
        .any(|(at, token)| (token.is("SET") || token.is("RESET")) && !assigns_a_column(at));
    // **And `RESET ALL`, which resets the path without naming it.** Asking for
    // a `RESET` *and* the name accepts the widest form of the statement it
    // refuses. Measured on PostgreSQL 18: `SET search_path =
    // r16probe; RESET ALL;` puts the path back to `"$user", public`, and the
    // journal's own tests connect with `options([("search_path", schema)])` —
    // one schema per test — so a `RESET ALL` in a migration aims every
    // unqualified statement after it at the developer's real `public` tables.
    //
    // The pin rule in this same file already carries the paragraph, and calls
    // `SET x TO v`, `SET x FROM CURRENT`, `RESET x` and `RESET ALL` "the closed
    // set rather than a list that grows". Without `RESET ALL` this rule knows
    // three of the four: one rule written twice, with the second copy narrower.
    let reset_the_path = tokens.iter().any(|token| token.is("RESET"))
        && (at_the_name.is_some()
            || joined.is_some()
            || tokens.iter().any(|token| token.is("ALL")));
    // **And the statements that re-aim the path without naming it.** The default
    // `search_path` begins `"$user"`, which PostgreSQL re-expands on every
    // statement — so `SET ROLE` moves where an unqualified `CREATE TABLE` lands
    // while `SHOW search_path` is unchanged. Measured on PostgreSQL 18: after
    // `SET ROLE r17role`, `SHOW search_path` still answers `"$user", public` and
    // `CREATE TABLE access_records_probe (id INT)` lands in `r17role`. This
    // gate's own `fix` says it "credits an unqualified name to `public`", and
    // that credit is what these statements make wrong.
    //
    // `SET SESSION AUTHORIZATION` and `RESET ROLE` are the same mechanism.
    let word = |at: usize| {
        tokens
            .get(at)
            .and_then(Token::name)
            .map(str::to_ascii_lowercase)
    };
    //
    // **And `session_authorization` is one token.** A rule knowing only the
    // two-word `SET SESSION AUTHORIZATION` misses the GUC spelling of the
    // very same statement — `SET session_authorization = archive`, which is how
    // `ALTER DATABASE … SET` must write it — which matches neither arm. Measured on
    // PostgreSQL 18.6: `SET LOCAL session_authorization = <role>` leaves
    // `SHOW search_path` at `"$user", public` and puts an unqualified
    // `CREATE TABLE access_records` in that role's schema; written as
    // `ALTER DATABASE … SET session_authorization`, it lands in
    // `pg_db_role_setting` and every future session resolves there.
    let a_role = |at: usize| {
        // A glued `=` keeps the name in its token: `search_path=archive` is
        // one word, which a reader of whole tokens walks past.
        let named = word(at).map(|word| word.split('=').next().unwrap_or_default().to_owned());
        matches!(named.as_deref(), Some("role" | "session_authorization"))
            || (matches!(named.as_deref(), Some("session"))
                && matches!(word(at + 1).as_deref(), Some("authorization")))
    };
    // **Anywhere in the statement, and in both spellings.** Written as "the
    // statement begins `SET` or `RESET`", it would be the mistake this same
    // walk records further down: a routine body's statements begin with
    // `BEGIN`, so wrapping one in a `DO` block moves it off position zero and a
    // first-token refusal never fires. Measured on PostgreSQL
    // 18: `DO $$ BEGIN SET ROLE r18; END $$;` leaves `current_user` changed after
    // the block and an unqualified `CREATE TABLE` lands in that role's schema.
    //
    // And `ALTER DATABASE … SET role = archive` writes `pg_db_role_setting`, so
    // *every future session* resolves there — measured in a brand-new session.
    // `ALTER ROLE … SET search_path` is in the refused list too, and one rule
    // written twice has to reach both.
    //
    // **`DISCARD ALL` is those statements written as one word.** It is
    // `SET SESSION AUTHORIZATION DEFAULT`, `RESET ALL` and more, and `RESET ALL`
    // is refused above, so this is too. Measured on
    // PostgreSQL 18.6: `SET search_path = 'pg_temp'; DISCARD ALL;` puts the path
    // back to `"$user", public`. It cannot run inside a transaction block, so it
    // needs a migration marked to run outside one — which sqlx supports and
    // nothing here forbids. Read anywhere in the statement rather than at token
    // zero, for the reason the paragraph below gives.
    let aims_the_path = (0..tokens.len()).any(|at| {
        !assigns_a_column(at)
            && ((matches!(word(at).as_deref(), Some("set" | "reset"))
                && (a_role(at + 1)
                    || (matches!(word(at + 1).as_deref(), Some("local" | "session"))
                        && a_role(at + 2))))
                || (matches!(word(at).as_deref(), Some("discard"))
                    && matches!(word(at + 1).as_deref(), Some("all"))))
    });
    let sets_the_path = aims_the_path
        || changes_a_setting
            && (at_the_name.is_some_and(assigns)
                || reset_the_path
                || joined.is_some_and(|at| {
                    // **The same rule as `assigns_a_new_path`**, where
                    // `current_schema` without parentheses is an identifier and
                    // not the function — so in its no-space form too,
                    // `search_path=current_schema` is a redirect.
                    // Measured on PostgreSQL 18: `CREATE SCHEMA "current_schema";
                    // SET search_path=current_schema;` puts a table in
                    // `current_schema`. There is nothing to excuse here either.
                    folded(at).is_some()
                }));
    // **Every `set_config`, and the whole of its first argument.** Taking the
    // *first* call in the statement and the *first* literal after it accepts
    // `set_config('search' || '_path', 'archive', false)` — a name SQL builds
    // rather than spells — and a second `set_config` beside a harmless first
    // one. Measured on PostgreSQL 18: the unqualified
    // `CREATE TABLE access_records` below such a line lands in `archive` while
    // `[tables.access_records]` goes on bounding and pruning `public`.
    //
    // **And the excuse is that call's own value, not the statement's.** An
    // exclusion asking whether `current_schema` or `current_setting` appears
    // *anywhere in the statement* accepts a hard-coded redirect the
    // moment either word stands beside it — in a different call, or in the
    // redirect's own value. Measured: `set_config('search_path', 'archive, ' ||
    // current_setting('search_path'), false)` — which **prepends** `archive` to
    // the path and is therefore a redirect built out of `current_setting` — and
    // `set_config('search_path', 'archive', false), current_schema()`, the
    // statement the test list already requires to be refused with a no-op
    // appended. Both pass with `check passed` and exit 0.
    //
    // What the excuse is really about is *order*: the path the database
    // already has must come **before** anything written down. Appending to it
    // — `current_setting(…) || ', pg_temp'`, which `0011` and `0012` write —
    // leaves the first schema on the path where it was; prepending replaces it,
    // and prepending is a literal before the function.
    let sets_by_function = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| names_the_function(token, "set_config"))
        .filter(|(at, _)| {
            // **And the settings that re-aim resolution, which `set_config`
            // writes too.** `role` and `session_authorization` are ordinary
            // GUCs; the keyword statement and the function call are the same
            // write, and refusing only the keyword lets the call through.
            // Measured on PostgreSQL 18: `SELECT set_config('role', 'r18', false)`
            // changes `current_user`, leaves `SHOW search_path` alone, and puts
            // `access_records` in that role's schema. The paragraph beside
            // `the_settings_a_set_writes` already says `set_config` is not the
            // only way to write a setting; this is the other direction of the
            // same sentence.
            matches!(
                the_first_argument_of(&tokens[*at..]).as_deref(),
                Some("search_path" | "role" | "session_authorization")
            )
        })
        .any(|(at, _)| match the_first_argument_of(&tokens[at..]) {
            // **The path and a role are cleared by different rules**, because
            // appending means different things to them. A path is an ordered
            // list and what decides resolution is its *first* schema, so
            // `current_setting('search_path') || ', pg_temp'` leaves the answer
            // where it was — which is why the shape below tolerates literals
            // after the call. A role is one value: append anything to it and it
            // names a different role. So the only `role` write this clears is
            // the exact restore, and it is cleared, because refusing it would
            // tell a migration not to put back what it has saved.
            Some(name) if name != "search_path" => {
                !the_value_is_the_setting_it_writes(&tokens[at..], &name)
            }
            _ => !the_value_keeps_the_path(&tokens[at..], settings),
        });
    // **A name the gate cannot read is a name it cannot clear** — and it is a
    // finding of its own, not the one above. A filter asking for `search_path`
    // and skipping everything else leaves a name PostgreSQL is perfectly happy
    // to compute unexamined: `concat('search', '_path')`,
    // `lower('SEARCH_PATH')` and `format('%s_path', 'search')` each set the path
    // to `archive` with `check passed`. Failing those into the branch above is
    // right in outcome and wrong in what it says — an ordinary PL/pgSQL
    // `set_config(knob, 'off', false)` naming `row_security` through a variable
    // would be told to qualify table names in a statement that creates none, and
    // `§15.1` asks a `fix` to say what to do about the thing that is wrong.
    // **A signature is not a call.** `REVOKE ALL ON FUNCTION set_config(text,
    // text, boolean) FROM PUBLIC` names the routine and lists its *parameter
    // types*; reading those as arguments makes the name unreadable and the
    // statement a finding — again refusing the hardening this gate exists to
    // encourage. What stands in front decides: a call has an expression before
    // it, a signature has `FUNCTION` or `PROCEDURE`.
    let names_unreadably = tokens
        .iter()
        .enumerate()
        .filter(|(at, token)| {
            names_the_function(token, "set_config")
                && !tokens[at.saturating_sub(3)..*at]
                    .iter()
                    .any(|before| before.is("FUNCTION") || before.is("PROCEDURE"))
        })
        .any(|(at, _)| the_first_argument_of(&tokens[at..]).is_none());
    (sets_the_path, sets_by_function, names_unreadably)
}

/// Whether a `set_config`'s value starts from the path the database already has.
///
/// **Order decides.** `current_setting('search_path') || ', pg_temp'` appends: the first
/// schema on the path is still the first, so an unqualified `CREATE TABLE` lands where it
/// did. `'archive, ' || current_setting('search_path')` prepends, which is a redirect —
/// and it is built out of `current_setting`, so a rule that asks only whether that word
/// appears excuses it.
///
/// **And order is not answerable, because the path's own argument is
/// a literal.** `current_setting('search_path')` puts one inside the call, so "no
/// literal ahead of the function" is true of every value that opens with such a
/// call — including one reading an unrelated setting, or discarding the path and
/// prepending a constant. So the question asked here is not *order* but *shape*: a
/// value built from the path, a `quote_ident` around it and literals appended to it
/// is cleared; a value with anything else in it is not this function's to judge, and
/// carries a written reason instead. See [`CLEARED_BY_HAND`].
fn the_value_keeps_the_path(tokens: &[Token], settings: &Settings<'_>) -> bool {
    let Some(value) = the_argument_after_the_first(tokens) else {
        return false;
    };
    the_value_is_the_path(value, settings)
}

/// Whether a value is the path the database already has, read as a shape.
///
/// Held apart from [`the_value_keeps_the_path`] because a `SET` writes its value
/// directly where a `set_config` writes it as an argument, and the two must be
/// judged by one rule rather than by two copies of it.
fn the_value_is_the_path(value: &[Token], settings: &Settings<'_>) -> bool {
    /// The only things that may appear in a value this rule clears silently.
    const READABLE: [&str; 3] = ["current_schema", "current_setting", "quote_ident"];

    // **A position test cannot work, because the path's own argument is a
    // literal.** `current_setting('search_path')` puts a literal inside the
    // call, so "the function appears before any literal" is true of *every*
    // value that opens with such a call — whatever it then does with it. Five
    // spellings walk through such a test, measured, each putting a constant on the path:
    // `current_setting('nc.target') || ', pg_temp'` where `nc.target` is set to
    // `'archive'` a statement earlier; `COALESCE(current_setting('nc.missing',
    // true), 'archive')`; `substr(current_setting('search_path'), 1, 0) ||
    // 'archive'`, which names the path and discards it; a `CASE` whose arms are
    // both constants; and two `current_setting`s with a constant between them.
    // Each gives `check passed` with the `ok` lines byte-identical.
    //
    // So the value is read as a shape rather than as an ordering, and the shape
    // is closed: the only things that may appear are the path itself, a
    // `quote_ident` around it, the concatenation that appends to it, and
    // literals after it. Any other function decides the value in a way this
    // rule cannot evaluate, and a value it cannot evaluate is one it must not
    // clear — `substr` and `CASE` are refused for the same reason a future
    // `regexp_replace` will be, without anybody adding it to a list.
    // **And a schema qualifier is not a function.** `pg_catalog.current_setting`
    // is the spelling `search_path` hygiene exists to encourage, and refusing
    // it would contradict the rule's own message, which names `current_setting(…)`
    // as the thing that makes a value safe. However the tokeniser splits it, the
    // qualifier is dropped and the function is read.
    //
    // **No spelling reaches the drop, and it stays anyway.** The tokeniser cuts
    // `pg_catalog.current_setting` into two tokens and cuts the quoted
    // `"pg_catalog".current_setting` into two as well, so the `pg_catalog` arm
    // below is what actually clears both and this line is decided by nothing —
    // it survives deletion with the suite green. It is kept because the two
    // failures are not symmetric: a qualifier this drops that the tokeniser
    // someday glues costs nothing, and one it fails to drop refuses the
    // schema-qualified spelling `search_path` hygiene exists to encourage.
    let named = |word: &str| -> String {
        let lowered = word.to_ascii_lowercase();
        lowered
            .rsplit('.')
            .next()
            .unwrap_or(lowered.as_str())
            .to_owned()
    };
    let mut path_at = None;
    for (at, token) in value.iter().enumerate() {
        let Some(word) = token.name() else { continue };
        if joins_a_name(word) || word == "pg_catalog" {
            continue;
        }
        let lowered = named(word);
        // **A boolean is an argument, not a function.**
        // `current_setting('search_path', true)` is PostgreSQL's documented
        // `missing_ok` form and appends exactly as the one-argument spelling
        // does — measured, `"$user", public, pg_temp` either way — and a loop
        // requiring every word to be one of three function names refuses it,
        // because `true` is a word.
        if matches!(lowered.as_str(), "true" | "false") {
            continue;
        }
        if !READABLE.contains(&lowered.as_str()) {
            return false;
        }
        if lowered != "quote_ident" && path_at.is_none() {
            path_at = Some(at);
        }
    }
    // **The first source, not the only one.** Written as *exactly one*, this
    // refuses `current_setting('search_path') || ', ' || quote_ident(current_schema())`
    // — an append, and identical on a database to what `0011` already ships.
    // Concatenation is ordered, so what decides the first schema on the path is
    // the *first* source; a later one is appended and cannot displace it.
    // `current_setting('nc.a') || 'archive' || current_setting('nc.b')` is still
    // refused, because its first source is not the path.
    let Some(path_at) = path_at else {
        return false;
    };
    if value[..path_at]
        .iter()
        .any(|token| matches!(token, Token::Literal(_)))
    {
        return false;
    }
    if value[path_at].is("current_schema") {
        return true;
    }
    // **And the setting read has to be the path.** `current_setting` names
    // whichever GUC it is given; reading an unrelated one is a constant wearing
    // the path's clothes. A custom setting counts only where this same file
    // saved the real path into it, which is what `0012` does across its own
    // statements — and only one step, so a value laundered through two settings
    // is not path-bearing either.
    match the_first_argument_of(&value[path_at..]) {
        Some(name) => name == "search_path" || settings.hold_the_path(&name),
        None => false,
    }
}

/// Whether a `set_config` writes back the very setting it is reading.
///
/// `SELECT set_config('role', current_setting('role'), false)` is a measured
/// no-op — it is the shape `0012` already writes for `search_path`, applied to
/// the role — and refusing it would tell a migration that saving and restoring
/// the role it found is a redirect.
///
/// Deliberately narrower than [`the_value_is_the_path`]: nothing may be appended
/// and no second reader may appear, because for a setting that is one value
/// rather than an ordered list, an append *is* the redirect. `pg_catalog.` and
/// the documented `missing_ok` boolean are the only other things allowed.
fn the_value_is_the_setting_it_writes(tokens: &[Token], writing: &str) -> bool {
    let Some(value) = the_argument_after_the_first(tokens) else {
        return false;
    };
    let mut read_at = None;
    for (at, token) in value.iter().enumerate() {
        let Some(word) = token.name() else { continue };
        let lowered = word.to_ascii_lowercase();
        let bare = lowered.rsplit('.').next().unwrap_or_default().to_owned();
        if bare == "pg_catalog" || matches!(bare.as_str(), "true" | "false") {
            continue;
        }
        if bare != "current_setting" || read_at.is_some() {
            return false;
        }
        read_at = Some(at);
    }
    let Some(read_at) = read_at else {
        return false;
    };
    the_first_argument_of(&value[read_at..]).is_some_and(|name| name == writing)
}

/// The custom settings this file saves the real `search_path` into.
///
/// `0012` writes `set_config('nc.path_before_0012', current_setting('search_path'),
/// false)` and restores from it thirty lines later, which is an ordinary and
/// correct thing for a migration to do. Without this the restore reads as a
/// constant on the path; with it, and only for a setting whose value came
/// straight from the path, the restore is what it says it is.
///
/// One step deep on purpose: the allow-list handed to the inner call is empty,
/// so a setting filled from *another* setting is not path-bearing. A launder
/// needs somewhere to start, and this is where it would.
fn the_settings_written_in(text: &str, dialect: Dialect) -> Vec<(String, usize, bool, bool)> {
    let mut written = Vec::new();
    for (index, statement) in statements_in(text, dialect).into_iter().enumerate() {
        written.extend(the_settings_a_set_writes(&statement, index));
        for (at, _) in statement
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| names_the_function(token, "set_config"))
        {
            let Some(name) = the_first_argument_of(&statement.tokens[at..]) else {
                continue;
            };
            if name == "search_path" {
                continue;
            }
            // One step deep: the allow-list handed to the inner call is empty,
            // so a setting filled from *another* setting is not path-bearing. A
            // launder needs somewhere to start, and this is where it would.
            // **A write inside a routine body cannot be ordered against the
            // rest.** `cut` finishes every top-level statement before it
            // recurses, so a `PERFORM set_config(…)` in a `DO $$ … $$` is
            // numbered *after* statements that run before it — measured, ordered
            // by that numbering, a refill in a `DO` block is invisible to the
            // comparison below and the read-back cleared. Ordering it properly would mean
            // reordering the walk; what this needs instead is to stop claiming
            // an order it does not have, so a nested write is never
            // path-bearing and the statement that reads it is judged on its own.
            let from_the_path =
                the_value_keeps_the_path(&statement.tokens[at..], &Settings::none());
            written.push((name, index, from_the_path, statement.nested));
        }
    }
    written
}

/// The tokens of a call's second argument, up to the comma that ends it.
fn the_argument_after_the_first(tokens: &[Token]) -> Option<&[Token]> {
    let opens = tokens
        .iter()
        .position(|token| matches!(token, Token::Punct('(')))
        .map(|at| at + 1)?;
    let mut depth = 0_usize;
    let mut commas = 0_usize;
    let mut from = None;
    for (at, token) in tokens[opens..].iter().enumerate() {
        match token {
            Token::Punct('(') => depth += 1,
            Token::Punct(')') if depth == 0 => {
                return from.map(|start| &tokens[opens + start..opens + at]);
            }
            Token::Punct(')') => depth -= 1,
            Token::Punct(',') if depth == 0 => {
                commas += 1;
                match commas {
                    1 => from = Some(at + 1),
                    _ => return from.map(|start| &tokens[opens + start..opens + at]),
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether a word is part of how a name is spelled rather than part of the name.
fn joins_a_name(word: &str) -> bool {
    word.chars().all(|character| character == '|')
        || word.eq_ignore_ascii_case("text")
        || word.starts_with(':')
}

/// A function call's first argument, when it is written entirely as literals.
///
/// **A name SQL builds is the same name.** `set_config('search' || '_path', …)`
/// names `search_path`, and reading only the first literal after the call sees
/// `search`. Every literal up to the first comma no bracket is open over is
/// joined, which is what `||` does; anything that is not a literal — a column,
/// a parameter, a nested call — answers `None`, because this rule is about a
/// name written down and not about one computed at run time.
fn the_first_argument_of(tokens: &[Token]) -> Option<String> {
    let opens = tokens
        .iter()
        .position(|token| matches!(token, Token::Punct('(')))
        .map(|at| at + 1)?;
    let mut depth = 0_usize;
    let mut built = String::new();
    for token in &tokens[opens..] {
        match token {
            Token::Punct('(') => depth += 1,
            Token::Punct(')' | ',') if depth == 0 => break,
            Token::Punct(')') => depth -= 1,
            Token::Literal(text) => built.push_str(text),
            // **How the name is written, not part of it.** `||` joins two
            // literals into one name and `::text` casts one; this tokeniser
            // separates only `.`, `(`, `)` and `,`, so both arrive as a word.
            Token::Word(word) if joins_a_name(word) => {}
            _ => return None,
        }
    }
    (!built.is_empty()).then(|| built.trim().to_ascii_lowercase())
}

/// One piece of a statement.
///
/// **A quoted identifier is one token**, which is the whole reason this is a
/// token list rather than flattened text. Flattening on whitespace truncates
/// `CREATE TABLE "login attempts"` to `login` — a finding naming a table nobody
/// wrote — and `CREATE TABLE "access_records archive"` to `access_records`,
/// which the catalog declares, so an unbounded table walks straight through.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// A bare word: a keyword, or one part of an unquoted name.
    Word(String),
    /// A quoted identifier, with its quotes removed and `""` unescaped.
    Name(String),
    /// Punctuation that matters: `.` joins a qualified name, `(` ends one.
    Punct(char),
    /// A string literal, and the text inside it.
    ///
    /// **The text is carried and never read as code.** A `CREATE TABLE` written
    /// inside a literal is data, and every rule that looks for a table's name
    /// reads `Word` and `Name` only — that is what this variant being its own
    /// kind is for. What the text is needed for is the *argument* of a function
    /// that takes a setting's name as a string: `set_config('search_path', …)`
    /// redirects every unqualified name below it, and with the contents dropped
    /// there is nothing to tell it from `set_config('nc.saved_path', …)`, which
    /// is what this repository's own migrations write.
    Literal(String),
}

impl Token {
    /// Whether this token is the keyword `word`, case-insensitively.
    ///
    /// A **quoted** identifier is never a keyword: `CREATE "TABLE" x` creates a
    /// table called `TABLE`, and reading the quoted word as the keyword would
    /// lose the name.
    fn is(&self, word: &str) -> bool {
        matches!(self, Self::Word(text) if text.eq_ignore_ascii_case(word))
    }

    /// The name this token carries, if it carries one.
    fn name(&self) -> Option<&str> {
        match self {
            Self::Word(text) | Self::Name(text) => Some(text),
            _ => None,
        }
    }

    /// The name this token carries, spelled as PostgreSQL will resolve it.
    ///
    /// **An unquoted name folds and a quoted one does not.** A reader that
    /// lowercases both makes `"RECORDED"` and `recorded` one column. Measured,
    /// with the gate green:
    ///
    /// ```text
    /// CREATE TABLE access_records (id TEXT, recorded TEXT NOT NULL);
    /// ALTER TABLE access_records ADD COLUMN "RECORDED" BIGINT NOT NULL;
    /// ```
    ///
    /// Such a gate concludes `recorded` is `bigint`; the database has `recorded`
    /// as TEXT and a second column beside it. The pruner writes
    /// `ORDER BY "recorded"` and sorts text. This module goes to real trouble to
    /// keep a quoted identifier one token — `CREATE TABLE "access_records
    /// archive"` is its own example — and folding it afterwards would undo that.
    fn resolved(&self) -> Option<String> {
        match self {
            Self::Word(text) => Some(text.to_lowercase()),
            Self::Name(text) => Some(text.clone()),
            _ => None,
        }
    }
}

// A disagreement a test asked for, and zero in every other build.
//
// **A tripwire has to be wired to something.** `the_readers_agree` has a unit
// test of its own, and this is what tests the `?` that carries its verdict out
// of the walk: `let _ = the_readers_agree(…)` in its place fails the test
// below, and nothing a clean run prints would change, under a paragraph saying
// *"a finding can be filtered — this cannot, and it stops the walk where it
// is"*.
//
// It cannot be driven from a file. Both readers fingerprint the same statements
// with the same tokeniser, which is the whole reason a disagreement means one
// of them was given less than the whole text rather than that the text is
// unusual. So the disagreement is injected, in a build where the injection
// exists at all, and the test below watches the walk stop.
//
// Written as prose rather than as a doc block because a `///` above a macro
// invocation lands on the *item below the macro* — which is the fusion this
// crate's own ratchet exists to catch.
#[cfg(test)]
thread_local! {
    /// One, once, for the next file the walk reads, and zero afterwards.
    static A_DISAGREEMENT_FOR_TESTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Makes the next file the walk reads fingerprint one higher than it should.
///
/// Taken rather than read, so one call disturbs one file and the walk is back to
/// itself afterwards.
#[cfg(test)]
#[doc(hidden)]
pub(crate) fn disagree_once_for_tests() {
    A_DISAGREEMENT_FOR_TESTS.with(|forced| forced.set(1));
}

/// What `disagree_once_for_tests`, the door above, asked for, taken rather than
/// read.
#[cfg(test)]
fn a_forced_disagreement() -> usize {
    A_DISAGREEMENT_FOR_TESTS.with(std::cell::Cell::take)
}

/// Zero, in every build that is not a test build, where the door is not there
/// to be opened.
#[cfg(not(test))]
const fn a_forced_disagreement() -> usize {
    0
}

/// That the two readers of one file arrived at the same fingerprint.
///
/// **An error, not a finding.** With the reader taking the other count and
/// reporting the disagreement as one of its own verdicts, three shapes get
/// past: a guard clause at the top of the reader returning before the
/// comparison runs; a filter in the caller on `files == 1` dropping every
/// verdict from the migrations no fixture writes; and a `retain` on the
/// verdict's own text silencing the comparison itself. A finding can be
/// filtered. This stops the walk where it stands.
fn the_readers_agree(named: &str, judged: usize, counted: usize) -> Result<(), String> {
    if judged == counted {
        return Ok(());
    }
    Err(format!(
        "{named} was read twice and the two readers disagree: {judged} and {counted}. They \
         are handed the same text and fingerprint it with the same tokeniser, so this is \
         not a judgement call: one of them was given less than the whole file, stopped \
         inside it, or was not run at all. Find which, rather than moving either number"
    ))
}

/// What one migration does to the set of tables that exist, and what it costs.
///
/// **Split out of the walk**, so the walk stays the length a reader keeps in
/// their head with one merge on statement order in it.
/// Returns the findings, the tables a marker excused by name, and how many
/// tables were judged — the three things the walk accumulates — and applies the
/// creations and retirements to `exists` in the order the statements run.
fn what_one_file_declares(
    changes: &Changes,
    declared: &Bounds<'_>,
    lines: &[&str],
    comments: &[usize],
    relative: &str,
    exists: &mut Vec<String>,
) -> (Vec<Finding>, Vec<String>, usize) {
    let mut findings = Vec::new();
    let mut excused_by_a_marker = Vec::new();
    let mut seen = 0_usize;
    let (mut made, mut moved) = (0_usize, 0_usize);
    while made < changes.created.len() || moved < changes.renamed.len() {
        let creates_next = match (changes.created.get(made), changes.renamed.get(moved)) {
            (Some(table), Some(rename)) => table.at <= rename.at,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if creates_next && let Some(table) = changes.created.get(made) {
            made += 1;
            exists.push(table.name.clone());
            seen += 1;
            let made_here = Declared {
                name: &table.name,
                line: table.line,
                alone: !table.shares_its_line && table.alone_in_its_statement,
            };
            match the_bound_for(&made_here, declared, lines, comments, relative) {
                Bound::Declared => {}
                Bound::Excused => excused_by_a_marker.push(table.name.clone()),
                Bound::Missing(finding) => findings.push(finding),
            }
            continue;
        }
        // **A rename creates a table and retires one.** Unread,
        // `ALTER TABLE access_records RENAME TO
        // login_attempts` leaves the bounded entry naming a table that no
        // longer exists, and the table that now holds the rows unbounded,
        // with `check passed`.
        let Some(rename) = changes.renamed.get(moved) else {
            break;
        };
        moved += 1;
        exists.retain(|name| *name != rename.from);
        // An empty `to` is a drop: the name is gone and nothing replaced it.
        if rename.to.is_empty() {
            continue;
        }
        exists.push(rename.to.clone());
        seen += 1;
        // **The same three answers, because a rename creates a table.** Testing
        // `declared.contains` and nothing else leaves the marker the remedy
        // tells a developer to write inert here — the gate naming a hatch
        // this path does not have, which is prose stating the inverse of the
        // code.
        let landed = Declared {
            name: &rename.to,
            line: rename.line,
            alone: !rename.shares_its_line && rename.alone_in_its_statement,
        };
        match the_bound_for(&landed, declared, lines, comments, relative) {
            Bound::Declared => {}
            Bound::Excused => excused_by_a_marker.push(rename.to.clone()),
            Bound::Missing(finding) => findings.push(finding),
        }
    }
    (findings, excused_by_a_marker, seen)
}

/// What one `CREATE TABLE` costs the census: a bound, an excuse, or a finding.
///
/// **Split out so the walk that reads a migration stays a walk.** The
/// reconciliation carries a merge on statement order — a drop and a create in one
/// file are read in the order they are written — and the decision about a single
/// table is the part of it that has nothing to do with ordering.
enum Bound {
    /// `catalog/limits.toml` bounds this table.
    Declared,
    /// A marker above the statement excuses it, and names it.
    Excused,
    /// Neither, which is the verdict.
    Missing(Finding),
}

/// Which of the three a table declared in one statement is.
///
/// **Named, not only counted.** Counted only, a table excused by the marker
/// moves the census total from 92 to 97, measured, and appears nowhere else —
/// and the one line a reader would look at, the one that says how many tables
/// were checked and which declare no bound, is silent about it. An exemption that moves only an
/// aggregate is an exemption nobody sees, so `Excused` carries the name back.
fn the_bound_for(
    table: &Declared<'_>,
    declared: &Bounds<'_>,
    lines: &[&str],
    comments: &[usize],
    relative: &str,
) -> Bound {
    if declared.declared.contains(&table.name) {
        return Bound::Declared;
    }
    if excused(lines, comments, table) {
        return Bound::Excused;
    }
    // **Bounded in the other database is not bounded.** The pruner looks for
    // the table where its entry says, so the finding names the database to
    // change rather than asking for an entry that is already written.
    let elsewhere = nc_catalog::TABLES
        .iter()
        .any(|entry| entry.name == table.name && entry.database != declared.database);
    if elsewhere {
        return Bound::Missing(in_the_other_database(relative, table, declared.database));
    }
    Bound::Missing(unbounded(relative, table.name, table.line))
}

/// The entries one directory's tables are bounded by, and which database that is.
struct Bounds<'a> {
    /// The names `catalog/limits.toml` bounds in this database.
    declared: &'a [&'a str],
    /// The database the directory builds.
    database: nc_catalog::Database,
}

/// Whether a statement runs SQL this reader cannot see.
///
/// A string literal is dropped by the tokeniser — deliberately, so a `CREATE
/// TABLE` written inside one is data — and `format('CREATE TABLE %I …')` puts
/// the name in the literal too. There is nothing left to match on but the verb.
///
/// **`EXECUTE` is also SQL's privilege on a routine, and an ordinary name.**
/// `REVOKE EXECUTE ON FUNCTION set_config(…) FROM PUBLIC` is the single most
/// on-point hardening there is for `§15.8`, and a reader of the word alone
/// refuses it — with a message about dynamic SQL, in the gate that exists for
/// `§15.8`. `GRANT EXECUTE ON
/// ALL FUNCTIONS IN SCHEMA …` and `ALTER DEFAULT PRIVILEGES … GRANT EXECUTE ON
/// FUNCTIONS` are the same word in the same place, and
/// `CREATE TRIGGER … EXECUTE FUNCTION f()` is the ordinary use every migration
/// here has one of.
///
/// **And it is a *non-reserved* keyword on PostgreSQL 18.6**, so a table or a
/// column may be called it, and the word alone says nothing about that. Measured,
/// three statements that run and build nothing:
///
/// ```text
/// CREATE TABLE t (execute INT);              -> refused as dynamic SQL
/// CREATE TABLE execute (id INT);             -> refused
/// ALTER TABLE t RENAME COLUMN a TO execute;  -> refused
/// ```
///
/// A refusal of something legitimate is what this module's header calls worse
/// than a miss: the developer is told a migration builds SQL out of text when it
/// declares a column, and the way past it is a hatch on a statement that needed
/// none.
///
/// What separates the three is where the word stands and what follows it.
/// Dynamic SQL *begins* a statement — first, or after a word that opens one,
/// which is the test `statements` uses to tell `RAISE` the keyword from `raise`
/// the function name. A privilege is followed by `ON`, a trigger by `FUNCTION`
/// or `PROCEDURE`, and dynamic SQL by the text it runs or by nothing this reader
/// can see.
fn builds_a_statement_out_of_text(tokens: &[Token]) -> bool {
    (0..tokens.len()).any(|at| {
        tokens[at].is("EXECUTE")
            && at
                .checked_sub(1)
                .and_then(|before| tokens.get(before))
                .is_none_or(opens_a_statement)
        // **And no carve-out for what follows the word.** A carve-out
        // reading what follows — `FUNCTION`, `PROCEDURE`, `ON` — is needed
        // only while the word is looked for anywhere in the statement, so
        // that a trigger's `EXECUTE FUNCTION f()` and a privilege's
        // `GRANT EXECUTE ON …` are carved back out. With the position asked,
        // neither of those `EXECUTE`s is at a statement boundary and neither
        // reaches here at all: measured with such a carve-out in place, the
        // runner reports `a trigger's PROCEDURE is a built statement` as a
        // weakening no test can see, which is what a clause that decides
        // nothing looks like from the outside. The corpus rows for those
        // shapes stand, and prove the boundary test carries them.
        //
        // Nor a disjunct like `tokens.get(at + 1).is_none_or(…)` for
        // `EXECUTE` with its text in a variable this reader cannot see — a
        // statement whose last token is the word: that is the boundary case
        // anyway, since `EXECUTE;` is one token, at position zero.
        // `RENAME COLUMN a TO execute` is not, which is what such a disjunct
        // gets wrong.
    })
}

/// Every statement this scanner refuses to read, and why.
///
/// **Read from the tokens, not from the text.** A scan of the raw file is wrong
/// in both directions at once: a `--` line above a `COPY` makes `copy` no longer
/// the statement's first word, so the refusal is skipped and the apostrophe in
/// the data blanks the file; and a comment *mentioning* `import foreign schema`
/// fails the build over prose.
///
/// Blanking comments and literals first means re-implementing the quoting
/// grammar, and a copy knowing neither dollar quoting nor `E'…\'…'` has both
/// gaps, measured: an `E'…'` above a `COPY` hides the refusal again, and
/// a `$doc$…$doc$` document holding the words fails the build. That is the
/// class this module already has a rule about — *a
/// grammar understood in two places is a grammar that will one day be understood
/// two ways* — and the answer is not a better second copy.
///
/// `cut` already knows every quoting form PostgreSQL has, and it already drops
/// comments and reduces a literal to one opaque token. A statement's *tokens*
/// therefore cannot contain a word that was written inside a comment or a
/// string, which is exactly the property a scan of the text would have to
/// reconstruct.
///
/// **It is given the weight the reader beside it measured**, and says so
/// itself when the two disagree.
///
/// **Counted where the judgement happens, not where the cutting does**, and in
/// tokens rather than in statements. Three other ways of taking this number
/// are wrong in the same direction, measured:
///
/// - `all.len()` above the loop measures what the *cutter* produces, so a
///   `.filter(|(_, s)| s.line <= 5)` on the loop's own line silences every rule
///   below it with both counts agreeing and `check`'s output byte-identical;
/// - `judged += 1` as the loop's *first* statement moves for every iteration
///   whatever the body then does, so re-slicing `tokens` to nothing for statements
///   past the eighth leaves the tail of every migration unjudged — 0001 holds 9
///   statements and 0012 holds 22 — and is byte-identical again. This
///   repository's own canary notes state the rule: *the
///   counter is the last statement a verdict reaches, not the first statement of
///   the body.*
/// - A count of *statements* says how much of the file was cut. What the rules
///   below read is tokens, so that is what is counted, at the end of the body,
///   through the same binding the rules use.
///
/// **And the comparison is not the caller's to remember.** Handed back as a
/// number it is one line of wiring, and replacing that line with `let _ = …`
/// restores the whole defect with the suite green, measured. A count that has to be
/// remembered is a count that can be forgotten, so what goes in is the other
/// reader's number and what comes out already holds the verdict. Without it this
/// rule is wired to nothing: handed `&text[..400]` it refuses nothing, and
/// `check`'s output is byte-identical to a full run — no count moves, because
/// it publishes none. Its sibling rules in this loop each have a second,
/// independent number (`files != on_disk`, `seen < bounded`, `found != creates`)
/// and without this one it would have only its own findings, which is the one
/// number a defect that stops it from looking will always leave at zero.
///
/// Read as `dialect` cuts it: the two directories of migrations are two SQLs.
// allow-logging: `unreadable_statements_in` hands back a list of statement kinds written in this file
fn unreadable_statements_in(
    text: &str,
    dialect: Dialect,
) -> (Vec<(usize, String, &'static str)>, usize) {
    let written = the_settings_written_in(text, dialect);
    let lines: Vec<&str> = text.lines().collect();
    let (mut found, mut judged) = (Vec::new(), 0_usize);
    for (index, statement) in statements_in(text, dialect).into_iter().enumerate() {
        let tokens = &statement.tokens;
        let is = |at: usize, word: &str| tokens.get(at).is_some_and(|token| token.is(word));
        // `COPY … FROM stdin`, however it is wrapped: the tokeniser has already
        // put the whole statement together, so a line break inside it is not a
        // shape this has to know about.
        if is(0, "COPY")
            && tokens
                .iter()
                .position(|token| token.is("FROM"))
                .is_some_and(|at| is(at + 1, "stdin"))
        {
            found.push((
                statement.line,
                "`COPY … FROM stdin` is in a migration".to_owned(),
                STDIN,
            ));
        }
        // **A redirected `search_path` renames every unqualified table below it**,
        // and this scanner credits an unqualified name to `public`
        // unconditionally. Measured through the gate: `SET search_path =
        // archive;` on the line above `CREATE TABLE access_records (…)` gives
        // `unbounded=[]` — the same table written `CREATE TABLE
        // archive.access_records` is caught — so a table is credited to
        // `[tables.access_records]`, whose pruner runs `DELETE FROM
        // access_records` against a different table, and grows unbounded with
        // `check passed`.
        //
        // Following the path properly means carrying it through the walk, and a
        // value this scanner cannot read — `current_setting(…)`, a variable —
        // would still have to be guessed at. So the shape is refused, which is
        // what this function is for. `SET search_path = current_schema()` is not
        // a redirect and is not refused: migrations 0011 and 0012 both write it.
        // **Every spelling PostgreSQL accepts, not one of them.** A rule asking
        // for `SET … search_path = …` and nothing else lets
        // `SET search_path TO archive;` — PostgreSQL's own documented form, and
        // the one every tutorial writes — into a migration above
        // `CREATE TABLE access_records` with every test green and `check
        // passed`, putting a table of personal data in another schema while
        // `[tables.access_records]` goes on pruning `public`. `SET SESSION`,
        // `SET LOCAL`, a quoted value and `set_config('search_path', …)` are the
        // same statement again. This is the class two functions in this very
        // file each carry a paragraph about: a rule written for one spelling of
        // something SQL spells several ways.
        // The name, however the tokeniser split it. `search_path=archive` has no
        // space in it and is one word, so asking for the token `search_path`
        // misses it — measured, accepted with `check passed`.
        let folded = |at: usize| {
            tokens
                .get(at)
                .and_then(Token::name)
                .map(str::to_ascii_lowercase)
        };
        let (at_the_name, joined) = where_the_path_is_named(tokens);
        let assigns = |at: usize| assigns_a_new_path(tokens, at);
        // A routine's own configuration clause is not a session redirect — but
        // only the clause, and only where it is written. See
        // [`the_routines_own_clause`].
        let its_own_clause = the_routines_own_clause(tokens, &the_source_of(&lines, &statement));
        let (sets_the_path, sets_by_function, names_unreadably) = the_path_redirects_in(
            tokens,
            at_the_name,
            joined,
            &assigns,
            &folded,
            &Settings {
                written: &written,
                index,
            },
        );
        found.extend(names_unreadably.then_some((
            statement.line,
            THE_UNREADABLE_NAME.0.to_owned(),
            THE_UNREADABLE_NAME.1,
        )));
        // A value the closed shape does not cover is reported unless this exact
        // statement is one somebody has read and written down.
        let excused = read_by_hand(&lines, statement.line, statement.ends_on);

        if sets_by_function && !excused {
            found.push((
                statement.line,
                "a migration redirects `search_path` through `set_config`".to_owned(),
                "every unqualified `CREATE TABLE` below it names a table in another \
                 schema, and this gate credits an unqualified name to `public` — so the \
                 table would be checked against, and pruned by, an entry for a different \
                 one (§15.8). The `set_config` calls this repository writes build their \
                 value from `current_schema()` or `current_setting(…)`, which is the \
                 path the database already has; a constant is a redirect. Qualify the \
                 table names instead",
            ));
        }
        if sets_the_path && !its_own_clause {
            found.push((
                statement.line,
                "a migration redirects `search_path`".to_owned(),
                "every unqualified `CREATE TABLE` below it names a table in another \
                 schema, and this gate credits an unqualified name to `public` — so the \
                 table would be checked against, and pruned by, an entry for a different \
                 one (§15.8). Qualify the table names instead, or set the path to \
                 `current_schema()`",
            ));
        }
        // **`IMPORT FOREIGN SCHEMA` creates arbitrarily many foreign tables**,
        // and this scanner cannot know which. `CREATE FOREIGN TABLE` is read
        // because a migration can add one with the count nobody reads staying
        // the same; this is the bulk form of the same statement.
        // **Anywhere in the statement, not at its first token.** A routine body's
        // statements begin with `BEGIN`, so wrapping the import in a `DO` block
        // moves it off position zero and a first-token refusal never fires —
        // measured, PL/pgSQL parses and dispatches it (only a missing server stops
        // it), and the gate then reports a count and no finding for a migration that creates
        // arbitrarily many unbounded foreign tables. Reading anywhere is safe because a quoted
        // string is only re-cut where something runs it: prose that merely names the
        // statement is not a statement.
        if tokens
            .windows(3)
            .any(|three| three[0].is("IMPORT") && three[1].is("FOREIGN") && three[2].is("SCHEMA"))
        {
            found.push((
                statement.line,
                "`IMPORT FOREIGN SCHEMA` is in a migration".to_owned(),
                "it creates a foreign table per table on the remote server, and this gate \
                 cannot know how many or what they are called — so none of them would \
                 declare a bound (§15.8). Write the `CREATE FOREIGN TABLE` statements out, \
                 which is also the only way a reader of the migrations can see what the \
                 database now holds",
            ));
        }
        // **And `EXECUTE` that builds DDL out of text**, which is the same class
        // as the two above: a real table is created and no name exists in the
        // file for this gate to read. Both shapes, measured without this, create
        // tables on PostgreSQL 18.6 with the count unmoved and no finding at all:
        //
        // ```text
        // DO $$ BEGIN EXECUTE format('CREATE TABLE %I (id INT)', 'login_attempts'); END $$;
        // DO $$ BEGIN EXECUTE 'CREATE TABLE ' || quote_ident('x') || ' (id INT)'; END $$;
        // ```
        //
        // `%I` is dropped by the name reader and a concatenation yields no name
        // token at all, so without this the gate's guess is "nothing here" — which
        // is the one answer it must not give about a statement that creates
        // tables. `IMPORT FOREIGN SCHEMA` is refused for exactly this reason.
        //
        // **Read as `EXECUTE` that is not `EXECUTE FUNCTION`**, because the text
        // it runs is unreadable by construction: a string literal is dropped by
        // the tokeniser — deliberately, so a `CREATE TABLE` written inside one
        // is data — and `format('CREATE TABLE %I …')` puts the name in the
        // literal too. There is nothing left to match on but the verb.
        //
        // `CREATE TRIGGER … EXECUTE FUNCTION f()` is the ordinary use and every
        // migration here has one, so those two forms are what this steps over.
        //
        // **And `EXECUTE` is also SQL's privilege on a routine.**
        // `REVOKE EXECUTE ON FUNCTION set_config(…) FROM PUBLIC` is the single
        // most on-point hardening there is for `§15.8`, and a reader of the word
        // alone refuses it — with a message about dynamic SQL, in the gate that
        // exists for `§15.8`.
        // `GRANT EXECUTE ON …`, `GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA …` and
        // `ALTER DEFAULT PRIVILEGES … GRANT EXECUTE ON FUNCTIONS` are the same
        // word in the same place. What tells the two apart is the statement's
        // own verb: dynamic SQL *begins* with `EXECUTE`, a privilege follows a
        // `GRANT` or a `REVOKE` and is followed by `ON` or by another privilege.
        // **Decided where the word stands, not by the statement's first token.**
        // The paragraph above states the rule as *a privilege follows a `GRANT`
        // or a `REVOKE`*, and asking whether the statement *begins* with one
        // drops `ALTER DEFAULT PRIVILEGES … REVOKE EXECUTE ON FUNCTIONS`,
        // which is how a privilege is made to persist, and a `GRANT` inside a
        // `DO` block, which is how one is made conditional, into the
        // dynamic-SQL refusal. What separates them is what follows the word: a
        // privilege is followed by `ON` or by the comma of a privilege list, a
        // trigger by `FUNCTION` or `PROCEDURE`, and dynamic SQL by the text it
        // runs.
        let dynamic = builds_a_statement_out_of_text(tokens);
        if dynamic {
            found.push((
                statement.line,
                "`EXECUTE` builds a statement out of text in a migration".to_owned(),
                DYNAMIC,
            ));
        }
        if creates_a_virtual_table(tokens) {
            found.push((
                statement.line,
                "`CREATE VIRTUAL TABLE` is in a migration".to_owned(),
                VIRTUAL,
            ));
        }
        // **What the rules read, not how much of it there was.** A count of
        // tokens is defeated by rewriting tokens rather than removing them: a
        // same-length map renaming `EXECUTE`, `IMPORT` and `COPY` switches three
        // refusals off with `judged == counted` exactly and `corpus::` entirely
        // green. So what is added is what the tokens *say*, which no rewrite
        // leaves alone.
        judged = judged.wrapping_add(the_weight_of(tokens));
    }
    (found, judged)
}

/// Why a statement assembled at run time cannot be in a migration.
const DYNAMIC: &str = "a name assembled at run time is not in this file, so a `CREATE TABLE` \
                       behind it declares no bound and this gate reads the migration as \
                       creating nothing (§15.8) — the same reason `IMPORT FOREIGN SCHEMA` is \
                       refused. Write the statement out, which is also the only way a reader \
                       of the migrations can see what the database now holds";

/// Whether a statement creates a virtual table.
///
/// **A virtual table's rows are not in it.** `CREATE VIRTUAL TABLE … USING
/// fts5(…)` — FTS5 is compiled into the SQLite this build bundles — keeps its
/// rows in shadow tables SQLite creates and names itself, so no `CREATE` for
/// them is ever written and no `[tables.*]` entry could bound them, and the
/// pruner refuses a virtual table by name. Measured in `local-migrations/`:
/// `seen 0` and no finding. Read in both directories, because PostgreSQL has
/// no such statement and refusing one costs nothing.
fn creates_a_virtual_table(tokens: &[Token]) -> bool {
    tokens
        .windows(3)
        .any(|three| three[0].is("CREATE") && three[1].is("VIRTUAL") && three[2].is("TABLE"))
}

/// Why a virtual table cannot be in a migration.
const VIRTUAL: &str = "its rows live in shadow tables SQLite creates and names itself, so no \
                       `CREATE` for them is in this file and no `[tables.*]` entry can bound \
                       them (§15.8), and the pruner refuses a virtual table outright. Keep the \
                       data in an ordinary table, which an entry can bound and the pruner can \
                       delete from";

/// Why `COPY … FROM stdin` cannot be in a migration.
const STDIN: &str = "`cargo xtask dev` applies migrations through sqlx, which has no stdin to \
                     give it, so this migration cannot be applied by the tool that applies \
                     migrations — and the data block that follows is not SQL, so an \
                     apostrophe in it hides every statement after it from this gate. Write \
                     the rows as `INSERT`, or load them with `COPY … FROM PROGRAM`";

/// The lines of one file that hold nothing but a comment.
///
/// Produced by the same walk that cuts statements, so the hatch and the
/// detection agree about what a comment is — **in the file's own dialect**,
/// because SQLite's `[name]` holds characters PostgreSQL would read as a quote
/// opening, and the two walks must agree about where one ends.
pub(crate) fn comment_only_lines(text: &str, dialect: Dialect) -> Vec<usize> {
    let mut statements = Vec::new();
    let mut found = Vec::new();
    cut(text, 1, false, &mut statements, &mut found, &[], dialect);
    found.sort_unstable();
    found
}

/// How much a statement's tokens *say*, which is what the rules below read.
///
/// **Not how many there are, and not what they hold alone.** A count of tokens
/// compared between the two readers of a migration is defeated by rewriting
/// rather than removing: a same-length map renaming `EXECUTE`, `IMPORT` and
/// `COPY` switches three refusals off with the counts equal to the token.
/// Weighing the text closes that and is defeated in turn, because `Word(w)` and
/// `Name(w)` hold the same text and `Punct`, `Literal("")` and `Name("")` hold
/// nothing: rewriting every token to `Name` turns every keyword test in the
/// judging loop off and moves no number. `Token::is` answers `false` for a
/// `Name`, which is exactly the difference such a weight cannot see.
///
/// Weighing the kind with the text's *length* is defeated as well: `COPY` and
/// `COPz` are the same length, so a rename that keeps the spelling's shape
/// moves no number and switches three refusals off. And a sum is order-blind —
/// reversing a statement's tokens leaves it exactly where it was.
///
/// So this is a **fingerprint**, not a weight: every byte of the text, the kind
/// it is written as, and the position it stands in. Wrapping arithmetic, because
/// nothing reads the number but the equality beside it — its size means nothing
/// and its difference means everything.
///
/// Written once and read from three places — the two readers and the fixtures —
/// because a measure spelled twice is a measure that will one day be spelled two
/// ways, which is the defect this file carries the most paragraphs about.
fn the_weight_of(tokens: &[Token]) -> usize {
    /// The kind, so that a `Word` and a `Name` holding one text differ.
    fn kind(token: &Token) -> usize {
        match token {
            Token::Word(_) => 1,
            Token::Name(_) => 2,
            Token::Punct(_) => 3,
            Token::Literal(_) => 4,
        }
    }

    tokens.iter().enumerate().fold(0_usize, |sum, (at, token)| {
        let text = match token {
            Token::Word(word) | Token::Name(word) | Token::Literal(word) => word.as_str(),
            Token::Punct(_) => "",
        };
        // Every character, not the count of them; and the position, because
        // a sum is order-blind. Wrapping, because this is a fingerprint and
        // not a quantity — nothing reads it but the equality.
        let mut held = text.bytes().fold(0_usize, |held, byte| {
            held.wrapping_mul(31).wrapping_add(byte as usize)
        });
        // A punctuation token's character is its whole content. Seeded with the
        // kind alone, `.`, `(`, `)` and `,` would be one token to this.
        if let Token::Punct(character) = token {
            held = held.wrapping_mul(31).wrapping_add(*character as usize);
        }
        // **The kind last, in bits the mixing cannot reach.** Seeded first it is
        // absorbed: `Word("COPY")` and `Name("#mnx")` come to the same number,
        // measured, and `Token::is` answers `false` for a `Name` — so the one
        // rewrite the kind term exists to catch leaves the two readers agreeing to the bit.
        // Four kinds, two bits, and the text above them.
        let held = held.wrapping_mul(4).wrapping_add(kind(token));
        sum.wrapping_add(held.wrapping_mul(at.wrapping_add(1)))
    })
}

/// Whether a token is one a plpgsql statement can follow.
///
/// What tells `RAISE` the keyword from `raise` the identifier: a statement opens
/// a statement, so it is either first or follows one of these. A function name
/// follows `FUNCTION`, and a parameter follows `(`.
///
/// **`;` is not one of them.** The cutter makes
/// a token of `.`, `(`, `)` and `,` and of nothing else — a `;` finishes the
/// statement and the character never reaches `tokens` — so a
/// `matches!(token, Token::Punct(';'))` here would answer for
/// a value that cannot be built, and deleting it would leave every test green.
/// It would be worse than dead: this predicate bounds the `UPDATE` carve-out in
/// `the_path_redirects_in`, and the clause would read as though a `;` were one of the
/// boundaries holding that carve-out in. A rule that cannot fire is not protection, and a documented one
/// that cannot fire is read as protection.
/// `the_cutter_makes_a_token_of_four_punctuation_characters` pins the closed set
/// over every migration, so making `;` a token again fails there rather than
/// here.
fn opens_a_statement(token: &Token) -> bool {
    const WORDS: [&str; 5] = ["BEGIN", "THEN", "ELSE", "LOOP", "DO"];
    WORDS.iter().any(|word| token.is(word))
}

/// Cuts PostgreSQL into statements, as [`statements_in`] does.
#[cfg(test)]
fn statements(text: &str) -> Vec<Statement> {
    statements_in(text, Dialect::PostgreSql)
}

/// Cuts SQL into statements, dropping comments and honouring every quote form.
fn statements_in(text: &str, dialect: Dialect) -> Vec<Statement> {
    let mut found = Vec::new();
    let mut comments = Vec::new();
    cut(text, 1, false, &mut found, &mut comments, &[], dialect);
    // **In the order the file is written, which is the order they run.**
    // `cut` finishes the top level and *then* re-reads the routine bodies, so
    // in cut order every statement inside a routine comes back numbered after
    // every statement outside one however the file reads — and three lists this file keys on
    // that number answer *what did the last statement to decide this say*.
    // Measured on PostgreSQL 18.6, in cut order, both directions from the one
    // root: a type altered inside a conditional block and altered back outside
    // it is read through the block, so the gate refuses a column that ends
    // `bigint`; and a table dropped, re-created inside a block and dropped again
    // is read as still existing, so its entry goes on bounding a table that is gone.
    //
    // **The line is not a total order, and a tie on it goes the wrong way.** A
    // stable sort on it keeps the cut order where two statements share a line —
    // and the cut order puts every top-level statement before every nested one,
    // so a body and a statement *after* it on the same line come back reversed.
    // Measured, one line of ordinary SQL:
    //
    // ```text
    // DO $$ BEGIN DROP TABLE t; END $$; CREATE TABLE t (id INT);
    //   -> created at 1, dropped at 2
    // ```
    //
    // The table is created and then read as dropped, so its `[tables.t]` entry
    // is reported as bounding a table that is gone — a finding about a table
    // that is there, which this file's header calls the one thing this gate
    // must not produce.
    //
    // The path `cut` writes down is the position itself, so this sorts on where
    // the statement stands rather than on a number that several statements
    // share.
    found.sort_by(|left, right| left.path.cmp(&right.path));
    found
}

/// Reads one piece of SQL into statements, then their routine bodies.
#[expect(
    clippy::too_many_lines,
    reason = "one walk over one grammar; splitting it would put the states in \
              two places, which is how the escape-string case was missed"
)]
fn cut(
    text: &str,
    first_line: usize,
    nested: bool,
    found: &mut Vec<Statement>,
    comments: &mut Vec<usize>,
    base: &[usize],
    dialect: Dialect,
) {
    // Where this level's statements begin, so the paths below number them from
    // their own start rather than from the whole walk's.
    let start = found.len();
    let characters: Vec<char> = text.chars().collect();
    let mut at = 0_usize;
    let mut line = first_line;

    let mut tokens: Vec<Token> = Vec::new();
    let mut word = String::new();
    let mut bodies: Vec<(String, usize)> = Vec::new();
    let mut began: Option<usize> = None;
    let mut last_code_line = first_line;
    // Which lines hold nothing but a comment, so the hatch can tell a marker
    // from the same text inside a string.
    let mut code_on_this_line = false;
    // Whether a comment has begun on the line being read. A line is
    // comment-only when it held a comment and no code — decided when the line
    // ends, not when the comment starts.
    let mut had_a_comment = false;

    // A word ends when anything that is not part of one arrives.
    macro_rules! flush {
        () => {
            if !word.is_empty() {
                tokens.push(Token::Word(core::mem::take(&mut word)));
            }
        };
    }

    while at < characters.len() {
        let here = characters[at];
        let next = characters.get(at + 1).copied();

        // `--` to the end of the line.
        if here == '-' && next == Some('-') {
            flush!();
            had_a_comment = true;
            while at < characters.len() && characters[at] != '\n' {
                at += 1;
            }
            continue;
        }

        // `/* … */`, which nests in PostgreSQL.
        if here == '/' && next == Some('*') {
            flush!();
            let opened_on = line;
            let mut depth = 1_usize;
            at += 2;
            while at < characters.len() && depth > 0 {
                if characters[at] == '\n' {
                    line += 1;
                }
                if characters[at] == '/' && characters.get(at + 1) == Some(&'*') {
                    depth += 1;
                    at += 2;
                } else if characters[at] == '*' && characters.get(at + 1) == Some(&'/') {
                    depth -= 1;
                    at += 2;
                } else {
                    at += 1;
                }
            }
            // **Only when nothing else is on the line.** Marking the line
            // comment-only the moment the comment starts on it, regardless of
            // the code that follows, reads `/* one row */ CREATE TABLE x (…);`
            // as a comment line, and `excused` then walks straight over that
            // statement to give one `nc:bounded:` marker to the table *after*
            // it. The decision has to wait until the line is finished, which is
            // what the `\n` arm below already does for `--`.
            if line != opened_on {
                // A block comment that spanned lines: every line it covered
                // held nothing else, and the line it *ends* on is decided when
                // that line ends, like any other.
                for covered in (opened_on + 1)..line {
                    comments.push(covered);
                }
            }
            had_a_comment = true;
            continue;
        }

        // A single-quoted literal. **Backslash escapes only after `E`**:
        // PostgreSQL's `standard_conforming_strings` is on by
        // default, so `'C:\'` is a *closed* string holding a backslash. Treating
        // `\` as an escape everywhere makes the scanner run past the end of that
        // literal, find no further quote, and blank the rest of the file —
        // exactly the desynchronisation the `E` handling exists to prevent,
        // arriving from the other side.
        if here == '\'' {
            // **Inside a routine body a literal may be a statement.**
            // `DO $$ … EXECUTE 'CREATE TABLE …' … $$;` is idiomatic conditional
            // DDL, and blanking the literal's contents — right everywhere else,
            // because a table's name is never inside one — would make it invisible.
            // The contents are kept as a nested body when the enclosing
            // statement is a routine's.
            // Flushed first: the `E` is still in `word` at this point, not in
            // `tokens`, so testing `tokens.last()` before flushing looks at the
            // token before the prefix and never finds it.
            flush!();
            let escapes = matches!(
                tokens.last(),
                Some(Token::Word(prefix)) if prefix.eq_ignore_ascii_case("E")
            );
            if escapes {
                tokens.pop();
            }
            at += 1;
            let began_on = line;
            let mut literal = String::new();
            while at < characters.len() {
                let inside = characters[at];
                if inside == '\n' {
                    line += 1;
                }
                if escapes && inside == '\\' && at + 1 < characters.len() {
                    if characters[at + 1] == '\n' {
                        line += 1;
                    }
                    literal.push(characters[at + 1]);
                    at += 2;
                    continue;
                }
                if inside == '\'' {
                    // `''` is an escaped quote, not the end.
                    if characters.get(at + 1) == Some(&'\'') {
                        literal.push('\'');
                        at += 2;
                        continue;
                    }
                    at += 1;
                    break;
                }
                literal.push(inside);
                at += 1;
            }
            // **A routine body is a body however it is quoted.** Keeping a
            // literal only when it is already inside one leaves `DO '…'` and
            // `CREATE FUNCTION … AS '…'`, the older spelling of the same
            // statement, never re-scanned, and every `CREATE TABLE` inside
            // one invisible. Live PostgreSQL 18.6 creates those tables. Catching
            // the dollar-quoted form and not the quoted one
            // is a rule about punctuation rather than about SQL.
            //
            // `declares_a_routine()` cannot be asked yet — the statement is
            // still being cut — so every top-level literal is kept and the
            // filter below decides which are re-scanned.
            // **A `RAISE`'s literal is a message, not SQL**, and the tokens so
            // far are what say which statement this literal is in. Re-scanning
            // it reads `RAISE EXCEPTION 'this table is append-only; if you need
            // another, CREATE TABLE ghost (id INT)'` as creating `ghost` — a
            // finding naming a table nobody wrote, in the gate whose whole value
            // is that its error names the table. `0001_journal.sql`'s own
            // trigger is written this way and happens not to contain the words.
            // Which clause this literal belongs to, read from the tokens before
            // it: `BEGIN RAISE EXCEPTION '…'` is one cut statement, so the
            // *first* token is `BEGIN` and the one that matters is the last
            // `RAISE` or `EXECUTE` before here.
            // **`RAISE` the statement, not `raise` the name.** Taking the last
            // token spelled either way, `CREATE FUNCTION raise() RETURNS void AS
            // 'CREATE TABLE …'` silences its own body — the function name is a
            // `Word` like any other. PostgreSQL 18.6 accepts that and creates
            // the table, and such a gate reports none.
            //
            // What tells them apart is what comes *before*: a plpgsql statement
            // opens a statement, so `RAISE` and `EXECUTE` are either the first
            // token or follow one of the words that end the statement before.
            // A function name follows `FUNCTION`, and a parameter follows `(`.
            let is_a_message = tokens
                .iter()
                .enumerate()
                .rev()
                .find(|(at, token)| {
                    (token.is("RAISE") || token.is("EXECUTE"))
                        && at
                            .checked_sub(1)
                            .and_then(|before| tokens.get(before))
                            .is_none_or(opens_a_statement)
                })
                .is_some_and(|(_, token)| token.is("RAISE"));
            let carried = literal.clone();
            if (nested || !literal.trim().is_empty()) && !is_a_message {
                bodies.push((literal, began_on));
            }
            tokens.push(Token::Literal(carried));
            code_on_this_line = true;
            began.get_or_insert(line);
            last_code_line = line;
            continue;
        }

        // A quoted identifier, kept whole: `""` escapes the quote.
        if here == '"' {
            flush!();
            let mut name = String::new();
            at += 1;
            while at < characters.len() {
                let inside = characters[at];
                if inside == '\n' {
                    line += 1;
                }
                if inside == '"' {
                    if characters.get(at + 1) == Some(&'"') {
                        name.push('"');
                        at += 2;
                        continue;
                    }
                    at += 1;
                    break;
                }
                name.push(inside);
                at += 1;
            }
            tokens.push(Token::Name(name));
            code_on_this_line = true;
            began.get_or_insert(line);
            last_code_line = line;
            continue;
        }

        // **SQLite quotes a name two more ways**, `[name]` and `` `name` ``,
        // for compatibility with MS Access and MySQL. Read as PostgreSQL, the
        // bracket and the backtick are characters of a word, the word cannot
        // be an identifier, and the table is not seen at all: measured,
        // `CREATE TABLE [bracketed] (…)` in `local-migrations/` gives `seen 0`
        // and no finding, while the same table quoted with `"` is named as
        // unbounded. A bracket has no escape and ends at the first `]`; a
        // backtick doubles itself.
        if dialect == Dialect::Sqlite && (here == '[' || here == '`') {
            flush!();
            let closing = if here == '[' { ']' } else { '`' };
            let mut name = String::new();
            at += 1;
            while at < characters.len() {
                let inside = characters[at];
                if inside == '\n' {
                    line += 1;
                }
                if inside == closing {
                    if closing == '`' && characters.get(at + 1) == Some(&'`') {
                        name.push('`');
                        at += 2;
                        continue;
                    }
                    at += 1;
                    break;
                }
                name.push(inside);
                at += 1;
            }
            tokens.push(Token::Name(name));
            code_on_this_line = true;
            began.get_or_insert(line);
            last_code_line = line;
            continue;
        }

        // Dollar quoting: `$tag$ … $tag$`.
        // **A `$` inside an identifier is not a quote opening.** PostgreSQL:
        // "subsequent characters in an identifier can be letters, underscores,
        // digits, or dollar signs" — so read as a quote opening,
        // `ADD COLUMN geo$ip$hash TEXT` opens a `$ip$` quote, and the scan runs
        // to end of file looking for a close that never comes. Everything after
        // it is blanked, `CREATE TABLE` included. A rule about the character,
        // rather than about each spelling, is what closes the class.
        //
        // **The test is the character *before*, not whether a word is being
        // built.** `word.is_empty()` is a proxy for it and a leaky one: an
        // operator accumulates into `word` too, so with that proxy
        // `SET user_agent=$doc$don't edit this$doc$` does not open a quote — and
        // the apostrophe inside then opens a string literal that runs past the
        // closing tag to the end of the file, taking the `CREATE TABLE` after
        // it. Live PostgreSQL 18.6 applies that migration and creates the table.
        // What PostgreSQL actually says is that a `$` continues an identifier;
        // after anything that is not an identifier character, `$tag$` opens a
        // quote.
        let after_a_name = characters[..at]
            .iter()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || *before == '_' || *before == '$');
        if here == '$'
            && !after_a_name
            && let Some(tag) = dollar_tag(&characters[at..])
        {
            flush!();
            let closing: Vec<char> = tag.chars().collect();
            at += closing.len();
            let body_line = line;
            let mut body = String::new();
            while at < characters.len() {
                if characters[at..].starts_with(closing.as_slice()) {
                    at += closing.len();
                    break;
                }
                if characters[at] == '\n' {
                    line += 1;
                }
                body.push(characters[at]);
                at += 1;
            }
            let carried = body.clone();
            bodies.push((body, body_line));
            tokens.push(Token::Literal(carried));
            code_on_this_line = true;
            began.get_or_insert(line);
            last_code_line = line;
            continue;
        }

        if here == '\n' {
            flush!();
            if !code_on_this_line && had_a_comment {
                comments.push(line);
            }
            line += 1;
            code_on_this_line = false;
            had_a_comment = false;
            at += 1;
            continue;
        }

        if here == ';' {
            flush!();
            // **Inside a SQLite trigger's body a `;` ends a statement of the
            // body, not the trigger.** Cut here, `CREATE TRIGGER … BEGIN SELECT
            // RAISE(ABORT, '…'); END;` would be two statements, the second of them
            // `END` — and a table created inside the body would be read as
            // created at the top level, by a statement that is not one.
            if dialect == Dialect::Sqlite && a_trigger_body_is_open(&tokens) {
                code_on_this_line = true;
                at += 1;
                continue;
            }
            if began.is_some() {
                found.push(finished(
                    core::mem::take(&mut tokens),
                    core::mem::take(&mut bodies),
                    began.take().unwrap_or(line),
                    last_code_line,
                    nested,
                    found.last(),
                ));
            }
            tokens.clear();
            bodies.clear();
            began = None;
            code_on_this_line = true;
            at += 1;
            continue;
        }

        if here.is_whitespace() {
            flush!();
            at += 1;
            continue;
        }

        if here == '.' || here == '(' || here == ')' || here == ',' {
            flush!();
            tokens.push(Token::Punct(here));
            code_on_this_line = true;
            began.get_or_insert(line);
            last_code_line = line;
            at += 1;
            continue;
        }

        word.push(here);
        code_on_this_line = true;
        began.get_or_insert(line);
        last_code_line = line;
        at += 1;
    }
    flush!();
    if began.is_some() {
        found.push(finished(
            tokens,
            bodies,
            began.unwrap_or(line),
            last_code_line,
            nested,
            found.last(),
        ));
    }

    // Where each of this level's statements stands, before any body of it is
    // read: the walk pushes them in file order, so the offset is the position.
    for (offset, statement) in found[start..].iter_mut().enumerate() {
        statement.path = base.to_vec();
        statement.path.push(offset);
    }

    // **Routine bodies only.** A dollar-quoted run is a routine body in
    // `DO $$ … $$` and `CREATE FUNCTION … AS $$ … $$`, and is *data* everywhere
    // else — so scanning every one as SQL makes an `INSERT` of a document
    // containing the words `CREATE TABLE widgets` fail the build naming a table
    // nobody wrote, which is the one thing this gate must not do.
    //
    // **This level's statements, not every one the walk has produced.** Reading
    // all of `found`, each recursion would sweep the outer levels again — harmless
    // only because their bodies have already been taken out of them.
    let nested_bodies: Vec<(Vec<usize>, String, usize)> = found[start..]
        .iter_mut()
        // **Or is itself already inside one.** `DO $$ … EXECUTE 'CREATE TABLE …'
        // … $$;` nests twice: the outer statement is a `DO`, and the inner one
        // is an `EXECUTE`, which declares no routine — so a rule asking only for
        // a routine drops its body and never reads the DDL string. Inside a routine, everything
        // quoted is a candidate statement.
        // **And only where the quoted text is *run*.** "Inside a routine,
        // everything quoted is a candidate statement" is too wide in the one
        // direction the paragraph above says this gate must not be wrong in: an
        // ordinary `INSERT INTO access_records (note) VALUES ('we never EXECUTE
        // anything here')` inside a `DO` block then fails the build three ways at
        // once — as dynamic SQL, as a foreign-schema import, and as a `COPY`
        // from stdin — because the *literal* is re-cut and its first word
        // lands at position zero. The same sentence at the top level produces
        // nothing. What makes a quoted string a statement is that something
        // executes it, and in PL/pgSQL that word is `EXECUTE`.
        .filter(|statement| {
            statement.declares_a_routine()
                || (statement.nested && statement.tokens.iter().any(|token| token.is("EXECUTE")))
        })
        .flat_map(|statement| {
            let path = statement.path.clone();
            core::mem::take(&mut statement.bodies)
                .into_iter()
                .map(move |(body, body_line)| (path.clone(), body, body_line))
        })
        .collect();
    for (path, body, body_line) in nested_bodies {
        cut(&body, body_line, true, found, comments, &path, dialect);
    }
}

/// Whether the tokens so far are a SQLite `CREATE TRIGGER` whose body is open.
///
/// The body opens at the trigger's `BEGIN` and closes at the `END` that is not
/// a `CASE`'s: `SELECT CASE WHEN … END;` inside a body is an expression, and
/// counting it as the body's end would cut the trigger one statement early.
fn a_trigger_body_is_open(tokens: &[Token]) -> bool {
    let is = |at: usize, word: &str| tokens.get(at).is_some_and(|token| token.is(word));
    let trigger = is(0, "CREATE")
        && (is(1, "TRIGGER") || ((is(1, "TEMP") || is(1, "TEMPORARY")) && is(2, "TRIGGER")));
    if !trigger {
        return false;
    }
    let Some(opens) = tokens.iter().position(|token| token.is("BEGIN")) else {
        return false;
    };
    let mut cases = 0_usize;
    for token in &tokens[opens + 1..] {
        if token.is("CASE") {
            cases += 1;
        } else if token.is("END") {
            if cases == 0 {
                return false;
            }
            cases -= 1;
        }
    }
    true
}

/// Assembles one statement from what the walk collected.
fn finished(
    tokens: Vec<Token>,
    bodies: Vec<(String, usize)>,
    line: usize,
    ends_on: usize,
    nested: bool,
    previous: Option<&Statement>,
) -> Statement {
    Statement {
        tokens,
        line,
        shares_its_line: previous.is_some_and(|before| before.ends_on == line),
        ends_on,
        nested,
        bodies,
        // Written by `cut` once this level's statements are all in, because a
        // statement's position is its index among them.
        path: Vec::new(),
    }
}

impl Statement {
    /// Whether a dollar-quoted run inside this statement is a routine body.
    fn declares_a_routine(&self) -> bool {
        let first = self.tokens.first();
        if first.is_some_and(|token| token.is("DO")) {
            return true;
        }
        first.is_some_and(|token| token.is("CREATE"))
            && self
                .tokens
                .iter()
                .any(|token| token.is("FUNCTION") || token.is("PROCEDURE"))
    }
}

/// The `$tag$` opening at the start of `rest`, if there is one.
fn dollar_tag(rest: &[char]) -> Option<String> {
    debug_assert_eq!(rest.first(), Some(&'$'));
    let mut tag = String::from("$");
    for (index, character) in rest[1..].iter().enumerate() {
        match character {
            '$' => {
                tag.push('$');
                return Some(tag);
            }
            // A tag is an identifier: letters and underscores, and **never a
            // digit first** — `$1$` is a parameter followed by a `$`, not a
            // quote, and reading it as one would swallow the rest of the file.
            c if *c == '_' || c.is_alphabetic() || (index > 0 && c.is_numeric()) => tag.push(*c),
            _ => return None,
        }
    }
    None
}

/// Every table one statement creates.
///
/// **All of them, not the first.** `CREATE SCHEMA … CREATE TABLE a … CREATE
/// TABLE b` is one statement declaring two tables, which is valid PostgreSQL —
/// and a reader of the first sees only `a`, so a `nc:bounded:` marker above it
/// hides both.
fn created_tables(statement: &Statement) -> Vec<String> {
    let mut found = Vec::new();
    let tokens = &statement.tokens;

    // `SELECT … INTO name FROM …` creates a table with no `CREATE` in it. Only
    // at the top level: inside a routine body it is plpgsql's assignment into a
    // variable, and reading that as a table would report one nobody wrote.
    // **A CTE puts `WITH` first**, so asking only whether the statement begins
    // with `SELECT` misses `WITH failed AS (…) SELECT id INTO login_attempts
    // FROM failed;`, which creates a table.
    let opens_a_select = tokens.first().is_some_and(|token| token.is("SELECT"))
        || (tokens.first().is_some_and(|token| token.is("WITH"))
            && tokens.iter().any(|token| token.is("SELECT")));
    if !statement.nested && opens_a_select {
        // **Every `INTO` in the statement, not the one after the first
        // `SELECT`.** A CTE's own `SELECT` comes first, so walking from there
        // meets the CTE's `FROM` and gives up before reaching the `INTO` that
        // names the new table. What identifies a creating `INTO` is what is
        // *before* it: `INSERT INTO` is not one, and everything else is.
        for (at, token) in tokens.iter().enumerate() {
            if !token.is("INTO") {
                continue;
            }
            // **`INSERT INTO` and `MERGE INTO` both write to a table that
            // already exists.** Skipping only the first reports
            // `WITH wanted AS (SELECT 1) MERGE INTO install_marker …` — valid
            // PostgreSQL 18.6, verified applied — as *creating*
            // `install_marker`, at the line of the `MERGE`, and tells the
            // developer to bound a table that has already declared, correctly, that
            // it cannot grow: a finding naming a table nobody wrote, which is
            // the one thing this gate must not produce.
            if at > 0 && (tokens[at - 1].is("INSERT") || tokens[at - 1].is("MERGE")) {
                continue;
            }
            let mut next = at + 1;
            // `INTO STRICT x`, the persistence keywords, and the optional
            // `TABLE`.
            //
            // **`TABLE` is on this list, or it is the name that gets
            // reported.** The grammar is
            // `SELECT … INTO [ TEMPORARY | TEMP | UNLOGGED ] [ TABLE ] new_table`,
            // so `SELECT id INTO TABLE login_attempts FROM …` — verified on
            // PostgreSQL 18.6 to really create `login_attempts` — would read as
            // creating a table called `table`. Measured, that buys: the finding
            // names `table`, the marker the finding's own `fix` text offers
            // excuses `table`, and `login_attempts` is created
            // unbounded with the whole gate green and
            // `the_tables_the_gate_inspects_are_the_ones_the_migrations_make`
            // passing, because the excused name is filtered out before it looks.
            // A finding naming a table nobody wrote is the one thing this gate
            // must not produce, and here it also hides one that is.
            //
            // A table genuinely called `table` has to be quoted, and `Token::is`
            // matches a `Word` and not a quoted identifier — so `INTO "table"`
            // and `INTO TABLE "table"` both still read as the table.
            // **`GLOBAL` and `LOCAL` qualify `TEMPORARY`, and are on the list.**
            // Without them, `SELECT id INTO GLOBAL TEMP scratch FROM …` stops the
            // walk on the qualifier, so the gate reports a table called
            // `global` — a finding naming a table nobody wrote — and the `TEMP`
            // return that excuses a temporary table is never reached. The
            // sibling reader `table_declared_at` steps over both under the
            // comment *"`GLOBAL` and `LOCAL` only ever qualify `TEMPORARY`"*:
            // one rule written twice, and the two copies have to agree.
            //
            // **Only in front of `TEMP`/`TEMPORARY`, which is the mirror.**
            // Both are unreserved keywords, so `SELECT id INTO global FROM t`
            // really does create a table called `global`; stepping over them
            // unconditionally would lose it. `TABLE` carries no such mirror —
            // it is reserved and cannot be a bare table name.
            if tokens
                .get(next)
                .is_some_and(|token| token.is("GLOBAL") || token.is("LOCAL"))
                && tokens
                    .get(next + 1)
                    .is_some_and(|token| token.is("TEMP") || token.is("TEMPORARY"))
            {
                next += 1;
            }
            while tokens.get(next).is_some_and(|token| {
                token.is("STRICT")
                    || token.is("UNLOGGED")
                    || token.is("TEMP")
                    || token.is("TEMPORARY")
                    || token.is("TABLE")
            }) {
                if tokens[next].is("TEMP") || tokens[next].is("TEMPORARY") {
                    return found;
                }
                next += 1;
            }
            if let Some(name) = qualified(tokens, tokens.get(next)) {
                found.push(name);
            }
            return found;
        }
        return found;
    }

    // **A schema element belongs to its schema.** `CREATE SCHEMA archive CREATE
    // TABLE t (…)` creates `archive.t`, and reading the name bare would credit
    // it to `[tables.t]` — the mis-attribution qualified names are read to prevent,
    // arriving through a shape that has no dot in it at all.
    let schema = tokens
        .first()
        .filter(|token| token.is("CREATE"))
        .and(tokens.get(1))
        .filter(|token| token.is("SCHEMA"))
        // `CREATE SCHEMA AUTHORIZATION noctua CREATE TABLE t (…)` names the
        // schema after the owner, so the name is not always at index 2 — read
        // there, the gate reports `AUTHORIZATION.auth_five` for a table PostgreSQL creates
        // as `noctua.auth_five`.
        .and_then(|_| {
            let at = if tokens.get(2).is_some_and(|token| token.is("AUTHORIZATION")) {
                3
            } else {
                2
            };
            tokens.get(at)
        })
        // **Spelled as PostgreSQL resolves it, not as it is written.** Read
        // verbatim, `CREATE SCHEMA Archive CREATE TABLE
        // access_records (…)` produces `Archive.access_records` while the
        // database creates `archive.access_records` — a name that matches no
        // `[tables.…]` entry, so the table reads as unbounded *and* the finding
        // names something the developer cannot find in `psql`. Every other
        // reader here folds an unquoted word and keeps a quoted one, and so
        // does this one, which is the case `resolved` exists for.
        .and_then(Token::resolved)
        .filter(|name| name != "public");

    // **A `RAISE`'s literal is a message, not SQL.** Inside a routine body every
    // string literal is re-scanned, which is what makes `EXECUTE 'CREATE TABLE
    // …'` visible — and it would also read
    // `RAISE EXCEPTION 'this table is append-only; if you need another, CREATE
    // TABLE ghost (id INT)'` as creating `ghost`. A finding naming a table
    // nobody wrote is the wrong-direction failure, in the gate whose whole value
    // is that its error names the table.
    for offset in 0..tokens.len() {
        if let Some(name) = table_declared_at(&tokens[offset..], tokens) {
            found.push(match &schema {
                // A name that already carries a schema keeps it.
                Some(schema) if !name.contains('.') => format!("{schema}.{name}"),
                _ => name,
            });
        }
    }
    found
}

/// The columns the migrations declare for one table and the statement that
/// declared them, or `None` if none creates it.
///
/// The **last** statement to declare it wins: a table dropped and re-created has
/// the columns of the second one, which is what a reader of the migrations in
/// order would conclude.
///
/// **And the caller needs to know *when*, which is why the index comes back.**
/// The columns alone let every `ALTER TABLE` in the file apply to the last
/// `CREATE TABLE`, whenever each ran. Measured: `ALTER TABLE
/// access_records DROP COLUMN recorded`, then `DROP TABLE access_records`, then
/// a `CREATE TABLE` declaring `recorded` again — a set of migrations that leaves
/// the column in place — is then reported as *"orders by `recorded`, which no
/// migration declares as a column of it"*, with a `fix` listing the columns as
/// `id` about a statement two lines above that declares both. That is the
/// refusing-something-legitimate direction, and [`what_the_migrations_leave`]
/// one function away handles exactly it.
fn declared_columns_of(created: &[Created], name: &str) -> Option<(usize, Vec<String>)> {
    created
        .iter()
        .rfind(|table| table.name == name)
        .map(|table| (table.at, table.columns.clone()))
}

/// Every bound whose `order_by` names no column the migrations declare.
///
/// **And the column each bound orders by.** `order_by` is written into the
/// pruner's `ORDER BY`; a name that is not a column fails the first time the
/// pruner runs, which is when the table is at its limit. Checked against the
/// migrations rather than against a live database, because the migrations are
/// what builds every database there will ever be — and a database check
/// ratifies a column added by hand, is satisfied by a decoy of the same bare
/// name in another schema, and cannot pass at all on the empty database CI
/// starts with.
fn order_by_names_a_column(
    all: &[Created],
    changed_columns: &[ColumnChange],
    domains: &[(String, String)],
    limits: &str,
    database: nc_catalog::Database,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for table in nc_catalog::TABLES
        .iter()
        .filter(|table| table.database == database)
    {
        // The last statement to declare it wins, which is what `ALTER TABLE …
        // RENAME` and a re-creation both mean.
        let Some((since, created)) = declared_columns_of(all, table.name) else {
            continue;
        };
        // **And what every `ALTER TABLE` did to it afterwards.** Reading only
        // the `CREATE` leaves `ALTER TABLE … RENAME COLUMN id TO record_id`
        // invisible, and the pruner's next `ORDER BY "id"` fails with `column
        // "id" does not exist` — the failure this check exists to prevent.
        //
        // **Afterwards, and in order.** Applying every change to the last
        // `CREATE TABLE` whatever its own position reads a column dropped and
        // then restored by a later `DROP TABLE` + `CREATE TABLE` as gone — a
        // migration set that leaves the column in place, refused. `at` orders
        // the two lists against each other, the same way
        // [`what_the_migrations_leave`] reads.
        let mut columns = created;
        let mut afterwards: Vec<&ColumnChange> = changed_columns
            .iter()
            .filter(|change| change.table == table.name && change.at >= since)
            .collect();
        afterwards.sort_by_key(|change| change.at);
        for change in afterwards {
            if let Some(gone) = &change.removed {
                columns.retain(|column| column != gone);
            }
            if let Some(new) = &change.added {
                columns.push(new.clone());
            }
        }
        // **An empty column list is "cannot say", not "pass".**
        // `CREATE TABLE … AS SELECT` names its columns in a query, so nothing
        // here can tell whether `order_by` is one of them or what it sorts like
        // — and the pruner deletes by that order regardless. Read as "pass",
        // both of this rule's halves go silent.
        if columns.is_empty() {
            findings.push(Finding {
                file: "catalog/limits.toml".to_owned(),
                line: line_of_entry(limits, table.name),
                what: format!(
                    "`{}` orders by `{}`, and the migrations do not say what it is",
                    table.name, table.order_by
                ),
                fix: "a column whose type cannot be read is one whose sort order cannot \
                      be judged, and the pruner deletes by that order. `CREATE TABLE … AS \
                      SELECT` and `SELECT … INTO` name their columns in a query: declare \
                      the table with a column list instead"
                    .to_owned(),
            });
            continue;
        }
        if columns.iter().any(|column| *column == table.order_by) {
            // **And it cannot be null.** `ORDER BY … ASC` puts NULLs last, so a
            // nullable ordering column makes every row missing that value the
            // newest rows in the table and the last ever pruned — the table then
            // grows past its limit with the pruner running and reporting
            // success. Measured on PostgreSQL 18.6: with five of ten rows null,
            // `ORDER BY at ASC LIMIT 4` answers `1, 2, 3, 4` and the null rows
            // are untouchable.
            //
            // Both of today's tables order by a primary key, so this is the
            // check that has to stop the third one.
            // **And it sorts in creation order.** `ORDER BY … ASC` on a text
            // column is byte-lexicographic, and a `text` column holding a
            // Crockford identifier holds an encoding that
            // is twelve characters below `32^12` and thirteen above, from about
            // 2034. From the first thirteen-character row until the last
            // twelve-character one is gone, ascending order is not creation
            // order. Measured with the real pruner over six identifiers minted
            // in order: it removes the oldest *and the third-newest*, and
            // reports an ordinary `Pruned`.
            //
            // `base32`'s own header says "sorting by identifier value still
            // works, and that is what anything reading these names should do" —
            // and SQL sorts the text, not the value. So the ordering column has
            // to be a type whose ascending order is numeric.
            findings.extend(the_ordering_column_sorts_and_is_there(
                all,
                changed_columns,
                domains,
                limits,
                table,
            ));
            continue;
        }
        let created = columns;
        findings.push(Finding {
            file: "catalog/limits.toml".to_owned(),
            // The `order_by` line itself, which is the one to edit — the entry's
            // header is where the sibling site points, because a table nothing
            // creates has no key to point at.
            line: line_of_key(limits, table.name, "order_by"),
            what: format!(
                "`{}` orders by `{}`, which no migration declares as a column of it",
                table.name, table.order_by
            ),
            fix: format!(
                "the pruner writes `ORDER BY {}` into its DELETE, so a name that is not a \
                 column fails the first time it runs — which is when the table is at its \
                 limit. The columns the migrations declare are: {}",
                table.order_by,
                created.join(", ")
            ),
        });
    }
    findings
}

/// Every column change one `ALTER TABLE` statement makes.
///
/// `ADD COLUMN a`, `DROP COLUMN a`, and `RENAME COLUMN a TO b` — the three that
/// change which names a table answers to. `ALTER COLUMN … TYPE` changes a type
/// and not a name, so it is not one.
fn column_changes(statement: &Statement) -> Vec<ColumnChange> {
    /// Words that may sit between the keyword and the name.
    const SKIPPED: [&str; 4] = ["COLUMN", "IF", "EXISTS", "NOT"];
    /// What `ADD` introduces when it is not introducing a column.
    const NOT_A_COLUMN: [&str; 5] = ["constraint", "primary", "foreign", "unique", "check"];

    let tokens = &statement.tokens;
    // **`ALTER TABLE` need not be the statement's first two words.** Requiring
    // them at index 0, while `created_tables` scans every offset, makes a
    // `DO $$ BEGIN ALTER TABLE … END $$` body, whose first cut statement begins
    // `BEGIN`, invisible to this and visible to that. Measured:
    //
    // ```text
    // CREATE inside a DO block  ->  tables seen: ["ghost"]
    // ALTER  inside a DO block  ->  column changes seen: []
    // ```
    //
    // So `DO $$ BEGIN ALTER TABLE access_records ALTER COLUMN recorded TYPE
    // TEXT; END $$;` passes the gate, while the same clause written *second* in
    // the body is caught. `changes_in`'s own header calls `DO $$ … $$` "the
    // most idiomatic conditional DDL there is", and it is read for
    // alterations as well as for creations.
    let Some(alter) = tokens.windows(2).position(|pair| {
        pair.first().is_some_and(|token| token.is("ALTER"))
            && pair.get(1).is_some_and(|token| token.is("TABLE"))
    }) else {
        return Vec::new();
    };
    let tokens = &tokens[alter..];
    let at = past_the_alter_prefix(tokens, 2);
    let Some(table) = qualified(tokens, tokens.get(at)) else {
        return Vec::new();
    };

    let named = |after: usize| -> Option<String> {
        let mut at = after;
        while tokens
            .get(at)
            .is_some_and(|token| SKIPPED.iter().any(|word| token.is(word)))
        {
            at += 1;
        }
        tokens.get(at)?.resolved()
    };

    let mut found = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        // The `COLUMN` keyword is optional after `ADD` and `DROP`, and required
        // after `RENAME` — without it, `RENAME TO` is a *table* rename, which
        // `renamed` reads.
        if token.is("RENAME") && tokens.get(index + 1).is_some_and(|next| next.is("COLUMN")) {
            let to = tokens
                .iter()
                .skip(index)
                .position(|token| token.is("TO"))
                .and_then(|at| tokens.get(index + at + 1))
                .and_then(Token::resolved);
            found.push(ColumnChange {
                table: table.clone(),
                removed: named(index + 1),
                added: to,
                kind: None,
                non_null: None,
                at: 0,
                nested: false,
                how: How::Renamed,
            });
        } else if token.is("ADD")
            && let Some(name) = named(index + 1)
            && !NOT_A_COLUMN.contains(&name.as_str())
        {
            let (kind, non_null) = declared_after(tokens, index + 1);
            found.push(ColumnChange {
                table: table.clone(),
                removed: None,
                added: Some(name),
                kind,
                non_null,
                at: 0,
                nested: false,
                how: How::Added,
            });
        } else if token.is("DROP")
            && tokens.get(index + 1).is_some_and(|next| next.is("COLUMN"))
            && let Some(name) = named(index + 1)
        {
            found.push(ColumnChange {
                table: table.clone(),
                removed: Some(name),
                added: None,
                kind: None,
                non_null: None,
                at: 0,
                nested: false,
                how: How::Dropped,
            });
        } else if token.is("ALTER")
            && index > 0
            && let Some(name) = named(index + 1)
        {
            // **`ALTER COLUMN` changes what a column *is*, and this reads it.**
            // `column_changes` is about names — added, dropped, renamed —
            // and says so: "`ALTER COLUMN … TYPE` changes a type and not a name,
            // so it is not one." True of names; and with the two rules that ask what
            // the ordering column sorts like and whether it can be null reading
            // the `CREATE TABLE` alone, one `ALTER COLUMN recorded TYPE
            // TEXT` in a later migration turns both of them off, with the gate
            // green and the resulting schema identical to the one it refuses.
            let (kind, non_null) = retyped_after(tokens, index + 1);
            if kind.is_some() || non_null.is_some() {
                found.push(ColumnChange {
                    table: table.clone(),
                    removed: None,
                    added: Some(name),
                    kind,
                    non_null,
                    at: 0,
                    nested: false,
                    how: How::Altered,
                });
            }
        }
    }
    found
}

/// The type an `ADD COLUMN` gives, and whether it forbids NULL.
///
/// Reads from the token after `ADD`: the optional `COLUMN`, `IF NOT EXISTS`, the
/// name, then the type phrase, then this column's constraints up to the next
/// top-level comma.
fn declared_after(tokens: &[Token], from: usize) -> (Option<String>, Option<bool>) {
    let mut at = from;
    if tokens.get(at).is_some_and(|token| token.is("COLUMN")) {
        at += 1;
    }
    if tokens.get(at).is_some_and(|token| token.is("IF")) {
        let past = the_existence_test_after_if(tokens.get(at + 1..).unwrap_or_default());
        if past > 0 {
            at += 1 + past;
        }
    }
    // Past the name.
    at += 1;
    let (kind, after) = type_phrase(tokens, at);
    (kind, Some(forbids_null(tokens, after)))
}

/// What an `ALTER COLUMN` decides: a new type, or whether NULL is allowed.
///
/// `SET DATA TYPE` and `TYPE` are the same clause; `SET NOT NULL` and
/// `DROP NOT NULL` are the other one. Anything else this clause can say —
/// `SET DEFAULT`, `SET STATISTICS`, `SET STORAGE` — decides neither, and returns
/// nothing rather than a guess.
fn retyped_after(tokens: &[Token], from: usize) -> (Option<String>, Option<bool>) {
    let mut at = from;
    if tokens.get(at).is_some_and(|token| token.is("COLUMN")) {
        at += 1;
    }
    // Past the name.
    at += 1;
    let mut kind = None;
    let mut non_null = None;
    if tokens.get(at).is_some_and(|token| token.is("SET"))
        && tokens.get(at + 1).is_some_and(|token| token.is("DATA"))
    {
        at += 2;
    }
    if tokens.get(at).is_some_and(|token| token.is("TYPE")) {
        kind = type_phrase(tokens, at + 1).0;
    } else if tokens.get(at + 1).is_some_and(|token| token.is("NOT"))
        && tokens.get(at + 2).is_some_and(|token| token.is("NULL"))
    {
        if tokens.get(at).is_some_and(|token| token.is("SET")) {
            non_null = Some(true);
        } else if tokens.get(at).is_some_and(|token| token.is("DROP")) {
            non_null = Some(false);
        }
    }
    (kind, non_null)
}

/// What the ordering column has to be, once it is known to exist.
///
/// Split out because the two rules — what it sorts like, and whether it can be
/// null — carry the measurements that justify them, and the loop above carries
/// the reading of every `ALTER TABLE`.
fn the_ordering_column_sorts_and_is_there(
    all: &[Created],
    changed_columns: &[ColumnChange],
    domains: &[(String, String)],
    limits: &str,
    table: &nc_catalog::TableLimit,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let at = line_of_entry(limits, table.name);
    match resolved_type_of(all, changed_columns, domains, table.name, table.order_by) {
        Some(kind) if SORTS_IN_CREATION_ORDER.contains(&kind.as_str()) => {}
        Some(kind) => findings.push(Finding {
            file: "catalog/limits.toml".to_owned(),
            line: at,
            what: format!(
                "`{}` orders by `{}`, which is `{kind}`",
                table.name, table.order_by
            ),
            fix: "the pruner writes `ORDER BY … ASC`. SQL sorts a text column byte \
          by byte, so a variable-width encoding — a Crockford identifier \
          is one, twelve characters until about 2034 and thirteen after — \
          stops sorting in creation order and the pruner starts deleting \
          the newest rows. A bare `timestamp` carries no zone, so at a \
          daylight-saving fall-back a local-clock column repeats an hour; \
          `real` and `double precision` hold about seven and fifteen \
          significant digits, and a millisecond epoch needs thirteen. \
          Order by an exact, monotonic column: `bigint` holding \
          milliseconds, a sequence, or `timestamptz`. A domain over one of \
          those is read as what it is built on"
                .to_owned(),
        }),
        None => findings.push(Finding {
            file: "catalog/limits.toml".to_owned(),
            line: at,
            what: format!(
                "`{}` orders by `{}`, and the migrations do not say what it is",
                table.name, table.order_by
            ),
            fix: "this is the hole the type rule sat in, not an exemption from it: \
          a column whose type cannot be read is one whose sort order \
          cannot be judged, and the pruner deletes by that order. \
          `CREATE TABLE … AS SELECT` names its columns in a query — \
          declare the table with a column list instead — and a domain is \
          followed only when a migration in this tree declares it"
                .to_owned(),
        }),
    }
    if !nulls_are_impossible(all, changed_columns, table.name, table.order_by) {
        findings.push(Finding {
            file: "catalog/limits.toml".to_owned(),
            line: at,
            what: format!(
                "`{}` orders by `{}`, which may be NULL",
                table.name, table.order_by
            ),
            fix: "the pruner writes `ORDER BY … ASC`, and PostgreSQL sorts NULLs \
          last — so every row missing this value would be the newest row \
          in the table and the last one ever removed, while the table \
          grows past its limit with the pruner reporting success. \
          Declare the column `NOT NULL` in the migration, or order by one \
          that is"
                .to_owned(),
        });
    }
    findings
}

/// A numbered migration whose body was removed.
///
/// **`0003_sequence_is_not_negative.sql` truncated to its comment header** — its
/// `CHECK (sequence >= 0)` gone, the file and its number still there — still
/// reads `numbered once each and in order` to the numbering gate, and without
/// this the run ends `check passed`. That
/// constraint's own header records what it was measured stopping: a row
/// recording `Escalate` by `attacker` that sat in the journal, could never be
/// removed, and which `verify(0)` called a clean history.
///
/// Every neighbouring gate is about *what* the migrations say; this asks
/// whether one still says anything. `sqlx` applies an empty file without
/// complaint and its checksum then matches the empty file, so nothing downstream
/// can tell either.
fn runs_nothing(relative: &str) -> Finding {
    Finding {
        file: relative.to_owned(),
        line: 1,
        what: "this migration runs no statement".to_owned(),
        fix: "a numbered migration that does nothing is one whose body was removed — \
              sqlx applies it, records it, and its checksum then matches the empty \
              file, so nothing downstream can tell. Restore what it did, or delete \
              the file if it was never applied anywhere"
            .to_owned(),
    }
}
/// The line one key of one table's entry sits on.
///
/// **The header is the entry; the key is the edit.** `line_of_entry` finds
/// `[tables.<name>]`, which is right for a finding about the entry as a whole and
/// one screen away from a finding about a value inside it. Falls back to the
/// header when the key is not there, which is itself a finding somebody else
/// makes.
fn line_of_key(limits: &str, table: &str, key: &str) -> usize {
    let header = format!("[tables.{table}]");
    let mut inside = false;
    for (at, line) in limits.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == header {
            inside = true;
            continue;
        }
        if inside {
            if trimmed.starts_with('[') {
                break;
            }
            if trimmed.starts_with(key) && trimmed[key.len()..].trim_start().starts_with('=') {
                return at + 1;
            }
        }
    }
    line_of_entry(limits, table)
}

/// The line `[tables.<name>]` is declared on in `catalog/limits.toml`.
///
/// **A finding about an entry reported at line 1**, with the entries running
/// from near the top of the file to its end, sits beside migration-side
/// findings in this same gate that carry real line numbers, so one run shows
/// both kinds and teaches a reader that the column is decorative.
///
/// Line 1 when the entry cannot be found, which is where a reader who opens the
/// file lands anyway.
fn line_of_entry(limits: &str, table: &str) -> usize {
    let header = format!("[tables.{table}]");
    limits
        .lines()
        .position(|line| line.trim() == header)
        .map_or(1, |index| index + 1)
}

/// The domain a `CREATE DOMAIN <name> [AS] <type>` declares, and its base type.
///
/// `CREATE DOMAIN` is the one shape that lets a column carry a name of its own
/// and still sort like an integer. Following it here is what keeps the type rule
/// from needing a second escape hatch in `catalog/limits.toml`.
fn declared_domain(statement: &Statement) -> Option<(String, String)> {
    let tokens = &statement.tokens;
    if !tokens.first()?.is("CREATE") || !tokens.get(1)?.is("DOMAIN") {
        return None;
    }
    let (name, after) = type_phrase(tokens, 2);
    let name = name?;
    let mut at = after;
    if tokens.get(at).is_some_and(|token| token.is("AS")) {
        at += 1;
    }
    // `type_phrase` reads the name the same way it reads a type, so a qualified
    // `CREATE DOMAIN public.epoch_millis` gives back the bare name — which is
    // how a column that uses it will be spelled.
    Some((name, type_phrase(tokens, at).0?))
}

/// The type phrase starting at `from`, and the index just past it.
///
/// The same reading [`declared_types`] does inside a column list, at the top
/// level of an `ALTER TABLE`: parenthesised precision is skipped, a schema
/// qualification is dropped rather than mistaken for the type, and the phrase
/// ends at the first word that begins a constraint or a clause.
fn type_phrase(tokens: &[Token], from: usize) -> (Option<String>, usize) {
    let mut words: Vec<String> = Vec::new();
    let mut at = from;
    let mut depth = 0_usize;
    while let Some(token) = tokens.get(at) {
        match token {
            Token::Punct('(') => depth += 1,
            Token::Punct(')') if depth > 0 => depth -= 1,
            Token::Punct('.') if depth == 0 => words.clear(),
            Token::Punct(',') if depth == 0 => break,
            _ if depth == 0 => {
                let Some(word) = token.name() else { break };
                let lowered = word.to_lowercase();
                if ENDS_A_TYPE.contains(&lowered.as_str()) || lowered == "using" {
                    break;
                }
                words.push(lowered);
            }
            _ => {}
        }
        at += 1;
    }
    let phrase = if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    };
    (phrase, at)
}

/// The column types that cannot hold a null, whatever else the declaration says.
///
/// **`SERIAL` is `NOT NULL`, and both readers know it.** PostgreSQL expands
/// `bigserial` to `bigint NOT NULL DEFAULT nextval(…)`, so the column is not
/// nullable and `attnotnull` says so — corroborated on a live server for both
/// `GENERATED ALWAYS AS IDENTITY` and `BIGSERIAL`. `SORTS_IN_CREATION_ORDER`
/// below names all six spellings as safe to order by, and the two rules that
/// decide whether an ordering column may be null have to agree. Refusing them
/// is the *too strong* half: the two most idiomatic ways of declaring an
/// insertion-order column, refused, and a migration cannot be edited once it
/// has been applied, so the advice would be about a file the developer may no
/// longer change.
const CANNOT_HOLD_A_NULL: [&str; 6] = [
    "serial",
    "serial2",
    "serial4",
    "serial8",
    "smallserial",
    "bigserial",
];

/// Whether this column's constraints make NULL impossible.
///
/// Four shapes say it, and the reader of a created table's columns knows the
/// same four: `NOT NULL`, `PRIMARY KEY`, a serial type, and
/// `GENERATED … AS IDENTITY`, which PostgreSQL documents as implying
/// `NOT NULL`. An identity column is the one shape that is monotonic
/// with insertion by construction rather than by a promise, so a rule that
/// reported it as nullable would refuse the best answer to its own question.
///
/// The constraints run from `from` to the next top-level comma.
fn forbids_null(tokens: &[Token], from: usize) -> bool {
    let mut depth = 0_usize;
    let mut at = from;
    while let Some(token) = tokens.get(at) {
        match token {
            Token::Punct('(') => depth += 1,
            Token::Punct(')') if depth > 0 => depth -= 1,
            Token::Punct(',') if depth == 0 => break,
            _ if depth == 0
                && token.is("NOT")
                && tokens.get(at + 1).is_some_and(|next| next.is("NULL")) =>
            {
                return true;
            }
            _ if depth == 0
                && token.is("PRIMARY")
                && tokens.get(at + 1).is_some_and(|next| next.is("KEY")) =>
            {
                return true;
            }
            _ if depth == 0 && token.is("IDENTITY") => return true,
            // A serial type is `NOT NULL` by expansion, and the list is shared
            // with the reader below so the two cannot answer differently.
            _ if depth == 0
                && token.resolved().is_some_and(|word| {
                    CANNOT_HOLD_A_NULL.contains(&word.to_lowercase().as_str())
                }) =>
            {
                return true;
            }
            _ => {}
        }
        at += 1;
    }
    // An `ADD COLUMN` with no `NOT NULL` allows null, which is a decision and
    // not a silence: the column exists and it is nullable.
    false
}

/// The types whose ascending order is the order rows were created in.
///
/// Integers, exact decimals, dates and a timestamp that carries its zone. Not
/// `text`: SQL sorts it byte by byte, and an identifier encoded in a variable
/// width stops sorting chronologically the day it gains a character.
///
/// **`timestamp` is absent.** Bare `timestamp` is
/// `timestamp without time zone`, which records a reading with no answer to
/// *whose clock took it*; at a daylight-saving fall-back a local-clock column
/// repeats an hour, and for that hour "oldest first" is wrong. The type that
/// carries the answer is `timestamptz`.
///
/// **Floating point is absent on purpose.** `real` and `double precision` sort
/// numerically, so the ordering is not the objection: `real` holds about seven
/// significant digits, and a millisecond epoch needs thirteen, so every row in a
/// span of hours compares equal. An exact type is what an ordering column is
/// for, and the diagnostic says so.
///
/// A list is what this has to be — the alternative is asking a database, and
/// `Created`'s own header records why the migrations are the authority instead —
/// so a type it does not name is refused with a fix rather than admitted
/// silently. A domain is the one shape that escapes the list without editing it:
/// `resolved_type_of` follows `CREATE DOMAIN … AS <type>` to what it is built on.
const SORTS_IN_CREATION_ORDER: [&str; 18] = [
    "bigint",
    "int8",
    "integer",
    "int",
    "int4",
    "smallint",
    "int2",
    "bigserial",
    "serial8",
    "serial",
    "serial4",
    "smallserial",
    "serial2",
    "numeric",
    "decimal",
    "date",
    "timestamptz",
    "timestamp with time zone",
];

/// What every statement together leaves each column of `table` being.
///
/// **The last statement to decide wins, and every clause is a statement.** A
/// type rule reading the `CREATE TABLE` alone is left by `ALTER COLUMN … TYPE TEXT` in a
/// later migration reading a type the column no longer has, and by
/// `ADD COLUMN … TEXT NOT NULL` with no type at all.
///
/// **And a rename moves what is known, rather than leaving it behind.** A
/// `RENAME COLUMN` carries no type, so a reader that only records it lets a
/// change renaming a *different* column into the ordering column's name leave
/// the old column's entry standing under that name. Measured
/// end to end, two ordinary renames in one migration:
///
/// ```text
/// ALTER TABLE access_records RENAME COLUMN recorded TO recorded_seq;
/// ALTER TABLE access_records RENAME COLUMN id TO recorded;
/// -> check passed
/// ```
///
/// `recorded` is then the TEXT Crockford identifier, and the real pruner over
/// seven rows minted in order removes the oldest **and the fourth- and
/// fifth-newest**, reporting an ordinary `Pruned`.
///
/// `at` orders the two lists against each other, so a table dropped and
/// re-created after an alteration is read as re-created and not as altered.
fn what_the_migrations_leave(
    all: &[Created],
    changed: &[ColumnChange],
    table: &str,
) -> std::collections::BTreeMap<String, (Option<String>, bool)> {
    let mut state = std::collections::BTreeMap::new();
    let created = all.iter().rfind(|created| created.name == table);
    let since = created.map_or(0, |created| created.at);
    if let Some(created) = created {
        for column in &created.columns {
            let kind = created
                .types
                .iter()
                .find(|(name, _)| name == column)
                .map(|(_, kind)| kind.clone());
            let forbidden = created.non_null.iter().any(|name| name == column);
            state.insert(column.clone(), (kind, forbidden));
        }
    }
    let mut ordered: Vec<&ColumnChange> = changed
        .iter()
        .filter(|change| change.table == table && change.at >= since)
        .collect();
    ordered.sort_by_key(|change| change.at);
    for change in ordered {
        match change.how {
            How::Dropped => {
                if let Some(gone) = &change.removed {
                    state.remove(gone);
                }
            }
            How::Renamed => {
                if let (Some(from), Some(to)) = (&change.removed, &change.added) {
                    // Whatever was known about the old name is now known about
                    // the new one — and nothing is known about the old one.
                    let moved = state.remove(from).unwrap_or((None, false));
                    state.insert(to.clone(), moved);
                }
            }
            How::Added => {
                if let Some(name) = &change.added {
                    state.insert(
                        name.clone(),
                        (change.kind.clone(), change.non_null.unwrap_or(false)),
                    );
                }
            }
            How::Altered => {
                if let Some(name) = &change.added {
                    let entry = state.entry(name.clone()).or_insert((None, false));
                    if change.kind.is_some() {
                        entry.0.clone_from(&change.kind);
                    }
                    if let Some(forbidden) = change.non_null {
                        entry.1 = forbidden;
                    }
                }
            }
        }
    }
    state
}

/// What the migrations, all of them, say `column` of `table` is.
///
/// `None` means they do not say — `CREATE TABLE … AS SELECT` names its columns
/// in a query, and a rename can move a name onto a column nothing typed — and
/// the caller reports that rather than passing it.
fn declared_type_of(
    all: &[Created],
    changed: &[ColumnChange],
    table: &str,
    column: &str,
) -> Option<String> {
    what_the_migrations_leave(all, changed, table)
        .get(column)
        .and_then(|(kind, _)| kind.clone())
}

/// The same reading, with a `CREATE DOMAIN` followed to what it is built on.
///
/// **A domain is how this column would idiomatically be declared.**
/// `CREATE DOMAIN epoch_millis AS BIGINT` and a column of that type sorts
/// exactly as a `bigint` does, and a type rule knowing only names refuses it — with no way to
/// permit it from `catalog/limits.toml`, in a repository whose principle is that
/// limits are data. The remedy is not a second escape hatch: the migrations
/// declare the domain, and the migrations are what this gate reads.
///
/// One level, deliberately. A domain over a domain is legal PostgreSQL and is
/// not in this tree; refusing it with a diagnostic that names the type is a
/// better answer than a resolver nothing exercises.
fn resolved_type_of(
    all: &[Created],
    changed: &[ColumnChange],
    domains: &[(String, String)],
    table: &str,
    column: &str,
) -> Option<String> {
    let declared = declared_type_of(all, changed, table, column)?;
    Some(
        domains
            .iter()
            .rfind(|(name, _)| *name == declared)
            .map_or_else(|| declared.clone(), |(_, over)| over.clone()),
    )
}

/// Whether `column` of `table` is left so that it cannot hold NULL.
fn nulls_are_impossible(
    all: &[Created],
    changed: &[ColumnChange],
    table: &str,
    column: &str,
) -> bool {
    what_the_migrations_leave(all, changed, table)
        .get(column)
        .is_some_and(|(_, forbidden)| *forbidden)
}

/// Each column's name and the first word of its declared type.
///
/// **The whole phrase, not its first word.** A first-word rule reads
/// `TIMESTAMP WITHOUT TIME ZONE` and `TIMESTAMP WITH TIME ZONE` as one type
/// called `timestamp`, so the spelling that carries no zone — and therefore no
/// answer to *whose clock wrote this* — is indistinguishable from the one that
/// does. A parenthesised precision is not part of the phrase (`numeric(20,0)` is
/// `numeric`), and a schema qualification is dropped rather than mistaken for
/// the type: kept, `pg_catalog.int8` reads as `pg_catalog`, which names a type
/// nobody wrote and sends the reader nowhere.
///
/// A column whose type this cannot read is left out rather than guessed at, and
/// the caller treats "cannot say" as a finding of its own.
fn declared_types(statement: &Statement) -> Vec<(String, String)> {
    let tokens = &statement.tokens;
    let Some(open) = tokens.iter().position(|token| *token == Token::Punct('(')) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut depth = 0_usize;
    let mut expect_a_name = true;
    let mut name: Option<String> = None;
    let mut phrase: Vec<String> = Vec::new();
    let close = |name: &mut Option<String>, phrase: &mut Vec<String>, found: &mut Vec<_>| {
        if let Some(column) = name.take()
            && !phrase.is_empty()
        {
            found.push((column, phrase.join(" ")));
        }
        phrase.clear();
    };
    for token in &tokens[open..] {
        match token {
            Token::Punct('(') => depth += 1,
            Token::Punct(')') => {
                depth -= 1;
                if depth == 0 {
                    close(&mut name, &mut phrase, &mut found);
                    break;
                }
            }
            // A qualified type: everything read so far was the schema.
            Token::Punct('.') if depth == 1 && name.is_some() => phrase.clear(),
            Token::Punct(',') if depth == 1 => {
                close(&mut name, &mut phrase, &mut found);
                expect_a_name = true;
            }
            _ if depth == 1 => {
                let Some(word) = token.name() else { continue };
                let lowered = word.to_lowercase();
                if expect_a_name {
                    expect_a_name = false;
                    // A constraint clause is not a column.
                    if matches!(token, Token::Word(_))
                        && matches!(
                            lowered.as_str(),
                            "primary"
                                | "foreign"
                                | "unique"
                                | "check"
                                | "constraint"
                                | "exclude"
                                | "like"
                        )
                    {
                        continue;
                    }
                    // The name as PostgreSQL resolves it: a quoted `"RECORDED"`
                    // is not `recorded`, and reading them as one column lets a
                    // second column stand in for the ordering column's type.
                    name = token.resolved();
                } else if name.is_some() && !ENDS_A_TYPE.contains(&lowered.as_str()) {
                    phrase.push(lowered);
                } else if name.is_some() {
                    // The type is over; what follows is this column's
                    // constraints, and they run to the next top-level comma.
                    close(&mut name, &mut phrase, &mut found);
                }
            }
            _ => {}
        }
    }
    found
}

/// The words that end a column's type and begin its constraints.
///
/// `ARRAY` is deliberately absent: `bigint ARRAY` is one type, and one that does
/// not sort in creation order, so it has to reach the allow-list as itself.
const ENDS_A_TYPE: [&str; 15] = [
    // `AS` never appears inside a type and always ends one: it separates a
    // domain's name from what it is built on, and a table's name from the query
    // that fills it.
    "as",
    "not",
    "null",
    "default",
    "primary",
    "unique",
    "check",
    "references",
    "constraint",
    "generated",
    "collate",
    "deferrable",
    "storage",
    "compression",
    "identity",
];

/// Which of the four shapes a word names, if it names one.
///
/// The four are `NOT NULL`, `PRIMARY KEY`, `GENERATED … AS IDENTITY` and a
/// serial type, in the order the reader keeps its flags. Its own function so
/// the walk below stays inside clippy's line bound and so the shapes are named
/// once rather than spelled inside a loop.
fn the_shape_a_word_names(lowered: &str, previous: Option<&str>) -> Option<usize> {
    if lowered == "null" && previous == Some("not") {
        return Some(0);
    }
    if lowered == "key" && previous == Some("primary") {
        return Some(1);
    }
    if lowered == "identity" {
        return Some(2);
    }
    if CANNOT_HOLD_A_NULL.contains(&lowered) {
        return Some(3);
    }
    None
}

/// The columns of a `CREATE TABLE` that cannot hold NULL.
///
/// **What "oldest" means when the ordering column may be null.** The pruner
/// writes `ORDER BY <column> ASC`, and PostgreSQL sorts NULLs *last* under
/// `ASC` — so a nullable `order_by` makes every row missing that value the
/// newest rows in the table, and the last ever to be removed. Measured:
///
/// ```text
/// INSERT INTO nulls SELECT g, CASE WHEN g <= 5 THEN g ELSE NULL END
///   FROM generate_series(1,10) g;
/// SELECT id FROM nulls ORDER BY at ASC LIMIT 4;   ->  1, 2, 3, 4
/// ```
///
/// Today's two tables both order by a primary key and are safe. The gate is what
/// has to stop the third one, and `order_by_names_a_column` asks only that the
/// column exists.
///
/// A column is non-null when its own definition says `NOT NULL` or
/// `PRIMARY KEY`, or when a table-level `PRIMARY KEY (…)` names it. Read from
/// the same token walk as [`declared_columns`], so the two agree about where a
/// definition starts and ends.
fn columns_that_cannot_be_null(statement: &Statement) -> Vec<String> {
    let tokens = &statement.tokens;
    let Some(open) = tokens.iter().position(|token| *token == Token::Punct('(')) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut depth = 0_usize;
    let mut name: Option<String> = None;
    let mut expect_a_name = true;
    // The definition's own words, so `NOT NULL` is read from the column it
    // belongs to rather than from anywhere in the statement.
    // A table-level `PRIMARY KEY (a, b)` names columns declared above it.
    let mut in_a_table_key = false;
    let mut keyed: Vec<String> = Vec::new();
    let mut previous_word: Option<String> = None;

    // **The same shapes the `ADD COLUMN` reader knows, all four.** With the
    // two readers knowing different shapes, the identical ordering column is
    // accepted when added and refused as *"which may be NULL"* when created,
    // measured.
    // The four flags in one place, so the walk closes a column in one line and
    // stays inside clippy's bound: `NOT NULL`, `PRIMARY KEY`, an identity
    // column and a serial type, in the order `the_shape_a_word_names` returns.
    let mut said = [false; 4];
    let mut close = |name: &mut Option<String>, said: &mut [bool; 4]| {
        if let Some(column) = name.take()
            && said.iter().any(|it| *it)
        {
            found.push(column);
        }
        *said = [false; 4];
    };

    for token in &tokens[open..] {
        match token {
            Token::Punct('(') => {
                depth += 1;
                if depth > 1 {
                    expect_a_name = false;
                }
            }
            Token::Punct(')') => {
                depth -= 1;
                if depth == 0 {
                    close(&mut name, &mut said);
                    break;
                }
                if depth == 1 {
                    in_a_table_key = false;
                }
            }
            Token::Punct(',') if depth == 1 => {
                close(&mut name, &mut said);
                expect_a_name = true;
            }
            _ if depth == 1 && expect_a_name => {
                expect_a_name = false;
                let Some(word) = token.name() else { continue };
                let lowered = word.to_lowercase();
                if matches!(token, Token::Word(_)) && lowered == "primary" {
                    in_a_table_key = true;
                    continue;
                }
                if matches!(token, Token::Word(_))
                    && matches!(
                        lowered.as_str(),
                        "foreign" | "unique" | "check" | "constraint" | "exclude" | "like"
                    )
                {
                    continue;
                }
                name = token.resolved();
            }
            _ => {
                let Some(word) = token.name() else {
                    previous_word = None;
                    continue;
                };
                let lowered = word.to_lowercase();
                // Inside a table-level key's parentheses, every name is a
                // column that key covers.
                if in_a_table_key && depth == 2 {
                    if let Some(covered) = token.resolved() {
                        keyed.push(covered);
                    }
                    continue;
                }
                if matches!(token, Token::Word(_)) {
                    if let Some(which) = the_shape_a_word_names(&lowered, previous_word.as_deref())
                    {
                        said[which] = true;
                    }
                    if lowered == "primary" && depth == 2 {
                        in_a_table_key = true;
                    }
                }
                previous_word = Some(lowered);
                continue;
            }
        }
        previous_word = None;
    }
    found.extend(keyed);
    found.sort();
    found.dedup();
    found
}

/// Every column a `CREATE TABLE … ( … )` declares, lowercased.
///
/// A column is a name at the top level of the parenthesised list — the first
/// token of the list, and the first after each top-level comma. A constraint
/// clause (`PRIMARY KEY (a, b)`, `CHECK (…)`, `FOREIGN KEY …`, `UNIQUE …`,
/// `CONSTRAINT …`, `EXCLUDE …`, `LIKE …`) is not a column and is skipped by its
/// leading keyword, which is the only thing that distinguishes the two at this
/// depth.
///
/// Returns nothing for a statement with no column list: `CREATE TABLE … AS
/// SELECT` names its columns in the query, and "cannot say" has to be
/// distinguishable from "none".
fn declared_columns(statement: &Statement) -> Vec<String> {
    /// A constraint clause opens with one of these, and a keyword is never a
    /// column name at this depth — which is the only thing distinguishing
    /// `PRIMARY KEY (a, b)` from a column called `primary`.
    /// **`period` is not here**: it needs no quoting in PostgreSQL and is an
    /// ordinary column name, and listing it would drop a real column from the list
    /// — which then reports a valid `order_by` as naming no column. The
    /// `PERIOD FOR` clause it would be for is SQL:2011 temporal syntax that
    /// PostgreSQL 18 does not have.
    const CLAUSES: [&str; 6] = [
        "primary",
        "foreign",
        "unique",
        "check",
        "constraint",
        "exclude",
    ];

    let tokens = &statement.tokens;
    let Some(open) = tokens.iter().position(|token| *token == Token::Punct('(')) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut depth = 0_usize;
    let mut expect_a_name = true;
    for token in &tokens[open..] {
        match token {
            Token::Punct('(') => {
                depth += 1;
                if depth > 1 {
                    expect_a_name = false;
                }
            }
            Token::Punct(')') => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Token::Punct(',') if depth == 1 => expect_a_name = true,
            _ if depth == 1 && expect_a_name => {
                expect_a_name = false;
                let Some(name) = token.name() else { continue };
                let lowered = name.to_lowercase();
                // **A quoted identifier is never a keyword**, which is this
                // module's own rule and applies here too: `"check"`,
                // `"like"` and `"primary"` are ordinary columns, and a reader
                // matching words would drop every one. `token.is` already knows the
                // difference — a `Token::Name` is quoted and a `Token::Word` is
                // not — so a clause is only a clause when it is unquoted.
                // **`LIKE` means the list is not the whole list** — but only
                // where a column name would go. `CREATE TABLE t (LIKE shape
                // INCLUDING ALL, extra TEXT)` copies every column of `shape`,
                // and reading only `extra` reports a real `order_by` as naming
                // no column; this scanner cannot know what the template holds,
                // so it says so, and "cannot say" is an empty list.
                //
                // **Not the whole statement scanned for the word**, which turns
                // the check off for any table with an ordinary `CHECK
                // (col LIKE '…')` constraint in it. Measured with such a scan:
                // one such line added to `migrations/0002_access_records.sql`
                // leaves
                // `order_by = "recorded_at"` — a column the table does not have
                // — passing with `check passed`, and PostgreSQL 18.6 answering
                // `ERROR: column "recorded_at" does not exist` for the pruner's
                // own statement. A `LIKE` in a constraint is at depth two; a
                // table-shape clone is a top-level item, which is exactly where
                // this looks.
                if matches!(token, Token::Word(_)) && lowered == "like" {
                    return Vec::new();
                }
                let is_a_clause =
                    matches!(token, Token::Word(_)) && CLAUSES.contains(&lowered.as_str());
                if !is_a_clause && let Some(column) = token.resolved() {
                    found.push(column);
                }
            }
            _ => {}
        }
    }
    found
}

/// The table a `CREATE … TABLE …` beginning at `tokens[0]` declares.
///
/// **`UNLOGGED` is a real table and `TEMP` is not.** A temporary table "cannot
/// grow without bound", which is
/// true of `TEMP` and false of `UNLOGGED`: an unlogged table is permanent,
/// survives every session, and is emptied only by an unclean shutdown, so
/// excluding the two together lets it walk straight past this gate.
fn table_declared_at(tokens: &[Token], whole: &[Token]) -> Option<String> {
    /// Whether a name is one PostgreSQL could have been given.
    ///
    /// `EXECUTE format('CREATE TABLE %I PARTITION OF access_records …', part)` is
    /// the standard dynamic-partition helper, and the literal is re-scanned as
    /// SQL — so unguarded, the gate fails the build naming `%I`, a placeholder rather than
    /// a table. A name that could not be an identifier is not a table this gate
    /// can say anything about.
    fn could_be_a_name(name: &str) -> bool {
        !name.is_empty()
            && name.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
            })
    }

    let mut rest = tokens.iter();
    if !rest.next()?.is("CREATE") {
        return None;
    }
    let mut word = rest.next()?;
    // `GLOBAL` and `LOCAL` only ever qualify `TEMPORARY`.
    if word.is("GLOBAL") || word.is("LOCAL") {
        word = rest.next()?;
    }
    // A temporary table lives and dies with its session, so it genuinely cannot
    // grow without bound — the one modifier that excuses a table here.
    if word.is("TEMP") || word.is("TEMPORARY") {
        return None;
    }
    if word.is("UNLOGGED") {
        word = rest.next()?;
    }
    // **A foreign table is a table this database queries and does not store.**
    // Its bound is the remote server's, not a `[tables.*]` row the pruner can
    // `DELETE` from — and unread, it is invisible to both directions of the
    // gate, so a migration can add one and the count nobody reads stays the same. It is
    // declared with the marker and the reason, like any other table that cannot
    // be pruned here.
    if word.is("FOREIGN") {
        word = rest.next()?;
    }
    // **A materialised view stores rows and grows with its source.** It is not
    // a `[tables.*]` row the pruner can `DELETE` from — its bound is a refresh
    // policy — and unread, it is invisible to both directions of a gate whose count is
    // the only thing anybody reads.
    if word.is("MATERIALIZED") {
        if !rest.next()?.is("VIEW") {
            return None;
        }
    } else if !word.is("TABLE") {
        return None;
    }

    let mut name = rest.next();
    if name.is_some_and(|token| token.is("IF")) {
        let past = the_existence_test_after_if(rest.as_slice());
        if past > 0 {
            for _ in 0..past {
                rest.next()?;
            }
            name = rest.next();
        }
    }
    // Only an *unquoted* name has to look like an identifier: a quoted one may
    // hold a space, a semicolon or a reserved word, which is the whole reason
    // quoting exists — and `%I` arrives unquoted.
    let quoted = matches!(name, Some(Token::Name(_)));
    qualified(whole, name).filter(|found| quoted || could_be_a_name(found))
}

/// Where the name begins in `ALTER TABLE [ IF EXISTS ] [ ONLY ] name`.
///
/// In that order — verified against PostgreSQL 18.6, where `ONLY IF EXISTS` is a
/// syntax error and `CREATE TABLE only` is one too, so `ONLY` is never the name
/// and `IF` may be.
fn past_the_alter_prefix(tokens: &[Token], from: usize) -> usize {
    let mut at = from;
    if tokens.get(at).is_some_and(|token| token.is("IF")) {
        let past = the_existence_test_after_if(tokens.get(at + 1..).unwrap_or_default());
        if past > 0 {
            at += 1 + past;
        }
    }
    while tokens.get(at).is_some_and(|token| token.is("ONLY")) {
        at += 1;
    }
    at
}

/// How many tokens after an `IF` belong to an existence test, if they do.
///
/// Two for `NOT EXISTS`, one for `EXISTS`, and **zero when the `IF` is a name**
/// — which it may be, because `IF` is a non-reserved keyword, and stepping over
/// a fixed number of tokens after an `IF` without asking what they are misreads
/// these. Measured on PostgreSQL 18.6, every one of them is accepted:
///
/// ```text
/// CREATE TABLE if (id INT);          DROP TABLE if;
/// ALTER TABLE if RENAME TO landed;   ALTER TABLE if SET SCHEMA archive;
/// CREATE SCHEMA if;  CREATE TABLE if.t (id INT);
/// ```
///
/// Such a scanner reads the first as declaring a table called `int` — a table
/// nobody wrote, which this file's own header calls the one thing this gate must not
/// produce — and drops the rename and the drop whole, so
/// `catalog/limits.toml` goes on bounding a table that is gone.
///
/// Written once for the five places that ask: `CREATE TABLE`'s
/// `IF NOT EXISTS`, `ALTER TABLE`'s and `DROP TABLE`'s `IF EXISTS`, the
/// `ALTER TABLE` prefix `column_changes` steps over, and `ADD COLUMN`'s own.
/// *A grammar understood in five places is a grammar that will one day be
/// understood five ways.*
fn the_existence_test_after_if(after: &[Token]) -> usize {
    match after {
        [not, exists, ..] if not.is("NOT") && exists.is("EXISTS") => 2,
        [exists, ..] if exists.is("EXISTS") => 1,
        _ => 0,
    }
}

/// The table a name token refers to, following the dots that qualify it.
fn qualified(whole: &[Token], name: Option<&Token>) -> Option<String> {
    let (parts, _) = the_qualified_name(whole, name)?;
    catalog_name(&parts)
}

/// One qualified name: the parts it resolves to, and the index one past its end.
///
/// **One walker, because two readers need this grammar.** [`renamed_table`] has
/// to look for its verb *after* the name — `SET` and `RENAME` are non-reserved
/// keywords, so `ALTER TABLE set.t RENAME TO landed` is legal PostgreSQL whose
/// third token spells a verb it is not — so where the name ends is returned by
/// the walk that already found it, rather than walked a second time. *A grammar
/// understood in two places is a grammar that will one day be understood two
/// ways.*
fn the_qualified_name(whole: &[Token], name: Option<&Token>) -> Option<(Vec<String>, usize)> {
    let name = name?;
    // **`U&"…"` is one name**, and `U&` is a bare word before it. Taken as the
    // name, the gate reports `` `U&` has no entry `` for a table PostgreSQL
    // creates as `login_attempts` — criterion 5's second half failing in the one
    // way it must not.
    if name.name() == Some("U&")
        && let Some(at) = whole.iter().position(|token| core::ptr::eq(token, name))
        && let Some(quoted @ Token::Name(_)) = whole.get(at + 1)
    {
        return the_qualified_name(whole, Some(quoted));
    }
    let at = whole.iter().position(|token| core::ptr::eq(token, name))?;
    // **Spelled as PostgreSQL will resolve it, which `resolved` already knows.**
    // Read through `name()`, an unquoted `CREATE TABLE Probe_Upper_A` would be
    // reported as `Probe_Upper_A` — a table the database never created, since it
    // folds an unquoted identifier — and the fix line would tell a developer to
    // write `[tables.Probe_Upper_A]`, an entry the pruner can never match. The
    // rule is written once, in `resolved`, which the column readers already
    // call, and this has to be the same copy of it.
    let mut parts = vec![name.resolved()?];
    let mut index = at + 1;
    while whole.get(index) == Some(&Token::Punct('.')) {
        parts.push(whole.get(index + 1)?.resolved()?);
        index += 2;
    }
    Some((parts, index))
}

/// A table reference as `catalog/limits.toml` would have to spell it.
///
/// **A schema other than `public` keeps it**: dropped,
/// `archive.access_records` is credited to `[tables.access_records]`, whose
/// pruner runs `DELETE FROM access_records` — a different table.
///
/// **And the parts are tokens, not a split on `.`.** Splitting the text reads a
/// quoted identifier holding a dot — `"public.access_records"`, one name — as
/// two parts, crediting it to the entry for a table it is not, and
/// a three-part `database.schema.table`, which PostgreSQL accepts for the
/// current database, yields nothing at all.
fn catalog_name(parts: &[String]) -> Option<String> {
    let (schema, table) = match parts {
        [table] => (None, table),
        [schema, table] => (Some(schema), table),
        // `database.schema.table`. The database is the one this connects to, so
        // what identifies the table is the last two parts.
        [_database, schema, table] => (Some(schema), table),
        _ => return None,
    };
    if table.is_empty() {
        return None;
    }
    Some(match schema {
        Some(schema) if schema != "public" => format!("{schema}.{table}"),
        _ => table.clone(),
    })
}

/// Where a statement's verb sits, when it is the statement's own verb.
///
/// **A plpgsql block keeps its opening word.** `$$ BEGIN ALTER TABLE …; END $$`
/// is cut on `;`, so the statement a reader is handed begins `BEGIN ALTER
/// TABLE` — and a reader that asks for its verb at token zero sees `BEGIN` and
/// gives up. Measured, each as the first statement of a routine body:
///
/// ```text
/// CREATE FUNCTION f() … AS $$ BEGIN ALTER TABLE access_records
///                              RENAME TO login_attempts; END $$
///   -> renamed = []       the entry goes on bounding a table that is gone
/// CREATE FUNCTION f() … AS $$ BEGIN DROP TABLE access_records; END $$
///   -> dropped = []       the same, and the table cannot come back
/// CREATE FUNCTION f() … AS $$ BEGIN CREATE TABLE ghost (id INT); END $$
///   -> created = [ghost]  seen, because `created_tables` scans every offset
/// ```
///
/// So a gate reading the verb at token zero believes a table created in a
/// routine body and neither of the two ways one can leave, which is the
/// asymmetry that makes a census drift upward and never down.
///
/// The offset is not searched for blindly. A statement's verb is either first
/// or follows a word that opens a statement — the same test `statements` uses
/// to tell `RAISE` the keyword from `raise` the function name — so `ALTER` and
/// `DROP` written *inside* a statement, which both are (`ALTER TABLE t DROP
/// COLUMN c`), are not read as one.
fn the_statement_verb_at(tokens: &[Token], verb: &str) -> Option<usize> {
    (0..tokens.len()).find(|&at| {
        tokens[at].is(verb)
            && at
                .checked_sub(1)
                .and_then(|before| tokens.get(before))
                .is_none_or(opens_a_statement)
    })
}

/// Where the name begins after `<verb> <kind>`, for the kinds that hold rows.
///
/// **The three kinds `table_declared_at` counts.** A
/// materialised view stores rows and a foreign table is a table this gate
/// declares; both are *created* into the census, and a reader of tables alone
/// renames or drops neither out of it:
///
/// ```text
/// CREATE MATERIALIZED VIEW recent_access AS …   -> counted
/// ALTER MATERIALIZED VIEW recent_access RENAME TO older  -> nothing
/// DROP MATERIALIZED VIEW recent_access                   -> nothing
/// ```
///
/// A census that can only go up is not a census. The list is here rather than
/// spelled at each caller so the two readers below ask the same
/// question of the same grammar.
fn past_the_object_kind(tokens: &[Token], at: usize) -> Option<usize> {
    let word = tokens.get(at)?;
    if word.is("TABLE") {
        return Some(at + 1);
    }
    let pairs = [("FOREIGN", "TABLE"), ("MATERIALIZED", "VIEW")];
    for (first, second) in pairs {
        if word.is(first) && tokens.get(at + 1).is_some_and(|token| token.is(second)) {
            return Some(at + 2);
        }
    }
    None
}

/// The table a statement renames or moves, if it moves one.
///
/// `RENAME TO` and `SET SCHEMA` both retire a name and create another, and
/// reading only the first leaves `ALTER TABLE access_records SET SCHEMA archive`
/// with the bounded entry naming a table that no longer exists, and the gate green.
fn renamed_table(statement: &Statement) -> Option<(String, String)> {
    let tokens = &statement.tokens;
    let verb_at = the_statement_verb_at(tokens, "ALTER")?;
    let mut rest = tokens[past_the_object_kind(tokens, verb_at + 1)?..].iter();
    let mut from = rest.next();
    if from.is_some_and(|token| token.is("IF")) {
        let past = the_existence_test_after_if(rest.as_slice());
        if past > 0 {
            for _ in 0..past {
                rest.next()?;
            }
            from = rest.next();
        }
    }
    // **`ONLY` is not a table name.** `ALTER TABLE ONLY t RENAME TO u` and
    // `ALTER TABLE ONLY t SET SCHEMA s` are both ordinary PostgreSQL — it is
    // what `pg_dump` writes — and a reader taking the next word reads `ONLY`
    // as the name. Two failures, measured:
    //
    // ```text
    // ALTER TABLE ONLY access_records RENAME TO login_attempts
    //   -> from = "ONLY", so the retired name is never retired and
    //      catalog/limits.toml goes on bounding a table that is gone
    //
    // ALTER TABLE ONLY access_records SET SCHEMA archive
    //   -> "`archive.ONLY` has no entry in catalog/limits.toml"
    // ```
    //
    // The second is a finding naming a table nobody wrote, which this file's
    // own header calls the one thing this gate must not produce — while the
    // real new table, `archive.access_records`, is unbounded and unnamed.
    if from.is_some_and(|token| token.is("ONLY")) {
        from = rest.next();
    }
    // **The verb is the word after the name, not the first one in the
    // statement.** `SET` and `RENAME` are both non-reserved keywords, so
    // `CREATE SCHEMA set` and `CREATE TABLE rename` are legal, and a verb looked
    // for from token zero is the wrong one:
    //
    // ```text
    // ALTER TABLE set.t RENAME TO landed
    //   -> the verb read was `set`; `SCHEMA` is not the token after it, so the
    //      rename was dropped whole. catalog/limits.toml goes on bounding
    //      `set.t` while `landed` holds the rows, unbounded and unnamed.
    // ```
    //
    // The name's end comes from the walk that reads it.
    let (parts, after) = the_qualified_name(tokens, from)?;
    let from = catalog_name(&parts)?;
    // **And it is the token *at* the end of the name, not the first one like it
    // anywhere below.** Searching forward reads a column named `set` of a type
    // named `schema` as the clause `SET SCHEMA`:
    //
    // ```text
    // ALTER TABLE login_attempts ADD COLUMN set schema NOT NULL
    //   -> renamed = [("login_attempts", "not.login_attempts")]
    // ```
    //
    // Measured on PostgreSQL 18.6, that statement adds a column and moves
    // nothing — so a forward search retires a table that still exists *and* invents
    // `not.login_attempts`, a table nobody wrote, which this file's own header
    // calls the one thing it must not produce. `ALTER TABLE <name>` is followed
    // by its action and by nothing else, so there is nothing to search for.
    let verb = after;
    if tokens
        .get(verb)
        .is_none_or(|token| !token.is("RENAME") && !token.is("SET"))
    {
        return None;
    }
    if tokens[verb].is("RENAME") {
        // `RENAME COLUMN a TO b` is not a table rename.
        if tokens.get(verb + 1).is_some_and(|token| !token.is("TO")) {
            return None;
        }
        let to = qualified(tokens, tokens.get(verb + 2))?;
        return Some((from, to));
    }
    // `SET SCHEMA archive`.
    if tokens.get(verb + 1).is_some_and(|token| token.is("SCHEMA")) {
        // **`resolved`, not `name`**: an unquoted schema folds, so
        // `SET SCHEMA Archive` would credit the rows to `Archive.t`, an entry
        // the pruner can never match. And the table that moves is the last part
        // the walk read, not the tail of a split on the old name's text: split,
        // a table PostgreSQL calls `a.b` moves to `archive.b`, a table nobody wrote.
        let schema = tokens.get(verb + 2)?.resolved()?;
        let bare = parts.last()?.clone();
        let to = catalog_name(&[schema, bare])?;
        return Some((from, to));
    }
    None
}

/// Every table a statement drops.
///
/// **All of them.** `DROP TABLE journal_entries, access_records;` is one valid
/// statement retiring both, and reading only the first reports
/// one bounded table with nothing creating it and stays silent about the other,
/// whose `[tables.*]` entry then names a table that does not exist.
fn dropped_tables(statement: &Statement) -> Vec<String> {
    let tokens = &statement.tokens;
    let Some(opens) = the_statement_verb_at(tokens, "DROP")
        .and_then(|verb_at| past_the_object_kind(tokens, verb_at + 1))
    else {
        return Vec::new();
    };
    let mut rest = tokens[opens..].iter();
    let mut names = Vec::new();
    let mut next = rest.next();
    if next.is_some_and(|token| token.is("IF")) {
        let past = the_existence_test_after_if(rest.as_slice());
        if past > 0 {
            for _ in 0..past {
                rest.next();
            }
            next = rest.next();
        }
    }
    while let Some(token) = next {
        // `CASCADE` and `RESTRICT` end the list.
        if token.is("CASCADE") || token.is("RESTRICT") {
            break;
        }
        if let Some(name) = qualified(tokens, Some(token)) {
            names.push(name);
        }
        // Step past this name's qualifying dots, then over the comma.
        let mut seen = rest.next();
        while seen == Some(&Token::Punct('.')) {
            rest.next();
            seen = rest.next();
        }
        next = if seen == Some(&Token::Punct(',')) {
            rest.next()
        } else {
            None
        };
    }
    names
}

#[cfg(test)]
mod tests {
    use super::{
        BOUNDED, Token, changes_in, every_migration_is_numbered_once, every_table_declares_a_bound,
        migrations, statements, tables_created_in, unreadable_statements_in,
    };
    use std::path::Path;

    /// A configuration name this gate cannot read is refused, and says so.
    ///
    /// **The same class as `'search' || '_path'`, one call over.** A filter
    /// asking for the literal `search_path` and skipping everything else never
    /// examines a name PostgreSQL is perfectly happy to compute:
    /// `concat('search', '_path')`, `lower('SEARCH_PATH')` and
    /// `format('%s_path', 'search')` each put `archive` on the path with
    /// `check passed`, measured.
    ///
    /// It is its own finding rather than a redirect, because failing it into the
    /// redirect branch tells an author to qualify table names in a statement that
    /// creates none — an ordinary PL/pgSQL `set_config(knob, 'off', false)`
    /// naming `row_security` through a variable would get a message about
    /// `search_path`, and `§15.1` asks a `fix` to be about what is wrong.
    #[test]
    fn a_configuration_name_this_gate_cannot_read_is_refused() {
        // **A signature is not a call**, and locking `set_config` away from
        // `PUBLIC` is the most on-point hardening there is for §15.8.
        for privilege in [
            "REVOKE ALL ON FUNCTION set_config(text, text, boolean) FROM PUBLIC;",
            "REVOKE ALL ON FUNCTION pg_catalog.set_config(text, text, boolean) FROM PUBLIC;",
        ] {
            let (found, _) = unreadable_statements_in(privilege, super::Dialect::PostgreSql);
            assert!(
                found
                    .iter()
                    .all(|(_, what, _)| !what.contains("cannot read")),
                "`{privilege}` sets no configuration and was refused: {found:?}"
            );
        }
        for computed in [
            "SELECT set_config(concat('search', '_path'), 'archive', false);",
            "SELECT set_config(lower('SEARCH_PATH'), 'archive', false);",
            "SELECT set_config(format('%s_path', 'search'), 'archive', false);",
        ] {
            let (found, _) = unreadable_statements_in(computed, super::Dialect::PostgreSql);
            assert!(
                found
                    .iter()
                    .any(|(_, what, _)| what.contains("cannot read")),
                "`{computed}` builds its name and was accepted: {found:?}"
            );
        }
        // And what it says is about the name. This statement sets `row_security`
        // and creates no table, so a message about qualifying table names sends
        // the reader where the defect is not.
        let plpgsql = concat!(
            "DO $guc$\nDECLARE\n    knob TEXT := 'row_security';\n",
            "BEGIN\n    PERFORM set_config(knob, 'off', false);\nEND\n$guc$;"
        );
        let (found, _) = unreadable_statements_in(plpgsql, super::Dialect::PostgreSql);
        assert!(
            found
                .iter()
                .all(|(_, what, _)| !what.contains("redirects `search_path`")),
            "a statement that names no path is not a path redirect: {found:?}"
        );
    }

    /// The word `as` inside a name does not open a routine's body.
    ///
    /// **The two word boundaries survive their own deletion, and this test does
    /// not change that** — it is written down here rather than left for the next
    /// reader to re-measure. What
    /// actually rejects `alias 'x'` is the clause three lines below them: a body
    /// has to open with `$` or `'`, and a spurious `as` is almost never followed
    /// by one. Measured on PostgreSQL 18.6, `AS'SELECT 2'` with no space *is*
    /// valid and `AS$$…$$` is not — so the one shape that could reach the
    /// boundaries is a quoted body, which is a single `Literal` and is re-cut
    /// and judged on its own. The boundaries are kept
    /// because they answer a different question — *is this the keyword* — and a
    /// gate that leans on the quote test alone would call `probe_alias '…'` a
    /// body opener the day someone writes it.
    ///
    /// What this pins is the outcome: in both shapes the body opens at the
    /// quote after the real `AS`, so a routine whose *name* holds those two
    /// letters keeps its configuration clause outside its body.
    #[test]
    fn the_word_as_inside_a_name_does_not_open_a_body() {
        // `as` after a space and before an underscore: only the *back* boundary.
        let after_a_space = "CREATE FUNCTION as_needed() RETURNS int LANGUAGE sql AS 'SELECT 1';";
        // `as` after a letter and before a space: only the boundary in *front*.
        let after_a_letter = "CREATE FUNCTION probe_alias () RETURNS int LANGUAGE sql \
                              AS 'SELECT 1';";
        for statement in [after_a_space, after_a_letter] {
            let opens = super::the_body_opener_in(statement);
            assert!(
                statement[opens..].starts_with('\''),
                "the body opens at the quote after the real `AS`, and this opened at {:?}",
                &statement[opens..(opens + 24).min(statement.len())]
            );
        }
    }

    /// The routine hatch excuses the clause, and not the body.
    ///
    /// **The narrowing, decided here.** Replaced by `true` — the shape whose own
    /// paragraph records such a redirect running and a table landing in the wrong
    /// schema — fails this test, and so does removing the keyword-body
    /// openers. Only the *false-refusal* direction is held elsewhere, by three
    /// fixtures that turn red if the bracketing goes.
    ///
    /// So this asks what the rule does rather than what a redirect does: a
    /// `search_path` the body holds is not the routine's own clause, and one
    /// written among the attributes is. Both fixtures parse on PostgreSQL 18.6.
    #[test]
    fn the_routine_hatch_reads_the_clause_and_not_the_body() {
        let in_the_body = "CREATE FUNCTION f() RETURNS int LANGUAGE sql\n\
                           BEGIN ATOMIC SELECT 1 AS search_path; END;";
        let lines: Vec<&str> = in_the_body.lines().collect();
        // **The first statement, because a keyword body holds a `;`.** The
        // cutter ends a statement at one, so the definition and the `END;` after
        // it are two — and the definition's own source still spans both lines,
        // which is what the rule reads.
        let statement = super::statements(in_the_body)
            .into_iter()
            .next()
            .expect("the fixture opens with the definition");
        assert!(
            !super::the_routines_own_clause(
                &statement.tokens,
                &super::the_source_of(&lines, &statement)
            ),
            "a `search_path` the body holds is not the routine's configuration clause"
        );

        // **And the other keyword body.** `BEGIN ATOMIC` and `RETURN` are one
        // list, and this is the second half's fixture: the `RETURN` entry
        // deleted fails it. PostgreSQL 14 and later parse this form.
        let returned = "CREATE FUNCTION f() RETURNS int LANGUAGE sql\n\
                        RETURN (SELECT search_path FROM paths LIMIT 1);";
        let lines: Vec<&str> = returned.lines().collect();
        let statement = super::statements(returned)
            .into_iter()
            .next()
            .expect("the fixture opens with the definition");
        assert!(
            !super::the_routines_own_clause(
                &statement.tokens,
                &super::the_source_of(&lines, &statement)
            ),
            "a `search_path` a `RETURN` body holds is not the routine's configuration clause"
        );

        let in_the_clause = "CREATE FUNCTION f() RETURNS int LANGUAGE sql\n\
                             SECURITY DEFINER SET search_path = pg_catalog, pg_temp\n\
                             BEGIN ATOMIC SELECT 1; END;";
        let lines: Vec<&str> = in_the_clause.lines().collect();
        let statement = super::statements(in_the_clause)
            .into_iter()
            .next()
            .expect("the fixture opens with the definition");
        assert!(
            super::the_routines_own_clause(
                &statement.tokens,
                &super::the_source_of(&lines, &statement)
            ),
            "the canonical `SECURITY DEFINER` hardening is the routine's own clause, and \
             refusing it is refusing the one thing this gate most wants written"
        );
    }

    /// No rewrite of a token weighs the same as the token.
    ///
    /// **The measure the whole two-reader identity rests on, pinned.** Replaced
    /// by a constant `1` it is a token count, which this file records as
    /// defeated; replaced by a constant `0` the identity reads `0 == 0`. Both
    /// fail this test, and a comparison of the readers cannot see either,
    /// because all three call this one function and a weakening moves them
    /// together —
    /// which is the *expectation that shrinks with the thing it measures*, in
    /// the one place this file argues hardest that it must not.
    ///
    /// What the weight has to see is what the rules see. `Token::is` answers
    /// `false` for a `Name` and `true` for the same text as a `Word`, so those
    /// two must not weigh alike; `Punct`, an empty `Literal` and an empty `Name`
    /// all hold nothing, and weighed alike, a rewrite to any of them would be free.
    #[test]
    fn no_rewrite_of_a_token_weighs_the_same_as_the_token() {
        let word = super::Token::Word("EXECUTE".to_owned());
        let name = super::Token::Name("EXECUTE".to_owned());
        let punct = super::Token::Punct('(');
        let empty = super::Token::Literal(String::new());
        let nameless = super::Token::Name(String::new());

        // **A kind mixed *into* the text is a kind the text can absorb.** Seeded
        // first, `Word("COPY")` and `Name("#mnx")` come to one number, and so do
        // `Word("SET")` and `Name("3cs")`, measured — a collision that is also a meaning
        // change, because `Token::is` answers `false` for a `Name`. These two
        // pairs are here by name so the next reader cannot reintroduce it.
        for (word, name) in [("COPY", "#mnx"), ("SET", "3cs")] {
            assert_ne!(
                super::the_weight_of(&[super::Token::Word(word.to_owned())]),
                super::the_weight_of(&[super::Token::Name(name.to_owned())]),
                "a `Word` and a `Name` of different text must not meet: the kind has to sit \
                 where the mixing cannot reach it"
            );
        }
        // And punctuation is not one token: its character is its whole content.
        assert_ne!(
            super::the_weight_of(&[super::Token::Punct('(')]),
            super::the_weight_of(&[super::Token::Punct(',')]),
            "two different marks are two different tokens"
        );

        // The rewrite that turns every keyword test off and, weighed by text
        // alone, moves no number.
        assert_ne!(
            super::the_weight_of(std::slice::from_ref(&word)),
            super::the_weight_of(std::slice::from_ref(&name)),
            "`Token::is` answers `false` for a `Name` and `true` for the same text as a \
             `Word`, so the two must not weigh alike"
        );
        // **And a mark against a literal holding that mark.** `SELECT '('` cuts
        // to `Literal("(")`, whose one byte mixes to exactly what `Punct('(')`'s
        // character mixes to — so the kind tag is the only thing between them,
        // and every pair above holds a token with *no* text, where the tag is
        // reached by a different route. `Token::Punct(_) => 4`, which gives
        // punctuation a literal's tag, fails this pair.
        let marked = super::Token::Literal("(".to_owned());
        for (one, other) in [
            (&punct, &empty),
            (&punct, &nameless),
            (&empty, &nameless),
            (&punct, &marked),
        ] {
            assert_ne!(
                super::the_weight_of(std::slice::from_ref(one)),
                super::the_weight_of(std::slice::from_ref(other)),
                "a token holding nothing is still of a kind, and the kinds are what the \
                 rules read"
            );
        }

        // **The text, not its length.** Compared with a word one character
        // longer, `COPY` pins the length and nothing else — and a
        // rename to `COPz`, which is the same length, switches three refusals
        // off with the two readers agreeing to the byte.
        assert_ne!(
            super::the_weight_of(&[super::Token::Word("COPY".to_owned())]),
            super::the_weight_of(&[super::Token::Word("COPz".to_owned())]),
            "two words of one length are two different words, and `Token::is` reads which"
        );
        assert_ne!(
            super::the_weight_of(&[super::Token::Word("COPY".to_owned())]),
            super::the_weight_of(&[super::Token::Word("COPYz".to_owned())]),
            "and a longer word is a different word too"
        );
        // **And the order.** A sum is blind to it: reversing a statement's
        // tokens leaves the number exactly where it was, so every rule that reads
        // a token's neighbours can be fed the statement backwards.
        assert_ne!(
            super::the_weight_of(&[word.clone(), super::Token::Word("SET".to_owned())]),
            super::the_weight_of(&[super::Token::Word("SET".to_owned()), word.clone()]),
            "the same tokens in another order are another statement"
        );
        // **A constant cannot tell one statement's tokens from another's.**
        // This is that shape — `1` for every token, then `0` — and it is what
        // must not pass.
        assert_ne!(
            super::the_weight_of(&[word.clone(), punct]),
            super::the_weight_of(std::slice::from_ref(&word)),
            "a statement with a token more weighs more"
        );
    }

    /// A file read to two different lengths is an error, not a finding.
    ///
    /// **A finding can be filtered.** Reported as a finding, three shapes get
    /// past with the two numbers agreeing: a guard clause at the top of
    /// the reader returning before the comparison runs; a filter in the caller on
    /// `files == 1` dropping every verdict from the migrations no fixture
    /// writes; and a `retain` on the verdict's own text silencing the comparison
    /// itself. What the walk returns as an `Err` is none of those — it stops the
    /// step where it stands.
    ///
    /// Driven through the real entry point, over a scratch tree of two files, so
    /// that the file-keyed filter is one of the things it decides.
    #[test]
    fn a_file_read_to_two_lengths_is_an_error() {
        let held = crate::check::LeavesNothingBehind::new("two");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        std::fs::write(
            scratch.join("migrations/0001_probe.sql"),
            "CREATE TABLE one (id INT);\n",
        )
        .expect("writable");
        std::fs::write(
            scratch.join("migrations/0002_probe.sql"),
            "COPY two FROM stdin;\nCREATE TABLE two (id INT);\n",
        )
        .expect("writable");

        let (findings, ..) =
            super::every_table_declares_a_bound(&scratch).expect("a readable scratch");
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("`COPY … FROM stdin`")),
            "the refusal reader runs over every migration, not the first: {findings:?}"
        );
    }

    /// A disagreement between the two readers of one file stops the walk.
    ///
    /// **Nothing can force one through the walk**, which is what a guard against
    /// a defect means: both readers run one tokeniser over one text, so on every
    /// file a fixture can write they agree, and reduced to `if true` the check
    /// leaves every such file judged the same — so it is driven where it is
    /// written rather than through a file that cannot disagree.
    ///
    /// **And carrying its verdict out of the walk is a second decision**, which
    /// this test cannot see: `let _ = the_readers_agree(…)` at the call site
    /// leaves this one green, and is what the forced disagreement in the test
    /// named below exists for. A function tested where it is written is not a
    /// function whose answer is used.
    /// [`a_forced_disagreement_stops_the_walk_rather_than_becoming_a_finding`]
    /// is the other half, through a door that injects the one disagreement a
    /// file cannot.
    #[test]
    fn a_disagreement_between_the_two_readers_stops_the_walk() {
        super::the_readers_agree("migrations/0001_probe.sql", 7, 7)
            .expect("two readers that agree are not an error");
        let said = super::the_readers_agree("migrations/0001_probe.sql", 7, 9)
            .expect_err("two readers that disagree stop the walk");
        assert!(
            said.contains("migrations/0001_probe.sql") && said.contains('7') && said.contains('9'),
            "the error names the file and both numbers, because which of the two moved is \
             the whole of what there is to look at: {said}"
        );
    }

    /// Every spelling of a `search_path` redirect, and the two that are not one.
    ///
    /// **Every spelling, because a rule written for one is not a rule.** Refusing
    /// `SET search_path = archive;` and not `SET search_path TO archive;`
    /// — PostgreSQL's own documented form — lets a migration written the
    /// ordinary way put `access_records`, which holds personal data and a
    /// `GeoIP` column, into another schema while `[tables.access_records]` goes on
    /// pruning `public`. Such a rule fails this test.
    ///
    /// The second half is what makes the first half safe. Refusing every
    /// `set_config` was measured too, and it refuses the six this repository
    /// writes in `0011` and `0012`: the rule reads the setting's *name* out of
    /// the literal, and asks whether the value came from the path the database
    /// already has.
    #[test]
    fn every_spelling_of_a_search_path_redirect_is_refused() {
        for redirect in [
            "SET search_path = archive;",
            "SET search_path TO archive;",
            "SET search_path=archive;",
            "SET SESSION search_path TO archive;",
            "SET LOCAL search_path TO archive;",
            "SET search_path TO 'archive';",
            "SELECT set_config('search_path', 'archive', false);",
            "SELECT set_config('SEARCH_PATH', 'archive', false);",
            // **A name SQL builds is the same name.** A rule reading the first
            // literal after the first `set_config` reads this one as naming
            // `search` and accepts it — measured against the live database, the
            // unqualified `CREATE TABLE access_records` below it lands in
            // `archive` while `[tables.access_records]` goes on bounding and
            // pruning `public`.
            "SELECT set_config('search' || '_path', 'archive', false);",
            "SELECT set_config('search_path'::text, 'archive', false);",
            // **And not only the first call in the statement**, which is how a
            // redirect hides behind a legitimate one.
            "SELECT set_config('nc.x', 'y', false), set_config('search_path', 'archive', false);",
            // **`SET` anywhere, not only as the statement's first word.** Both
            // of these outlive the migration that runs them.
            "ALTER DATABASE noctua SET search_path TO archive;",
            "ALTER ROLE noctua SET search_path TO archive;",
            // **And putting the path back is changing it**, to one this gate
            // has not read.
            "RESET search_path;",
            // **A redirect built out of `current_setting` is still a redirect.**
            // A statement-wide excuse lets any mention of the path the
            // database already has switch the rule off — including in the
            // redirect's own value. This one *prepends* `archive`, which
            // replaces the first schema on the path; appending leaves it where
            // it was, and appending is what `0011` and `0012` write.
            "SELECT set_config('search_path', 'archive, ' || current_setting('search_path'), false);",
            // And beside a save this repository genuinely performs.
            "SELECT set_config('nc.saved', current_setting('search_path'), false), \
             set_config('search_path', 'archive', false);",
            // And with a no-op appended to the select list, which is the
            // statement two lines above with four words added.
            "SELECT set_config('search_path', 'archive', false), current_schema();",
            // **One space moved.** The tokeniser flushes `search_path` as a word
            // and glues `=archive` into the next, so a reader of a bare `=` and
            // the joined reader see no assignment. Measured on PostgreSQL 18:
            // `access_records` lands in `archive`.
            "SET search_path =archive;",
            // **PostgreSQL's SQL-standard body lives in the statement's own
            // tokens.** A dollar-quoted body is re-cut and judged; these two are
            // not, so a whole-statement exemption swallows the redirect.
            // Measured on PostgreSQL 18: the function runs and an unqualified
            // `CREATE TABLE` lands in `archive`.
            "CREATE FUNCTION f() RETURNS text LANGUAGE sql RETURN set_config('search_path', 'archive', false);",
            "CREATE FUNCTION g() RETURNS void LANGUAGE sql BEGIN ATOMIC \
             SELECT set_config('search_path', 'archive', false); END;",
            // **And `RESET ALL`, which resets the path without naming it.**
            "RESET ALL;",
            // **`FROM CURRENT` at session scope makes a redirect permanent.**
            // Measured on PostgreSQL 18: it writes `pg_db_role_setting`, and a
            // brand-new session afterwards resolves an unqualified `CREATE
            // TABLE` into the saved schema.
            "ALTER DATABASE noctua SET search_path FROM CURRENT;",
            "ALTER ROLE noctua SET search_path FROM CURRENT;",
            // **And `SET ROLE`, which re-aims the `"$user"` the default path
            // begins with** — `SHOW search_path` is unchanged and an unqualified
            // `CREATE TABLE` lands in the role's own schema, which is exactly
            // the credit this gate's message says it gives to `public`.
            "SET ROLE archive;",
            "SET SESSION AUTHORIZATION archive;",
            // **The `LOCAL` and `SESSION` arm, decided here.** These two are the
            // only statements it stands for, so deleting the arm fails them.
            // Measured on
            // PostgreSQL 18.6, both leave `SHOW search_path` unchanged and put
            // an unqualified `CREATE TABLE` in the role's schema.
            "SET LOCAL ROLE archive;",
            "SET SESSION ROLE archive;",
            // **And the GUC spelling of the same statement.** `session_authorization`
            // is one token, so it matches neither the `role` arm nor the
            // two-word `SESSION AUTHORIZATION` one.
            "SET session_authorization = archive;",
            "SET session_authorization=archive;",
            "SET session_authorization TO archive;",
            "SET LOCAL session_authorization = archive;",
            "SET SESSION session_authorization = archive;",
            "RESET session_authorization;",
            "ALTER DATABASE noctua SET session_authorization = archive;",
            // **`RESET ALL` written as one word.** It is that statement plus
            // `SET SESSION AUTHORIZATION DEFAULT`, and `RESET ALL` is refused.
            "DISCARD ALL;",
            // **A custom setting overwritten between the save and the read-back.**
            // With only `SET LOCAL nc.p` fixtured, the arm that reads
            // `SET SESSION nc.p` is deletable with the suite green.
            // Measured on PostgreSQL 18.6: the read-back then puts `archive` on
            // the path and an unqualified `CREATE TABLE` lands there.
            "SELECT set_config('nc.p', current_setting('search_path'), false);\n\
             SET SESSION nc.p = 'archive';\n\
             SELECT set_config('search_path', current_setting('nc.p'), false);",
            // **And the same restore with anything appended to it.** A path is
            // an ordered list and an append leaves its first schema alone; a
            // role is one value, so an append names a *different* role. The
            // clearance above must not be the path's.
            "SELECT set_config('role', current_setting('role') || 'x', false);",
            "SELECT set_config('role', current_setting('search_path'), false);",
            // **And a second reader inside the value.** The clearance above is
            // for a setting written back exactly as it was found; a value that
            // reads one setting to *name* another is not that. Measured on
            // PostgreSQL 18.6, this moves `current_user` to whichever role the
            // outer read names.
            "SELECT set_config('role', current_setting(current_setting('role')), false);",
            // **A rider on the line the hatch clears.** `read_by_hand` matches
            // the cleared statement by *equality*, and its paragraph says a
            // second statement on the same line therefore fails to match and is
            // reported — "the safe direction". This decides that: relaxed to
            // `contains`, the match lets this line, whose first half is the
            // shape `0011` and `0012` write, carry a redirect through the hatch,
            // and the case fails.
            concat!(
                "SELECT set_config('search_path', ",
                "regexp_replace(current_setting('search_path'), ', pg_temp$', ''), false); ",
                "SELECT set_config('search_path', 'archive', false);"
            ),
            // **The function spelling of the same write.** `role` and
            // `session_authorization` are ordinary GUCs; measured on PostgreSQL
            // 18, this changes `current_user`, leaves `SHOW search_path` alone, and
            // puts `access_records` in that role's schema.
            "SELECT set_config('role', 'archive', false);",
            "SELECT set_config('session_authorization', 'archive', false);",
            // **And inside a `DO` block**, which is how this repository writes a
            // conditional migration — the role outlives the block.
            "DO $a$ BEGIN SET ROLE archive; END $a$;",
            // **And made permanent**, so every future session resolves there.
            "ALTER DATABASE noctua SET role = archive;",
            "ALTER ROLE noctua SET role = archive;",
            // **A `SET` value of bare `current_schema` is an identifier**, so it
            // stores a constant — and a migration may create a schema of that
            // name. Read as the function, the write is path-bearing and the
            // restore cleared, which opens `hold_the_path`'s own exception.
            "CREATE SCHEMA \"current_schema\";\n\
             SET nc.p = current_schema;\n\
             SELECT set_config('search_path', current_setting('nc.p'), false);",
            // **A top-level overwrite between two routine bodies.** `cut`
            // finishes the top level before it recurses, so the overwrite is
            // numbered before both bodies, and a reader trusting that order
            // trusts a save that has already been replaced.
            "DO $a$ BEGIN PERFORM set_config('nc.p', current_setting('search_path'), false); END $a$;\n\
             SET nc.p = 'archive';\n\
             DO $b$ BEGIN PERFORM set_config('search_path', current_setting('nc.p'), false); END $b$;",
            // And the save written inside a function body the migration never
            // calls, which is a save that never runs at all.
            "CREATE FUNCTION saver() RETURNS void LANGUAGE plpgsql AS $a$ BEGIN \
             PERFORM set_config('nc.p', current_setting('search_path'), false); END $a$;\n\
             SET nc.p = 'archive';\n\
             DO $b$ BEGIN PERFORM set_config('search_path', current_setting('nc.p'), false); END $b$;",
            // **The paren-less spelling, without the space**, which the second
            // copy of the rule has to read the same way.
            "CREATE SCHEMA \"current_schema\";\nSET search_path=current_schema;",
            // **A quoted lower-case function name is the same function.**
            "SELECT \"set_config\"('search_path', 'archive', false);",
            // **And the overwrite spelled without a space**, which a `SET`
            // reader keeping the glued `=` records under the name `nc.p=`.
            "SELECT set_config('nc.p', current_setting('search_path'), false);\n\
             SET nc.p='archive';\n\
             SELECT set_config('search_path', current_setting('nc.p'), false);",
            // **The paren-less spelling names a schema, not the function.** A
            // migration may create one called `current_schema`, and PostgreSQL
            // then puts an unqualified `CREATE TABLE` in it.
            "CREATE SCHEMA \"current_schema\";\nSET search_path TO current_schema;",
            "CREATE SCHEMA \"current_schema\";\nSET search_path TO CURRENT_SCHEMA;",
            // **A value that opens with the path's own function but reads a
            // different setting.** A rule asking whether a literal stands ahead
            // of `current_setting` fails, because `current_setting`'s own argument is a
            // literal — so the position test is true of every value that begins
            // with such a call, whatever it goes on to do. `nc.target` is a
            // constant a statement earlier; nothing here saved the path into it.
            "SELECT set_config('nc.target', 'archive', false),              set_config('search_path', current_setting('nc.target') || ', pg_temp', false);",
            // **And a function this rule cannot evaluate deciding the value.**
            // Each of these names the path and then discards it.
            "SELECT set_config('search_path', COALESCE(current_setting('nc.missing', true),              'archive'), false);",
            "SELECT set_config('search_path', substr(current_setting('search_path'), 1, 0)              || 'archive', false);",
            "SELECT set_config('search_path', CASE WHEN current_setting('search_path') = ''              THEN 'archive' ELSE 'archive' END, false);",
            // **A save overwritten before it is read back.** `set_config` is not
            // the only way to write a setting, and a name is not a value:
            // measured on PostgreSQL 18, `SHOW search_path` → `archive` and an
            // unqualified `CREATE TABLE` lands there.
            "SELECT set_config('nc.p', current_setting('search_path'), false);\n\
             SET nc.p = 'archive';\n\
             SELECT set_config('search_path', current_setting('nc.p'), false);",
            "SELECT set_config('nc.p', current_setting('search_path'), false);\n\
             SET LOCAL nc.p = 'archive';\n\
             SELECT set_config('search_path', current_setting('nc.p'), false);",
            // **And a refill inside a routine body**, which the walk finishes
            // every top-level statement before it reaches — so it is numbered
            // after statements that run before it and can be ordered against
            // nothing. A name written somewhere unorderable is not trusted.
            "SELECT set_config('nc.p', current_setting('search_path'), false);\n\
             DO $b$ BEGIN PERFORM set_config('nc.p', 'archive', false); END $b$;\n\
             SELECT set_config('search_path', current_setting('nc.p'), false);",
            // **Two sources and a constant between them**, where the rule cannot
            // say which decides the first schema on the path.
            "SELECT set_config('search_path', current_setting('nc.a') || 'archive'              || current_setting('nc.b'), false);",
        ] {
            let (found, _) = unreadable_statements_in(redirect, super::Dialect::PostgreSql);
            assert!(
                found
                    .iter()
                    .any(|(_, what, _)| what.contains("search_path")),
                "`{redirect}` redirects the path and was accepted: {found:?}"
            );
        }
    }

    /// And the shapes this repository actually writes are not refused.
    ///
    /// **This half is what makes the other half safe.** Refusing every
    /// `set_config` was measured too, and it refuses the six this repository
    /// writes in `0011` and `0012`. A routine's own configuration clause, a
    /// save-and-restore pair, and the `missing_ok` and schema-qualified
    /// spellings of `current_setting` are all legitimate, and a narrower rule
    /// refuses each.
    #[test]
    fn the_shapes_this_repository_writes_are_not_refused() {
        for allowed in [
            // `SET search_path = current_schema()` is not written here because
            // PostgreSQL does not parse it: `SET` takes an identifier or a
            // literal, never a call. A migration that means to keep the path
            // writes `set_config`, and those are below.
            "SELECT set_config('search_path', quote_ident(current_schema()), false);",
            // PostgreSQL's documented `missing_ok` form, which appends exactly
            // as the one-argument spelling does.
            "SELECT set_config('search_path', current_setting('search_path', true) || ', pg_temp', false);",
            // And the schema-qualified spelling that `search_path` hygiene
            // exists to encourage.
            "SELECT set_config('search_path', pg_catalog.current_setting('search_path') || ', pg_temp', false);",
            // **A restore is a restore because the save is there.** On its own
            // this reads an unrelated setting and the gate cannot know what is
            // in it — `nc.target` in the refused list above is the identical
            // statement with a constant behind it. What makes this one legible
            // is the save, in the same file, straight from the path.
            "SELECT set_config('nc.saved', current_setting('search_path'), false);\n             SELECT set_config('search_path', current_setting('nc.saved'), false);",
            "SELECT set_config('nc.saved', '', false);",
            // **A procedure's signature is not a call**, and this decides the
            // `PROCEDURE` half of that test. What follows the name
            // here is a list of parameter *types*, which reads as unreadable
            // arguments — so without the guard this hardening is a finding.
            "REVOKE ALL ON PROCEDURE set_config(text, text, boolean) FROM PUBLIC;",
            // **A data-modifying CTE**, which is the fifth spelling of a column
            // called `search_path` and the one an opener list misses.
            // Measured on PostgreSQL 18.6: the row is written and
            // `SHOW search_path` is unchanged before and after.
            "WITH bumped AS (UPDATE paths SET search_path = 'a' RETURNING id) SELECT * FROM bumped;",
            // **`missing_ok` written as `false`.** PostgreSQL accepts both
            // booleans there, and with only `true` fixtured, the skip that
            // makes the second argument an argument rather than a function is
            // decided in one direction.
            "SELECT set_config('search_path', current_setting('search_path', false), false);",
            // **A column may be called `search_path`.** Neither of these is a
            // session write: the first names no setting-writing keyword at all,
            // and the second names one but never assigns to the path. They are
            // here because they decide the two clauses that tell them apart from
            // a redirect — "a `SET` or a `RESET` is present" and "an operator
            // follows the name": deleting either clause fails them.
            "SELECT set_config('search_path', \"pg_catalog\".current_setting('search_path'), false);",
            "CREATE VIEW v AS SELECT * FROM paths WHERE search_path = 'archive';",
            "ALTER TABLE paths ALTER COLUMN search_path SET NOT NULL;",
            // **And writing to that column**, which is the third shape of the
            // same thing and which a `SET` anywhere refuses. No statement writes a
            // session setting and rows at once; `ON CONFLICT … DO UPDATE SET` is the same clause
            // reached from an `INSERT`.
            "UPDATE paths SET search_path = 'archive';",
            "INSERT INTO paths (id, search_path) VALUES (1, 'a') \
             ON CONFLICT (id) DO UPDATE SET search_path = 'b';",
            // **And a setting written back exactly as it was found.** This is
            // the shape of the path set back from its own `current_setting`
            // above, applied to the other settings the gate reads. It is a
            // measured no-op, and refusing it would tell a migration not to put
            // back what it has saved.
            "SELECT set_config('role', current_setting('role'), false);",
            // **A body written between single quotes, not dollar tags.** The
            // dollar-quoted twin of this is fixtured too, and this decides the
            // branch that brackets it — and what it
            // prevents is refusing the canonical hardening. Measured on
            // PostgreSQL 18.6: this writes `proconfig` and leaves
            // `SHOW search_path` at the session's own schema.
            "CREATE FUNCTION probe() RETURNS int LANGUAGE sql AS 'SELECT 1' \
             SECURITY DEFINER SET search_path = pg_catalog, pg_temp;",
            "SELECT set_config('session_authorization', \
             pg_catalog.current_setting('session_authorization', true), false);",
            // **Two shapes each already cleared here, in one file.** Every write
            // of the name comes straight from the path, so no ordering could
            // reorder them into a redirect — measured on PostgreSQL 18 as a
            // no-op. `0012` is one edit from this: it writes its saved setting
            // at the top level and carries a `DO` block, and making the save
            // conditional puts them at both levels.
            "SELECT set_config('nc.saved', current_setting('search_path'), false);\n\
             DO $b$ BEGIN\n\
             PERFORM set_config('nc.saved', current_setting('search_path'), false);\n\
             PERFORM set_config('search_path', current_setting('nc.saved'), false);\n\
             END $b$;\n\
             SELECT set_config('search_path', current_setting('nc.saved'), false);",
            // **A routine's own configuration clause writes `proconfig`, not
            // the session.** This is the canonical `SECURITY DEFINER` hardening
            // and `0011`'s own comment block measures it; the pin gate beside
            // this one is what judges it.
            "CREATE FUNCTION probe() RETURNS int LANGUAGE sql SECURITY DEFINER \
             SET search_path = pg_catalog, pg_temp AS $probe$ SELECT 1 $probe$;",
            "ALTER FUNCTION probe() SET search_path TO pg_catalog, pg_temp;",
            // **PostgreSQL takes a routine's attributes in any order**, so the
            // clause may follow the body. Measured on PostgreSQL 18: this writes
            // `proconfig` and leaves `SHOW search_path` alone, exactly as the
            // entry above it does.
            "CREATE FUNCTION probe() RETURNS int LANGUAGE sql SECURITY DEFINER \
             AS $probe$ SELECT 1 $probe$ SET search_path = pg_catalog, pg_temp;",
            // **A block comment in the header is a comment**, and a string in a
            // parameter default is a string — neither opens a body.
            "CREATE FUNCTION probe() RETURNS int LANGUAGE sql /* return the row */ \
             SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $probe$ SELECT 1 $probe$;",
            "CREATE FUNCTION probe(note text DEFAULT ' return ') RETURNS int LANGUAGE sql \
             SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $probe$ SELECT 1 $probe$;",
            // And the pin every function in this tree writes, which is the same
            // keyword the session-scope form uses.
            "CREATE FUNCTION probe() RETURNS int LANGUAGE sql SECURITY DEFINER \
             SET search_path FROM CURRENT AS $probe$ SELECT 1 $probe$;",
            // **A save and its restore inside one routine body** — a genuine
            // no-op, measured, and the shape a conditional migration needs.
            "DO $b$ BEGIN\n\
             PERFORM set_config('nc.saved', current_setting('search_path'), false);\n\
             PERFORM set_config('search_path', current_setting('nc.saved'), false);\n\
             END $b$;",
            // **And what the gate cannot evaluate, read by hand and written
            // down.** `0011` removes the `, pg_temp` it appended; the anchor is
            // what makes that safe, and no list of functions can see an anchor.
            "SELECT set_config('search_path',\n    regexp_replace(current_setting('search_path'), ', pg_temp$', ''), false);",
        ] {
            let (found, _) = unreadable_statements_in(allowed, super::Dialect::PostgreSql);
            // **Any refusal, not only one naming the path.** Asking whether the
            // verdict says `search_path` passes a shape refused for a different
            // reason — a signature read as an unreadable call, a file the reader
            // disagrees with itself about — here while `check` fails on it.
            assert!(
                found.is_empty(),
                "`{allowed}` is a shape this repository writes, and it was refused: \
                 {found:?}"
            );
        }
    }

    /// The one table a fragment of SQL creates, for the many one-statement cases.
    ///
    /// The fragment is given a `;` so it is a whole statement, which is what the
    /// scanner reads.
    fn created_table_at_top(fragment: &str) -> Option<String> {
        let mut names = caught(&format!("{fragment});"));
        names.pop()
    }

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    /// Every condition a migration tests, written down.
    ///
    /// **A migration can protect something and leave no trace in the catalog.**
    /// `PROTECTIONS` and `SHAPES` are snapshots of what the *database* holds —
    /// triggers, constraints, indexes, privileges — and a `DO $tag$ … IF … THEN
    /// RAISE` block holds nothing there at all. So `0012`'s guard, the one line
    /// that decides whether `0012` may be applied, is checked by nothing else:
    /// measured with `IF current_schema() IS NULL OR
    /// current_schema() LIKE 'pg\_temp%'` replaced by `IF FALSE`, and the schema
    /// rebuilt from the planted file —
    ///
    /// ```text
    /// ok  12 migration(s) applied, and the database now holds 12 of 12 on disk
    /// ok  44 protection(s) on the journal and the access records …
    /// check passed
    /// ```
    ///
    /// — while applying `0012` under a `pg_temp`-first path writes both trigger
    /// functions into the temporary schema, where they vanish at disconnect,
    /// leaves the triggers pointing at `0011`'s unpinned functions, and records
    /// `0012` as applied. That is the state `0012`'s own header calls *recorded
    /// as applied* while broken.
    ///
    /// So the conditions themselves are the census, read from the files and
    /// compared for equality — the same rule `PROTECTIONS` applies to the
    /// database, applied to the half of a migration the database does not keep.
    /// A condition that becomes a constant is a guard that stopped guarding, and
    /// it is a line in the diff rather than a number that drifts.
    ///
    /// The statement floor beside this one cannot reach the class: it counts
    /// statements, and `IF FALSE THEN` is the same count.
    #[test]
    fn every_condition_a_migration_tests_is_the_one_written_down() {
        /// `(the migration, the condition, how many times it is written)`.
        const CONDITIONS: [(&str, &str, usize); 8] = [
            ("0006", "EXISTS (SELECT 1 FROM access_records)", 1),
            ("0006", "TG_OP = 'INSERT'", 1),
            ("0010", "NEW.sequence <> expected", 1),
            ("0011", "NEW.sequence <> expected", 1),
            ("0011", "TG_OP = 'INSERT'", 1),
            // The guard this census exists for, and the two bodies beside it.
            (
                "0012",
                "current_schema() IS NULL OR current_schema() LIKE 'pg\\_temp%'",
                1,
            ),
            ("0012", "NEW.sequence <> expected", 1),
            ("0012", "TG_OP = 'INSERT'", 1),
        ];
        let files = super::migrations(root()).expect("the migrations are readable");
        let mut found: Vec<(String, String)> = Vec::new();
        for path in &files {
            let text = std::fs::read_to_string(path).expect("a migration is readable");
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.get(..4))
                .expect("a migration is numbered")
                .to_owned();
            for line in text.lines() {
                let said = line.trim();
                if let Some(rest) = said.strip_prefix("IF ")
                    && let Some(condition) = rest.strip_suffix(" THEN")
                {
                    found.push((name.clone(), condition.to_owned()));
                }
            }
        }
        let mut written: Vec<(String, String)> = Vec::new();
        for (file, condition, times) in CONDITIONS {
            for _ in 0..times {
                written.push((file.to_owned(), condition.to_owned()));
            }
        }
        found.sort();
        written.sort();
        assert_eq!(
            found, written,
            "the conditions the migrations test are not the ones written down. A guard \
             that became a constant leaves the database exactly as it was, so no snapshot \
             of triggers, constraints or privileges can see it — which is why these are \
             read from the files"
        );
    }

    /// The headroom each bound leaves, as the migration that argues for it says.
    ///
    /// **A number in a load-bearing argument, wrong by three orders of
    /// magnitude.** `0008` moved the journal's ceiling to 2^62 and justified it
    /// with *"4.6 x 10^18 entries — at one entry per microsecond, 146 million
    /// years"*. It is 146 **thousand**: `0007`, whose bound is twice as large,
    /// gets the same arithmetic right at *"292 000 years"*, so `0008` must say
    /// half of that and said a thousand times more. Nothing read it — the
    /// written-count gate covers `AGENTS.md`, `README.md` and `CLAUDE.md`, not
    /// `migrations/`, and the plant that corrected the figure left
    /// `check passed`.
    ///
    /// Both halves are held: the phrase must be in the file, and the phrase must
    /// be what the arithmetic gives. Editing the file alone fails the first;
    /// editing both to agree on a wrong number fails the second.
    #[test]
    fn each_bound_leaves_the_headroom_its_migration_argues_for() {
        /// Seconds in a Gregorian year, which is what a century of drift is
        /// measured in and what both files' figures were computed from.
        const A_YEAR: f64 = 31_556_952.0;
        /// The years a phrase names, for the two shapes these files write.
        fn the_years_said_in(phrase: &str) -> f64 {
            let (count, scale) = phrase
                .trim_end_matches(" years")
                .rsplit_once(' ')
                .map_or_else(
                    || (phrase.trim_end_matches(" years").to_owned(), 1.0),
                    |(digits, last)| match last {
                        "thousand" => (digits.to_owned(), 1_000.0),
                        "million" => (digits.to_owned(), 1_000_000.0),
                        // `292 000`: the space is a thousands separator, not a word.
                        _ => (format!("{digits}{last}"), 1.0),
                    },
                );
            count
                .replace(' ', "")
                .parse::<f64>()
                .expect("a year count is a number")
                * scale
        }
        for (file, bits, said) in [
            (
                "0007_the_journal_leaves_room_above_its_tail.sql",
                63,
                "292 000 years",
            ),
            (
                "0008_a_history_ends_where_it_ends_and_says_so.sql",
                62,
                "146 thousand years",
            ),
        ] {
            // One entry per microsecond, which is the rate both files reason at.
            let years = 2_f64.powi(bits) / 1e6 / A_YEAR;
            let named = the_years_said_in(said);
            assert!(
                (named - years).abs() / years < 0.01,
                "`{said}` is not the headroom 2^{bits} leaves at one entry per \
                 microsecond, which is {years:.0} years"
            );
            let text = std::fs::read_to_string(root().join("migrations").join(file))
                .expect("the migration is in this repository");
            assert!(
                text.contains(said),
                "`{file}` no longer says `{said}`. The figure is the whole argument for \
                 where its bound sits, and nothing else in this tree reads a number \
                 written in a migration"
            );
        }
    }

    /// The names one piece of SQL creates, in order.
    fn names(sql: &str) -> Vec<String> {
        tables_created_in(sql)
            .into_iter()
            .map(|table| table.name)
            .collect()
    }

    #[test]
    fn a_create_table_statement_yields_its_name() {
        assert_eq!(
            created_table_at_top("CREATE TABLE journal_entries (").as_deref(),
            Some("journal_entries")
        );
        assert_eq!(
            created_table_at_top("create table if not exists access_records(").as_deref(),
            Some("access_records")
        );
    }

    #[test]
    fn a_name_on_the_next_line_is_still_a_table() {
        // **Acceptance criterion 5 rests on this.** Read line by line, this
        // ordinary, valid statement creates an unbounded table with
        // `cargo xtask check` green — verified against a real PostgreSQL.
        let sql = "CREATE TABLE\n    login_attempts (\n    id TEXT PRIMARY KEY\n);\n";
        assert_eq!(names(sql), vec!["login_attempts".to_owned()]);
        assert_eq!(tables_created_in(sql)[0].line, 1);
    }

    #[test]
    fn the_second_statement_on_a_line_is_still_a_table() {
        // Read line by line, also green, and also creates the table.
        let sql = "CREATE TABLE a (id INT); CREATE TABLE login_attempts (id INT);\n";
        assert_eq!(
            names(sql),
            vec!["a".to_owned(), "login_attempts".to_owned()]
        );
    }

    #[test]
    fn a_block_comment_hides_a_table_and_does_not_invent_one() {
        // Read line by line, both directions are defeated by two characters.
        // Commenting a table out leaves the reverse gate silent about a
        // `limits.toml` entry with nothing behind it; prose inside a block
        // comment produces a finding naming a table nobody wrote.
        assert_eq!(
            names("/* CREATE TABLE ghost (id INT); */\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            names(
                "/*\n  CREATE TABLE ghost (\n    id INT\n  );\n*/\nCREATE TABLE real_one (id INT);\n"
            ),
            vec!["real_one".to_owned()]
        );
        // Nested, which PostgreSQL allows and a toggle would get wrong: the inner
        // `*/` would end the comment and expose the rest.
        assert_eq!(
            names("/* outer /* inner */ CREATE TABLE ghost (id INT); */\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_dollar_quoted_body_is_read_as_sql_of_its_own() {
        // Every trigger function in `migrations/` is written this way. The `;`s
        // inside one are not boundaries of the *outer* statement — the body is
        // cut separately — and a `--` inside one is a comment there too, which
        // it would not be if the body were also copied into the outer
        // statement verbatim.
        let sql = "CREATE FUNCTION f() RETURNS TRIGGER AS $$\nBEGIN\n  -- CREATE TABLE ghost (id INT);\n  RETURN NEW;\nEND;\n$$ LANGUAGE plpgsql;\nCREATE TABLE real_one (id INT);\n";
        assert_eq!(names(sql), vec!["real_one".to_owned()]);
    }

    #[test]
    fn a_line_number_points_at_the_statement_that_starts_there() {
        let sql =
            "-- a comment\n\nCREATE TABLE first (id INT);\n\n\nCREATE TABLE second (id INT);\n";
        let found = tables_created_in(sql);
        assert_eq!(found[0].line, 3, "{found:#?}");
        assert_eq!(found[1].line, 6, "{found:#?}");
    }

    #[test]
    fn an_unlogged_table_is_a_table_and_a_temporary_one_is_not() {
        // Excluded together with `TEMP` — a temporary table "cannot grow without
        // bound" — `UNLOGGED` walks straight past this gate, and that is false of
        // it: an unlogged table is permanent, survives every session and is
        // emptied only by an unclean shutdown.
        assert_eq!(
            created_table_at_top("CREATE UNLOGGED TABLE widgets (").as_deref(),
            Some("widgets")
        );
        for temporary in [
            "CREATE TEMP TABLE scratch (",
            "CREATE TEMPORARY TABLE scratch (",
            "CREATE GLOBAL TEMPORARY TABLE scratch (",
            "CREATE LOCAL TEMP TABLE scratch (",
        ] {
            assert_eq!(created_table_at_top(temporary), None, "{temporary}");
        }
    }

    #[test]
    fn whitespace_is_not_a_way_past_the_gate() {
        // `CREATE  TABLE` and `CREATE\tTABLE` are the same statement to
        // PostgreSQL and are two ways past a prefix match on a single space.
        for spelling in [
            "CREATE  TABLE widgets (",
            "CREATE\tTABLE widgets (",
            "  create   table   widgets(",
        ] {
            assert_eq!(
                created_table_at_top(spelling).as_deref(),
                Some("widgets"),
                "{spelling:?}"
            );
        }
    }

    #[test]
    fn a_quoted_name_is_reported_whole_and_a_schema_is_dropped() {
        // Trimmed of its quotes and then re-sliced from the quoted original for
        // that many bytes, `"widgets"` comes back as `widget` —
        // a diagnostic naming a table nobody wrote, which is the one thing
        // acceptance criterion 5 asks of it.
        assert_eq!(
            created_table_at_top("CREATE TABLE \"widgets\" (").as_deref(),
            Some("widgets")
        );
        assert_eq!(
            names("CREATE TABLE \"a--b\" (id INT);\n"),
            vec!["a--b".to_owned()],
            "the comment strip must not eat a quoted name"
        );
        // `public.widgets` is `widgets`, which is what `limits.toml` spells;
        // kept as `public.widgets`, it produces a spurious finding.
        assert_eq!(
            created_table_at_top("CREATE TABLE public.widgets (").as_deref(),
            Some("widgets")
        );
    }

    #[test]
    fn a_name_outside_ascii_is_read_rather_than_panicked_on() {
        // `code.len() - rest.len()` assumes `to_uppercase` preserves byte length.
        // It does not: `ı` uppercases to `I`, and such a slice lands inside a
        // character — a panic where a diagnostic belongs.
        assert_eq!(
            created_table_at_top("CREATE TABLE ıx (").as_deref(),
            Some("ıx")
        );
        assert_eq!(
            created_table_at_top("CREATE TABLE straße (").as_deref(),
            Some("straße")
        );
    }

    #[test]
    fn a_mention_in_a_comment_is_not_a_table() {
        // Every migration in this repository explains what it creates in a
        // comment above it. A gate that read its own prose would report the same
        // table twice and be switched off within a day.
        assert_eq!(
            names("-- CREATE TABLE something_else (id INT);\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            created_table_at_top("CREATE INDEX journal_entries_realm ON"),
            None
        );
        assert_eq!(
            created_table_at_top("CREATE FUNCTION journal_is_append_only()"),
            None
        );
    }

    #[test]
    fn a_comment_is_dropped_and_the_line_it_was_on_is_still_counted() {
        // A finding points at a line number, so dropping comments must not move
        // the statements that follow them.
        let found = tables_created_in("-- one\n/* two\n   three */\nCREATE TABLE x (id INT);\n");
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].name, "x");
        assert_eq!(found[0].line, 4, "the comment moved the line number");
    }

    /// The names one piece of SQL creates, in order, as the gate would see them.
    fn caught(sql: &str) -> Vec<String> {
        let changes = changes_in(sql);
        changes
            .created
            .into_iter()
            .map(|table| table.name)
            .chain(changes.renamed.into_iter().map(|rename| rename.to))
            .collect()
    }

    #[test]
    fn an_escape_string_does_not_desynchronise_the_rest_of_the_file() {
        // **The worst of the four.** `E'…\'…'` uses backslash escapes, and a
        // scanner closing the literal at `\'` makes `edit rows here` code, the
        // `/*` inside the literal a real block comment that nothing closes, and
        // **everything to the end of the file blanked** — the
        // `CREATE TABLE` with it. Verified against PostgreSQL 18.6: the file
        // applies and the table is created, with `check passed`.
        let sql = "COMMENT ON TABLE journal_entries IS\n  \
                   E'append-only; don\\'t edit rows here /* see the trigger';\n\n\
                   CREATE TABLE login_attempts (\n    id TEXT PRIMARY KEY\n);\n";
        assert_eq!(caught(sql), vec!["login_attempts".to_owned()]);
        // And the ordinary `''` escape.
        let doubled = "COMMENT ON TABLE t IS 'don''t /* ';\nCREATE TABLE x (id INT);\n";
        assert_eq!(caught(doubled), vec!["x".to_owned()]);
    }

    #[test]
    fn a_table_created_inside_a_routine_body_is_still_a_table() {
        // The most idiomatic conditional DDL there is. With the body copied
        // whole and never looked inside, and the outer statement a `DO`, this
        // creates an unbounded table with the gate green.
        let sql = "DO $$\nBEGIN\n    IF NOT EXISTS (SELECT 1 FROM pg_tables \
                   WHERE tablename = 'login_attempts') THEN\n        \
                   CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n    \
                   END IF;\nEND\n$$;\n";
        assert_eq!(caught(sql), vec!["login_attempts".to_owned()]);
    }

    #[test]
    fn a_select_into_creates_a_table_with_no_create_in_it() {
        assert_eq!(
            caught("SELECT id, realm INTO login_attempts FROM access_records WHERE FALSE;\n"),
            vec!["login_attempts".to_owned()]
        );
        // `INSERT INTO` is not a creation, and neither is a temporary one.
        assert_eq!(
            caught("INSERT INTO access_records (id) VALUES ('a');\n"),
            [] as [String; 0]
        );
        assert_eq!(
            caught("SELECT id INTO TEMP scratch FROM access_records;\n"),
            [] as [String; 0]
        );
        // **And the optional `TABLE`, in each of the four places it can stand.**
        // Every one of these creates the table it names; with the keyword not
        // stepped over, each reports a table called `table` and leaves the real
        // one unbounded.
        for statement in [
            "SELECT id INTO TABLE login_attempts FROM access_records;\n",
            "SELECT id INTO UNLOGGED TABLE login_attempts FROM access_records;\n",
        ] {
            assert_eq!(
                caught(statement),
                vec!["login_attempts".to_owned()],
                "{statement}"
            );
        }
        assert_eq!(
            caught("SELECT id INTO TABLE archive.login_attempts FROM access_records;\n"),
            vec!["archive.login_attempts".to_owned()]
        );
        // A temporary one is still not a creation, whichever way it is spelled.
        assert_eq!(
            caught("SELECT id INTO TEMP TABLE scratch FROM access_records;\n"),
            [] as [String; 0]
        );
        // **And the two qualifiers that only ever stand in front of `TEMPORARY`.**
        // Not stepped over, each of these is reported as creating a
        // table called `global` or `local`, and the temporary excuse below them
        // is never reached.
        for statement in [
            "SELECT id INTO GLOBAL TEMP scratch FROM access_records;\n",
            "SELECT id INTO LOCAL TEMPORARY scratch FROM access_records;\n",
            "SELECT id INTO GLOBAL TEMPORARY TABLE scratch FROM access_records;\n",
        ] {
            assert!(
                caught(statement).is_empty(),
                "{statement} -> {:?}",
                caught(statement)
            );
        }
        // **The mirror, and it is why they are stepped over only in front of a
        // temporary.** Both are unreserved keywords, so a table really can be
        // called `global`.
        assert_eq!(
            caught("SELECT id INTO global FROM access_records;\n"),
            vec!["global".to_owned()]
        );
        assert_eq!(
            caught("SELECT id INTO TABLE local FROM access_records;\n"),
            vec!["local".to_owned()]
        );
        // And a table genuinely called `table` is quoted, which is not a
        // keyword — both spellings read it as the name it is.
        assert_eq!(
            caught("SELECT id INTO \"table\" FROM access_records;\n"),
            vec!["table".to_owned()]
        );
        assert_eq!(
            caught("SELECT id INTO TABLE \"table\" FROM access_records;\n"),
            vec!["table".to_owned()]
        );
        // **And plpgsql's assignment into a variable is not a table.** Inside a
        // routine body `SELECT … INTO x` reads a row into a local, so reading it
        // as a creation would report a table nobody wrote — which is the defect
        // this gate exists not to have.
        let body = "CREATE FUNCTION f() RETURNS TRIGGER AS $$\nDECLARE n BIGINT;\n\
                    BEGIN\n  SELECT count(*) INTO n FROM access_records;\n  \
                    RETURN NEW;\nEND;\n$$ LANGUAGE plpgsql;\n";
        assert!(caught(body).is_empty(), "{:?}", caught(body));
    }

    /// The ordering rule over one file's changes, with that file's domains.
    ///
    /// The catalog text is empty, so every finding lands at line 1 — which is
    /// what it means when the entry cannot be found, and these fixtures have no
    /// catalog. `the_entry_is_located_where_it_is_written` is what checks the
    /// other case.
    fn ordering(changes: &super::Changes) -> Vec<crate::gates::Finding> {
        super::order_by_names_a_column(
            &changes.created,
            &changes.columns,
            &changes.domains,
            "",
            nc_catalog::Database::Server,
        )
    }

    /// An ordering column a later `CREATE TABLE` declares again is a column.
    ///
    /// **An `ALTER TABLE` applies to the table as it stands when it runs**, not
    /// to the last `CREATE TABLE` whatever order the two ran in — or a column
    /// dropped, then restored by a `DROP TABLE` and a re-creation below it,
    /// reads as gone. Measured against the real migration set: a legitimate set
    /// of three statements is then reported as *"orders by `recorded`, which no
    /// migration declares as a column of it"*, with a `fix` listing the columns
    /// as `id` about a statement two lines above it that declares `recorded`.
    /// That is the refusing-something-legitimate direction, and a migration
    /// cannot be edited once applied, so the advice is about a file the
    /// developer may no longer change.
    ///
    /// The second half is the control this needs in order not to be the
    /// opposite mistake: a column that really was dropped and not restored is
    /// still a finding.
    #[test]
    fn an_ordering_column_a_later_creation_declares_again_is_read_as_there() {
        let missing = |changes: &super::Changes| {
            ordering(changes).iter().any(|finding| {
                finding
                    .what
                    .contains("which no migration declares as a column of it")
            })
        };

        let restored = super::changes_in(
            "CREATE TABLE access_records (id TEXT PRIMARY KEY, recorded BIGINT NOT NULL);\n\
             ALTER TABLE access_records DROP COLUMN recorded;\n\
             DROP TABLE access_records;\n\
             CREATE TABLE access_records (id TEXT PRIMARY KEY, recorded BIGINT NOT NULL);\n",
        );
        assert!(
            !missing(&restored),
            "a column the last `CREATE TABLE` declares was read as dropped by an \
             `ALTER TABLE` that ran before it: {:#?}",
            ordering(&restored)
        );

        let dropped = super::changes_in(
            "CREATE TABLE access_records (id TEXT PRIMARY KEY, recorded BIGINT NOT NULL);\n\
             ALTER TABLE access_records DROP COLUMN recorded;\n",
        );
        assert!(
            missing(&dropped),
            "a column that really was dropped is not a finding any more: {:#?}",
            ordering(&dropped)
        );

        // And the order is the statements', not the list's: the same three
        // statements with the drop *after* the re-creation leave the column
        // gone, and that is a finding again.
        let dropped_last = super::changes_in(
            "CREATE TABLE access_records (id TEXT PRIMARY KEY, recorded BIGINT NOT NULL);\n\
             DROP TABLE access_records;\n\
             CREATE TABLE access_records (id TEXT PRIMARY KEY, recorded BIGINT NOT NULL);\n\
             ALTER TABLE access_records DROP COLUMN recorded;\n",
        );
        assert!(
            missing(&dropped_last),
            "a drop below the re-creation was read as one above it: {:#?}",
            ordering(&dropped_last)
        );
    }

    #[test]
    fn an_ordering_column_that_does_not_sort_in_creation_order_is_a_finding() {
        // **A `text` column holding an identifier, as the ordering column.**
        // Crockford base32 is variable width — twelve characters below `32^12`
        // and thirteen at or above it, which `nc-ids`' `base32` header dates at
        // about 2034 — and SQL sorts `text` byte by byte. From the first
        // thirteen-character row until the last twelve-character one is gone,
        // ascending order is not creation order:
        //
        // ```text
        // minted oldest first: 100000000000  100000000001  G00000000000
        //                      1000000000000 1000000000001 4000000000000
        // ORDER BY id ASC:     100000000000  1000000000000 1000000000001
        //                      100000000001  4000000000000 G00000000000
        // ```
        //
        // Measured with the real pruner over those six rows: it removes the
        // oldest *and the third-newest*, and reports an ordinary `Pruned`.
        // `base32`'s own header says "sorting by identifier value still works,
        // and that is what anything reading these names should do" — and the
        // pruner is a reader that cannot, because SQL sorts the text.
        let text_ordered = super::changes_in(
            "CREATE TABLE access_records (id TEXT NOT NULL, recorded TEXT NOT NULL);\n",
        );
        let findings = ordering(&text_ordered);
        assert!(
            findings.iter().any(|finding| finding.what.contains("text")),
            "a text ordering column was accepted: {findings:#?}"
        );

        // The types whose ascending order is creation order — including the
        // four an allow-list of nine would refuse: an exact decimal, a date, and the
        // narrow integers. All of them sort in creation order, and refusing one
        // is a diagnostic a developer cannot act on except by editing this file.
        for numeric in [
            "BIGINT",
            "bigint",
            "INT8",
            "integer",
            "SMALLINT",
            "INT2",
            "NUMERIC",
            "NUMERIC(20, 0)",
            "DECIMAL",
            "DATE",
            "timestamptz",
            "TIMESTAMP WITH TIME ZONE",
            // **The eight this list names, and this drives them.** `int`, `int4`
            // and the six serial spellings removed from
            // `SORTS_IN_CREATION_ORDER` fail these rows, and with them gone an
            // ordinary
            // `INT NOT NULL` ordering column is reported as one that does not
            // sort in creation order. A list entry nothing drives is a list
            // entry nobody can tell from a typo.
            "INT",
            "int4",
            "BIGSERIAL",
            "serial8",
            "SERIAL",
            "serial4",
            "SMALLSERIAL",
            "serial2",
            "pg_catalog.int8",
        ] {
            let safe =
                format!("CREATE TABLE access_records (id TEXT, recorded {numeric} NOT NULL);\n");
            let changes = super::changes_in(&safe);
            let findings = ordering(&changes);
            assert!(
                findings.is_empty(),
                "`{numeric}` was reported: {findings:#?}"
            );
        }

        // And the ones that are refused, each for its own reason. `timestamp`
        // is `timestamp without time zone`: a
        // reading with no answer to whose clock took it, which at a
        // daylight-saving fall-back repeats an hour. A first-word rule cannot
        // tell those two spellings apart at all.
        for unsafe_type in [
            "TIMESTAMP",
            "TIMESTAMP WITHOUT TIME ZONE",
            "REAL",
            "DOUBLE PRECISION",
            "UUID",
            "BIGINT ARRAY",
        ] {
            let refused = format!(
                "CREATE TABLE access_records (id TEXT, recorded {unsafe_type} NOT NULL);\n"
            );
            let changes = super::changes_in(&refused);
            let findings = ordering(&changes);
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.contains("which is")),
                "`{unsafe_type}` was accepted: {findings:#?}"
            );
        }
    }

    /// A routine body runs where it is written, not after everything else.
    ///
    /// **`cut` finishes the top level and then re-reads the bodies**, so in cut
    /// order every statement inside a `DO` block comes back numbered after every statement
    /// outside one — and three lists keyed on that number answer *what did the
    /// last statement to decide this say*. Wrong in both directions from one
    /// root, and both measured on PostgreSQL 18.6.
    ///
    /// The too-strong half is the one that will be hit first, because a
    /// conditional block is the idiomatic way to write guarded DDL: a type
    /// altered inside one and altered back outside it ends as the outside
    /// statement leaves it, and a gate reading cut order refuses the migration
    /// for the type the block set. The too-weak half is silent: a table dropped,
    /// re-created inside a block and dropped again does not exist, and such a gate
    /// goes on bounding it.
    #[test]
    fn a_routine_body_runs_where_it_is_written() {
        let both = super::changes_in(
            "CREATE TABLE access_records (id TEXT, recorded BIGINT NOT NULL);\n\
             DO $$ BEGIN ALTER TABLE access_records ALTER COLUMN recorded TYPE TEXT; \
             END $$;\n\
             ALTER TABLE access_records ALTER COLUMN recorded TYPE BIGINT USING \
             recorded::bigint;\n",
        );
        let findings = ordering(&both);
        assert!(
            !findings
                .iter()
                .any(|finding| finding.what.contains("`text`")),
            "the last statement of the file leaves `recorded` a `bigint`, and the gate read \
             it through the block written above that line: {findings:#?}"
        );

        // And the control, so the assertion above is about the *order* and not
        // about a block being ignored: the same two statements with the block
        // written last leave the column `text`, and the gate says so.
        let blocked = super::changes_in(
            "CREATE TABLE access_records (id TEXT, recorded BIGINT NOT NULL);\n\
             ALTER TABLE access_records ALTER COLUMN recorded TYPE BIGINT USING \
             recorded::bigint;\n\
             DO $$ BEGIN ALTER TABLE access_records ALTER COLUMN recorded TYPE TEXT; \
             END $$;\n",
        );
        let findings = ordering(&blocked);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("`text`")),
            "the block is the last statement of the file and leaves `recorded` a `text`: \
             {findings:#?}"
        );

        // The other direction, and it is silent rather than loud: a table
        // dropped, re-created inside a block, and dropped again is gone.
        let gone = super::changes_in(
            "DROP TABLE access_records;\n\
             DO $$ BEGIN CREATE TABLE access_records (id TEXT); END $$;\n\
             DROP TABLE access_records;\n",
        );
        let made = gone
            .created
            .iter()
            .find(|created| created.name == "access_records")
            .map(|created| created.at)
            .expect("the block creates the table");
        let drops: Vec<usize> = gone
            .renamed
            .iter()
            .filter(|rename| rename.from == "access_records" && rename.to.is_empty())
            .map(|rename| rename.at)
            .collect();
        assert_eq!(drops.len(), 2, "{gone:#?}");
        assert!(
            drops[0] < made && made < drops[1],
            "the block is written between the two drops and must run between them: the \
             create is statement {made} and the drops are {drops:?}"
        );
    }

    /// Every shape that decides what the ordering column is, after the `CREATE`.
    ///
    /// Each of these is a way past a type rule that reads the `CREATE TABLE` and
    /// nothing else, and each is driven here as its own case.
    #[test]
    fn every_statement_that_changes_the_ordering_column_is_read() {
        // **A later `ALTER COLUMN … TYPE` turns such a rule off.** With the type
        // read from the `CREATE TABLE` alone, the same change written as
        // an alteration passes — same database, same resulting schema, and the
        // gate firing for one spelling and not the other:
        //
        // ```text
        // in the CREATE:  error `access_records` orders by its column, which is `text`
        // in an ALTER:    check passed
        // ```
        let altered = super::changes_in(
            "CREATE TABLE access_records (id TEXT, recorded BIGINT NOT NULL);\n\
             ALTER TABLE access_records ALTER COLUMN recorded TYPE TEXT USING \
             recorded::text;\n",
        );
        let findings = ordering(&altered);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("`text`")),
            "an `ALTER COLUMN … TYPE` was not followed: {findings:#?}"
        );

        // The ordering column arriving by `ADD COLUMN`, which leaves a
        // `CREATE`-only type rule with nothing to read, and a `CREATE`-only NULL
        // rule reporting *may be NULL* about a column declared `NOT NULL`.
        let added = super::changes_in(
            "CREATE TABLE access_records (id TEXT);\n\
             ALTER TABLE access_records ADD COLUMN recorded BIGINT NOT NULL;\n",
        );
        let findings = ordering(&added);
        assert!(findings.is_empty(), "{findings:#?}");

        // A domain over one of those types is what it is built on. There is no
        // way to permit this from `catalog/limits.toml`, and the migrations are
        // what this gate reads, so it follows the declaration.
        let domain = super::changes_in(
            "CREATE DOMAIN epoch_millis AS BIGINT;\n\
             CREATE TABLE access_records (id TEXT, recorded EPOCH_MILLIS NOT NULL);\n",
        );
        let findings = ordering(&domain);
        assert!(findings.is_empty(), "{findings:#?}");

        // And a domain over a type that does not sort in creation order is
        // still refused — the resolution is not an exemption.
        let over_text = super::changes_in(
            "CREATE DOMAIN record_id AS TEXT;\n\
             CREATE TABLE access_records (id TEXT, recorded RECORD_ID NOT NULL);\n",
        );
        let findings = ordering(&over_text);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("`text`")),
            "a domain over `text` was accepted: {findings:#?}"
        );

        // **A rename carries no type, and can move a different column's type
        // onto the ordering column's name.** Two ordinary renames in one
        // migration, and a reader that only records them reads `recorded` as the
        // `bigint` identity column while the database has the TEXT Crockford
        // identifier under that name. The real pruner over seven rows minted in
        // order then removes the oldest *and the fourth- and fifth-newest*, reporting an ordinary
        // `Pruned` — verbatim the failure `migrations/0002_access_records.sql`'s header, `0006`,
        // the catalog entry and `SORTS_IN_CREATION_ORDER` all exist to prevent.
        let renamed = super::changes_in(
            "CREATE TABLE access_records (id TEXT NOT NULL, recorded BIGINT NOT NULL);\n\
             ALTER TABLE access_records RENAME COLUMN recorded TO recorded_seq;\n\
             ALTER TABLE access_records RENAME COLUMN id TO recorded;\n",
        );
        let findings = ordering(&renamed);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("`text`")),
            "a rename moved a `text` column onto the ordering name: {findings:#?}"
        );

        // And the nullability half, which falls the same way: a nullable column
        // renamed into the ordering column's name is read as `NOT NULL` by such a
        // reader.
        let nullable = super::changes_in(
            "CREATE TABLE access_records (id TEXT, recorded BIGINT NOT NULL);\n\
             ALTER TABLE access_records DROP COLUMN recorded;\n\
             ALTER TABLE access_records ADD COLUMN spare BIGINT;\n\
             ALTER TABLE access_records RENAME COLUMN spare TO recorded;\n",
        );
        let findings = ordering(&nullable);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("may be NULL")),
            "a nullable column renamed into the ordering name was read as NOT NULL: \
             {findings:#?}"
        );

        // The rename that is ordinary and correct stays clean, or the two
        // assertions above would hold against a rule that reports every rename.
        let moved = super::changes_in(
            "CREATE TABLE access_records (id TEXT, at_ms BIGINT NOT NULL);\n\
             ALTER TABLE access_records RENAME COLUMN at_ms TO recorded;\n",
        );
        let findings = ordering(&moved);
        assert!(findings.is_empty(), "{findings:#?}");

        // **A quoted name is not the same column as an unquoted one.** Folding
        // both, `"RECORDED"` stands in for `recorded`, and a gate doing that
        // concludes the ordering column is `bigint` while the database
        // has it as TEXT with a second column beside it.
        let quoted = super::changes_in(
            "CREATE TABLE access_records (id TEXT, recorded TEXT NOT NULL);\n\
             ALTER TABLE access_records ADD COLUMN \"RECORDED\" BIGINT NOT NULL;\n",
        );
        let findings = ordering(&quoted);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("`text`")),
            "a quoted `\"RECORDED\"` was read as the ordering column: {findings:#?}"
        );

        // **And an `ALTER TABLE` inside a `DO $$ … $$` body is one.** A clause
        // reader requiring `ALTER TABLE` at the statement's first two tokens
        // misses it, because a body's first cut statement begins `BEGIN` — so this
        // passes, while the same clause written second in the body is caught.
        let conditional = super::changes_in(
            "CREATE TABLE access_records (id TEXT, recorded BIGINT NOT NULL);\n\
             DO $$ BEGIN ALTER TABLE access_records ALTER COLUMN recorded TYPE TEXT; \
             END $$;\n",
        );
        let findings = ordering(&conditional);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("`text`")),
            "an `ALTER TABLE` inside a `DO` body was not read: {findings:#?}"
        );

        // **A column whose type cannot be read is a finding, not a pass.** An
        // assertion of the opposite would turn the exclusion into a rule and pin
        // the hole open. A sort order that cannot be judged is one the pruner deletes by
        // anyway.
        let unknown = super::changes_in("CREATE TABLE access_records AS SELECT 1;\n");
        let findings = ordering(&unknown);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("do not say what it is")),
            "a table whose columns cannot be read was passed: {findings:#?}"
        );
    }

    /// A finding about an entry is reported where the entry is written.
    #[test]
    fn the_entry_is_located_where_it_is_written() {
        let catalog = "# a header\n\n[tables.journal_entries]\nmax_rows = 1\n\n\
                       [tables.access_records]\nmax_rows = 2\norder_by = \"recorded\"\n";
        assert_eq!(super::line_of_entry(catalog, "journal_entries"), 3);
        assert_eq!(super::line_of_entry(catalog, "access_records"), 6);
        // Line 1 when it is not there, which is where a reader who opens the
        // file lands anyway.
        assert_eq!(super::line_of_entry(catalog, "sessions"), 1);

        let changes =
            super::changes_in("CREATE TABLE access_records (id TEXT, recorded TEXT NOT NULL);\n");
        let findings = super::order_by_names_a_column(
            &changes.created,
            &changes.columns,
            &changes.domains,
            catalog,
            nc_catalog::Database::Server,
        );
        assert!(
            findings.iter().any(|finding| finding.line == 6),
            "every finding about an entry was reported at line 1, and the entries run \
             from line 20 to the end of the real file: {findings:#?}"
        );

        // **And the key, not the entry.** `line_of_key` sits beside
        // `line_of_entry`, and pointing the column-missing finding at the
        // entry's `strategy` line instead of its `order_by` line fails this. A
        // finding's line is the line to edit, and this one is the key that is
        // wrong.
        let missing =
            super::changes_in("CREATE TABLE access_records (id TEXT, note TEXT NOT NULL);\n");
        let findings = super::order_by_names_a_column(
            &missing.created,
            &missing.columns,
            &missing.domains,
            catalog,
            nc_catalog::Database::Server,
        );
        let column = findings
            .iter()
            .find(|finding| {
                finding
                    .what
                    .contains("which no migration declares as a column of it")
            })
            .unwrap_or_else(|| {
                panic!(
                    "`recorded` is not a column of that table and was not reported: {findings:#?}"
                )
            });
        assert_eq!(
            column.line, 8,
            "the column-missing finding points at line {}, and `order_by` is written on \
             line 8 — a finding's line is the line to edit",
            column.line
        );
        // The fallback the sibling site relies on: an entry with no `order_by`
        // key at all is located at its header rather than at line 1.
        assert_eq!(
            super::line_of_key(catalog, "journal_entries", "order_by"),
            3
        );
        assert_eq!(super::line_of_key(catalog, "access_records", "order_by"), 8);
    }

    #[test]
    fn an_ordering_column_that_may_be_null_is_a_finding() {
        // **A gate asking that `order_by` names a column cannot stop there.**
        // `ORDER BY … ASC` sorts NULLs last, so a nullable ordering column makes
        // every row missing that value the newest rows in the table and the last
        // ever removed — the table grows past its limit while the pruner runs
        // and reports success. Measured on PostgreSQL 18.6, five of ten rows
        // null: `ORDER BY at ASC LIMIT 4` answers `1, 2, 3, 4`.
        //
        // Driven against `access_records`, whose real bound orders by
        // `recorded`.
        let nullable =
            super::changes_in("CREATE TABLE access_records (id TEXT, recorded BIGINT);\n");
        let findings = ordering(&nullable);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("may be NULL")),
            "a nullable ordering column was accepted: {findings:#?}"
        );

        // And the shapes that are safe, so the rule is not "every column is a
        // finding": a primary key, and an explicit `NOT NULL`.
        for safe in [
            "CREATE TABLE access_records (id TEXT, recorded BIGINT PRIMARY KEY);\n",
            "CREATE TABLE access_records (id TEXT, recorded BIGINT NOT NULL);\n",
            "CREATE TABLE access_records (id TEXT, recorded BIGINT, PRIMARY KEY (recorded));\n",
        ] {
            let changes = super::changes_in(safe);
            let findings = ordering(&changes);
            // Empty, for the reason above: `!any(…)` over an empty list is
            // true however the reader behaved.
            assert!(findings.is_empty(), "`{safe}` was reported: {findings:#?}");
        }
    }

    #[test]
    fn a_column_that_cannot_be_null_is_told_from_one_that_can() {
        let read = |sql: &str| {
            let statements = super::statements(sql);
            super::columns_that_cannot_be_null(statements.first().expect("one statement"))
        };
        assert_eq!(
            read("CREATE TABLE t (id TEXT PRIMARY KEY, at BIGINT NOT NULL, note TEXT);\n"),
            vec!["at".to_owned(), "id".to_owned()]
        );
        // **The two idiomatic ways of declaring an insertion-order column**,
        // which a reader that knows only `NOT NULL` and `PRIMARY KEY` refuses.
        // PostgreSQL expands `bigserial` to `bigint NOT NULL
        // DEFAULT nextval(…)` and an identity column is not nullable either;
        // `SORTS_IN_CREATION_ORDER` names all six serial spellings as safe to
        // order by, so a gate refusing them refuses what the tree recommends.
        for declared in [
            "CREATE TABLE t (at BIGINT GENERATED BY DEFAULT AS IDENTITY, note TEXT);\n",
            "CREATE TABLE t (at BIGINT GENERATED ALWAYS AS IDENTITY, note TEXT);\n",
            "CREATE TABLE t (at BIGSERIAL, note TEXT);\n",
            "CREATE TABLE t (at SERIAL, note TEXT);\n",
            "CREATE TABLE t (at smallserial, note TEXT);\n",
        ] {
            assert_eq!(
                read(declared),
                vec!["at".to_owned()],
                "`{declared}` declares a column that cannot hold a null"
            );
        }
        // And what it must not accept: an ordinary column of a type whose name
        // merely begins the same way.
        let left_over = read("CREATE TABLE t (at serial_number, note TEXT);\n");
        assert!(
            left_over.is_empty(),
            "`serial_number` is a type name, not a serial column: {left_over:?}"
        );

        // **And the other reader's three arms, decided here.**
        // `forbids_null` reads an `ADD COLUMN`, and with only its `NOT NULL` arm
        // fixtured, `PRIMARY KEY`, `IDENTITY` and the serial list each
        // survive deletion, and all three together leave `check passed` with a
        // transcript byte-identical to a clean run. That is the *too strong*
        // direction the sibling above holds, unheld — a migration
        // adding `recorded BIGSERIAL` would be refused with advice about a file
        // that has already been applied.
        let added = |sql: &str| {
            let statements = super::statements(sql);
            let statement = statements.first().expect("one statement");
            let at = statement
                .tokens
                .iter()
                .position(|token| token.is("COLUMN"))
                .expect("the statement adds a column");
            super::forbids_null(&statement.tokens, at)
        };
        for declared in [
            "ALTER TABLE t ADD COLUMN at BIGINT NOT NULL;\n",
            "ALTER TABLE t ADD COLUMN at BIGINT PRIMARY KEY;\n",
            "ALTER TABLE t ADD COLUMN at BIGINT GENERATED BY DEFAULT AS IDENTITY;\n",
            "ALTER TABLE t ADD COLUMN at BIGSERIAL;\n",
            "ALTER TABLE t ADD COLUMN at smallserial;\n",
        ] {
            assert!(
                added(declared),
                "`{declared}` adds a column that cannot be null"
            );
        }
        for declared in [
            "ALTER TABLE t ADD COLUMN at BIGINT;\n",
            "ALTER TABLE t ADD COLUMN at TEXT DEFAULT 'x';\n",
            "ALTER TABLE t ADD COLUMN at serial_number;\n",
        ] {
            assert!(
                !added(declared),
                "`{declared}` adds a column that may be null"
            );
        }
        // A table-level key names columns declared above it.
        assert_eq!(
            read("CREATE TABLE t (a TEXT, b TEXT, PRIMARY KEY (a, b));\n"),
            vec!["a".to_owned(), "b".to_owned()]
        );
        // `NULL` written out is the default and says the opposite.
        assert_eq!(
            read("CREATE TABLE t (at BIGINT NULL);\n"),
            Vec::<String>::new()
        );
        // A `CHECK` mentioning `NOT NULL` belongs to no column's definition.
        assert_eq!(
            read("CREATE TABLE t (at BIGINT, CHECK (at IS NOT NULL));\n"),
            Vec::<String>::new()
        );
        // A default does not make a column non-null either.
        assert_eq!(
            read("CREATE TABLE t (at BIGINT DEFAULT 0);\n"),
            Vec::<String>::new()
        );
    }

    /// A table can leave the census the same ways it can enter it.
    ///
    /// **A table created in a routine body can leave it the same two ways.**
    /// `created_tables` scans every offset, so a `CREATE
    /// TABLE` written as a body's first statement is seen; a `renamed_table` and
    /// `dropped_tables` asking for their verb at token zero, with a body cut on
    /// `;`, are handed a statement that begins `BEGIN`, and both return
    /// at the first token. Measured, each on its own:
    ///
    /// ```text
    /// $$ BEGIN ALTER TABLE access_records RENAME TO login_attempts; END $$
    ///   -> renamed = []   the entry goes on bounding a table that is gone
    /// $$ BEGIN DROP TABLE access_records; END $$
    ///   -> dropped = []   the same, and nothing takes its place
    /// $$ BEGIN CREATE TABLE ghost (id INT); END $$
    ///   -> created = [ghost]
    /// ```
    ///
    /// A census that can only go up is not a census: it drifts one way, and the
    /// number nobody reads is the only thing that says so.
    ///
    /// **And the two kinds beside `TABLE`.** A materialised view stores rows and
    /// a foreign table is declared here; both are counted by `CREATE`, and a
    /// reader of tables alone renames or drops neither out of the count.
    #[test]
    fn a_table_leaves_the_census_however_it_is_written() {
        for (body, from, to) in [
            (
                "CREATE FUNCTION f() RETURNS void AS $$ BEGIN \
                 ALTER TABLE access_records RENAME TO login_attempts; END $$ LANGUAGE plpgsql;\n",
                "access_records",
                "login_attempts",
            ),
            (
                "CREATE FUNCTION f() RETURNS void AS $$ BEGIN \
                 ALTER TABLE access_records SET SCHEMA archive; END $$ LANGUAGE plpgsql;\n",
                "access_records",
                "archive.access_records",
            ),
            // The kinds beside `TABLE`, at the top level.
            (
                "ALTER MATERIALIZED VIEW recent_access RENAME TO older_access;\n",
                "recent_access",
                "older_access",
            ),
            (
                "ALTER FOREIGN TABLE remote_rows RENAME TO remote_records;\n",
                "remote_rows",
                "remote_records",
            ),
        ] {
            let changes = changes_in(body);
            assert_eq!(changes.renamed.len(), 1, "{body}: {changes:#?}");
            assert_eq!(changes.renamed[0].from, from, "{body}");
            assert_eq!(changes.renamed[0].to, to, "{body}");
        }
        // A drop retires the name and nothing takes its place, which is `to`
        // empty — the same three shapes.
        for (body, gone) in [
            (
                "CREATE FUNCTION f() RETURNS void AS $$ BEGIN \
                 DROP TABLE access_records; END $$ LANGUAGE plpgsql;\n",
                "access_records",
            ),
            ("DROP MATERIALIZED VIEW recent_access;\n", "recent_access"),
            ("DROP FOREIGN TABLE remote_rows;\n", "remote_rows"),
        ] {
            let changes = changes_in(body);
            assert_eq!(changes.renamed.len(), 1, "{body}: {changes:#?}");
            assert_eq!(changes.renamed[0].from, gone, "{body}");
            assert!(changes.renamed[0].to.is_empty(), "{body}");
        }
        // **And a verb written inside a statement is not the statement's
        // verb**, which is what keeps the offset walk from reading a clause as
        // a command. `ALTER TABLE t DROP COLUMN c` drops a column, and an
        // ordinary view is not a table at all.
        for still_there in [
            "ALTER TABLE access_records DROP COLUMN recorded;\n",
            "ALTER TABLE access_records DROP CONSTRAINT access_records_pkey;\n",
            "DROP VIEW recent_access;\n",
            "DROP INDEX access_records_recorded_idx;\n",
            "ALTER VIEW recent_access RENAME TO older_access;\n",
        ] {
            let changes = changes_in(still_there);
            assert!(
                changes.renamed.is_empty(),
                "`{still_there}` retires no table and was read as retiring one: {changes:#?}"
            );
        }
    }

    /// Two statements on one line run in the order they are written.
    ///
    /// **The line is not a total order, and a tie on it goes the wrong way.** `cut`
    /// finishes the top level and then re-reads the routine bodies, so a stable
    /// sort on the line puts every top-level statement before every nested one
    /// that shares it. Measured, one line of ordinary SQL:
    ///
    /// ```text
    /// DO $$ BEGIN DROP TABLE t; END $$; CREATE TABLE t (id INT);
    ///   -> created at 1, dropped at 2
    /// ```
    ///
    /// The table is created and then read as dropped, so `[tables.t]` is
    /// reported as bounding a table that is gone — a finding about a table that
    /// is there, which this file's own header calls the one thing this gate must
    /// not produce.
    ///
    /// Both directions, because "the nested one first" is the same defect
    /// mirrored: written after the body, the drop really does come last.
    #[test]
    fn two_statements_on_one_line_run_in_the_order_they_are_written() {
        let dropped_first =
            changes_in("DO $$ BEGIN DROP TABLE t; END $$; CREATE TABLE t (id INT);\n");
        assert_eq!(dropped_first.created.len(), 1, "{dropped_first:#?}");
        assert_eq!(dropped_first.renamed.len(), 1, "{dropped_first:#?}");
        assert!(
            dropped_first.renamed[0].at < dropped_first.created[0].at,
            "the body drops the table and the line then creates it, so the drop runs \
             first: {dropped_first:#?}"
        );
        let created_first =
            changes_in("CREATE TABLE t (id INT); DO $$ BEGIN DROP TABLE t; END $$;\n");
        assert_eq!(created_first.created.len(), 1, "{created_first:#?}");
        assert_eq!(created_first.renamed.len(), 1, "{created_first:#?}");
        assert!(
            created_first.created[0].at < created_first.renamed[0].at,
            "the line creates the table and the body then drops it, so the create runs \
             first: {created_first:#?}"
        );
    }

    #[test]
    fn a_rename_creates_one_table_and_retires_another() {
        // After this the bounded entry names a table that no longer exists and
        // the table now holding the rows is unbounded. Unread, neither direction
        // of the gate notices.
        let changes = changes_in("ALTER TABLE access_records RENAME TO login_attempts;\n");
        assert_eq!(changes.renamed.len(), 1, "{changes:#?}");
        assert_eq!(changes.renamed[0].from, "access_records");
        assert_eq!(changes.renamed[0].to, "login_attempts");
        // A column rename is not a table rename.
        let column = changes_in("ALTER TABLE access_records RENAME COLUMN a TO b;\n");
        assert!(column.renamed.is_empty(), "{column:#?}");

        // **And `ONLY`, which `pg_dump` writes and a naive reader takes for the
        // name.** Under `RENAME TO` the retired name is then `ONLY`, so nothing
        // is retired and `catalog/limits.toml` goes on bounding a table that is
        // gone; under `SET SCHEMA` the finding names `archive.ONLY` — a table
        // nobody wrote — while `archive.access_records` is unbounded and unnamed. Both
        // statements are ordinary PostgreSQL.
        for shape in [
            "ALTER TABLE ONLY access_records RENAME TO login_attempts;\n",
            // The grammar is `ALTER TABLE [ IF EXISTS ] [ ONLY ] name`, in that
            // order — verified against PostgreSQL 18.6, where `ONLY IF EXISTS`
            // is a syntax error.
            "ALTER TABLE IF EXISTS ONLY access_records RENAME TO login_attempts;\n",
        ] {
            let only = changes_in(shape);
            assert_eq!(only.renamed.len(), 1, "{shape}: {only:#?}");
            assert_eq!(only.renamed[0].from, "access_records", "{shape}");
            assert_eq!(only.renamed[0].to, "login_attempts", "{shape}");
        }
        let moved = changes_in("ALTER TABLE ONLY access_records SET SCHEMA archive;\n");
        assert_eq!(moved.renamed.len(), 1, "{moved:#?}");
        assert_eq!(moved.renamed[0].from, "access_records");
        assert_eq!(
            moved.renamed[0].to, "archive.access_records",
            "the table that now holds the rows is not the one named"
        );

        // **And a schema or table spelled with the verb's own word.** `SET` and
        // `RENAME` are non-reserved keywords, so `CREATE SCHEMA set` and
        // `CREATE TABLE rename` are legal PostgreSQL; with the verb looked for
        // from token zero, the name's own word arrives first and the whole
        // rename is dropped — `catalog/limits.toml` goes on bounding a table
        // that is gone while the one holding the rows is unbounded and unnamed.
        for (shape, from, to) in [
            ("ALTER TABLE set.t RENAME TO landed;\n", "set.t", "landed"),
            // **And the table, not only the schema.** With the verb looked for
            // after the name, a schema called `set` is stepped over by the
            // search whichever end index the name walk returns — it is the
            // *table* whose word is the verb's that says where the name ended.
            ("ALTER TABLE t.set RENAME TO landed;\n", "t.set", "landed"),
            (
                "ALTER TABLE rename.t RENAME TO landed;\n",
                "rename.t",
                "landed",
            ),
            (
                "ALTER TABLE set.t SET SCHEMA archive;\n",
                "set.t",
                "archive.t",
            ),
            ("ALTER TABLE t RENAME TO set.u;\n", "t", "set.u"),
        ] {
            let named = changes_in(shape);
            assert_eq!(named.renamed.len(), 1, "{shape}: {named:#?}");
            assert_eq!(named.renamed[0].from, from, "{shape}");
            assert_eq!(named.renamed[0].to, to, "{shape}");
        }

        // **The schema a table moves to folds like every other unquoted name.**
        // Read with `name`, which returns the text as written,
        // `SET SCHEMA Archive` would credit the rows to `Archive.t` — an entry the
        // pruner can never match, since the database made the schema `archive`.
        let folded = changes_in("ALTER TABLE t SET SCHEMA Archive;\n");
        assert_eq!(folded.renamed.len(), 1, "{folded:#?}");
        assert_eq!(folded.renamed[0].to, "archive.t");

        // **A quoted name holding a dot moves whole.** Built by splitting the old
        // name's text on `.` and keeping the tail, the new name of a table
        // PostgreSQL calls `a.b` is `archive.b` — a table nobody wrote, which is
        // the one thing this gate must not say.
        let dotted = changes_in("ALTER TABLE \"a.b\" SET SCHEMA archive;\n");
        assert_eq!(dotted.renamed.len(), 1, "{dotted:#?}");
        assert_eq!(dotted.renamed[0].to, "archive.a.b");
    }

    /// A SQLite trigger's body is one statement, and so is a `CASE` inside it.
    ///
    /// **The rule that differs between the two directories.** PostgreSQL has no
    /// `BEGIN … END` trigger body — its triggers call a function — so the same
    /// text cut as PostgreSQL is two statements plus a stray `END`, and the
    /// table after it is where the count says it is only when the cutter knows
    /// the dialect.
    #[test]
    fn a_trigger_body_is_one_statement_in_sqlite_and_not_in_postgresql() {
        let sql = "CREATE TABLE a (id INTEGER NOT NULL);\n\
                   CREATE TRIGGER a_is_never_edited BEFORE UPDATE ON a\n\
                   BEGIN\n\
                   \x20   SELECT CASE WHEN 1 THEN RAISE(ABORT, 'one; two') END;\n\
                   \x20   SELECT RAISE(ABORT, 'never');\n\
                   END;\n\
                   CREATE TABLE b (id INTEGER NOT NULL);\n";
        let sqlite = super::statements_in(sql, super::Dialect::Sqlite);
        assert_eq!(sqlite.len(), 3, "the trigger was cut inside its body");
        assert!(
            sqlite[1]
                .tokens
                .first()
                .is_some_and(|token| token.is("CREATE"))
        );
        assert!(
            sqlite[1].tokens.last().is_some_and(|token| token.is("END")),
            "the trigger does not end at its own `END`"
        );
        let names: Vec<String> = super::changes_in_as(sql, super::Dialect::Sqlite)
            .created
            .into_iter()
            .map(|created| created.name)
            .collect();
        assert_eq!(names, ["a", "b"]);
        // The same text as PostgreSQL: the body's `;` ends a statement.
        assert!(
            statements(sql).len() > 3,
            "the dialects cut alike, so neither rule is tested"
        );
        // And a `TEMP` trigger is one too.
        let temporary = "CREATE TEMP TRIGGER t AFTER INSERT ON a BEGIN SELECT 1; SELECT 2; END;";
        assert_eq!(
            super::statements_in(temporary, super::Dialect::Sqlite).len(),
            1
        );
    }

    /// A table is bounded only by an entry of its own database, and the finding
    /// says which database the entry names.
    #[test]
    fn a_table_bounded_in_the_other_database_is_named_as_such() {
        let held = crate::check::LeavesNothingBehind::new("limits-databases");
        let scratch = held.root.clone();
        let server = nc_catalog::TABLES
            .iter()
            .find(|table| table.database == nc_catalog::Database::Server)
            .expect("the catalog bounds a server table");
        let local = nc_catalog::TABLES
            .iter()
            .find(|table| table.database == nc_catalog::Database::LocalFile)
            .expect("the catalog bounds a local table");
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        std::fs::create_dir_all(scratch.join(super::LOCAL_MIGRATIONS)).expect("writable");
        // Each directory creates the other's table.
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            format!("CREATE TABLE {} (id INTEGER NOT NULL);\n", local.name),
        )
        .expect("writable");
        std::fs::write(
            scratch.join(super::LOCAL_MIGRATIONS).join("0001_x.sql"),
            format!("CREATE TABLE {} (id INTEGER NOT NULL);\n", server.name),
        )
        .expect("writable");
        let (findings, seen, _, files) = every_table_declares_a_bound(&scratch).expect("readable");
        assert_eq!((seen, files), (2, 2), "both directories were read");
        for (table, directory, says) in [
            (local.name, "migrations/", "local"),
            (server.name, "local-migrations/", "server"),
        ] {
            let what = format!(
                "`{table}` is created in {directory} and its entry in catalog/limits.toml says \
                 `database = \"{says}\"`"
            );
            assert!(
                findings.iter().any(|finding| finding.what == what),
                "{what}\nwas not said: {findings:#?}"
            );
            assert!(
                findings.iter().any(|finding| finding.what
                    == format!("`{table}` is bounded and nothing creates it")),
                "`{table}` has no table in its own database's directory, and nothing said so"
            );
        }
    }

    /// A table `local-migrations/` creates is seen however SQLite lets it be
    /// written, and a virtual table is refused with a finding of its own.
    ///
    /// **Three shapes a PostgreSQL reading walks past**, measured through this same
    /// function: `CREATE VIRTUAL TABLE notes_search USING fts5(body)`,
    /// `CREATE TABLE [bracketed] (…)` and ``CREATE TABLE `backticked` (…)``
    /// each give `seen 0` and no finding, while the plain and `"quoted"`
    /// controls are named as unbounded — so a local migration in any of the
    /// three passes `record limits` with a table nothing bounds.
    #[test]
    fn a_sqlite_table_is_seen_however_sqlite_lets_it_be_written() {
        for (written, named, seen) in [
            (
                "CREATE TABLE plain (id INTEGER NOT NULL);",
                Some("plain"),
                1,
            ),
            (
                "CREATE TABLE \"quoted\" (id INTEGER NOT NULL);",
                Some("quoted"),
                1,
            ),
            (
                "CREATE TABLE [bracketed] (id INTEGER NOT NULL);",
                Some("bracketed"),
                1,
            ),
            (
                "CREATE TABLE `backticked` (id INTEGER NOT NULL);",
                Some("backticked"),
                1,
            ),
            (
                "CREATE TABLE `back``tick` (id INTEGER NOT NULL);",
                Some("back`tick"),
                1,
            ),
            (
                "CREATE TABLE IF NOT EXISTS [with space] (id INTEGER NOT NULL);",
                Some("with space"),
                1,
            ),
            (
                "CREATE VIRTUAL TABLE notes_search USING fts5(body);",
                None,
                0,
            ),
            (
                "CREATE VIRTUAL TABLE IF NOT EXISTS main.notes_search USING fts5(body);",
                None,
                0,
            ),
        ] {
            let held = crate::check::LeavesNothingBehind::new("limits-sqlite-shapes");
            let scratch = held.root.clone();
            std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
            std::fs::create_dir_all(scratch.join(super::LOCAL_MIGRATIONS)).expect("writable");
            std::fs::write(
                scratch.join(super::LOCAL_MIGRATIONS).join("0001_x.sql"),
                format!("{written}\n"),
            )
            .expect("writable");
            let (findings, counted, _, files) =
                every_table_declares_a_bound(&scratch).expect("readable");
            assert_eq!((counted, files), (seen, 1), "{written}: {findings:#?}");
            if let Some(name) = named {
                assert!(
                    findings.iter().any(|finding| finding.what
                        == format!("`{name}` has no entry in catalog/limits.toml")),
                    "{written}: `{name}` was not named: {findings:#?}"
                );
            } else {
                assert!(
                    findings.iter().any(|finding| finding.what
                        == "`CREATE VIRTUAL TABLE` is in a migration"
                        && finding.line == 1),
                    "{written}: the virtual table was not refused: {findings:#?}"
                );
                assert!(
                    !findings
                        .iter()
                        .any(|finding| finding.what.contains("has no entry")),
                    "{written}: a virtual table is refused, not counted: {findings:#?}"
                );
            }
        }
    }

    #[test]
    fn a_semicolon_inside_a_quoted_name_does_not_cut_the_statement() {
        // Cut at the `;`, the finding names `` `a;` `` — a table nobody wrote,
        // which is criterion 5's second half failing in the one way it must not.
        assert_eq!(
            caught("CREATE TABLE \"a;b\" (id INT);\n"),
            vec!["a;b".to_owned()]
        );
        assert_eq!(
            caught("CREATE TABLE \"a\"\"b\" (id INT);\n"),
            vec!["a\"b".to_owned()],
            "a doubled quote escapes itself"
        );
    }

    #[test]
    fn a_schema_other_than_public_is_a_different_table() {
        // Credited to `[tables.access_records]`, `archive.access_records` would
        // be pruned by `DELETE FROM access_records` — a different table. An
        // entry cannot bound a table it does not name.
        assert_eq!(
            caught("CREATE TABLE archive.access_records (id INT);"),
            vec!["archive.access_records".to_owned()]
        );
        assert_eq!(
            caught("CREATE TABLE public.access_records (id INT);"),
            vec!["access_records".to_owned()]
        );
        assert_eq!(
            caught("CREATE TABLE access_records (id INT);"),
            vec!["access_records".to_owned()]
        );
        // **A three-part name**, which PostgreSQL accepts for the current
        // database. Split on dots, it yields nothing at all, so the table is never
        // seen and `seen` is not even incremented.
        assert_eq!(
            caught("CREATE TABLE noctua.public.login_attempts (id INT);"),
            vec!["login_attempts".to_owned()]
        );
        // **And a dot *inside* a quoted name is not a qualifier.** Splitting the
        // text on `.` reads `"public.access_records"` — one name — as two parts
        // and credits it to the entry for a table it is not.
        assert_eq!(
            caught("CREATE TABLE \"public.access_records\" (id INT);"),
            vec!["public.access_records".to_owned()]
        );
        // **A schema element's schema is folded like any other unquoted name.**
        // Read verbatim while every other name here is resolved, the schema
        // would make this `Archive.access_records` — which matches no
        // `[tables.…]` entry, so the table reads as unbounded, and which names
        // in the finding a schema `psql` will not show the developer. The
        // quoted form is the control: it is not folded, and the two must differ.
        assert_eq!(
            caught("CREATE SCHEMA Archive CREATE TABLE access_records (id INT);"),
            vec!["archive.access_records".to_owned()]
        );
        assert_eq!(
            caught("CREATE SCHEMA \"Archive\" CREATE TABLE access_records (id INT);"),
            vec!["Archive.access_records".to_owned()]
        );
        // And the owner spelling, where the schema name is not at index 2.
        assert_eq!(
            caught("CREATE SCHEMA AUTHORIZATION Noctua CREATE TABLE t (id INT);"),
            vec!["noctua.t".to_owned()]
        );
    }

    #[test]
    fn a_quoted_name_survives_whitespace_and_reserved_words() {
        // **Flattening on whitespace truncates the name.**
        // `CREATE TABLE "login attempts"` is then reported as `` `login` `` — a
        // table nobody wrote — and `CREATE TABLE "access_records archive"` is
        // truncated to `access_records`, which the catalog declares, so an
        // unbounded table walks straight through.
        assert_eq!(
            caught("CREATE TABLE \"login attempts\" (id INT);"),
            vec!["login attempts".to_owned()]
        );
        assert_eq!(
            caught("CREATE TABLE \"access_records archive\" (id INT);"),
            vec!["access_records archive".to_owned()]
        );
        // A quoted word is a name, never a keyword: this creates a table called
        // `TABLE`.
        assert_eq!(
            caught("CREATE TABLE \"TABLE\" (id INT);"),
            vec!["TABLE".to_owned()]
        );
    }

    #[test]
    fn one_statement_may_declare_more_than_one_table() {
        // `CREATE SCHEMA` takes schema elements, which is valid PostgreSQL, and
        // a reader of the first alone sees one — so a `nc:bounded:` marker above
        // it hides both.
        let sql = "CREATE SCHEMA hollywood\n                       CREATE TABLE install_marker (id BIGINT PRIMARY KEY)\n                       CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n";
        assert_eq!(
            caught(sql),
            vec![
                "hollywood.install_marker".to_owned(),
                "hollywood.login_attempts".to_owned()
            ],
            "a schema element is a table and it belongs to the schema it is in"
        );
    }

    /// A disagreement carried out of the walk stops it, rather than arriving as
    /// a finding.
    ///
    /// **The `?`, decided here.** `let _ = the_readers_agree(…)` fails this
    /// test, and nothing a clean run prints would change, under a paragraph in
    /// the walk
    /// saying *"a finding can be filtered — this cannot, and it stops the walk
    /// where it is"*. The function has a unit test; carrying its verdict out is
    /// this one, and the three shapes that paragraph lists make it a tripwire
    /// for a real failure and not a hypothetical one.
    ///
    /// A disagreement cannot be written into a file: both readers fingerprint
    /// the same statements with the same tokeniser, which is exactly why a
    /// difference means one of them was given less than the whole text. So it is
    /// injected through the door beside it, and what is watched is that the walk
    /// returns `Err` rather than a finding a caller could filter.
    #[test]
    fn a_forced_disagreement_stops_the_walk_rather_than_becoming_a_finding() {
        let held = crate::check::LeavesNothingBehind::new("tripwire");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            "CREATE TABLE journal_entries (id TEXT PRIMARY KEY, recorded BIGINT NOT NULL);\n",
        )
        .expect("writable");

        every_table_declares_a_bound(&scratch).expect("the fixture reads without the door");

        super::disagree_once_for_tests();
        let stopped = every_table_declares_a_bound(&scratch)
            .expect_err("the two readers disagreed and the walk carried on");
        assert!(
            stopped.contains("was read twice and the two readers disagree"),
            "the walk stopped for some other reason: {stopped}"
        );

        // And the door is one file wide, so the next walk is the walk again.
        every_table_declares_a_bound(&scratch).expect("the door did not close behind itself");
    }

    #[test]
    fn a_marker_excuses_one_table_and_not_a_statement_full_of_them() {
        // `shares_its_line` catches two *statements* on one line. This is one
        // statement declaring two tables, so the marker cannot say which it
        // meant — and excusing both lets an unbounded table ride in
        // beside a genuinely bounded one.
        let held = crate::check::LeavesNothingBehind::new("limits-rename");
        let scratch = held.root.clone();
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            format!(
                "-- {BOUNDED} `install_marker` holds exactly one row, written by the installer\n\
                 CREATE SCHEMA hollywood\n    \
                 CREATE TABLE install_marker (id BIGINT PRIMARY KEY)\n    \
                 CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"
            ),
        )
        .expect("writable");
        let (findings, seen, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert_eq!(seen, 2);
        for table in ["hollywood.install_marker", "hollywood.login_attempts"] {
            assert!(
                findings.iter().any(|finding| finding.what.contains(table)),
                "one marker excused a statement declaring two tables: {findings:#?}"
            );
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn an_ordinary_literal_ending_in_a_backslash_does_not_run_past_its_own_end() {
        // **`standard_conforming_strings` is on by default**, so `'C:\'` is a
        // *closed* string holding a backslash. Treating `\` as an escape in
        // every literal makes the scanner run past the end of that one, find no
        // further quote, and blank the rest of the file — the exact
        // desynchronisation the `E'…'` handling exists to prevent, arriving from
        // the other side.
        let sql = "COMMENT ON TABLE access_records IS 'device paths look like C:\\';\n\n                   CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n";
        assert_eq!(caught(sql), vec!["login_attempts".to_owned()]);
        // And the `E` form, where a backslash *is* an escape, still works.
        let escaped = "COMMENT ON TABLE t IS E'don\\'t /* see the trigger';\n                       CREATE TABLE login_attempts (id TEXT);\n";
        assert_eq!(caught(escaped), vec!["login_attempts".to_owned()]);
    }

    #[test]
    fn a_table_that_moves_or_is_dropped_stops_existing() {
        // `ALTER TABLE … SET SCHEMA` retires a name as surely as a rename, and
        // so does `DROP TABLE` — and unread, neither direction of the gate sees
        // either, so a bounded entry can end up naming a table that does not exist.
        let moved = changes_in("ALTER TABLE access_records SET SCHEMA archive;\n");
        assert_eq!(moved.renamed.len(), 1, "{moved:#?}");
        assert_eq!(moved.renamed[0].from, "access_records");
        assert_eq!(moved.renamed[0].to, "archive.access_records");

        let dropped = changes_in("DROP TABLE access_records;\n");
        assert_eq!(dropped.renamed.len(), 1, "{dropped:#?}");
        assert_eq!(dropped.renamed[0].from, "access_records");
        let left_over = &dropped.renamed[0].to;
        assert!(
            left_over.is_empty(),
            "a drop replaces nothing: {left_over:?}"
        );
    }

    #[test]
    fn a_dollar_quoted_document_is_data_and_not_sql() {
        // Scanning every dollar-quoted run as SQL makes an `INSERT` of a
        // document containing the words `CREATE TABLE widgets` fail the build
        // naming a table nobody wrote — the one thing this gate must not do.
        let sql = "INSERT INTO journal_entries (resource) VALUES ($doc$\n                   The change we are about to make: CREATE TABLE widgets (id INT);\n                   $doc$);\n";
        assert!(caught(sql).is_empty(), "{:?}", caught(sql));
        // And a routine body still is SQL.
        let routine = "DO $$ BEGIN CREATE TABLE login_attempts (id INT); END $$;\n";
        assert_eq!(caught(routine), vec!["login_attempts".to_owned()]);
    }

    #[test]
    fn a_marker_excuses_one_statement_and_not_the_line() {
        // With detection statement-scoped and the hatch line-scoped, one reason
        // excuses two tables.
        let scratch = std::env::temp_dir().join(format!("nc-limits-line-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            format!(
                "-- {BOUNDED} `install_marker` holds exactly one row, written by the installer\n\
                 CREATE TABLE install_marker (id INT); CREATE TABLE login_attempts (id TEXT);\n"
            ),
        )
        .expect("writable");
        let (findings, seen, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert_eq!(seen, 2);
        assert!(
            findings.iter().any(|f| f.what.contains("login_attempts")),
            "one reason excused two tables: {findings:#?}"
        );
        assert!(
            !findings.iter().any(|f| f.what.contains("install_marker")),
            "the marker stopped excusing the statement it names: {findings:#?}"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// And the same, on the path a *rename* takes.
    ///
    /// **The clause is written twice, and this decides the second copy.**
    /// `alone: !rename.shares_its_line && rename.alone_in_its_statement`
    /// replaced by `alone: true` fails this test — while `mutants.py`'s operator
    /// *"a second statement on the line is not excused"* rewrites the
    /// **create** path thirty-three rows above and reports `killed`, so without
    /// this the run would print a verdict reading *this rule is decided* over a
    /// copy decided by nothing. That is this tree's most-caught defect shape, in the
    /// runner's own output.
    ///
    /// The discriminating case is the *second* statement on the line: a comment
    /// above a line belongs to what that line opens with, so a marker naming the
    /// second rename is a marker about a statement it does not sit above.
    /// `shares_its_line` is what refuses it, and the first rename on the line is
    /// deliberately still excusable — `a_marker_excuses_one_statement_and_not_the_line`
    /// asserts that same asymmetry on the create path.
    #[test]
    fn a_marker_excuses_one_rename_and_not_the_line() {
        let held = crate::check::LeavesNothingBehind::new("limits-rename");
        let scratch = held.root.clone();
        let run = |sql: &str| {
            std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
            std::fs::write(scratch.join("migrations/0001_x.sql"), sql).expect("writable");
            every_table_declares_a_bound(&scratch).expect("readable").0
        };
        let two_on_one_line = format!(
            "CREATE TABLE first_source (id INT);\n\
             CREATE TABLE second_source (id INT);\n\
             -- {BOUNDED} `second_landing` holds exactly one row, written by the installer\n\
             ALTER TABLE first_source RENAME TO first_landing; \
             ALTER TABLE second_source RENAME TO second_landing;\n\
             -- {BOUNDED} `first_landing` holds exactly one row, written by the installer\n\
             -- {BOUNDED} `first_source` is the name before the rename above\n\
             -- {BOUNDED} `second_source` is the name before the rename above\n"
        );
        let findings = run(&two_on_one_line);
        assert!(
            findings.iter().any(|f| f.what.contains("second_landing")),
            "a marker above a line excused the *second* rename on it, so a table that \
             acquired a new name kept an excuse written above a different statement: \
             {findings:#?}"
        );

        // **And the shape the tree could legitimately write**, which is the
        // other way to get this wrong: one rename on its own line, with a marker
        // naming the name it lands on.
        let one_on_its_own_line = format!(
            "CREATE TABLE first_source (id INT);\n\
             -- {BOUNDED} `first_landing` holds exactly one row, written by the installer\n\
             ALTER TABLE first_source RENAME TO first_landing;\n\
             -- {BOUNDED} `first_source` is the name before the rename above\n"
        );
        let findings = run(&one_on_its_own_line);
        assert!(
            !findings.iter().any(|f| f.what.contains("first_landing")),
            "a rename alone on its line, with a marker naming what it lands on, was \
             refused — the rule is about a line holding two statements: {findings:#?}"
        );
    }

    #[test]
    fn a_dollar_tag_that_is_a_parameter_is_not_a_quote() {
        // `$1$` is a parameter followed by a `$`. Reading it as an opening quote
        // would swallow the rest of the file.
        assert_eq!(
            caught("SELECT $1$x;\nCREATE TABLE widgets (id INT);\n"),
            vec!["widgets".to_owned()]
        );
        // And a real tag with a digit after its first character still works.
        let tagged = "CREATE FUNCTION f() RETURNS void AS $body1$ BEGIN END; $body1$ \
                      LANGUAGE plpgsql;\nCREATE TABLE widgets (id INT);\n";
        assert_eq!(caught(tagged), vec!["widgets".to_owned()]);
    }

    /// A migration that runs no statement is reported, not counted as fine.
    ///
    /// **The guard, held by its behaviour and not only by its sentence.**
    /// Switched off — `if false` — `if changes.statements == 0` fails this
    /// test; the only other reader of `runs_nothing` is the finding-message
    /// census, which pins the *string*. A census sees that a sentence exists; it
    /// cannot see whether anything ever says it.
    ///
    /// The defect it exists for is recorded on the function itself:
    /// `0003_sequence_is_not_negative.sql` truncated to its comment header still
    /// reads `numbered once each and in order` to the numbering gate.
    #[test]
    fn a_migration_that_runs_nothing_is_a_finding() {
        let held = crate::check::LeavesNothingBehind::new("limits-again");
        let scratch = held.root.clone();
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        // A file truncated to its header: numbered, readable, and doing nothing.
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            "-- the statements this file used to run\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("runs no statement")),
            "a migration holding only a comment was read and nothing said so: {findings:#?}"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_bound_on_a_table_nothing_creates_is_a_finding() {
        use std::fmt::Write as _;
        // **The reverse gate's own test.** Deleting the reverse gate whole fails
        // it. Its own comment argues that a
        // limit on a table that does not exist "reads as protection and is
        // not" — and this is what makes that argument true.
        let scratch = std::env::temp_dir().join(format!("nc-limits-rev-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        // **And an empty `local-migrations/`**, so the local database's
        // entries are held to a directory that creates none of them too — the
        // reverse gate runs once per directory, and an absent one is not read.
        std::fs::create_dir_all(scratch.join(super::LOCAL_MIGRATIONS)).expect("writable");
        // A migrations directory that creates nothing the catalog bounds.
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            format!("-- {BOUNDED} `marker` holds one row, written at install\nCREATE TABLE marker (id INT);\n"),
        )
        .expect("writable");
        // **The scratch gets a real `catalog/limits.toml`.** Without one,
        // `limits` is `""` and `line_of_entry("", name)` returns 1 — the very
        // answer a broken `line_of_entry` gives — so replacing `line_of_entry`
        // with a literal `1` would leave this test green. A finding's line is part of the
        // finding, and a test that reads only `what` cannot see it.
        std::fs::create_dir_all(scratch.join("catalog")).expect("writable");
        let mut limits = String::from("# a scratch catalog, one entry per bounded table\n");
        let mut header_line = std::collections::BTreeMap::new();
        for table in nc_catalog::TABLES {
            limits.push('\n');
            header_line.insert(table.name, limits.lines().count() + 1);
            writeln!(
                limits,
                "[tables.{}]\nstrategy = \"oldest-first\"\nmax_rows = 1",
                table.name
            )
            .expect("a String never fails to grow");
        }
        std::fs::write(scratch.join("catalog/limits.toml"), &limits).expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        for table in nc_catalog::TABLES {
            let what = format!("`{}` is bounded and nothing creates it", table.name);
            let finding = findings
                .iter()
                .find(|finding| finding.what == what)
                .unwrap_or_else(|| {
                    panic!(
                        "`{}` is bounded and no migration creates it, and nothing said so: \
                         {findings:#?}",
                        table.name
                    )
                });
            let at = header_line[table.name];
            assert_eq!(
                finding.line, at,
                "the finding for `{}` points at line {}, and `[tables.{}]` is written at \
                 line {at}. A located finding that points at the file's first line is the \
                 defect this asserts against",
                table.name, finding.line, table.name
            );
        }
        assert!(
            !nc_catalog::TABLES.is_empty(),
            "no table is bounded, so this proves nothing"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A rename creates a table and retires one, and both halves are judged.
    ///
    /// **Both branches, run here.** No migration in this tree renames anything,
    /// so a run of the gate never reaches the rule that tracks a rename, and
    /// deleting it fails this test. Its own comment records what it exists for:
    /// `ALTER TABLE access_records RENAME TO login_attempts` leaves the bounded
    /// entry naming a table that no longer exists, and the table that now holds
    /// the rows unbounded.
    ///
    /// Both of those are asserted here, which needs a table the shipped catalog
    /// actually bounds — the retire half is only visible when something was
    /// counting on the old name.
    #[test]
    fn a_renamed_table_is_a_table_created_and_a_table_retired() {
        use std::fmt::Write as _;
        let bounded = nc_catalog::TABLES
            .first()
            .expect("the catalog bounds at least one table")
            .name;
        let scratch = std::env::temp_dir().join(format!("nc-limits-ren-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        // Every other bounded table created and left alone, so the only thing
        // this test changes about the answer is the rename.
        let mut made = String::new();
        // The server's tables: a local one created here is in the wrong
        // directory, which is a finding of its own and not this test's.
        for table in nc_catalog::TABLES
            .iter()
            .filter(|table| table.database == nc_catalog::Database::Server)
        {
            let _ = writeln!(made, "CREATE TABLE {} (id INT);", table.name);
        }
        std::fs::write(scratch.join("migrations/0001_make.sql"), &made).expect("writable");
        std::fs::write(
            scratch.join("migrations/0002_rename.sql"),
            format!("ALTER TABLE {bounded} RENAME TO landed;\n"),
        )
        .expect("writable");

        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        let said = |what: &str| findings.iter().any(|finding| finding.what == what);
        assert!(
            said("`landed` has no entry in catalog/limits.toml"),
            "the table the rows are in now is unbounded and nothing said so: {findings:#?}"
        );
        assert!(
            said(&format!("`{bounded}` is bounded and nothing creates it")),
            "`{bounded}` was renamed away and its entry now bounds nothing, and nothing \
             said so: {findings:#?}"
        );

        // The floor, and it is what makes both assertions about the *rename*:
        // without the second migration neither sentence is said.
        std::fs::remove_file(scratch.join("migrations/0002_rename.sql")).expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            !findings
                .iter()
                .any(|finding| finding.what.contains("landed")
                    || finding.what == format!("`{bounded}` is bounded and nothing creates it")),
            "neither sentence is about the rename: {findings:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A table dropped and re-created in one migration still exists at the end
    /// of it.
    ///
    /// **The refusal of something legitimate, which is the harder half.** A
    /// walk reading every `CREATE TABLE` in a file before any `DROP` makes a drop
    /// always win within one migration:
    ///
    /// ```text
    /// DROP TABLE access_records;
    /// CREATE TABLE access_records (recorded BIGINT NOT NULL, note TEXT);
    ///   -> "`access_records` is bounded and nothing creates it"
    /// ```
    ///
    /// A finding telling a developer to delete a bound that is correct. Every
    /// other test here fires the gate on purpose; this one asserts it stays
    /// quiet, and the second half — the same two statements the other way round
    /// — is what keeps that from being a test that cannot fail.
    #[test]
    fn a_table_dropped_and_recreated_in_one_migration_still_exists() {
        use std::fmt::Write as _;
        let bounded = nc_catalog::TABLES
            .first()
            .expect("the catalog bounds at least one table")
            .name;
        let held = crate::check::LeavesNothingBehind::new("limits-again");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        let mut made = String::new();
        // The server's tables: a local one created here is in the wrong
        // directory, which is a finding of its own and not this test's.
        for table in nc_catalog::TABLES
            .iter()
            .filter(|table| table.database == nc_catalog::Database::Server)
        {
            let _ = writeln!(made, "CREATE TABLE {} (id INT);", table.name);
        }
        std::fs::write(scratch.join("migrations/0001_make.sql"), &made).expect("writable");
        let gone = format!("`{bounded}` is bounded and nothing creates it");

        std::fs::write(
            scratch.join("migrations/0002_again.sql"),
            format!("DROP TABLE {bounded};\nCREATE TABLE {bounded} (id INT);\n"),
        )
        .expect("writable");
        let (findings, ..) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            !findings.iter().any(|finding| finding.what == gone),
            "`{bounded}` is dropped and re-created in one file, so it exists, and the gate \
             asked for its bound to be deleted: {findings:#?}"
        );

        // The other way round, where the drop really is the last word. Without
        // this the assertion above would also pass on a walk that ignored drops.
        std::fs::write(
            scratch.join("migrations/0002_again.sql"),
            format!("CREATE TABLE {bounded} (id INT);\nDROP TABLE {bounded};\n"),
        )
        .expect("writable");
        let (findings, ..) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            findings.iter().any(|finding| finding.what == gone),
            "`{bounded}` is created and then dropped, so its entry bounds nothing, and \
             nothing said so: {findings:#?}"
        );
    }

    /// A column altered in one file and re-declared in a later one is read in
    /// the order the files run.
    ///
    /// **`stamp` is load-bearing, and this decides it.** `Created.at` and
    /// `ColumnChange.at` come out of `changes_in` numbered from zero *per file*,
    /// and the caller adds the running total so the two lists share one order.
    /// Freezing that total — `stamp += changes.statements` to `stamp = 0` —
    /// fails this test, and its own comment states the failure it exists for:
    /// *a table dropped and re-created after an `ALTER COLUMN … TYPE` would be
    /// read through the stale alteration.*
    ///
    /// Measured, with the total frozen: the re-created `bigint` column is read
    /// as the `text` the earlier file made it, and the gate asks for a
    /// correct `order_by` to be changed.
    #[test]
    fn a_column_altered_in_one_file_and_redeclared_in_a_later_one_reads_the_later_one() {
        use std::fmt::Write as _;
        let first = nc_catalog::TABLES
            .first()
            .expect("the catalog bounds at least one table");
        let (bounded, column) = (first.name, first.order_by);
        let held = crate::check::LeavesNothingBehind::new("limits-stamp");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        let mut made = String::new();
        for table in nc_catalog::TABLES {
            let _ = writeln!(
                made,
                "CREATE TABLE {} ({} BIGINT NOT NULL);",
                table.name, table.order_by
            );
        }
        std::fs::write(scratch.join("migrations/0001_make.sql"), &made).expect("writable");
        // Two statements before the alteration, so the alteration does not sit
        // where the re-creation sits in its own file: with the running total
        // frozen the two are compared by their position *within* their files.
        std::fs::write(
            scratch.join("migrations/0002_retype.sql"),
            format!(
                "SELECT 1;\nSELECT 1;\nALTER TABLE {bounded} ALTER COLUMN {column} TYPE TEXT;\n"
            ),
        )
        .expect("writable");
        let stale = format!("`{bounded}` orders by `{column}`, which is `text`");

        // The control first: with nothing after it, the alteration is the last
        // word and the gate says so.
        let (findings, ..) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            findings.iter().any(|finding| finding.what == stale),
            "`{column}` is `text` after 0002 and nothing said so: {findings:#?}"
        );

        std::fs::write(
            scratch.join("migrations/0003_again.sql"),
            format!("DROP TABLE {bounded};\nCREATE TABLE {bounded} ({column} BIGINT NOT NULL);\n"),
        )
        .expect("writable");
        let (findings, ..) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            !findings.iter().any(|finding| finding.what == stale),
            "0003 re-creates `{bounded}` with a `bigint` `{column}`, and the gate read it \
             through 0002's alteration: {findings:#?}"
        );
    }

    /// The punctuation the cutter can make a token of, which is a closed set.
    ///
    /// **Because a rule can read one that is not in it.** A
    /// `Token::Punct(';')` disjunct in `opens_a_statement` would answer for a
    /// value that cannot exist — `;` finishes a statement in the cutter and is
    /// never tokenised — and that is the predicate that bounds the `UPDATE`
    /// carve-out, so a reader would take `;` for one of the boundaries holding
    /// that carve-out in.
    ///
    /// A census over every migration this repository ships rather than over
    /// three fixtures, because what is claimed is that the set is *closed*.
    /// Making `;` a token again fails here, which is where somebody then has to
    /// decide whether the openers want it back.
    #[test]
    fn the_cutter_makes_a_token_of_four_punctuation_characters() {
        let mut seen = std::collections::BTreeSet::new();
        let mut read = 0_usize;
        for path in migrations(root()).expect("the migrations are readable") {
            let text = std::fs::read_to_string(&path).expect("a readable migration");
            for statement in statements(&text) {
                read += 1;
                for token in &statement.tokens {
                    if let Token::Punct(character) = token {
                        seen.insert(*character);
                    }
                }
            }
        }
        assert!(
            read >= THE_STATEMENTS_THIS_READS,
            "only {read} statement(s) were read, and this census is over at least \
             {THE_STATEMENTS_THIS_READS}: a walk that stops early makes a closed set look \
             closed"
        );
        assert_eq!(
            seen.into_iter().collect::<Vec<char>>(),
            vec!['(', ')', ',', '.'],
            "the cutter makes a token of a punctuation character this set does not name. \
             Every rule that reads `Token::Punct` was written against these four; a fifth \
             is a decision, not a bump"
        );

        // **And a mark the migrations do not contain.** The census above is over
        // the twelve files this repository ships, so it can only refuse a fifth
        // character those files hold *as code* — measured, `|| here == '*'`
        // added to the cutter's punctuation branch leaves it green, and widening
        // that branch to every mark shows the migrations hold only
        // `( ) + , . : < = > | ~` between them: seven of the marks below are
        // decided by the census above and fourteen are not. So the closed set is
        // asserted a second time over text written to hold every mark there is,
        // in the two positions a mark can reach the cutter — inside a statement,
        // and inside a name.
        let mut swept = std::collections::BTreeSet::new();
        for mark in EVERY_MARK.chars() {
            for text in [
                format!("SELECT a{mark}b;\n"),
                format!("CREATE TABLE t{mark}u (id INT);\n"),
            ] {
                for statement in statements(&text) {
                    for token in &statement.tokens {
                        if let Token::Punct(character) = token {
                            swept.insert(*character);
                        }
                    }
                }
            }
        }
        assert!(
            swept.iter().all(|character| ".(),".contains(*character)),
            "the cutter made a token of {swept:?}, and it makes a token of `.`, `(`, `)` \
             and `,` and of nothing else. A mark that reaches `Token::Punct` reaches every \
             rule written against those four"
        );
    }

    /// Every ASCII mark, less the one that ends a statement.
    ///
    /// `;` is left out because it is the terminator rather than a character the
    /// cutter can weigh — that is the whole claim of the census above.
    const EVERY_MARK: &str = "!\"#$%&'()*+,-./:<=>?@[\\]^_`{|}~";

    /// A floor under the census above, so it cannot pass on an empty walk.
    ///
    /// The twelve migrations hold exactly this many statements today, so the
    /// floor is the measurement and not a number with room under it: one
    /// migration going unread fails here rather than passing with a smaller
    /// corpus. It rises when a migration is added and never falls quietly.
    const THE_STATEMENTS_THIS_READS: usize = 84;

    #[test]
    fn a_migration_set_that_cannot_be_applied_in_order_is_a_finding() {
        // **The gate's own test.** Replacing its body with
        // `Ok((Vec::new(), files.len()))` fails it; so replaced, two files
        // sharing number 2 — the case its own doc comment calls "the
        // worst" — are reported as *numbered once each and in order*. Every
        // other gate in this file has a fire-it-on-purpose test; this is
        // `§19.3`'s opening failure, in the file that quotes `§19.3`.
        let held = crate::check::LeavesNothingBehind::new("scratch");
        let scratch = held.root.clone();
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        let write = |names: &[&str]| {
            let _ = std::fs::remove_dir_all(scratch.join("migrations"));
            std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
            for name in names {
                std::fs::write(scratch.join("migrations").join(name), "-- nothing\n")
                    .expect("writable");
            }
            every_migration_is_numbered_once(&scratch, super::MIGRATIONS).expect("readable")
        };

        // The shape that ships is clean, so the cases below mean something.
        let (findings, files) = write(&["0001_a.sql", "0002_b.sql", "0003_c.sql"]);
        assert!(findings.is_empty(), "{findings:#?}");
        assert_eq!(files, 3);

        // **A reversal, which is what `sqlx migrate revert` runs.** The guard for
        // this is decided here: deleting it fails this case, while the numbering
        // line alone would still read `numbered once each and in order` with a
        // thirteenth migration whose name ends in the reversal suffix holding
        // `DROP TABLE journal_entries CASCADE` on disk. `sqlx` names a reversal
        // for the migration it undoes, so a file named this way is one whether
        // or not anybody meant it — and it is a one-command undo of an
        // append-only audit log, in a tree with no git.
        let (findings, files) = write(&["0001_a.sql", "0002_b.sql", "0002_b.down.sql"]);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("this is a reversal")),
            "a reversal suffix was not reported: {findings:#?}"
        );
        assert_eq!(files, 3, "the reversal was not even read");
        // And the ordinary set beside it is still clean, so the case above is
        // about the suffix and not about the numbering.
        let (findings, _) = write(&["0001_a.sql", "0002_b.sql"]);
        assert!(findings.is_empty(), "{findings:#?}");

        // Two files sharing a number: the first to run takes it and the second
        // never runs at all, leaving a half-migrated schema.
        let (findings, _) = write(&["0001_a.sql", "0002_b.sql", "0002_c.sql"]);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("already has number 2")),
            "two files sharing a number: {findings:#?}"
        );

        // A gap: either a migration was deleted after being applied somewhere,
        // or the next number was guessed.
        let (findings, _) = write(&["0001_a.sql", "0005_b.sql"]);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("2 is missing")),
            "a gap: {findings:#?}"
        );

        // And a file with no number, which nothing can order or record.
        let (findings, _) = write(&["0001_a.sql", "later.sql"]);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("does not begin with a number")),
            "an unnumbered migration: {findings:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_dollar_inside_an_identifier_is_not_a_quote() {
        // **The worst of the five.** PostgreSQL: "subsequent characters in an
        // identifier can be letters, underscores, digits, or dollar signs" — so
        // read as a quote opening, `geo$ip$hash` opens a phantom `$ip$` quote,
        // the scan runs to end of file looking for a close that never comes, and
        // everything after it is blanked.
        let sql = "ALTER TABLE access_records ADD COLUMN geo$ip$hash TEXT;\n\
                   CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n";
        assert_eq!(caught(sql), vec!["login_attempts".to_owned()]);
        // And a real dollar-quoted body, which begins where no word does.
        let body = "DO $$ BEGIN CREATE TABLE login_attempts (id INT); END $$;\n";
        assert_eq!(caught(body), vec!["login_attempts".to_owned()]);
    }

    #[test]
    fn a_cte_before_a_select_into_does_not_hide_it() {
        // The CTE's own `SELECT` comes first, so walking from there meets the
        // CTE's `FROM` and gives up before the `INTO` that names the new table.
        let sql = "WITH failed AS (SELECT id FROM access_records WHERE FALSE)\n\
                   SELECT id INTO login_attempts FROM failed;\n";
        assert_eq!(caught(sql), vec!["login_attempts".to_owned()]);
        // `INSERT INTO` is still not a creation, whatever precedes it.
        let inserting = "WITH x AS (SELECT 1) INSERT INTO access_records (id) SELECT 1;\n";
        assert!(caught(inserting).is_empty(), "{:?}", caught(inserting));
    }

    #[test]
    fn a_ddl_string_executed_inside_a_routine_is_a_table() {
        // Idiomatic conditional DDL. The body *is* re-scanned as SQL, and the
        // statement arrives as a single-quoted literal — whose contents are
        // blanked everywhere else, correctly, because a name is never inside
        // one. Inside a routine they are statements.
        let sql = "DO $$ BEGIN\n                     IF NOT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'login_attempts') \
                   THEN\n                       EXECUTE 'CREATE TABLE login_attempts (id TEXT PRIMARY KEY)';\n                     END IF; END $$;\n";
        assert_eq!(caught(sql), vec!["login_attempts".to_owned()]);
        // And a literal outside a routine is still data.
        let data = "INSERT INTO access_records (note) VALUES ('CREATE TABLE ghost (id INT)');\n";
        assert!(caught(data).is_empty(), "{:?}", caught(data));
    }

    #[test]
    fn a_block_comment_does_not_stretch_the_hatch_over_the_next_statement() {
        // Marking a line comment-only the moment a comment starts on it,
        // regardless of the code after, reads `/* one row */ CREATE TABLE x (…);`
        // as a comment line, and `excused` then walks over that statement to
        // give one marker to the table *after* it.
        let scratch = std::env::temp_dir().join(format!("nc-limits-blk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            format!(
                "-- {BOUNDED} `install_marker` holds exactly one row, written by the installer\n\
                 /* one row, ever */ CREATE TABLE install_marker (id INT PRIMARY KEY);\n\
                 CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"
            ),
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            findings.iter().any(|f| f.what.contains("login_attempts")),
            "a block comment stretched the hatch over a statement: {findings:#?}"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_materialised_view_stores_rows_and_is_seen() {
        assert_eq!(
            caught("CREATE MATERIALIZED VIEW recent_access AS SELECT id FROM access_records;\n"),
            vec!["recent_access".to_owned()]
        );
        // An ordinary view stores nothing and is not a table.
        assert_eq!(caught("CREATE VIEW v AS SELECT 1;\n"), [] as [String; 0]);
    }

    #[test]
    fn a_statement_that_writes_to_a_table_is_not_one_that_creates_it() {
        // `MERGE INTO` writes; skipping only `INSERT INTO` reads a `WITH …
        // MERGE INTO t` as `SELECT … INTO t` and reports it as creating
        // `t` — at the line of the `MERGE`, for a table declared bounded three
        // lines above. Valid PostgreSQL 18.6, verified applied.
        let left_over = caught(
            "WITH wanted AS (SELECT 1 AS id)\n\
                 MERGE INTO install_marker t USING wanted s ON t.id = s.id\n\
                 WHEN NOT MATCHED THEN INSERT (id) VALUES (s.id);\n",
        );
        assert!(
            left_over.is_empty(),
            "a MERGE was read as creating a table: {left_over:?}"
        );
        assert_eq!(
            caught("WITH wanted AS (SELECT 1 AS id) INSERT INTO t (id) SELECT id FROM wanted;\n"),
            [] as [String; 0]
        );
        // And the shape that really does create one still does.
        assert_eq!(
            caught("WITH failed AS (SELECT 1 AS id) SELECT id INTO login_attempts FROM failed;\n"),
            vec!["login_attempts".to_owned()]
        );
    }

    #[test]
    fn a_finding_never_names_a_table_nobody_wrote() {
        // **The wrong-direction failure, in the gate whose whole value is that
        // its error names the table.** Inside a routine body every literal is
        // re-scanned as SQL, which is what makes `EXECUTE 'CREATE TABLE …'`
        // visible — and unguarded it also reads a placeholder and a sentence as
        // tables.
        //
        // `format('CREATE TABLE %I …')` is the standard dynamic-partition
        // helper, and it would fail the build naming `%I`.
        let left_over = caught(
            "DO $$ BEGIN EXECUTE format('CREATE TABLE %I PARTITION OF access_records \
                 FOR VALUES FROM (0) TO (1)', part); END $$;\n",
        );
        assert!(
            left_over.is_empty(),
            "a format placeholder was reported as a table: {left_over:?}"
        );
        // And a `RAISE`'s message is prose. This one is the shape
        // `0001_journal.sql`'s own trigger is written in, and it happens not to
        // contain the words — so the repository is one sentence away from it.
        let left_over = caught(
            "DO $$ BEGIN RAISE EXCEPTION 'this table is append-only; if you need \
                 another, CREATE TABLE ghost (id INT)'; END $$;\n",
        );
        assert!(
            left_over.is_empty(),
            "a RAISE message was read as SQL: {left_over:?}"
        );
        // **A routine *named* `raise`**, which a rule reading the last `RAISE`
        // alone does not tell from the keyword: the function name is a `Word`
        // like any other, so the body it introduces is classified as a message
        // and dropped. Live PostgreSQL 18.6 creates the table.
        assert_eq!(
            caught(
                "CREATE FUNCTION raise() RETURNS void AS \
                 'CREATE TABLE login_attempts (id TEXT PRIMARY KEY)' LANGUAGE sql;\n"
            ),
            vec!["login_attempts".to_owned()]
        );
        // And a parameter spelled the same way.
        assert_eq!(
            caught(
                "CREATE FUNCTION f(raise INT) RETURNS void AS \
                 'CREATE TABLE login_attempts (id TEXT PRIMARY KEY)' LANGUAGE sql;\n"
            ),
            vec!["login_attempts".to_owned()]
        );

        // And the case that must keep working: a routine that really does create
        // a table through `EXECUTE`.
        assert_eq!(
            caught(
                "DO $$ BEGIN EXECUTE 'CREATE TABLE login_attempts (id TEXT PRIMARY KEY)'; \
                 END $$;\n"
            ),
            vec!["login_attempts".to_owned()]
        );
    }

    /// The columns one `CREATE TABLE` declares, for the tests below.
    fn columns_of(sql: &str) -> Vec<String> {
        let statements = super::statements(sql);
        statements
            .first()
            .map(super::declared_columns)
            .unwrap_or_default()
    }

    #[test]
    fn a_column_list_is_read_as_postgresql_reads_it() {
        // Each of these is measured against a live PostgreSQL 18.6, and each is
        // a shape a naive column reader gets wrong.
        //
        // **A quoted identifier is never a keyword** — this module's own rule,
        // applied to the clause filter too: unapplied, `"check"`, `"like"`, `"primary"`
        // and `"unique"` are all dropped and the table reads as having no
        // columns at all.
        assert_eq!(
            columns_of(
                "CREATE TABLE q (\"check\" INT, \"like\" TEXT, \"primary\" INT, \"unique\" INT);"
            ),
            ["check", "like", "primary", "unique"]
        );
        // **`period` needs no quoting**, and listing it as a clause drops a
        // real column: `order_by = "period"` is then reported as naming none,
        // against a migration PostgreSQL applies and a pruner that runs.
        assert_eq!(
            columns_of("CREATE TABLE p (id INT, period TEXT);"),
            ["id", "period"]
        );
        // And the clause keywords, unquoted, are still not columns.
        assert_eq!(
            columns_of(
                "CREATE TABLE c (id INT, at BIGINT, PRIMARY KEY (id), \
                 CONSTRAINT ok CHECK (at > 0), UNIQUE (at));"
            ),
            ["id", "at"]
        );
    }

    #[test]
    fn a_table_built_from_a_template_says_it_cannot_say() {
        // `CREATE TABLE t (LIKE shape INCLUDING ALL, extra TEXT)` copies every
        // column of `shape`, and reading only `extra` reports a real `order_by`
        // as naming no column. This scanner cannot know what the template holds,
        // so it says nothing rather than something wrong — and an empty list is
        // what the caller treats as cannot-say.
        assert_eq!(
            columns_of("CREATE TABLE t (LIKE shape INCLUDING ALL, extra TEXT);"),
            [] as [String; 0]
        );
        assert_eq!(
            columns_of("CREATE TABLE t (LIKE shape);"),
            [] as [String; 0]
        );
        // **And a `LIKE` that is not a table-shape clone does not turn the
        // check off.** Looked for anywhere in the statement, the word lets one
        // ordinary pattern constraint make every column of that table
        // unreadable — and `order_by` then matches nothing and is accepted.
        assert_eq!(
            columns_of("CREATE TABLE t (id TEXT PRIMARY KEY, CHECK (id LIKE 'NC-%'), n INT);"),
            ["id", "n"]
        );
        assert_eq!(
            columns_of("CREATE TABLE t (id TEXT CHECK (id NOT LIKE '%/%'));"),
            ["id"]
        );
        // A column *called* `like` is quoted, and a quoted identifier is never a
        // keyword — this module's own rule, applied here too.
        assert_eq!(
            columns_of("CREATE TABLE t (\"like\" TEXT, n INT);"),
            ["like", "n"]
        );
    }

    #[test]
    fn a_column_an_alter_changes_is_followed() {
        // **The failure this whole check exists to prevent**: read from the
        // `CREATE TABLE` alone, a migration renaming the column the catalog
        // orders by leaves the gate green, and the pruner's next `ORDER BY "id"` fails
        // with `column "id" does not exist`.
        let scratch = std::env::temp_dir().join(format!("nc-limits-alter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        let bounded = nc_catalog::TABLES
            .iter()
            .find(|table| table.strategy == nc_catalog::Strategy::OldestFirst)
            .expect("the catalog bounds something oldest-first");

        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            format!(
                "CREATE TABLE {} ({} BIGINT PRIMARY KEY, note TEXT);\n",
                bounded.name, bounded.order_by
            ),
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            !findings.iter().any(|f| f.what.contains("orders by")),
            "a column the migration declares was reported as missing: {findings:#?}"
        );

        // Renamed away.
        std::fs::write(
            scratch.join("migrations/0002_y.sql"),
            format!(
                "ALTER TABLE {} RENAME COLUMN {} TO renamed_away;\n",
                bounded.name, bounded.order_by
            ),
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            findings.iter().any(|f| f
                .what
                .contains("which no migration declares as a column of it")),
            "a renamed order_by column was not reported as missing — every finding this \
             rule can build begins `X orders by Y`, so the shorter needle cannot tell \
             this verdict from `the migrations do not say what it is` or from \
             `may be NULL`: {findings:#?}"
        );

        // Dropped.
        std::fs::write(
            scratch.join("migrations/0002_y.sql"),
            format!(
                "ALTER TABLE {} DROP COLUMN {};\n",
                bounded.name, bounded.order_by
            ),
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            findings.iter().any(|f| f
                .what
                .contains("which no migration declares as a column of it")),
            "a dropped order_by column was not reported as missing: {findings:#?}"
        );

        // And added back, so the walk applies the changes in order rather than
        // noticing that one happened. Nullable, which is what `ADD COLUMN` with
        // no `NOT NULL` means — and the rule reads the `ALTER` for that too, so
        // the column exists and is reported for what it now is rather than for
        // being missing.
        std::fs::write(
            scratch.join("migrations/0003_z.sql"),
            format!(
                "ALTER TABLE {} ADD COLUMN {} BIGINT;\n",
                bounded.name, bounded.order_by
            ),
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        // **The sentence this gate actually writes.** A needle such as
        // `"does not name"`, which no finding this file builds contains — the
        // real one reads "`{table}` orders by `{column}`, which no migration
        // declares as a column of it" — makes the assertion unfalsifiable:
        // replaced with any string at all, nothing moves. A negative assertion
        // over a phrase that never appears is true for the same reason an
        // assertion over an empty collection is.
        assert!(
            !findings
                .iter()
                .any(|f| f.what.contains("no migration declares as a column of it")),
            "the column was added back and is still reported missing: {findings:#?}"
        );
        assert!(
            findings.iter().any(|f| f.what.contains("may be NULL")),
            "an `ADD COLUMN` with no `NOT NULL` gives a nullable column, and the ordering \
             rule read the `CREATE TABLE` for that: {findings:#?}"
        );

        // Declared not-null by a fourth statement, which is the other half of
        // the same reading.
        std::fs::write(
            scratch.join("migrations/0004_w.sql"),
            format!(
                "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL;\n",
                bounded.name, bounded.order_by
            ),
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            !findings.iter().any(|f| f.what.contains("orders by")),
            "the column is there, `bigint` and `NOT NULL`, and is still reported: \
             {findings:#?}"
        );

        // `ADD CONSTRAINT` is not a column.
        let added = super::column_changes(
            super::statements("ALTER TABLE t ADD CONSTRAINT ok CHECK (a > 0);")
                .first()
                .expect("one statement"),
        );
        assert!(added.is_empty(), "{added:?}");

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn one_statements_columns_do_not_bleed_onto_another_table() {
        // A list computed once and cloned to every name a statement
        // declares gives both tables of `CREATE SCHEMA archive CREATE TABLE a (…) CREATE TABLE
        // b (…)` the first one's columns — and a
        // schema-qualified entry is a shape the catalog explicitly supports.
        let statements = super::statements(
            "CREATE SCHEMA archive CREATE TABLE access_records (id BIGINT, realm TEXT) \
             CREATE TABLE journal_entries (sequence BIGINT, hash BYTEA);",
        );
        let changes = changes_in(
            "CREATE SCHEMA archive CREATE TABLE access_records (id BIGINT, realm TEXT) \
             CREATE TABLE journal_entries (sequence BIGINT, hash BYTEA);",
        );
        assert_eq!(statements.len(), 1, "one statement declares both");
        assert_eq!(changes.created.len(), 2);
        for table in &changes.created {
            assert!(
                table.columns.is_empty(),
                "`{}` was given another table's columns: {:?}",
                table.name,
                table.columns
            );
        }
    }

    #[test]
    fn a_schema_qualified_name_is_offered_as_valid_toml() {
        // `[tables.archive.access_records]` is a *nested* table to TOML, so an
        // unquoted fix text followed literally produces `unknown field
        // 'access_records'` — a gate telling a developer to write something that
        // does not parse. The quoted form is what the compiler reads, and it is
        // what `nc-prune`'s identifier guard accepts.
        let scratch = std::env::temp_dir().join(format!("nc-limits-qual-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            "CREATE SCHEMA archive;\nCREATE TABLE archive.access_records (id TEXT);\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        let named = findings
            .iter()
            .find(|finding| finding.what.contains("archive.access_records"))
            .expect("the qualified table is reported");
        assert!(
            named.fix.contains("[tables.\"archive.access_records\"]"),
            "the fix offers TOML that does not parse: {}",
            named.fix
        );
        // And an unqualified one is still offered unquoted, which is how every
        // entry in the shipped registry is written.
        assert_eq!(super::toml_key("access_records"), "access_records");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_dollar_quote_after_an_operator_is_a_quote() {
        // **Measured blanking a whole migration.** With `word.is_empty()` as a
        // proxy for "not inside an identifier" — and an operator accumulates into
        // `word` too — `=$doc$…$doc$` does not open a quote, the apostrophe
        // inside opens a string literal that runs to the end of the file, and
        // the `CREATE TABLE` after it is invisible with `check passed`. Live
        // PostgreSQL 18.6 applies this and creates the table.
        assert_eq!(
            caught(
                "UPDATE access_records SET user_agent=$doc$don't edit this$doc$ WHERE id='x';\n\
                 CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"
            ),
            vec!["login_attempts".to_owned()]
        );
        // The same with an operator that is two characters, and with a space —
        // the shape a proxy gets right, kept so the rule is not one-sided.
        for shape in ["<>$doc$don't$doc$", "= $doc$don't$doc$", "=$doc$don't$doc$"] {
            assert_eq!(
                caught(&format!(
                    "UPDATE access_records SET a{shape} WHERE id='x';\n\
                     CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"
                )),
                vec!["login_attempts".to_owned()],
                "{shape}"
            );
        }
        // And the case the guard exists for: a `$` *inside* an identifier is not
        // a quote, so `geo$ip$hash` must still not blank the file.
        assert_eq!(
            caught(
                "ALTER TABLE access_records ADD COLUMN geo$ip$hash TEXT;\n\
                 CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"
            ),
            vec!["login_attempts".to_owned()]
        );
    }

    #[test]
    fn a_routine_body_written_as_a_string_literal_is_read() {
        // Scanning `DO $$ … $$` and not `DO '…'` — the same statement, one
        // quoting form older — leaves every `CREATE TABLE` inside the second
        // invisible. Live PostgreSQL 18.6 creates all three of these.
        assert_eq!(
            caught(
                "DO 'BEGIN CREATE TABLE login_attempts (id TEXT PRIMARY KEY); END' LANGUAGE plpgsql;\n"
            ),
            vec!["login_attempts".to_owned()]
        );
        assert_eq!(
            caught(
                "CREATE FUNCTION make_it() RETURNS void AS '\n\
                 BEGIN CREATE TABLE login_attempts (id TEXT PRIMARY KEY); END;' LANGUAGE plpgsql;\n"
            ),
            vec!["login_attempts".to_owned()]
        );
        assert_eq!(
            caught(
                "CREATE FUNCTION make_it_sql() RETURNS void AS \
                 'CREATE TABLE login_attempts (id TEXT PRIMARY KEY)' LANGUAGE sql;\n"
            ),
            vec!["login_attempts".to_owned()]
        );
        // And a literal outside a routine is still data, not SQL.
        assert_eq!(
            caught("INSERT INTO access_records (note) VALUES ('CREATE TABLE ghost (id INT)');\n"),
            [] as [String; 0]
        );
    }

    #[test]
    fn a_foreign_table_is_seen() {
        // Its rows live on another server, so its bound is not a `DELETE` this
        // pruner can run — and unread, a migration adding one leaves the count
        // unchanged and the gate green, which is the shape `§15.8` exists to refuse.
        assert_eq!(
            caught(
                "CREATE FOREIGN TABLE remote_events (id BIGINT) SERVER other OPTIONS \
                 (table_name 'events');\n"
            ),
            vec!["remote_events".to_owned()]
        );
        // And the neighbours it must not be confused with.
        assert_eq!(
            caught("CREATE SERVER other FOREIGN DATA WRAPPER postgres_fdw;\n"),
            [] as [String; 0]
        );
        assert_eq!(
            caught("ALTER TABLE access_records ADD FOREIGN KEY (who) REFERENCES people (id);\n"),
            [] as [String; 0]
        );
    }

    /// The two quoting forms a hand-rolled blanker misses, both ways.
    fn a_quoting_form_does_not_hide_or_invent_a_statement(scratch: &std::path::Path) {
        // **The two quoting forms a hand-rolled blanker misses.** With one, an
        // `E'…\'…'` above the `COPY` hides the refusal again — and then the
        // apostrophe in the data hides the `CREATE TABLE` after it — while a
        // `$doc$…$doc$` document holding the words fails the build over prose.
        // Both are forms `cut` has dedicated tests for, which is the argument
        // for having one grammar.
        for hidden in [
            "COMMENT ON TABLE access_records IS E'seed rows; don\\'t edit them here';\n\
             COPY access_records (note) FROM stdin;\nit's a note\n\\.\n",
            "SELECT $$don't$$;\nCOPY access_records (note) FROM stdin;\nit's a note\n\\.\n",
        ] {
            std::fs::write(
                scratch.join("migrations/0001_x.sql"),
                format!("{hidden}CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"),
            )
            .expect("writable");
            let (findings, _, _, _) = every_table_declares_a_bound(scratch).expect("readable");
            assert!(
                findings.iter().any(|f| f.what.contains("FROM stdin")),
                "`{hidden}` hid the refusal: {findings:#?}"
            );
        }

        // And the same forms in the other direction: a document that *mentions*
        // the statements is data, and failing a build over it is the
        // wrong-direction failure this file names as the one it must not have.
        for prose in [
            "INSERT INTO access_records (note) VALUES ($doc$we copy the rows from stdin by \
             hand, and never import foreign schema wholesale$doc$);\n",
            "COMMENT ON TABLE access_records IS E'don\\'t copy anything from stdin here';\n",
        ] {
            std::fs::write(scratch.join("migrations/0001_x.sql"), prose).expect("writable");
            let (findings, _, _, _) = every_table_declares_a_bound(scratch).expect("readable");
            assert!(
                !findings
                    .iter()
                    .any(|f| f.what.contains("is in a migration")),
                "a document was read as a statement: {findings:#?}"
            );
        }

        // **And prose is not a statement.** Read as text, the words in a comment
        // fail the build over a migration that holds no such statement — the
        // wrong-direction failure this file names as the one it must not have.
        for prose in [
            "-- We never import foreign schema wholesale, because then nothing\n             -- here would say how many tables the database holds.\n             ALTER TABLE access_records ADD COLUMN note TEXT;\n",
            "INSERT INTO access_records (note) VALUES ('copy the rows from stdin by hand');\n",
        ] {
            std::fs::write(scratch.join("migrations/0001_x.sql"), prose).expect("writable");
            let (findings, _, _, _) = every_table_declares_a_bound(scratch).expect("readable");
            assert!(
                !findings
                    .iter()
                    .any(|f| f.what.contains("is in a migration")),
                "prose was read as a statement: {findings:#?}"
            );
        }

        // **`IMPORT FOREIGN SCHEMA`**, the bulk form of
        // `CREATE FOREIGN TABLE`: it creates one foreign table per
        // table on the remote server, and this gate cannot know how many.
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            "IMPORT FOREIGN SCHEMA remote LIMIT TO (events) FROM SERVER other INTO public;\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(scratch).expect("readable");
        assert!(
            findings
                .iter()
                .any(|f| f.what.contains("IMPORT FOREIGN SCHEMA")),
            "a bulk foreign-table import was not reported: {findings:#?}"
        );
    }

    #[test]
    fn a_statement_built_at_run_time_is_refused_rather_than_guessed_at() {
        // **Both shapes create real tables**, verified on PostgreSQL 18.6:
        //
        // ```text
        // DO $$ BEGIN EXECUTE format('CREATE TABLE %I (id INT)', 'login_attempts'); END $$;
        // DO $$ BEGIN EXECUTE 'CREATE TABLE ' || quote_ident('x') || ' (id INT)'; END $$;
        //   -> pg_tables: login_attempts, x
        // ```
        //
        // and unrefused, both leave the gate reporting a count with no finding:
        // `%I` is not a name and a concatenation has no name token at all, so the
        // gate's answer is "this migration creates nothing". `IMPORT FOREIGN SCHEMA`
        // is refused for exactly that reason, and so is this.
        let scratch = std::env::temp_dir().join(format!("nc-limits-exec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        for built in [
            "DO $$ BEGIN EXECUTE format('CREATE TABLE %I (id INT)', 'login_attempts'); END $$;\n",
            "DO $$ BEGIN EXECUTE 'CREATE TABLE ' || quote_ident('x') || ' (id INT)'; END $$;\n",
            "CREATE FUNCTION f() RETURNS void AS $$ BEGIN EXECUTE stmt; END; $$ LANGUAGE plpgsql;\n",
        ] {
            std::fs::write(scratch.join("migrations/0001_x.sql"), built).expect("writable");
            let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
            assert!(
                findings.iter().any(|f| f.what.contains("EXECUTE")),
                "`{built}` was read as creating nothing: {findings:#?}"
            );
        }

        // **And the ordinary use is not refused.** Every migration in this tree
        // ends with `CREATE TRIGGER … EXECUTE FUNCTION …`, so a rule that read
        // the verb alone would refuse the whole tree — which is a gate turned
        // off by being turned up.
        for ordinary in [
            "CREATE TRIGGER t BEFORE UPDATE ON access_records FOR EACH ROW \
             EXECUTE FUNCTION f();\n",
            "CREATE TRIGGER t BEFORE UPDATE ON access_records FOR EACH ROW \
             EXECUTE PROCEDURE f();\n",
            // **Prose in a literal inside a routine body is data.** The same
            // sentence at the top level produces nothing; inside a `DO` block,
            // re-cut, the literal's first word lands at position zero and fails
            // the build as dynamic SQL, as a foreign-schema import and
            // as a `COPY` from stdin at once.
            "DO $$ BEGIN INSERT INTO access_records (note) VALUES \
             ('we never EXECUTE anything here'); END $$;\n",
            "DO $$ BEGIN INSERT INTO access_records (note) VALUES \
             ('import foreign schema is banned here'); END $$;\n",
            "DO $$ BEGIN INSERT INTO access_records (note) VALUES \
             ('copy t from stdin is banned here'); END $$;\n",
            // **And `EXECUTE` is SQL's privilege on a routine as well as its
            // verb for dynamic SQL.** Revoking it from `PUBLIC` is the ordinary
            // next hardening step for the three functions this tree ships in
            // migrations, and a reader of the verb alone refuses it with a
            // message about dynamic SQL.
            "REVOKE EXECUTE ON FUNCTION public.f() FROM PUBLIC;\n",
            "GRANT EXECUTE ON FUNCTION public.f() TO noctua;\n",
            "GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO noctua;\n",
            // **And the two forms that do not begin with the verb** — one makes
            // a privilege persist, the other makes it conditional.
            "ALTER DEFAULT PRIVILEGES IN SCHEMA public REVOKE EXECUTE ON FUNCTIONS \
             FROM PUBLIC;\n",
            "DO $$ BEGIN GRANT EXECUTE ON FUNCTION public.f() TO noctua; END $$;\n",
        ] {
            std::fs::write(scratch.join("migrations/0001_x.sql"), ordinary).expect("writable");
            let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
            assert!(
                !findings.iter().any(|f| f.what.contains("EXECUTE")),
                "an ordinary trigger was refused: {findings:#?}"
            );
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_copy_from_stdin_is_refused_rather_than_read_as_sql() {
        // **And a foreign-schema import wherever it is written.** A routine
        // body's statements begin with `BEGIN`, so a rule anchored at token zero
        // never sees one inside a `DO` block — and PL/pgSQL parses and dispatches
        // it.
        for wrapped in [
            "IMPORT FOREIGN SCHEMA remote LIMIT TO (events) FROM SERVER other INTO public;\n",
            "DO $$ BEGIN IMPORT FOREIGN SCHEMA remote LIMIT TO (events) FROM SERVER other \
             INTO public; END $$;\n",
        ] {
            let scratch = std::env::temp_dir().join(format!("nc-import-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&scratch);
            std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
            std::fs::write(scratch.join("migrations/0001_x.sql"), wrapped).expect("writable");
            let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
            assert!(
                findings
                    .iter()
                    .any(|f| f.what.contains("IMPORT FOREIGN SCHEMA")),
                "a bulk foreign-table import was not reported: {wrapped}{findings:#?}"
            );
            let _ = std::fs::remove_dir_all(&scratch);
        }
        // Measured unrefused: the apostrophe in the data opens a string literal
        // that runs to the end of the file, so `CREATE TABLE login_attempts`
        // after it is invisible and the gate passes reporting a count.
        let scratch = std::env::temp_dir().join(format!("nc-limits-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        std::fs::write(
            scratch.join("migrations/0001_x.sql"),
            "COPY access_records (note) FROM stdin;\n             it's a note\n             \\.\n             CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
        assert!(
            findings.iter().any(|f| f.what.contains("FROM stdin")),
            "a COPY … FROM stdin was read as SQL: {findings:#?}"
        );

        // **Wrapped, and after another statement on the same line**, which a
        // line-at-a-time reader walks straight past — and the apostrophe then
        // blanks the file the same way.
        for wrapped in [
            "COPY access_records (note)\nFROM stdin;\nit's a note\n\\.\n",
            "BEGIN; COPY access_records (note) FROM stdin;\nit's a note\n\\.\n",
            "COPY access_records (note)\n  FROM\n  stdin;\nit's a note\n\\.\n",
        ] {
            std::fs::write(
                scratch.join("migrations/0001_x.sql"),
                format!("{wrapped}CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"),
            )
            .expect("writable");
            let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
            assert!(
                findings.iter().any(|f| f.what.contains("FROM stdin")),
                "`{wrapped}` was read as SQL: {findings:#?}"
            );
        }

        // **Behind a comment, and behind a quoted name.** Read from the text, a
        // `--` line above the `COPY` makes `copy` no longer the statement's first
        // word, so the refusal is skipped and the apostrophe in the data blanks
        // the file — the defect the refusal exists to prevent,
        // reachable by writing the comment every migration in this tree has.
        for hidden in [
            "-- Seed the first access record.\nCOPY access_records (note) FROM stdin;\nit's a note\n\\.\n",
            "/* seed rows */ COPY access_records (note) FROM stdin;\nit's a note\n\\.\n",
            "COPY access_records FROM /* the client sends them */ stdin;\nit's a note\n\\.\n",
        ] {
            std::fs::write(
                scratch.join("migrations/0001_x.sql"),
                format!("{hidden}CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"),
            )
            .expect("writable");
            let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
            assert!(
                findings.iter().any(|f| f.what.contains("FROM stdin")),
                "`{hidden}` was read as SQL: {findings:#?}"
            );
        }

        a_quoting_form_does_not_hide_or_invent_a_statement(&scratch);

        // The shapes that are not it: `COPY … TO` writes, and `FROM PROGRAM`
        // and `FROM '/path'` are ordinary statements sqlx can run.        // The shapes that are not it: `COPY … TO` writes, and `FROM PROGRAM`
        // and `FROM '/path'` are ordinary statements sqlx can run.
        for legal in [
            "COPY access_records TO stdout;\n",
            "COPY access_records FROM PROGRAM 'cat rows.csv';\n",
            "COPY access_records FROM '/srv/rows.csv';\n",
            // And an ordinary foreign table, which the gate reads rather than
            // refuses — the singular form is not the bulk one.
            "CREATE FOREIGN TABLE remote_events (id BIGINT) SERVER other;\n",
        ] {
            std::fs::write(scratch.join("migrations/0001_x.sql"), legal).expect("writable");
            let (findings, _, _, _) = every_table_declares_a_bound(&scratch).expect("readable");
            assert!(
                !findings
                    .iter()
                    .any(|f| f.what.contains("is in a migration")),
                "`{legal}` was refused and it is not the shape: {findings:#?}"
            );
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn the_name_is_read_from_every_shape_postgresql_writes_it_in() {
        // Read naively, each of these names the wrong thing, which is criterion
        // 5's second half — *the error names the table* — failing in the one way
        // it must not.
        assert_eq!(
            caught("CREATE TABLE U&\"login_attempts\" (id TEXT);\n"),
            vec!["login_attempts".to_owned()],
            "`U&` was taken as the name"
        );
        assert_eq!(
            caught("CREATE SCHEMA AUTHORIZATION noctua CREATE TABLE auth_five (id INT);\n"),
            vec!["noctua.auth_five".to_owned()],
            "the owner was taken as the schema"
        );
        // And every table a `DROP` retires, not the first.
        let dropped = changes_in("DROP TABLE journal_entries, access_records;\n");
        let retired: Vec<&str> = dropped.renamed.iter().map(|r| r.from.as_str()).collect();
        assert_eq!(retired, vec!["journal_entries", "access_records"]);
    }

    /// The real tree, checked here as well as by the step that prints it.
    ///
    /// **Two enforcements, and only one of them should be read.** The `record
    /// limits` step has already located the defect, named the table, given the
    /// four keys it needs and cited the rule; this is the net under the step
    /// itself being dropped from `check`, and it has nothing to add about any
    /// particular finding. A `{findings:#?}` of the whole list would make one
    /// unbounded table produce a first-rate diagnostic and then, 800 lines
    /// later, the same finding again as a `Debug` dump — with the fix text
    /// repeated inside it.
    ///
    /// So it says how many and where to look, and the step says what.
    #[test]
    fn the_repository_declares_a_bound_for_every_table_it_creates() {
        let (findings, seen, _, _) = every_table_declares_a_bound(root()).expect("readable");
        assert!(
            findings.is_empty(),
            "{} finding(s) about row limits — the `record limits` step above names each \
             one, with its file, its line and what to do",
            findings.len()
        );
        assert!(seen > 0, "no table was read, so this gate proved nothing");
    }

    #[test]
    fn a_table_with_no_entry_is_a_finding_and_the_marker_excuses_it() {
        // Fired on purpose, in a scratch tree, because a gate nobody has watched
        // fire is a gate nobody knows is wired up.
        let scratch = std::env::temp_dir().join(format!("nc-limits-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");

        let write = |sql: String| {
            std::fs::write(scratch.join("migrations/0001_x.sql"), sql).expect("writable");
            every_table_declares_a_bound(&scratch).expect("readable")
        };
        let caught = |findings: &[crate::gates::Finding]| {
            findings
                .iter()
                .any(|finding| finding.what.contains("something_unbounded"))
        };

        let (findings, seen, _, _) =
            write("CREATE TABLE something_unbounded (\n    id BIGINT PRIMARY KEY\n);\n".to_owned());
        assert_eq!(seen, 1);
        assert!(caught(&findings), "{findings:#?}");

        let (findings, _, _, _) = write(format!(
            "-- {BOUNDED} `something_unbounded` holds one row, written at install and never again\n\
             CREATE TABLE something_unbounded (\n    id BIGINT PRIMARY KEY\n);\n"
        ));
        assert!(
            !caught(&findings),
            "the marker did not excuse it: {findings:#?}"
        );

        // **And a bare marker excuses nothing.** Checked with
        // `contains(BOUNDED)`, the reason this file demands is worth nothing,
        // and the hatch costs nothing to use. `references::marked_in` exists
        // because every gate that invents its own forgets the reason.
        let (findings, _, _, _) = write(format!(
            "-- {BOUNDED}\nCREATE TABLE something_unbounded (\n    id BIGINT\n);\n"
        ));
        assert!(
            caught(&findings),
            "a bare marker with no reason excused it: {findings:#?}"
        );

        // **A comment that describes the hatch is not the hatch.** This exact
        // block — the sentence this gate's own `fix` text tells you to write —
        // excuses every table beneath it under a rule that finds the marker
        // anywhere, in the file where the gate is loudest
        // about not being routed around.
        let (findings, _, _, _) = write(format!(
            "-- House style: a table that genuinely cannot grow is excused by writing\n\
             -- {BOUNDED} followed by the reason, in the comment block above it.\n\
             -- Everything else needs an entry in catalog/limits.toml.\n\
             \n\
             CREATE TABLE something_unbounded (\n    id BIGINT\n);\n"
        ));
        assert!(
            caught(&findings),
            "prose describing the marker excused a table: {findings:#?}"
        );

        // And the marker has to *begin* the comment, so a sentence that happens
        // to contain it does not count even without the blank line.
        let (findings, _, _, _) = write(format!(
            "-- see {BOUNDED} elsewhere for how this is written\n\
             CREATE TABLE something_unbounded (\n    id BIGINT\n);\n"
        ));
        assert!(
            caught(&findings),
            "a mention inside a sentence excused a table: {findings:#?}"
        );

        // **And the marker has to *open* its line, and this holds it.** The rule
        // is `strip_prefix("--")` and its own doc says the reason must begin its
        // line, and this is what tells that apart from `find("--")`.
        // A block comment's interior lines are comment lines to the walk and
        // need not begin with `--` at all, so a line holding ` x -- ` and then
        // the marker is the shape that walks through a `find`. Measured: with
        // the prefix rule weakened to a search, this fixture excuses an
        // unbounded table, and the assertion below fails.
        let (findings, _, _, _) = write(format!(
            "/*\n x -- {BOUNDED} `something_unbounded` holds one row, written at install\n*/\n\
             CREATE TABLE something_unbounded (\n    id BIGINT\n);\n"
        ));
        assert!(
            caught(&findings),
            "a marker with something before it on the line excused a table: {findings:#?}"
        );

        // And the control, so the rule is not the opposite mistake: the same
        // line inside the same block comment, with nothing before the `--`,
        // still excuses. A block comment is where the migrations write prose.
        let (findings, _, _, _) = write(format!(
            "/*\n -- {BOUNDED} `something_unbounded` holds one row, written at install\n*/\n\
             CREATE TABLE something_unbounded (\n    id BIGINT\n);\n"
        ));
        assert!(
            !caught(&findings),
            "the marker stopped working inside a block comment: {findings:#?}"
        );

        // The marker still works one comment line up, which is the shape the real
        // migrations use — the rules above must not close the hatch itself.
        let (findings, _, _, _) = write(format!(
            "-- The install marker.\n\
             -- {BOUNDED} `something_unbounded` holds one row, written at install and \
             never again\n\
             CREATE TABLE something_unbounded (\n    id BIGINT\n);\n"
        ));
        assert!(
            !caught(&findings),
            "the marker stopped working: {findings:#?}"
        );

        // **And an excuse written about a different table does not travel.** The
        // marker sits above a statement, so the next table added above it
        // inherits whatever the comment said — and with the comment only
        // required to be non-empty, `x` is a reason.
        let (findings, _, _, _) = write(format!(
            "-- {BOUNDED} `install_marker` holds one row, written at install\n\
             CREATE TABLE something_unbounded (\n    id BIGINT\n);\n"
        ));
        assert!(
            caught(&findings),
            "an excuse about another table excused this one: {findings:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }
}

#[cfg(test)]
mod the_migration_function_gate {
    use super::every_function_resolves_its_own_names;

    /// The gate reaches a function wherever in a migration set it sits.
    ///
    /// **Both halves of this gate, tested.** Its count is `latest.len()` —
    /// the very map the loop judges — so the two move together by construction,
    /// and with the count alone, `if !text.is_empty() { return Vec::new(); }`
    /// inside `functions_in` prints `ok 0 function(s) in migrations pin how they
    /// resolve a name` with `check passed`, measured. A floor on zero answers
    /// that one. What it cannot answer is
    /// a walk that reads every file and judges only the first: this does, with a
    /// file position and a body depth this process picks.
    #[test]
    fn the_gate_reaches_an_unpinned_function_wherever_it_sits() {
        let scratch = std::env::temp_dir().join(format!("nc-migfn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        // **The last file, and past the longest migration.** `pid % 5 + 3` files
        // with the plant at `pid % files` puts it in the *first* file for 17.9% of
        // process ids — the very shape this test's own name says it answers — and
        // `pid % 30 + 2` puts it between lines 5 and 34, a window written in this
        // line. A filter at `line > 40` then judges nothing at all in the real
        // tree, where every surviving definition is past line 40, and prints
        // `ok 3 function(s)` unchanged.
        //
        // The plant goes in the last file and past the longest migration, so a
        // filter that lets any real migration's tail through lets this through.
        let files = 3 + std::process::id() as usize % 5;
        let planted_in = files - 1;
        // **The real tree, not the scratch.** Measured against the scratch — which
        // is empty at this point — `longest` is zero and the plant lands in the
        // first hundred lines, which is the window this widening exists to
        // remove. The number that matters is the longest file the gate reads when
        // it runs for real.
        let longest = crate::gates::longest_file_read(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.."
        )));
        let depth = longest + 1 + std::process::id() as usize % 100;
        // The relationship, asserted rather than assumed: `longest + 1 > longest`
        // is a tautology, and the number has to come from the tree a second time
        // for the comparison to mean anything.
        assert!(
            depth > longest && longest > 100,
            "the fixture is {depth} deep and the longest file this gate reads is \
             {longest}: a filter tuned above the fixture would be invisible to it"
        );
        for at in 0..files {
            let mut body = format!("-- migration {at}\nCREATE TABLE t{at} (id BIGINT);\n");
            for _ in 0..depth {
                body.push_str("-- filler\n");
            }
            // Every file but one declares a function that pins its path.
            if at == planted_in {
                body.push_str(
                    "CREATE FUNCTION planted() RETURNS TRIGGER LANGUAGE plpgsql AS $$\n\
                     BEGIN RETURN NEW; END;\n$$;\n",
                );
            } else {
                use core::fmt::Write as _;
                let _ = write!(
                    body,
                    "CREATE FUNCTION pinned{at}() RETURNS TRIGGER LANGUAGE plpgsql \
                     SET search_path FROM CURRENT AS $$\nBEGIN RETURN NEW; END;\n$$;\n"
                );
            }
            std::fs::write(
                scratch.join(format!("migrations/{:04}_probe.sql", at + 1)),
                body,
            )
            .expect("writable");
        }

        let (findings, functions) =
            every_function_resolves_its_own_names(&scratch, "public").expect("the scratch walks");
        assert_eq!(
            functions, files,
            "the walk found {functions} function(s) in {files} migration(s)"
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("planted")),
            "the gate did not reach the unpinned function in file {planted_in} of \
             {files}, {depth} line(s) down: a filter between the counter and the \
             judgement leaves every count where it was: {findings:#?}"
        );
        assert_eq!(findings.len(), 1, "{findings:#?}");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// PostgreSQL's other two quoting forms do not blank the rest of the file.
    ///
    /// **Both string forms, and both identifier forms.** A literal walk that
    /// knows `''` doubling and nothing else fails on the rest. With
    /// `standard_conforming_strings` `on` — measured on the developer's cluster,
    /// PostgreSQL 18.6 — an **escape string** `E'don\'t'` is one five-character
    /// string, and `SELECT length(E'don\'t')` answers 5. Such a walk closes the
    /// literal at the backslash-escaped quote, re-opens one at the real closing
    /// quote, and blanks everything after it — so a `CREATE FUNCTION` below
    /// such a line is invisible to every later pass and an unpinned definition
    /// goes unreported with the audit printing its ordinary line. A `'` inside a
    /// `"quoted identifier"` is the same hole from the other side. Applied to a
    /// real cluster, that migration reads
    /// `pg_proc.proconfig` back as `NULL`.
    #[test]
    fn a_quoting_form_this_walk_did_not_know_does_not_hide_a_function() {
        for (what, preamble) in [
            (
                "an escape string with a backslash-escaped quote",
                "COMMENT ON SCHEMA public IS E'don\\'t edit it';\n",
            ),
            (
                "a quoted identifier holding an apostrophe",
                "CREATE TABLE \"don't\" (id BIGINT);\n",
            ),
        ] {
            let scratch = std::env::temp_dir().join(format!(
                "nc-quoting-{}-{}",
                std::process::id(),
                what.len()
            ));
            let _ = std::fs::remove_dir_all(&scratch);
            std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
            std::fs::write(
                scratch.join("migrations/0001_probe.sql"),
                format!(
                    "{preamble}CREATE FUNCTION planted() RETURNS TRIGGER LANGUAGE plpgsql \
                     AS $$\nBEGIN RETURN NEW; END;\n$$;\n"
                ),
            )
            .expect("writable");
            let (findings, functions) = every_function_resolves_its_own_names(&scratch, "public")
                .expect("the scratch walks");
            assert_eq!(
                functions, 1,
                "{what}: the walk found {functions} function(s) where one is declared — \
                 the quoting form swallowed the declaration"
            );
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.contains("planted")),
                "{what}: an unpinned function below it went unreported, which is the \
                 audit printing its ordinary line over a definition nobody read: \
                 {findings:#?}"
            );
            let _ = std::fs::remove_dir_all(&scratch);
        }
    }

    /// The two halves of this gate spell a routine's name the same way.
    ///
    /// **Spelled two ways, the disagreement is not a miss.** A `CREATE` side
    /// walking the characters counting brackets and an `ALTER` side taking one
    /// whitespace-separated token disagree where the argument list holds a space, or a
    /// space sits before the bracket: the `ALTER` lands in the map as a
    /// *second* function — pinned, because that is what it says — and the real
    /// one is reported unpinned, with the message quoting the `CREATE` while
    /// the statement that is wrong sits two lines below it.
    ///
    /// **And an `ALTER` that names nothing this tree creates is a finding.** As a
    /// new entry, the phantom would satisfy the gate. Every
    /// `ALTER FUNCTION` in these migrations names a function an earlier one
    /// creates, which is the only way one can work at all.
    #[test]
    fn the_two_halves_spell_a_routine_the_same_way() {
        /// A migration holding `body`, walked by the function gate.
        fn walked(body: &str) -> (Vec<super::Finding>, usize) {
            // Removed however this leaves, because a failing assertion below is
            // exactly the path a bare `remove_dir_all` at the end never takes.
            let held =
                crate::check::LeavesNothingBehind::new(&format!("routine-name-{}", body.len()));
            let scratch = held.root.clone();
            std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
            std::fs::write(scratch.join("migrations/0001_probe.sql"), body).expect("writable");
            let read = every_function_resolves_its_own_names(&scratch, "public").expect("walks");
            drop(held);
            read
        }
        const BODY: &str = "RETURNS TRIGGER LANGUAGE plpgsql AS $$\nBEGIN RETURN NEW; END;\n$$;\n";
        // One function, pinned by an `ALTER`, in each of the spellings the two
        // halves have to agree about.
        for (created, altered) in [
            ("archive.f()", "archive.f()"),
            ("archive.f()", "archive.f ()"),
            ("archive.f(a text)", "archive.f(a text)"),
            ("f(a text, b int)", "f(a text,  b int)"),
        ] {
            let body = format!(
                "CREATE FUNCTION {created} {BODY}ALTER FUNCTION {altered} \
                 SET search_path FROM CURRENT;\n"
            );
            let (findings, functions) = walked(&body);
            assert_eq!(
                functions, 1,
                "`{created}` and `{altered}` are one function and were read as \
                 {functions}: {findings:#?}"
            );
            assert!(
                findings.is_empty(),
                "`{created}` is pinned by `{altered}` and was reported: {findings:#?}"
            );
        }
        // And the `ALTER` that names nothing: the schema is on one and not the
        // other, which PostgreSQL resolves through `search_path` and this file
        // cannot. It is the `ALTER` that is named, not the `CREATE`.
        let (findings, functions) = walked(&format!(
            "CREATE FUNCTION f() {BODY}ALTER FUNCTION archive.f() SET search_path FROM CURRENT;\n"
        ));
        assert_eq!(functions, 1, "{findings:#?}");
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("no migration here creates")
                    && finding.what.contains("archive.f()")),
            "the `ALTER` names a function nothing creates and was filed as one: {findings:#?}"
        );
    }

    /// A quote in the signature does not open the body.
    ///
    /// Bracketing on whichever of `$` and `'` comes first, a parameter
    /// with a text default — ordinary SQL — brackets the declaration against a
    /// quote in the *signature* and folds the real `$tag$…$tag$` body into it.
    /// `declared.contains("set search_path")` is then satisfied by a string
    /// literal inside the function's own body: measured, `ok 4 function(s) in
    /// migrations pin how they resolve a name` while the database holds a null
    /// `proconfig`. The same defect as a pin inside a comment reading as a real
    /// pin, one quoting form over.
    #[test]
    fn a_quote_in_the_signature_does_not_open_the_body() {
        let held = crate::check::LeavesNothingBehind::new("bodyfn");
        let scratch = held.root.clone();
        for (pinned, function) in [
            // The pin is a string inside the body, and the signature has a
            // quote in front of it.
            (
                false,
                "CREATE FUNCTION probe(note TEXT DEFAULT '')\nRETURNS BIGINT\nLANGUAGE sql\n\
                 AS $probe$ SELECT 1 WHERE 'set search_path from current' <> note $probe$;",
            ),
            // The same function with a real pin, which is what the tree writes.
            (
                true,
                "CREATE FUNCTION probe(note TEXT DEFAULT '')\nRETURNS BIGINT\nLANGUAGE sql\n\
                 SET search_path FROM CURRENT\n\
                 AS $probe$ SELECT 1 WHERE '' <> note $probe$;",
            ),
        ] {
            let _ = std::fs::remove_dir_all(&scratch);
            std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
            std::fs::write(
                scratch.join("migrations/0001_probe.sql"),
                format!("{function}\n"),
            )
            .expect("writable");
            let (findings, _) = every_function_resolves_its_own_names(&scratch, "public")
                .expect("the scratch walks");
            assert_eq!(
                findings.is_empty(),
                pinned,
                "a pin is what the declaration says, not what the body quotes: {findings:?}"
            );
        }
    }

    /// Every clause `ALTER FUNCTION` has for lifting a pin lifts it here too.
    ///
    /// PostgreSQL's grammar for a configuration clause is `SET x TO v`,
    /// `SET x FROM CURRENT`, `RESET x` and `RESET ALL`. A rule knowing two of
    /// the three ways to lift one lets `RESET ALL` — which clears `proconfig`
    /// entirely and so lifts the pin without naming the path at all — print
    /// `ok 3 function(s) in migrations pin how they resolve a name`, measured. Only the
    /// database half objects, and `check --offline` is what `just check-offline`
    /// runs.
    #[test]
    fn every_way_of_lifting_a_pin_is_a_lifted_pin() {
        // The guard, not a `remove_dir_all` at the foot: an assertion below
        // panics, and a panic reaches neither.
        let held = crate::check::LeavesNothingBehind::new("liftfn");
        let scratch = held.root.clone();
        for (reported, lift) in [
            (false, ""),
            (true, "ALTER FUNCTION planted() RESET search_path;"),
            (true, "ALTER FUNCTION planted() RESET ALL;"),
            (true, "ALTER FUNCTION planted() SET search_path TO DEFAULT;"),
            // The one that is not a lift: it re-pins from the session, which is
            // what every function in this repository actually does.
            (
                false,
                "ALTER FUNCTION planted() SET search_path FROM CURRENT;",
            ),
        ] {
            let _ = std::fs::remove_dir_all(&scratch);
            std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
            std::fs::write(
                scratch.join("migrations/0001_probe.sql"),
                format!(
                    "CREATE FUNCTION planted() RETURNS TRIGGER LANGUAGE plpgsql\n\
                     SET search_path FROM CURRENT AS $$\nBEGIN RETURN NEW; END;\n$$;\n{lift}\n"
                ),
            )
            .expect("writable");
            let (findings, _) = every_function_resolves_its_own_names(&scratch, "public")
                .expect("the scratch walks");
            assert_eq!(
                findings.iter().any(|finding| finding.what.contains("pin")),
                reported,
                "`{lift}` should {} lift the pin: {findings:?}",
                if reported { "" } else { "not" }
            );
        }
    }

    /// A keyword on its own line is the same statement.
    ///
    /// **A parser and its independent count that both require a literal single
    /// space** miss the *same* statement when a line break follows `CREATE`, or
    /// falls between `OR REPLACE` and `FUNCTION` — and `found != creates`, the
    /// one thing standing between a partial parse and a green run, is satisfied
    /// by their agreeing about nothing. Measured over a scratch tree, with two
    /// such readers:
    ///
    /// ```text
    /// one line                                  count 1, parser 1, findings 1
    /// a break between OR REPLACE and FUNCTION         0,        0,          0
    /// a break straight after CREATE                   0,        0,          0
    /// ```
    ///
    /// `CREATE  FUNCTION` with two spaces is caught — by the mismatch, not by
    /// either reader — which is what makes such a pair look tested.
    #[test]
    fn a_keyword_on_its_own_line_is_still_a_function() {
        let scratch = std::env::temp_dir().join(format!("nc-wrapped-fn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("migrations")).expect("writable");
        for (at, opener) in [
            "CREATE FUNCTION",
            "CREATE\nFUNCTION",
            "CREATE OR REPLACE\nFUNCTION",
            "CREATE\n  OR REPLACE FUNCTION",
            "CREATE  FUNCTION",
            "CREATE\tPROCEDURE",
        ]
        .iter()
        .enumerate()
        {
            std::fs::write(
                scratch.join(format!("migrations/{:04}_probe.sql", at + 1)),
                format!(
                    "-- probe {at}\n{opener} planted{at}() RETURNS TRIGGER LANGUAGE plpgsql \
                     AS $$\nBEGIN RETURN NEW; END;\n$$;\n"
                ),
            )
            .expect("writable");
        }
        let written = 6;
        let counted = super::functions_the_migrations_create(&scratch).expect("the scratch walks");
        assert_eq!(
            counted, written,
            "the independent count read {counted} of {written} routine(s): a keyword on \
             its own line is the same statement, and this is the number that floors the \
             parser"
        );
        let (findings, seen) =
            super::every_function_resolves_its_own_names(&scratch, "public").expect("walks");
        assert_eq!(
            seen, written,
            "the parser read {seen} of {written} routine(s)"
        );
        assert_eq!(
            findings.len(),
            written,
            "every one of these pins nothing and {} was reported: {findings:#?}",
            findings.len()
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
