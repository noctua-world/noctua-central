-- The journal on the person's machine (NOCTUA.md §14.9, §11.5): the
-- hash-chained record of every command the kernels of this profile decided,
-- one history for every realm the file holds, as the server's
-- `journal_entries` is one history for every realm it holds.
--
-- **`local_journal_entries`, never `journal_entries`**, so no reader, gate or
-- person takes one table for the other: the server's is PostgreSQL's and is
-- built by `migrations/`, this one is SQLite's and is built here.
--
-- **The columns are the server's**, with its meanings, so a row reads back as
-- the same `nc_journal::Entry` from either store. Two differ in type and not in
-- what they hold: `actor_chain` is the canonical JSON array of identifiers,
-- SQLite having no `jsonb`, and every integer is SQLite's own. The bounds the
-- server's migrations add are here from the first: a sequence that is not
-- negative and ends far from the top (`0003`, `0008`), and a hash and a link of
-- sixty-four lowercase hex characters (`0009`) — so a row planted from another
-- connection is refused where it would otherwise end the history.
--
-- **Append-only, by the table.** `BEFORE UPDATE` and `BEFORE DELETE` refuse
-- every statement that would change or remove a row, whoever runs it, and a
-- `DELETE` with no `WHERE` meets them too: a table with a delete trigger is
-- never emptied by SQLite's truncate optimisation. `RAISE(ABORT)` holds under
-- `trusted_schema = OFF`, which `crates/nc-local` sets, and SQLite answers it
-- with the extended code 1811. What no trigger answers is a process running as
-- the same user, which can `DROP TRIGGER`: SQLite has no roles
-- (`crates/nc-local`'s header).
--
-- **The link is stated inside the insert.** `crates/nc-local` appends with one
-- `INSERT … SELECT … WHERE` the tail's sequence and hash are the ones the entry
-- names, and its unit's brackets allow it, so the check and the write are one
-- statement — the argument `crates/nc-journal/src/postgres.rs` makes, in the
-- one transaction a writer holds here.
--
-- `catalog/limits.toml` says `policy-pack`: the journal is the audit log, and
-- its retention is the realm's pack's to state, never a number here. Until a
-- pack states one this table only grows, as the server's does, and the trigger
-- above refuses the `DELETE` a pruner would send.
CREATE TABLE local_journal_entries (
    sequence        INTEGER PRIMARY KEY
                            CHECK (sequence >= 0 AND sequence < 4611686018427387904),
    realm           TEXT    NOT NULL,
    principal       TEXT    NOT NULL,
    actor_chain     TEXT    NOT NULL CHECK (json_valid(actor_chain)),
    action          TEXT    NOT NULL,
    resource        TEXT    NOT NULL,
    result          TEXT    NOT NULL,
    deciding_policy TEXT,
    policy_pack     TEXT,
    policy_epoch    INTEGER CHECK (policy_epoch >= 0),
    grant_used      TEXT,
    work_unit       TEXT,
    recorded_at     INTEGER NOT NULL CHECK (recorded_at >= 0),
    prev_hash       TEXT    CHECK (prev_hash IS NULL
                                   OR (length(prev_hash) = 64
                                       AND prev_hash NOT GLOB '*[^0-9a-f]*')),
    hash            TEXT    NOT NULL
                            CHECK (length(hash) = 64 AND hash NOT GLOB '*[^0-9a-f]*')
) STRICT;

-- What a unit's brackets are asked by: whether one is open, how it closed, and
-- which a stopped process left open — every question about a unit names the
-- realm and the unit, so none of them reads the history.
CREATE INDEX local_journal_entries_by_unit ON local_journal_entries (realm, work_unit);

CREATE TRIGGER local_journal_entries_are_never_edited BEFORE UPDATE ON local_journal_entries
BEGIN
    SELECT RAISE(ABORT, 'local_journal_entries: the journal is append-only, and an entry is never edited');
END;

CREATE TRIGGER local_journal_entries_are_never_deleted BEFORE DELETE ON local_journal_entries
BEGIN
    SELECT RAISE(ABORT, 'local_journal_entries: the journal is append-only, and an entry is never deleted');
END;
