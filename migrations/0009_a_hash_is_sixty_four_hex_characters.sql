-- A `hash` this code cannot read locks the journal for ever, and the repair
-- this repository documents cannot be performed.
--
-- `0001` declares `hash TEXT NOT NULL` with no constraint, so one INSERT with
-- `hash = 'not-a-hash'` — the privilege the writing service already has — makes
-- every read of the tail answer `Undecodable`. Measured:
--
--   head()          -> Undecodable  NC-JRNL-0001
--   tail()          -> Undecodable  NC-JRNL-0001
--   Journal::record -> Undecodable  NC-JRNL-0001, critical, retryable = false
--   DELETE / UPDATE / TRUNCATE       -> refused by this table's own triggers
--   SET session_replication_role     -> still refused (ENABLE ALWAYS)
--
-- `crates/nc-journal/src/lib.rs` says an operator reads the unreadable row and
-- appends an anchor through `Store::append`. That is impossible for exactly
-- this row, twice over: the refusal path reads the tail before it compares the
-- link, and `Entry::previous` is 32 bytes rendered as lowercase hex, so the
-- statement's `$14 IS NOT DISTINCT FROM 'not-a-hash'` can never hold. Measured
-- with a *hex* forged hash the same anchor lands and the journal recovers — so
-- the door exists, and was closed only for the case the fail-closed decision
-- was taken about.
--
-- §14.9 fails closed, so this is every command that must journal itself,
-- stopped, permanently, with no repair short of dropping the append-only
-- trigger. The column now refuses the row instead.
--
-- `prev_hash` carries the same shape and is nullable: the genesis entry has no
-- predecessor, and every other row's link is compared against a hash that is
-- already constrained here.
ALTER TABLE journal_entries
    ADD CONSTRAINT journal_entries_hash_is_hex
    CHECK (hash ~ '^[0-9a-f]{64}$');

ALTER TABLE journal_entries
    ADD CONSTRAINT journal_entries_prev_hash_is_hex
    CHECK (prev_hash IS NULL OR prev_hash ~ '^[0-9a-f]{64}$');
