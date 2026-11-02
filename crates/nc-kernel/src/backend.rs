//! Where a plugin's data is kept: what the kernel reads for a plugin, and what
//! one unit's commit writes.
//!
//! **One trait, two stores, one behaviour.** [`StoreBackend`] is everything the
//! kernel asks of a plugin store, in `nc_local`'s types: a key's committed
//! value, a page of keys, one unit committed whole or not at all, the units a
//! person's history is read from, and the blobs this machine holds — and one
//! answer no kernel path asks in this build, `forget_beyond`, which the bound
//! on a person's history (`undo_history`) is for and nothing enforces yet:
//! the two stores are held to it alike until a kernel does. A daemon's
//! kernel runs on `nc_local::LocalStore`, the SQLite file of `NOCTUA.md §4`;
//! `MemoryBackend` holds the same in memory, behind `test-support`, so a suite
//! can load and run plugins without a file. **The same answers, not similar
//! ones**: `crates/nc-kernel/tests/both_stores_answer_alike.rs` runs one script
//! through each and holds every answer of one to the other's, and a property
//! test drives both through random sequences of the same operations — the
//! refusals' sentences included, because a kernel that tells a plugin why a
//! write was refused tells it what the store said.
//!
//! **The plugin is named on every call, and it is the kernel's to name.** A
//! plugin hands a host function a key and never the plugin it belongs to: the
//! kernel binds that from the manifest it loaded, so no plugin reads or writes
//! another's keys through here. **And so is the realm**: the kernels of the
//! realms a person belongs to share one store, each names its own, and what a
//! plugin keeps for one realm is never read or written in another.
//!
//! **Asynchronous, and not a trait object**, like `nc_journal::Store`: the store
//! that runs is SQLite through `sqlx`, and the kernel is generic over the store
//! rather than boxing every call.
//!
//! What this answers (`NOCTUA.md §2.4`): a store the kernel's suites run on
//! that behaves otherwise than the one a person's data is in, which would make
//! every suite a claim about a store nobody uses. What it does not answer: the
//! limits of the in-memory store, which holds everything in one process and
//! copies itself to commit, and is a test's for that reason.

use core::future::Future;

#[cfg(any(test, feature = "test-support"))]
use std::collections::BTreeMap;
#[cfg(any(test, feature = "test-support"))]
use std::sync::{Mutex, PoisonError};

use nc_ids::{OpaqueId, Snowflake};
use nc_local::{
    BlobRow, CommitError, Committed, Entry, LocalError, LocalStore, Page, Quotas, UnitDetail,
    UnitRecord, UnitRow, UnitState,
};
#[cfg(any(test, feature = "test-support"))]
use nc_local::{Expect, Image, Revision, UnitKind};

