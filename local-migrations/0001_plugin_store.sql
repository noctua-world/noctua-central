-- The plugin store: what each plugin keeps on the user's own machine, and the
-- units that wrote it (NOCTUA.md §11.4, §11.5).
--
-- **SQLite, not PostgreSQL, and never mixed with `migrations/`.** This file is
-- applied by `crates/nc-local` to a database file inside the user's profile
-- (NOCTUA.md §4, "Local app database"), and `cargo xtask check` applies it to a
-- scratch file and holds what it builds to the list `crates/nc-local/src/schema.rs`
-- carries — the same list the crate compares a database against every time it
-- opens one. A statement added here without a row there is a database the
-- kernel refuses to open, so the two change together.
--
-- **A migration a build has carried is never edited**, a comment included:
-- every file that build made records the checksum of this text, and
-- `crates/nc-local` refuses a file whose record differs at every open after.
-- A change is a new migration; `LOCAL_MIGRATIONS_ON_DISK` in
-- `xtask/src/check.rs` pins each file's checksum, so a rewrite before any
-- release is an edit of the pin too.
--
-- Every table is `STRICT`: a column holds the type it declares and nothing
-- else, which is what a column of PostgreSQL would do and what SQLite does not
-- by default. A value is nc-canon's canonical JSON text, and `json_valid` holds
-- it to JSON at the row.

-- The file says whose it is. `crates/nc-local` refuses a database whose header
-- carries another application's number, and this is the one statement that
-- writes ours ('NCL1' read as a big-endian integer) — in the same transaction as
-- the tables, so a file either has both or neither.
PRAGMA application_id = 1313033265;

-- What each plugin holds in each realm, one row per key. **User state: the
-- pruner never deletes from it**, and `catalog/limits.toml` says
-- `refuse-at-limit` — a plugin at its quota in a realm is refused a new key
-- rather than losing an old one.
--
-- **A key is a realm's**, as every entity is (NOCTUA.md §10.3): the kernels of
-- the realms one person belongs to share this file, and the same key in
-- another realm is another row — what a plugin keeps for one realm's entity is
-- never read or written through another's decision point.
--
-- `rev_unit` and `rev_n` are the revision a reader is handed and a writer must
-- name: the unit that last wrote the row, and which of that unit's writes it
-- was. Another unit never hands out the same revision, because a unit is never
-- reused; inside one unit the revision names the key, whatever values the
-- unit writes to it.
CREATE TABLE store_entries (
    realm     TEXT    NOT NULL,
    plugin    TEXT    NOT NULL,
    store_key TEXT    NOT NULL,
    value     TEXT    NOT NULL CHECK (json_valid(value)),
    rev_unit  INTEGER NOT NULL,
    rev_n     INTEGER NOT NULL CHECK (rev_n >= 0),
    PRIMARY KEY (realm, plugin, store_key)
) STRICT, WITHOUT ROWID;

-- One row per committed unit. A unit that was aborted never reaches this file:
-- its writes lived in the kernel until the commit, and the commit is one
-- transaction.
--
-- `target` names the unit an undo or a redo reverses, and `undone_by` the unit
-- that reversed this one. **Neither is a foreign key**, deliberately: the
-- pruner removes the oldest units first, and a reference from a newer unit to
-- an older one would either stop that (a refusal) or reach past it (a cascade
-- through history that nobody asked to prune). The kernel reads a target that
-- is gone as one undo cannot reach.
CREATE TABLE store_units (
    unit         INTEGER PRIMARY KEY,
    realm        TEXT    NOT NULL,
    principal    TEXT    NOT NULL,
    kind         TEXT    NOT NULL CHECK (kind IN ('step', 'undo', 'redo')),
    target       INTEGER,
    label        TEXT    NOT NULL,
    state        TEXT    NOT NULL CHECK (state IN ('committed', 'undone')),
    undone_by    INTEGER,
    committed_at INTEGER NOT NULL CHECK (committed_at >= 0),
    CHECK ((kind = 'step') = (target IS NULL)),
    CHECK ((state = 'undone') = (undone_by IS NOT NULL))
) STRICT;

-- Newest first, per person: what undo reads.
CREATE INDEX store_units_by_principal ON store_units (realm, principal, unit);

-- The net before and after image of every key a unit wrote. NULL is "absent"
-- before and "deleted" after, and a value and its revision are NULL together.
--
-- Bounded per unit, in `catalog/limits.toml`: `crates/nc-local` refuses a unit
-- that would record more images than its entry says, and the rows are deleted
-- with their unit by `ON DELETE CASCADE` — so the table holds at most that many
-- for every unit `store_units` holds.
CREATE TABLE store_writes (
    unit            INTEGER NOT NULL REFERENCES store_units (unit) ON DELETE CASCADE,
    plugin          TEXT    NOT NULL,
    store_key       TEXT    NOT NULL,
    before_value    TEXT    CHECK (json_valid(before_value)),
    before_rev_unit INTEGER,
    before_rev_n    INTEGER,
    after_value     TEXT    CHECK (json_valid(after_value)),
    after_rev_unit  INTEGER,
    after_rev_n     INTEGER,
    PRIMARY KEY (unit, plugin, store_key),
    CHECK ((before_value IS NULL) = (before_rev_unit IS NULL)),
    CHECK ((before_rev_unit IS NULL) = (before_rev_n IS NULL)),
    CHECK ((after_value IS NULL) = (after_rev_unit IS NULL)),
    CHECK ((after_rev_unit IS NULL) = (after_rev_n IS NULL))
) STRICT, WITHOUT ROWID;

-- An image is what undo restores, so one that could be edited is an undo that
-- could be made to write anything. `RAISE(ABORT)` works under
-- `trusted_schema = OFF`, which `crates/nc-local` sets; SQLite answers it with
-- the extended code 1811, which is what the crate matches. Deleting stays
-- possible, because a unit is deleted whole.
CREATE TRIGGER store_writes_are_never_edited BEFORE UPDATE ON store_writes
BEGIN
    SELECT RAISE(ABORT, 'store_writes: a unit''s images are never edited');
END;

-- What a unit did besides writing keys: the actions it was authorised for, the
-- compensations it registered, what left the machine, the blobs it created.
-- Each note is canonical JSON, in the order the unit made it.
--
-- Bounded per unit, as the images are: `catalog/limits.toml` says how many
-- notes one unit may record, `crates/nc-local` refuses a unit past it, and the
-- rows go with their unit.
CREATE TABLE store_notes (
    unit INTEGER NOT NULL REFERENCES store_units (unit) ON DELETE CASCADE,
    seq  INTEGER NOT NULL CHECK (seq >= 0),
    note TEXT    NOT NULL CHECK (json_valid(note)),
    PRIMARY KEY (unit, seq)
) STRICT, WITHOUT ROWID;

CREATE TRIGGER store_notes_are_never_edited BEFORE UPDATE ON store_notes
BEGIN
    SELECT RAISE(ABORT, 'store_notes: a unit''s notes are never edited');
END;
