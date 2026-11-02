-- A trigger function names its own table, and not whichever one is in front.
--
-- **`pg_temp` is searched before `search_path`, and two of these functions read
-- an unqualified name.** PostgreSQL resolves a relation name by looking in the
-- temporary schema first — before `search_path` is consulted at all — and every
-- role has `CREATE TEMP TABLE` by default. So one ordinary statement from the
-- writing service's own connection decides what the trigger sees:
--
--     INSERT INTO journal_entries (...) VALUES (0, ...);          -- INSERT 0 1
--     CREATE TEMP TABLE journal_entries (sequence BIGINT);
--     INSERT INTO pg_temp.journal_entries VALUES (4611686018427387902);
--     INSERT INTO public.journal_entries (...) VALUES (4611686018427387903, 'attacker', ...);
--     INSERT 0 1
--
-- The trigger asked the temp table for `MAX(sequence)`, got a number one below
-- the leap, and let the row through. `SHOW search_path` was the default the
-- whole time and no DDL touched `journal_entries`. The history is then over:
--
--     ERROR:  the journal is dense: sequence 1 is not the next one,
--             which is 4611686018427387904
--     ERROR:  new row for relation "journal_entries" violates check constraint
--             "journal_entries_ends_far_from_the_top"
--     ERROR:  the journal is append-only: DELETE on journal_entries was refused
--     ERROR:  the journal is append-only: TRUNCATE on journal_entries was refused
--
-- which is the outage `0007`, `0008` and `0010` were each written to close, and
-- `§14.9`'s fail-closed rule then stops every command that has to journal itself.
-- `0006`'s sequence is the same defect one table over — a temporary sequence
-- named `access_records_recorded_seq` hands the new row `1000000`, and the
-- pruner orders by that column, so the row is the newest for ever. `0006`'s own
-- comment states the invariant it loses: *a row that can renumber itself can
-- make itself immortal*.
--
-- **Setting `search_path` is not enough on its own, and this was measured before
-- it was written.** Four functions, a real `public.t` holding 7 and a temporary
-- `t` holding 999:
--
--     search_path = ''                  + unqualified body  ->  999
--     search_path = ''                  + qualified body    ->    7
--     search_path = pg_catalog, pg_temp + qualified body    ->    7
--     no SET at all                     + qualified body    ->    7
--
-- An empty path does **not** take `pg_temp` out of relation resolution: left
-- unlisted, the temporary schema is searched first whatever the path says. What
-- takes it out is listing it, and listing it last:
--
--     search_path = probe2, pg_temp     + unqualified body  ->    7
--
-- So that is the shape used here. Every function gets `SET search_path FROM
-- CURRENT` — which freezes the path this migration is running with — and the
-- path has `pg_temp` appended for the length of this file. `pg_temp` is then
-- searched last rather than first, and an unqualified name in a body resolves to
-- the schema the tables are actually in.
--
-- **The schema is never written down here, and that is the point.** The
-- integration tests migrate into a schema of their own and set the pool's path
-- to it, so a hardcoded `public.` would aim a test's trigger at the developer's
-- real journal. `FROM CURRENT` takes whatever schema this migration is being
-- applied into, which is the same one the tables were created in.
--
-- Written as plain statements rather than `EXECUTE format(...)` for the same
-- reason `§15.8`'s gate refuses dynamic SQL in a migration: a statement
-- assembled at run time is not in this file, and a reader of `migrations/`
-- cannot see what the database now holds. The first draft of this file built the
-- text with `current_schema()` and that gate refused it, correctly.

-- `pg_temp` last, for the length of this migration.
SELECT set_config('search_path', current_setting('search_path') || ', pg_temp', false);

CREATE OR REPLACE FUNCTION journal_entries_sequence_is_the_next_one()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path FROM CURRENT
AS $$
DECLARE
    expected BIGINT;
BEGIN
    SELECT COALESCE(MAX(sequence) + 1, 0) INTO expected FROM journal_entries;
    IF NEW.sequence <> expected THEN
        RAISE EXCEPTION
            'the journal is dense: sequence % is not the next one, which is %',
            NEW.sequence, expected
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION access_records_recorded_is_the_databases()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path FROM CURRENT
AS $$
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
$$;

-- `journal_is_append_only()` from `0001` reads no relation at all — it raises
-- unconditionally — so nothing can be planted in front of it, and the first
-- draft of this file left it alone and said so. It is pinned anyway: the gate
-- that keeps this class closed asks every function in `migrations/` the same
-- question, and a function that has to be exempted from a rule is a rule with a
-- hole in it and a list somebody has to maintain. One line is cheaper.
ALTER FUNCTION journal_is_append_only() SET search_path FROM CURRENT;

-- And the path back as it was, so nothing after this file inherits an addition
-- made for it.
SELECT set_config('search_path',
    regexp_replace(current_setting('search_path'), ', pg_temp$', ''), false);
