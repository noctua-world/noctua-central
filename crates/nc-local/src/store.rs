//! The plugin store: committed reads, and a unit's commit as one transaction.
//!
//! **An open unit never touches this file.** Its writes live in the kernel
//! until it commits (`NOCTUA.md §11.5`), and [`LocalStore::commit`] is one
//! `BEGIN IMMEDIATE`: every key is checked against what the unit expected,
//! the rows are written, and the unit's row, its images, its notes, the
//! blobs it created and the pins it takes off the blobs it reverses land with
//! them — or nothing does. A unit that was aborted,
//! dropped or killed, or whose daemon stopped, left no trace here, because
//! there was never anything to take back.
//!
//! **The plugin is named on every read and every write**, and it is the
//! kernel's to name: a plugin supplies keys and values, never the plugin they
//! belong to, and never SQL.
//!
//! These types are the kernel's too: its `StoreBackend` wraps this store, and
//! a unit record is what its overlay becomes when the unit commits.

use std::collections::BTreeMap;
use std::path::Path;

use nc_ids::{OpaqueId, Snowflake};
use sqlx::Row as _;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow};

use crate::{LocalError, said, schema};

/// Where one key's committed value came from: the unit that wrote it, and
/// which of that unit's writes it was.
///
/// Written as `"{unit in Crockford base32}.{n}"`, which is what a plugin is
/// handed as a revision and what it names when it writes back. Never reused
/// by another unit, because a unit is never reused: a key restored by undo is
/// written by the undo's unit and carries a revision of its own. **Inside one
/// unit it names the key, not each value**: every write of a key there is
/// handed the revision the unit commits it with. **Reading one back from its
/// text is not offered here**, because that is `Snowflake::from_crockford`
/// over text a plugin supplies, and the kernel reaches the central log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision {
    /// The unit that wrote it.
    pub unit: Snowflake,
    /// Its position among that unit's writes.
    pub n: u32,
}

impl core::fmt::Display for Revision {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}.{}", self.unit.to_crockford(), self.n)
    }
}

/// One committed key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its value, as nc-canon's canonical text.
    pub value: String,
    /// Where it came from.
    pub revision: Revision,
}

/// One page of keys.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Page {
    /// The keys, in byte order.
    pub keys: Vec<String>,
    /// What to hand back for the next page, when there is one: the last key
    /// on this page, after which the next begins.
    pub next: Option<String>,
}

/// What a write expects to find when it lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    /// Nothing: the write lands over whatever is there.
    Anything,
    /// The key holds nothing.
    Absent,
    /// The key holds exactly this revision — what the unit read.
    Revision(Revision),
    /// The key holds this value, or nothing when `None` — what undo and redo
    /// check, because the revision an image was taken at has since been
    /// replaced by the step being reversed.
    Value(Option<String>),
}

/// One key a unit writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedWrite {
    /// The plugin the key belongs to, bound by the kernel from the manifest.
    pub plugin: String,
    /// The key.
    pub key: String,
    /// What the key must hold when the unit commits.
    pub expect: Expect,
    /// What it holds afterwards, as canonical text, or `None` to delete it.
    pub after: Option<String>,
}

/// A blob a unit created, recorded with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobCreated {
    /// Its BLAKE3, sixty-four lowercase hex characters.
    pub hash: String,
    /// Its size in bytes.
    pub size: u64,
}

/// What kind of unit this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    /// Ordinary work.
    Step,
    /// The reversal of `target`.
    Undo {
        /// The unit it reverses.
        target: Snowflake,
    },
    /// The reversal of the undo `target`.
    Redo {
        /// The undo it reverses.
        target: Snowflake,
    },
}

/// Where a committed unit stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitState {
    /// Its writes are what the store holds, as far as it is concerned.
    Committed,
    /// Reversed, by this unit.
    Undone {
        /// The unit that reversed it.
        by: Snowflake,
    },
}

/// Who a unit was for, and what it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitMeta {
    /// The realm it ran in.
    pub realm: OpaqueId,
    /// The principal it ran as.
    pub principal: OpaqueId,
    /// Step, undo or redo.
    pub kind: UnitKind,
    /// What a person is shown for it.
    pub label: String,
    /// When it committed, in milliseconds since the Unix epoch.
    pub committed_at_ms: u64,
}

/// Everything one unit commits, in one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitRecord {
    /// Its identifier: the journal's work unit.
    pub unit: Snowflake,
    /// Who it was for, and what it was.
    pub meta: UnitMeta,
    /// The net write per key. One key once.
    pub writes: Vec<StagedWrite>,
    /// The blobs it created.
    pub blobs: Vec<BlobCreated>,
    /// The blobs it takes the pin off, as of when it commits: what the unit it
    /// reverses created and no committed unit held before it.
    ///
    /// **In the same transaction as the rest**, so a reversal that lands has
    /// let go of the bytes the reversed unit made and one that does not still
    /// holds them, and the sweep never finds a blob unpinned by a reversal the
    /// store refused.
    pub unpinned: Vec<String>,
    /// What else it did, each as canonical JSON, in order.
    pub notes: Vec<String>,
    /// Other units whose state changes with this one — an undo marks its
    /// target `Undone` in the same transaction that restores its images.
    pub marks: Vec<(Snowflake, UnitState)>,
}

/// Whether a commit holds the unit from the pruner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hold {
    /// It holds nothing: a reversal the kernel commits, which no journal
    /// opened, or a unit a test seeds.
    None,
    /// It holds the unit and every unit it marks.
    Placed,
}

/// The bounds a commit is held to.
///
/// **Handed in, not read here**: the numbers are `catalog/limits.toml`'s
/// `max_rows` for `store_entries`, `blobs`, `store_writes` and `store_notes`,
/// and a plugin's manifest may lower its own — which only the kernel knows.
///
/// **Per plugin, because a unit is not.** One unit writes for every plugin a
/// nested dispatch reaches, so one number for the whole commit would judge one
/// of them wrongly whatever the kernel passed. Measured, a unit writing three
/// keys of `big` (whose quota is three) and two of `small` (whose manifest
/// lowers it to one): handed one, the commit would refuse `big` within its
/// three; handed three, it would commit and leave `small` holding two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quotas {
    /// The keys a plugin may hold when its manifest does not lower the number.
    pub entries_per_plugin: u64,
    /// The plugins whose manifest lowers it, each with its own number.
    pub lowered: BTreeMap<String, u64>,
    /// The blobs this machine may hold.
    pub blobs: u64,
    /// The images one unit may record — one per key it writes: `store_writes`'
    /// `max_rows`, counted over each unit's rows.
    pub writes_per_unit: u64,
    /// The notes one unit may record: `store_notes`' `max_rows`, counted over
    /// each unit's rows.
    pub notes_per_unit: u64,
}

impl Quotas {
    /// The keys `plugin` may hold: its manifest's number when it lowers the
    /// catalog's, and the catalog's otherwise.
    ///
    /// **Lowered and never raised**: a number above
    /// [`entries_per_plugin`](Self::entries_per_plugin) is held to it, because
    /// that is the bound `catalog/limits.toml` declares for every plugin.
    #[must_use]
    pub fn entries_for(&self, plugin: &str) -> u64 {
        self.lowered
            .get(plugin)
            .map_or(self.entries_per_plugin, |own| {
                (*own).min(self.entries_per_plugin)
            })
    }
}

/// What a commit made: the revision of each write, in order, `None` for a
/// delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// One per write.
    pub revisions: Vec<Option<Revision>>,
}

/// Why a unit did not commit. Nothing it wrote is in the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitError {
    /// A key did not hold what the unit expected: somebody else wrote it.
    Conflict {
        /// Whose key.
        plugin: String,
        /// Which.
        key: String,
    },
    /// The unit would have left a plugin holding more keys than its quota.
    Quota {
        /// Whose.
        plugin: String,
    },
    /// The unit would have left this machine holding more blobs than its bound.
    TooManyBlobs,
    /// The record is not one a unit can be: a key written twice, a value that
    /// is not canonical, a blob hash that is not one, a blob both pinned and
    /// unpinned or unpinned twice, a blob unpinned that the store does not
    /// hold, a mark on a unit the store does not hold, a unit the store
    /// already holds, more images or notes than one unit may record.
    Invalid(String),
    /// The database failed.
    Store(LocalError),
}

impl From<LocalError> for CommitError {
    fn from(error: LocalError) -> Self {
        Self::Store(error)
    }
}

