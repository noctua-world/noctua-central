-- No bound on a column can stop one INSERT ending the history.
--
-- `0007` bounded the sequence and its top row could not be followed. `0008`
-- moved the bound to 2^62 and claimed the margin below it meant "a foreign
-- INSERT anywhere in it is refused by the database rather than accepted and
-- left to brick the next append". That is false, and the value that shows it is
-- one below the bound: 4611686018427387903 is *permitted*, and the entry after
-- it is the one the constraint refuses. Measured, with the privilege the
-- writing service already has and no DDL:
--
--   INSERT at 4611686018427387903   -> Ok(1)
--   the anchor at the next sequence -> Unstorable, and every later append too
--   DELETE / UPDATE / TRUNCATE      -> refused by this table's own triggers
--
-- Every CHECK has that shape, because a column bound says which values are
-- legal and nothing about which value comes next. So the rule this table needs
-- is not about a value at all: **a sequence is the next one, and nothing else.**
--
-- A row may only land at `MAX(sequence) + 1`, or at 0 in an empty table. That
-- makes the ceiling unreachable rather than differently placed — a foreign
-- writer cannot leap to it, because it cannot leap anywhere — and it enforces
-- at the database the density `verify` already assumes: `0003`'s floor and
-- `0008`'s ceiling stay as they are, and this is what stops anything reaching
-- either.
--
-- **Concurrency.** Two writers reading the same `MAX` both compute the same
-- next sequence; the primary key admits one and the other gets a unique
-- violation, which `crates/nc-journal/src/postgres.rs` already reads as
-- contention. This trigger adds no new race, it only refuses the values the
-- primary key would have allowed.
--
-- **The repair still works.** An operator's anchor is written at exactly
-- `MAX + 1`, which is the one value this permits — so the door
-- `0009_a_hash_is_sixty_four_hex_characters.sql` documents stays open.
CREATE OR REPLACE FUNCTION journal_entries_sequence_is_the_next_one()
RETURNS TRIGGER
LANGUAGE plpgsql
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

CREATE TRIGGER journal_entries_sequence_is_the_next_one
    BEFORE INSERT ON journal_entries
    FOR EACH ROW
    EXECUTE FUNCTION journal_entries_sequence_is_the_next_one();

ALTER TABLE journal_entries
    ENABLE ALWAYS TRIGGER journal_entries_sequence_is_the_next_one;
