//! Opening the local database: what it creates, what it refuses, and the
//! self-check that refuses a file that is not the one this build expects.
//!
//! Every test works on a file in a scratch directory of its own, removed
//! however the test leaves. What a foreign owner does cannot be staged without
//! the superuser, so that clause is driven in `nc-platform`'s own tests with
//! the owner handed to it; everything else here is the filesystem and SQLite.
//!
//! **One body for every system.** A path another user can reach, one its
//! owner cannot write, and a link are staged through `nc_platform::testing`,
//! which answers the words the refusal names each by on this system — a mode
//! on Unix, an access list on Windows — so each test here is the Unix test and
//! its Windows twin at once.

use std::path::{Path, PathBuf};

use nc_catalog::ErrorCode;
use nc_local::{LocalError, LocalStore, schema};
use nc_platform::testing;
use sqlx::{ConnectOptions as _, Connection as _};

/// A scratch directory for one test, removed however the test leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nc-local-opening-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("a scratch directory");
        Self(root)
    }

    /// Where the store's directory goes: one level down, so the store is the
    /// one to create it.
    fn profile(&self) -> PathBuf {
        self.0.join("profile")
    }
}

/// What a path lets in, as two readings compare.
fn access(path: &Path) -> testing::Access {
    testing::access(path).expect("the path exists")
}

/// The path made private to this user, as the store makes one.
fn private(path: &Path) {
    testing::make_private(path).expect("made private");
}

/// A connection to the file the way somebody with write access to it has one:
/// none of the store's pragmas, none of its checks.
async fn raw(profile: &Path) -> sqlx::SqliteConnection {
    sqlx::sqlite::SqliteConnectOptions::new()
        .filename(profile.join(nc_local::FILE_NAME))
        .create_if_missing(false)
        .connect()
        .await
        .expect("the file opens")
}

/// Opens, expecting a refusal, and hands back the error.
async fn refused(profile: &Path) -> LocalError {
    LocalStore::open(profile)
        .await
        .expect_err("the store must not open this")
}

/// A missing directory and file are created, and each is private.
///
/// **The companions too**: on Unix SQLite gives `-wal` and `-shm` the database
/// file's mode rather than the umask's, so a file made `0600` before SQLite saw
/// it is what keeps all three away from another user; on Windows they inherit
/// the one entry of the directory's access list, which names this user alone.
#[tokio::test]
async fn a_missing_directory_and_file_are_created_private() {
    let scratch = Scratch::new("created");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    assert!(testing::is_as_created(&profile), "the directory");
    let file = profile.join(nc_local::FILE_NAME);
    assert!(testing::is_as_created(&file), "the database file");
    for suffix in ["-wal", "-shm"] {
        let companion = PathBuf::from(format!("{}{suffix}", file.display()));
        assert!(testing::is_private(&companion), "{suffix}");
    }
    let names: Vec<String> = std::fs::read_dir(&profile)
        .expect("readable")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert!(
        names
            .iter()
            .all(|name| name.starts_with(nc_local::FILE_NAME)),
        "the owner probe was left behind: {names:?}"
    );
    let lock = profile.join(nc_local::LOCK_NAME);
    assert_eq!(
        names.iter().any(|name| name == nc_local::LOCK_NAME),
        testing::HOLDS_THROUGH_A_FILE,
        "a lock file is left where a file holds the profile, and only there: {names:?}"
    );
    assert!(
        !testing::HOLDS_THROUGH_A_FILE || testing::is_as_created(&lock),
        "the lock file"
    );
    store.close().await;
}

/// The pragmas are the ones the crate's header names, on every connection.
#[tokio::test]
async fn every_connection_carries_the_pragmas() {
    let scratch = Scratch::new("pragmas");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let mut connection = store.pool().acquire().await.expect("a connection");
    for (pragma, expected) in [
        ("PRAGMA synchronous", 2_i64),
        ("PRAGMA foreign_keys", 1),
        ("PRAGMA trusted_schema", 0),
        ("PRAGMA cell_size_check", 1),
        ("PRAGMA temp_store", 2),
        ("PRAGMA application_id", i64::from(schema::APPLICATION_ID)),
    ] {
        let read: i64 = sqlx::query_scalar(pragma)
            .fetch_one(&mut *connection)
            .await
            .expect("readable");
        assert_eq!(read, expected, "{pragma}");
    }
    let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&mut *connection)
        .await
        .expect("readable");
    assert_eq!(journal, "wal");
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&mut *connection)
        .await
        .expect("readable");
    assert!(
        schema::at_least(&version, schema::MINIMUM_SQLITE),
        "this build links SQLite {version}"
    );
    drop(connection);
    store.close().await;
}

/// A database this crate built holds exactly what the list says.
#[tokio::test]
async fn a_fresh_database_holds_exactly_what_the_list_says() {
    let scratch = Scratch::new("fresh");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let mut connection = store.pool().acquire().await.expect("a connection");
    let found = schema::found_in(&mut connection).await.expect("readable");
    assert_eq!(schema::judge(&found), Ok(()));
    assert_eq!(
        found.len(),
        schema::EXPECTED.len(),
        "the list and the database differ in size: {found:?}"
    );
    drop(connection);
    store.close().await;
    // And a second open of the same file finds the same, having migrated
    // nothing twice.
    let again = LocalStore::open(&scratch.profile())
        .await
        .expect("opens again");
    again.close().await;
}

/// A directory another user can reach is refused, and nothing is made in it.
#[tokio::test]
async fn a_directory_another_user_can_reach_is_refused() {
    let scratch = Scratch::new("directory-mode");
    let profile = scratch.profile();
    std::fs::create_dir(&profile).expect("a directory");
    private(&profile);
    let said = testing::open_to_another_user(&profile).expect("opened up");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalOpenRefused, "{error}");
    assert!(error.to_string().contains(&said), "{said}: {error}");
    assert_eq!(
        std::fs::read_dir(&profile).expect("readable").count(),
        0,
        "something was created in a directory that was refused"
    );
}

/// A database file another user can reach is refused, and left as it is.
#[tokio::test]
async fn a_file_another_user_can_reach_is_refused() {
    let scratch = Scratch::new("file-mode");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    let file = profile.join(nc_local::FILE_NAME);
    let said = testing::open_to_another_user(&file).expect("opened up");
    let before = access(&file);
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalOpenRefused, "{error}");
    assert!(error.to_string().contains(&said), "{said}: {error}");
    assert_eq!(
        access(&file),
        before,
        "a refused file was repaired rather than left"
    );
}

/// A link where the database file should be is refused, wherever it points.
#[tokio::test]
async fn a_linked_file_is_refused() {
    let scratch = Scratch::new("file-link");
    let profile = scratch.profile();
    let elsewhere = scratch.0.join("elsewhere");
    LocalStore::open(&elsewhere)
        .await
        .expect("opens")
        .close()
        .await;
    std::fs::create_dir(&profile).expect("a directory");
    private(&profile);
    testing::link_file(
        &elsewhere.join(nc_local::FILE_NAME),
        &profile.join(nc_local::FILE_NAME),
    )
    .expect("a link");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalOpenRefused, "{error}");
    assert!(error.to_string().contains("symbolic link"), "{error}");
}