impl core::fmt::Display for CommitError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Conflict { plugin, key } => write!(
                formatter,
                "the unit did not commit: `{plugin}`'s key `{key}` no longer held what the \
                 unit expected, because something else wrote it"
            ),
            Self::Quota { plugin } => write!(
                formatter,
                "the unit did not commit: it would have left `{plugin}` holding more keys \
                 than its quota"
            ),
            Self::TooManyBlobs => formatter.write_str(
                "the unit did not commit: it would have left this machine holding more \
                 blobs than its bound",
            ),
            Self::Invalid(why) => write!(
                formatter,
                "the unit did not commit: it is not a record a unit can be: {why}"
            ),
            Self::Store(error) => write!(formatter, "the unit did not commit: {error}"),
        }
    }
}

/// A commit's refusal is an error, as `NOCTUA.md §15.1` says every error type
/// here is.
///
/// **So `?` carries it into a `Box<dyn Error>`**: without this impl a commit's
/// refusal would not convert into one, and the database failure inside it
/// would not be its `source`.
impl std::error::Error for CommitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Conflict { .. } | Self::Quota { .. } | Self::TooManyBlobs | Self::Invalid(_) => {
                None
            }
        }
    }
}

/// One committed unit, as a listing shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitRow {
    /// Its identifier.
    pub unit: Snowflake,
    /// Who it was for, and what it was.
    pub meta: UnitMeta,
    /// Where it stands.
    pub state: UnitState,
}

/// One key's before and after image in one unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Whose key.
    pub plugin: String,
    /// Which.
    pub key: String,
    /// What it held before the unit, or `None` when it held nothing.
    pub before: Option<Entry>,
    /// What it held after, or `None` when the unit deleted it.
    pub after: Option<Entry>,
}

/// One committed unit, whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitDetail {
    /// The unit.
    pub row: UnitRow,
    /// Its images, in key order.
    pub images: Vec<Image>,
    /// Its notes, in the order it made them.
    pub notes: Vec<String>,
}

/// One blob this machine holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRow {
    /// Its BLAKE3.
    pub hash: String,
    /// Its size in bytes.
    pub size: u64,
    /// The unit that first created it.
    pub created_unit: Snowflake,
    /// Whether a committed unit still holds it.
    pub pinned: bool,
    /// When it was unpinned, in milliseconds since the Unix epoch.
    pub unpinned_at_ms: Option<u64>,
}

/// The plugin store over one local database.
#[derive(Debug, Clone)]
pub struct LocalStore {
    pool: SqlitePool,
}

/// How many connections the pool may hold. Readers run beside the one writer
/// under WAL; a writer waits for another on `busy_timeout`.
const CONNECTIONS: u32 = 4;

impl LocalStore {
    /// Opens the local database in `directory`, creating both when missing.
    ///
    /// The steps and what each refuses are the crate's header.
    ///
    /// # Errors
    ///
    /// [`LocalError::Refused`] when the directory or a file is not private to
    /// this user; [`LocalError::Migration`] when a migration fails;
    /// [`LocalError::Unexpected`] when the file is another application's, the
    /// self-check fails or the file's migrations are not this build's;
    /// [`LocalError::Unavailable`] when SQLite or the filesystem cannot be
    /// reached — a parent directory that does not exist, a full disk.
    pub async fn open(directory: &Path) -> Result<Self, LocalError> {
        use sqlx::{ConnectOptions as _, Connection as _};
        let (file, _one_open_at_a_time) = crate::open::off_the_runtime(directory).await?;
        // **Judged on a connection that writes nothing, and only then the
        // pool**, whose every connection sets `journal_mode = WAL` — which
        // converts a file in another journal mode before anything has asked
        // whose it is. Read-only, so that neither its first read nor its close
        // writes the file (`crate::open::for_judging`); and with the profile
        // held (`crate::open::private_file_in`), so that no other open
        // migrates it between two of its reads — measured, a history read after
        // another open's migrations landed would be held against a schema read
        // before them, and refused.
        let mut judging = crate::open::for_judging(&file)
            .connect()
            .await
            .map_err(|error| unread("opening the local database", &error))?;
        let judged = judged_before_migrating(&mut judging).await;
        let closed = judging.close().await;
        judged?;
        closed.map_err(|error| said("closing the connection that judged the file", &error))?;
        let pool = SqlitePoolOptions::new()
            .max_connections(CONNECTIONS)
            .connect_with(crate::open::options(&file))
            .await
            .map_err(|error| said("opening the local database", &error))?;
        Self::over(pool).await
    }

