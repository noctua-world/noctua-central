//! `store`: a plugin's own structured data.
//!
//! Gated by the `store` capability. Every function asks it first and answers
//! `denied` to a plugin not granted it.
//!
//! **Under the plugin's own identity** — the one its manifest named, which the
//! plugin never supplies — so no key of another plugin is reachable from
//! here, and a revision is the text the store wrote it as, which a write names
//! back.
//!
//! **A read sees the unit over what is committed.** Inside a command, the call
//! tree's unit of work holds every write made in it, and `get` and `list` see
//! those writes over the store; outside one — in `activate` — they see only
//! what is committed.
//!
//! **A write is staged, never written.** `put` and `delete` are allowed only in
//! the frame of a command that changes state, and stage their write in the
//! tree's unit (`crate::units`), which the decision point commits whole once
//! the dispatch is done or lets go of; anywhere else — `activate`, a callback,
//! a command that only reads — they are refused with `[NC-PLUG-0024]`. A
//! `put` with no revision creates a key only where none is; a `delete` with
//! none removes whatever is there, and is nothing where nothing is; a
//! revision is compared with the one the key holds now. A key is 1 to
//! `store_key_bytes` bytes, a value canonical JSON of at most
//! `store_value_bytes` — kept as its canonical text — and a page holds 1 to
//! `store_list_max` keys whatever was asked for.
//!
//! A store that does not answer is `unavailable`, and one that holds what it
//! did not write is `internal`, each with the store's own code and sentence: a
//! refusal the plugin reads, never a trap. A store that does not answer at all
//! is a host call past its deadline (`host_call_deadline_ms`), and that one is
//! a trap: the plugin waiting on it is killed rather than left to hold its call
//! tree for ever.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin reaching another plugin's
//! keys, since whose a key is comes from the identity its manifest named and
//! never from the plugin; a write from where nothing may change state —
//! `activate`, a callback, a command that only reads — refused with
//! `[NC-PLUG-0024]`; a write over something that changed since it was read,
//! by the revision it names, compared again when its unit commits; a value
//! that is not canonical JSON; and keys, values, pages and units past the
//! catalog's bounds. What it does not answer: what a plugin writes inside its
//! own keys and its own quota, which is its content and not the kernel's to
//! judge, and what it reads of what it holds itself.

use nc_catalog::{Capability, ErrorCode, KERNEL};
use nc_local::{Entry, Page, Revision};

use super::{HostState, refused};
use crate::Environment;
use crate::backend::StoreBackend as _;
use crate::bindings::KernelError;
use crate::bindings::noctua::kernel::store as world;
use crate::dispatch::{Frame, Refusal};
use crate::supervisor::in_time;
use crate::units::InUnit;
use crate::{RefusalKind, units};

impl<E: Environment> world::Host for HostState<E> {
    async fn get(&mut self, key: String) -> Result<Option<world::Entry>, KernelError> {
        self.require(Capability::Store, "get")?;
        let held = in_time("store#get", self.left(), self.current(&key)).await?;
        Ok(held.map(|entry| world::Entry {
            value: entry.value,
            rev: entry.revision.to_string(),
        }))
    }

    async fn put(
        &mut self,
        key: String,
        value: String,
        expected: Option<String>,
    ) -> Result<String, KernelError> {
        self.require(Capability::Store, "put")?;
        let value = payload(&value).map_err(|refusal| refused(&refusal))?;
        let written = in_time(
            "store#put",
            self.left(),
            self.write("store#put", &key, Some(value), expected.as_deref()),
        )
        .await?;
        Ok(written
            .map(|revision| revision.to_string())
            .unwrap_or_default())
    }

    async fn delete(&mut self, key: String, expected: Option<String>) -> Result<(), KernelError> {
        self.require(Capability::Store, "delete")?;
        in_time(
            "store#delete",
            self.left(),
            self.write("store#delete", &key, None, expected.as_deref()),
        )
        .await
        .map(drop)
    }

