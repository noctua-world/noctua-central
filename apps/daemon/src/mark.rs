//! The identifiers' mark: the highest millisecond this profile's minter may
//! hand out, made durable before it is reached (`NOCTUA.md §16.2`).
//!
//! **A ceiling, raised a lease at a time.** The minter asks the mark to raise
//! its ceiling to `mark_lease_ms` past the clock before it hands out a
//! millisecond past the ceiling it holds (`nc_ids::Ceiling`), and hands it out
//! only once the write is synced — so a daemon stopped at any instant has
//! minted nothing above what the mark gives its next start, and under load it
//! writes once a lease, never once an identifier. A clean stop writes the
//! exact high-water mark instead, so a clean restart has nothing to wait for.
//!
//! **Two slots, overwritten in place, and never a file renamed over.** A
//! rename is not durable on NTFS without a flag the standard library does not
//! pass, and an overwrite of one slot is. A write goes to the slot that does
//! not hold the newest ceiling, is synced with `sync_all` — `F_FULLFSYNC` on
//! macOS — and only then answers, so a write torn by a crash spoils one slot
//! and the other still holds the ceiling the minter held before it, which
//! nothing was minted past. **Each slot carries a checksum**, sixteen bytes of
//! BLAKE3 over its generation and its ceiling under a domain of its own; the
//! newest whole slot wins, a slot that does not check is passed over, and a
//! file with neither refuses the start (`NC-DMN-0007`) rather than resume from
//! nothing, which is the one way to repeat an identifier. A mark that is not
//! there is a profile's first start, and is made private, with its directory
//! synced so the name survives a crash too.
//!
//! **A start resumes above everything handed out, and waits for the clock to
//! pass it**: the larger of the mark and the newest unit the start hands in,
//! which is the local journal's to answer, so a mark lost or rolled back is
//! still above every unit kept. A clock behind it is waited out — further
//! behind than the lease is a clock that moved back, logged as
//! `ids.clock_backwards` — and a mark more than a day ahead of the clock is a
//! wrong number, and the start refuses (`MintError::MarkOutOfRange`).
//!
//! **The lease is the catalog's**, `nc_catalog::DAEMON.mark_lease_ms`, read
//! when the mark is opened: no caller hands one in, so the bound the compiler
//! holds below a day is the one every raise writes by.

use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

use core::time::Duration;

use nc_catalog::ErrorCode;
use nc_ids::{
    Ceiling, CeilingRefused, Clock, EPOCH_MS, MachineFlake, MintError, Minter, Snowflake,
};
use nc_kernel::logging::LogSink;
use nc_platform::{Judged, Kind, User};

use crate::Refused;

/// How long one slot is: its generation, its ceiling and its checksum.
const SLOT: usize = 32;

/// What a slot's checksum is computed under, so no other digest in the
/// platform can stand for one.
const DOMAIN: &str = "noctua-central/daemon/ids-mark/v1";

/// The identifiers' mark file, open, and the slot holding its newest ceiling.
#[derive(Debug)]
pub struct MarkFile {
    file: std::fs::File,
    path: PathBuf,
    /// The generation of the newest whole slot; the next write is one more.
    generation: u64,
    /// Which slot holds it, so the next write goes to the other.
    newest: usize,
    /// How far past the clock a raise writes the ceiling: the catalog's.
    lease_ms: u64,
}