    /// A store held in memory, which forgets everything when it is dropped.
    ///
    /// **For the model-based test**, which runs thousands of units and wants
    /// them measured against a `BTreeMap` rather than against a disk. One
    /// connection that never closes, because each in-memory connection is a
    /// database of its own. Behind `test-support`, so a kernel cannot be built
    /// on a store that forgets.
    ///
    /// # Errors
    ///
    /// As `LocalStore::open`, less the file.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub async fn in_memory_for_tests() -> Result<Self, LocalError> {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .in_memory(true)
            .foreign_keys(true)
            .pragma("trusted_schema", "OFF");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await
            .map_err(|error| said("opening a database in memory", &error))?;
        let mut connection = pool
            .acquire()
            .await
            .map_err(|error| said("reaching the database in memory", &error))?;
        judged_before_migrating(&mut connection).await?;
        drop(connection);
        Self::over(pool).await
    }

    /// The migrations and the self-check after them, on one connection of a
    /// pool over a file already judged by [`judged_before_migrating`].
    async fn over(pool: SqlitePool) -> Result<Self, LocalError> {
        let mut connection = pool
            .acquire()
            .await
            .map_err(|error| said("reaching the local database", &error))?;
        // **`run_direct`, which is what `run` calls**, and not `run`: `run`
        // takes anything that can `Acquire` a connection, and the compiler
        // cannot prove that bound for every lifetime a spawned task needs —
        // so through `run`, `open`'s future would not be `Send`, and neither
        // `tokio::spawn` nor a `Send`-bound kernel future could hold it
        // ("implementation of `Acquire` is not general enough"). `sqlx` keeps
        // this door for that error; `false` is "apply, do not skip".
        // `every_call_the_store_offers_runs_in_a_spawned_task` does not compile
        // without it.
        MIGRATOR
            .run_direct(None, &mut *connection, false)
            .await
            .map_err(|error| migrated(&error))?;
        schema::after_migrating(&mut connection).await?;
        drop(connection);
        Ok(Self { pool })
    }

    /// The pool, for the pruner's SQLite target (`nc_prune::sqlite`).
    #[must_use]
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// The journal's store in this same file, over this store's pool
    /// (`crate::journal`).
    ///
    /// **One file, opened and judged once**: the journal's table is built by
    /// `local-migrations/` and held to [`crate::schema::EXPECTED`] with every
    /// other object, so a journal is only ever handed out over a database
    /// this build opened.
    #[must_use]
    pub fn journal(&self) -> crate::LocalJournal {
        crate::LocalJournal::over(self.pool.clone())
    }

    /// Closes every connection, which checkpoints the WAL into the file.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// One key's committed value, a plugin's in a realm.
    ///
    /// **Every key is a realm's**, as every entity is (`NOCTUA.md §10.3`): the
    /// kernels of two realms on one machine share this file, and a key one
    /// writes is never one the other reads.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked, or holds a row this crate did not write.
    pub async fn get(
        &self,
        realm: &OpaqueId,
        plugin: &str,
        key: &str,
    ) -> Result<Option<Entry>, LocalError> {
        let row = sqlx::query(
            "SELECT value, rev_unit, rev_n FROM main.store_entries \
             WHERE realm = ?1 AND plugin = ?2 AND store_key = ?3",
        )
        .bind(realm.as_str())
        .bind(plugin)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| said("reading a key", &error))?;
        row.map(|row| entry(&row, "value", "rev_unit", "rev_n"))
            .transpose()
    }

    /// Up to `limit` of a plugin's keys in a realm beginning with `prefix`,
    /// after `cursor` when one is given, in byte order.
    ///
    /// **A limit of zero is read as one.** A page that holds nothing has no
    /// last key to hand back as where the next begins, and one with no `next`
    /// says the listing is over — which is what a limit of zero would answer
    /// over a plugin holding keys: `Page { keys: [], next: None }`. One key is
    /// the smallest page that can say where the rest begins.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked.
    pub async fn list(
        &self,
        realm: &OpaqueId,
        plugin: &str,
        prefix: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<Page, LocalError> {
        let limit = limit.max(1);
        // A key begins with `prefix` exactly when it sorts at or after it and
        // before the first text that sorts after every such key — a range
        // SQLite walks in the primary key's order, with no pattern syntax a
        // prefix could smuggle a `%` into.
        let upper = past_every_key_beginning_with(prefix);
        let rows = sqlx::query(
            "SELECT store_key FROM main.store_entries \
             WHERE realm = ?1 AND plugin = ?2 AND store_key >= ?3 \
             AND (?4 IS NULL OR store_key < ?4) AND (?5 IS NULL OR store_key > ?5) \
             ORDER BY store_key ASC LIMIT ?6",
        )
        .bind(realm.as_str())
        .bind(plugin)
        .bind(prefix)
        .bind(upper.as_deref())
        .bind(cursor)
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| said("listing keys", &error))?;
        let mut keys = Vec::with_capacity(rows.len());
        for row in &rows {
            keys.push(
                row.try_get::<String, _>("store_key")
                    .map_err(|error| said("reading a key", &error))?,
            );
        }
        let more = keys.len() > usize::try_from(limit).unwrap_or(usize::MAX);
        keys.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let next = if more { keys.last().cloned() } else { None };
        Ok(Page { keys, next })
    }

    /// How many keys a plugin holds in a realm.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked.
    pub async fn count(&self, realm: &OpaqueId, plugin: &str) -> Result<u64, LocalError> {
        let held: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM main.store_entries WHERE realm = ?1 AND plugin = ?2",
        )
        .bind(realm.as_str())
        .bind(plugin)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| said("counting keys", &error))?;
        as_unsigned(held, "a count of keys")
    }

    /// Commits one unit, whole or not at all.
    ///
    /// # Errors
    ///
    /// [`CommitError::Conflict`] when a key does not hold what the unit
    /// expected; [`CommitError::Quota`] and [`CommitError::TooManyBlobs`] at a
    /// bound; [`CommitError::Invalid`] for a record no unit can be;
    /// [`CommitError::Store`] when the database fails. In every case nothing
    /// the unit wrote is in the store.
    pub async fn commit(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> Result<Committed, CommitError> {
        self.commit_with(unit, quotas, Hold::None, |_| {}).await
    }

    /// Commits one unit, whole or not at all, holding it — and every unit
    /// whose state it moves — from the pruner until [`release`](Self::release)
    /// lets go of what it holds.
    ///
    /// **What a kernel commits a unit it has yet to hear the journal close
    /// with.** The kernels of the realms one person belongs to share this file
    /// and the pruner takes the oldest units of every realm, while a kernel
    /// knows only the units its own journal holds open: a unit one realm's
    /// stopped process left committed here and open in its journal is the
    /// record that realm's recovery reverts it from, and its reversal puts
    /// back the state of each unit it moved, so each is held where every
    /// realm's prune sees it (`store_unit_holds`, which the pruner's SQLite
    /// target skips a row of).
    ///
    /// # Errors
    ///
    /// As [`commit`](Self::commit).
    pub async fn commit_held(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> Result<Committed, CommitError> {
        self.commit_with(unit, quotas, Hold::Placed, |_| {}).await
    }

    /// Lets go of every hold `holder` placed when it was committed, answering
    /// how many there were.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked, or the statement changed another number of
    /// rows than the holds it found.
    pub async fn release(&self, holder: Snowflake) -> Result<u64, LocalError> {
        let id = as_signed(holder.value(), "a unit")?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("beginning a release", &error))?;
        schema::no_temporary_object(&mut transaction).await?;
        let held: i64 =
            sqlx::query_scalar("SELECT count(*) FROM main.store_unit_holds WHERE holder = ?1")
                .bind(id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| said("reading a unit's holds", &error))?;
        let held = as_unsigned(held, "a count of holds")?;
        write_exactly(
            sqlx::query("DELETE FROM main.store_unit_holds WHERE holder = ?1")
                .bind(id)
                .execute(&mut *transaction)
                .await,
            held,
            "letting go of a unit's holds",
        )?;
        transaction
            .commit()
            .await
            .map_err(|error| said("finishing a release", &error))?;
        Ok(held)
    }

    /// Every unit holding another or itself from the pruner, oldest first:
    /// the units committed whose close no kernel has said it heard, of every
    /// realm the file holds.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked, or holds a row this crate did not write.
    pub async fn holders(&self) -> Result<Vec<Snowflake>, LocalError> {
        let holders: Vec<i64> =
            sqlx::query_scalar("SELECT DISTINCT holder FROM main.store_unit_holds ORDER BY holder")
                .fetch_all(&self.pool)
                .await
                .map_err(|error| said("listing the holders", &error))?;
        holders.into_iter().map(snowflake).collect()
    }

    /// Commits one unit and stops the process after `after` of its statements.
    ///
    /// **For the kill test**, which runs this in a child process and then opens
    /// the file to find the unit absent or whole. `std::process::abort` runs no
    /// destructor, flushes nothing and rolls nothing back — what a crash or a
    /// `kill -9` leaves. A power cut also loses what the operating system had
    /// not yet written, which this does not simulate — the page cache outlives
    /// the process — and which `synchronous = FULL` is what answers: measured,
    /// the test passes with `OFF`. A count past the last statement lets the
    /// commit finish, and the one just past `COMMIT` stops the process after
    /// it. Behind `test-support`, because a kernel that could abort itself here
    /// is a kernel with a door out of every unit.
    ///
    /// # Errors
    ///
    /// As `LocalStore::commit`.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub async fn commit_then_abort_for_tests(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
        after: usize,
    ) -> Result<Committed, CommitError> {
        self.commit_with(unit, quotas, Hold::Placed, |step| {
            if step == after {
                std::process::abort();
            }
        })
        .await
    }

    /// The commit, holding what `hold` says, calling `between` with a running
    /// count before each statement and once after `COMMIT`.
    async fn commit_with(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
        hold: Hold,
        between: impl FnMut(usize) + Send,
    ) -> Result<Committed, CommitError> {
        within_what_one_unit_may_record(unit, quotas)?;
        let shaped = Shaped::of(unit)?;
        let mut steps = Steps { count: 0, between };
        steps.next();
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("beginning a unit's commit", &error))?;
        steps.next();
        schema::no_temporary_object(&mut transaction).await?;
        steps.next();
        record_the_unit(&mut transaction, unit, &shaped).await?;
        let mut revisions = Vec::with_capacity(unit.writes.len());
        let mut grew: Vec<&str> = Vec::new();
        for (n, write) in unit.writes.iter().enumerate() {
            let revision = Revision {
                unit: unit.unit,
                n: u32::try_from(n).map_err(|_| {
                    CommitError::Invalid(format!("a unit writes at most {} keys", u32::MAX))
                })?,
            };
            let new_key = write_one(
                &mut transaction,
                &mut steps,
                &unit.meta.realm,
                write,
                revision,
            )
            .await?;
            if new_key && !grew.contains(&write.plugin.as_str()) {
                grew.push(&write.plugin);
            }
            revisions.push(write.after.as_ref().map(|_| revision));
        }
        for plugin in grew {
            steps.next();
            within_its_quota(&mut transaction, &unit.meta.realm, plugin, quotas).await?;
        }
        for blob in &unit.blobs {
            record_blob(
                &mut transaction,
                &mut steps,
                blob,
                shaped.unit,
                quotas.blobs,
            )
            .await?;
        }
        for hash in &unit.unpinned {
            steps.next();
            if !unpin(&mut transaction, hash, shaped.committed_at).await? {
                return Err(CommitError::Invalid(format!(
                    "blob `{hash}` is unpinned by unit {} and the store does not hold it",
                    unit.unit.to_crockford()
                )));
            }
        }
        for (seq, note) in unit.notes.iter().enumerate() {
            steps.next();
            write_exactly(
                sqlx::query("INSERT INTO main.store_notes (unit, seq, note) VALUES (?1, ?2, ?3)")
                    .bind(shaped.unit)
                    .bind(as_signed(
                        u64::try_from(seq).unwrap_or(u64::MAX),
                        "a note's place",
                    )?)
                    .bind(note)
                    .execute(&mut *transaction)
                    .await,
                1,
                "recording a note",
            )?;
        }
        for (marked, state) in &unit.marks {
            steps.next();
            if !mark(&mut transaction, *marked, *state).await? {
                return Err(CommitError::Invalid(format!(
                    "unit {} is marked by unit {} and the store does not hold it",
                    marked.to_crockford(),
                    unit.unit.to_crockford()
                )));
            }
        }
        if hold == Hold::Placed {
            held_from_the_pruner(&mut transaction, &mut steps, unit, shaped.unit).await?;
        }
        steps.next();
        transaction
            .commit()
            .await
            .map_err(|error| said("committing a unit", &error))?;
        steps.next();
        Ok(Committed { revisions })
    }

    /// A principal's committed units in a realm, newest first.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked, or holds a row this crate did not write.
    pub async fn units(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> Result<Vec<UnitRow>, LocalError> {
        let rows = sqlx::query(
            "SELECT unit, realm, principal, kind, target, label, state, undone_by, committed_at \
             FROM main.store_units WHERE realm = ?1 AND principal = ?2 \
             ORDER BY unit DESC LIMIT ?3",
        )
        .bind(realm.as_str())
        .bind(principal.as_str())
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| said("listing units", &error))?;
        rows.iter().map(unit_row).collect()
    }

    /// A principal's steps in a realm, newest first: the units of theirs the
    /// store keeps that are neither an undo nor a redo, undone or not.
    ///
    /// **Where an undo looks for the step it undoes**, so its bound counts
    /// steps alone: counted among every unit, a step redone and undone often
    /// enough was pushed past the bound by its own undos and redos, and was
    /// then nothing to undo.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when SQLite cannot be asked, or holds a row this crate
    /// did not write.
    pub async fn steps(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> Result<Vec<UnitRow>, LocalError> {
        let rows = sqlx::query(
            "SELECT unit, realm, principal, kind, target, label, state, undone_by, committed_at \
             FROM main.store_units WHERE realm = ?1 AND principal = ?2 AND kind = 'step' \
             ORDER BY unit DESC LIMIT ?3",
        )
        .bind(realm.as_str())
        .bind(principal.as_str())
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| said("listing steps", &error))?;
        rows.iter().map(unit_row).collect()
    }

    /// One committed unit, with its images and its notes.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked, or holds a row this crate did not write.
    pub async fn unit(&self, unit: Snowflake) -> Result<Option<UnitDetail>, LocalError> {
        let id = as_signed(unit.value(), "a unit")?;
        let Some(row) = sqlx::query(
            "SELECT unit, realm, principal, kind, target, label, state, undone_by, committed_at \
             FROM main.store_units WHERE unit = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| said("reading a unit", &error))?
        else {
            return Ok(None);
        };
        let row = unit_row(&row)?;
        let images = sqlx::query(
            "SELECT plugin, store_key, before_value, before_rev_unit, before_rev_n, \
             after_value, after_rev_unit, after_rev_n \
             FROM main.store_writes WHERE unit = ?1 ORDER BY plugin, store_key",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| said("reading a unit's images", &error))?
        .iter()
        .map(image)
        .collect::<Result<Vec<_>, _>>()?;
        let notes: Vec<String> =
            sqlx::query_scalar("SELECT note FROM main.store_notes WHERE unit = ?1 ORDER BY seq")
                .bind(id)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| said("reading a unit's notes", &error))?;
        Ok(Some(UnitDetail { row, images, notes }))
    }

    /// Sets a committed unit's state. `false` when the store does not hold it.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked, or the update changed a different number
    /// of rows than the unit's existence said it had to.
    pub async fn set_state(&self, unit: Snowflake, state: UnitState) -> Result<bool, LocalError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("beginning a change of state", &error))?;
        schema::no_temporary_object(&mut transaction).await?;
        let held = mark(&mut transaction, unit, state).await?;
        transaction
            .commit()
            .await
            .map_err(|error| said("committing a change of state", &error))?;
        Ok(held)
    }

    /// Forgets a principal's units in a realm beyond the newest `keep`, with
    /// their images and notes. Returns how many units went.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked; when it refuses the delete — a unit beyond
    /// `keep` still held in `store_unit_holds` is one its `ON DELETE RESTRICT`
    /// refuses, and the whole delete with it, nothing forgotten; and when the
    /// delete changed a different number of rows than were counted beyond
    /// `keep`.
    pub async fn forget_beyond(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        keep: u32,
    ) -> Result<u64, LocalError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("beginning to forget units", &error))?;
        schema::no_temporary_object(&mut transaction).await?;
        let held: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM main.store_units WHERE realm = ?1 AND principal = ?2",
        )
        .bind(realm.as_str())
        .bind(principal.as_str())
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| said("counting units", &error))?;
        let beyond = as_unsigned(held, "a count of units")?.saturating_sub(u64::from(keep));
        write_exactly(
            sqlx::query(
                "DELETE FROM main.store_units WHERE realm = ?1 AND principal = ?2 AND unit NOT IN \
                 (SELECT unit FROM main.store_units WHERE realm = ?1 AND principal = ?2 \
                  ORDER BY unit DESC LIMIT ?3)",
            )
            .bind(realm.as_str())
            .bind(principal.as_str())
            .bind(i64::from(keep))
            .execute(&mut *transaction)
            .await,
            beyond,
            "forgetting units",
        )?;
        transaction
            .commit()
            .await
            .map_err(|error| said("committing forgotten units", &error))?;
        Ok(beyond)
    }

    /// Whether a committed unit other than `except` holds a note that is
    /// exactly `note`.
    ///
    /// **What an undo asks before it takes a blob's pin off**: every unit that
    /// made the same bytes holds a note naming them, so a blob another unit
    /// still holds keeps its pin. Every realm's units are asked, because a blob
    /// is this machine's and no one realm's, and every kind of unit is: an undo
    /// whose compensation made a blob holds it as a step does, and is never
    /// redone, so its effect stands. A unit undone holds nothing, and is passed
    /// over.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when SQLite cannot be asked.
    pub async fn noted_elsewhere(&self, note: &str, except: Snowflake) -> Result<bool, LocalError> {
        let except = as_signed(except.value(), "a unit")?;
        let held: i64 = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM main.store_notes AS noted \
             JOIN main.store_units AS holder ON holder.unit = noted.unit \
             WHERE noted.note = ?1 AND noted.unit <> ?2 \
             AND holder.state = 'committed')",
        )
        .bind(note)
        .bind(except)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| said("asking which steps hold a note", &error))?;
        Ok(held == 1)
    }

    /// One blob's row.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked, or holds a row this crate did not write.
    pub async fn blob(&self, hash: &str) -> Result<Option<BlobRow>, LocalError> {
        let row = sqlx::query(
            "SELECT hash, size, created_unit, pinned, unpinned_at FROM main.blobs WHERE hash = ?1",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| said("reading a blob", &error))?;
        row.map(|row| {
            let read = |error: sqlx::Error| said("reading a blob", &error);
            Ok(BlobRow {
                hash: row.try_get("hash").map_err(read)?,
                size: as_unsigned(row.try_get("size").map_err(read)?, "a blob's size")?,
                created_unit: snowflake(row.try_get("created_unit").map_err(read)?)?,
                pinned: row.try_get::<i64, _>("pinned").map_err(read)? == 1,
                unpinned_at_ms: row
                    .try_get::<Option<i64>, _>("unpinned_at")
                    .map_err(read)?
                    .map(|at| as_unsigned(at, "a time"))
                    .transpose()?,
            })
        })
        .transpose()
    }

    /// Pins a blob again, or unpins it as of `at_ms`. `false` when the store
    /// does not hold it.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked, or the update changed a different number
    /// of rows than the blob's existence said it had to.
    pub async fn set_pinned(
        &self,
        hash: &str,
        pinned: bool,
        at_ms: u64,
    ) -> Result<bool, LocalError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("beginning to pin a blob", &error))?;
        schema::no_temporary_object(&mut transaction).await?;
        let held: i64 = sqlx::query_scalar("SELECT count(*) FROM main.blobs WHERE hash = ?1")
            .bind(hash)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| said("reading a blob", &error))?;
        let at = if pinned {
            None
        } else {
            Some(as_signed(at_ms, "a time")?)
        };
        write_exactly(
            sqlx::query("UPDATE main.blobs SET pinned = ?2, unpinned_at = ?3 WHERE hash = ?1")
                .bind(hash)
                .bind(i64::from(pinned))
                .bind(at)
                .execute(&mut *transaction)
                .await,
            as_unsigned(held, "a count of blobs")?,
            "pinning a blob",
        )?;
        transaction
            .commit()
            .await
            .map_err(|error| said("committing a pin", &error))?;
        Ok(held == 1)
    }

    /// Up to `limit` blobs unpinned before `before_ms`, oldest first: what the
    /// sweep may delete.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked.
    pub async fn unpinned_before(
        &self,
        before_ms: u64,
        limit: u32,
    ) -> Result<Vec<String>, LocalError> {
        sqlx::query_scalar(
            "SELECT hash FROM main.blobs WHERE pinned = 0 AND unpinned_at < ?1 \
             ORDER BY unpinned_at, hash LIMIT ?2",
        )
        .bind(as_signed(before_ms, "a time")?)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| said("listing unpinned blobs", &error))
    }

    /// Forgets an unpinned blob. `false` when the store does not hold it or it
    /// is pinned — a pinned blob is a committed unit's, and forgetting it here
    /// would leave that unit pointing at nothing.
    ///
    /// # Errors
    ///
    /// When SQLite cannot be asked, or the delete changed a different number
    /// of rows than the blob's state said it had to.
    pub async fn forget_blob(&self, hash: &str) -> Result<bool, LocalError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| said("beginning to forget a blob", &error))?;
        schema::no_temporary_object(&mut transaction).await?;
        let pinned: Option<i64> =
            sqlx::query_scalar("SELECT pinned FROM main.blobs WHERE hash = ?1")
                .bind(hash)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| said("reading a blob", &error))?;
        let forgettable = pinned == Some(0);
        write_exactly(
            sqlx::query("DELETE FROM main.blobs WHERE hash = ?1 AND pinned = 0")
                .bind(hash)
                .execute(&mut *transaction)
                .await,
            u64::from(forgettable),
            "forgetting a blob",
        )?;
        transaction
            .commit()
            .await
            .map_err(|error| said("committing a forgotten blob", &error))?;
        Ok(forgettable)
    }
}

