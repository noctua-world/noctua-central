//! The agents a profile's person made, and what each was granted: `agents`
//! and `agent_grants` in the local database
//! (`local-migrations/0005_agents.sql`).
//!
//! [`LocalStore::agents`](crate::LocalStore::agents) hands [`LocalAgents`] out
//! over the store's own pool, so the agents, the plugin store and the journal
//! are one file, opened and judged once, and every statement that writes these
//! tables is in this crate, where the source gate over the crates that open
//! the file reads it (invariant 18).
//!
//! # What it keeps, and what it refuses
//!
//! - **An agent and a grant to one, as rows of the realm they were made in.**
//!   It learns no type of the decision point's: an agent's operator and a
//!   grant's giver are opaque identifiers, a grant's scope is a type and an
//!   identifier as text, and whoever opens a kernel over these rows makes the
//!   principals and the entities of them.
//! - **Nothing is edited but a grant's end, and nothing is deleted**, by the
//!   tables' own triggers, whoever writes. A grant that replaces the live one
//!   on its scope ends it in the transaction that writes it, and the file's
//!   unique index refuses a second live grant on one scope written any other
//!   way.
//! - **Every write is counted** through the crate's `write_exactly`, inside a
//!   `BEGIN IMMEDIATE` that first asks that no temporary object sits on its
//!   connection (invariant 22): a write a trigger silenced is refused, never
//!   believed.
//! - **A table at its bound refuses a new row** ([`KeepError::Full`]). Both
//!   are `refuse-at-limit` in `catalog/limits.toml`, user state that nothing
//!   prunes, and the bound read from there is counted over the whole table,
//!   every realm's rows and every ended grant included, in the transaction
//!   that would add the row.
//! - **A row read back is one this build writes, or it is refused**
//!   (`NC-DB-0007`): an identifier that is not one the minter gives, a realm or
//!   a person that is not an opaque identifier, a budget no agent holds, a time
//!   below zero, or a grant whose window ends where it begins or before.
//!
//! # What it does not answer (`§2.4`)
//!
//! A process running as the same user, which can drop the triggers and then
//! edit or delete any row, or write a grant nobody gave: SQLite has no roles
//! (the crate's header), and no row is signed. Who may make an agent, grant it
//! or end its grants is not this crate's to decide either: it keeps what it is
//! handed, and refuses only what the file could never hold.

use nc_ids::{OpaqueId, Snowflake};
use sqlx::Row as _;
use sqlx::SqliteConnection;
use sqlx::sqlite::{SqlitePool, SqliteRow};

use crate::store::{as_signed, as_unsigned, write_exactly};
use crate::{LocalError, said, schema};

/// The agents a profile's person made, and their grants, in the local
/// database.
#[derive(Debug, Clone)]
pub struct LocalAgents {
    /// The store's pool, which [`crate::LocalStore`] opened and judged.
    pool: SqlitePool,
    /// How many rows each table may hold.
    bounds: Bounds,
}

/// How many rows each table may hold.
///
/// `max_rows` of `[tables.agents]` and of `[tables.agent_grants]`, read from
/// the catalog where the number is written once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bounds {
    /// The most agents the file holds.
    agents: u64,
    /// The most grants the file holds, ended ones included.
    grants: u64,
}

/// An agent, as the file keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRow {
    /// Its identifier, as the machine's minter gave it.
    pub agent: Snowflake,
    /// The realm it was made in.
    pub realm: OpaqueId,
    /// The person it acts for.
    pub operator: OpaqueId,
    /// What its person calls it.
    pub label: String,
    /// What it may spend in a session.
    pub budget: u32,
}

/// A grant to an agent, as the file keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRow {
    /// Its identifier, as the machine's minter gave it.
    pub grant: Snowflake,
    /// The agent it is given to.
    pub agent: Snowflake,
    /// The realm it was given in.
    pub realm: OpaqueId,
    /// The type of the one resource it scopes.
    pub scope_type: String,
    /// That resource's identifier.
    pub scope_id: String,
    /// Who gave it.
    pub granted_by: OpaqueId,
    /// Why, in its giver's words.
    pub reason: String,
    /// When it begins, in milliseconds since the Unix epoch.
    pub valid_from_ms: u64,
    /// When it ends, in milliseconds since the Unix epoch.
    pub valid_until_ms: u64,
}

/// What the file keeps of one realm's agents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentsKept {
    /// The realm's agents, in the order of their identifiers, which is the
    /// order the minter gave them in.
    pub agents: Vec<AgentRow>,
    /// The realm's grants not ended, in the order of their identifiers — the
    /// ones past their end among them, which the decision point refuses.
    pub live: Vec<GrantRow>,
    /// How many grants the file holds, of every realm and ended ones included,
    /// which is what its bound counts.
    pub grants_kept: u64,
}

/// Why an agent or a grant was not kept; nothing the call wrote is in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepError {
    /// The table already holds as many rows as `catalog/limits.toml` bounds
    /// it to, and nothing in it is ever deleted to make room.
    Full {
        /// Which: `agents` or `agent_grants`.
        table: &'static str,
    },
    /// The file refused the row, holds what this build did not write, or
    /// could not be asked.
    Store(LocalError),
}

impl From<LocalError> for KeepError {
    fn from(error: LocalError) -> Self {
        Self::Store(error)
    }
}