/// A link where the directory should be is refused, wherever it points.
///
/// **Made by both doors**: on Windows a junction and a directory's symbolic
/// link are reparse points of two tags, and on Unix the two are one link.
#[tokio::test]
async fn a_linked_directory_is_refused() {
    /// A door that makes a link at its second path to the directory at its first.
    type Door = fn(&Path, &Path) -> std::io::Result<()>;

    let scratch = Scratch::new("directory-link");
    let real = scratch.0.join("real");
    LocalStore::open(&real).await.expect("opens").close().await;
    let doors: [(&str, Door); 2] = [
        ("linked", testing::link_directory),
        ("symlinked", testing::symlink_directory),
    ];
    for (name, link) in doors {
        let profile = scratch.0.join(name);
        link(&real, &profile).expect("a link");
        let error = refused(&profile).await;
        assert_eq!(
            error.code(),
            ErrorCode::DbLocalOpenRefused,
            "{name}: {error}"
        );
        assert!(
            error.to_string().contains("symbolic link"),
            "{name}: {error}"
        );
    }
}

/// A companion that is a link is refused before SQLite follows it.
#[tokio::test]
async fn a_linked_companion_is_refused() {
    let scratch = Scratch::new("companion-link");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    let wal = profile.join(format!("{}-wal", nc_local::FILE_NAME));
    let _ = std::fs::remove_file(&wal);
    testing::link_file(&scratch.0.join("somewhere"), &wal).expect("a link");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalOpenRefused, "{error}");
    assert!(error.to_string().contains("-wal"), "{error}");
}

/// A directory the path names that is not one is refused.
#[tokio::test]
async fn a_file_where_the_directory_should_be_is_refused() {
    let scratch = Scratch::new("not-a-directory");
    let profile = scratch.profile();
    std::fs::write(&profile, b"").expect("a file");
    private(&profile);
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalOpenRefused, "{error}");
    assert!(error.to_string().contains("not a directory"), "{error}");
}

/// A directory whose parent does not exist is the data not being reachable, and
/// not a claim that somebody else can reach it.
///
/// **Not `NC-DB-0006`**, critical and not retryable, for a path that holds
/// nothing at all: an I/O failure before SQLite — a full disk and a read-only
/// mount included — is what this crate's own `Unavailable` names.
#[tokio::test]
async fn a_missing_parent_is_unavailable_and_not_refused() {
    let scratch = Scratch::new("missing-parent");
    let parent = scratch.0.join("no-such-parent");
    let error = refused(&parent.join("profile")).await;
    assert_eq!(error.code(), ErrorCode::DbUnreachable, "{error}");
    assert!(matches!(error, LocalError::Unavailable(_)), "{error:?}");
    assert!(
        error.to_string().contains("could not be created"),
        "{error}"
    );
    assert!(
        !parent.exists(),
        "a missing parent was created: the store makes one level, its own"
    );
}

/// A directory this process may not write in is refused, and nothing is made
/// in it.
///
/// **Refused by its mode, before anything is tried in it.** Its mode has no
/// group or other bits, so judged by those alone only the owner probe would
/// meet it. The owner's own bits are judged beside the others', and the
/// refusal is `NC-DB-0006`, whose sentence says what is true of every refusal
/// under that code: the folder or the files are not set up for this user alone
/// to read and write.
#[tokio::test]
async fn a_directory_this_process_cannot_write_in_is_refused() {
    let scratch = Scratch::new("unwritable");
    let profile = scratch.profile();
    std::fs::create_dir(&profile).expect("a directory");
    private(&profile);
    let said = testing::shut_its_owner_out(&profile).expect("shut out");
    let error = LocalStore::open(&profile).await;
    // Writable again before anything is asserted, so the scratch directory can
    // be removed however this ends.
    private(&profile);
    let error = error.expect_err(
        "a directory its owner may not write in was written in: this test must run as a user \
         its access binds, not the superuser",
    );
    assert_eq!(error.code(), ErrorCode::DbLocalOpenRefused, "{error}");
    assert!(error.to_string().contains(&said), "{said}: {error}");
    assert_eq!(
        std::fs::read_dir(&profile).expect("readable").count(),
        0,
        "something was created in a directory that was refused"
    );
}

/// A profile under a parent this process may not write in is refused, and
/// nothing is created there.
///
/// **The one place the filesystem itself says no.** `failed` reads a
/// `Permission denied` as a refusal, and no other test reaches that arm: the
/// `0500` profile above is refused by its mode before anything is tried in it,
/// so with `PermissionDenied` dropped from the arm every other test of
/// `cargo test -p nc-local` passes, measured. Here the directory the store
/// makes, one level down, is what it may not create — the denial is the
/// kernel's, not this crate's reading of a mode.
#[tokio::test]
async fn a_profile_under_a_parent_this_process_cannot_write_in_is_refused() {
    let scratch = Scratch::new("unwritable-parent");
    let parent = scratch.0.join("parent");
    std::fs::create_dir(&parent).expect("a directory");
    testing::shut_its_owner_out(&parent).expect("shut out");
    let error = LocalStore::open(&parent.join("profile")).await;
    // Writable again before anything is asserted, so the scratch directory can
    // be removed however this ends.
    private(&parent);
    let error = error.expect_err(
        "a profile was made under a directory its owner may not write in: this test must run \
         as a user its access binds, not the superuser",
    );
    assert_eq!(error.code(), ErrorCode::DbLocalOpenRefused, "{error}");
    assert!(matches!(error, LocalError::Refused(_)), "{error:?}");
    assert!(
        error.to_string().contains("could not be created"),
        "{error}"
    );
    assert!(
        !parent.join("profile").exists(),
        "the profile was created under a parent the store was refused"
    );
}

/// A database file its owner may only read is refused at open, and left as
/// it was.
///
/// **Refused at open, because the self-check only reads**: opened, a `0400`
/// file would pass every check, and the next commit come back `NC-DB-0007` —
/// *not what this version expects* — for a file whose only fault is its mode,
/// while this crate's own refusal text says it opens only files that
/// are `0600`.
#[tokio::test]
async fn a_file_its_owner_cannot_write_is_refused_and_left_as_it_was() {
    let scratch = Scratch::new("read-only-file");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    let file = profile.join(nc_local::FILE_NAME);
    let said = testing::shut_its_owner_out(&file).expect("shut out");
    let before = access(&file);
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalOpenRefused, "{error}");
    assert!(error.to_string().contains(&said), "{said}: {error}");
    assert_eq!(access(&file), before, "a refused file was repaired");
    private(&file);
}

/// Opens of one profile awaited together on one runtime each open, however
/// few threads drive it: two on a current-thread runtime, three on one of two
/// workers.
///
/// **The wait is the blocking pool's**: an open that waited for the profile's
/// lock on the thread that polls it, and held the lock across every `.await`
/// of its judging, would make the second open block the very thread the first
/// needs to go on and let go — both runs answering nothing for thirty seconds.
/// Each runtime runs on a thread of its own, so a deadlock is a timeout here
/// rather than a suite that never ends.
#[test]
fn opens_of_one_profile_awaited_together_on_one_runtime_each_open() {
    let scratch = Scratch::new("one-runtime");
    for (workers, opens) in [(None, 2_usize), (Some(2), 3)] {
        let (sent, received) = std::sync::mpsc::channel();
        let profile = scratch.0.join(format!("profile-{opens}"));
        std::thread::spawn(move || {
            let mut builder = match workers {
                None => tokio::runtime::Builder::new_current_thread(),
                Some(workers) => {
                    let mut builder = tokio::runtime::Builder::new_multi_thread();
                    builder.worker_threads(workers);
                    builder
                }
            };
            let runtime = builder.enable_all().build().expect("a runtime");
            let opened = runtime.block_on(async move {
                let tasks: Vec<_> = (0..opens)
                    .map(|_| {
                        let profile = profile.clone();
                        tokio::spawn(async move {
                            match LocalStore::open(&profile).await {
                                Ok(store) => {
                                    store.close().await;
                                    true
                                }
                                Err(_) => false,
                            }
                        })
                    })
                    .collect();
                let mut opened = 0_usize;
                for task in tasks {
                    opened += usize::from(task.await.unwrap_or(false));
                }
                opened
            });
            let _gone_when_the_test_is = sent.send(opened);
        });
        assert_eq!(
            received.recv_timeout(std::time::Duration::from_secs(30)),
            Ok(opens),
            "{opens} opens of one profile on a runtime of {workers:?} worker(s)"
        );
    }
}

