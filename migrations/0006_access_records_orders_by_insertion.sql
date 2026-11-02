-- The pruner's order is decided by the database, not by whoever writes a row.
--
-- `catalog/limits.toml` ordered `access_records` by `occurred_at` and recorded
-- the reason: the primary key is a Crockford identifier held as TEXT, and that
-- encoding is variable width — twelve characters below `32^12`, thirteen at or
-- above, from about 2034 — so a `text` sort stops being chronological. That is
-- true, and `occurred_at` does not fix it.
--
-- `occurred_at` is a BIGINT with no DEFAULT, no CHECK and no trigger: it is
-- whatever the caller supplies. Measured with the real pruner, ten rows written
-- in order with the last three from a device whose clock was a day slow:
--
--   PRUNED    removed 3
--   SURVIVING r01 r02 r03 r04 r05 r06 r07
--
-- The three newest rows in the table, deleted, with an ordinary `Pruned`
-- reported. The mirror image is worse — rows stamped a year ahead survive every
-- round, and the table converges on holding only the bogus ones.
--
-- So the ordering column is one the caller cannot write.
--
-- **`GENERATED ALWAYS AS IDENTITY` is not enough, and it was this file's first
-- answer.** It refuses a plain INSERT that names the column, which is what
-- `catalog/limits.toml` claimed — and two ordinary paths go straight round it.
-- Measured on PostgreSQL 18.6:
--
--   INSERT ... (id, occurred_at, recorded) VALUES ('a', 1, 42);
--     ERROR:  cannot insert a non-DEFAULT value into column "recorded"
--   INSERT ... OVERRIDING SYSTEM VALUE VALUES ('b', 1, -9223372036854775808);
--     INSERT 0 1
--   COPY access_records (id, occurred_at, recorded) FROM STDIN;
--     COPY 1                              -- no keyword needed at all
--
-- Through the real pruner, a row planted at `i64::MAX` sorts last under
-- `ORDER BY … ASC` on every future run and is **unprunable for ever**; enough of
-- them and the table can never be brought under `max_rows`, with every run
-- reporting ordinary. A row planted at `-1` consumes a slot of the batch that a
-- real row should have had. A bulk import is the ordinary way rows get loaded.
--
-- **A row trigger is the path nothing goes round.** It fires for `COPY` as well
-- as for `INSERT`, it sees the row before any constraint is checked, and
-- `ENABLE ALWAYS` makes it fire under `session_replication_role = replica` too —
-- which is the same reasoning, and the same three words, that
-- `0004_the_journal_refuses_replication_mode.sql` records for the journal.
--
-- What it does not survive is somebody with rights to `ALTER SEQUENCE … RESTART`
-- or to drop the trigger. That is the same trust boundary the journal's
-- append-only trigger sits on, and it is stated rather than papered over:
-- `ALTER TABLE access_records ALTER COLUMN recorded RESTART` on the identity
-- version gave two rows the same value and the pruner then deleted brand-new
-- rows. The sequence below is `NO CYCLE`, so exhaustion is an error and never a
-- wrap.
--
-- `journal_entries` already separates *when it happened* from *what orders it*,
-- with `sequence`; this is the same separation one table along. `occurred_at`
-- stays and keeps its meaning — when the access happened, which is what the two
-- GeoIP indexes are for. What it stops being is the pruner's idea of oldest.
--
-- **This migration refuses a table that already holds rows**, and that is the
-- honest half of adding an ordering column after the fact. There is nothing here
-- to derive a true insertion order from: `ADD COLUMN … GENERATED ALWAYS AS
-- IDENTITY` backfills in *heap* order, not insertion order — measured, ten rows
-- inserted in order with two of them updated first came back with the two oldest
-- carrying the two highest values — so the pruner would treat the oldest rows as
-- the newest. `access_records` is empty in the only place this has ever run.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM access_records) THEN
        RAISE EXCEPTION 'access_records already holds rows, and this migration has no '
            'true insertion order to give them: ADD COLUMN backfills in heap order, '
            'which the pruner would read as newest-first. Decide the order deliberately '
            '— by occurred_at, or by an export — and write that decision as its own '
            'migration before this one';
    END IF;
END
$$;

ALTER TABLE access_records ADD COLUMN recorded BIGINT NOT NULL;

CREATE SEQUENCE access_records_recorded_seq AS BIGINT NO CYCLE
    OWNED BY access_records.recorded;

CREATE FUNCTION access_records_recorded_is_the_databases() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.recorded := nextval('access_records_recorded_seq');
    ELSE
        -- An UPDATE may change anything else about a row and never this: the
        -- pruner's order is what the row's age *is*, and a row that can renumber
        -- itself can make itself immortal.
        NEW.recorded := OLD.recorded;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER access_records_recorded_is_assigned
    BEFORE INSERT OR UPDATE ON access_records
    FOR EACH ROW EXECUTE FUNCTION access_records_recorded_is_the_databases();

ALTER TABLE access_records ENABLE ALWAYS TRIGGER access_records_recorded_is_assigned;

-- Without this, `ORDER BY recorded ASC LIMIT 50000` is a sequential scan and an
-- external merge sort. Measured on 200,000 rows: 125 ms and 6.7 MB spilled to
-- temp files, against 80 ms and no temp I/O for the indexed primary key — and at
-- the declared `max_rows` of 500,000 that spill happens on every scheduled run,
-- now inside a transaction holding row locks.
CREATE INDEX access_records_recorded ON access_records (recorded);