impl core::fmt::Display for KeepError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Full { table } => write!(
                formatter,
                "nothing was kept: `{table}` already holds as many rows as its bound allows, \
                 and none of them is ever deleted to make room"
            ),
            Self::Store(error) => write!(formatter, "nothing was kept: {error}"),
        }
    }
}

impl std::error::Error for KeepError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Full { .. } => None,
            Self::Store(error) => Some(error),
        }
    }
}

impl LocalAgents {
    /// The agents' tables over `pool`, bounded by the catalog.
    pub(crate) fn over(pool: SqlitePool) -> Self {
        Self {
            pool,
            bounds: Bounds::of_the_catalog(),
        }
    }

    /// Every agent of `realm`, its grants not ended, and how many grants the
    /// file holds.
    ///
    /// **One snapshot**: the three reads are one transaction, so no write
    /// lands between a grant and the count that holds it.
    ///
    /// # Errors
    ///
    /// [`LocalError::Unexpected`] for a row this build does not write, or a
    /// file SQLite answers for as damaged; [`LocalError::Unavailable`] when
    /// the file could not be asked.
    pub async fn kept(&self, realm: &OpaqueId) -> Result<AgentsKept, LocalError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| said("beginning to read the agents", &error))?;
        let agents = sqlx::query(
            "SELECT agent, realm, operator, label, budget FROM main.agents \
             WHERE realm = ?1 ORDER BY agent",
        )
        .bind(realm.as_str())
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| said("reading the agents", &error))?
        .iter()
        .map(agent_row)
        .collect::<Result<Vec<_>, _>>()?;
        let live = sqlx::query(
            "SELECT grant_id, agent, realm, scope_type, scope_id, granted_by, reason, \
             valid_from, valid_until FROM main.agent_grants \
             WHERE realm = ?1 AND ended_at IS NULL ORDER BY grant_id",
        )
        .bind(realm.as_str())
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| said("reading the agents' grants", &error))?
        .iter()
        .map(grant_row)
        .collect::<Result<Vec<_>, _>>()?;
        let grants_kept: i64 = sqlx::query_scalar("SELECT count(*) FROM main.agent_grants")
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| said("counting the agents' grants", &error))?;
        transaction
            .commit()
            .await
            .map_err(|error| said("finishing reading the agents", &error))?;
        Ok(AgentsKept {
            agents,
            live,
            grants_kept: as_unsigned(grants_kept, "a count of grants")?,
        })
    }

    /// Keeps a new agent.
    ///
    /// # Errors
    ///
    /// [`KeepError::Full`] when `agents` is at its bound;
    /// [`KeepError::Store`] when the file refuses the row — an identifier it
    /// holds already, a label or a budget its checks refuse — or a write
    /// something silenced, or when SQLite could not be asked.
    pub async fn keep_agent(&self, row: &AgentRow) -> Result<(), KeepError> {
        let agent = as_signed(row.agent.value(), "an agent")?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("beginning to keep an agent", &error))?;
        schema::no_temporary_object(&mut transaction).await?;
        room_for_one(
            &mut transaction,
            "agents",
            "SELECT count(*) FROM main.agents",
            self.bounds.agents,
        )
        .await?;
        let written = sqlx::query(
            "INSERT INTO main.agents (agent, realm, operator, label, budget) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(agent)
        .bind(row.realm.as_str())
        .bind(row.operator.as_str())
        .bind(&row.label)
        .bind(i64::from(row.budget))
        .execute(&mut *transaction)
        .await;
        write_exactly(written, 1, "keeping an agent")?;
        transaction
            .commit()
            .await
            .map_err(|error| said("finishing keeping an agent", &error))?;
        Ok(())
    }

    /// Keeps a new grant, and in the same transaction ends `ends` — the grant
    /// it replaces, ended when and by whom the two numbers after it say.
    ///
    /// **Both or neither**: the grant `ends` names must be the live grant of
    /// the same agent, realm and scope, and a refusal of either write leaves
    /// the file as it was, the old grant live.
    ///
    /// # Errors
    ///
    /// [`KeepError::Full`] when `agent_grants` is at its bound;
    /// [`KeepError::Store`] when the file refuses the row — an identifier it
    /// holds already, an agent it does not hold, a second live grant on one
    /// scope, a reason or a window its checks refuse — when `ends` is not the
    /// live grant on the new one's scope, for a write something silenced, or
    /// when SQLite could not be asked.
    pub async fn keep_grant(
        &self,
        row: &GrantRow,
        ends: Option<(Snowflake, u64, &OpaqueId)>,
    ) -> Result<(), KeepError> {
        let grant = as_signed(row.grant.value(), "a grant")?;
        let agent = as_signed(row.agent.value(), "an agent")?;
        let from = as_signed(row.valid_from_ms, "a grant's beginning")?;
        let until = as_signed(row.valid_until_ms, "a grant's end")?;
        let ending = match ends {
            Some((ended, at, by)) => Some((
                as_signed(ended.value(), "a grant")?,
                as_signed(at, "a grant's ending")?,
                by,
            )),
            None => None,
        };
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("beginning to keep a grant", &error))?;
        schema::no_temporary_object(&mut transaction).await?;
        room_for_one(
            &mut transaction,
            "agent_grants",
            "SELECT count(*) FROM main.agent_grants",
            self.bounds.grants,
        )
        .await?;
        // **The end first**, so the new row never meets the live one it
        // replaces in the index that holds one live grant per scope.
        if let Some((ended, at, by)) = ending {
            let replaced = sqlx::query(
                "UPDATE main.agent_grants SET ended_at = ?2, ended_by = ?3 \
                 WHERE grant_id = ?1 AND ended_at IS NULL AND agent = ?4 AND realm = ?5 \
                 AND scope_type = ?6 AND scope_id = ?7",
            )
            .bind(ended)
            .bind(at)
            .bind(by.as_str())
            .bind(agent)
            .bind(row.realm.as_str())
            .bind(&row.scope_type)
            .bind(&row.scope_id)
            .execute(&mut *transaction)
            .await;
            write_exactly(replaced, 1, "ending the grant a new one replaces")?;
        }
        let written = sqlx::query(
            "INSERT INTO main.agent_grants (grant_id, agent, realm, scope_type, scope_id, \
             granted_by, reason, valid_from, valid_until) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(grant)
        .bind(agent)
        .bind(row.realm.as_str())
        .bind(&row.scope_type)
        .bind(&row.scope_id)
        .bind(row.granted_by.as_str())
        .bind(&row.reason)
        .bind(from)
        .bind(until)
        .execute(&mut *transaction)
        .await;
        write_exactly(written, 1, "keeping a grant")?;
        transaction
            .commit()
            .await
            .map_err(|error| said("finishing keeping a grant", &error))?;
        Ok(())
    }

    /// Ends each of `grants`, at `at_ms` and by `by`, all of them or none.
    ///
    /// # Errors
    ///
    /// [`LocalError::Unexpected`] for a grant the file does not hold live, a
    /// write something silenced, or a temporary object on the connection;
    /// [`LocalError::Unavailable`] when the file could not be asked.
    pub async fn end_grants(
        &self,
        grants: &[Snowflake],
        at_ms: u64,
        by: &OpaqueId,
    ) -> Result<(), LocalError> {
        let at = as_signed(at_ms, "a grant's ending")?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("beginning to end grants", &error))?;
        schema::no_temporary_object(&mut transaction).await?;
        for grant in grants {
            let ended = sqlx::query(
                "UPDATE main.agent_grants SET ended_at = ?2, ended_by = ?3 \
                 WHERE grant_id = ?1 AND ended_at IS NULL",
            )
            .bind(as_signed(grant.value(), "a grant")?)
            .bind(at)
            .bind(by.as_str())
            .execute(&mut *transaction)
            .await;
            write_exactly(ended, 1, "ending a grant")?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| said("finishing ending grants", &error))
    }
}

