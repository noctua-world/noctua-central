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

/// Pumps `kernel` every `pump_every_ms`, the first at once, for as long as it
/// is not stopped.
pub async fn pumping(
    kernel: Arc<nc_kernel::Kernel<crate::environment::Daemon>>,
    done: Arc<Mutex<Done>>,
    log: crate::environment::Sink,
) {
    let mut every = tokio::time::interval(pump_every());
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        every.tick().await;
        let pumped_now = kernel.pump().await;
        pumped(
            &done,
            nc_ids::SystemClock.now_ms(),
            pumped_now.prune_refused,
            &log,
        );
    }
}

/// Sweeps `kernel`'s blobs every `sweep_every_ms`, the first at once, for as
/// long as it is not stopped.
pub async fn sweeping(
    kernel: Arc<nc_kernel::Kernel<crate::environment::Daemon>>,
    done: Arc<Mutex<Done>>,
) {
    let mut every = tokio::time::interval(sweep_every());
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        every.tick().await;
        let refused = kernel
            .sweep_blobs()
            .await
            .err()
            .map(|refusal| refusal.code());
        swept(&done, nc_ids::SystemClock.now_ms(), refused);
    }
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