/// A running count of a commit's statements, handed to a hook before each.
struct Steps<F> {
    count: usize,
    between: F,
}

impl<F: FnMut(usize)> Steps<F> {
    /// Calls the hook with the count, then counts one more.
    fn next(&mut self) {
        (self.between)(self.count);
        self.count += 1;
    }
}

/// Holds `unit` from the pruner, and every unit it marks, as placed by
/// `holder`, its own row.
///
/// **Each a row of its own**, after the marks, so each unit held is one the
/// store holds: the reference refuses a hold of a unit it does not.
async fn held_from_the_pruner<F: FnMut(usize)>(
    connection: &mut sqlx::SqliteConnection,
    steps: &mut Steps<F>,
    unit: &UnitRecord,
    holder: i64,
) -> Result<(), CommitError> {
    let marked = unit.marks.iter().map(|(marked, _)| *marked);
    for held in core::iter::once(unit.unit).chain(marked) {
        steps.next();
        write_exactly(
            sqlx::query("INSERT INTO main.store_unit_holds (unit, holder) VALUES (?1, ?2)")
                .bind(as_signed(held.value(), "a unit")?)
                .bind(holder)
                .execute(&mut *connection)
                .await,
            1,
            "holding a unit from the pruner",
        )?;
    }
    Ok(())
}