/// One profile opened from several threads at once opens every time.
///
/// **With one fixed name for the owner probe, a quarter of them would fail**,
/// measured: four threads opening an existing profile a hundred times each
/// would lose 175 of 400 opens, because every open would remove whatever stood
/// under that name — another open's live probe — as a crashed run's leftover.
/// Some would come back `File exists`, as `NC-DB-0006`; others `No such file or
/// directory`. SQLite's WAL lets a second app instance or a command-line tool
/// open the file beside the daemon, which is this.
#[test]
fn one_profile_opened_from_several_threads_at_once_opens_every_time() {
    let (threads, opens) = (4_usize, 25_usize);
    let scratch = Scratch::new("concurrent");
    let profile = scratch.profile();
    let runtime = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
    };
    runtime().block_on(async {
        LocalStore::open(&profile)
            .await
            .expect("opens")
            .close()
            .await;
    });
    let failed: Vec<String> = std::thread::scope(|scope| {
        let running: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    runtime().block_on(async {
                        let mut failed = Vec::new();
                        for _ in 0..opens {
                            match LocalStore::open(&profile).await {
                                Ok(store) => store.close().await,
                                Err(error) => failed.push(error.to_string()),
                            }
                        }
                        failed
                    })
                })
            })
            .collect();
        running
            .into_iter()
            .flat_map(|thread| thread.join().expect("the thread ran"))
            .collect()
    });
    assert!(
        failed.is_empty(),
        "{} of {} opens failed: {:?}",
        failed.len(),
        threads * opens,
        &failed[..failed.len().min(3)]
    );
    let left: Vec<String> = std::fs::read_dir(&profile)
        .expect("readable")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| !name.starts_with(nc_local::FILE_NAME))
        .collect();
    assert!(left.is_empty(), "a probe was left behind: {left:?}");
}

/// Statistics gathered by hand make the next open refuse the file — the
/// hazard the source gate's refusal of `ANALYZE` exists for.
///
/// **Written down where it bites.** SQLite recommends `PRAGMA optimize` before
/// a connection closes, and `sqlx` offers it as `optimize_on_close`; each
/// writes `sqlite_stat1`, and this build's SQLite `sqlite_stat4` beside it —
/// tables no migration builds, so the self-check refuses the user's own file
/// at every open after, `NC-DB-0007`, critical and not retryable. Measured with
/// both statements after fifty units. `cargo xtask check` refuses either in
/// the crates that open the file; this is what it would cost if one got in.
#[tokio::test]
async fn statistics_gathered_by_hand_are_refused_at_the_next_open() {
    for (index, gathering) in ["PRAGMA optimize", "ANALYZE"].into_iter().enumerate() {
        let scratch = Scratch::new(&format!("statistics-{index}"));
        let profile = scratch.profile();
        let store = LocalStore::open(&profile).await.expect("opens");
        for n in 1..=50_u64 {
            let unit =
                nc_ids::Snowflake::from_crockford(&nc_ids::base32::encode(n)).expect("a unit");
            let record = nc_local::UnitRecord {
                unit,
                meta: nc_local::UnitMeta {
                    realm: nc_ids::OpaqueId::new("home").expect("an identifier"),
                    principal: nc_ids::OpaqueId::new("ana").expect("an identifier"),
                    kind: nc_local::UnitKind::Step,
                    label: format!("unit {n}"),
                    committed_at_ms: n,
                },
                writes: Vec::new(),
                blobs: Vec::new(),
                notes: Vec::new(),
                unpinned: Vec::new(),
                marks: Vec::new(),
            };
            let quotas = nc_local::Quotas {
                entries_per_plugin: 1,
                lowered: std::collections::BTreeMap::new(),
                blobs: 1,
                writes_per_unit: 1,
                notes_per_unit: 1,
            };
            store.commit(&record, &quotas).await.expect("commits");
        }
        sqlx::query(gathering)
            .execute(store.pool())
            .await
            .unwrap_or_else(|error| panic!("{gathering}: {error}"));
        store.close().await;
        let error = refused(&profile).await;
        assert_eq!(
            error.code(),
            ErrorCode::DbLocalSchemaUnexpected,
            "{gathering}: {error}"
        );
        assert!(
            error.to_string().contains("`sqlite_stat1`"),
            "{gathering}: {error}"
        );
    }
}

/// Another application's SQLite file is refused as unexpected, and left as it
/// was.
///
/// **`NC-DB-0007`, as `AGENTS.md` promises**, for a file whose only fault is
/// its header — and not `NC-DB-0006`, whose message is about who may reach the
/// data.
#[tokio::test]
async fn another_applications_database_is_refused_and_left_alone() {
    let scratch = Scratch::new("foreign");
    let profile = scratch.profile();
    std::fs::create_dir(&profile).expect("a directory");
    private(&profile);
    let file = profile.join(nc_local::FILE_NAME);
    let mut foreign = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&file)
        .create_if_missing(true)
        .connect()
        .await
        .expect("a database of somebody else's");
    sqlx::query("CREATE TABLE theirs (x INTEGER)")
        .execute(&mut foreign)
        .await
        .expect("a table");
    foreign.close().await.expect("closes");
    private(&file);
    let before = std::fs::read(&file).expect("readable");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(error.to_string().contains("application 0"), "{error}");
    // **Byte for byte, and nothing beside it.** Set before the file was asked
    // whose it is, the pool's `journal_mode = WAL` would convert it, measured:
    // its header going from `[1, 1]` to `[2, 2]` and a `-wal` and a `-shm`
    // appearing, under a message saying it was left as it is.
    assert!(
        std::fs::read(&file).expect("readable") == before,
        "a file refused as another application's was written to"
    );
    let beside: Vec<String> = std::fs::read_dir(&profile)
        .expect("readable")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| !is_the_lock_file(name))
        .collect();
    assert_eq!(beside, [nc_local::FILE_NAME], "{beside:?}");
    let mut connection = raw(&profile).await;
    let tables: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_schema")
        .fetch_all(&mut connection)
        .await
        .expect("readable");
    assert_eq!(tables, ["theirs"], "a refused file was migrated anyway");
}

/// A file that is not a database at all is refused as unexpected.
#[tokio::test]
async fn a_file_that_is_not_a_database_is_refused() {
    let scratch = Scratch::new("not-a-database");
    let profile = scratch.profile();
    std::fs::create_dir(&profile).expect("a directory");
    private(&profile);
    let file = profile.join(nc_local::FILE_NAME);
    std::fs::write(&file, vec![0x5a_u8; 8192]).expect("garbage");
    private(&file);
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
}

