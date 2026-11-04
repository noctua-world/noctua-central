//! The journal's futures cross threads.
//!
//! **This test is about whether it compiles**, and the assertions are only there
//! so that it does something once it has. `Store`'s methods return
//! `impl Future<…> + Send` rather than being a bare `async fn`, and the whole
//! point of that bound is `tokio::spawn` on a multi-threaded runtime — which is
//! where the kernel calls this from, on every command.
//!
//! Written as `async fn` in the trait instead, this file would not build, and the
//! failure would arrive in whichever service first tried to spawn a task that
//! writes a journal entry — a long way from the decision that caused it.

use nc_ids::{MachineFlake, Minter, OpaqueId, SystemClock};
use nc_journal::{InMemory, Journal, Outcome, Record};

fn id(text: &str) -> OpaqueId {
    OpaqueId::new(text).expect("a valid identifier")
}

fn journal() -> Journal<InMemory> {
    Journal::new(
        InMemory::default(),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(1).expect("fits in ten bits"),
            SystemClock,
        ),
    )
}

/// Verifies a journal over **any** store, from a spawned task.
///
/// **Generic on purpose, and that is the whole test.** A concrete
/// `Journal<InMemory>` spawns whether or not the trait promises `Send` — the
/// compiler monomorphises it and sees a `Send` future either way. This function
/// cannot: it knows only `S: Store`, so the future's `Send`-ness has to come
/// from the trait's own bound.
///
/// Written as a bare `async fn` in the trait, this does not compile — measured,
/// by writing it that way and watching `future cannot be sent between threads
/// safely` arrive here. Every service that writes a journal entry from a
/// handler is this function.
async fn verified_anywhere<S>(journal: Journal<S>) -> u64
where
    S: nc_journal::Store + Send + Sync + 'static,
{
    tokio::spawn(async move { journal.verify(0).await })
        .await
        .expect("the task ran")
        .expect("the store is readable")
        .expect("an untouched history is clean")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_journal_verifies_from_a_spawned_task() {
    // An empty journal is clean, and it is clean because the store agrees it is
    // empty rather than because the read returned nothing.
    assert_eq!(verified_anywhere(journal()).await, 0);
}

/// Writes a work unit through a journal over **any** store, from a spawned task.
///
/// **The write path needs its own generic helper.** Dropping `+ Send` from
/// `tail` and `from` breaks `verified_anywhere`, and dropping it from `append`
/// breaks this helper and nothing else in this file, whose other write test
/// spawns a concrete `Journal<InMemory>` — which this file's own doc says
/// proves nothing. The write is the path the kernel takes on every command.
async fn written_anywhere<S>(mut journal: Journal<S>) -> u64
where
    S: nc_journal::Store + Send + Sync + 'static,
{
    tokio::spawn(async move {
        {
            let mut unit = journal.begin("across threads").await.expect("opens");
            unit.record(Record::new(
                id("principal-1"),
                "Read",
                nc_canon::Value::Null,
                Outcome::Allowed,
                1_767_225_600_000,
            ))
            .await
            .expect("records");
            unit.commit().await.expect("commits");
        }
        // **And `head`.** This helper covers `append`, `tail`, `from`,
        // `unit_opened` and the closed-unit question, and without this line
        // nothing in this file would ask for `head`'s `+ Send`, so dropping it
        // from the trait would compile here. `Store` is public and not
        // dyn-compatible, so a kernel generic over `S: Store` that spawns a
        // task and asks where the chain ends is exactly the case this file
        // exists for.
        let (sequence, _) = journal
            .store()
            .head()
            .await
            .expect("the store answers")
            .expect("the chain has a head");
        assert!(sequence > 0, "a written journal has a head past zero");
        journal.verify(0).await.expect("readable").expect("clean")
    })
    .await
    .expect("the task ran")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_journal_writes_through_a_generic_store_from_a_spawned_task() {
    // Opening entry, the record, and the closing bracket.
    assert_eq!(written_anywhere(journal()).await, 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_journal_writes_a_work_unit_from_a_spawned_task() {
    // The whole write path across a thread boundary: opening a unit mints an
    // identifier, every entry chains onto the tail, and committing brackets it.
    let entries = tokio::spawn(async {
        let mut journal = journal();
        {
            let mut unit = journal.begin("across threads").await.expect("opens");
            unit.record(Record::new(
                id("principal-1"),
                "Read",
                nc_canon::Value::Null,
                Outcome::Allowed,
                1_767_225_600_000,
            ))
            .await
            .expect("records");
            unit.commit().await.expect("commits");
        }
        let verdict = journal.verify(0).await.expect("readable");
        (journal.store().len(), verdict)
    })
    .await
    .expect("the task ran");

    // Opening entry, the record, and the closing bracket.
    assert_eq!(entries.0, 3);
    assert_eq!(entries.1, Ok(3));
}
