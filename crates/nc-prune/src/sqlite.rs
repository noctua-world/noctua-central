//! The pruner against the local database: the SQLite file `crates/nc-local`
//! opens on the user's machine.
//!
//! The same two statements as [`crate::postgres`] — a count, and a `DELETE` of
//! the oldest rows chosen by key — against the tables `catalog/limits.toml`
//! declares with `database = "local"`. The kernel's `pump` runs [`run`](crate::run)
//! over [`Sqlite`], so `§15.8`'s one pruner still bounds every table there is.
//!
//! # The identifier is written, not bound, and so it is checked
//!
//! As in PostgreSQL, a table and a column cannot be parameters. Every name here
//! comes from `nc_catalog::TABLES`; `identifier` refuses anything but a plain
//! lowercase identifier and then quotes it, and every statement names `main.`,
//! because a temporary table of the same name shadows `main` for an
//! unqualified one on the connection that made it.
//!
//! # What a `DELETE` may reach, and what it may not
//!
//! Refused before anything is deleted: a name that is not an ordinary table (a
//! view, a virtual table), a temporary object of that name or on that table, a
//! trigger that fires on `DELETE`, a table another references with
//! `ON DELETE SET NULL` or `SET DEFAULT` — each changes rows the count below
//! would not see — and a connection whose foreign keys are off. **Each of the
//! first three is asked of every table a declared cascade reaches from the
//! table**, recursively, because the `DELETE` takes rows from all of them.
//!
//! **A cascade is not refused, and that is the one difference from
//! PostgreSQL.** `store_writes` and `store_notes` reference `store_units` with
//! `ON DELETE CASCADE` on purpose: a unit's images go with it, and
//! `catalog/limits.toml` bounds each of the two per unit, so the bound on
//! `store_units` is what bounds them. The schema those cascades
//! are in is the one `nc-local` compares with its list every time it opens the
//! file, so a cascade here is one the schema declares — and the foreign keys
//! must be on for it to run at all, which is why a connection with them off is
//! refused: the parent would go and its children would stay, orphaned and
//! uncounted.
//!
//! `rows_affected` counts the rows deleted from the table itself, never a
//! cascade's, so the over-deletion guard holds the batch to what was asked of
//! this table.
//!
//! # A row another table holds is skipped
//!
//! **A reference `ON DELETE RESTRICT` or `NO ACTION` holds the row it names**,
//! and the oldest rows chosen are the oldest no such reference holds: deleting
//! one SQLite would refuse is a prune refused whole, and the row is not the
//! pruner's to take. `store_unit_holds` is the one this schema declares: a
//! kernel holds each unit whose close it has not heard the journal take, and
//! each unit whose state that one moves, because the kernels of the realms one
//! person belongs to share this file and the pruner takes the oldest units of
//! every realm — and a reviewer found the record of a unit one realm's stopped
//! process left open taken by another realm's prune, its recovery then closing
//! the unit as aborted with its writes left in place. A reference of more than
//! one column is refused rather than read.

use sqlx::Row as _;
use sqlx::sqlite::SqlitePool;

use crate::Target;

/// The tables in one local database.
#[derive(Debug, Clone)]
pub struct Sqlite {
    pool: SqlitePool,
}