/// A trigger dropped by hand is found at the next open, and the store refuses.
///
/// **This is the attack the append-only images answer, and the one the file
/// cannot stop by itself**: SQLite has no roles, so whoever can open the file
/// can `DROP TRIGGER`. What this crate can do is refuse to go on as though the
/// images were still protected.
#[tokio::test]
async fn a_dropped_trigger_is_refused_at_open() {
    let scratch = Scratch::new("dropped-trigger");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    let mut connection = raw(&profile).await;
    sqlx::query("DROP TRIGGER main.store_writes_are_never_edited")
        .execute(&mut connection)
        .await
        .expect("the owner can drop it");
    connection.close().await.expect("closes");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(
        error
            .to_string()
            .contains("the trigger `store_writes_are_never_edited` is missing"),
        "{error}"
    );
}

/// A trigger added by hand is found at the next open, and the store refuses.
#[tokio::test]
async fn an_added_trigger_is_refused_at_open() {
    let scratch = Scratch::new("added-trigger");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    let mut connection = raw(&profile).await;
    sqlx::query(
        "CREATE TRIGGER main.swallow BEFORE INSERT ON store_entries \
         BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&mut connection)
    .await
    .expect("the owner can add one");
    connection.close().await.expect("closes");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(
        error.to_string().contains("the trigger `swallow`"),
        "{error}"
    );
}

/// A table edited by hand is found at the next open by its SQL's digest.
#[tokio::test]
async fn an_edited_table_is_refused_at_open() {
    let scratch = Scratch::new("edited-table");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    let mut connection = raw(&profile).await;
    sqlx::query("ALTER TABLE main.store_entries ADD COLUMN spare TEXT")
        .execute(&mut connection)
        .await
        .expect("the owner can alter it");
    connection.close().await.expect("closes");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(
        error
            .to_string()
            .contains("the table `store_entries` is not the one"),
        "{error}"
    );
}

/// A header rewritten by hand is another application's file, which is not the
/// one this build expects.
#[tokio::test]
async fn a_rewritten_header_is_refused_at_open() {
    let scratch = Scratch::new("header");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    let mut connection = raw(&profile).await;
    sqlx::query("PRAGMA application_id = 7")
        .execute(&mut connection)
        .await
        .expect("the owner can rewrite it");
    connection.close().await.expect("closes");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(error.to_string().contains("application 7"), "{error}");
}

/// A migration edited after it ran is a database this build does not know,
/// refused before anything writes the file: one in rollback mode is left in it.
///
/// **Not left to the migrator**: the pool that runs it converts a file to `WAL`
/// as it connects, so the header's bytes would go from `[1, 1]` to `[2, 2]`
/// and a `-wal` appear, under a message saying nothing was changed.
#[tokio::test]
async fn a_migration_edited_after_it_ran_is_refused() {
    let scratch = Scratch::new("edited-migration");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    tamper(
        &profile,
        &[
            "UPDATE main._sqlx_migrations SET checksum = x'00' WHERE version = 1",
            "PRAGMA journal_mode = DELETE",
        ],
    )
    .await;
    let before = in_rollback_mode(&profile);
    let error = refused(&profile).await;
    left_as_it_stood(&profile, &before);
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(
        error
            .to_string()
            .contains("migration 1 as other SQL than this build carries"),
        "{error}"
    );
}

/// A migration the file records as not run to its end is reported as one
/// that failed, as the migrator reports it, and refused before anything
/// writes the file: one in rollback mode is left in it.
#[tokio::test]
async fn a_migration_recorded_as_not_run_to_its_end_is_refused_and_left_as_it_stood() {
    let scratch = Scratch::new("unfinished-in-rollback-mode");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    tamper(
        &profile,
        &[
            "UPDATE main._sqlx_migrations SET success = 0 WHERE version = 3",
            "PRAGMA journal_mode = DELETE",
        ],
    )
    .await;
    let before = in_rollback_mode(&profile);
    let error = refused(&profile).await;
    left_as_it_stood(&profile, &before);
    assert_eq!(error.code(), ErrorCode::DbMigrationFailed, "{error}");
    assert!(
        error
            .to_string()
            .contains("migration 3 is partially applied — the file records it"),
        "{error}"
    );
}

/// The profile as it stands, once its file is shown to be in rollback mode —
/// the header's two journal bytes `[1, 1]`, which the store's pool would turn
/// to `[2, 2]` — with nothing beside it.
fn in_rollback_mode(profile: &Path) -> Vec<(String, Vec<u8>)> {
    let before = as_it_stands(profile);
    assert_eq!(
        before
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        [nc_local::FILE_NAME],
        "a file in rollback mode keeps nothing beside it"
    );
    assert_eq!(
        before[0].1.get(18..20),
        Some([1_u8, 1].as_slice()),
        "the file is not in rollback mode, so a conversion would not show"
    );
    before
}

/// A migration this build does not carry means a newer build's database.
#[tokio::test]
async fn a_migration_this_build_does_not_carry_is_refused() {
    let scratch = Scratch::new("newer-migration");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    let mut connection = raw(&profile).await;
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, \
         execution_time) VALUES (9999, 'from the future', 1, x'00', 0)",
    )
    .execute(&mut connection)
    .await
    .expect("writable");
    connection.close().await.expect("closes");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(error.to_string().contains("9999"), "{error}");
}

/// Runs each statement on the file the way somebody with write access to it can.
async fn tamper(profile: &Path, statements: &[&'static str]) {
    let mut connection = raw(profile).await;
    for &statement in statements {
        sqlx::query(statement)
            .execute(&mut connection)
            .await
            .unwrap_or_else(|error| panic!("{statement}: {error}"));
    }
    connection.close().await.expect("closes");
}

/// Puts a file one migration behind: `0003_unit_holds.sql`'s table gone and
/// its row out of the history, as the build before it would have left the
/// file.
async fn one_migration_behind(profile: &Path) {
    tamper(
        profile,
        &[
            "DROP TABLE main.store_unit_holds",
            "DELETE FROM main._sqlx_migrations WHERE version = 3",
        ],
    )
    .await;
}

/// Whether the file holds `store_unit_holds`, the newest migration's table, and
/// which migrations it records — read without the store, after it answered.
async fn what_the_file_holds(profile: &Path) -> (bool, Vec<i64>) {
    let mut connection = raw(profile).await;
    let newest: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM main.sqlite_schema WHERE name = 'store_unit_holds'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("the schema reads");
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM main._sqlx_migrations ORDER BY version")
            .fetch_all(&mut connection)
            .await
            .expect("the history reads");
    connection.close().await.expect("closes");
    (newest == 1, versions)
}

/// A table the file's history does not account for is refused before the
/// migrator runs, and the migration that builds it is not applied again.
///
/// **A history that forgot `0002` while its table stayed**: left to the
/// migrator, it would apply the migration again and SQLite refuse the table it
/// already has — `NC-DB-0002`. The self-check holds the file to what its own
/// history built before anything is applied, so the table is one no recorded
/// migration made, and the file is left as it was. It forgets the newest
/// migration here, which is what a history may lose and still be a beginning of
/// this build's.
#[tokio::test]
async fn a_table_its_history_does_not_account_for_is_refused_and_left_as_it_was() {
    let scratch = Scratch::new("unaccounted-table");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    tamper(
        &profile,
        &["DELETE FROM main._sqlx_migrations WHERE version = 3"],
    )
    .await;
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(
        error.to_string().contains(
            "the table `store_unit_holds` on `store_unit_holds` is not one `local-migrations/` \
             builds"
        ),
        "{error}"
    );
    assert_eq!(what_the_file_holds(&profile).await, (true, vec![1, 2]));
}

/// A migration the history records as unfinished is reported as a migration
/// that failed, and nothing is applied after it.
///
/// **What `NC-DB-0002` answers at open.** A file whose tables are what its
/// history built is the one the migrator is let near, and a row it wrote for a
/// migration that did not finish is what is left of one that failed.
#[tokio::test]
async fn a_migration_recorded_as_unfinished_is_reported_as_one() {
    let scratch = Scratch::new("unfinished-migration");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    tamper(
        &profile,
        &["UPDATE main._sqlx_migrations SET success = 0 WHERE version = 2"],
    )
    .await;
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbMigrationFailed, "{error}");
    assert!(
        error
            .to_string()
            .contains("migration 2 is partially applied"),
        "{error}"
    );
}

