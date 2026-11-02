-- Which blobs this machine holds, and which unit created each (NOCTUA.md
-- §11.5: undo reverses "blobs created inside the group"). The bytes live in a directory beside this file, named by their
-- BLAKE3; what lives here is what the bytes cannot say about themselves.
--
-- `pinned` is whether a committed unit still holds the blob. Undo unpins what
-- the undone step created and stamps `unpinned_at`, and the bytes are kept for
-- a grace period so that redo can pin them again; after it, the kernel's sweep
-- deletes both the bytes and the row.
--
-- `created_unit` has no foreign key for the reason `store_units.target` has
-- none: the pruner removes old units, and a blob outlives the unit that made it.
--
-- `catalog/limits.toml` says `refuse-at-limit`: a pinned blob is user state, so
-- at the bound a new blob is refused rather than an old one lost.
CREATE TABLE blobs (
    hash         TEXT    NOT NULL PRIMARY KEY
                         CHECK (length(hash) = 64 AND hash NOT GLOB '*[^0-9a-f]*'),
    size         INTEGER NOT NULL CHECK (size >= 0),
    created_unit INTEGER NOT NULL,
    pinned       INTEGER NOT NULL CHECK (pinned IN (0, 1)),
    unpinned_at  INTEGER,
    CHECK ((pinned = 1) = (unpinned_at IS NULL))
) STRICT, WITHOUT ROWID;