impl Sqlite {
    /// A target over an existing pool — `nc_local::LocalStore::pool`'s.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The pool it prunes through.
    #[must_use]
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// A name safe to write into a statement, quoted — or why it is not one.
///
/// **One part, not two**: the local database has one schema that holds tables,
/// `main`, and every statement here names it; a name with a schema of its own
/// is one the catalog did not write for this database.
fn identifier(name: &str) -> Result<String, String> {
    let shaped = !name.is_empty()
        && name.len() <= 63
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if shaped {
        Ok(format!("\"{name}\""))
    } else {
        Err(format!(
            "`{name}` is not a plain identifier, and this writes it into the statement \
             rather than binding it. Every name here comes from catalog/limits.toml \
             through the compiler, so one that does not look like it did was not \
             written there"
        ))
    }
}

/// The primary result codes that mean *the work was not done, and the next tick
/// may do it*: `SQLITE_BUSY`, another connection writing past the pool's busy
/// timeout, and `SQLITE_LOCKED`, a conflict inside this one.
const WAITED_RATHER_THAN_REFUSED: [u32; 2] = [5, 6];

/// One database error, carrying whether it is worth another tick.
fn said(doing: &str, error: &sqlx::Error) -> crate::Failure {
    let Some(database) = error.as_database_error() else {
        return crate::Failure {
            why: format!("{doing}: {error}"),
            waited: false,
            answered: false,
        };
    };
    let waited = database
        .code()
        .and_then(|code| code.parse::<u32>().ok())
        .is_some_and(|code| WAITED_RATHER_THAN_REFUSED.contains(&(code & 0xff)));
    crate::Failure {
        why: format!("{doing}: {}", database.message()),
        waited,
        answered: true,
    }
}

/// Why the table cannot be pruned safely, or `None` when it can.
///
/// Asked on the connection the statement will run on, inside its transaction,
/// because a temporary object belongs to one connection and no other sees it.
///
/// **Of every table the `DELETE` reaches, not of the table alone.** A declared
/// `ON DELETE CASCADE` takes rows from the children too, so a trigger on a
/// child, a temporary object watching one, and a reference that rewrites a
/// child's referrers reach as far as the same on the table itself. Measured
/// when only the table was asked: `children` cascading from `parents`, a
/// trigger `AFTER DELETE ON children BEGIN DELETE FROM bystanders; END`, and the
/// prune of `parents` succeeded with `bystanders` emptied — rows outside the
/// table counted, and no refusal.
async fn beyond_reach(
    connection: &mut sqlx::SqliteConnection,
    table: &str,
) -> Result<Option<String>, crate::Failure> {
    let kind: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT type, sql FROM main.sqlite_schema WHERE name = ?1")
            .bind(table)
            .fetch_optional(&mut *connection)
            .await
            .map_err(|error| said("reading the schema", &error))?;
    match kind {
        None => return Ok(Some("the local database holds no such table".to_owned())),
        Some((kind, sql)) => {
            let virtual_table = sql.as_deref().is_some_and(is_a_virtual_table);
            if kind != "table" || virtual_table {
                return Ok(Some(
                    "it is not an ordinary table — a view and a virtual table answer to a \
                     name and neither is what this crate prunes"
                        .to_owned(),
                ));
            }
        }
    }
    for reached in reached_by_a_delete_from(connection, table).await? {
        let it = if reached == table {
            "it".to_owned()
        } else {
            format!("`{reached}` (a cascade from it reaches that table)")
        };
        let temporary: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM temp.sqlite_schema \
             WHERE name = ?1 COLLATE NOCASE OR tbl_name = ?1 COLLATE NOCASE",
        )
        .bind(&reached)
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| said("reading the temporary schema", &error))?;
        if temporary > 0 {
            return Ok(Some(format!(
                "a temporary object on this connection is named for or watches {it}, and \
                 a temporary table shadows the real one while a temporary trigger decides \
                 what a `DELETE` does"
            )));
        }
        let triggers: Vec<String> = sqlx::query_scalar(
            "SELECT sql FROM main.sqlite_schema \
             WHERE type = 'trigger' AND tbl_name = ?1 COLLATE NOCASE",
        )
        .bind(&reached)
        .fetch_all(&mut *connection)
        .await
        .map_err(|error| said("reading the schema", &error))?;
        if triggers.iter().any(|sql| fires_on_delete(sql)) {
            return Ok(Some(format!(
                "a trigger on {it} fires on DELETE and may touch another table"
            )));
        }
        let rewriting: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM main.sqlite_schema AS m, \
             pragma_foreign_key_list(m.name, 'main') AS f \
             WHERE m.type = 'table' AND f.\"table\" = ?1 COLLATE NOCASE \
             AND f.on_delete IN ('SET NULL', 'SET DEFAULT')",
        )
        .bind(&reached)
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| said("reading the foreign keys", &error))?;
        if rewriting > 0 {
            return Ok(Some(format!(
                "another table references {it} with ON DELETE SET NULL or SET DEFAULT, so \
                 deleting a row there changes rows elsewhere"
            )));
        }
    }
    let enforced: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| said("reading the connection's foreign keys", &error))?;
    if enforced != 1 {
        return Ok(Some(
            "foreign keys are off on this connection, so a cascade the schema declares \
             would not run and a deleted row's children would stay behind"
                .to_owned(),
        ));
    }
    Ok(None)
}