/// A file one migration behind is migrated, and opens.
///
/// **The other side of judging a file before it is migrated**: it is held to
/// what its own history built, so the file the build before this one left — no
/// `store_unit_holds` yet, and no row for `0003_unit_holds.sql` — is the file
/// the migrator is for. Held to the whole list, it would be refused for lacking the very table
/// the migration it waits for builds.
#[tokio::test]
async fn a_file_one_migration_behind_is_migrated_and_opens() {
    let scratch = Scratch::new("one-behind");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    one_migration_behind(&profile).await;
    assert_eq!(what_the_file_holds(&profile).await, (false, vec![1, 2]));
    LocalStore::open(&profile)
        .await
        .expect("a file one migration behind opens")
        .close()
        .await;
    assert_eq!(what_the_file_holds(&profile).await, (true, vec![1, 2, 3]));
}

/// A file one migration behind that the self-check refuses is refused before
/// the migrator writes anything into it.
///
/// **Not written to first**: with the migrator run before the self-check, a
/// file one migration behind with a trigger dropped would have `0002_blobs.sql`
/// applied and then be refused as `NC-DB-0007` — whose message tells the user
/// nothing was changed.
#[tokio::test]
async fn a_file_one_migration_behind_is_refused_before_the_migrator_writes() {
    let scratch = Scratch::new("behind-and-tampered");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    one_migration_behind(&profile).await;
    tamper(
        &profile,
        &["DROP TRIGGER main.store_notes_are_never_edited"],
    )
    .await;
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(
        error
            .to_string()
            .contains("the trigger `store_notes_are_never_edited` is missing"),
        "{error}"
    );
    assert_eq!(
        what_the_file_holds(&profile).await,
        (false, vec![1, 2]),
        "the migrator wrote into a file the store refused"
    );
}

/// A history with a gap in it is not one this build wrote, and the file is
/// refused as it is.
///
/// **The one history the migrator would write into before refusing it**: it
/// applies what a file lacks in order and checks a recorded migration's
/// checksum only when it reaches it, so a file that records `0002` and not
/// `0001` would have `0001_plugin_store.sql` applied before a modified `0002`
/// was found.
#[tokio::test]
async fn a_history_with_a_gap_is_refused_and_left_as_it_was() {
    let scratch = Scratch::new("gap");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    // What `0001_plugin_store.sql` built goes with its row, so the objects are
    // exactly what the history that is left would have built.
    tamper(
        &profile,
        &[
            "DROP TABLE main.store_notes",
            "DROP TABLE main.store_writes",
            "DROP TABLE main.store_units",
            "DROP TABLE main.store_entries",
            "DELETE FROM main._sqlx_migrations WHERE version = 1",
        ],
    )
    .await;
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(
        error
            .to_string()
            .contains("records the migrations [2, 3], and this build applies [1, 2, 3]"),
        "{error}"
    );
    assert_eq!(what_the_file_holds(&profile).await, (true, vec![2, 3]));
    let mut connection = raw(&profile).await;
    let entries: i64 =
        sqlx::query_scalar("SELECT count(*) FROM main.sqlite_schema WHERE name = 'store_entries'")
            .fetch_one(&mut connection)
            .await
            .expect("the schema reads");
    connection.close().await.expect("closes");
    assert_eq!(
        entries, 0,
        "`0001_plugin_store.sql` was applied to a refused file"
    );
}

/// A newer build's file is refused by the migration it records and this build
/// does not carry, and left as it was, byte for byte.
///
/// **Refused by the version, before the schema is judged**: what a newer
/// build made is not this build's list to judge, and its table would
/// otherwise be refused as one `local-migrations/` does not build — true, and
/// not the reason. The file lacks `0003_unit_holds.sql` as well, so a migrator
/// that applied it on the way to refusing would leave `store_unit_holds`
/// behind; and it is in rollback mode, which the pool that runs the migrator
/// would have converted.
#[tokio::test]
async fn a_newer_builds_file_is_refused_by_its_version_and_left_as_it_was() {
    let scratch = Scratch::new("newer-build");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    one_migration_behind(&profile).await;
    tamper(
        &profile,
        &[
            "INSERT INTO main._sqlx_migrations (version, description, success, checksum, \
             execution_time) VALUES (9999, 'from a newer build', 1, x'00', 0)",
            "CREATE TABLE main.from_a_newer_build (x INTEGER)",
            "PRAGMA journal_mode = DELETE",
        ],
    )
    .await;
    let before = in_rollback_mode(&profile);
    let error = refused(&profile).await;
    left_as_it_stood(&profile, &before);
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(
        error
            .to_string()
            .contains("migration 9999, which this build does not carry"),
        "{error}"
    );
    assert_eq!(
        what_the_file_holds(&profile).await,
        (false, vec![1, 2, 9999])
    );
}

/// A file damaged on disk is refused rather than read.
///
/// **This is SQLite refusing, not the self-check.** Page 2 is the root of
/// `_sqlx_migrations`, the first table the migrator builds, so the migrator's
/// own read of its history meets `SQLITE_CORRUPT` before `quick_check` is
/// asked — measured, with the self-check's verdict ignored, this still passes.
/// A page nothing reads before the self-check is
/// `a_damaged_index_page_is_refused_by_the_self_check`'s.
#[tokio::test]
async fn a_damaged_file_is_refused() {
    let scratch = Scratch::new("damaged");
    let profile = scratch.profile();
    LocalStore::open(&profile)
        .await
        .expect("opens")
        .close()
        .await;
    let file = profile.join(nc_local::FILE_NAME);
    let mut bytes = std::fs::read(&file).expect("readable");
    let page = 4096;
    assert!(
        bytes.len() > 2 * page,
        "the file has fewer pages than this test damages"
    );
    for byte in &mut bytes[page..2 * page] {
        *byte = 0xa5;
    }
    std::fs::write(&file, &bytes).expect("writable");
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
}