impl Bounds {
    /// The catalog's, for both tables.
    fn of_the_catalog() -> Self {
        Self {
            agents: max_rows("agents"),
            grants: max_rows("agent_grants"),
        }
    }
}

/// The catalog's `max_rows` for `table`.
///
/// **Zero when the catalog bounds no such table**, so a build that lost the
/// entry refuses every new row rather than holding none to a bound.
fn max_rows(table: &str) -> u64 {
    nc_catalog::TABLES
        .iter()
        .find(|declared| declared.name == table)
        .and_then(|declared| declared.max_rows)
        .unwrap_or(0)
}

/// Whether `table` has room for one more row under `bound`, counted in the
/// transaction that would add it by the statement `counting`.
async fn room_for_one(
    connection: &mut SqliteConnection,
    table: &'static str,
    counting: &'static str,
    bound: u64,
) -> Result<(), KeepError> {
    let held: i64 = sqlx::query_scalar(counting)
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| said("counting what a table holds", &error))?;
    if as_unsigned(held, "a count of rows")? >= bound {
        return Err(KeepError::Full { table });
    }
    Ok(())
}

/// One agent row, or why it is not one this build writes.
fn agent_row(row: &SqliteRow) -> Result<AgentRow, LocalError> {
    let read = |error: sqlx::Error| said("reading an agent", &error);
    let budget: i64 = row.try_get("budget").map_err(read)?;
    Ok(AgentRow {
        agent: minted(row, "agent", "an agent")?,
        realm: identifier(row, "realm", "an agent")?,
        operator: identifier(row, "operator", "an agent")?,
        label: row.try_get("label").map_err(read)?,
        budget: u32::try_from(budget)
            .ok()
            .filter(|budget| *budget > 0)
            .ok_or_else(|| {
                LocalError::Unexpected(format!(
                    "an agent's budget of {budget} is not one this build writes"
                ))
            })?,
    })
}

/// One grant row, or why it is not one this build writes.
fn grant_row(row: &SqliteRow) -> Result<GrantRow, LocalError> {
    let read = |error: sqlx::Error| said("reading a grant", &error);
    let time = |column: &str| -> Result<u64, LocalError> {
        as_unsigned(row.try_get(column).map_err(read)?, "a grant's time")
    };
    let (valid_from_ms, valid_until_ms) = (time("valid_from")?, time("valid_until")?);
    if valid_until_ms <= valid_from_ms {
        return Err(LocalError::Unexpected(format!(
            "a grant runs from {valid_from_ms} to {valid_until_ms}, and this build writes none \
             whose end is not after its beginning"
        )));
    }
    Ok(GrantRow {
        grant: minted(row, "grant_id", "a grant")?,
        agent: minted(row, "agent", "a grant")?,
        realm: identifier(row, "realm", "a grant")?,
        scope_type: row.try_get("scope_type").map_err(read)?,
        scope_id: row.try_get("scope_id").map_err(read)?,
        granted_by: identifier(row, "granted_by", "a grant")?,
        reason: row.try_get("reason").map_err(read)?,
        valid_from_ms,
        valid_until_ms,
    })
}