/// Every table a `DELETE` from `table` takes rows from: the table, and each
/// table an `ON DELETE CASCADE` reaches from it, followed as far as it goes.
///
/// **Recursive, because a cascade is.** A grandchild that cascades from a child
/// loses rows to a prune of the parent as surely as the child does. `UNION`
/// rather than `UNION ALL`, so a cycle of references ends where it began.
async fn reached_by_a_delete_from(
    connection: &mut sqlx::SqliteConnection,
    table: &str,
) -> Result<Vec<String>, crate::Failure> {
    sqlx::query_scalar(
        "WITH RECURSIVE reached(name) AS ( \
             SELECT ?1 \
             UNION \
             SELECT m.name FROM reached, main.sqlite_schema AS m, \
                 pragma_foreign_key_list(m.name, 'main') AS f \
             WHERE m.type = 'table' AND f.\"table\" = reached.name COLLATE NOCASE \
             AND f.on_delete = 'CASCADE' \
         ) SELECT name FROM reached",
    )
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| said("following the cascades", &error))
}

/// A statement's bare words, upper-cased, for reading its shape.
///
/// **What SQLite reads as keywords, and nothing else.** `sqlite_schema` keeps
/// a trigger's SQL as it was written, comments included, and the reader this
/// replaced split on whitespace: `AFTER DELETE/**/ON` and `AFTER DELETE --x`
/// followed by `ON` on the next line held no ` DELETE ON `, and a prune ran
/// through that trigger and emptied a table it was never asked about —
/// measured, three bystanders before and none after. So a `--` or `/* … */`
/// comment, a `'…'` literal and a name quoted with `"…"`, `[…]` or `` `…` `` are
/// each a gap, and a word is a run of letters, digits, `_` and `$` between
/// them. A quoted name is never a keyword, which is why it is a gap and not a
/// word: a trigger called `"on"` is not the word `ON`.
fn bare_words(sql: &str) -> Vec<String> {
    let characters: Vec<char> = sql.chars().collect();
    let mut found = Vec::new();
    let mut word = String::new();
    let mut at = 0;
    while at < characters.len() {
        let here = characters[at];
        let next = characters.get(at + 1).copied();
        // Where a gap that is not a single character ends, past its closing.
        let skip_to = match (here, next) {
            ('-', Some('-')) => Some(gap_until(&characters, at + 2, &['\n'], false)),
            ('/', Some('*')) => Some(gap_until(&characters, at + 2, &['*', '/'], false)),
            ('\'', _) => Some(gap_until(&characters, at + 1, &['\''], true)),
            ('"', _) => Some(gap_until(&characters, at + 1, &['"'], true)),
            ('`', _) => Some(gap_until(&characters, at + 1, &['`'], true)),
            ('[', _) => Some(gap_until(&characters, at + 1, &[']'], false)),
            _ => None,
        };
        if here.is_alphanumeric() || here == '_' || here == '$' {
            word.push(here.to_ascii_uppercase());
            at += 1;
            continue;
        }
        if !word.is_empty() {
            found.push(core::mem::take(&mut word));
        }
        at = skip_to.unwrap_or(at + 1);
    }
    if !word.is_empty() {
        found.push(word);
    }
    found
}