/// A damaged page of an index is refused at open, by the self-check's own
/// `quick_check`.
///
/// **`quick_check` is what refuses it.** Measured, with its verdict ignored —
/// `if verdicts.is_empty() && verdicts != ["ok"]` — a store would open over a
/// file whose index `store_units_by_principal` has its root page overwritten,
/// and this fails, as do the tests of a damaged file one migration behind and
/// of one with frames in its `-wal`. Nothing before the self-check reads that
/// index, so this is the one damage only `quick_check` can see: the header and
/// the migrator's history are untouched, and the store would have
/// answered `units` from a broken index.
#[tokio::test]
async fn a_damaged_index_page_is_refused_by_the_self_check() {
    let scratch = Scratch::new("damaged-index");
    let profile = scratch.profile();
    a_damaged_index(&profile, false).await;
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(error.to_string().contains("quick_check"), "{error}");
}

/// A damaged file one migration behind is refused before the migrator writes
/// a page of it.
///
/// **No `CREATE TABLE` written into it first**: with the migrator run before
/// `quick_check`, this file would have `0002_blobs.sql` applied — 20,632 bytes
/// of WAL — and then be refused as damaged, with a message that says nothing
/// was changed. A write into a damaged b-tree is how a damaged page becomes a
/// lost one.
#[tokio::test]
async fn a_damaged_file_one_migration_behind_is_refused_before_the_migrator_writes() {
    let scratch = Scratch::new("damaged-and-behind");
    let profile = scratch.profile();
    a_damaged_index(&profile, true).await;
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(error.to_string().contains("quick_check"), "{error}");
    assert_eq!(
        what_the_file_holds(&profile).await,
        (false, vec![1, 2]),
        "the migrator wrote into a file `quick_check` calls damaged"
    );
}

/// A store whose index `store_units_by_principal` has its root page
/// overwritten, and nothing else damaged — one migration behind when `behind`.
async fn a_damaged_index(profile: &Path, behind: bool) {
    let store = LocalStore::open(profile).await.expect("opens");
    // Enough units that the index's root holds cells to damage; an empty one is
    // a header and nothing else.
    for n in 1..=400_u64 {
        let unit = nc_ids::Snowflake::from_crockford(&nc_ids::base32::encode(n)).expect("a unit");
        let record = nc_local::UnitRecord {
            unit,
            meta: nc_local::UnitMeta {
                realm: nc_ids::OpaqueId::new("home").expect("an identifier"),
                principal: nc_ids::OpaqueId::new("ana").expect("an identifier"),
                kind: nc_local::UnitKind::Step,
                label: format!("unit {n}"),
                committed_at_ms: n,
            },
            writes: Vec::new(),
            blobs: Vec::new(),
            notes: Vec::new(),
            unpinned: Vec::new(),
            marks: Vec::new(),
        };
        let quotas = nc_local::Quotas {
            entries_per_plugin: 1,
            lowered: std::collections::BTreeMap::new(),
            blobs: 1,
            writes_per_unit: 1,
            notes_per_unit: 1,
        };
        store.commit(&record, &quotas).await.expect("commits");
    }
    store.close().await;
    if behind {
        one_migration_behind(profile).await;
    }

    let mut connection = raw(profile).await;
    let root: i64 = sqlx::query_scalar(
        "SELECT rootpage FROM main.sqlite_schema WHERE name = 'store_units_by_principal'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("the index is there");
    let size: i64 = sqlx::query_scalar("PRAGMA main.page_size")
        .fetch_one(&mut connection)
        .await
        .expect("readable");
    connection.close().await.expect("closes");
    let (root, size) = (
        usize::try_from(root).expect("a page number"),
        usize::try_from(size).expect("a page size"),
    );
    assert!(
        root > 2,
        "the index's root is page {root}, which the migrator reads"
    );

    // Everything past the twelve bytes of a b-tree page's header.
    let file = profile.join(nc_local::FILE_NAME);
    let mut bytes = std::fs::read(&file).expect("readable");
    let page = (root - 1) * size;
    assert!(
        bytes.len() >= page + size,
        "the file ends before page {root}"
    );
    for byte in &mut bytes[page + 12..page + size] {
        *byte = 0xa5;
    }
    std::fs::write(&file, &bytes).expect("writable");
}

/// The migrations this build embedded are the ones `local-migrations/` holds.
///
/// **`sqlx::migrate!` reads the directory when this crate is compiled, and on
/// stable Rust it does not tell cargo that it did** — `sqlx-macros-core`
/// tracks the path only under `cfg(sqlx_macros_unstable)`. So a migration
/// added there is not in this crate until one of its sources changes.
/// Measured: with a migration holding `SELECT 1;` added there, a fresh
/// database is migrated without it until `crates/nc-local/src/store.rs` is
/// touched, and a migration that builds no
/// object changes nothing the schema list or the gate compares. So the
/// directory is read here at run time, as `cargo xtask check` reads it for its
/// scratch file, and held to what the embedded migrator applied — version and
/// checksum, in order.
#[tokio::test]
async fn the_embedded_migrations_are_the_ones_on_disk() {
    let scratch = Scratch::new("embedded");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let applied: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version, checksum FROM main._sqlx_migrations ORDER BY version")
            .fetch_all(store.pool())
            .await
            .expect("the history reads");
    store.close().await;
    let directory = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../local-migrations"
    ));
    let on_disk = sqlx::migrate::Migrator::new(directory)
        .await
        .expect("`local-migrations/` reads");
    let carried: Vec<(i64, Vec<u8>)> = on_disk
        .iter()
        .map(|migration| (migration.version, migration.checksum.to_vec()))
        .collect();
    assert!(
        carried.len() >= 2,
        "`local-migrations/` read as {} migration(s), so this read the wrong directory",
        carried.len()
    );
    let versions =
        |held: &[(i64, Vec<u8>)]| held.iter().map(|(version, _)| *version).collect::<Vec<_>>();
    assert_eq!(
        applied,
        carried,
        "this build embedded the migrations {:?} and `local-migrations/` holds {:?}: \
         `sqlx::migrate!` read the directory when this crate was last compiled, and cargo \
         is not told to compile it again when a migration changes. `touch \
         crates/nc-local/src/store.rs` and run this again",
        versions(&applied),
        versions(&carried)
    );
}