/// The unit's own row, refused when the store already holds one.
///
/// **A unit commits once, and a second commit of it is the caller's
/// mistake.** The primary key refused it anyway, as `UNIQUE constraint
/// failed` — which `said` reads as a file this build does not expect,
/// `NC-DB-0007`, critical, telling the user their data is not what this
/// version expects for what is a kernel's or a recovery's bug. Asked inside
/// the transaction, so the answer is the one the insert meets.
async fn record_the_unit(
    connection: &mut sqlx::SqliteConnection,
    unit: &UnitRecord,
    shaped: &Shaped,
) -> Result<(), CommitError> {
    let held: i64 = sqlx::query_scalar("SELECT count(*) FROM main.store_units WHERE unit = ?1")
        .bind(shaped.unit)
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| said("reading whether the unit is held", &error))?;
    if held != 0 {
        return Err(CommitError::Invalid(format!(
            "unit {} is already committed, and a unit commits once",
            unit.unit.to_crockford()
        )));
    }
    Ok(write_exactly(
        sqlx::query(
            "INSERT INTO main.store_units \
             (unit, realm, principal, kind, target, label, state, undone_by, committed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'committed', NULL, ?7)",
        )
        .bind(shaped.unit)
        .bind(unit.meta.realm.as_str())
        .bind(unit.meta.principal.as_str())
        .bind(shaped.kind)
        .bind(shaped.target)
        .bind(&unit.meta.label)
        .bind(shaped.committed_at)
        .execute(&mut *connection)
        .await,
        1,
        "recording the unit",
    )?)
}

/// Refuses unless `plugin` holds no more keys in `realm` than its quota: what
/// a unit that made it a new key is held to before it commits.
async fn within_its_quota(
    connection: &mut sqlx::SqliteConnection,
    realm: &OpaqueId,
    plugin: &str,
    quotas: &Quotas,
) -> Result<(), CommitError> {
    let held: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM main.store_entries WHERE realm = ?1 AND plugin = ?2",
    )
    .bind(realm.as_str())
    .bind(plugin)
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| said("counting a plugin's keys", &error))?;
    if as_unsigned(held, "a count of keys")? > quotas.entries_for(plugin) {
        return Err(CommitError::Quota {
            plugin: plugin.to_owned(),
        });
    }
    Ok(())
}

/// One key of a unit, of its realm: checked, written, and its images
/// recorded. Returns whether it made a key the plugin did not hold there.
async fn write_one<F: FnMut(usize)>(
    connection: &mut sqlx::SqliteConnection,
    steps: &mut Steps<F>,
    realm: &OpaqueId,
    write: &StagedWrite,
    revision: Revision,
) -> Result<bool, CommitError> {
    steps.next();
    let before = sqlx::query(
        "SELECT value, rev_unit, rev_n FROM main.store_entries \
         WHERE realm = ?1 AND plugin = ?2 AND store_key = ?3",
    )
    .bind(realm.as_str())
    .bind(&write.plugin)
    .bind(&write.key)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|error| said("reading a key the unit writes", &error))?
    .map(|row| entry(&row, "value", "rev_unit", "rev_n"))
    .transpose()?;
    if !holds_what_was_expected(before.as_ref(), &write.expect) {
        return Err(CommitError::Conflict {
            plugin: write.plugin.clone(),
            key: write.key.clone(),
        });
    }
    let unit = as_signed(revision.unit.value(), "a unit")?;
    steps.next();
    if let Some(after) = &write.after {
        write_exactly(
            sqlx::query(
                "INSERT INTO main.store_entries \
                 (realm, plugin, store_key, value, rev_unit, rev_n) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT (realm, plugin, store_key) DO UPDATE SET \
                 value = excluded.value, rev_unit = excluded.rev_unit, rev_n = excluded.rev_n",
            )
            .bind(realm.as_str())
            .bind(&write.plugin)
            .bind(&write.key)
            .bind(after)
            .bind(unit)
            .bind(i64::from(revision.n))
            .execute(&mut *connection)
            .await,
            1,
            "writing a key",
        )?;
    } else {
        write_exactly(
            sqlx::query(
                "DELETE FROM main.store_entries \
                 WHERE realm = ?1 AND plugin = ?2 AND store_key = ?3",
            )
            .bind(realm.as_str())
            .bind(&write.plugin)
            .bind(&write.key)
            .execute(&mut *connection)
            .await,
            u64::from(before.is_some()),
            "deleting a key",
        )?;
    }
    steps.next();
    let (before_value, before_unit, before_n) = columns_of(before.as_ref())?;
    write_exactly(
        sqlx::query(
            "INSERT INTO main.store_writes \
             (unit, plugin, store_key, before_value, before_rev_unit, before_rev_n, \
              after_value, after_rev_unit, after_rev_n) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(unit)
        .bind(&write.plugin)
        .bind(&write.key)
        .bind(before_value)
        .bind(before_unit)
        .bind(before_n)
        .bind(write.after.as_deref())
        .bind(write.after.as_ref().map(|_| unit))
        .bind(write.after.as_ref().map(|_| i64::from(revision.n)))
        .execute(&mut *connection)
        .await,
        1,
        "recording a key's images",
    )?;
    Ok(before.is_none() && write.after.is_some())
}