    async fn list(
        &mut self,
        prefix: String,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<world::Page, KernelError> {
        self.require(Capability::Store, "list")?;
        let page = in_time(
            "store#list",
            self.left(),
            self.page(&prefix, limit, cursor.as_deref()),
        )
        .await?;
        Ok(world::Page {
            keys: page.keys,
            next_cursor: page.next,
        })
    }
}

impl<E: Environment> HostState<E> {
    /// What `key` holds as this plugin sees it now: its unit's own write, or
    /// what the store holds.
    async fn current(&self, key: &str) -> Result<Option<Entry>, KernelError> {
        bounded("a key", key, 1).map_err(|refusal| refused(&refusal))?;
        match self.staged(key) {
            InUnit::Written(entry) => return Ok(Some(entry)),
            InUnit::Deleted => return Ok(None),
            InUnit::Untouched => {}
        }
        self.store
            .get(&self.realm, &self.plugin, key)
            .await
            .map_err(|failed| refused(&units::store_failed(&failed)))
    }

    /// What the tree's unit staged for `key`, when a command is running in
    /// one and it staged something.
    fn staged(&self, key: &str) -> InUnit {
        let Some(frame) = self.frame() else {
            return InUnit::Untouched;
        };
        let tree = frame.tree();
        tree.unit().map_or(InUnit::Untouched, |unit| {
            unit.overlay().read(unit.unit(), &self.plugin, key)
        })
    }

    /// Stages `after` for `key` in the tree's unit — `None` deletes it — for
    /// `function`, the store's put or delete.
    ///
    /// Only when the key holds what `expected` says, and answering the
    /// revision a put commits with.
    async fn write(
        &self,
        function: &'static str,
        key: &str,
        after: Option<String>,
        expected: Option<&str>,
    ) -> Result<Option<Revision>, KernelError> {
        bounded("a key", key, 1).map_err(|refusal| refused(&refusal))?;
        if !self.frame().is_some_and(Frame::mutating) {
            return Err(self.refused_here(function, &read_only()));
        }
        let (current, committed) = match self.staged(key) {
            InUnit::Written(entry) => (Some(entry), None),
            InUnit::Deleted => (None, None),
            InUnit::Untouched => {
                let committed = self
                    .store
                    .get(&self.realm, &self.plugin, key)
                    .await
                    .map_err(|failed| refused(&units::store_failed(&failed)))?;
                (committed.clone(), committed)
            }
        };
        units::expected(key, current.as_ref(), expected, after.is_some())
            .map_err(|refusal| refused(&refusal))?;
        if after.is_none() && current.is_none() {
            return Ok(None);
        }
        let Some(frame) = self.frame() else {
            return Err(self.refused_here(function, &read_only()));
        };
        let mut tree = frame.tree();
        let Some(unit) = tree.unit_mut() else {
            return Err(self.refused_here(function, &read_only()));
        };
        let id = unit.unit();
        unit.overlay_mut()
            .stage(id, &self.plugin, key, committed.as_ref(), after)
            .map_err(|refusal| refused(&refusal))
    }