/// The position just past the first `closing` at or after `from`, or the end.
///
/// `doubled` is SQLite's escape for a quote inside its own kind of quote —
/// `''` and `""` and two backticks — which does not close it.
fn gap_until(characters: &[char], from: usize, closing: &[char], doubled: bool) -> usize {
    let mut at = from;
    while at < characters.len() {
        if characters[at..].starts_with(closing) {
            let past = at + closing.len();
            if doubled && characters[past..].starts_with(closing) {
                at = past + closing.len();
                continue;
            }
            return past;
        }
        at += 1;
    }
    characters.len()
}

/// Whether a table's SQL is a virtual table's.
///
/// **This was a branch nothing entered.** It asked whether the words began
/// `"CREATE VIRTUAL TABLE "`, and the words it read began with a space — so an
/// FTS5 table named for a declared one was counted and pruned as an ordinary
/// table, measured. A virtual table's rows live in its module, and in shadow
/// tables the catalog names nowhere.
fn is_a_virtual_table(sql: &str) -> bool {
    bare_words(sql).starts_with(&[
        "CREATE".to_owned(),
        "VIRTUAL".to_owned(),
        "TABLE".to_owned(),
    ])
}

/// Whether a trigger's SQL fires on `DELETE`.
///
/// **Its event is the word before the first bare `ON`.** SQLite's grammar is
/// `CREATE [TEMP] TRIGGER [IF NOT EXISTS] name [BEFORE | AFTER | INSTEAD OF]
/// event ON table …`, so the event sits between the name and that `ON`; and
/// `ON` is reserved — an unquoted `on` as a name is a syntax error, measured on
/// 3.51 — while `BEGIN` is not, so a trigger may be called `begin`, and the
/// first `BEGIN` is not where the header ends. A `DELETE` in the body is a
/// statement the trigger runs, which is not what decides when it fires.
fn fires_on_delete(sql: &str) -> bool {
    bare_words(sql)
        .iter()
        .take_while(|word| *word != "ON")
        .any(|word| word == "DELETE")
}

/// The table's primary key, quoted and joined — or why there is none.
async fn primary_key_of(
    connection: &mut sqlx::SqliteConnection,
    table: &str,
) -> Result<String, crate::Failure> {
    let rows =
        sqlx::query("SELECT name FROM pragma_table_info(?1, 'main') WHERE pk > 0 ORDER BY pk")
            .bind(table)
            .fetch_all(&mut *connection)
            .await
            .map_err(|error| said("reading the primary key", &error))?;
    let mut names = Vec::with_capacity(rows.len());
    for row in &rows {
        let name: String = row
            .try_get("name")
            .map_err(|error| said("reading the primary key", &error))?;
        names.push(identifier(&name).map_err(crate::Failure::refused)?);
    }
    if names.is_empty() {
        return Err(crate::Failure::refused(format!(
            "`{table}` has no primary key, and this pruner deletes rows by theirs. Give \
             the table one"
        )));
    }
    Ok(names.join(", "))
}

/// The condition that leaves out of a prune of `table` every row a reference
/// holds — one `NOT EXISTS` for each table that references it `ON DELETE
/// RESTRICT` or `NO ACTION` — as a `WHERE` clause of the rows chosen, or none
/// when nothing references it so; `key` is the table's own primary key, quoted.
///
/// # Errors
///
/// A refusal when a reference is of more than one column, or a name in one is
/// not a plain identifier; and what SQLite answered when the schema could not
/// be read.
async fn unheld(
    connection: &mut sqlx::SqliteConnection,
    table: &str,
    key: &str,
) -> Result<String, crate::Failure> {
    let references: Vec<(String, i64, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT m.name, f.id, f.seq, f.\"from\", f.\"to\" FROM main.sqlite_schema AS m, \
         pragma_foreign_key_list(m.name, 'main') AS f \
         WHERE m.type = 'table' AND f.\"table\" = ?1 COLLATE NOCASE \
         AND f.on_delete IN ('RESTRICT', 'NO ACTION') ORDER BY m.name, f.id, f.seq",
    )
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| said("reading what holds the table's rows", &error))?;
    let mut held = Vec::with_capacity(references.len());
    for (holder, _, seq, from, to) in references {
        if seq > 0 {
            return Err(crate::Failure::refused(format!(
                "`{holder}` holds rows of `{table}` by a reference of more than one column, \
                 which this pruner does not read"
            )));
        }
        let to = match to {
            Some(column) => identifier(&column).map_err(crate::Failure::refused)?,
            None => key.to_owned(),
        };
        held.push(format!(
            "NOT EXISTS (SELECT 1 FROM main.{} AS held WHERE held.{} = pruned.{to})",
            identifier(&holder).map_err(crate::Failure::refused)?,
            identifier(&from).map_err(crate::Failure::refused)?,
        ));
    }
    Ok(if held.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", held.join(" AND "))
    })
}