/// One blob a unit created: pinned again when this machine holds it, recorded
/// when it does not and the bound allows.
async fn record_blob<F: FnMut(usize)>(
    connection: &mut sqlx::SqliteConnection,
    steps: &mut Steps<F>,
    blob: &BlobCreated,
    unit: i64,
    bound: u64,
) -> Result<(), CommitError> {
    steps.next();
    let held: Option<i64> = sqlx::query_scalar("SELECT pinned FROM main.blobs WHERE hash = ?1")
        .bind(&blob.hash)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|error| said("reading a blob", &error))?;
    steps.next();
    if held.is_some() {
        return Ok(write_exactly(
            sqlx::query("UPDATE main.blobs SET pinned = 1, unpinned_at = NULL WHERE hash = ?1")
                .bind(&blob.hash)
                .execute(&mut *connection)
                .await,
            1,
            "pinning a blob again",
        )?);
    }
    let blobs: i64 = sqlx::query_scalar("SELECT count(*) FROM main.blobs")
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| said("counting blobs", &error))?;
    if as_unsigned(blobs, "a count of blobs")? >= bound {
        return Err(CommitError::TooManyBlobs);
    }
    steps.next();
    Ok(write_exactly(
        sqlx::query(
            "INSERT INTO main.blobs (hash, size, created_unit, pinned, unpinned_at) \
             VALUES (?1, ?2, ?3, 1, NULL)",
        )
        .bind(&blob.hash)
        .bind(as_signed(blob.size, "a blob's size")?)
        .bind(unit)
        .execute(&mut *connection)
        .await,
        1,
        "recording a blob",
    )?)
}

/// Takes the pin off one blob inside a transaction, as of `at`. `false` when
/// the store does not hold it.
async fn unpin(
    connection: &mut sqlx::SqliteConnection,
    hash: &str,
    at: i64,
) -> Result<bool, LocalError> {
    let held: i64 = sqlx::query_scalar("SELECT count(*) FROM main.blobs WHERE hash = ?1")
        .bind(hash)
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| said("reading a blob", &error))?;
    write_exactly(
        sqlx::query("UPDATE main.blobs SET pinned = 0, unpinned_at = ?2 WHERE hash = ?1")
            .bind(hash)
            .bind(at)
            .execute(&mut *connection)
            .await,
        as_unsigned(held, "a count of blobs")?,
        "unpinning a blob",
    )?;
    Ok(held == 1)
}

/// The migrations this build carries, embedded when it was compiled.
///
/// **Embedded when this crate compiles, and cargo is not told what was read.**
/// On stable Rust `sqlx::migrate!` does not register the directory with the
/// compiler, so a migration added there is missing from this crate until one
/// of its sources changes — touch this file after adding one.
/// `the_embedded_migrations_are_the_ones_on_disk` holds the embedded list to
/// the directory, version and checksum, and fails naming both when they differ.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../local-migrations");

/// A migrator's failure, as the failure it is here.
///
/// **A history this build did not write is not a migration that failed.** A
/// version the file records and this build does not carry is a newer build's
/// database; one whose checksum differs is a migration edited after it ran.
/// Both are a database this build was not tested against, so they are
/// `NC-DB-0007`; a migration that ran and failed is `NC-DB-0002`. Both are
/// refused before the migrator runs (`schema::before_migrating`), on a
/// connection that writes nothing, and this is the second net.
fn migrated(error: &sqlx::migrate::MigrateError) -> LocalError {
    use sqlx::migrate::MigrateError;
    match error {
        MigrateError::VersionMissing(_)
        | MigrateError::VersionMismatch(_)
        | MigrateError::VersionTooOld(..)
        | MigrateError::VersionTooNew(..) => LocalError::Unexpected(format!(
            "the file's migrations are not the ones this build carries: {error}"
        )),
        MigrateError::Execute(inner) => said("reading the migrations' history", inner),
        other => LocalError::Migration(format!("applying the local migrations: {other}")),
    }
}

/// Whose the file is, and the self-check that only reads: everything a file
/// is held to before the migrator may write it.
///
/// **Everything that reads the file comes before the migrator, which writes
/// it.** Run right after the header, the migrator would apply a missing
/// migration to a file one migration behind before `quick_check` or the list
/// refused it — measured, both ways — and the message the person reads would
/// say nothing was changed.
async fn judged_before_migrating(
    connection: &mut sqlx::SqliteConnection,
) -> Result<(), LocalError> {
    belongs_here(connection).await?;
    let carried: Vec<(i64, Vec<u8>)> = MIGRATOR
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .map(|migration| (migration.version, migration.checksum.to_vec()))
        .collect();
    schema::before_migrating(connection, schema::MINIMUM_SQLITE, &carried).await
}

/// Why the connection that judges the file could not do what it was `doing`.
///
/// **A hot rollback journal is a file this build did not leave**: its
/// database keeps a write-ahead log, and a read-only connection refuses to
/// read past a journal only a writer may roll back — at its first statement,
/// which `sqlx` runs as it connects. So it is `NC-DB-0007`, the file left as
/// it stood, and not the store being unreachable.
fn unread(doing: &str, error: &sqlx::Error) -> LocalError {
    let hot = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == crate::open::HOT_JOURNAL);
    if hot {
        return LocalError::Unexpected(
            "the file has a rollback journal beside it of a write that did not finish, which \
             this build never leaves, so it is not this build's database. It was left as it is"
                .to_owned(),
        );
    }
    said(doing, error)
}

/// Whether the file belongs to this platform, before anything is migrated.
///
/// Its header carries [`schema::APPLICATION_ID`], or it carries zero and holds
/// no table but, at most, the migrator's own — a file this crate created and
/// has not finished building, which the first migration completes.
///
/// **Anything else is `NC-DB-0007`, not `NC-DB-0006`.** Another application's
/// header is a file that is not the one this build expects, which is what that
/// code says; `NC-DB-0006` tells the user the folder or the files are not set
/// up for them alone to read and write, and nothing about a foreign header says
/// so. `AGENTS.md` names `NC-DB-0007` for such a file.
async fn belongs_here(connection: &mut sqlx::SqliteConnection) -> Result<(), LocalError> {
    let application: i32 = sqlx::query_scalar("PRAGMA main.application_id")
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| unread("reading the file's header", &error))?;
    if application == schema::APPLICATION_ID {
        return Ok(());
    }
    let objects: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM main.sqlite_schema WHERE tbl_name <> '_sqlx_migrations'",
    )
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| said("reading the schema", &error))?;
    if application == 0 && objects == 0 {
        return Ok(());
    }
    Err(LocalError::Unexpected(format!(
        "the file's header carries application {application} and it holds {objects} \
         object(s), so it is not this platform's local database. It was left as it is"
    )))
}

/// Refuses a record past what one unit may record, before a statement runs.
///
/// **What bounds `store_writes` and `store_notes`.** Their rows live and die
/// with their unit, so `store_units`' bound is theirs only times what one unit
/// may make — and without this the store would hold a unit to nothing: one
/// with 20,000 images and 50,000 notes would commit. `catalog/limits.toml`
/// counts both tables per unit, and this is the writer refusing at that count.
/// An `Invalid` and not a quota, because the kernel holds its overlay to the
/// same number (`units::writes_per_unit` in `crates/nc-kernel`), so a unit past
/// it is the kernel's mistake rather than a plugin's.
fn within_what_one_unit_may_record(unit: &UnitRecord, quotas: &Quotas) -> Result<(), CommitError> {
    for (what, table, held, bound) in [
        (
            "images",
            "store_writes",
            unit.writes.len(),
            quotas.writes_per_unit,
        ),
        (
            "notes",
            "store_notes",
            unit.notes.len(),
            quotas.notes_per_unit,
        ),
    ] {
        if u64::try_from(held).unwrap_or(u64::MAX) > bound {
            return Err(CommitError::Invalid(format!(
                "unit {} records {held} {what}, and one unit records at most {bound}: \
                 `catalog/limits.toml` counts `{table}` per unit",
                unit.unit.to_crockford()
            )));
        }
    }
    Ok(())
}

/// The record's shape, checked before anything is written.
struct Shaped {
    unit: i64,
    kind: &'static str,
    target: Option<i64>,
    committed_at: i64,
}

