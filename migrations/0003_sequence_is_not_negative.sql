-- A sequence is never negative, and nothing said so.
--
-- `sequence` is a BIGINT because PostgreSQL has no unsigned integer, and the
-- column carries a `u64`. Without this a second connection can INSERT a row at
-- sequence -1 — the append-only trigger guards UPDATE and DELETE and says
-- nothing about what an INSERT may contain — and that row is invisible to every
-- read and to verification: `from` filters `sequence >= $1` and `tail` orders
-- descending, so both step over it.
--
-- Measured before this landed: a row recording `Escalate` by `attacker` sat in
-- the table, could never be removed, and `verify(0)` called the history clean.
--
-- Its own migration rather than an edit to 0002, because a migration that has
-- been applied is not a file to change — `cargo xtask dev` refuses one, which
-- is how this rule was met.
ALTER TABLE journal_entries
    ADD CONSTRAINT journal_entries_sequence_is_not_negative CHECK (sequence >= 0);