/// A refusal of a table this crate will not prune, with its code.
fn unreachable(table: &str, why: &str) -> crate::Failure {
    crate::Failure::refused(format!(
        "`{table}` cannot be pruned safely: {why}. A `DELETE` here would reach rows this \
         crate was not asked about, and `rows_affected` would not count them — so it is \
         refused rather than run"
    ))
}

impl Target for Sqlite {
    fn database(&self) -> nc_catalog::Database {
        nc_catalog::Database::LocalFile
    }

    async fn count(&mut self, table: &str) -> Result<u64, crate::Failure> {
        let quoted = identifier(table)?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| said("could not begin", &error))?;
        if let Some(why) = beyond_reach(&mut transaction, table).await? {
            return Err(unreachable(table, &why));
        }
        let held: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM main.{quoted}"
        )))
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| said("could not count", &error))?;
        transaction
            .commit()
            .await
            .map_err(|error| said("could not finish counting", &error))?;
        u64::try_from(held)
            .map_err(|_| crate::Failure::refused(format!("`{table}` counted {held} rows")))
    }

    async fn delete_oldest(
        &mut self,
        table: &str,
        order_by: &str,
        batch: u64,
    ) -> Result<u64, crate::Failure> {
        crate::the_catalog_orders_its_own_tables(table, order_by, self.database())?;
        let quoted = identifier(table)?;
        let ordered = identifier(order_by)?;
        let limit = i64::try_from(batch).map_err(|_| {
            crate::Failure::refused(format!(
                "`{table}`: a batch of {batch} is more rows than SQLite can be asked for"
            ))
        })?;
        // **`IMMEDIATE`, so the guard and the `DELETE` see one database.** A
        // deferred transaction takes its write lock at the `DELETE`, and a
        // writer between the guard and it could change what the guard read.
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("could not begin", &error))?;
        if let Some(why) = beyond_reach(&mut transaction, table).await? {
            return Err(unreachable(table, &why));
        }
        let key = primary_key_of(&mut transaction, table).await?;
        let unheld = unheld(&mut transaction, table, &key).await?;
        let removed = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM main.{quoted} WHERE ({key}) IN \
             (SELECT {key} FROM main.{quoted} AS pruned{unheld} \
              ORDER BY {ordered} ASC, {key} ASC LIMIT ?1)"
        )))
        .bind(limit)
        .execute(&mut *transaction)
        .await
        .map_err(|error| said("could not prune", &error))?
        .rows_affected();
        if let Some(why) = crate::postgres::over_deleted(removed, batch) {
            transaction.rollback().await.map_err(|error| {
                crate::Failure::refused(format!(
                    "`{table}`: {why}, and rolling that back failed: {error}"
                ))
            })?;
            return Err(crate::Failure::refused(format!("`{table}`: {why}")));
        }
        transaction
            .commit()
            .await
            .map_err(|error| said("could not commit the pruning", &error))?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::{bare_words, fires_on_delete, identifier, is_a_virtual_table, said};

    /// A name the catalog did not write is refused, and one it did is quoted.
    #[test]
    fn a_name_the_catalog_did_not_write_is_refused() {
        assert_eq!(identifier("store_units"), Ok("\"store_units\"".to_owned()));
        assert_eq!(identifier("_x9"), Ok("\"_x9\"".to_owned()));
        for name in [
            "",
            "main.store_units",
            "Store",
            "9lives",
            "a b",
            "a\"b",
            "a;b",
            &"a".repeat(64),
        ] {
            assert!(identifier(name).is_err(), "{name:?} was accepted");
        }
        assert!(
            identifier(&"a".repeat(63)).is_ok(),
            "the longest name is refused"
        );
    }

    /// A failure with no database answer is one the next tick may not meet.
    #[test]
    fn a_failure_says_whether_the_database_answered() {
        let failure = said("counting", &sqlx::Error::PoolTimedOut);
        assert!(!failure.answered && !failure.waited, "{failure:?}");
        assert_eq!(failure.code(), nc_catalog::ErrorCode::DbUnreachable);
    }

    /// A statement is read as SQLite reads its keywords: in one case, with
    /// comments, literals and quoted names as gaps.
    #[test]
    fn a_statement_is_read_for_its_keywords_and_not_its_comments_or_quotes() {
        assert_eq!(
            bare_words("create /* a ON b */ trigger -- ON\n\"on\" [on] `o``n` 'it''s' t$1"),
            ["CREATE", "TRIGGER", "T$1"]
        );
        // `/**/` and a line end are both gaps between two words.
        assert_eq!(bare_words("DELETE/**/ON"), ["DELETE", "ON"]);
    }

    /// A trigger fires on `DELETE` when the word stands before its first bare
    /// `ON`, however it is spaced, commented or named.
    #[test]
    fn a_trigger_fires_on_delete_by_its_event_and_not_by_its_body() {
        for fires in [
            "create trigger t after\n\tdelete   on x begin end",
            "CREATE TRIGGER t AFTER DELETE/**/ON x BEGIN SELECT 1; END",
            "CREATE TRIGGER t AFTER DELETE --x\nON x BEGIN SELECT 1; END",
            "CREATE TRIGGER t AFTER /* ON */ DELETE ON x BEGIN SELECT 1; END",
            "CREATE TRIGGER t AFTER -- ON\nDELETE ON x BEGIN SELECT 1; END",
            "CREATE TRIGGER \"on\" AFTER DELETE ON x BEGIN SELECT 1; END",
            "CREATE TRIGGER [on] AFTER DELETE ON x BEGIN SELECT 1; END",
            "CREATE TRIGGER `on` AFTER DELETE ON x BEGIN SELECT 1; END",
            "CREATE TRIGGER begin AFTER DELETE ON x BEGIN SELECT 1; END",
            "CREATE TRIGGER t INSTEAD OF DELETE ON x BEGIN SELECT 1; END",
        ] {
            assert!(fires_on_delete(fires), "{fires}");
        }
        for silent in [
            "CREATE TRIGGER t BEFORE UPDATE ON deleted_on BEGIN SELECT 1; END",
            "CREATE TRIGGER t AFTER INSERT ON x BEGIN DELETE FROM y; END",
            "CREATE TRIGGER \"delete\" AFTER UPDATE OF a ON x BEGIN SELECT 1; END",
            "CREATE TRIGGER t AFTER INSERT ON x WHEN new.a = 'delete' BEGIN SELECT 1; END",
        ] {
            assert!(!fires_on_delete(silent), "{silent}");
        }
    }

    /// A virtual table is told from an ordinary one by its own words.
    #[test]
    fn a_virtual_table_is_told_from_an_ordinary_one() {
        assert!(is_a_virtual_table(
            "CREATE VIRTUAL TABLE rows_over_time USING fts5(id, at)"
        ));
        assert!(is_a_virtual_table(
            "CREATE /* x */ VIRTUAL TABLE t USING rtree(id, a, b)"
        ));
        assert!(!is_a_virtual_table(
            "CREATE TABLE \"virtual\" (id INTEGER PRIMARY KEY)"
        ));
        assert!(!is_a_virtual_table(
            "CREATE TABLE t (id INTEGER PRIMARY KEY)"
        ));
    }
}