impl MarkFile {
    /// The mark at `path` and the ceiling it holds — nothing for a profile's
    /// first start, which makes the file — raised by the catalog's lease.
    ///
    /// # Errors
    ///
    /// [`Refused::MarkUnreadable`] when neither slot is whole,
    /// [`Refused::NotPrivate`] for a file that is not this user's alone, and
    /// [`Refused::Unavailable`] when it could not be read, made or synced.
    pub fn open(path: &Path, user: &User) -> Result<(Self, Option<u64>), Refused> {
        let lease_ms = nc_catalog::DAEMON.mark_lease_ms;
        let failed = |doing: &str, error| {
            Refused::unavailable(format!("`{}` could not be {doing}", path.display()), error)
        };
        match nc_platform::judge(path, Kind::File, user) {
            Ok(()) => {}
            Err(Judged::Failed(failed))
                if failed.error().kind() == std::io::ErrorKind::NotFound =>
            {
                return Self::made(path, lease_ms);
            }
            Err(judged) => return Err(Refused::judged(judged)),
        }
        let mut file = opened(path).map_err(|error| failed("opened", error))?;
        let mut bytes = Vec::with_capacity(2 * SLOT);
        file.read_to_end(&mut bytes)
            .map_err(|error| failed("read", error))?;
        let newest = [0, 1]
            .into_iter()
            .filter_map(|slot| {
                decode(bytes.get(slot * SLOT..(slot + 1) * SLOT)?).map(|read| (slot, read))
            })
            .max_by_key(|(_, (generation, _))| *generation);
        let Some((slot, (generation, ceiling))) = newest else {
            return Err(Refused::MarkUnreadable {
                path: path.to_owned(),
            });
        };
        let mark = Self {
            file,
            path: path.to_owned(),
            generation,
            newest: slot,
            lease_ms,
        };
        Ok((mark, (ceiling > 0).then_some(ceiling)))
    }

    /// A profile's first mark: made private, a first slot holding no ceiling
    /// written and synced, and the directory holding it synced.
    fn made(path: &Path, lease_ms: u64) -> Result<(Self, Option<u64>), Refused> {
        let failed = |doing: &str, error| {
            Refused::unavailable(format!("`{}` could not be {doing}", path.display()), error)
        };
        let mut file = nc_platform::create_private(path).map_err(|error| failed("made", error))?;
        let mut first = [0_u8; 2 * SLOT];
        first[..SLOT].copy_from_slice(&encode(1, 0));
        file.write_all(&first)
            .and_then(|()| file.sync_all())
            .map_err(|error| failed("written", error))?;
        if let Some(directory) = path.parent() {
            nc_platform::sync_directory(directory).map_err(|error| failed("synced", error))?;
        }
        let mark = Self {
            file,
            path: path.to_owned(),
            generation: 1,
            newest: 0,
            lease_ms,
        };
        Ok((mark, None))
    }

    /// How far past the clock a raise writes the ceiling.
    #[must_use]
    pub const fn lease_ms(&self) -> u64 {
        self.lease_ms
    }

    /// Writes `ceiling` into the slot that does not hold the newest one, and
    /// syncs it, before it answers.
    fn write(&mut self, ceiling: u64) -> std::io::Result<()> {
        let generation = self.generation + 1;
        let slot = 1 - self.newest;
        self.file
            .seek(SeekFrom::Start(u64::try_from(slot * SLOT).unwrap_or(0)))?;
        self.file.write_all(&encode(generation, ceiling))?;
        self.file.sync_all()?;
        self.generation = generation;
        self.newest = slot;
        Ok(())
    }

    /// Where the mark is, for a sentence that names it.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Ceiling for MarkFile {
    fn raise(&mut self, at_least: u64) -> Result<u64, CeilingRefused> {
        let ceiling = at_least.checked_add(self.lease_ms).ok_or(CeilingRefused)?;
        self.write(ceiling).map_err(|_| CeilingRefused)?;
        Ok(ceiling)
    }

    fn settle(&mut self, high_water: u64) -> Result<(), CeilingRefused> {
        self.write(high_water).map_err(|_| CeilingRefused)
    }
}

/// The mark file opened to read and to overwrite in place, never truncated.
#[expect(
    clippy::disallowed_methods,
    reason = "the mark is reopened to read its slots and overwrite them in place, which \
              `create_private` cannot do, and only once it is judged this user's alone; \
              `ui` is the output path and the mark is never a terminal"
)]
fn opened(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::options().read(true).write(true).open(path)
}

/// One slot: its generation and its ceiling, little-endian, then sixteen bytes
/// of their checksum.
fn encode(generation: u64, ceiling: u64) -> [u8; SLOT] {
    let mut slot = [0_u8; SLOT];
    slot[..8].copy_from_slice(&generation.to_le_bytes());
    slot[8..16].copy_from_slice(&ceiling.to_le_bytes());
    let check = checksum(&slot[..16]);
    slot[16..].copy_from_slice(&check);
    slot
}