/// An identifier the minter gave, out of `column`, or why it is not one.
fn minted(row: &SqliteRow, column: &str, of: &str) -> Result<Snowflake, LocalError> {
    let value: i64 = row
        .try_get(column)
        .map_err(|error| said("reading an identifier", &error))?;
    crate::store::snowflake(value).map_err(|_| {
        LocalError::Unexpected(format!(
            "{of}'s {column} of {value} is not an identifier the minter gives"
        ))
    })
}

/// An opaque identifier out of `column`, or why it is not one.
fn identifier(row: &SqliteRow, column: &str, of: &str) -> Result<OpaqueId, LocalError> {
    let text: String = row
        .try_get(column)
        .map_err(|error| said("reading an identifier", &error))?;
    OpaqueId::new(text).map_err(|error| {
        LocalError::Unexpected(format!("{of}'s {column} is not an identifier: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::{AgentRow, AgentsKept, Bounds, GrantRow, KeepError, LocalAgents, max_rows};
    use crate::{LocalError, LocalStore};
    use nc_catalog::ErrorCode;
    use nc_ids::{OpaqueId, Snowflake};

    /// The identifier the minter would give as `n`.
    fn minted(n: u64) -> Snowflake {
        Snowflake::from_crockford(&nc_ids::base32::encode(n)).expect("an identifier")
    }

    fn id(text: &str) -> OpaqueId {
        OpaqueId::new(text).expect("an identifier")
    }

    fn agent(n: u64, realm: &str, operator: &str) -> AgentRow {
        AgentRow {
            agent: minted(n),
            realm: id(realm),
            operator: id(operator),
            label: format!("assistant {n}"),
            budget: 10,
        }
    }

    /// A grant `n` to `agent` on a feature, given by ana from `from` for a
    /// minute.
    fn grant(n: u64, agent: u64, realm: &str, feature: &str, from: u64) -> GrantRow {
        GrantRow {
            grant: minted(n),
            agent: minted(agent),
            realm: id(realm),
            scope_type: "Feature".to_owned(),
            scope_id: feature.to_owned(),
            granted_by: id("ana"),
            reason: format!("grant {n}"),
            valid_from_ms: from,
            valid_until_ms: from + 60_000,
        }
    }

    /// An empty store in memory, and its agents.
    async fn agents() -> (LocalStore, LocalAgents) {
        let store = LocalStore::in_memory_for_tests()
            .await
            .expect("a store in memory");
        let agents = store.agents();
        (store, agents)
    }

    /// Runs `statement` on the file, the way somebody with write access to
    /// it can.
    async fn tamper(store: &LocalStore, statement: &'static str) -> Result<(), sqlx::Error> {
        sqlx::query(statement)
            .execute(store.pool())
            .await
            .map(|_| ())
    }

    /// Every row of both tables, each column quoted as SQLite writes it, in a
    /// fixed order.
    async fn everything_in(store: &LocalStore) -> Vec<String> {
        let mut rows: Vec<String> = sqlx::query_scalar(
            "SELECT printf('%s|%s|%s|%s|%s', quote(agent), quote(realm), quote(operator), \
             quote(label), quote(budget)) FROM main.agents ORDER BY agent",
        )
        .fetch_all(store.pool())
        .await
        .expect("the agents read");
        rows.extend(
            sqlx::query_scalar::<_, String>(
                "SELECT printf('%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s', quote(grant_id), \
                 quote(agent), quote(realm), quote(scope_type), quote(scope_id), \
                 quote(granted_by), quote(reason), quote(valid_from), quote(valid_until), \
                 quote(ended_at), quote(ended_by)) FROM main.agent_grants ORDER BY grant_id",
            )
            .fetch_all(store.pool())
            .await
            .expect("the grants read"),
        );
        rows
    }

    /// When and by whom the file says grant `n` ended, if it did.
    async fn ended(store: &LocalStore, n: u64) -> (Option<i64>, Option<String>) {
        sqlx::query_as("SELECT ended_at, ended_by FROM main.agent_grants WHERE grant_id = ?1")
            .bind(i64::try_from(n).expect("fits"))
            .fetch_one(store.pool())
            .await
            .expect("the grant is held")
    }

    /// Whether `error` is a trigger of the file refusing the statement.
    fn a_trigger_refused(error: &sqlx::Error, says: &str) -> bool {
        crate::is_a_trigger_refusal(error)
            && error
                .as_database_error()
                .is_some_and(|database| database.message().contains(says))
    }

    /// Agents and grants read back as they were kept.
    ///
    /// The realm's agents and its grants not ended, each in the order of the
    /// identifiers the minter gave, and every grant of the file counted.
    #[tokio::test]
    async fn an_agent_and_its_grants_read_back_as_they_were_kept() {
        let (store, agents) = agents().await;
        let (first, second, abroad) = (
            agent(30, "home", "ana"),
            agent(10, "home", "carla"),
            agent(20, "work", "ana"),
        );
        for row in [&first, &second, &abroad] {
            agents.keep_agent(row).await.expect("kept");
        }
        let (naming, finding, given_and_ended, elsewhere) = (
            grant(100, 30, "home", "naming", 1_000),
            grant(110, 10, "home", "registry", 2_000),
            grant(105, 30, "home", "registry", 3_000),
            grant(120, 20, "work", "naming", 4_000),
        );
        for row in [&naming, &finding, &given_and_ended, &elsewhere] {
            agents.keep_grant(row, None).await.expect("kept");
        }
        agents
            .end_grants(&[minted(105)], 5_000, &id("ana"))
            .await
            .expect("ended");

        assert_eq!(
            agents.kept(&id("home")).await.expect("read"),
            AgentsKept {
                agents: vec![second, first],
                live: vec![naming, finding],
                grants_kept: 4,
            }
        );
        assert_eq!(
            agents.kept(&id("work")).await.expect("read"),
            AgentsKept {
                agents: vec![abroad],
                live: vec![elsewhere],
                grants_kept: 4,
            }
        );
        assert_eq!(
            agents.kept(&id("elsewhere")).await.expect("read"),
            AgentsKept {
                agents: Vec::new(),
                live: Vec::new(),
                grants_kept: 4,
            }
        );
        assert_eq!(
            ended(&store, 105).await,
            (Some(5_000), Some("ana".to_owned()))
        );
        store.close().await;
    }

    /// A grant is ended once, with when and by whom, and nothing else of it
    /// changes; an agent is never edited.
    #[tokio::test]
    async fn a_grant_is_ended_once_and_nothing_else_of_it_changes() {
        let (store, agents) = agents().await;
        agents
            .keep_agent(&agent(1, "home", "ana"))
            .await
            .expect("kept");
        for n in [10, 11] {
            agents
                .keep_grant(&grant(n, 1, "home", &format!("feature{n}"), 1_000), None)
                .await
                .expect("kept");
        }
        agents
            .end_grants(&[minted(10)], 2_000, &id("ana"))
            .await
            .expect("ended");
        let before = everything_in(&store).await;

        // Through the store: a grant ended already is not live.
        let again = agents
            .end_grants(&[minted(10)], 3_000, &id("carla"))
            .await
            .expect_err("a grant ended already");
        assert_eq!(again.code(), ErrorCode::DbLocalSchemaUnexpected, "{again}");
        assert!(
            again
                .to_string()
                .contains("ending a grant changed 0 row(s)"),
            "{again}"
        );

        // Around it: the file's own triggers.
        for (statement, says) in [
            (
                "UPDATE main.agent_grants SET ended_at = 3000, ended_by = 'carla' \
                 WHERE grant_id = 10",
                "ended once",
            ),
            (
                "UPDATE main.agent_grants SET reason = 'another' WHERE grant_id = 11",
                "ended once",
            ),
            (
                "UPDATE main.agent_grants SET ended_at = 3000, ended_by = 'ana', \
                 valid_until = 99999999 WHERE grant_id = 11",
                "ended once",
            ),
            (
                "UPDATE main.agent_grants SET ended_by = 'ana' WHERE grant_id = 11",
                "ended once",
            ),
            (
                "UPDATE main.agent_grants SET scope_id = 'feature10' WHERE grant_id = 10",
                "ended once",
            ),
            (
                "UPDATE main.agents SET label = 'another' WHERE agent = 1",
                "never edited",
            ),
            ("UPDATE main.agents SET budget = budget + 1", "never edited"),
        ] {
            let refused = tamper(&store, statement)
                .await
                .expect_err("the file refuses it");
            assert!(a_trigger_refused(&refused, says), "{statement}: {refused}");
        }
        // **With the checks off, the trigger holds**: an end with no one who
        // ended it is not one.
        tamper(&store, "PRAGMA ignore_check_constraints = ON")
            .await
            .expect("whoever can write the file can turn the checks off");
        let unsigned = tamper(
            &store,
            "UPDATE main.agent_grants SET ended_at = 3000 WHERE grant_id = 11",
        )
        .await
        .expect_err("an end with nobody who ended it");
        assert!(a_trigger_refused(&unsigned, "ended once"), "{unsigned}");
        tamper(&store, "PRAGMA ignore_check_constraints = OFF")
            .await
            .expect("and back on");
        assert_eq!(everything_in(&store).await, before);

        // And the one change a grant takes, made around the store, lands.
        tamper(
            &store,
            "UPDATE main.agent_grants SET ended_at = 3000, ended_by = 'ana' WHERE grant_id = 11",
        )
        .await
        .expect("an end once");
        assert_eq!(
            ended(&store, 11).await,
            (Some(3_000), Some("ana".to_owned()))
        );
        store.close().await;
    }

    /// Neither an agent nor a grant, live or ended, is ever deleted, whoever
    /// asks and however.
    #[tokio::test]
    async fn no_agent_and_no_grant_is_ever_deleted() {
        let (store, agents) = agents().await;
        agents
            .keep_agent(&agent(1, "home", "ana"))
            .await
            .expect("kept");
        for n in [10, 11] {
            agents
                .keep_grant(&grant(n, 1, "home", &format!("feature{n}"), 1_000), None)
                .await
                .expect("kept");
        }
        agents
            .end_grants(&[minted(10)], 2_000, &id("ana"))
            .await
            .expect("ended");
        let before = everything_in(&store).await;
        for (statement, says) in [
            (
                "DELETE FROM main.agent_grants WHERE grant_id = 10",
                "never deleted",
            ),
            (
                "DELETE FROM main.agent_grants WHERE grant_id = 11",
                "never deleted",
            ),
            ("DELETE FROM main.agent_grants", "never deleted"),
            ("DELETE FROM main.agents WHERE agent = 1", "never deleted"),
            ("DELETE FROM main.agents", "never deleted"),
        ] {
            let refused = tamper(&store, statement)
                .await
                .expect_err("the file refuses it");
            assert!(a_trigger_refused(&refused, says), "{statement}: {refused}");
        }
        assert_eq!(everything_in(&store).await, before);
        store.close().await;
    }

    /// The file holds one live grant per agent and scope.
    ///
    /// A second one, written without ending the first, is refused, and an
    /// ended one leaves room for the next.
    #[tokio::test]
    async fn a_second_live_grant_on_one_scope_is_refused_by_the_file() {
        let (store, agents) = agents().await;
        for n in [1, 2] {
            agents
                .keep_agent(&agent(n, "home", "ana"))
                .await
                .expect("kept");
        }
        agents
            .keep_grant(&grant(10, 1, "home", "naming", 1_000), None)
            .await
            .expect("the first");
        let before = everything_in(&store).await;
        let refused = agents
            .keep_grant(&grant(11, 1, "home", "naming", 2_000), None)
            .await
            .expect_err("a second live grant on one scope");
        let KeepError::Store(error) = refused else {
            panic!("not the file's refusal: {refused:?}");
        };
        assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
        assert!(error.to_string().contains("UNIQUE"), "{error}");
        assert_eq!(everything_in(&store).await, before);

        // Another scope, and another agent's same scope, are other grants.
        agents
            .keep_grant(&grant(12, 1, "home", "registry", 2_000), None)
            .await
            .expect("another scope");
        agents
            .keep_grant(&grant(13, 2, "home", "naming", 2_000), None)
            .await
            .expect("another agent");
        // And once the first is ended, the scope takes a live grant again.
        agents
            .end_grants(&[minted(10)], 3_000, &id("ana"))
            .await
            .expect("ended");
        agents
            .keep_grant(&grant(14, 1, "home", "naming", 4_000), None)
            .await
            .expect("after the end");
        store.close().await;
    }

    /// A grant that replaces the live one on its scope ends it in the same
    /// commit: both land, or neither does and the old one stays live.
    #[tokio::test]
    async fn a_superseding_grant_ends_the_old_one_in_the_same_commit() {
        let (store, agents) = agents().await;
        agents
            .keep_agent(&agent(1, "home", "ana"))
            .await
            .expect("kept");
        let first = grant(10, 1, "home", "naming", 1_000);
        agents.keep_grant(&first, None).await.expect("the first");
        agents
            .keep_grant(&grant(9, 1, "home", "registry", 1_000), None)
            .await
            .expect("another scope's");
        let second = grant(11, 1, "home", "naming", 2_000);
        agents
            .keep_grant(&second, Some((minted(10), 2_000, &id("ana"))))
            .await
            .expect("the second replaces the first");
        assert_eq!(
            ended(&store, 10).await,
            (Some(2_000), Some("ana".to_owned()))
        );
        assert_eq!(
            agents
                .kept(&id("home"))
                .await
                .expect("read")
                .live
                .iter()
                .map(|live| live.grant)
                .collect::<Vec<_>>(),
            vec![minted(9), minted(11)]
        );

        // A failure between the two: the end lands, the insert is refused —
        // its reason past what the file holds — and the commit never comes.
        let before = everything_in(&store).await;
        let too_long = GrantRow {
            reason: "r".repeat(257),
            ..grant(12, 1, "home", "naming", 3_000)
        };
        let refused = agents
            .keep_grant(&too_long, Some((minted(11), 3_000, &id("ana"))))
            .await
            .expect_err("a reason the file refuses");
        assert!(matches!(refused, KeepError::Store(_)), "{refused:?}");
        assert_eq!(everything_in(&store).await, before);
        assert_eq!(
            ended(&store, 11).await,
            (None, None),
            "the old one stays live"
        );

        // A grant it does not replace — ended already, of another scope, or
        // of nothing — is refused, and nothing lands.
        for (ends, what) in [
            (minted(10), "ended already"),
            (minted(9), "another scope's"),
            (minted(99), "no grant"),
        ] {
            let refused = agents
                .keep_grant(
                    &grant(13, 1, "home", "naming", 3_000),
                    Some((ends, 3_000, &id("ana"))),
                )
                .await
                .expect_err(what);
            let KeepError::Store(error) = refused else {
                panic!("{what}: {refused:?}");
            };
            assert!(
                error
                    .to_string()
                    .contains("ending the grant a new one replaces changed 0 row(s)"),
                "{what}: {error}"
            );
            assert_eq!(everything_in(&store).await, before, "{what}");
        }
        store.close().await;
    }

    /// One write the store makes, through a trigger that silences it.
    #[derive(Debug, Clone, Copy)]
    enum Silenced {
        /// Keeping an agent.
        Agent,
        /// Keeping a grant.
        Grant,
        /// Ending the grant a new one replaces.
        Replaced,
        /// Ending grants.
        Ended,
    }

    /// A write a trigger in `main` silences is refused, at each of the four
    /// places the agents are written, and the file is left as it was.
    ///
    /// **A trigger added to `main` after the store opened is one the
    /// open-time self-check cannot see**, so each write is believed for the
    /// rows it changed (invariant 22), and each is driven here through a
    /// trigger's `RAISE(IGNORE)`, which makes it report success having
    /// written nothing.
    #[tokio::test]
    async fn a_write_to_the_agents_a_trigger_silences_is_refused_and_leaves_nothing() {
        let mut tried = 0_usize;
        for (trigger, doing, silenced) in [
            (
                "CREATE TRIGGER main.silence BEFORE INSERT ON main.agents \
                 BEGIN SELECT RAISE(IGNORE); END",
                "keeping an agent",
                Silenced::Agent,
            ),
            (
                "CREATE TRIGGER main.silence BEFORE INSERT ON main.agent_grants \
                 BEGIN SELECT RAISE(IGNORE); END",
                "keeping a grant",
                Silenced::Grant,
            ),
            (
                "CREATE TRIGGER main.silence BEFORE UPDATE ON main.agent_grants \
                 BEGIN SELECT RAISE(IGNORE); END",
                "ending the grant a new one replaces",
                Silenced::Replaced,
            ),
            (
                "CREATE TRIGGER main.silence BEFORE UPDATE ON main.agent_grants \
                 BEGIN SELECT RAISE(IGNORE); END",
                "ending a grant",
                Silenced::Ended,
            ),
        ] {
            let (store, agents) = agents().await;
            agents
                .keep_agent(&agent(1, "home", "ana"))
                .await
                .expect("kept");
            agents
                .keep_grant(&grant(10, 1, "home", "naming", 1_000), None)
                .await
                .expect("kept");
            tamper(&store, trigger)
                .await
                .expect("whoever can write the file can add a trigger");
            let before = everything_in(&store).await;
            let answered = match silenced {
                Silenced::Agent => agents.keep_agent(&agent(2, "home", "ana")).await,
                Silenced::Grant => {
                    agents
                        .keep_grant(&grant(11, 1, "home", "registry", 1_000), None)
                        .await
                }
                Silenced::Replaced => {
                    agents
                        .keep_grant(
                            &grant(11, 1, "home", "naming", 2_000),
                            Some((minted(10), 2_000, &id("ana"))),
                        )
                        .await
                }
                Silenced::Ended => agents
                    .end_grants(&[minted(10)], 2_000, &id("ana"))
                    .await
                    .map_err(KeepError::Store),
            };
            let Err(KeepError::Store(error)) = answered else {
                panic!("{doing}: a silenced write was answered {answered:?}");
            };
            assert_eq!(
                error.code(),
                ErrorCode::DbLocalSchemaUnexpected,
                "{doing}: {error}"
            );
            assert!(
                error
                    .to_string()
                    .contains(&format!("{doing} changed 0 row(s)")),
                "{doing}: {error}"
            );
            assert_eq!(
                everything_in(&store).await,
                before,
                "{doing}: a refused write left something behind"
            );
            store.close().await;
            tried += 1;
        }
        assert_eq!(
            tried, 4,
            "every statement the agents are written with is one case"
        );
    }

    /// A temporary object on the connection stops every write to the agents
    /// before it is made.
    #[tokio::test]
    async fn a_temporary_object_on_the_connection_stops_every_write_to_the_agents() {
        let (store, agents) = agents().await;
        agents
            .keep_agent(&agent(1, "home", "ana"))
            .await
            .expect("kept");
        agents
            .keep_grant(&grant(10, 1, "home", "naming", 1_000), None)
            .await
            .expect("kept");
        tamper(
            &store,
            "CREATE TEMP TRIGGER watching BEFORE INSERT ON main.agents BEGIN SELECT 1; END",
        )
        .await
        .expect("the connection takes a temporary trigger");
        let before = everything_in(&store).await;
        for answered in [
            agents.keep_agent(&agent(2, "home", "ana")).await,
            agents
                .keep_grant(&grant(11, 1, "home", "registry", 1_000), None)
                .await,
            agents
                .end_grants(&[minted(10)], 2_000, &id("ana"))
                .await
                .map_err(KeepError::Store),
        ] {
            let Err(KeepError::Store(error)) = answered else {
                panic!("a write went past a temporary object: {answered:?}");
            };
            assert!(
                error.to_string().contains("temporary object(s) exist"),
                "{error}"
            );
        }
        assert_eq!(everything_in(&store).await, before);
        store.close().await;
    }

    /// A table at its bound refuses a new row, counting every realm's rows
    /// and every ended grant, and keeps what it holds.
    #[tokio::test]
    async fn a_table_at_its_bound_refuses_a_new_row_and_keeps_what_it_holds() {
        let (store, roomy) = agents().await;
        assert_eq!(roomy.bounds, Bounds::of_the_catalog());
        let agents = LocalAgents {
            bounds: Bounds {
                agents: 2,
                grants: 2,
            },
            ..roomy
        };
        agents
            .keep_agent(&agent(1, "home", "ana"))
            .await
            .expect("the first");
        agents
            .keep_agent(&agent(2, "work", "ana"))
            .await
            .expect("the second, in another realm");
        let before = everything_in(&store).await;
        assert_eq!(
            agents.keep_agent(&agent(3, "home", "ana")).await,
            Err(KeepError::Full { table: "agents" })
        );
        assert_eq!(everything_in(&store).await, before);

        agents
            .keep_grant(&grant(10, 1, "home", "naming", 1_000), None)
            .await
            .expect("the first");
        agents
            .end_grants(&[minted(10)], 2_000, &id("ana"))
            .await
            .expect("ended");
        agents
            .keep_grant(&grant(11, 2, "work", "naming", 1_000), None)
            .await
            .expect("the second, in another realm");
        let before = everything_in(&store).await;
        for ends in [None, Some((minted(11), 3_000, &id("ana")))] {
            assert_eq!(
                agents
                    .keep_grant(&grant(12, 2, "work", "naming", 3_000), ends)
                    .await,
                Err(KeepError::Full {
                    table: "agent_grants"
                }),
                "{ends:?}"
            );
            assert_eq!(everything_in(&store).await, before, "{ends:?}");
        }
        // Ending takes no room.
        agents
            .end_grants(&[minted(11)], 3_000, &id("ana"))
            .await
            .expect("an end adds no row");
        store.close().await;
    }

    /// A refusal to keep names the full table, or carries the store's own
    /// failure as its source.
    #[test]
    fn a_refusal_to_keep_names_the_full_table_or_carries_the_stores_failure() {
        use std::error::Error as _;
        let full = KeepError::Full { table: "agents" };
        assert!(
            full.to_string().contains("`agents` already holds"),
            "{full}"
        );
        assert!(full.source().is_none());
        let failed = LocalError::Unavailable("busy".to_owned());
        let kept = KeepError::from(failed.clone());
        assert_eq!(kept, KeepError::Store(failed.clone()));
        assert!(kept.to_string().ends_with(&failed.to_string()), "{kept}");
        assert_eq!(
            kept.source().map(ToString::to_string),
            Some(failed.to_string())
        );
    }

    /// The bounds are the catalog's `max_rows`, and a table the catalog does
    /// not bound takes no row at all.
    #[test]
    fn the_bounds_are_the_catalogs_and_a_table_it_does_not_bound_takes_no_row() {
        let declared = |table: &str| {
            nc_catalog::TABLES
                .iter()
                .find(|declared| declared.name == table)
                .and_then(|declared| declared.max_rows)
                .filter(|bound| *bound > 0)
                .unwrap_or_else(|| panic!("the catalog bounds `{table}`"))
        };
        assert_eq!(
            Bounds::of_the_catalog(),
            Bounds {
                agents: declared("agents"),
                grants: declared("agent_grants"),
            }
        );
        assert_eq!(max_rows("no_such_table"), 0);
    }

    /// A row this build does not write is refused when it is read.
    ///
    /// Each is written here with the file's checks off, as anybody with write
    /// access to it can, and the refusal names what is wrong with it.
    #[tokio::test]
    async fn a_row_this_build_does_not_write_is_refused_when_read() {
        for (planted, says) in [
            (
                "INSERT INTO main.agents VALUES (-5, 'home', 'ana', 'a', 1)",
                "agent of -5 is not an identifier the minter gives",
            ),
            (
                "INSERT INTO main.agents VALUES (5, 'home', 'not one', 'a', 1)",
                "operator is not an identifier",
            ),
            (
                "INSERT INTO main.agents VALUES (5, 'home', 'ana', 'a', 0)",
                "budget of 0 is not one this build writes",
            ),
            (
                "INSERT INTO main.agents VALUES (5, 'home', 'ana', 'a', 4294967296)",
                "budget of 4294967296 is not one this build writes",
            ),
            (
                "INSERT INTO main.agent_grants (grant_id, agent, realm, scope_type, scope_id, \
                 granted_by, reason, valid_from, valid_until) \
                 VALUES (10, 1, 'home', 'Feature', 'naming', 'ana', 'r', 5, 5)",
                "runs from 5 to 5",
            ),
            (
                "INSERT INTO main.agent_grants (grant_id, agent, realm, scope_type, scope_id, \
                 granted_by, reason, valid_from, valid_until) \
                 VALUES (10, 1, 'home', 'Feature', 'naming', 'ana', 'r', -1, 5)",
                "a grant's time of -1 is below zero",
            ),
            (
                "INSERT INTO main.agent_grants (grant_id, agent, realm, scope_type, scope_id, \
                 granted_by, reason, valid_from, valid_until) \
                 VALUES (-10, 1, 'home', 'Feature', 'naming', 'ana', 'r', 5, 6)",
                "grant_id of -10 is not an identifier the minter gives",
            ),
            (
                "INSERT INTO main.agent_grants (grant_id, agent, realm, scope_type, scope_id, \
                 granted_by, reason, valid_from, valid_until) \
                 VALUES (10, 1, 'home', 'Feature', 'naming', '', 'r', 5, 6)",
                "granted_by is not an identifier",
            ),
        ] {
            let (store, agents) = agents().await;
            agents
                .keep_agent(&agent(1, "home", "ana"))
                .await
                .expect("kept");
            tamper(&store, "PRAGMA ignore_check_constraints = ON")
                .await
                .expect("whoever can write the file can turn the checks off");
            tamper(&store, planted)
                .await
                .unwrap_or_else(|error| panic!("{planted}: {error}"));
            let refused = agents
                .kept(&id("home"))
                .await
                .expect_err("a row this build does not write");
            assert!(
                matches!(refused, LocalError::Unexpected(ref why) if why.contains(says)),
                "{planted}: {refused:?}"
            );
            store.close().await;
        }
    }
}