impl Shaped {
    /// Refuses a record no unit can be, naming what is wrong with it.
    fn of(unit: &UnitRecord) -> Result<Self, CommitError> {
        let mut seen: Vec<(&str, &str)> = Vec::with_capacity(unit.writes.len());
        for write in &unit.writes {
            let key = (write.plugin.as_str(), write.key.as_str());
            if seen.contains(&key) {
                return Err(CommitError::Invalid(format!(
                    "`{}` of `{}` is written twice in one unit, and a unit holds one image per key",
                    write.key, write.plugin
                )));
            }
            seen.push(key);
            for value in write.after.iter().chain(match &write.expect {
                Expect::Value(Some(value)) => Some(value),
                _ => None,
            }) {
                canonical(value, "a value")?;
            }
        }
        for note in &unit.notes {
            canonical(note, "a note")?;
        }
        let mut hashes: Vec<&str> = Vec::with_capacity(unit.blobs.len());
        for blob in &unit.blobs {
            hash_shaped(&blob.hash)?;
            if hashes.contains(&blob.hash.as_str()) {
                return Err(CommitError::Invalid(format!(
                    "blob `{}` is recorded twice in one unit",
                    blob.hash
                )));
            }
            hashes.push(&blob.hash);
        }
        let mut unpinned: Vec<&str> = Vec::with_capacity(unit.unpinned.len());
        for hash in &unit.unpinned {
            hash_shaped(hash)?;
            if hashes.contains(&hash.as_str()) || unpinned.contains(&hash.as_str()) {
                return Err(CommitError::Invalid(format!(
                    "blob `{hash}` is unpinned twice, or pinned and unpinned, by one unit"
                )));
            }
            unpinned.push(hash);
        }
        let (kind, target) = match unit.meta.kind {
            UnitKind::Step => ("step", None),
            UnitKind::Undo { target } => ("undo", Some(target)),
            UnitKind::Redo { target } => ("redo", Some(target)),
        };
        Ok(Self {
            unit: as_signed(unit.unit.value(), "a unit")?,
            kind,
            target: target
                .map(|target| as_signed(target.value(), "a unit"))
                .transpose()?,
            committed_at: as_signed(unit.meta.committed_at_ms, "a time")?,
        })
    }
}

/// Refuses a blob hash that is not one: sixty-four lowercase hex characters.
fn hash_shaped(hash: &str) -> Result<(), CommitError> {
    let shaped = hash.len() == 64
        && hash
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if shaped {
        Ok(())
    } else {
        Err(CommitError::Invalid(format!(
            "`{hash}` is not a BLAKE3 hash: sixty-four lowercase hex characters"
        )))
    }
}

/// Refuses text that is not nc-canon's canonical form of itself.
fn canonical(text: &str, what: &str) -> Result<(), CommitError> {
    match nc_canon::parse(text) {
        Ok(value) if value.to_canonical() == text => Ok(()),
        Ok(value) => Err(CommitError::Invalid(format!(
            "{what} is JSON but not its canonical form, which is `{}`",
            value.to_canonical()
        ))),
        Err(error) => Err(CommitError::Invalid(format!(
            "{what} is not JSON nc-canon reads: {error}"
        ))),
    }
}

/// Whether a key holds what a write expects of it.
fn holds_what_was_expected(held: Option<&Entry>, expect: &Expect) -> bool {
    match expect {
        Expect::Anything => true,
        Expect::Absent => held.is_none(),
        Expect::Revision(revision) => held.is_some_and(|entry| entry.revision == *revision),
        Expect::Value(value) => held.map(|entry| &entry.value) == value.as_ref(),
    }
}

/// A write, and the number of rows it had to change.
///
/// **The number is checked, because success is not proof.** A temporary
/// trigger — `BEFORE INSERT … RAISE(IGNORE)` — makes an `INSERT` succeed having
/// written nothing, and the transaction-start check is only as current as the
/// statement that made it. The same trigger in `main`, added by another
/// connection after this store opened, is seen by neither that check nor the
/// open-time one; this is what stops it.
pub(crate) fn write_exactly(
    result: Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error>,
    expected: u64,
    doing: &str,
) -> Result<(), LocalError> {
    let changed = result.map_err(|error| said(doing, &error))?.rows_affected();
    if changed == expected {
        Ok(())
    } else {
        Err(LocalError::Unexpected(format!(
            "{doing} changed {changed} row(s) where it had to change {expected}, so \
             something between this crate and the table decided otherwise"
        )))
    }
}

/// Sets one unit's state inside a transaction. `false` when it is not there.
async fn mark(
    connection: &mut sqlx::SqliteConnection,
    unit: Snowflake,
    state: UnitState,
) -> Result<bool, LocalError> {
    let id = as_signed(unit.value(), "a unit")?;
    let held: i64 = sqlx::query_scalar("SELECT count(*) FROM main.store_units WHERE unit = ?1")
        .bind(id)
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| said("reading a unit", &error))?;
    let (state, by) = match state {
        UnitState::Committed => ("committed", None),
        UnitState::Undone { by } => ("undone", Some(as_signed(by.value(), "a unit")?)),
    };
    write_exactly(
        sqlx::query("UPDATE main.store_units SET state = ?2, undone_by = ?3 WHERE unit = ?1")
            .bind(id)
            .bind(state)
            .bind(by)
            .execute(&mut *connection)
            .await,
        as_unsigned(held, "a count of units")?,
        "changing a unit's state",
    )?;
    Ok(held == 1)
}

/// A value, the unit that wrote it and its position, as three columns.
type Columns<'a> = (Option<&'a str>, Option<i64>, Option<i64>);

/// The three image columns a key's committed entry becomes.
fn columns_of(entry: Option<&Entry>) -> Result<Columns<'_>, LocalError> {
    Ok(match entry {
        Some(entry) => (
            Some(entry.value.as_str()),
            Some(as_signed(entry.revision.unit.value(), "a unit")?),
            Some(i64::from(entry.revision.n)),
        ),
        None => (None, None, None),
    })
}

/// A committed entry out of three columns of a row.
fn entry(row: &SqliteRow, value: &str, unit: &str, n: &str) -> Result<Entry, LocalError> {
    let read = |error: sqlx::Error| said("reading an entry", &error);
    Ok(Entry {
        value: row.try_get(value).map_err(read)?,
        revision: Revision {
            unit: snowflake(row.try_get(unit).map_err(read)?)?,
            n: u32::try_from(row.try_get::<i64, _>(n).map_err(read)?).map_err(|_| {
                LocalError::Unexpected("a revision's position is not one a unit writes".to_owned())
            })?,
        },
    })
}

/// An entry out of three nullable columns, or `None` when the first is NULL.
fn maybe_entry(
    row: &SqliteRow,
    value: &str,
    unit: &str,
    n: &str,
) -> Result<Option<Entry>, LocalError> {
    let held: Option<String> = row
        .try_get(value)
        .map_err(|error| said("reading an image", &error))?;
    held.map(|_| entry(row, value, unit, n)).transpose()
}

/// One image row.
fn image(row: &SqliteRow) -> Result<Image, LocalError> {
    let read = |error: sqlx::Error| said("reading an image", &error);
    Ok(Image {
        plugin: row.try_get("plugin").map_err(read)?,
        key: row.try_get("store_key").map_err(read)?,
        before: maybe_entry(row, "before_value", "before_rev_unit", "before_rev_n")?,
        after: maybe_entry(row, "after_value", "after_rev_unit", "after_rev_n")?,
    })
}

/// One unit row.
fn unit_row(row: &SqliteRow) -> Result<UnitRow, LocalError> {
    let read = |error: sqlx::Error| said("reading a unit", &error);
    let target: Option<i64> = row.try_get("target").map_err(read)?;
    let kind: String = row.try_get("kind").map_err(read)?;
    let kind = match (kind.as_str(), target) {
        ("step", None) => UnitKind::Step,
        ("undo", Some(target)) => UnitKind::Undo {
            target: snowflake(target)?,
        },
        ("redo", Some(target)) => UnitKind::Redo {
            target: snowflake(target)?,
        },
        _ => {
            return Err(LocalError::Unexpected(format!(
                "a unit of kind `{kind}` is not one this crate writes"
            )));
        }
    };
    let undone_by: Option<i64> = row.try_get("undone_by").map_err(read)?;
    let state: String = row.try_get("state").map_err(read)?;
    let state = match (state.as_str(), undone_by) {
        ("committed", None) => UnitState::Committed,
        ("undone", Some(by)) => UnitState::Undone { by: snowflake(by)? },
        _ => {
            return Err(LocalError::Unexpected(format!(
                "a unit in state `{state}` is not one this crate writes"
            )));
        }
    };
    let identifier = |column: &str| -> Result<OpaqueId, LocalError> {
        let text: String = row.try_get(column).map_err(read)?;
        OpaqueId::new(text).map_err(|error| {
            LocalError::Unexpected(format!("a unit's {column} is not an identifier: {error}"))
        })
    };
    Ok(UnitRow {
        unit: snowflake(row.try_get("unit").map_err(read)?)?,
        meta: UnitMeta {
            realm: identifier("realm")?,
            principal: identifier("principal")?,
            kind,
            label: row.try_get("label").map_err(read)?,
            committed_at_ms: as_unsigned(row.try_get("committed_at").map_err(read)?, "a time")?,
        },
        state,
    })
}