/// A slot's generation and ceiling, or nothing for one that does not check or
/// was never written.
fn decode(slot: &[u8]) -> Option<(u64, u64)> {
    let head = slot.get(..16)?;
    if slot.len() != SLOT || slot[16..] != checksum(head) {
        return None;
    }
    let generation = u64::from_le_bytes(head[..8].try_into().ok()?);
    let ceiling = u64::from_le_bytes(head[8..16].try_into().ok()?);
    (generation > 0).then_some((generation, ceiling))
}

/// Sixteen bytes of BLAKE3 over `head`, under this file's domain.
fn checksum(head: &[u8]) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(DOMAIN.len() as u64).to_le_bytes());
    hasher.update(DOMAIN.as_bytes());
    hasher.update(head);
    let digest = hasher.finalize();
    let mut check = [0_u8; 16];
    check.copy_from_slice(&digest.as_bytes()[..16]);
    check
}

/// Where a start resumes: past the mark, and past the newest unit the start
/// hands in from the local journal, in milliseconds since the Unix epoch.
#[must_use]
pub fn barrier(mark: Option<u64>, newest_unit: Option<Snowflake>) -> Option<u64> {
    let unit = newest_unit.map(|unit| unit.millis_since_epoch().saturating_add(EPOCH_MS));
    mark.max(unit)
}

/// The minter a start mints with: resumed above the barrier of `read`, the
/// ceiling the mark held, and `newest_unit`, the newest unit the local journal
/// holds, which the start reads and hands in — behind the mark as its ceiling,
/// once the clock has passed the barrier.
///
/// **A clock behind the barrier is waited for, and a wait is the caller's**:
/// `wait` is handed how far the clock still has to go, and a daemon sleeps
/// while it answers that it is waiting for the clock, where a test moves a
/// scripted clock on. Further behind than the lease is a clock that moved back
/// rather than a restart inside one lease, and `ids.clock_backwards` is
/// written once, to `log`, with how far.
///
/// # Errors
///
/// [`Refused::Mark`] for a barrier more than a day ahead of the clock, or
/// outside what an identifier can hold.
pub fn minter<C: Clock + Clone>(
    mark: MarkFile,
    read: Option<u64>,
    newest_unit: Option<Snowflake>,
    flake: MachineFlake,
    clock: &C,
    log: &impl LogSink,
    wait: &mut dyn FnMut(Duration),
) -> Result<Minter<C>, Refused> {
    let held = read.unwrap_or(0);
    let Some(at) = barrier(read, newest_unit) else {
        return Ok(Minter::new(flake, clock.clone()).with_ceiling(Box::new(mark), held));
    };
    let lease = Duration::from_millis(mark.lease_ms());
    let mut logged = false;
    loop {
        match Minter::resuming_at(flake, clock.clone(), at) {
            Ok(minter) => return Ok(minter.with_ceiling(Box::new(mark), held)),
            Err(MintError::ClockMovedBackwards { drift }) => {
                if drift > lease && !logged {
                    log.record(&nc_log::events::IdsClockBackwards {
                        drift,
                        code: ErrorCode::IdsClockBackwards,
                    });
                    logged = true;
                }
                wait(drift);
            }
            Err(refused) => return Err(Refused::Mark(refused)),
        }
    }
}

