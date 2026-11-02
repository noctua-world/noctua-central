-- Access records (NOCTUA.md §15.6).
--
-- "Accesses must be recorded with a great richness of data", including hardware
-- information and GeoIP location, and whether something like a VPN is masking
-- it. This is the table that holds it.
--
-- **It is genuinely about a person**, and that decides everything about where it
-- lives: in PostgreSQL under normal authorization, **never** written to the
-- central log (§15.4), subject to the row limits of §15.8, and visible to the
-- user themselves — somebody should be able to see where their own account has
-- been used from.
--
-- The columns are here before the stage that fills them, and that is deliberate.
-- §15.8's gate needs one real table with a real row bound to act on, and a
-- pruner proved only against a table its own test invented is a pruner nobody
-- has watched work on anything that ships.

CREATE TABLE access_records (
    -- A Snowflake, in Crockford base32.
    --
    -- **Not what the pruner orders by**, and this comment used to say it was.
    -- The encoding is variable width — twelve characters below `32^12` and
    -- thirteen at or above it, which `nc-ids`' `base32` module dates at about
    -- 2034 — and a `text` column sorts byte-lexicographically. From the first
    -- thirteen-character identifier until the last twelve-character row is
    -- gone, ascending order is not creation order:
    --
    --   minted oldest first:  100000000000  100000000001  G00000000000
    --                         1000000000000 1000000000001 4000000000000
    --   ORDER BY id ASC:      100000000000  1000000000000 1000000000001
    --                         100000000001  4000000000000 G00000000000
    --
    -- Measured with the real pruner over those six rows: it removed the oldest
    -- *and the third-newest*, and reported an ordinary `Pruned`. `base32`'s own
    -- header draws the conclusion — "sorting by identifier value still works,
    -- and that is what anything reading these names should do" — and the pruner
    -- is a reader that cannot: SQL sorts the text, not the value.
    id                TEXT        PRIMARY KEY,
    realm             TEXT        NOT NULL,
    principal         TEXT        NOT NULL,

    -- When the access happened, as the writer of the row saw it.
    --
    -- **Not what the pruner orders by either**, and it was for a round. Nothing
    -- gives this column a value: no DEFAULT, no CHECK, no trigger. It is what a
    -- caller supplies, so it is only as monotonic as that caller's clock, and
    -- `migrations/0006_access_records_orders_by_insertion.sql` records what the real pruner did to ten rows when three
    -- of them came from a device a day behind. `recorded`, added there, is the
    -- column the database decides.
    occurred_at       BIGINT      NOT NULL,
    -- What the request said about itself. Free-form because it is a person's
    -- own device describing itself, and this table is the one place that is
    -- allowed — it is under authorization and never leaves the database.
    user_agent        TEXT,
    client_address    INET,

    -- §15.6's GeoIP fields, from a free database with no account required.
    country           TEXT,
    region            TEXT,
    city              TEXT,
    autonomous_system BIGINT,

    -- §15.6's VPN, proxy and datacenter signal. **A risk signal, never a
    -- control**: free lists catch commodity VPNs, Tor and datacenters well and
    -- miss residential proxies almost entirely, so nothing may be gated on it.
    -- It may raise friction or flag an access for review.
    masked            BOOLEAN     NOT NULL DEFAULT FALSE,
    masked_reason     TEXT,

    outcome           TEXT        NOT NULL
);

CREATE INDEX access_records_principal ON access_records (principal, occurred_at);
CREATE INDEX access_records_realm ON access_records (realm, occurred_at);