/// A unit read back from its column.
///
/// **Through `from_crockford`, which is allowed here and nowhere the log is.**
/// `Snowflake` has no constructor from a number, deliberately; the value is one
/// this store wrote from a unit the kernel minted, and a value that does not
/// fit the layout is refused rather than truncated.
fn snowflake(value: i64) -> Result<Snowflake, LocalError> {
    let value = u64::try_from(value)
        .map_err(|_| LocalError::Unexpected(format!("{value} is not a unit")))?;
    Snowflake::from_crockford(&nc_ids::base32::encode(value))
        .map_err(|error| LocalError::Unexpected(format!("{value} is not a unit: {error}")))
}

/// An unsigned number as SQLite's signed integer, or why it does not fit.
fn as_signed(value: u64, what: &str) -> Result<i64, LocalError> {
    i64::try_from(value).map_err(|_| {
        LocalError::Unexpected(format!("{what} of {value} does not fit a SQLite integer"))
    })
}

/// A SQLite integer as an unsigned number, or why it cannot be one.
fn as_unsigned(value: i64, what: &str) -> Result<u64, LocalError> {
    u64::try_from(value)
        .map_err(|_| LocalError::Unexpected(format!("{what} of {value} is below zero")))
}

/// The first text that sorts after every text beginning with `prefix`, or
/// `None` when nothing does — the empty prefix, or one made of the last
/// character there is.
///
/// SQLite's default collation compares UTF-8 bytes, and UTF-8's byte order is
/// the order of the characters it encodes, so the answer is the prefix with
/// its last character raised by one — skipping the surrogates, which no text
/// holds — after dropping any trailing characters that cannot be raised.
fn past_every_key_beginning_with(prefix: &str) -> Option<String> {
    let mut characters: Vec<char> = prefix.chars().collect();
    while let Some(last) = characters.pop() {
        let raised = match u32::from(last) {
            0xD7FF => Some('\u{E000}'),
            code => char::from_u32(code + 1),
        };
        if let Some(raised) = raised {
            characters.push(raised);
            return Some(characters.into_iter().collect());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Entry, Expect, Revision, holds_what_was_expected, past_every_key_beginning_with};

    /// A commit's refusal is an error, and the database's failure inside one
    /// is its source.
    #[test]
    fn a_commit_refused_is_an_error_whose_source_is_the_database_failure_in_it() {
        use std::error::Error as _;

        let failed = super::CommitError::Store(crate::LocalError::Unavailable(
            "the file is gone".to_owned(),
        ));
        let boxed: Box<dyn std::error::Error> = Box::new(failed.clone());
        assert!(boxed.to_string().contains("the file is gone"), "{boxed}");
        assert_eq!(
            failed.source().map(ToString::to_string),
            Some(crate::LocalError::Unavailable("the file is gone".to_owned()).to_string())
        );
        let quota = super::CommitError::Quota {
            plugin: "noctua:reference".to_owned(),
        };
        assert!(quota.source().is_none());
        assert!(quota.to_string().contains("`noctua:reference`"), "{quota}");
    }

    /// The bound after a prefix is past every key with it and before the rest.
    #[test]
    fn the_bound_after_a_prefix_is_the_prefix_raised_by_one_character() {
        assert_eq!(past_every_key_beginning_with(""), None);
        assert_eq!(past_every_key_beginning_with("a").as_deref(), Some("b"));
        assert_eq!(
            past_every_key_beginning_with("note/").as_deref(),
            Some("note0")
        );
        assert_eq!(
            past_every_key_beginning_with("a\u{D7FF}").as_deref(),
            Some("a\u{E000}"),
            "the surrogates are skipped"
        );
        assert_eq!(
            past_every_key_beginning_with("a\u{10FFFF}").as_deref(),
            Some("b"),
            "a character that cannot be raised is dropped and the one before raised"
        );
        assert_eq!(past_every_key_beginning_with("\u{10FFFF}"), None);
        for key in ["a", "a/", "a\u{10FFFF}", "a\u{10FFFF}\u{10FFFF}"] {
            let upper = past_every_key_beginning_with("a").expect("a bound");
            assert!(
                key < upper.as_str(),
                "{key:?} begins with `a` and sorts past the bound"
            );
        }
        assert!(
            "b" >= past_every_key_beginning_with("a")
                .expect("a bound")
                .as_str()
        );
    }

    /// Each expectation holds for exactly what it names.
    #[test]
    fn an_expectation_holds_only_for_what_it_names() {
        let revision = |n| Revision {
            unit: nc_ids::Snowflake::from_crockford("1").expect("a unit"),
            n,
        };
        let held = Entry {
            value: "1".to_owned(),
            revision: revision(0),
        };
        assert!(holds_what_was_expected(None, &Expect::Anything));
        assert!(holds_what_was_expected(Some(&held), &Expect::Anything));
        assert!(holds_what_was_expected(None, &Expect::Absent));
        assert!(!holds_what_was_expected(Some(&held), &Expect::Absent));
        assert!(holds_what_was_expected(
            Some(&held),
            &Expect::Revision(revision(0))
        ));
        assert!(!holds_what_was_expected(
            Some(&held),
            &Expect::Revision(revision(1))
        ));
        assert!(!holds_what_was_expected(
            None,
            &Expect::Revision(revision(0))
        ));
        assert!(holds_what_was_expected(
            Some(&held),
            &Expect::Value(Some("1".to_owned()))
        ));
        assert!(!holds_what_was_expected(
            Some(&held),
            &Expect::Value(Some("2".to_owned()))
        ));
        assert!(!holds_what_was_expected(Some(&held), &Expect::Value(None)));
        assert!(holds_what_was_expected(None, &Expect::Value(None)));
        assert!(!holds_what_was_expected(
            None,
            &Expect::Value(Some("1".to_owned()))
        ));
    }

    /// A revision is written as its unit in Crockford base32 and its position.
    #[test]
    fn a_revision_is_its_unit_and_its_position() {
        let unit = nc_ids::Snowflake::from_crockford("7ZZ").expect("a unit");
        assert_eq!(Revision { unit, n: 3 }.to_string(), "0000000007ZZ.3");
    }

    /// A write that changed a number of rows it could not have is refused.
    ///
    /// **The rule, driven with the answers SQLite gives**; every place it is
    /// applied is driven end to end by
    /// `a_write_a_trigger_silences_is_refused_and_leaves_nothing`, through a
    /// trigger planted in `main` after the store opened — which the open-time
    /// self-check cannot see, and which a connection of anybody's who can write
    /// the file can add. Such a count can be produced from outside: not by a
    /// temporary trigger, which only its own connection holds, but by one
    /// in `main`.
    #[test]
    fn a_write_that_changed_the_wrong_number_of_rows_is_refused() {
        let nothing = || Ok(sqlx::sqlite::SqliteQueryResult::default());
        let refused = super::write_exactly(nothing(), 1, "writing a key")
            .expect_err("an insert that wrote nothing is refused");
        assert!(
            matches!(&refused, crate::LocalError::Unexpected(why)
                if why.contains("writing a key changed 0 row(s) where it had to change 1")),
            "{refused:?}"
        );
        assert_eq!(
            super::write_exactly(nothing(), 0, "deleting an absent key"),
            Ok(())
        );
        let failed = super::write_exactly(Err(sqlx::Error::PoolTimedOut), 1, "writing a key")
            .expect_err("a statement that failed is refused");
        assert!(
            matches!(failed, crate::LocalError::Unavailable(_)),
            "{failed:?}"
        );
    }
}
