//! What the daemon runs on a clock (`NOCTUA.md §11.1`, `§15.8`): the pump
//! every `pump_every_ms` — the timers due, the events queued, and the pruner
//! over the local database — and the blob sweep every `sweep_every_ms`.
//!
//! **Between call trees**: each takes the bus every top-level dispatch takes
//! (`Kernel::pump`, `Kernel::sweep_blobs`), so neither runs inside one. The
//! first of each runs as soon as the daemon is running, and each runs to its
//! end before the next is waited for, so a slow one is never run twice at
//! once. The bounds are the catalog's, read here, and no caller chooses them.
//!
//! **A stop is asked between runs, never inside one.** A pump's timer
//! callbacks and event deliveries are call trees of their own, so a stop that
//! cut one short would close the local database under a callback that still
//! writes to it. The schedule asks whether a stop was asked for only while it
//! waits for the next run, begins none once one was, and the stop waits for
//! the run it found as it waits for a dispatch — `stop_drain_ms` at most, past
//! which the run is given up on and counted as one ([`ended`]).
//!
//! **A table the pruner skips on every pump is escalated.** The pruner leaves
//! a table it waited too long for alone and says so, and the next pump is a
//! better answer than ending this one — but a wait that never ends is a
//! refusal wearing a wait's clothes, and only the run of pumps that met it
//! tells the two apart (`nc_prune::Failure::waited`). So a run of pumps whose
//! pruning was refused is counted, and logged as `daemon.prune_stuck` at two
//! and at each doubling after — two, four, eight — so a stuck table keeps
//! coming back to the log without filling it; a pump that prunes cleanly ends
//! the run.

use std::sync::{Arc, Mutex, PoisonError};

use core::time::Duration;

use nc_catalog::ErrorCode;
use nc_ids::Clock as _;
use nc_kernel::logging::LogSink;

/// A run of pumps whose pruning was refused, and the code of the last.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Streak {
    /// How many pumps in a row were refused.
    pub pumps: u32,
    /// The code the last of them was refused with.
    pub code: Option<ErrorCode>,
}

impl Streak {
    /// Notes one pump's pruning: `refused` with the code it was refused with,
    /// or nothing for a pump that pruned cleanly, which ends the run.
    ///
    /// Answers the code and the length of the run when it is to be logged: at
    /// two pumps, and at each doubling after.
    pub fn noted(&mut self, refused: Option<ErrorCode>) -> Option<(ErrorCode, u32)> {
        let Some(code) = refused else {
            *self = Self::default();
            return None;
        };
        self.pumps = self.pumps.saturating_add(1);
        self.code = Some(code);
        (self.pumps >= 2 && self.pumps.is_power_of_two()).then_some((code, self.pumps))
    }
}

/// What the schedule has done, as the status shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Done {
    /// How many pumps ran.
    pub pumps: u64,
    /// When the last pump ended, in milliseconds since the Unix epoch.
    pub last_pump: Option<u64>,
    /// The run of pumps whose pruning was refused.
    pub prune: Streak,
    /// How many sweeps ran.
    pub sweeps: u64,
    /// When the last sweep ended, in milliseconds since the Unix epoch.
    pub last_sweep: Option<u64>,
    /// The code the last sweep was refused with, when it was.
    pub sweep_refused: Option<ErrorCode>,
}

/// How far apart pumps are: `pump_every_ms`.
#[must_use]
pub const fn pump_every() -> Duration {
    Duration::from_millis(nc_catalog::DAEMON.pump_every_ms)
}

/// How far apart sweeps are: `sweep_every_ms`.
#[must_use]
pub const fn sweep_every() -> Duration {
    Duration::from_millis(nc_catalog::DAEMON.sweep_every_ms)
}

/// Notes a pump that ended at `at` with `refused` as its pruning's verdict,
/// and logs a run that is to be logged.
pub fn pumped(done: &Mutex<Done>, at: u64, refused: Option<ErrorCode>, log: &impl LogSink) {
    let stuck = {
        let mut done = done.lock().unwrap_or_else(PoisonError::into_inner);
        done.pumps += 1;
        done.last_pump = Some(at);
        done.prune.noted(refused)
    };
    if let Some((code, pumps)) = stuck {
        log.record(&nc_log::events::DaemonPruneStuck { code, pumps });
    }
}

/// Notes a sweep that ended at `at`, refused with `refused` or not.
pub fn swept(done: &Mutex<Done>, at: u64, refused: Option<ErrorCode>) {
    let mut done = done.lock().unwrap_or_else(PoisonError::into_inner);
    done.sweeps += 1;
    done.last_sweep = Some(at);
    done.sweep_refused = refused;
}

