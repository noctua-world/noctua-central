-- The agents a profile's person made, and what each was granted (NOCTUA.md
-- §14.6): `nc_local::LocalAgents` keeps them, in the profile's own file and
-- never in the world file a daemon starts from (§5.13).
--
-- **Every realm's, because the file is** (`0003`): an agent and each of its
-- grants name the realm they were made in, and a kernel reads its own.
--
-- **An identifier is the minted Snowflake, as an integer**: the machine's one
-- minter gave it, and its Crockford spelling is made from this number when it
-- is read. A person is an opaque identifier, as every principal is.
--
-- **What a person granted is never rewritten, and never forgotten.** An agent
-- is written once and never edited or deleted, so whatever names it still
-- finds it. A grant is written once and ended once, by writing when and by
-- whom, and nothing else of it ever changes; an ended grant stays, so whatever
-- names it still resolves. Each rule is a trigger, which holds under
-- `trusted_schema = OFF`, and which `PRAGMA ignore_check_constraints` does not
-- switch off as it does a `CHECK`. **The two that refuse an insert run before
-- SQLite resolves its conflict**, so an `INSERT OR REPLACE`, or a `REPLACE`,
-- is refused as a plain `INSERT` is, rather than deleting the row it meets: a
-- delete no delete trigger would see, since SQLite fires one on a replace only
-- under `recursive_triggers`.
--
-- **What no trigger answers is a process running as the same user.** It can
-- `DROP TRIGGER`: SQLite has no roles (`crates/nc-local`'s header). Through a
-- statement, undoing what a person granted takes that schema change first,
-- which the open-time check refuses for as long as it stands; a trigger put
-- back before the next open is not seen. What runs no statement needs no
-- schema change, and is not seen either. SQLite's incremental BLOB I/O fires
-- no trigger and rewrites in place, at the same length, a text column no
-- index holds: an agent's realm, operator or label, and a grant's realm,
-- giver, reason or ender — an agent or a grant whose realm is rewritten is
-- gone from its own realm's reads. It opens no integer, no null and no
-- indexed column, so an agent's budget, and a grant's agent, scope, window
-- and whether and when it ended, stay as written. The file's bytes written
-- directly change anything, and no row is signed.
--
-- `catalog/limits.toml` says `refuse-at-limit` for both tables: they are user
-- state, which nothing prunes, and their writer refuses a new row at the bound
-- rather than losing an old one.

-- One row per agent: who it acts for, what its person calls it, and what it
-- may spend in a session.
CREATE TABLE agents (
    agent    INTEGER PRIMARY KEY CHECK (agent > 0),
    realm    TEXT    NOT NULL,
    operator TEXT    NOT NULL,
    label    TEXT    NOT NULL CHECK (length(label) BETWEEN 1 AND 64),
    budget   INTEGER NOT NULL CHECK (budget > 0)
) STRICT;

-- **An agent is written once**, refused before SQLite resolves a conflict: a
-- plain `INSERT` of an identifier the file holds, and an `INSERT OR REPLACE`
-- or a `REPLACE` that would delete the agent it meets and write another under
-- its identifier.
CREATE TRIGGER agents_are_written_once BEFORE INSERT ON agents
WHEN EXISTS (SELECT 1 FROM agents WHERE agent = NEW.agent)
BEGIN
    SELECT RAISE(ABORT, 'agents: an agent is written once');
END;

CREATE TRIGGER agents_are_never_edited BEFORE UPDATE ON agents
BEGIN
    SELECT RAISE(ABORT, 'agents: an agent is never edited');
END;

CREATE TRIGGER agents_are_never_deleted BEFORE DELETE ON agents
BEGIN
    SELECT RAISE(ABORT, 'agents: an agent is never deleted');
END;

-- One relation tuple per row: an agent, the one resource it may act on — its
-- scope, a type and an identifier — and the grant's provenance: who gave it,
-- why, and the window it runs in. **`scope_type` has no `CHECK`**: SQLite
-- cannot widen one without rebuilding the table, and the types a grant may
-- scope are the reader's to refuse.
CREATE TABLE agent_grants (
    grant_id    INTEGER PRIMARY KEY CHECK (grant_id > 0),
    agent       INTEGER NOT NULL REFERENCES agents (agent),
    realm       TEXT    NOT NULL,
    scope_type  TEXT    NOT NULL,
    scope_id    TEXT    NOT NULL,
    granted_by  TEXT    NOT NULL,
    reason      TEXT    NOT NULL CHECK (length(reason) BETWEEN 1 AND 256),
    valid_from  INTEGER NOT NULL CHECK (valid_from >= 0),
    valid_until INTEGER NOT NULL CHECK (valid_until > valid_from),
    ended_at    INTEGER,
    ended_by    TEXT,
    CHECK ((ended_at IS NULL) = (ended_by IS NULL))
) STRICT;

-- **One live grant per agent and scope, held by the file**: a new grant on a
-- scope ends the live one in the transaction that writes it, and a second live
-- row on the same scope is refused whoever writes it and however — a plain
-- `INSERT` by this index, and a replace, which would delete the live one to
-- make room, by `agent_grants_are_written_once` before the index is asked.
CREATE UNIQUE INDEX agent_grants_one_live_per_scope
    ON agent_grants (agent, scope_type, scope_id) WHERE ended_at IS NULL;

-- **A grant is written once, and a scope holds one live grant**, refused
-- before SQLite resolves a conflict: an insert of an identifier the file
-- holds, which a replace would answer by deleting that grant and writing it
-- again, live or ended as the insert says; and a live insert on a scope that
-- holds a live grant, which a replace would answer by deleting the live one.
-- The second clause names the columns and the `WHERE` of the index above.
CREATE TRIGGER agent_grants_are_written_once BEFORE INSERT ON agent_grants
WHEN EXISTS (SELECT 1 FROM agent_grants WHERE grant_id = NEW.grant_id)
  OR (NEW.ended_at IS NULL AND EXISTS (SELECT 1 FROM agent_grants
      WHERE agent = NEW.agent AND scope_type = NEW.scope_type
        AND scope_id = NEW.scope_id AND ended_at IS NULL))
BEGIN
    SELECT RAISE(ABORT, 'agent_grants: a grant is written once, and a scope holds one live grant');
END;

-- Ending is the one change a grant takes: once, with both when and by whom,
-- and with every other column as it was.
CREATE TRIGGER agent_grants_end_once BEFORE UPDATE ON agent_grants
WHEN OLD.ended_at IS NOT NULL OR NEW.ended_at IS NULL OR NEW.ended_by IS NULL
  OR NEW.grant_id IS NOT OLD.grant_id OR NEW.agent IS NOT OLD.agent
  OR NEW.realm IS NOT OLD.realm OR NEW.scope_type IS NOT OLD.scope_type
  OR NEW.scope_id IS NOT OLD.scope_id OR NEW.granted_by IS NOT OLD.granted_by
  OR NEW.reason IS NOT OLD.reason OR NEW.valid_from IS NOT OLD.valid_from
  OR NEW.valid_until IS NOT OLD.valid_until
BEGIN
    SELECT RAISE(ABORT, 'agent_grants: a grant is ended once, and nothing else of it changes');
END;

CREATE TRIGGER agent_grants_are_never_deleted BEFORE DELETE ON agent_grants
BEGIN
    SELECT RAISE(ABORT, 'agent_grants: a grant is never deleted');
END;
