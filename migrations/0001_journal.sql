-- The command journal (NOCTUA.md §14.9).
--
-- It is the audit log. There is not a second one, and building one would split
-- the answer to "what happened" across two stores that disagree.
--
-- Append-only is enforced *here* as well as in Rust. A role that can UPDATE or
-- DELETE a journal row can rewrite history, and the application is not the only
-- thing that ever holds a connection to this database.
--
-- There is deliberately no work-units table. A unit is bracketed by two entries
-- in this one — an opening entry carrying its label, a closing entry saying
-- whether it committed — so a second table would be a second place to look for
-- the same fact. A group whose closing entry never arrives is one whose process
-- stopped, and its absence says so.

CREATE TABLE journal_entries (
    -- Dense and gapless: it is the chain's position, not an identifier. A gap
    -- would be indistinguishable from a deletion.
    sequence          BIGINT      PRIMARY KEY,

    -- Every entity belongs to exactly one realm (§10.3), and this table's
    -- retention is a policy-pack property per realm (§15.8) — which a pruner
    -- cannot apply to a row that does not name its realm.
    realm             TEXT        NOT NULL,

    -- Who acted, and on whose behalf. `actor_chain` is delegation, never
    -- impersonation: the log always shows both (§14.6).
    principal         TEXT        NOT NULL,
    actor_chain       JSONB       NOT NULL,

    -- What they did, and to what. `resource` is canonical JSON (§16.4), because
    -- it is whatever the command's resource selector extracted and the chain
    -- hashes it reproducibly.
    action            TEXT        NOT NULL,
    resource          TEXT        NOT NULL,
    result            TEXT        NOT NULL,

    -- Which policy decided, and the state it decided from. Every one of these is
    -- required by §14.9 and populated by the enforcement point from the kernel
    -- on; they exist now because adding a field to a chained structure later
    -- invalidates every hash before it.
    deciding_policy   TEXT,
    policy_pack       TEXT,
    policy_epoch      BIGINT,
    grant_used        TEXT,

    work_unit         TEXT,
    recorded_at       BIGINT      NOT NULL,

    -- The chain. `prev_hash` is NULL only at sequence 0, and both are lowercase
    -- hex rather than BYTEA so that a person reading a row sees what a person
    -- reading a log line sees.
    prev_hash         TEXT,
    hash              TEXT        NOT NULL
);

CREATE INDEX journal_entries_realm ON journal_entries (realm, sequence);
CREATE INDEX journal_entries_work_unit ON journal_entries (work_unit);
CREATE INDEX journal_entries_recorded_at ON journal_entries (recorded_at);

CREATE FUNCTION journal_is_append_only() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION
        'the journal is append-only: % on % was refused', TG_OP, TG_TABLE_NAME
        USING HINT = 'audit that can be edited is not audit (NOCTUA.md §14.9)';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER journal_entries_no_update
    BEFORE UPDATE ON journal_entries
    FOR EACH ROW EXECUTE FUNCTION journal_is_append_only();

CREATE TRIGGER journal_entries_no_delete
    BEFORE DELETE ON journal_entries
    FOR EACH ROW EXECUTE FUNCTION journal_is_append_only();

-- **How retention and append-only both hold.** §15.8 makes this table's
-- retention a policy-pack property, and the trigger above refuses every DELETE —
-- which reads like a contradiction and is not. Pack-driven retention, when the
-- stage that reads packs arrives, drops a *partition*: `DROP TABLE` on a
-- partition is DDL, so it never fires a row trigger, it cannot remove a single
-- row selectively, and it leaves a migration behind as its own record.
--
-- That is the difference the rule is protecting. Deleting rows is editing
-- history; ageing out a whole time range that a pack said to age out is
-- retention, and it is visible as a schema change rather than invisible as a
-- statement. This table is not partitioned yet because nothing prunes it yet,
-- and partitioning it before then would be a shape chosen with no measurement
-- behind it.
--
-- **What that answer does not yet reach**, said here because this is the
-- paragraph a reader will trust:
--
--   * §15.8 makes retention **per realm**, and `realm` exists above for exactly
--     that. Dropping a time-range partition ages out every realm at once. Per
--     realm needs partitioning by `(realm, time)`, and PostgreSQL requires the
--     partition key in every unique constraint — so `PRIMARY KEY (sequence)`
--     alone cannot survive it, and the chain's dense, gapless sequence changes
--     shape. That is a decision with a measurement behind it, and neither exists
--     yet.
--   * A row trigger does not fire on `TRUNCATE`, and the table's owner can
--     `ALTER TABLE … DISABLE TRIGGER`. The first is closed by
--     `0005_the_journal_refuses_truncate.sql`, which is the `BEFORE TRUNCATE …
--     FOR EACH STATEMENT` trigger this line asked for; the second is a
--     privilege question, not a schema one.
--
-- None of this is Stage 10's to solve. All of it is Stage 10's to write down.