/// Everything the kernel asks of the store a plugin's data is kept in.
///
/// Each method answers exactly as the method of the same name on
/// `nc_local::LocalStore` does, which is the specification: its documentation
/// says what each refuses and why.
pub trait StoreBackend: core::fmt::Debug + Send + Sync + 'static {
    /// One key's committed value, a plugin's in a realm.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or holds a row it did not write.
    fn get(
        &self,
        realm: &OpaqueId,
        plugin: &str,
        key: &str,
    ) -> impl Future<Output = Result<Option<Entry>, LocalError>> + Send;

    /// Up to `limit` of a plugin's keys in a realm beginning with `prefix`, in
    /// byte order after `cursor` when one is given; a limit of zero is read as
    /// one.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked.
    fn list(
        &self,
        realm: &OpaqueId,
        plugin: &str,
        prefix: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> impl Future<Output = Result<Page, LocalError>> + Send;

    /// How many keys a plugin holds in a realm.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked.
    fn count(
        &self,
        realm: &OpaqueId,
        plugin: &str,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send;

    /// Commits one unit, whole or not at all.
    ///
    /// # Errors
    ///
    /// [`CommitError::Conflict`] when a key does not hold what the unit
    /// expected; [`CommitError::Quota`] and [`CommitError::TooManyBlobs`] at a
    /// bound; [`CommitError::Invalid`] for a record no unit can be;
    /// [`CommitError::Store`] when the store fails. In every case nothing the
    /// unit wrote is kept.
    fn commit(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<Committed, CommitError>> + Send;

    /// Commits one unit, whole or not at all, holding it and every unit it
    /// marks from the pruner until [`release`](Self::release) lets go of what
    /// it holds: what the kernel commits a unit it has not yet heard the
    /// journal close with.
    ///
    /// # Errors
    ///
    /// As [`commit`](Self::commit).
    fn commit_held(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<Committed, CommitError>> + Send;

    /// Lets go of every hold `holder` placed when it was committed, and
    /// answers how many there were.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or a write changed
    /// another number of rows than it had to.
    fn release(&self, holder: Snowflake) -> impl Future<Output = Result<u64, LocalError>> + Send;

    /// Every unit holding another or itself from the pruner, oldest first, of
    /// every realm the store holds.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or holds a row it did not write.
    fn holders(&self) -> impl Future<Output = Result<Vec<Snowflake>, LocalError>> + Send;

    /// A principal's committed units in a realm, newest first.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or holds a row it did not write.
    fn units(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<UnitRow>, LocalError>> + Send;

    /// A principal's steps in a realm, newest first: their units that are
    /// neither an undo nor a redo, undone or not.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or holds a row it did not write.
    fn steps(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<UnitRow>, LocalError>> + Send;

    /// One committed unit, with its images and its notes.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or holds a row it did not write.
    fn unit(
        &self,
        unit: Snowflake,
    ) -> impl Future<Output = Result<Option<UnitDetail>, LocalError>> + Send;

    /// Sets a committed unit's state; `false` when the store does not hold it.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked.
    fn set_state(
        &self,
        unit: Snowflake,
        state: UnitState,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send;

    /// Forgets a principal's units in a realm beyond the newest `keep`, with
    /// their images and notes, and answers how many went.
    ///
    /// **No kernel path asks it in this build**: nothing forgets a person's
    /// units yet (`crate::history`), and this is the answer the bound on their
    /// history will ask for, which both stores give alike meanwhile.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or refuses the delete —
    /// as `nc_local::LocalStore::forget_beyond` says.
    fn forget_beyond(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        keep: u32,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send;

    /// Whether a committed unit other than `except`, of any kind, holds the
    /// note `note`, in any realm.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked.
    fn noted_elsewhere(
        &self,
        note: &str,
        except: Snowflake,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send;

    /// One blob's row.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or holds a row it did not write.
    fn blob(&self, hash: &str) -> impl Future<Output = Result<Option<BlobRow>, LocalError>> + Send;

    /// Pins a blob again, or unpins it as of `at_ms`; `false` when the store
    /// does not hold it.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or `at_ms` is past what it can hold.
    fn set_pinned(
        &self,
        hash: &str,
        pinned: bool,
        at_ms: u64,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send;

    /// Up to `limit` blobs unpinned before `before_ms`, oldest first.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked, or `before_ms` is past what
    /// it can hold.
    fn unpinned_before(
        &self,
        before_ms: u64,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<String>, LocalError>> + Send;

    /// Forgets an unpinned blob; `false` when the store does not hold it or it
    /// is pinned.
    ///
    /// # Errors
    ///
    /// [`LocalError`] when the store cannot be asked.
    fn forget_blob(&self, hash: &str) -> impl Future<Output = Result<bool, LocalError>> + Send;

    /// The one pruner's target over the database this store keeps, or none
    /// for a store with no database under it.
    ///
    /// **The pruner `NOCTUA.md §15.8` asks for, over the file this store
    /// opened**: `nc_prune::sqlite::Sqlite` over `nc_local::LocalStore`'s own
    /// pool, which the kernel's `pump` runs for the tables
    /// `catalog/limits.toml` bounds in the local database — the one method
    /// with no namesake on that store, which holds the pool and not the
    /// pruner.
    fn pruner(&self) -> Option<nc_prune::sqlite::Sqlite>;
}

/// The store a daemon's kernel runs on: one SQLite file, private to its user.
impl StoreBackend for LocalStore {
    fn get(
        &self,
        realm: &OpaqueId,
        plugin: &str,
        key: &str,
    ) -> impl Future<Output = Result<Option<Entry>, LocalError>> + Send {
        Self::get(self, realm, plugin, key)
    }

    fn list(
        &self,
        realm: &OpaqueId,
        plugin: &str,
        prefix: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> impl Future<Output = Result<Page, LocalError>> + Send {
        Self::list(self, realm, plugin, prefix, limit, cursor)
    }

    fn count(
        &self,
        realm: &OpaqueId,
        plugin: &str,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        Self::count(self, realm, plugin)
    }

    fn commit(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<Committed, CommitError>> + Send {
        Self::commit(self, unit, quotas)
    }

    fn commit_held(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<Committed, CommitError>> + Send {
        Self::commit_held(self, unit, quotas)
    }

    fn release(&self, holder: Snowflake) -> impl Future<Output = Result<u64, LocalError>> + Send {
        Self::release(self, holder)
    }

    fn holders(&self) -> impl Future<Output = Result<Vec<Snowflake>, LocalError>> + Send {
        Self::holders(self)
    }

    fn units(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<UnitRow>, LocalError>> + Send {
        Self::units(self, realm, principal, limit)
    }

    fn steps(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<UnitRow>, LocalError>> + Send {
        Self::steps(self, realm, principal, limit)
    }

    fn unit(
        &self,
        unit: Snowflake,
    ) -> impl Future<Output = Result<Option<UnitDetail>, LocalError>> + Send {
        Self::unit(self, unit)
    }

    fn set_state(
        &self,
        unit: Snowflake,
        state: UnitState,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        Self::set_state(self, unit, state)
    }

    fn forget_beyond(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        keep: u32,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        Self::forget_beyond(self, realm, principal, keep)
    }

    fn noted_elsewhere(
        &self,
        note: &str,
        except: Snowflake,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        Self::noted_elsewhere(self, note, except)
    }

    fn blob(&self, hash: &str) -> impl Future<Output = Result<Option<BlobRow>, LocalError>> + Send {
        Self::blob(self, hash)
    }

    fn set_pinned(
        &self,
        hash: &str,
        pinned: bool,
        at_ms: u64,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        Self::set_pinned(self, hash, pinned, at_ms)
    }

    fn unpinned_before(
        &self,
        before_ms: u64,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<String>, LocalError>> + Send {
        Self::unpinned_before(self, before_ms, limit)
    }

    fn forget_blob(&self, hash: &str) -> impl Future<Output = Result<bool, LocalError>> + Send {
        Self::forget_blob(self, hash)
    }

    fn pruner(&self) -> Option<nc_prune::sqlite::Sqlite> {
        Some(nc_prune::sqlite::Sqlite::new(self.pool().clone()))
    }
}

/// A plugin store held in memory, which forgets everything when it is
/// dropped. **For tests**, behind `test-support`.
///
/// It answers every call as `nc_local::LocalStore` does, refusals and their
/// sentences included. A commit is made on a copy of everything held and kept
/// only when every write, bound and mark of the unit passed, which is what one
/// SQLite transaction gives the file — and costs a copy per commit, which is
/// one reason it is a test's.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct MemoryBackend {
    held: Mutex<Held>,
}

/// Everything a [`MemoryBackend`] holds.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Default)]
struct Held {
    /// Each key by its realm, its plugin and itself, as the file keys them.
    entries: BTreeMap<(String, String, String), Entry>,
    units: BTreeMap<Snowflake, UnitDetail>,
    blobs: BTreeMap<String, BlobRow>,
    /// Each unit held from the pruner, with the unit that holds it.
    holds: std::collections::BTreeSet<(Snowflake, Snowflake)>,
}

#[cfg(any(test, feature = "test-support"))]
impl MemoryBackend {
    /// What is held, whoever last held the lock: nothing here panics while
    /// holding it, and a copy is what a commit changes.
    fn held(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl StoreBackend for MemoryBackend {
    // **`ready` rather than an `async` block**, as `nc_journal::InMemory`
    // answers: nothing here waits, and the trait is asynchronous because the
    // store that runs is SQLite.
    fn get(
        &self,
        realm: &OpaqueId,
        plugin: &str,
        key: &str,
    ) -> impl Future<Output = Result<Option<Entry>, LocalError>> + Send {
        let held = self
            .held()
            .entries
            .get(&slot_of(realm, plugin, key))
            .cloned();
        core::future::ready(Ok(held))
    }

    fn list(
        &self,
        realm: &OpaqueId,
        plugin: &str,
        prefix: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> impl Future<Output = Result<Page, LocalError>> + Send {
        let limit = usize::try_from(limit.max(1)).unwrap_or(usize::MAX);
        let held = self.held();
        let mut keys: Vec<String> = held
            .entries
            .keys()
            .filter(|(of, owner, key)| {
                of == realm.as_str()
                    && owner == plugin
                    && key.starts_with(prefix)
                    && cursor.is_none_or(|after| key.as_str() > after)
            })
            .map(|(_, _, key)| key.clone())
            .take(limit.saturating_add(1))
            .collect();
        let more = keys.len() > limit;
        keys.truncate(limit);
        let next = if more { keys.last().cloned() } else { None };
        core::future::ready(Ok(Page { keys, next }))
    }

    fn count(
        &self,
        realm: &OpaqueId,
        plugin: &str,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        let held = self
            .held()
            .entries
            .keys()
            .filter(|(of, owner, _)| of == realm.as_str() && owner == plugin)
            .count();
        core::future::ready(Ok(u64::try_from(held).unwrap_or(u64::MAX)))
    }

    fn commit(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<Committed, CommitError>> + Send {
        let mut held = self.held();
        let answer = committed(&held, unit, quotas).map(|(next, committed)| {
            *held = next;
            committed
        });
        core::future::ready(answer)
    }

    fn commit_held(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<Committed, CommitError>> + Send {
        let mut held = self.held();
        let answer = committed(&held, unit, quotas).and_then(|(mut next, committed)| {
            let marked = unit.marks.iter().map(|(marked, _)| *marked);
            for at in core::iter::once(unit.unit).chain(marked) {
                // **One hold per unit and holder**, as the file's primary key
                // refuses a second, the whole commit with it. A property test
                // found a unit marking itself held twice here and refused there.
                if !next.holds.insert((at, unit.unit)) {
                    return Err(CommitError::Store(LocalError::Unexpected(
                        "holding a unit from the pruner: UNIQUE constraint failed: \
                         store_unit_holds.unit, store_unit_holds.holder"
                            .to_owned(),
                    )));
                }
            }
            *held = next;
            Ok(committed)
        });
        core::future::ready(answer)
    }

    fn release(&self, holder: Snowflake) -> impl Future<Output = Result<u64, LocalError>> + Send {
        let mut held = self.held();
        let before = held.holds.len();
        held.holds.retain(|(_, by)| *by != holder);
        let released = before.saturating_sub(held.holds.len());
        core::future::ready(Ok(u64::try_from(released).unwrap_or(u64::MAX)))
    }

    fn holders(&self) -> impl Future<Output = Result<Vec<Snowflake>, LocalError>> + Send {
        let holders: std::collections::BTreeSet<Snowflake> =
            self.held().holds.iter().map(|(_, by)| *by).collect();
        core::future::ready(Ok(holders.into_iter().collect()))
    }

    fn units(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<UnitRow>, LocalError>> + Send {
        let rows: Vec<UnitRow> = self
            .held()
            .units
            .values()
            .rev()
            .filter(|detail| {
                detail.row.meta.realm == *realm && detail.row.meta.principal == *principal
            })
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .map(|detail| detail.row.clone())
            .collect();
        core::future::ready(Ok(rows))
    }

    fn steps(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<UnitRow>, LocalError>> + Send {
        let held = self.held();
        let rows: Vec<UnitRow> = held
            .units
            .values()
            .rev()
            .filter(|detail| {
                detail.row.meta.realm == *realm
                    && detail.row.meta.principal == *principal
                    && detail.row.meta.kind == UnitKind::Step
            })
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .map(|detail| detail.row.clone())
            .collect();
        core::future::ready(Ok(rows))
    }

    fn unit(
        &self,
        unit: Snowflake,
    ) -> impl Future<Output = Result<Option<UnitDetail>, LocalError>> + Send {
        let detail = self.held().units.get(&unit).cloned();
        core::future::ready(Ok(detail))
    }

    fn set_state(
        &self,
        unit: Snowflake,
        state: UnitState,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        let marked = match self.held().units.get_mut(&unit) {
            Some(detail) => {
                detail.row.state = state;
                true
            }
            None => false,
        };
        core::future::ready(Ok(marked))
    }

    fn forget_beyond(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        keep: u32,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        let mut held = self.held();
        let beyond: Vec<Snowflake> = held
            .units
            .values()
            .rev()
            .filter(|detail| {
                detail.row.meta.realm == *realm && detail.row.meta.principal == *principal
            })
            .skip(usize::try_from(keep).unwrap_or(usize::MAX))
            .map(|detail| detail.row.unit)
            .collect();
        // **A unit still held refuses the whole delete**, nothing forgotten,
        // as the file's `ON DELETE RESTRICT` does. A reviewer found this store
        // forgetting it, leaving a hold on a unit it no longer had.
        if held.holds.iter().any(|(unit, _)| beyond.contains(unit)) {
            return core::future::ready(Err(LocalError::Unexpected(format!(
                "forgetting units: {} — {}",
                nc_local::STILL_REFERRED_TO,
                nc_local::STILL_HELD
            ))));
        }
        for unit in &beyond {
            held.units.remove(unit);
        }
        core::future::ready(Ok(u64::try_from(beyond.len()).unwrap_or(u64::MAX)))
    }

    fn noted_elsewhere(
        &self,
        note: &str,
        except: Snowflake,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        let held = self.held().units.values().any(|detail| {
            detail.row.unit != except
                && detail.row.state == UnitState::Committed
                && detail.notes.iter().any(|noted| noted == note)
        });
        core::future::ready(Ok(held))
    }

    fn blob(&self, hash: &str) -> impl Future<Output = Result<Option<BlobRow>, LocalError>> + Send {
        let row = self.held().blobs.get(hash).cloned();
        core::future::ready(Ok(row))
    }

    fn set_pinned(
        &self,
        hash: &str,
        pinned: bool,
        at_ms: u64,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        let at = if pinned {
            Ok(None)
        } else {
            as_signed(at_ms, "a time").map(|_| Some(at_ms))
        };
        let answer = at.map(|at| match self.held().blobs.get_mut(hash) {
            Some(row) => {
                row.pinned = pinned;
                row.unpinned_at_ms = at;
                true
            }
            None => false,
        });
        core::future::ready(answer)
    }

    fn unpinned_before(
        &self,
        before_ms: u64,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<String>, LocalError>> + Send {
        let answer = as_signed(before_ms, "a time").map(|_| {
            let held = self.held();
            let mut unpinned: Vec<(u64, &String)> = held
                .blobs
                .values()
                .filter(|row| !row.pinned)
                .filter_map(|row| {
                    row.unpinned_at_ms
                        .filter(|at| *at < before_ms)
                        .map(|at| (at, &row.hash))
                })
                .collect();
            unpinned.sort();
            unpinned
                .into_iter()
                .take(usize::try_from(limit).unwrap_or(usize::MAX))
                .map(|(_, hash)| hash.clone())
                .collect()
        });
        core::future::ready(answer)
    }

    fn forget_blob(&self, hash: &str) -> impl Future<Output = Result<bool, LocalError>> + Send {
        let mut held = self.held();
        let forgettable = held.blobs.get(hash).is_some_and(|row| !row.pinned);
        if forgettable {
            held.blobs.remove(hash);
        }
        core::future::ready(Ok(forgettable))
    }

    /// None: a store held in memory has no database to prune.
    ///
    /// What it holds goes with it when it is dropped. The pruner's arithmetic is
    /// `nc_prune`'s own suite's, and its run over the local file the kernel's,
    /// in `crates/nc-kernel/tests/the_rest_of_the_world.rs`.
    fn pruner(&self) -> Option<nc_prune::sqlite::Sqlite> {
        None
    }
}

/// What a store holds once `unit` has committed over `held`, and what the
/// commit answers — or why it refused, in which case `held` stands.
///
/// **In the order `nc_local::LocalStore::commit` works**, because the order
/// decides which refusal a unit with two problems meets: the bounds one unit
/// may record, then the record's shape, then the unit itself, each write in
/// order, each plugin's quota, each blob, each pin taken off, the notes and
/// the marks.
#[cfg(any(test, feature = "test-support"))]
fn committed(
    held: &Held,
    unit: &UnitRecord,
    quotas: &Quotas,
) -> Result<(Held, Committed), CommitError> {
    within_what_one_unit_may_record(unit, quotas)?;
    shaped(unit)?;
    if held.units.contains_key(&unit.unit) {
        return Err(CommitError::Invalid(format!(
            "unit {} is already committed, and a unit commits once",
            unit.unit.to_crockford()
        )));
    }
    let mut next = held.clone();
    let mut images = Vec::with_capacity(unit.writes.len());
    let mut revisions = Vec::with_capacity(unit.writes.len());
    let mut grew: Vec<&str> = Vec::new();
    // Numbered as the file numbers them, by a `u32`, which the bound one unit
    // may record keeps far from its end.
    for (write, n) in unit.writes.iter().zip(0_u32..) {
        let revision = Revision { unit: unit.unit, n };
        let slot = slot_of(&unit.meta.realm, &write.plugin, &write.key);
        let before = next.entries.get(&slot).cloned();
        if !holds_what_was_expected(before.as_ref(), &write.expect) {
            return Err(CommitError::Conflict {
                plugin: write.plugin.clone(),
                key: write.key.clone(),
            });
        }
        let after = write.after.as_ref().map(|value| Entry {
            value: value.clone(),
            revision,
        });
        match &after {
            Some(entry) => next.entries.insert(slot, entry.clone()),
            None => next.entries.remove(&slot),
        };
        if before.is_none() && after.is_some() && !grew.contains(&write.plugin.as_str()) {
            grew.push(&write.plugin);
        }
        revisions.push(after.as_ref().map(|_| revision));
        images.push(Image {
            plugin: write.plugin.clone(),
            key: write.key.clone(),
            before,
            after,
        });
    }
    for plugin in grew {
        let holds = next
            .entries
            .keys()
            .filter(|(of, owner, _)| of == unit.meta.realm.as_str() && owner == plugin)
            .count();
        if u64::try_from(holds).unwrap_or(u64::MAX) > quotas.entries_for(plugin) {
            return Err(CommitError::Quota {
                plugin: plugin.to_owned(),
            });
        }
    }
    pinned(&mut next, unit, quotas)?;
    images.sort_by(|one, other| (&one.plugin, &one.key).cmp(&(&other.plugin, &other.key)));
    next.units.insert(
        unit.unit,
        UnitDetail {
            row: UnitRow {
                unit: unit.unit,
                meta: unit.meta.clone(),
                state: UnitState::Committed,
            },
            images,
            notes: unit.notes.clone(),
        },
    );
    for (marked, state) in &unit.marks {
        let Some(detail) = next.units.get_mut(marked) else {
            return Err(CommitError::Invalid(format!(
                "unit {} is marked by unit {} and the store does not hold it",
                marked.to_crockford(),
                unit.unit.to_crockford()
            )));
        };
        detail.row.state = *state;
    }
    Ok((next, Committed { revisions }))
}

/// Where a [`MemoryBackend`] holds the key `key` of `plugin` in `realm`.
#[cfg(any(test, feature = "test-support"))]
fn slot_of(realm: &OpaqueId, plugin: &str, key: &str) -> (String, String, String) {
    (realm.as_str().to_owned(), plugin.to_owned(), key.to_owned())
}

/// Pins in `next` the blobs `unit` created, and takes off the pins it names.
///
/// A blob it does not hold is recorded when the bound allows, one it holds is
/// pinned again, as the local database does them.
#[cfg(any(test, feature = "test-support"))]
fn pinned(next: &mut Held, unit: &UnitRecord, quotas: &Quotas) -> Result<(), CommitError> {
    for blob in &unit.blobs {
        if let Some(row) = next.blobs.get_mut(&blob.hash) {
            row.pinned = true;
            row.unpinned_at_ms = None;
            continue;
        }
        if u64::try_from(next.blobs.len()).unwrap_or(u64::MAX) >= quotas.blobs {
            return Err(CommitError::TooManyBlobs);
        }
        as_signed(blob.size, "a blob's size")?;
        next.blobs.insert(
            blob.hash.clone(),
            BlobRow {
                hash: blob.hash.clone(),
                size: blob.size,
                created_unit: unit.unit,
                pinned: true,
                unpinned_at_ms: None,
            },
        );
    }
    for hash in &unit.unpinned {
        let Some(row) = next.blobs.get_mut(hash) else {
            return Err(CommitError::Invalid(format!(
                "blob `{hash}` is unpinned by unit {} and the store does not hold it",
                unit.unit.to_crockford()
            )));
        };
        row.pinned = false;
        row.unpinned_at_ms = Some(unit.meta.committed_at_ms);
    }
    Ok(())
}

/// Refuses a record past what one unit may record, as the local database does
/// before any statement.
#[cfg(any(test, feature = "test-support"))]
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

/// Refuses a record no unit can be, naming what is wrong, as the local database does.
///
/// A key written twice, text that is not canonical, a blob hash that is not one
/// or is recorded or unpinned twice, a time past what the file can hold.
#[cfg(any(test, feature = "test-support"))]
fn shaped(unit: &UnitRecord) -> Result<(), CommitError> {
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
    as_signed(unit.meta.committed_at_ms, "a time")?;
    Ok(())
}

/// Refuses a blob hash that is not one, in the local database's words.
#[cfg(any(test, feature = "test-support"))]
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
#[cfg(any(test, feature = "test-support"))]
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
#[cfg(any(test, feature = "test-support"))]
fn holds_what_was_expected(held: Option<&Entry>, expect: &Expect) -> bool {
    match expect {
        Expect::Anything => true,
        Expect::Absent => held.is_none(),
        Expect::Revision(revision) => held.is_some_and(|entry| entry.revision == *revision),
        Expect::Value(value) => held.map(|entry| &entry.value) == value.as_ref(),
    }
}

/// A number the file holds as a signed integer, refused past one as the file
/// refuses it.
#[cfg(any(test, feature = "test-support"))]
fn as_signed(value: u64, what: &str) -> Result<i64, LocalError> {
    i64::try_from(value).map_err(|_| {
        LocalError::Unexpected(format!("{what} of {value} does not fit a SQLite integer"))
    })
}

#[cfg(test)]
mod tests {
    use super::{MemoryBackend, StoreBackend as _, as_signed, holds_what_was_expected};
    use nc_local::{Entry, Expect, LocalError, Revision};

    /// A number past what the file holds is refused with the file's sentence.
    #[test]
    fn a_number_past_a_signed_integer_is_refused_as_the_file_refuses_it() {
        assert_eq!(as_signed(7, "a time"), Ok(7));
        assert_eq!(
            as_signed(u64::try_from(i64::MAX).expect("fits"), "a time"),
            Ok(i64::MAX)
        );
        assert_eq!(
            as_signed(u64::MAX, "a time"),
            Err(LocalError::Unexpected(format!(
                "a time of {} does not fit a SQLite integer",
                u64::MAX
            )))
        );
    }

    /// Each expectation holds for exactly what it names.
    #[tokio::test]
    async fn an_expectation_holds_only_for_what_it_names() {
        let unit = nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        )
        .mint()
        .expect("a unit");
        let held = Entry {
            value: "1".to_owned(),
            revision: Revision { unit, n: 0 },
        };
        assert!(holds_what_was_expected(None, &Expect::Anything));
        assert!(holds_what_was_expected(Some(&held), &Expect::Anything));
        assert!(holds_what_was_expected(None, &Expect::Absent));
        assert!(!holds_what_was_expected(Some(&held), &Expect::Absent));
        assert!(holds_what_was_expected(
            Some(&held),
            &Expect::Revision(Revision { unit, n: 0 })
        ));
        assert!(!holds_what_was_expected(
            Some(&held),
            &Expect::Revision(Revision { unit, n: 1 })
        ));
        assert!(!holds_what_was_expected(
            None,
            &Expect::Revision(Revision { unit, n: 0 })
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
        // And an empty store holds nothing for anybody.
        assert_eq!(
            MemoryBackend::default()
                .get(&crate::host::HostState::<crate::InMemory>::home(), "p", "k")
                .await,
            Ok(None)
        );
    }
}