/// Settles the ceiling of the minter a start handed out, shared by every
/// journal of the profile, to exactly its high-water mark: what a clean stop
/// writes, so a clean restart has nothing to wait for.
///
/// # Errors
///
/// [`CeilingRefused`] when the mark could not be written; the ceiling the
/// last raise wrote stands then, and nothing was minted past it.
pub fn settled<C: Clock>(minter: &std::sync::Mutex<Minter<C>>) -> Result<(), CeilingRefused> {
    minter
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .settle_ceiling()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use core::time::Duration;

    use nc_catalog::ErrorCode;
    use nc_ids::{Ceiling, Clock, EPOCH_MS, MachineFlake, MintError};

    use super::{MarkFile, SLOT};
    use crate::Refused;

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("nc-daemon-mark-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        /// Where the mark is kept in the scratch, and who it is private to.
        fn mark(&self) -> (PathBuf, nc_platform::User) {
            let user = nc_platform::user_in(&self.0).expect("the scratch is private");
            (self.0.join("ids.mark"), user)
        }
    }

    /// A clock the test sets, which a wait moves past what it was asked.
    #[derive(Debug, Clone)]
    struct Set(Arc<AtomicU64>);

    impl Clock for Set {
        fn now_ms(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
        fn wait_past(&self, millis: u64) {
            self.0.fetch_max(millis + 1, Ordering::SeqCst);
        }
    }

    // The lease every raise writes by, which the mark reads from the catalog.
    const LEASE: u64 = nc_catalog::DAEMON.mark_lease_ms;
    const NOW: u64 = EPOCH_MS + 10 * 24 * 60 * 60 * 1_000;

    fn flake() -> MachineFlake {
        MachineFlake::from_value(3).expect("a flake")
    }

    /// A profile's first mark holds no ceiling, and a raise is read back by
    /// the next open, from the other slot.
    #[test]
    fn a_raise_is_durable_and_read_back_by_the_next_open() {
        let held = Scratch::new("raise");
        let (path, user) = held.mark();
        let (mut mark, read) = MarkFile::open(&path, &user).expect("made");
        assert_eq!(read, None);
        assert!(nc_platform::testing::is_private(&path));
        assert_eq!(mark.raise(NOW), Ok(NOW + LEASE));
        drop(mark);
        let (mut mark, read) = MarkFile::open(&path, &user).expect("opened");
        assert_eq!(read, Some(NOW + LEASE));
        assert_eq!(
            mark.newest, 1,
            "the raise went to the slot that was not newest"
        );
        mark.settle(NOW + 5).expect("settled");
        drop(mark);
        let (_, read) = MarkFile::open(&path, &user).expect("opened");
        assert_eq!(read, Some(NOW + 5), "the newest slot did not win");
    }

    /// A first start that stops before it mints leaves a mark the next start
    /// reads as a first start, whose minter raises it before it hands out.
    #[test]
    fn a_mark_nothing_raised_is_read_again_as_a_first_start() {
        let held = Scratch::new("unraised");
        let (path, user) = held.mark();
        drop(MarkFile::open(&path, &user).expect("made"));
        let (mark, read) = MarkFile::open(&path, &user).expect("a mark nothing raised reopens");
        assert_eq!(read, None, "a mark nothing raised was read as a ceiling");
        let clock = Set(Arc::new(AtomicU64::new(NOW)));
        let mut minter = super::minter(
            mark,
            read,
            None,
            flake(),
            &clock,
            &nc_kernel::logging::Captured::default(),
            &mut |_| panic!("a first start waited for the clock"),
        )
        .expect("the start resumes");
        let handed = minter.mint().expect("mints").millis_since_epoch() + EPOCH_MS;
        drop(minter);
        let (_, read) = MarkFile::open(&path, &user).expect("reopened");
        assert!(
            read.is_some_and(|ceiling| ceiling >= handed),
            "{handed} was handed out with the mark at {read:?}"
        );
    }

    /// A torn or corrupt slot is passed over and the other one read; with
    /// both spoiled the start refuses.
    #[test]
    fn a_corrupt_slot_falls_back_to_the_other_and_two_refuse_the_start() {
        let held = Scratch::new("torn");
        let (path, user) = held.mark();
        let (mut mark, _) = MarkFile::open(&path, &user).expect("made");
        mark.raise(NOW).expect("raised into the second slot");
        mark.raise(NOW + 100).expect("raised into the first slot");
        drop(mark);
        let spoil = |slot: usize, at: usize| {
            let mut bytes = std::fs::read(&path).expect("reads");
            bytes[slot * SLOT + at] ^= 0x40;
            std::fs::write(&path, bytes).expect("written");
        };
        // The newest, at its ceiling: the older one wins.
        spoil(0, 9);
        let (_, read) = MarkFile::open(&path, &user).expect("the other slot is whole");
        assert_eq!(read, Some(NOW + LEASE));
        // And the older one, at its checksum: neither is whole.
        spoil(1, 20);
        let refusal = MarkFile::open(&path, &user).expect_err("neither slot is whole");
        assert_eq!(refusal.code(), ErrorCode::DaemonMarkUnreadable, "{refusal}");
        // A file cut short holds no whole slot either.
        std::fs::write(&path, [0_u8; 7]).expect("written");
        assert!(matches!(
            MarkFile::open(&path, &user),
            Err(Refused::MarkUnreadable { .. })
        ));
    }

    /// A mark another account can reach is not read, and is left as it was:
    /// one it could rewrite is a ceiling it could lower.
    #[test]
    fn a_mark_another_user_can_reach_is_refused_and_left_as_it_was() {
        let held = Scratch::new("reachable");
        let (path, user) = held.mark();
        let (mut mark, _) = MarkFile::open(&path, &user).expect("made");
        mark.raise(NOW).expect("raised");
        drop(mark);
        nc_platform::testing::open_to_another_user(&path).expect("opened to another user");
        let before = std::fs::read(&path).expect("reads");
        let refusal = MarkFile::open(&path, &user).expect_err("a mark another user can reach");
        assert_eq!(
            refusal.code(),
            ErrorCode::DaemonProfileNotPrivate,
            "{refusal}"
        );
        assert_eq!(std::fs::read(&path).expect("reads"), before);
        assert!(!nc_platform::testing::is_private(&path));
    }

    /// A start resumes past the larger of the mark and the journal's newest
    /// unit.
    #[test]
    fn a_start_resumes_past_the_mark_and_the_newest_unit() {
        let mut journal = nc_ids::Minter::new(flake(), Set(Arc::new(AtomicU64::new(NOW + 50))));
        let unit = journal.mint().expect("a unit");
        assert_eq!(super::barrier(None, None), None);
        assert_eq!(super::barrier(Some(NOW), None), Some(NOW));
        assert_eq!(super::barrier(None, Some(unit)), Some(NOW + 50));
        assert_eq!(super::barrier(Some(NOW), Some(unit)), Some(NOW + 50));
        assert_eq!(super::barrier(Some(NOW + 60), Some(unit)), Some(NOW + 60));
    }

    /// The minter a start hands back raises the mark before it hands out a
    /// millisecond past it, on a profile's first start and on a resumed one,
    /// so the mark the next start reads is above everything minted.
    #[test]
    fn a_minter_from_a_start_raises_the_mark_before_it_hands_out_past_it() {
        let held = Scratch::new("minted");
        let (path, user) = held.mark();
        let mut earlier: Option<u64> = None;
        for start in ["first", "resumed"] {
            let (mark, read) = MarkFile::open(&path, &user).expect("opened");
            assert_eq!(read, earlier, "the {start} start read another mark");
            let at = earlier.map_or(NOW, |ceiling| ceiling + 10);
            let clock = Set(Arc::new(AtomicU64::new(at)));
            let mut minter = super::minter(
                mark,
                read,
                None,
                flake(),
                &clock,
                &nc_kernel::logging::Captured::default(),
                &mut |_| panic!("the {start} start's clock is past its mark"),
            )
            .expect("the start resumes");
            // Across more than one lease, so the mark is raised more than once.
            let mut last = 0;
            for _ in 0..3 {
                last = minter.mint().expect("mints").millis_since_epoch() + EPOCH_MS;
                clock.0.fetch_add(LEASE + 1, Ordering::SeqCst);
            }
            drop(minter);
            let (_, read) = MarkFile::open(&path, &user).expect("reopened");
            let Some(ceiling) = read else {
                panic!("the {start} start minted with no ceiling written");
            };
            assert!(
                ceiling >= last,
                "the {start} start handed out {last} with the mark at {ceiling}"
            );
            if let Some(earlier) = earlier {
                assert!(ceiling > earlier, "the resumed start never raised the mark");
            }
            earlier = Some(ceiling);
        }
    }

    /// A clean stop settles the mark to exactly the last millisecond handed
    /// out, below the ceiling the raises wrote, so the next start resumes
    /// there and waits for nothing.
    #[test]
    fn a_settled_mark_is_the_last_millisecond_handed_out() {
        let held = Scratch::new("settled");
        let (path, user) = held.mark();
        let (mark, read) = MarkFile::open(&path, &user).expect("opened");
        let clock = Set(Arc::new(AtomicU64::new(NOW)));
        let minter = super::minter(
            mark,
            read,
            None,
            flake(),
            &clock,
            &nc_kernel::logging::Captured::default(),
            &mut |_| panic!("a first start waits for nothing"),
        )
        .expect("the start resumes");
        let minter = std::sync::Mutex::new(minter);
        let last = minter.lock().expect("held").mint().expect("mints");
        let (_, raised) = MarkFile::open(&path, &user).expect("reopened");
        assert!(raised > Some(last.millis_since_epoch() + EPOCH_MS));
        super::settled(&minter).expect("settled");
        drop(minter);
        let (_, read) = MarkFile::open(&path, &user).expect("reopened");
        assert_eq!(
            read,
            Some(last.millis_since_epoch() + EPOCH_MS),
            "a clean stop left the ceiling a lease ahead"
        );
    }

    /// A clock behind the mark by no more than the lease is waited out with
    /// nothing logged; behind by more, the wait is logged once; and a mark more
    /// than a day ahead refuses the start.
    #[test]
    fn a_clock_behind_the_mark_is_waited_for_and_logged_past_the_lease() {
        let held = Scratch::new("behind");
        let (path, user) = held.mark();
        // A clock exactly the lease behind is a restart inside one lease, and
        // one a millisecond further is the first that moved back.
        for (behind, logged) in [(LEASE / 2, 0), (LEASE, 0), (LEASE + 1, 1), (LEASE * 3, 1)] {
            let (mark, _) = MarkFile::open(&path, &user).expect("opened");
            let clock = Set(Arc::new(AtomicU64::new(NOW - behind)));
            let log = nc_kernel::logging::Captured::default();
            let mut waits = 0_usize;
            let moved = clock.clone();
            let mut minter =
                super::minter(mark, Some(NOW), None, flake(), &clock, &log, &mut |drift| {
                    waits += 1;
                    moved.0.fetch_add(
                        u64::try_from(drift.as_millis()).unwrap_or(0) + 1,
                        Ordering::SeqCst,
                    );
                })
                .expect("the clock passes the mark");
            assert!(waits >= 1, "a clock behind the mark was not waited for");
            let after = minter.mint().expect("mints past the mark");
            assert!(after.millis_since_epoch() + EPOCH_MS > NOW);
            let lines = log.named("ids.clock_backwards");
            assert_eq!(lines.len(), logged, "{behind} ms behind: {lines:?}");
        }
        let (mark, _) = MarkFile::open(&path, &user).expect("opened");
        let day_behind = Set(Arc::new(AtomicU64::new(NOW - 25 * 60 * 60 * 1_000)));
        let refusal = super::minter(
            mark,
            Some(NOW),
            None,
            flake(),
            &day_behind,
            &nc_kernel::logging::Captured::default(),
            &mut |_| panic!("a mark a day ahead was waited for"),
        )
        .expect_err("a mark more than a day ahead");
        assert!(
            matches!(refusal, Refused::Mark(MintError::MarkOutOfRange { .. })),
            "{refusal}"
        );
    }

    /// The catalog's lease is one a restart waits out, not one a minter reads
    /// as a corrupt mark.
    ///
    /// **The compiler holds the lease below a day, and this holds the day to
    /// the minter's**: a mark the full lease ahead of a clock is resumed as a
    /// clock to wait for, so a crash right after a raise is a start that waits.
    #[test]
    fn the_lease_is_inside_the_drift_a_minter_waits_out() {
        let lease = nc_catalog::DAEMON.mark_lease_ms;
        let clock = Set(Arc::new(AtomicU64::new(NOW)));
        let refused = nc_ids::Minter::resuming_at(flake(), clock, NOW + lease)
            .expect_err("a mark ahead of the clock is waited for");
        assert!(
            matches!(refused, MintError::ClockMovedBackwards { drift } if drift == Duration::from_millis(lease)),
            "{refused:?}"
        );
    }
}