/// Runs `once` every `period`, the first at once, until `stopped` ends.
///
/// **`stopped` is asked only while the next run is waited for**, and before
/// a tick that came due with it, so a run that began is run to its end and
/// none begins once a stop was asked for.
pub async fn on_schedule<Run: Future<Output = ()>>(
    period: Duration,
    stopped: impl Future<Output = ()>,
    mut once: impl FnMut() -> Run,
) {
    let mut every = tokio::time::interval(period);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut stopped = core::pin::pin!(stopped);
    loop {
        tokio::select! {
            biased;
            () = &mut stopped => return,
            _ = every.tick() => {}
        }
        once().await;
    }
}

/// Pumps `kernel` every `pump_every_ms`, the first at once, until `stopped`
/// ends, never inside a pump.
pub async fn pumping(
    kernel: Arc<nc_kernel::Kernel<crate::environment::Daemon>>,
    done: Arc<Mutex<Done>>,
    log: crate::environment::Sink,
    stopped: impl Future<Output = ()>,
) {
    let (kernel, done, log) = (&kernel, &done, &log);
    on_schedule(pump_every(), stopped, move || async move {
        let pumped_now = kernel.pump().await;
        pumped(
            done,
            nc_ids::SystemClock.now_ms(),
            pumped_now.prune_refused,
            log,
        );
    })
    .await;
}

/// Sweeps `kernel`'s blobs every `sweep_every_ms`, the first at once, until
/// `stopped` ends, never inside a sweep.
pub async fn sweeping(
    kernel: Arc<nc_kernel::Kernel<crate::environment::Daemon>>,
    done: Arc<Mutex<Done>>,
    stopped: impl Future<Output = ()>,
) {
    let (kernel, done) = (&kernel, &done);
    on_schedule(sweep_every(), stopped, move || async move {
        let refused = kernel
            .sweep_blobs()
            .await
            .err()
            .map(|refusal| refusal.code());
        swept(done, nc_ids::SystemClock.now_ms(), refused);
    })
    .await;
}

