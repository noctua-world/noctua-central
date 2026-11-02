-- The path a trigger function is frozen with holds no name resolved later.
--
-- **`0011` moved `pg_temp` to the back and left `"$user"` at the front.** It
-- built the path with `current_setting('search_path')`, whose default value on
-- this stack is `"$user", public` — and `$user` is not a schema, it is an
-- instruction to look for one named after the connecting role, re-expanded at
-- **call** time. So the very substitution `0011` was written to stop still
-- happened, one schema over. Measured on a fresh stack, three ordinary
-- statements, no DDL on `journal_entries`, no temporary table, no trigger
-- disabled:
--
--     INSERT INTO public.journal_entries (...) VALUES (0, 'r', ...);   -- INSERT 0 1
--     CREATE SCHEMA noctua;
--     CREATE TABLE noctua.journal_entries (sequence BIGINT);
--     INSERT INTO noctua.journal_entries VALUES (4611686018427387902);
--     INSERT INTO public.journal_entries (...) VALUES (4611686018427387903, 'attacker', ...);
--     INSERT 0 1
--
-- The planted schema can then be dropped and the row stays. `access_records`
-- goes the same way through the unqualified sequence name, and the row that
-- results is the one `0006` says must not exist — *a row that can renumber
-- itself can make itself immortal*.
--
-- The role this repository connects as owns its database, so `CREATE SCHEMA` is
-- a privilege it already has; and a deployment that gives each role a schema of
-- its own — `CREATE SCHEMA AUTHORIZATION`, an ordinary arrangement — has the
-- `$user` schema existing legitimately, with no attacker anywhere.
--
-- **So the frozen path names a schema and nothing that stands for one.**
-- `current_schema()` is the first *existing* schema on the path, which is the
-- schema the tables were created in one migration earlier, and freezing its name
-- leaves nothing to re-expand.
--
-- **And `pg_temp` is placed, not appended.** `0011` wrote
-- `current_setting(...) || ', pg_temp'` unconditionally, so a session whose path
-- already named `pg_temp` — a role or database default, `PGOPTIONS`, a pooler's
-- startup parameters — got it *twice*, and PostgreSQL resolves at the first
-- occurrence while the gate read the last. Measured: `pg_temp, public, pg_temp`
-- returned the temporary table, and the gate called it safe. The path is built
-- from scratch here, so there is exactly one.

-- **A temporary schema in front means this migration cannot know where the
-- tables are, and guessing is how the defect above happened.** Refused rather
-- than applied: `CREATE OR REPLACE FUNCTION` names no schema, so under such a
-- path it would write the functions into `pg_temp` where they vanish at
-- disconnect, leaving the triggers pointing at the old unpinned ones and the
-- migration recorded as applied.
DO $guard$
BEGIN
    IF current_schema() IS NULL OR current_schema() LIKE 'pg\_temp%' THEN
        RAISE EXCEPTION
            'the search path resolves to % , so this migration cannot tell which schema '
            'holds the journal. Apply migrations with the schema they created first on '
            'the path', COALESCE(current_schema(), 'nothing');
    END IF;
END;
$guard$;

-- **The path as it was, kept somewhere it survives the rebuild.** The trailer at
-- the end of this file was copied from `0011`, which *appends* `, pg_temp` to
-- whatever the session already had — so stripping that suffix there restores the
-- original exactly. This file does not append: it rebuilds the path from
-- `current_schema()`, so the strip leaves `public` and `"$user"` is gone from
-- the migrating session for good. Measured on a live stack:
--
--     search_path                                             -> "$user", public
--     set_config(quote_ident(current_schema()) || ', pg_temp') -> public, pg_temp
--     regexp_replace(…, ', pg_temp$', '')                      -> public
--
-- Nothing runs after this file today, which is what made it latent rather than
-- broken; `0013` would inherit it, and `set_config(…, false)` is session-level
-- on a connection sqlx returns to a pool.
SELECT set_config('nc.path_before_0012', current_setting('search_path'), false);

SELECT set_config('search_path', quote_ident(current_schema()) || ', pg_temp', false);

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

ALTER FUNCTION journal_is_append_only() SET search_path FROM CURRENT;

-- The path back as it was, so nothing after this file inherits a change made for
-- it — the value taken above rather than a suffix stripped off a rebuilt one.
SELECT set_config('search_path', current_setting('nc.path_before_0012'), false);
SELECT set_config('nc.path_before_0012', '', false);