/// The names in a directory, sorted.
fn names_in(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .expect("readable")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// Whether `name` is the profile's lock file, which an open leaves beside the
/// file where a file is what holds the profile: on Windows, which locks no
/// directory.
///
/// **Nowhere else is any name the lock**, so on a system that locks the
/// directory itself a file at `nc_local::LOCK_NAME` is something the open
/// made, and every listing here sees it.
fn is_the_lock_file(name: &str) -> bool {
    testing::HOLDS_THROUGH_A_FILE && name == nc_local::LOCK_NAME
}

/// The file, and each companion SQLite keeps beside it, as they stand: by
/// name, byte for byte.
///
/// **Not the profile's lock file** ([`is_the_lock_file`]), where a file is
/// what holds the profile: it holds nothing, and is no companion of SQLite's.
fn as_it_stands(profile: &Path) -> Vec<(String, Vec<u8>)> {
    names_in(profile)
        .into_iter()
        .filter(|name| !is_the_lock_file(name))
        .map(|name| {
            let bytes = std::fs::read(profile.join(&name)).expect("readable");
            (name, bytes)
        })
        .collect()
}

/// Holds a profile a refusal was handed to what stood in it before: every
/// file byte for byte, and nothing new beside them but the `-shm` SQLite
/// builds to read a `-wal` that had none.
fn left_as_it_stood(profile: &Path, before: &[(String, Vec<u8>)]) {
    let after = as_it_stands(profile);
    let index = format!("{}-shm", nc_local::FILE_NAME);
    let kept: Vec<&(String, Vec<u8>)> = after.iter().filter(|(name, _)| *name != index).collect();
    let stood: Vec<&(String, Vec<u8>)> = before.iter().collect();
    assert!(
        kept == stood,
        "a refused file was written to: {:?} stood, {:?} stand",
        before
            .iter()
            .map(|(name, bytes)| (name, bytes.len()))
            .collect::<Vec<_>>(),
        after
            .iter()
            .map(|(name, bytes)| (name, bytes.len()))
            .collect::<Vec<_>>()
    );
}

/// `from` copied to `to` byte for byte, though a writer holds `from` open.
///
/// **Read and written rather than `std::fs::copy`**, which on Windows is
/// `CopyFileExW`: it shares the file it reads for reading alone, so a file a
/// connection holds open to write is refused to it. A file opened to read
/// shares it for writing too, as SQLite's own handles do.
fn copied(from: impl AsRef<Path>, to: &Path) {
    std::fs::write(to, std::fs::read(from).expect("readable")).expect("copied");
}

/// A profile holding the file and a `-wal` with committed frames that were
/// never checkpointed — what a process killed after a commit leaves — made by
/// running `statements` through a writer still open when the two are copied.
async fn a_profile_with_frames_in_its_wal(
    scratch: &Scratch,
    name: &str,
    setup: &[&'static str],
    statements: &[&'static str],
) -> PathBuf {
    let source = scratch.0.join(format!("{name}-source"));
    std::fs::create_dir(&source).expect("a directory");
    let file = source.join(nc_local::FILE_NAME);
    let mut writer = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&file)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .connect()
        .await
        .expect("a WAL database");
    for statement in setup
        .iter()
        .chain(&[
            "PRAGMA wal_checkpoint(TRUNCATE)",
            "PRAGMA wal_autocheckpoint = 0",
        ])
        .chain(statements)
    {
        sqlx::query(*statement)
            .execute(&mut writer)
            .await
            .expect("written");
    }
    let profile = scratch.0.join(name);
    std::fs::create_dir(&profile).expect("a directory");
    private(&profile);
    for suffix in ["", "-wal"] {
        let to = profile.join(format!("{}{suffix}", nc_local::FILE_NAME));
        copied(format!("{}{suffix}", file.display()), &to);
        private(&to);
    }
    writer.close().await.expect("closes");
    profile
}

/// Another application's file in WAL mode, its `-wal` holding frames, is
/// refused and left as it stood.
///
/// **What it holds against**: the connection that judged the file being the
/// last to close it, so that SQLite checkpoints the `-wal` into the file and
/// deletes it — twenty-three bytes of the file rewritten — under a message
/// saying it was left as it is.
#[tokio::test]
async fn another_applications_wal_file_is_refused_and_left_as_it_stood() {
    let scratch = Scratch::new("foreign-wal");
    let profile = a_profile_with_frames_in_its_wal(
        &scratch,
        "profile",
        &[
            "PRAGMA application_id = 42",
            "CREATE TABLE theirs (x INTEGER)",
        ],
        &["INSERT INTO theirs VALUES (1), (2), (3)"],
    )
    .await;
    let before = as_it_stands(&profile);
    assert_eq!(
        names_in(&profile),
        [
            nc_local::FILE_NAME.to_owned(),
            format!("{}-wal", nc_local::FILE_NAME)
        ]
    );
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(
        error.to_string().contains("It was left as it is"),
        "{error}"
    );
    left_as_it_stood(&profile, &before);
}

/// A file `quick_check` calls damaged, its `-wal` holding committed frames,
/// is refused and left as it stood: the frames are not written into it.
///
/// The same cause as the test above's: the judging connection's close would
/// checkpoint the log into a file it had just found damaged.
#[tokio::test]
async fn a_damaged_file_with_frames_in_its_wal_is_refused_and_left_as_it_stood() {
    let scratch = Scratch::new("damaged-wal");
    let base = scratch.0.join("base");
    let store = LocalStore::open(&base).await.expect("opens");
    for n in 1..=400_u64 {
        let record = nc_local::UnitRecord {
            unit: nc_ids::Snowflake::from_crockford(&nc_ids::base32::encode(n)).expect("a unit"),
            meta: nc_local::UnitMeta {
                realm: nc_ids::OpaqueId::new("home").expect("an identifier"),
                principal: nc_ids::OpaqueId::new("ana").expect("an identifier"),
                kind: nc_local::UnitKind::Step,
                label: format!("unit {n}"),
                committed_at_ms: n,
            },
            writes: Vec::new(),
            blobs: Vec::new(),
            notes: Vec::new(),
            unpinned: Vec::new(),
            marks: Vec::new(),
        };
        let quotas = nc_local::Quotas {
            entries_per_plugin: 1,
            lowered: std::collections::BTreeMap::new(),
            blobs: 1,
            writes_per_unit: 1,
            notes_per_unit: 1,
        };
        store.commit(&record, &quotas).await.expect("commits");
    }
    store.close().await;
    let file = base.join(nc_local::FILE_NAME);
    let mut writer = raw(&base).await;
    for statement in [
        "PRAGMA wal_autocheckpoint = 0",
        "INSERT INTO main.store_entries (realm, plugin, store_key, value, rev_unit, rev_n) \
         VALUES ('home', 'p', 'k', '1', 1, 0)",
    ] {
        sqlx::query(statement)
            .execute(&mut writer)
            .await
            .expect("a committed key left in the log");
    }
    let profile = scratch.0.join("profile");
    std::fs::create_dir(&profile).expect("a directory");
    private(&profile);
    for suffix in ["", "-wal"] {
        let to = profile.join(format!("{}{suffix}", nc_local::FILE_NAME));
        copied(format!("{}{suffix}", file.display()), &to);
        private(&to);
    }
    let root: i64 = sqlx::query_scalar(
        "SELECT rootpage FROM main.sqlite_schema WHERE name = 'store_units_by_principal'",
    )
    .fetch_one(&mut writer)
    .await
    .expect("the index is there");
    let size: i64 = sqlx::query_scalar("PRAGMA main.page_size")
        .fetch_one(&mut writer)
        .await
        .expect("readable");
    writer.close().await.expect("closes");
    let copy = profile.join(nc_local::FILE_NAME);
    let (root, size) = (
        usize::try_from(root).expect("a page"),
        usize::try_from(size).expect("a page size"),
    );
    let mut bytes = std::fs::read(&copy).expect("readable");
    let page = (root - 1) * size;
    for byte in &mut bytes[page + 12..page + size] {
        *byte = 0xa5;
    }
    std::fs::write(&copy, &bytes).expect("writable");
    let before = as_it_stands(&profile);
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(error.to_string().contains("quick_check"), "{error}");
    left_as_it_stood(&profile, &before);
}

/// A file beside a hot rollback journal is refused as one this build did not
/// leave, and left as it stood: the journal is not rolled back into it.
///
/// The variant of the two above: a connection that may write rolls a hot
/// journal back on its first read, and this build's database keeps a
/// write-ahead log, never such a journal.
#[tokio::test]
async fn a_file_beside_a_hot_journal_is_refused_and_left_as_it_stood() {
    let scratch = Scratch::new("hot-journal");
    let source = scratch.0.join("source");
    std::fs::create_dir(&source).expect("a directory");
    let file = source.join(nc_local::FILE_NAME);
    let mut writer = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&file)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Delete)
        .connect()
        .await
        .expect("a database in rollback mode");
    for statement in [
        "PRAGMA application_id = 42",
        "CREATE TABLE theirs (x BLOB)",
        "INSERT INTO theirs VALUES (1)",
        "PRAGMA cache_size = 1",
        "BEGIN",
        "INSERT INTO theirs SELECT randomblob(4000) FROM (WITH RECURSIVE n(i) AS \
         (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 200) SELECT i FROM n)",
    ] {
        sqlx::query(statement)
            .execute(&mut writer)
            .await
            .expect("written");
    }
    let profile = scratch.profile();
    std::fs::create_dir(&profile).expect("a directory");
    private(&profile);
    for suffix in ["", "-journal"] {
        let to = profile.join(format!("{}{suffix}", nc_local::FILE_NAME));
        copied(format!("{}{suffix}", file.display()), &to);
        private(&to);
    }
    sqlx::query("ROLLBACK")
        .execute(&mut writer)
        .await
        .expect("rolled back");
    writer.close().await.expect("closes");
    let before = as_it_stands(&profile);
    let error = refused(&profile).await;
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(error.to_string().contains("rollback journal"), "{error}");
    left_as_it_stood(&profile, &before);
    assert_eq!(
        as_it_stands(&profile).len(),
        before.len(),
        "a file was made beside them other than the lock a profile held through a file keeps"
    );
}