/// Waits for the schedule's task `running` to end, until `deadline` at most,
/// and answers how many runs it gave up on: none when it ended by then, and
/// the one it is inside when it did not, which is let go.
pub async fn ended(
    mut running: tokio::task::JoinHandle<()>,
    deadline: tokio::time::Instant,
) -> u32 {
    if tokio::time::timeout_at(deadline, &mut running)
        .await
        .is_ok()
    {
        return 0;
    }
    running.abort();
    1
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use nc_catalog::ErrorCode;

    use super::{Done, Streak};

    /// A run of refused pumps is logged at two and at each doubling, and a
    /// clean pump ends it.
    #[test]
    fn a_run_of_refused_pumps_is_logged_at_two_and_each_doubling() {
        let mut streak = Streak::default();
        let refused = Some(ErrorCode::DbPruneFailed);
        let logged: Vec<Option<u32>> = (0..9)
            .map(|_| streak.noted(refused).map(|(_, pumps)| pumps))
            .collect();
        assert_eq!(
            logged,
            [
                None,
                Some(2),
                None,
                Some(4),
                None,
                None,
                None,
                Some(8),
                None
            ]
        );
        assert_eq!(streak.pumps, 9);
        assert_eq!(streak.code, refused);
        assert_eq!(streak.noted(None), None);
        assert_eq!(
            streak,
            Streak::default(),
            "a clean pump did not end the run"
        );
        assert_eq!(
            streak.noted(refused),
            None,
            "one refusal is a wait, not a run"
        );
    }

    /// The code logged is the last pump's.
    #[test]
    fn the_code_logged_is_the_last_pumps() {
        let mut streak = Streak::default();
        assert_eq!(streak.noted(Some(ErrorCode::DbPruneFailed)), None);
        assert_eq!(
            streak.noted(Some(ErrorCode::DbUnreachable)),
            Some((ErrorCode::DbUnreachable, 2))
        );
    }

    /// A pump is counted and timed, and a stuck run reaches the log as
    /// `daemon.prune_stuck` with its code and its length.
    #[test]
    fn a_pump_is_counted_and_a_stuck_run_is_logged() {
        let done = Mutex::new(Done::default());
        let log = nc_kernel::logging::Captured::default();
        super::pumped(&done, 10, Some(ErrorCode::DbPruneFailed), &log);
        super::pumped(&done, 20, Some(ErrorCode::DbPruneFailed), &log);
        let done = done.into_inner().expect("held");
        assert_eq!(done.pumps, 2);
        assert_eq!(done.last_pump, Some(20));
        let lines = log.named("daemon.prune_stuck");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].text("pumps").as_deref(), Some("2"));
        assert_eq!(
            lines[0].text("code").as_deref(),
            Some(ErrorCode::DbPruneFailed.code())
        );
    }

    /// A sweep is counted and timed with its refusal, and a later one that is
    /// not refused clears it.
    #[test]
    fn a_sweep_is_counted_with_its_refusal() {
        let done = Mutex::new(Done::default());
        super::swept(&done, 10, Some(ErrorCode::BlobIo));
        assert_eq!(
            done.lock().expect("held").sweep_refused,
            Some(ErrorCode::BlobIo)
        );
        super::swept(&done, 20, None);
        let done = done.into_inner().expect("held");
        assert_eq!(done.sweeps, 2);
        assert_eq!(done.last_sweep, Some(20));
        assert_eq!(done.sweep_refused, None);
    }

    /// A stop that arrives inside a run waits for it, and no run begins
    /// once it was asked for, though a tick came due meanwhile.
    ///
    /// **Thirty-two schedules, each with a tick due when its stop is**, so a
    /// select that takes either of the two when both are ready — the tick
    /// half the time — escapes all of them one time in 2^32. **Found by a
    /// review**: with one schedule, that select passed three runs in four.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_stop_waits_for_the_run_it_finds_and_begins_none_after() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};

        use core::time::Duration;

        for schedule in 0..32 {
            let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
            let (began, mut beginning) = tokio::sync::mpsc::unbounded_channel::<()>();
            let release = Arc::new(tokio::sync::Notify::new());
            let ran = Arc::new(AtomicU32::new(0));
            let (releasing, counting) = (Arc::clone(&release), Arc::clone(&ran));
            let mut task = tokio::spawn(async move {
                super::on_schedule(
                    Duration::from_millis(1),
                    async {
                        let _ = stopped.await;
                    },
                    move || {
                        let (began, release, ran) =
                            (began.clone(), Arc::clone(&releasing), Arc::clone(&counting));
                        async move {
                            let _ = began.send(());
                            release.notified().await;
                            ran.fetch_add(1, Ordering::SeqCst);
                        }
                    },
                )
                .await;
            });
            tokio::time::timeout(Duration::from_secs(30), beginning.recv())
                .await
                .expect("the first run began at once")
                .expect("a run began");
            stop.send(()).expect("the schedule is waiting for a stop");
            // Longer than the schedule's period, so its next tick is due
            // when the run ends.
            assert!(
                tokio::time::timeout(Duration::from_millis(20), &mut task)
                    .await
                    .is_err(),
                "schedule {schedule} ended inside the run a stop found"
            );
            release.notify_one();
            let ended = tokio::time::timeout(Duration::from_secs(30), async {
                tokio::select! {
                    ended = &mut task => ended.is_ok(),
                    // A run begun says so; a schedule that ended closes the
                    // channel, which is no run.
                    Some(()) = beginning.recv() => false,
                }
            })
            .await
            .expect("the schedule ended once its run did");
            task.abort();
            assert!(
                ended,
                "schedule {schedule} began a run after the stop was asked for"
            );
            assert_eq!(
                ran.load(Ordering::SeqCst),
                1,
                "schedule {schedule} ran a run after the stop was asked for"
            );
        }
    }

    /// A stop waits for the schedule's task until its bound, and gives up on
    /// the run it is still inside then, counting it and letting it go.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_run_still_going_at_the_bound_is_given_up_on_and_counted() {
        use core::time::Duration;

        let soon = || tokio::time::Instant::now() + Duration::from_secs(30);
        let finished = tokio::spawn(async {});
        assert_eq!(super::ended(finished, soon()).await, 0);
        let going = tokio::spawn(std::future::pending::<()>());
        let letting_go = going.abort_handle();
        let bound = tokio::time::Instant::now() + Duration::from_millis(50);
        assert_eq!(super::ended(going, bound).await, 1);
        let deadline = soon();
        while !letting_go.is_finished() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "a run given up on was not let go"
            );
            tokio::task::yield_now().await;
        }
    }

    /// The bounds the schedule keeps are the catalog's.
    #[test]
    fn the_schedule_keeps_the_catalogs_bounds() {
        assert_eq!(
            super::pump_every().as_millis(),
            u128::from(nc_catalog::DAEMON.pump_every_ms)
        );
        assert_eq!(
            super::sweep_every().as_millis(),
            u128::from(nc_catalog::DAEMON.sweep_every_ms)
        );
    }
}
