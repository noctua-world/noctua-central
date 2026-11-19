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
-- **No `UPDATE` and no `DELETE` undoes what a person granted.** An agent is
-- never edited and never deleted by one, so whatever names it still finds it.
-- A grant takes one `UPDATE`, its end, written once with when and by whom, and
-- nothing else of it ever changes; an ended grant stays, so whatever names it
-- still resolves. Each rule is a trigger, which holds under
-- `trusted_schema = OFF`, and which `PRAGMA ignore_check_constraints` does not
-- switch off as it does a `CHECK`. A plain `INSERT` of an identifier the file
-- holds is the primary key's to refuse.
--
-- **What no trigger answers is a process running as the same user.** It can
-- `DROP TRIGGER`: SQLite has no roles (`crates/nc-local`'s header). Nor does
-- it need to: an `INSERT OR REPLACE`, or a `REPLACE`, deletes the row it
-- conflicts with and fires no delete trigger, since SQLite fires one on a
-- replace only under `recursive_triggers`. So it rewrites an agent, brings an
-- ended grant back, or removes the live grant on a scope, and changes no
-- object the open-time check compares.
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
-- scope ends the live one in the transaction that writes it, and a plain
-- `INSERT` of a second live row on the same scope is refused whoever writes it
-- (a `REPLACE` deletes the live one instead, as the header says).
CREATE UNIQUE INDEX agent_grants_one_live_per_scope
    ON agent_grants (agent, scope_type, scope_id) WHERE ended_at IS NULL;

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