/// Opens `profile` from `threads` threads at once, each with a runtime of its
/// own, and answers every failure.
fn opened_at_once(profile: &Path, threads: usize) -> Vec<String> {
    let barrier = std::sync::Barrier::new(threads);
    std::thread::scope(|scope| {
        let running: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("a runtime");
                    barrier.wait();
                    runtime.block_on(async {
                        match LocalStore::open(profile).await {
                            Ok(store) => {
                                store.close().await;
                                None
                            }
                            Err(error) => Some(error.to_string()),
                        }
                    })
                })
            })
            .collect();
        running
            .into_iter()
            .filter_map(|thread| thread.join().expect("the thread ran"))
            .collect()
    })
}

/// A profile nobody has opened yet, opened by several at once — two app
/// instances, the daemon and a command-line tool — opens for every one.
///
/// **What it holds against**: the directory and the file one open has just made
/// refused to the others as `NC-DB-0006`, ninety of a hundred and twenty opens.
#[test]
fn a_fresh_profile_opened_from_several_threads_at_once_opens_every_time() {
    let mut failed = Vec::new();
    let rounds = 30;
    for round in 0..rounds {
        let scratch = Scratch::new(&format!("fresh-{round}"));
        failed.extend(opened_at_once(&scratch.profile(), 4));
    }
    assert!(
        failed.is_empty(),
        "{} of {} opens failed: {:#?}",
        failed.len(),
        rounds * 4,
        &failed[..failed.len().min(6)]
    );
}

/// A profile whose file was made and never migrated — what a process stopped
/// between the two leaves — opened by several at once, opens for every one.
///
/// **What it holds against**: forty-nine of a hundred and twenty opens failing
/// — a migration applied twice at once, and a history read after another open's
/// migrations landed held against a schema read before them.
#[test]
fn an_unmigrated_file_opened_from_several_threads_at_once_opens_every_time() {
    let mut failed = Vec::new();
    let rounds = 30;
    for round in 0..rounds {
        let scratch = Scratch::new(&format!("unmigrated-{round}"));
        let profile = scratch.profile();
        std::fs::create_dir(&profile).expect("a directory");
        private(&profile);
        let file = profile.join(nc_local::FILE_NAME);
        std::fs::write(&file, b"").expect("an empty file");
        private(&file);
        failed.extend(opened_at_once(&profile, 4));
    }
    assert!(
        failed.is_empty(),
        "{} of {} opens failed: {:#?}",
        failed.len(),
        rounds * 4,
        &failed[..failed.len().min(6)]
    );
}

/// The environment variable that turns the test below into its own child,
/// and the store it opens.
const OPENED_IN: &str = "NC_LOCAL_OPENED_IN";

/// A store migrates a new file with the migrations the build embedded, from
/// whatever directory its process runs in — never with a directory it reads
/// when it runs.
///
/// **The test runs itself as a child**, started in a scratch directory two
/// levels below a copy of `local-migrations/` with a key written into the
/// first migration: cargo runs a test binary from its package's root, where
/// that relative path is this repository's own directory, so a child moved
/// elsewhere is what tells the embedded migrations from ones read where the
/// process stands. **The door this shuts**: a store migrating from
/// `Migrator::new(Path::new("../../local-migrations"))` — a fix the stale
/// embed above `MIGRATOR` invites — would pass a test run from the package's
/// root, and on a user's machine migrate their file with whatever SQL sits
/// beside the daemon's working directory.
#[tokio::test]
async fn a_store_migrates_with_what_the_build_embedded_wherever_it_runs() {
    if let Ok(profile) = std::env::var(OPENED_IN) {
        let profile = PathBuf::from(profile);
        let store = LocalStore::open(&profile).await.expect("the child opens");
        store.close().await;
        let mut connection = raw(&profile).await;
        let written: i64 = sqlx::query_scalar("SELECT count(*) FROM main.store_entries")
            .fetch_one(&mut connection)
            .await
            .expect("counted");
        assert_eq!(written, 0, "a key no unit wrote is in the new file");
        return;
    }
    let scratch = Scratch::new("wherever-it-runs");
    let runs_in = scratch.0.join("a").join("b");
    std::fs::create_dir_all(&runs_in).expect("a working directory");
    let beside = scratch.0.join("local-migrations");
    std::fs::create_dir(&beside).expect("a directory of migrations");
    let ours = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../local-migrations"
    ));
    let mut planted = 0_usize;
    for entry in std::fs::read_dir(ours).expect("the migrations are readable") {
        let entry = entry.expect("an entry");
        let mut text = std::fs::read_to_string(entry.path()).expect("readable");
        if entry.file_name().to_string_lossy().starts_with("0001") {
            text.push_str(
                "INSERT INTO main.store_entries VALUES \
                 ('planted', 'noctua:reference', 'planted', '\"beside\"', 1, 0);\n",
            );
            planted += 1;
        }
        std::fs::write(beside.join(entry.file_name()), text).expect("written");
    }
    assert_eq!(planted, 1, "the first migration was planted");
    let ran = the_child_in(&runs_in, &scratch.profile());
    let said = String::from_utf8_lossy(&ran.stdout);
    assert!(
        ran.status.success() && said.contains("test result: ok. 1 passed"),
        "the child that opened a new store from {} did not pass: {}\n{said}",
        runs_in.display(),
        ran.status
    );
}

/// Runs the test above in a child started in `runs_in`, opening `profile`,
/// and answers what it said: the parent holds it to having run that one test,
/// since a child that matched no test would pass having opened nothing.
#[expect(
    clippy::disallowed_methods,
    reason = "the child is this same test binary, started in another working directory — \
              the one way to move where a store runs without moving every other test of \
              this binary with it"
)]
fn the_child_in(runs_in: &Path, profile: &Path) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--exact",
            "a_store_migrates_with_what_the_build_embedded_wherever_it_runs",
            "--test-threads=1",
        ])
        .current_dir(runs_in)
        .env(OPENED_IN, profile)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the child runs")
}
