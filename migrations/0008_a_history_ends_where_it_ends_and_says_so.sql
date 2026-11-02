-- `0007` was off by one, and it made the failure it was written for worse.
--
-- Its own header claims that with the tail at most 9223372036854775806,
-- `MAX(sequence) + 1` is "always a value the column can hold". It is a value the
-- *type* holds and not one that constraint permits: the next entry after the
-- largest permitted tail is 9223372036854775807, which `0007` refuses. So one
-- ordinary INSERT at the value `0007` allows still ends the history — and the
-- refusal a caller then gets is a check violation, SQLSTATE class 23, which
-- `unavailable()` did not remap:
--
--   append -> Unavailable("… violates check constraint
--                          journal_entries_leaves_room_above_its_tail")
--             NC-JRNL-0003, severity critical, retryable = true
--
-- Before `0007` the same state answered `Unstorable`, `NC-JRNL-0005`,
-- `retryable = false`. The migration turned a permanent, honest refusal into a
-- critical one that says *the store did not answer* and *try again*, for a
-- database answering instantly and for ever.
--
-- **No bound can give a history a next entry at its own top.** The column is
-- finite; whatever the last permitted value is, nothing follows it. What a bound
-- can do is two things this one now does deliberately:
--
--   * put the end where nothing legitimate reaches it. 2^62 is 4.6 x 10^18
--     entries — at one entry per microsecond, 146 thousand years — so the end of
--     the history is not a state this platform can be walked into; and
--   * leave the remaining half of the range as a margin no honest writer uses,
--     so a foreign INSERT anywhere in it is refused by the database rather than
--     accepted and left to brick the next append.
--
-- What is *not* claimed: that the history is endless, or that a row at the bound
-- can be followed. `crates/nc-journal` maps the refusal honestly instead — a
-- class-23 violation is the record being unstorable, not the store being down.
ALTER TABLE journal_entries
    DROP CONSTRAINT journal_entries_leaves_room_above_its_tail;

ALTER TABLE journal_entries
    ADD CONSTRAINT journal_entries_ends_far_from_the_top
    CHECK (sequence < 4611686018427387904);