    /// A page of this plugin's keys beginning with `prefix`, after `cursor`,
    /// as it sees them now.
    async fn page(
        &self,
        prefix: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<Page, KernelError> {
        bounded("a prefix", prefix, 0).map_err(|refusal| refused(&refusal))?;
        if let Some(cursor) = cursor {
            bounded("a cursor", cursor, 1).map_err(|refusal| refused(&refusal))?;
        }
        let limit = limit.clamp(1, KERNEL.store_list_max);
        let (held, deleted) = self
            .frame()
            .and_then(|frame| {
                let tree = frame.tree();
                tree.unit()
                    .map(|unit| unit.overlay().listed(&self.plugin, prefix, cursor))
            })
            .unwrap_or_default();
        let wider = limit.saturating_add(u32::try_from(deleted.len()).unwrap_or(u32::MAX));
        let committed = self
            .store
            .list(&self.realm, &self.plugin, prefix, wider, cursor)
            .await
            .map_err(|failed| refused(&units::store_failed(&failed)))?;
        Ok(units::merged(committed, held, &deleted, limit))
    }
}

/// The refusal of a write where only reading is allowed.
fn read_only() -> Refusal {
    Refusal::new(
        RefusalKind::Denied,
        ErrorCode::PluginReadOnlyFrame,
        "a write to the store is allowed only in a command that changes state: `activate`, a \
         callback and a command that only reads write nothing themselves — a timer's callback \
         changes state only through the commands it dispatches"
            .to_owned(),
    )
}

/// `text`, named `what`, held to `least` to `store_key_bytes` bytes.
fn bounded(what: &str, text: &str, least: usize) -> Result<(), Refusal> {
    let most = usize::try_from(KERNEL.store_key_bytes).unwrap_or(usize::MAX);
    if (least..=most).contains(&text.len()) {
        Ok(())
    } else {
        Err(Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::ValidationOutOfRange,
            format!(
                "{what} is {least} to {most} bytes, and this one is {}",
                text.len()
            ),
        ))
    }
}

/// `value` as the canonical text it is kept as, when it is canonical JSON of
/// at most `store_value_bytes` bytes.
fn payload(value: &str) -> Result<String, Refusal> {
    if u64::try_from(value.len()).unwrap_or(u64::MAX) > KERNEL.store_value_bytes {
        return Err(Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::ValidationOutOfRange,
            format!(
                "a value is at most {} bytes, and this one is {}",
                KERNEL.store_value_bytes,
                value.len()
            ),
        ));
    }
    nc_canon::parse(value)
        .map(|value| value.to_canonical())
        .map_err(|refusal| {
            Refusal::new(
                RefusalKind::Invalid,
                refusal.code(),
                format!("a value is canonical JSON, and this one is not: {refusal}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::backend::StoreBackend;
    use crate::bindings::KernelError;
    use crate::bindings::noctua::kernel::store::Host as _;
    use crate::bindings::noctua::kernel::types;
    use crate::capability::GrantSet;
    use crate::dispatch::{Call, Frame};
    use crate::host::HostState;
    use crate::host::tests::denied_its_capability;
    use crate::limits::{Limits, MemoryGuard};
    use crate::{Environment, InMemory};
    use nc_catalog::{Capability, ErrorCode, KERNEL};
    use nc_local::{
        Entry, Expect, LocalError, Quotas, Revision, StagedWrite, UnitKind, UnitMeta, UnitRecord,
    };

    /// A store that answers every `get` with one answer, and remembers what it
    /// was asked.
    #[derive(Debug)]
    struct Answering {
        asked: Mutex<Vec<(String, String, String)>>,
        answer: Result<Option<Entry>, LocalError>,
    }

    /// The kernel over an [`Answering`] store.
    #[derive(Debug)]
    struct OverAnswering;

    impl Environment for OverAnswering {
        type Store = Answering;
        type Blobs = crate::blobs::MemoryBlobs;
        type Journal = nc_journal::InMemory;
        type Clock = nc_ids::SystemClock;
        type Directory = nc_authz::MemoryDirectory;
        type Log = crate::logging::Captured;
        type Transport = crate::net::Scripted;
    }

    impl StoreBackend for Answering {
        fn get(
            &self,
            realm: &nc_ids::OpaqueId,
            plugin: &str,
            key: &str,
        ) -> impl Future<Output = Result<Option<Entry>, LocalError>> + Send {
            self.asked.lock().expect("nothing panics holding it").push((
                realm.as_str().to_owned(),
                plugin.to_owned(),
                key.to_owned(),
            ));
            core::future::ready(self.answer.clone())
        }

        fn list(
            &self,
            _: &nc_ids::OpaqueId,
            _: &str,
            _: &str,
            _: u32,
            _: Option<&str>,
        ) -> impl Future<Output = Result<nc_local::Page, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn count(
            &self,
            _: &nc_ids::OpaqueId,
            _: &str,
        ) -> impl Future<Output = Result<u64, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn commit(
            &self,
            _: &nc_local::UnitRecord,
            _: &nc_local::Quotas,
        ) -> impl Future<Output = Result<nc_local::Committed, nc_local::CommitError>> + Send
        {
            core::future::ready(Err(nc_local::CommitError::Invalid("not asked".to_owned())))
        }

        fn commit_held(
            &self,
            _: &nc_local::UnitRecord,
            _: &nc_local::Quotas,
        ) -> impl Future<Output = Result<nc_local::Committed, nc_local::CommitError>> + Send
        {
            core::future::ready(Err(nc_local::CommitError::Invalid("not asked".to_owned())))
        }

        fn release(
            &self,
            _: nc_ids::Snowflake,
        ) -> impl Future<Output = Result<u64, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn holders(
            &self,
        ) -> impl Future<Output = Result<Vec<nc_ids::Snowflake>, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn units(
            &self,
            _: &nc_ids::OpaqueId,
            _: &nc_ids::OpaqueId,
            _: u32,
        ) -> impl Future<Output = Result<Vec<nc_local::UnitRow>, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn steps(
            &self,
            _: &nc_ids::OpaqueId,
            _: &nc_ids::OpaqueId,
            _: u32,
        ) -> impl Future<Output = Result<Vec<nc_local::UnitRow>, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn unit(
            &self,
            _: nc_ids::Snowflake,
        ) -> impl Future<Output = Result<Option<nc_local::UnitDetail>, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn set_state(
            &self,
            _: nc_ids::Snowflake,
            _: nc_local::UnitState,
        ) -> impl Future<Output = Result<bool, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn forget_beyond(
            &self,
            _: &nc_ids::OpaqueId,
            _: &nc_ids::OpaqueId,
            _: u32,
        ) -> impl Future<Output = Result<u64, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn noted_elsewhere(
            &self,
            _: &str,
            _: nc_ids::Snowflake,
        ) -> impl Future<Output = Result<bool, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn blob(
            &self,
            _: &str,
        ) -> impl Future<Output = Result<Option<nc_local::BlobRow>, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn set_pinned(
            &self,
            _: &str,
            _: bool,
            _: u64,
        ) -> impl Future<Output = Result<bool, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn unpinned_before(
            &self,
            _: u64,
            _: u32,
        ) -> impl Future<Output = Result<Vec<String>, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn forget_blob(&self, _: &str) -> impl Future<Output = Result<bool, LocalError>> + Send {
            core::future::ready(Err(LocalError::Unavailable("not asked".to_owned())))
        }

        fn pruner(&self) -> Option<nc_prune::sqlite::Sqlite> {
            None
        }
    }

    /// The plugin `noctua:notes`, granted the store, over a store answering
    /// with what `answer` says.
    fn over(answer: Result<Option<Entry>, LocalError>) -> HostState<OverAnswering> {
        HostState::new(
            GrantSet::of([Capability::Store]),
            (
                HostState::<crate::InMemory>::home(),
                "noctua:notes".to_owned(),
            ),
            Vec::new(),
            Arc::new(Answering {
                asked: Mutex::new(Vec::new()),
                answer,
            }),
            Arc::new(crate::blobs::MemoryBlobs::default()),
            Arc::new(crate::net::Scripted::default()),
            MemoryGuard::new(&Limits::catalog()),
            crate::host::log::SharedRing::default(),
        )
    }

    /// A unit, minted for a test.
    fn a_unit() -> nc_ids::Snowflake {
        nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        )
        .mint()
        .expect("a unit")
    }

    /// The plugin `tests:door`, granted the store, running a command that
    /// changes state in the unit `unit`.
    fn writing(unit: nc_ids::Snowflake) -> HostState<InMemory> {
        let mut state = HostState::in_memory(GrantSet::of([Capability::Store]));
        state.enter(Frame::for_tests(true, Some(unit)));
        state
    }

    /// The code a refusal of the world carries, and its case's word.
    fn refusal(answer: &Result<impl core::fmt::Debug, KernelError>) -> (String, String) {
        match answer {
            Err(KernelError::Refused(error)) => {
                let (case, why) = match error {
                    types::Error::Denied(why) => ("denied", why),
                    types::Error::NotFound(why) => ("not-found", why),
                    types::Error::Invalid(why) => ("invalid", why),
                    types::Error::Conflict(why) => ("conflict", why),
                    types::Error::Unavailable(why) => ("unavailable", why),
                    types::Error::Internal(why) => ("internal", why),
                };
                (case.to_owned(), why.clone())
            }
            other => panic!("not a refusal: {other:?}"),
        }
    }

    /// Whether `why` opens on `code`.
    fn opens_on(why: &str, code: ErrorCode) -> bool {
        why.starts_with(&format!("[{code}] "))
    }

    #[tokio::test]
    async fn every_function_is_denied_without_the_capability_and_a_write_outside_a_command_is_refused()
     {
        for granted in [false, true] {
            let mut state = HostState::in_memory(if granted {
                GrantSet::of([Capability::Store])
            } else {
                GrantSet::none()
            });
            let read = state.get("k".to_owned()).await;
            let listed = state.list(String::new(), 1, None).await;
            let writes = [
                state
                    .put("k".to_owned(), "{}".to_owned(), None)
                    .await
                    .map(drop),
                state.delete("k".to_owned(), None).await,
            ];
            if granted {
                assert!(matches!(read, Ok(None)), "an empty store: {read:?}");
                assert!(
                    listed.as_ref().is_ok_and(|page| page.keys.is_empty()),
                    "{listed:?}"
                );
                for written in &writes {
                    let (case, why) = refusal(written);
                    assert_eq!(case, "denied");
                    assert!(opens_on(&why, ErrorCode::PluginReadOnlyFrame), "{why}");
                }
            } else {
                assert!(denied_its_capability(&read.map(drop)));
                assert!(denied_its_capability(&listed.map(drop)));
                for written in &writes {
                    assert!(denied_its_capability(written), "{written:?}");
                }
            }
            assert_eq!(state.reached().len(), 4);
        }
    }

    #[tokio::test]
    async fn a_write_in_a_command_that_only_reads_is_refused_and_stages_nothing() {
        let unit = a_unit();
        let mut state = HostState::in_memory(GrantSet::of([Capability::Store]));
        state.enter(Frame::for_tests(false, Some(unit)));
        let (case, why) = refusal(&state.put("k".to_owned(), "1".to_owned(), None).await);
        assert_eq!(case, "denied");
        assert!(opens_on(&why, ErrorCode::PluginReadOnlyFrame), "{why}");
        let frame = state.frame().expect("the frame is held");
        let tree = frame.tree();
        assert_eq!(
            tree.unit().map(|unit| unit.overlay().changes()),
            Some(0),
            "a refused write staged something"
        );
    }

    #[tokio::test]
    async fn a_timers_callback_that_began_a_unit_writes_nothing_itself_and_is_told_why() {
        // **A review found the old sentence false**: it said a callback runs
        // in no unit of work, and a timer's callback may begin one.
        let mut state = HostState::in_memory(GrantSet::of([Capability::Store]));
        state.enter(
            Frame::for_tests(false, Some(a_unit()))
                .calling_for_tests(Call::Timer { may_change: true }),
        );
        let (case, why) = refusal(&state.put("k".to_owned(), "1".to_owned(), None).await);
        assert_eq!(case, "denied");
        assert!(opens_on(&why, ErrorCode::PluginReadOnlyFrame), "{why}");
        assert!(
            why.ends_with(
                "write nothing themselves — a timer's callback changes state only through the \
                 commands it dispatches"
            ),
            "{why}"
        );
        assert_eq!(
            state
                .frame()
                .expect("the frame is held")
                .tree()
                .unit()
                .map(|unit| unit.overlay().changes()),
            Some(0),
            "a refused write staged something"
        );
    }

    #[tokio::test]
    async fn a_write_is_staged_in_the_unit_and_read_back_before_anything_is_committed() {
        let unit = a_unit();
        let mut state = writing(unit);
        let revision = state
            .put(
                "note/a".to_owned(),
                r#"{ "b": 1, "a": 2 }"#.to_owned(),
                None,
            )
            .await
            .expect("a key created");
        assert_eq!(revision, Revision { unit, n: 0 }.to_string());
        let held = state
            .get("note/a".to_owned())
            .await
            .expect("read")
            .expect("staged");
        assert_eq!(held.value, r#"{"a":2,"b":1}"#, "kept as its canonical text");
        assert_eq!(held.rev, revision);
        assert_eq!(
            state
                .store
                .get(
                    &HostState::<crate::InMemory>::home(),
                    "tests:door",
                    "note/a"
                )
                .await,
            Ok(None),
            "a staged write reached the store before the unit was kept"
        );
        let again = state
            .put("note/a".to_owned(), "3".to_owned(), Some(revision.clone()))
            .await
            .expect("over the revision it holds");
        assert_eq!(
            again, revision,
            "a key keeps its place among the unit's writes"
        );
        state
            .delete("note/a".to_owned(), None)
            .await
            .expect("deleted");
        assert!(
            state
                .get("note/a".to_owned())
                .await
                .expect("read")
                .is_none(),
            "a deleted key reads as nothing"
        );
        state
            .delete("note/never".to_owned(), None)
            .await
            .expect("nothing deleted where nothing is");
        let frame = state.frame().expect("the frame is held");
        let tree = frame.tree();
        let writes = tree.unit().expect("a unit").overlay().writes();
        assert_eq!(
            writes.len(),
            1,
            "a key never held stages no delete: {writes:?}"
        );
    }

    #[tokio::test]
    async fn a_write_lands_only_over_what_it_names() {
        let unit = a_unit();
        let mut state = writing(unit);
        let first = state
            .put("k".to_owned(), "1".to_owned(), None)
            .await
            .expect("created");
        let (case, why) = refusal(&state.put("k".to_owned(), "2".to_owned(), None).await);
        assert_eq!(case, "conflict", "a write naming no revision over a key");
        assert!(opens_on(&why, ErrorCode::PluginStoreConflict), "{why}");
        let another = Revision { unit, n: 9 }.to_string();
        let (case, _) = refusal(&state.delete("k".to_owned(), Some(another)).await);
        assert_eq!(case, "conflict", "a delete naming another revision");
        let (case, why) = refusal(
            &state
                .put("k".to_owned(), "2".to_owned(), Some("a guess".to_owned()))
                .await,
        );
        assert_eq!(case, "invalid", "text that is not a revision");
        assert!(opens_on(&why, ErrorCode::PluginNotARevision), "{why}");
        assert!(
            state.delete("k".to_owned(), Some(first)).await.is_ok(),
            "a delete naming the revision the key holds"
        );
    }

    #[tokio::test]
    async fn a_key_and_a_value_are_held_to_their_bounds() {
        let mut state = writing(a_unit());
        let longest = usize::try_from(KERNEL.store_key_bytes).expect("fits");
        assert!(
            state
                .put("k".repeat(longest), "1".to_owned(), None)
                .await
                .is_ok(),
            "a key of exactly the bound"
        );
        for key in [String::new(), "k".repeat(longest + 1)] {
            let (case, why) = refusal(&state.put(key.clone(), "1".to_owned(), None).await);
            assert_eq!(case, "invalid", "a key of {} bytes", key.len());
            assert!(opens_on(&why, ErrorCode::ValidationOutOfRange), "{why}");
            let (case, _) = refusal(&state.get(key).await);
            assert_eq!(case, "invalid");
        }
        let (case, why) = refusal(&state.put("v".to_owned(), "1.5".to_owned(), None).await);
        assert_eq!(case, "invalid");
        assert!(opens_on(&why, ErrorCode::CanonNumber), "{why}");
        let past = format!(
            "\"{}\"",
            "v".repeat(usize::try_from(KERNEL.store_value_bytes).expect("fits"))
        );
        let (case, why) = refusal(&state.put("v".to_owned(), past, None).await);
        assert_eq!(case, "invalid");
        assert!(opens_on(&why, ErrorCode::ValidationOutOfRange), "{why}");
        let (case, _) = refusal(&state.list("p".repeat(longest + 1), 1, None).await);
        assert_eq!(case, "invalid", "a prefix past the bound");
        let (case, _) = refusal(&state.list(String::new(), 1, Some(String::new())).await);
        assert_eq!(case, "invalid", "an empty cursor names no key");
    }

    /// One unit holds no more of a plugin's values than `unit_bytes`, however
    /// many keys it may write: each value at its own bound, the writes past the
    /// unit's are refused with the bound's code.
    ///
    /// A verifier staged 80 MiB this way in one unit, past the 64 MiB the
    /// plugin's own memory may grow to, when the only bound was the count of
    /// keys.
    #[tokio::test]
    async fn a_unit_holds_no_more_bytes_than_the_catalog_allows_whatever_its_values() {
        let mut state = writing(a_unit());
        let value = format!(
            "\"{}\"",
            "v".repeat(usize::try_from(KERNEL.store_value_bytes - 2).expect("fits"))
        );
        let one = u64::try_from("k000".len() + value.len()).expect("fits");
        let fits = KERNEL.unit_bytes / one;
        let mut landed = 0_u64;
        let refused = loop {
            let put = state
                .put(format!("k{landed:03}"), value.clone(), None)
                .await;
            if put.is_err() {
                break refusal(&put);
            }
            landed += 1;
            assert!(landed <= fits, "a write past the unit's bytes was staged");
        };
        assert_eq!(
            landed, fits,
            "every write within the unit's bytes is staged"
        );
        assert!(landed * one <= KERNEL.memory_bytes);
        assert_eq!(refused.0, "unavailable");
        assert!(
            opens_on(&refused.1, ErrorCode::PluginBoundReached),
            "{}",
            refused.1
        );
    }

    #[tokio::test]
    async fn a_page_shows_the_units_writes_over_what_is_committed() {
        let unit = a_unit();
        let mut state = writing(unit);
        for key in ["a/1", "a/2", "b/1"] {
            state
                .put(key.to_owned(), "1".to_owned(), None)
                .await
                .expect("staged");
        }
        let page = state.list("a/".to_owned(), 0, None).await.expect("listed");
        assert_eq!(page.keys, ["a/1"], "a limit of zero is a page of one");
        assert_eq!(page.next_cursor.as_deref(), Some("a/1"));
        let rest = state
            .list("a/".to_owned(), u32::MAX, page.next_cursor)
            .await
            .expect("listed");
        assert_eq!(rest.keys, ["a/2"]);
        assert_eq!(rest.next_cursor, None);
    }

    /// A page reads the store past the keys the unit deleted, so a plugin is
    /// never told its listing has ended while keys it was not shown remain.
    ///
    /// A verifier found the widening of the committed read held by nothing:
    /// without it, a page of one over a store holding `a`, `b` and `c`, with `a`
    /// deleted in the unit, came back empty and as the last.
    #[tokio::test]
    async fn a_page_reads_past_the_keys_the_unit_deleted() {
        let mut minter = nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(2).expect("a flake"),
            nc_ids::SystemClock,
        );
        let seed = minter.mint().expect("a unit");
        let mut state = writing(minter.mint().expect("a unit"));
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let world = nc_authz::MemoryDirectory::from_world(
            include_str!("../../../../policy/world.json"),
            &pdp,
        )
        .expect("the world loads");
        let principal = world
            .principal(nc_authz::PrincipalKind::User, "ana")
            .expect("ana is a user of the world");
        let record = UnitRecord {
            unit: seed,
            meta: UnitMeta {
                realm: world.realm_id("home").expect("home is a realm"),
                principal: principal.id().clone(),
                kind: UnitKind::Step,
                label: "seeding the test".to_owned(),
                committed_at_ms: 1,
            },
            writes: ["a", "b", "c"]
                .map(|key| StagedWrite {
                    plugin: "tests:door".to_owned(),
                    key: key.to_owned(),
                    expect: Expect::Anything,
                    after: Some("1".to_owned()),
                })
                .to_vec(),
            blobs: Vec::new(),
            notes: Vec::new(),
            unpinned: Vec::new(),
            marks: Vec::new(),
        };
        let quotas = Quotas {
            entries_per_plugin: 10,
            lowered: std::collections::BTreeMap::new(),
            blobs: 1,
            writes_per_unit: 10,
            notes_per_unit: 1,
        };
        state
            .store
            .commit(&record, &quotas)
            .await
            .expect("the seed commits");
        state
            .delete("a".to_owned(), None)
            .await
            .expect("deleted in the unit");
        let page = state.list(String::new(), 1, None).await.expect("listed");
        assert_eq!(page.keys, ["b"], "a page of one, read past the deleted key");
        assert_eq!(page.next_cursor.as_deref(), Some("b"), "and more follow");
        let rest = state
            .list(String::new(), 1, page.next_cursor)
            .await
            .expect("listed");
        assert_eq!(rest.keys, ["c"]);
        assert_eq!(rest.next_cursor, None);
    }

    /// A page holds no more than `store_list_max` keys, whatever it was asked
    /// to hold.
    #[tokio::test]
    async fn a_page_holds_no_more_keys_than_the_catalog_allows() {
        let mut state = writing(a_unit());
        let most = KERNEL.store_list_max;
        for at in 0..=most {
            state
                .put(format!("k{at:05}"), "1".to_owned(), None)
                .await
                .expect("staged");
        }
        let page = state
            .list(String::new(), u32::MAX, None)
            .await
            .expect("listed");
        assert_eq!(
            page.keys.len(),
            usize::try_from(most).expect("fits"),
            "a page past the bound"
        );
        assert!(page.next_cursor.is_some(), "and more follow");
    }

    /// A read asks the store for the plugin's own key, and hands back its
    /// value and its revision as the text a write names.
    #[tokio::test]
    async fn a_read_asks_for_the_plugins_own_key_and_answers_its_value_and_revision() {
        let unit = a_unit();
        let mut state = over(Ok(Some(Entry {
            value: r#"{"a":1}"#.to_owned(),
            revision: Revision { unit, n: 3 },
        })));
        let got = state
            .get("note/1".to_owned())
            .await
            .expect("the store answered")
            .expect("the key is held");
        assert_eq!(got.value, r#"{"a":1}"#);
        assert_eq!(got.rev, format!("{}.3", unit.to_crockford()));
        assert_eq!(
            *state.store.asked.lock().expect("nothing panics holding it"),
            [(
                HostState::<crate::InMemory>::home().as_str().to_owned(),
                "noctua:notes".to_owned(),
                "note/1".to_owned()
            )],
            "the realm and the plugin are the ones the kernel bound, and the key the one it \
             named"
        );
    }

    /// A store that did not answer is `unavailable`, and one holding what it
    /// did not write is `internal`, each with the store's own code.
    #[tokio::test]
    async fn a_store_that_fails_is_a_refusal_the_plugin_reads_with_the_stores_code() {
        for (failed, unavailable) in [
            (LocalError::Unavailable("busy".to_owned()), true),
            (LocalError::Unexpected("a row".to_owned()), false),
            (LocalError::Refused("a mode".to_owned()), false),
            (LocalError::Migration("a step".to_owned()), false),
        ] {
            let code = failed.code();
            let answered = over(Err(failed)).get("k".to_owned()).await;
            let (case, why) = refusal(&answered);
            assert_eq!(
                case,
                if unavailable {
                    "unavailable"
                } else {
                    "internal"
                }
            );
            assert!(opens_on(&why, code), "{why}");
            assert!(
                !why[code.code().len() + 3..].starts_with('['),
                "the code is written once: {why}"
            );
        }
    }
}
