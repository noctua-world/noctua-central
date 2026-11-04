//! Blobs: content named by its BLAKE3, kept on this machine (`NOCTUA.md §13.1`).
//!
//! **Two halves, kept apart on purpose.** A [`BlobStore`] holds the bytes —
//! [`DirectoryBlobs`] one file per blob in a directory of their own, each named
//! by its hash — and knows nothing of who made them. The local database holds
//! what the bytes cannot say about themselves: which unit created each, and
//! whether a committed unit still holds it (`nc_local::LocalStore::blob`,
//! `local-migrations/0002_blobs.sql`). A blob is *on this machine* when both
//! agree: its row is pinned, or the unit of the call tree asking made it, and
//! its bytes are in the store.
//!
//! **Written aside, then named.** A writer streams into a file of its own in
//! the same directory, hashing as it goes, and its `finish` syncs the file and
//! renames it to the hash — so a file under a hash's name is always the whole
//! of what that hash names, and a process stopped part-way leaves only a
//! writing no name points at, which [`BlobStore::forget_unfinished`] removes. A
//! hash the store already holds is not written again: the bytes are the same
//! bytes, by what a hash is.
//!
//! **A unit's blobs are its own until it is kept.** The blobs a command makes
//! are recorded in its call tree's unit (`Made`) and become rows only when
//! the unit commits, in the same transaction as its writes, each with a note
//! saying whether a committed unit held it before (`note`). A unit let go
//! deletes the bytes it made that nothing else holds (`let_go`); one the
//! journal does not keep is reversed with the pins taken off the blobs it made
//! (`unpinned_by`); and `sweep` deletes what no unit has held for
//! `blob_orphan_grace_ms`, the bytes no row names, and every writing no writer
//! holds — bytes, in either case, only once nobody has named them for that
//! grace, which a unit in flight of any realm sharing the bytes always has.
//!
//! What this answers (`NOCTUA.md §2.4`): another account on the machine
//! reading or replacing the bytes, because the directory is held to the local
//! database's own rule — this user's alone, not a link
//! (`nc_local::blob_directory`) — and every file is created private to that
//! user (`nc_platform::create_private`: `0600` on Unix, an access list naming
//! the user alone on Windows); a blob
//! half written passing for a whole one, because a name is given only to bytes
//! synced in full; and bytes outliving the unit that made them, because a unit
//! let go deletes them and the sweep deletes whatever was named and never
//! recorded — by a stopped process, or by a finish that failed or was
//! abandoned after it named its bytes.
//! What it does not answer: a process running as the same user, which can
//! write any file here — the bytes of a blob are trusted to be what its name
//! says when they are read, and not hashed again; two stores of blobs opened
//! over one directory, each of whose sweeps takes the other's writings in
//! progress for a stopped process's — the kernels of one profile share one;
//! the same bytes made by two realms' units in flight at once, when one is let
//! go, which deletes them under the other; the moment between a sweep finding
//! bytes old and deleting them, in which another realm's unit may name them
//! again; a wall clock stepped on by a grace, which makes bytes in flight old;
//! and a disk filled by blobs a plugin is granted to write, within
//! `blob_bytes` each and the units' bounds.

use core::fmt;
use core::future::Future;
use std::collections::BTreeSet;
use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

#[cfg(any(test, feature = "test-support"))]
use std::collections::BTreeMap;

use nc_catalog::{ErrorCode, KERNEL};
use nc_local::LocalError;

use crate::backend::StoreBackend;
use crate::dispatch::{Refusal, RefusalKind};
use crate::units;

/// A blob's name: the BLAKE3 of its bytes, as sixty-four lowercase
/// hexadecimal characters.
///
/// **Only ever built from text that is one, or from bytes.** The world calls a
/// hash a string, so a plugin can hand the kernel any text at all; [`Hash::parse`]
/// is the one door from text, and a name that is not a hash is never a file's.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash(String);

impl Hash {
    /// The hash `text` is, when it is one.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let shaped = text.len() == 64
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        shaped.then(|| Self(text.to_owned()))
    }

    /// The hash of `bytes`.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    /// The hash as its sixty-four characters.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Why a blob store could not do what it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobError {
    /// The directory is not this user's alone, or could not be made: the local
    /// database's own refusal, with its code.
    Directory(LocalError),
    /// Reading or writing a blob's bytes failed.
    Io(String),
}

impl BlobError {
    /// The registry code this failure carries.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Directory(refused) => refused.code(),
            Self::Io(_) => ErrorCode::BlobIo,
        }
    }

    /// The failure as a refusal the kernel answers with.
    ///
    /// A directory is refused as the local database refuses one, and failed
    /// bytes are `unavailable`, since a disk that failed once may answer the
    /// next time.
    pub(crate) fn refusal(&self) -> Refusal {
        match self {
            Self::Directory(refused) => units::store_failed(refused),
            Self::Io(why) => Refusal::new(RefusalKind::Unavailable, ErrorCode::BlobIo, why.clone()),
        }
    }
}

impl fmt::Display for BlobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Directory(refused) => write!(formatter, "{refused}"),
            Self::Io(why) => write!(formatter, "[{}] {why}", self.code()),
        }
    }
}

impl std::error::Error for BlobError {}

/// A writing, ended: the name its bytes are held under now, how many there
/// are, and whether the store held them under that name already.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    /// The BLAKE3 of what was written.
    pub hash: Hash,
    /// How many bytes were written.
    pub size: u64,
    /// Whether the store held these bytes before this writing ended — and so
    /// kept what it had rather than writing them again.
    pub held_before: bool,
}

/// Where a blob's bytes are kept: what the kernel reads of them, and what a
/// writer adds.
///
/// **Asynchronous, and not a trait object**, like the kernel's store: the
/// store that runs reads and writes a disk, which may not answer, and every
/// host function that waits on it is held to `host_call_deadline_ms`. A
/// writer is moved into a write and handed back, so a write the deadline
/// abandons takes its writer with it, and the writing is let go when it ends.
pub trait BlobStore: fmt::Debug + Send + Sync + 'static {
    /// One writing in progress: bytes no name points at yet.
    type Writer: fmt::Debug + Send + Sync + 'static;

    /// How many bytes `hash` names, or `None` when this store does not hold it.
    ///
    /// # Errors
    ///
    /// [`BlobError::Io`] when the store cannot be read, or holds something
    /// other than bytes under that name.
    fn size(&self, hash: &Hash) -> impl Future<Output = Result<Option<u64>, BlobError>> + Send;

    /// Up to `max` of the bytes `hash` names, from `offset` — fewer at the end,
    /// none past it — or `None` when this store does not hold it.
    ///
    /// # Errors
    ///
    /// [`BlobError::Io`] when the bytes cannot be read.
    fn read(
        &self,
        hash: &Hash,
        offset: u64,
        max: usize,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, BlobError>> + Send;

    /// A new writing, holding nothing yet.
    ///
    /// # Errors
    ///
    /// [`BlobError::Io`] when nowhere could be made to write it.
    fn create(&self) -> impl Future<Output = Result<Self::Writer, BlobError>> + Send;

    /// Adds `bytes` to the writing, and hands the writer back.
    ///
    /// # Errors
    ///
    /// [`BlobError::Io`] when the bytes could not be written, after which
    /// the writing holds an unknown part of them.
    fn write(
        &self,
        writer: Self::Writer,
        bytes: Vec<u8>,
    ) -> impl Future<Output = (Self::Writer, Result<(), BlobError>)> + Send;

    /// The hash of what `writer` holds so far: the name [`BlobStore::finish`]
    /// gives its bytes when it ends with nothing more written.
    ///
    /// **What lets a writing's blob be asked about before it is named.** The
    /// kernel reads a blob's row before its `finish`, so a row that is refused
    /// or never comes lets the writing go with nothing named, where a row read
    /// after would leave bytes under the hash that nothing records.
    fn hash_so_far(&self, writer: &Self::Writer) -> Hash;

    /// Ends the writing: its bytes are held under their hash from now on.
    ///
    /// # Errors
    ///
    /// [`BlobError::Io`] when the bytes could not be kept. Nothing is then held
    /// under the hash that was not held before, unless the failure came after
    /// the bytes were named — the directory refused its sync, on a system that
    /// syncs one (`nc_platform::SYNCS_A_DIRECTORY`) — and then they are held
    /// under it all the same.
    fn finish(
        &self,
        writer: Self::Writer,
    ) -> impl Future<Output = Result<Finished, BlobError>> + Send;

    /// Deletes the bytes `hash` names; `false` when the store did not hold
    /// them.
    ///
    /// # Errors
    ///
    /// [`BlobError::Io`] when they could not be deleted.
    fn remove(&self, hash: &Hash) -> impl Future<Output = Result<bool, BlobError>> + Send;

    /// Every hash the store holds bytes under that were last named at least
    /// `named` ago, on the store's own clock: every one for no time at all.
    ///
    /// **Named again by every finish that finds them held**, so bytes a unit
    /// in flight made — in any kernel over the same bytes — are never old.
    ///
    /// # Errors
    ///
    /// [`BlobError::Io`] when the store cannot be listed.
    fn hashes(
        &self,
        named: std::time::Duration,
    ) -> impl Future<Output = Result<Vec<Hash>, BlobError>> + Send;

    /// Deletes every writing no writer of this store holds — what a stopped
    /// process was still writing — and answers how many went.
    ///
    /// # Errors
    ///
    /// [`BlobError::Io`] when the store cannot be listed or one could not be
    /// deleted.
    fn forget_unfinished(&self) -> impl Future<Output = Result<u64, BlobError>> + Send;
}

/// A store shared, answering as the store it shares: what lets a test keep a
/// hand on the store a kernel was opened with.
impl<B: BlobStore> BlobStore for Arc<B> {
    type Writer = B::Writer;

    fn size(&self, hash: &Hash) -> impl Future<Output = Result<Option<u64>, BlobError>> + Send {
        (**self).size(hash)
    }

    fn read(
        &self,
        hash: &Hash,
        offset: u64,
        max: usize,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, BlobError>> + Send {
        (**self).read(hash, offset, max)
    }

    fn create(&self) -> impl Future<Output = Result<Self::Writer, BlobError>> + Send {
        (**self).create()
    }

    fn write(
        &self,
        writer: Self::Writer,
        bytes: Vec<u8>,
    ) -> impl Future<Output = (Self::Writer, Result<(), BlobError>)> + Send {
        (**self).write(writer, bytes)
    }

    fn hash_so_far(&self, writer: &Self::Writer) -> Hash {
        (**self).hash_so_far(writer)
    }

    fn finish(
        &self,
        writer: Self::Writer,
    ) -> impl Future<Output = Result<Finished, BlobError>> + Send {
        (**self).finish(writer)
    }

    fn remove(&self, hash: &Hash) -> impl Future<Output = Result<bool, BlobError>> + Send {
        (**self).remove(hash)
    }

    fn hashes(
        &self,
        named: std::time::Duration,
    ) -> impl Future<Output = Result<Vec<Hash>, BlobError>> + Send {
        (**self).hashes(named)
    }

    fn forget_unfinished(&self) -> impl Future<Output = Result<u64, BlobError>> + Send {
        (**self).forget_unfinished()
    }
}

/// How the name of a writing begins: never a hash, which is sixty-four
/// hexadecimal characters with no dot.
const WRITING: &str = ".writing-";

/// How many writings this process has begun, so two begun at once never
/// carry the same name.
static WRITINGS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The blobs of one machine, one file each in a directory of their own.
///
/// **The directory is judged once, when the store is opened**, by the local
/// database's rule (`nc_local::blob_directory`), and every file in it is
/// created private by the writer that makes it (`nc_platform::create_private`).
/// Every call is a blocking read or write of the disk, made on a thread of its
/// own so that the host call waiting on it can be abandoned at its deadline.
#[derive(Debug)]
pub struct DirectoryBlobs {
    root: PathBuf,
    /// The names of the writings this store's writers hold now.
    writing: Arc<Mutex<BTreeSet<String>>>,
}

impl DirectoryBlobs {
    /// The blobs kept in `root`, which is created private when it is missing.
    ///
    /// # Errors
    ///
    /// [`BlobError::Directory`] when `root` is not this user's alone — another
    /// user can reach it or owns it, it is a link, or its owner may not read,
    /// enter and write in it — or the filesystem would not let it be made.
    pub fn open(root: &Path) -> Result<Self, BlobError> {
        nc_local::blob_directory(root).map_err(BlobError::Directory)?;
        Ok(Self {
            root: root.to_owned(),
            writing: Arc::default(),
        })
    }
}

/// One writing into a [`DirectoryBlobs`]: a file of its own, hashed as it is
/// written.
///
/// **Deleted when it is dropped unfinished** — a plugin that lets its writer
/// go, a plugin killed while it holds one — so an abandoned writing leaves
/// nothing behind but in a process stopped outright.
///
/// **On a thread of the blocking pool, which nothing waits for**, where a
/// runtime runs: closing and deleting a file are calls of the disk like any
/// read, and a disk that does not answer then holds that thread, not the one
/// every plugin runs on. Where no runtime runs, it is deleted at once.
#[derive(Debug)]
pub struct DirectoryWriter {
    file: Option<std::fs::File>,
    path: PathBuf,
    name: String,
    hasher: blake3::Hasher,
    size: u64,
    /// Whether the writing's file is gone: renamed to its hash, or deleted.
    ended: bool,
    writing: Arc<Mutex<BTreeSet<String>>>,
}

impl DirectoryWriter {
    /// Records that the writing's file is gone — named, or deleted — so that
    /// dropping the writer deletes nothing.
    const fn end(&mut self) {
        self.ended = true;
    }
}

impl Drop for DirectoryWriter {
    fn drop(&mut self) {
        if self.ended {
            held(&self.writing).remove(&self.name);
            return;
        }
        let abandoned = Abandoned {
            file: self.file.take(),
            path: core::mem::take(&mut self.path),
            name: core::mem::take(&mut self.name),
            writing: Arc::clone(&self.writing),
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            drop(runtime.spawn_blocking(move || drop(abandoned)));
        } else {
            drop(abandoned);
        }
    }
}

/// A writing let go unfinished: closed, deleted and no longer held when this
/// is dropped, on whichever thread drops it.
#[derive(Debug)]
struct Abandoned {
    file: Option<std::fs::File>,
    path: PathBuf,
    name: String,
    writing: Arc<Mutex<BTreeSet<String>>>,
}

impl Drop for Abandoned {
    fn drop(&mut self) {
        self.file = None;
        // A writing that could not be deleted is one no writer holds, and the
        // next sweep deletes it. Its name is held until it is gone, so a
        // sweep never counts a writing its writer is still deleting.
        let _left_for_the_sweep = std::fs::remove_file(&self.path);
        held(&self.writing).remove(&self.name);
    }
}

/// The names of the writings held now, whoever last held the lock: nothing
/// panics holding it.
fn held(writing: &Mutex<BTreeSet<String>>) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
    writing.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A filesystem failure while `doing`, as the failure of a blob's bytes.
fn failed(doing: &str, error: &std::io::Error) -> BlobError {
    BlobError::Io(format!("{doing}: {error}"))
}

/// `work` on a thread of its own, answering what it answered.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, BlobError> + Send + 'static,
) -> Result<T, BlobError> {
    tokio::task::spawn_blocking(work).await.unwrap_or_else(|_| {
        Err(BlobError::Io(
            "the thread the disk was read on stopped".to_owned(),
        ))
    })
}

/// The bytes' file under `root`, and whether it is one: `None` when nothing
/// is there.
fn file_of(root: &Path, hash: &Hash) -> Result<Option<(PathBuf, u64)>, BlobError> {
    let path = root.join(hash.as_str());
    match std::fs::symlink_metadata(&path) {
        Ok(found) if found.file_type().is_file() => Ok(Some((path, found.len()))),
        Ok(_) => Err(BlobError::Io(format!(
            "`{}` is not a regular file, so it is not a blob's bytes",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failed(&format!("reading `{}`", path.display()), &error)),
    }
}

impl BlobStore for DirectoryBlobs {
    type Writer = DirectoryWriter;

    fn size(&self, hash: &Hash) -> impl Future<Output = Result<Option<u64>, BlobError>> + Send {
        let (root, hash) = (self.root.clone(), hash.clone());
        blocking(move || Ok(file_of(&root, &hash)?.map(|(_, size)| size)))
    }

    fn read(
        &self,
        hash: &Hash,
        offset: u64,
        max: usize,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, BlobError>> + Send {
        let (root, hash) = (self.root.clone(), hash.clone());
        blocking(move || {
            let Some((path, size)) = file_of(&root, &hash)? else {
                return Ok(None);
            };
            let reading =
                |error: std::io::Error| failed(&format!("reading `{}`", path.display()), &error);
            let mut file = std::fs::File::open(&path).map_err(reading)?;
            file.seek(std::io::SeekFrom::Start(offset))
                .map_err(reading)?;
            let left = usize::try_from(size.saturating_sub(offset))
                .unwrap_or(usize::MAX)
                .min(max);
            let mut bytes = Vec::with_capacity(left);
            file.take(u64::try_from(left).unwrap_or(u64::MAX))
                .read_to_end(&mut bytes)
                .map_err(reading)?;
            Ok(Some(bytes))
        })
    }

    fn create(&self) -> impl Future<Output = Result<Self::Writer, BlobError>> + Send {
        let (root, writing) = (self.root.clone(), Arc::clone(&self.writing));
        blocking(move || {
            let made = WRITINGS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos());
            let name = format!("{WRITING}{}-{now}-{made}", std::process::id());
            let path = root.join(&name);
            // Private before any byte is written: a name that is there, a
            // link included, is refused rather than opened.
            let file = nc_platform::create_private(&path)
                .map_err(|error| failed(&format!("creating `{}`", path.display()), &error))?;
            held(&writing).insert(name.clone());
            Ok(DirectoryWriter {
                file: Some(file),
                path,
                name,
                hasher: blake3::Hasher::new(),
                size: 0,
                ended: false,
                writing,
            })
        })
    }

    async fn write(
        &self,
        mut writer: Self::Writer,
        bytes: Vec<u8>,
    ) -> (Self::Writer, Result<(), BlobError>) {
        {
            let written = tokio::task::spawn_blocking(move || {
                let Some(file) = writer.file.as_mut() else {
                    return (
                        writer,
                        Err(BlobError::Io("the writing has ended".to_owned())),
                    );
                };
                let wrote = file.write_all(&bytes).map_err(|error| {
                    failed(&format!("writing `{}`", writer.path.display()), &error)
                });
                if wrote.is_ok() {
                    writer.hasher.update(&bytes);
                    writer.size = writer
                        .size
                        .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
                }
                (writer, wrote)
            })
            .await;
            match written {
                Ok(written) => written,
                // The thread stopped holding the writer, and dropped it, which
                // deleted the writing: what is handed back holds nothing.
                Err(_) => (
                    DirectoryWriter {
                        file: None,
                        path: PathBuf::new(),
                        name: String::new(),
                        hasher: blake3::Hasher::new(),
                        size: 0,
                        ended: true,
                        writing: Arc::default(),
                    },
                    Err(BlobError::Io(
                        "the thread the writing ran on stopped".to_owned(),
                    )),
                ),
            }
        }
    }

    fn hash_so_far(&self, writer: &Self::Writer) -> Hash {
        Hash(writer.hasher.finalize().to_hex().to_string())
    }

    fn finish(
        &self,
        mut writer: Self::Writer,
    ) -> impl Future<Output = Result<Finished, BlobError>> + Send {
        let root = self.root.clone();
        blocking(move || {
            let file = writer
                .file
                .take()
                .ok_or_else(|| BlobError::Io("the writing has ended".to_owned()))?;
            file.sync_all()
                .map_err(|error| failed(&format!("syncing `{}`", writer.path.display()), &error))?;
            drop(file);
            let hash = Hash(writer.hasher.finalize().to_hex().to_string());
            let size = writer.size;
            // **Bytes the store holds already are not written again**, and a
            // file under the name that is not those bytes — another size — is
            // not that blob, so the writing takes its place.
            let held_before = file_of(&root, &hash)?.is_some_and(|(_, held)| held == size);
            if held_before {
                std::fs::remove_file(&writer.path).map_err(|error| {
                    failed(&format!("deleting `{}`", writer.path.display()), &error)
                })?;
                // **Named again**, so a sweep of any kernel over this
                // directory finds the bytes young while the unit that made
                // them again is in flight (`sweep`).
                let named = root.join(hash.as_str());
                std::fs::File::open(&named)
                    .and_then(|bytes| bytes.set_modified(std::time::SystemTime::now()))
                    .map_err(|error| failed(&format!("naming `{}`", named.display()), &error))?;
            } else {
                let named = root.join(hash.as_str());
                std::fs::rename(&writer.path, &named)
                    .map_err(|error| failed(&format!("naming `{}`", named.display()), &error))?;
                // The rename is the directory's to keep, and a directory is
                // synced through a handle of its own where it has one.
                // **A sync refused is not undone**: the bytes are named
                // already, and the file whose place they took — not these
                // bytes — is gone, so there is nothing to put back. They stay
                // named, and the sweep deletes them unless a row names their
                // hash.
                nc_platform::sync_directory(&root)
                    .map_err(|error| failed(&format!("syncing `{}`", root.display()), &error))?;
            }
            writer.end();
            Ok(Finished {
                hash,
                size,
                held_before,
            })
        })
    }

    fn remove(&self, hash: &Hash) -> impl Future<Output = Result<bool, BlobError>> + Send {
        let path = self.root.join(hash.as_str());
        blocking(move || {
            // A directory under the name is not the blob's bytes, and no
            // unlink deletes one (`names_in` says why that matters).
            if std::fs::symlink_metadata(&path).is_ok_and(|found| found.is_dir()) {
                return Ok(false);
            }
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(failed(&format!("deleting `{}`", path.display()), &error)),
            }
        })
    }

    fn hashes(
        &self,
        named: std::time::Duration,
    ) -> impl Future<Output = Result<Vec<Hash>, BlobError>> + Send {
        let root = self.root.clone();
        blocking(move || {
            let now = std::time::SystemTime::now();
            let mut hashes = Vec::new();
            for hash in names_in(&root)?.iter().filter_map(|name| Hash::parse(name)) {
                let path = root.join(hash.as_str());
                let modified = std::fs::symlink_metadata(&path)
                    .and_then(|found| found.modified())
                    .map_err(|error| failed(&format!("reading `{}`", path.display()), &error))?;
                // A name the clock says is from the future is not old.
                if now
                    .duration_since(modified)
                    .map_or(named.is_zero(), |since| since >= named)
                {
                    hashes.push(hash);
                }
            }
            hashes.sort();
            Ok(hashes)
        })
    }

    fn forget_unfinished(&self) -> impl Future<Output = Result<u64, BlobError>> + Send {
        let (root, writing) = (self.root.clone(), Arc::clone(&self.writing));
        blocking(move || {
            let mut forgotten = 0_u64;
            for name in names_in(&root)? {
                if !name.starts_with(WRITING) || held(&writing).contains(&name) {
                    continue;
                }
                let path = root.join(&name);
                match std::fs::remove_file(&path) {
                    Ok(()) => forgotten += 1,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(failed(&format!("deleting `{}`", path.display()), &error));
                    }
                }
            }
            Ok(forgotten)
        })
    }
}

/// The name of every entry in `root` that is one, but a directory's, in no
/// order.
///
/// **A directory is never a blob's bytes or a writing**, and no unlink deletes
/// one. Listed, one under a hash's name would stop every sweep at it for good —
/// each refused at the same entry — and the writings no writer holds, which a
/// sweep deletes last, would never be deleted at all.
fn names_in(root: &Path) -> Result<Vec<String>, BlobError> {
    let listing = |error: std::io::Error| failed(&format!("listing `{}`", root.display()), &error);
    let mut names = Vec::new();
    for entry in std::fs::read_dir(root).map_err(listing)? {
        let entry = entry.map_err(listing)?;
        if entry.file_type().map_err(listing)?.is_dir() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_owned());
        }
    }
    Ok(names)
}

/// The blobs of one machine, held in memory, which forgets them when it is
/// dropped. **For tests**, behind `test-support`.
///
/// It answers every call as [`DirectoryBlobs`] does: one script run through
/// both, in `the_two_stores_answer_one_script_alike`, holds it to
/// that.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct MemoryBlobs {
    held: Mutex<BTreeMap<Hash, Vec<u8>>>,
    /// When the bytes of each hash were last named.
    named: Mutex<BTreeMap<Hash, std::time::Instant>>,
}

/// One writing into a [`MemoryBlobs`]: the bytes so far.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct MemoryWriter {
    bytes: Vec<u8>,
}

#[cfg(any(test, feature = "test-support"))]
impl MemoryBlobs {
    /// What is held, whoever last held the lock: nothing here panics holding it.
    fn held(&self) -> std::sync::MutexGuard<'_, BTreeMap<Hash, Vec<u8>>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// When each hash was last named, whoever last held the lock.
    fn named(&self) -> std::sync::MutexGuard<'_, BTreeMap<Hash, std::time::Instant>> {
        self.named.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ages every blob held by `by`, as though each had been named that much
    /// earlier: what a test of the sweep's grace waits out.
    pub fn age(&self, by: std::time::Duration) {
        for named in self.named().values_mut() {
            *named = named.checked_sub(by).unwrap_or(*named);
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl BlobStore for MemoryBlobs {
    type Writer = MemoryWriter;

    fn size(&self, hash: &Hash) -> impl Future<Output = Result<Option<u64>, BlobError>> + Send {
        let size = self
            .held()
            .get(hash)
            .map(|bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        core::future::ready(Ok(size))
    }

    fn read(
        &self,
        hash: &Hash,
        offset: u64,
        max: usize,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, BlobError>> + Send {
        let read = self.held().get(hash).map(|bytes| {
            let from = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            let to = from.saturating_add(max).min(bytes.len());
            bytes[from..to].to_vec()
        });
        core::future::ready(Ok(read))
    }

    fn create(&self) -> impl Future<Output = Result<Self::Writer, BlobError>> + Send {
        core::future::ready(Ok(MemoryWriter::default()))
    }

    fn write(
        &self,
        mut writer: Self::Writer,
        bytes: Vec<u8>,
    ) -> impl Future<Output = (Self::Writer, Result<(), BlobError>)> + Send {
        writer.bytes.extend_from_slice(&bytes);
        core::future::ready((writer, Ok(())))
    }

    fn hash_so_far(&self, writer: &Self::Writer) -> Hash {
        Hash::of(&writer.bytes)
    }

    fn finish(
        &self,
        writer: Self::Writer,
    ) -> impl Future<Output = Result<Finished, BlobError>> + Send {
        let hash = Hash::of(&writer.bytes);
        let size = u64::try_from(writer.bytes.len()).unwrap_or(u64::MAX);
        let mut held = self.held();
        let held_before = held
            .get(&hash)
            .is_some_and(|bytes| bytes.len() == writer.bytes.len());
        if !held_before {
            held.insert(hash.clone(), writer.bytes);
        }
        self.named().insert(hash.clone(), std::time::Instant::now());
        core::future::ready(Ok(Finished {
            hash,
            size,
            held_before,
        }))
    }

    fn remove(&self, hash: &Hash) -> impl Future<Output = Result<bool, BlobError>> + Send {
        self.named().remove(hash);
        core::future::ready(Ok(self.held().remove(hash).is_some()))
    }

    fn hashes(
        &self,
        named: std::time::Duration,
    ) -> impl Future<Output = Result<Vec<Hash>, BlobError>> + Send {
        let held = self.held();
        let times = self.named();
        let hashes = held
            .keys()
            .filter(|hash| {
                times
                    .get(*hash)
                    .is_none_or(|at| named.is_zero() || at.elapsed() >= named)
            })
            .cloned()
            .collect();
        core::future::ready(Ok(hashes))
    }

    fn forget_unfinished(&self) -> impl Future<Output = Result<u64, BlobError>> + Send {
        // A writing in memory is its writer's, and goes when the writer does.
        core::future::ready(Ok(0))
    }
}

/// A blob a unit made: its name and size, and what this machine held of it
/// before the unit made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Made {
    /// The blob.
    pub(crate) hash: Hash,
    /// Its size in bytes.
    pub(crate) size: u64,
    /// Whether this machine held its bytes before — the store already held
    /// them, or a row named them — so letting the unit go leaves them.
    pub(crate) kept_before: bool,
    /// Whether a committed unit held it pinned before: what reversing this
    /// unit leaves pinned.
    pub(crate) pinned_before: bool,
}

/// What a note says when it records a blob a unit made.
const BLOB_CREATED: &str = "blob_created";

/// The note a unit keeps of a blob it made, as canonical JSON.
///
/// Its fields are `{"hash":…,"note":"blob_created","pinned_before":…}`.
///
/// **What a reversal reads back.** The kernel that reverses a unit — at once,
/// when the journal will not take its close, or later, from the history — has
/// nothing but the store to read what the unit did from, and the pin a
/// reversal takes off is the one the unit put on.
pub(crate) fn note(made: &Made) -> String {
    noted(made.hash.as_str(), made.pinned_before)
}

/// The note of a blob `hash` a unit made, `pinned_before` when a committed
/// unit held it before: [`note`]'s text, which an undo also asks the store
/// whether another unit holds.
pub(crate) fn noted(hash: &str, pinned_before: bool) -> String {
    nc_canon::Value::object([
        ("hash", nc_canon::Value::text(hash.to_owned())),
        ("note", nc_canon::Value::text(BLOB_CREATED.to_owned())),
        ("pinned_before", nc_canon::Value::Bool(pinned_before)),
    ])
    .to_canonical()
}

/// The blob and whether a committed unit held it before, when `written` is a
/// note of a blob a unit made, and nothing when it is a note of another kind.
///
/// # Errors
///
/// [`LocalError::Unexpected`] for a blob's note that is not one [`note`]
/// writes: the store holds what the kernel did not write.
pub(crate) fn created(written: &str) -> Result<Option<(String, bool)>, LocalError> {
    let Some(mut fields) = nc_canon::parse(written)
        .ok()
        .and_then(nc_canon::Value::into_object)
    else {
        return Ok(None);
    };
    if fields.get("note") != Some(&nc_canon::Value::text(BLOB_CREATED)) {
        return Ok(None);
    }
    let hash = fields
        .remove("hash")
        .and_then(nc_canon::Value::into_text)
        .and_then(|hash| Hash::parse(&hash));
    let pinned_before = match fields.remove("pinned_before") {
        Some(nc_canon::Value::Bool(pinned_before)) => Some(pinned_before),
        _ => None,
    };
    // What is left is the note's kind, and nothing beside it.
    let (Some(hash), Some(pinned_before), 1) = (hash, pinned_before, fields.len()) else {
        return Err(LocalError::Unexpected(format!(
            "a unit's note of a blob it made is not one the kernel writes: {written}"
        )));
    };
    Ok(Some((hash.0, pinned_before)))
}

/// The blobs a unit whose notes are `notes` made and no committed unit held
/// before it: the ones its reversal takes the pin off.
///
/// A note of another kind is not a blob's, and is passed over.
///
/// # Errors
///
/// [`LocalError::Unexpected`] for a blob's note that is not one [`note`]
/// writes: the store holds what the kernel did not write.
pub(crate) fn unpinned_by(notes: &[String]) -> Result<Vec<String>, LocalError> {
    let mut unpinned = Vec::new();
    for written in notes {
        if let Some((hash, false)) = created(written)? {
            unpinned.push(hash);
        }
    }
    Ok(unpinned)
}

/// Deletes the bytes of every blob of `made` that only the unit being let go
/// held: not held before it, and named by no row now.
///
/// **What fails here is left for the sweep**, which deletes the bytes no row
/// names: the unit is let go whether or not its bytes could be deleted at
/// once, and a store that cannot say whether a row names them is taken to
/// say one does.
pub(crate) async fn let_go<S: StoreBackend, B: BlobStore>(store: &S, blobs: &B, made: &[Made]) {
    for blob in made.iter().filter(|blob| !blob.kept_before) {
        if let Ok(None) = store.blob(blob.hash.as_str()).await {
            let _left_for_the_sweep = blobs.remove(&blob.hash).await;
        }
    }
}

/// What one sweep of the blobs let go of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Swept {
    /// Blobs no committed unit had held for `blob_orphan_grace_ms`: each one's
    /// row and its bytes.
    pub forgotten: u64,
    /// Bytes no row names: what a unit made and a stopped process neither
    /// kept nor let go.
    pub orphans: u64,
    /// Writings no writer holds: what a stopped process was still writing.
    pub unfinished: u64,
}

/// How many unpinned rows one read of the sweep takes: a page, not a bound —
/// the sweep reads pages until none is left.
const SWEPT_AT_ONCE: u32 = 1_000;

/// Sweeps the blobs as of `now_ms`: the ones no unit has held for their
/// grace, then the bytes no row names, then the writings no writer holds.
///
/// **Only between call trees.** A unit's blobs are bytes no row names until it
/// commits, so a sweep while a tree runs would take them for orphans; the
/// kernel runs this holding the bus every top-level dispatch takes.
///
/// **And only bytes no one has named for `blob_orphan_grace_ms`**, row or no
/// row. The bus is one kernel's, and the kernels of the realms one person
/// belongs to share the file and the bytes: judging by rows alone, one realm's
/// sweep would delete, as an orphan, the bytes another realm's unit has made
/// and not yet committed, whose step would then be kept pinning bytes that are
/// gone. A unit in flight names its bytes — again, when they are held already
/// ([`BlobStore::hashes`]) — so they are never old while it is.
///
/// **And never a blob a reversal still owed pins again.** An undo whose close
/// no kernel has heard the journal take — this kernel's, whose reversal the
/// store refused, or another realm's stopped process's, sharing the file — is
/// held from the pruner (`nc_local::LocalStore::commit_held`), and its
/// reversal, when it comes, pins again each blob the undo took the pin off. A
/// sweep that forgot such a row and its bytes after their grace would leave the
/// reversal inserting a pinned row for bytes that are gone; so the blobs the
/// notes of a held unit record as unpinned by it are left for after its
/// reversal.
///
/// # Errors
///
/// What the store or the blobs refused, with its code. What was let go before
/// the refusal stays let go, and the next sweep begins again: it stops at the
/// same place only while what refused this one still refuses.
pub(crate) async fn sweep<S: StoreBackend, B: BlobStore>(
    store: &S,
    blobs: &B,
    now_ms: u64,
) -> Result<Swept, Refusal> {
    let read = |failed: LocalError| units::store_failed(&failed);
    let bytes = |failed: BlobError| failed.refusal();
    let before = now_ms.saturating_sub(KERNEL.blob_orphan_grace_ms);
    let owed = owed_back(store).await.map_err(read)?;
    let old: BTreeSet<Hash> = blobs
        .hashes(std::time::Duration::from_millis(
            KERNEL.blob_orphan_grace_ms,
        ))
        .await
        .map_err(bytes)?
        .into_iter()
        .collect();
    let mut swept = Swept::default();
    loop {
        let due = store
            .unpinned_before(before, SWEPT_AT_ONCE)
            .await
            .map_err(read)?;
        let mut forgot = false;
        for hash in &due {
            if owed.contains(hash) || !store.forget_blob(hash).await.map_err(read)? {
                continue;
            }
            forgot = true;
            swept.forgotten += 1;
            if let Some(hash) = Hash::parse(hash).filter(|hash| old.contains(hash)) {
                blobs.remove(&hash).await.map_err(bytes)?;
            }
        }
        if !forgot || due.len() < usize::try_from(SWEPT_AT_ONCE).unwrap_or(usize::MAX) {
            break;
        }
    }
    for hash in old {
        if store.blob(hash.as_str()).await.map_err(read)?.is_none()
            && blobs.remove(&hash).await.map_err(bytes)?
        {
            swept.orphans += 1;
        }
    }
    swept.unfinished = blobs.forget_unfinished().await.map_err(bytes)?;
    Ok(swept)
}

/// Every blob a reversal still owed would pin again: each the notes of a unit
/// held from the pruner record as unpinned by it.
///
/// # Errors
///
/// What the store refused the question with, and [`LocalError::Unexpected`]
/// for a note the kernel did not write.
async fn owed_back<S: StoreBackend>(
    store: &S,
) -> Result<std::collections::BTreeSet<String>, LocalError> {
    let mut owed = std::collections::BTreeSet::new();
    for holder in store.holders().await? {
        if let Some(detail) = store.unit(holder).await? {
            let moved = crate::history::pins_moved(&detail.notes)?;
            owed.extend(moved.unpinned.into_iter().map(|blob| blob.hash));
        }
    }
    Ok(owed)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{
        BlobStore, DirectoryBlobs, Hash, Made, MemoryBlobs, Swept, WRITING, let_go, note, sweep,
        unpinned_by,
    };
    use crate::backend::{MemoryBackend, StoreBackend as _};
    use nc_catalog::KERNEL;
    use nc_local::{BlobCreated, Quotas, UnitKind, UnitMeta, UnitRecord};

    /// A unit of `ana`'s in the realm `home`, minted as `n`, recording the
    /// blobs `blobs` and nothing else.
    fn recording(n: u16, blobs: &[&Hash]) -> UnitRecord {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let world =
            nc_authz::MemoryDirectory::from_world(include_str!("../../../policy/world.json"), &pdp)
                .expect("the world loads");
        UnitRecord {
            unit: nc_ids::Minter::new(
                nc_ids::MachineFlake::from_value(n).expect("a flake"),
                nc_ids::SystemClock,
            )
            .mint()
            .expect("a unit"),
            meta: UnitMeta {
                realm: world.realm_id("home").expect("home is a realm"),
                principal: world
                    .principal(nc_authz::PrincipalKind::User, "ana")
                    .expect("ana is a user")
                    .id()
                    .clone(),
                kind: UnitKind::Step,
                label: "a test's".to_owned(),
                committed_at_ms: 0,
            },
            writes: Vec::new(),
            blobs: blobs
                .iter()
                .map(|hash| BlobCreated {
                    hash: hash.to_string(),
                    size: 1,
                })
                .collect(),
            notes: Vec::new(),
            unpinned: Vec::new(),
            marks: Vec::new(),
        }
    }

    /// Room for every blob a test records.
    const ROOMY: Quotas = Quotas {
        entries_per_plugin: 10,
        lowered: std::collections::BTreeMap::new(),
        blobs: 10_000,
        writes_per_unit: 10,
        notes_per_unit: 10,
    };

    /// Letting a unit go deletes the bytes it alone made, and leaves the bytes
    /// this machine held before it and the bytes a committed unit's row names.
    #[tokio::test]
    async fn letting_a_unit_go_deletes_only_the_bytes_it_alone_made() {
        let (store, blobs) = (MemoryBackend::default(), MemoryBlobs::default());
        let fresh = written(&blobs, &[b"fresh"]).await.hash;
        let before = written(&blobs, &[b"before"]).await.hash;
        let named = written(&blobs, &[b"named"]).await.hash;
        store
            .commit(&recording(1, &[&named]), &ROOMY)
            .await
            .expect("a row names it");
        let made = |hash: &Hash, kept_before| Made {
            hash: hash.clone(),
            size: 1,
            kept_before,
            pinned_before: false,
        };
        let_go(
            &store,
            &blobs,
            &[
                made(&fresh, false),
                made(&before, true),
                made(&named, false),
            ],
        )
        .await;
        assert_eq!(
            blobs.size(&fresh).await,
            Ok(None),
            "the unit's own bytes stay"
        );
        assert_eq!(
            blobs.size(&before).await,
            Ok(Some(6)),
            "bytes held before went"
        );
        assert_eq!(
            blobs.size(&named).await,
            Ok(Some(5)),
            "bytes a row names went"
        );
    }

    /// A sweep forgets a row no unit has held for its grace, and deletes its
    /// bytes only once nobody has named them for as long: bytes named again
    /// since — a unit in flight, of another realm, making them again — stay,
    /// and go with a later sweep, once their own grace has passed, as bytes no
    /// row names.
    #[tokio::test]
    async fn a_sweep_deletes_the_bytes_of_a_row_it_forgets_only_once_they_are_old() {
        let (store, blobs) = (MemoryBackend::default(), MemoryBlobs::default());
        let grace = Duration::from_millis(KERNEL.blob_orphan_grace_ms + 1);
        let old = written(&blobs, &[b"old"]).await.hash;
        let again = written(&blobs, &[b"again"]).await.hash;
        store
            .commit(&recording(2, &[&old, &again]), &ROOMY)
            .await
            .expect("recorded");
        for hash in [&old, &again] {
            assert!(
                store
                    .set_pinned(hash.as_str(), false, 0)
                    .await
                    .expect("unpinned")
            );
        }
        blobs.age(grace);
        assert!(written(&blobs, &[b"again"]).await.held_before);
        let later = KERNEL.blob_orphan_grace_ms + 1;
        assert_eq!(
            sweep(&store, &blobs, later).await.expect("swept"),
            Swept {
                forgotten: 2,
                orphans: 0,
                unfinished: 0
            }
        );
        assert_eq!(blobs.size(&old).await, Ok(None));
        assert_eq!(
            blobs.size(&again).await,
            Ok(Some(5)),
            "bytes named since their row's grace began were deleted with it"
        );
        blobs.age(grace);
        assert_eq!(
            sweep(&store, &blobs, later).await.expect("swept"),
            Swept {
                forgotten: 0,
                orphans: 1,
                unfinished: 0
            }
        );
        assert_eq!(blobs.size(&again).await, Ok(None));
    }

    /// A sweep reads page after page until none is left: more unpinned blobs
    /// than one page holds are every one forgotten in one sweep.
    #[tokio::test]
    async fn a_sweep_forgets_every_page_of_what_is_due() {
        let store = MemoryBackend::default();
        let due: Vec<Hash> = (0..=super::SWEPT_AT_ONCE)
            .map(|n| Hash::of(&n.to_le_bytes()))
            .collect();
        let named: Vec<&Hash> = due.iter().collect();
        store
            .commit(&recording(2, &named), &ROOMY)
            .await
            .expect("recorded");
        for hash in &due {
            assert!(
                store
                    .set_pinned(hash.as_str(), false, 0)
                    .await
                    .expect("unpinned")
            );
        }
        let swept = sweep(
            &store,
            &MemoryBlobs::default(),
            KERNEL.blob_orphan_grace_ms + 1,
        )
        .await
        .expect("swept");
        assert_eq!(
            swept,
            Swept {
                forgotten: u64::from(super::SWEPT_AT_ONCE) + 1,
                orphans: 0,
                unfinished: 0
            }
        );
        assert_eq!(
            store
                .unpinned_before(u64::MAX >> 1, 10)
                .await
                .expect("listed"),
            [] as [String; 0]
        );
    }

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "nc-kernel-blob-store-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir(&root).expect("a scratch directory");
            Self(root)
        }

        /// The blobs' directory inside it, which the store makes itself.
        fn blobs(&self) -> PathBuf {
            self.0.join("blobs")
        }
    }

    /// What `bytes` written in `pieces` ended as.
    ///
    /// **And the hash the writer held so far is the one it was finished
    /// under**, which is what the kernel reads a blob's row by before the
    /// bytes are named.
    async fn written(store: &impl BlobStore, pieces: &[&[u8]]) -> super::Finished {
        let mut writer = store.create().await.expect("a writing");
        for piece in pieces {
            let (back, wrote) = store.write(writer, piece.to_vec()).await;
            wrote.expect("written");
            writer = back;
        }
        let so_far = store.hash_so_far(&writer);
        assert_eq!(so_far, Hash::of(&pieces.concat()), "the hash so far");
        let finished = store.finish(writer).await.expect("finished");
        assert_eq!(
            finished.hash, so_far,
            "a writer was finished under another hash than the one it held so far"
        );
        finished
    }

    #[test]
    fn a_hash_is_sixty_four_lowercase_hex_characters_and_nothing_else() {
        let hash = "0123456789abcdef".repeat(4);
        assert_eq!(
            Hash::parse(&hash).map(|hash| hash.to_string()),
            Some(hash.clone())
        );
        for refused in [
            String::new(),
            hash[..63].to_owned(),
            format!("{hash}0"),
            hash.to_uppercase(),
            format!("{}g", &hash[..63]),
            format!("{}é", &hash[..62]),
            format!("../{}", &hash[..61]),
        ] {
            assert_eq!(Hash::parse(&refused), None, "{refused:?}");
        }
        assert_eq!(
            Hash::of(b"").as_str(),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
            "BLAKE3 of nothing"
        );
    }

    /// The bytes are written aside and named by their hash once finished.
    ///
    /// The directory and every file are private, and a hash held already is
    /// not written again.
    #[tokio::test]
    async fn a_writing_is_named_by_its_hash_only_once_it_is_whole() {
        use nc_platform::testing::{identity, is_as_created};

        let scratch = Scratch::new("named");
        let store = DirectoryBlobs::open(&scratch.blobs()).expect("opens");
        assert!(is_as_created(&scratch.blobs()), "the directory");
        let mut writer = store.create().await.expect("a writing");
        let (back, wrote) = store.write(writer, b"half".to_vec()).await;
        wrote.expect("written");
        writer = back;
        let unfinished: Vec<String> = super::names_in(&scratch.blobs()).expect("lists");
        assert_eq!(unfinished.len(), 1, "{unfinished:?}");
        assert!(unfinished[0].starts_with(WRITING), "{unfinished:?}");
        assert!(
            is_as_created(&scratch.blobs().join(&unfinished[0])),
            "the writing"
        );
        assert_eq!(
            store.hashes(Duration::ZERO).await.expect("lists"),
            [],
            "nothing is named yet"
        );
        let (back, wrote) = store.write(writer, b" and the rest".to_vec()).await;
        wrote.expect("written");
        let finished = store.finish(back).await.expect("finished");
        assert_eq!(finished.hash, Hash::of(b"half and the rest"));
        assert_eq!((finished.size, finished.held_before), (17, false));
        assert_eq!(
            store.hashes(Duration::ZERO).await.expect("lists"),
            core::slice::from_ref(&finished.hash)
        );
        assert_eq!(
            super::names_in(&scratch.blobs()).expect("lists"),
            [finished.hash.to_string()],
            "the writing is gone once it is named"
        );
        assert!(
            is_as_created(&scratch.blobs().join(finished.hash.as_str())),
            "the bytes named"
        );

        // The same bytes again are the same blob, and the file is not written
        // — the same file, the same bytes — though it is named again.
        let named = scratch.blobs().join(finished.hash.as_str());
        named_long_ago(&named);
        let before = std::fs::symlink_metadata(&named).expect("there");
        let was = identity(&named).expect("there");
        let again = written(&store, &[b"half and the rest"]).await;
        assert_eq!(
            (again.hash.clone(), again.held_before),
            (finished.hash.clone(), true)
        );
        let after = std::fs::symlink_metadata(&named).expect("there");
        assert_eq!(
            (identity(&named).expect("there"), after.len()),
            (was, before.len())
        );
        assert_eq!(std::fs::read(&named).expect("read"), b"half and the rest");
        assert!(
            after.modified().expect("a time") > before.modified().expect("a time"),
            "bytes held already are named again by a finish that finds them"
        );
        assert_eq!(
            store
                .hashes(Duration::from_millis(KERNEL.blob_orphan_grace_ms))
                .await
                .expect("lists"),
            [],
            "and are young again"
        );
        assert_eq!(super::names_in(&scratch.blobs()).expect("lists").len(), 1);
    }

    /// Names the file at `path` a grace and a second ago: bytes the sweep may
    /// take once no row names them.
    fn named_long_ago(path: &std::path::Path) {
        let long_ago = std::time::SystemTime::now()
            - Duration::from_millis(KERNEL.blob_orphan_grace_ms)
            - Duration::from_secs(1);
        std::fs::File::open(path)
            .and_then(|file| file.set_modified(long_ago))
            .expect("this test's own file is aged");
    }

    /// A runtime whose blocking pool has one thread, which runs the work handed
    /// to it in the order it was handed over.
    fn one_blocking_thread() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .build()
            .expect("a runtime")
    }

    /// A writing let go unfinished is deleted with its writer: on a thread of
    /// the blocking pool where a runtime runs, and at once where none does.
    ///
    /// **Not on the thread that lets it go**, the one every plugin runs on:
    /// closing and deleting a file wait on the disk as a read does. This pool
    /// has one thread, held busy while the writer is let go, so the writing is
    /// still there when the drop returns, and gone once the work handed over
    /// after it has run.
    #[test]
    fn a_writing_let_go_unfinished_is_deleted_off_the_thread_that_lets_it_go() {
        let scratch = Scratch::new("let-go");
        let store = DirectoryBlobs::open(&scratch.blobs()).expect("opens");
        let left = || super::names_in(&scratch.blobs()).expect("lists").len();
        let runtime = one_blocking_thread();
        runtime.block_on(async {
            let writer = store.create().await.expect("a writing");
            let (go, waiting) = std::sync::mpsc::channel::<()>();
            let busy = tokio::task::spawn_blocking(move || waiting.recv());
            drop(writer);
            assert_eq!(
                left(),
                1,
                "the writing was deleted on the thread that let it go"
            );
            go.send(()).expect("the busy thread waits");
            busy.await.expect("it ran").expect("it was let go on");
            tokio::task::spawn_blocking(|| ()).await.expect("it ran");
            assert_eq!(left(), 0, "a writing let go was not deleted");
        });
        // With no runtime running, the drop deletes it itself.
        let writer = runtime.block_on(store.create()).expect("a writing");
        assert_eq!(left(), 1);
        drop(writer);
        assert_eq!(
            left(),
            0,
            "a writing let go where no runtime runs was not deleted"
        );
    }

    /// A file under a hash's name that is not those bytes is not that blob,
    /// and a finished writing takes its place.
    #[tokio::test]
    async fn bytes_under_a_hash_that_are_not_its_bytes_are_replaced() {
        let scratch = Scratch::new("replaced");
        let store = DirectoryBlobs::open(&scratch.blobs()).expect("opens");
        let hash = Hash::of(b"the real bytes");
        std::fs::write(scratch.blobs().join(hash.as_str()), b"short").expect("planted");
        let finished = written(&store, &[b"the real bytes"]).await;
        assert_eq!(
            (finished.hash.clone(), finished.held_before),
            (hash.clone(), false)
        );
        assert_eq!(
            std::fs::read(scratch.blobs().join(hash.as_str())).expect("there"),
            b"the real bytes"
        );
    }

    /// A finish whose directory refuses its sync is refused, and the bytes it
    /// named before the refusal stay under their hash, which the sweep deletes
    /// as bytes no row names.
    ///
    /// **The exception `finish` documents**: a refused finish may leave held
    /// under the hash what was not held before. A directory its owner may write
    /// in and enter but not list lets the rename through, and refuses the
    /// handle the directory is synced through.
    ///
    /// **Where no directory is synced — Windows — there is no such refusal to
    /// stage**, and the same finish, in the same directory, is kept: what is
    /// asserted there is that the bytes are named and nothing is refused,
    /// since a refusal could only come from a step that does not run.
    #[tokio::test]
    async fn bytes_named_before_their_directory_refused_its_sync_are_left_for_the_sweep() {
        let scratch = Scratch::new("unsynced");
        let store = DirectoryBlobs::open(&scratch.blobs()).expect("opens");
        let writer = store.create().await.expect("a writing");
        let (writer, wrote) = store.write(writer, b"named, not synced".to_vec()).await;
        wrote.expect("written");
        nc_platform::testing::hide_its_entries(&scratch.blobs())
            .expect("this test made the directory it is shutting");
        let finished = store.finish(writer).await;
        // Listed again before anything is asserted, so the scratch directory
        // can be removed however this ends.
        nc_platform::testing::make_private(&scratch.blobs()).expect("private again");
        let hash = Hash::of(b"named, not synced");
        if !nc_platform::SYNCS_A_DIRECTORY {
            let kept = finished.expect("nothing after the rename can refuse a finish here");
            assert_eq!((kept.hash, kept.size), (hash.clone(), 17));
            assert_eq!(store.size(&hash).await, Ok(Some(17)));
            return;
        }
        let refused = finished.expect_err(
            "a finish was kept in a directory that refused its sync: this test must run as a \
             user its mode binds, not the superuser",
        );
        assert_eq!(refused.code(), nc_catalog::ErrorCode::BlobIo);
        assert!(
            refused
                .to_string()
                .contains(&format!("syncing `{}`", scratch.blobs().display())),
            "{refused}"
        );
        assert_eq!(
            super::names_in(&scratch.blobs()).expect("lists"),
            [hash.to_string()],
            "the bytes named before the refusal are not held under their hash"
        );
        assert_eq!(store.size(&hash).await, Ok(Some(17)));
        assert_eq!(
            sweep(&MemoryBackend::default(), &store, 0)
                .await
                .expect("swept"),
            Swept::default(),
            "bytes named less than a grace ago are no orphan yet"
        );
        named_long_ago(&scratch.blobs().join(hash.as_str()));
        assert_eq!(
            sweep(&MemoryBackend::default(), &store, 0)
                .await
                .expect("swept"),
            Swept {
                forgotten: 0,
                orphans: 1,
                unfinished: 0
            }
        );
        assert_eq!(store.size(&hash).await, Ok(None));
    }

    /// A read is bounded by what it asks for and by the end.
    ///
    /// A hash the store does not hold is none, and a name that is a directory
    /// or a link is not a blob's bytes.
    #[tokio::test]
    async fn a_read_answers_what_it_asked_for_up_to_the_end() {
        let scratch = Scratch::new("read");
        let store = DirectoryBlobs::open(&scratch.blobs()).expect("opens");
        let finished = written(&store, &[b"0123456789"]).await;
        assert_eq!(store.size(&finished.hash).await, Ok(Some(10)));
        assert_eq!(
            store.read(&finished.hash, 2, 3).await,
            Ok(Some(b"234".to_vec()))
        );
        assert_eq!(
            store.read(&finished.hash, 8, 100).await,
            Ok(Some(b"89".to_vec()))
        );
        assert_eq!(
            store.read(&finished.hash, 10, 100).await,
            Ok(Some(Vec::new()))
        );
        assert_eq!(
            store.read(&finished.hash, 99, 100).await,
            Ok(Some(Vec::new()))
        );
        let absent = Hash::of(b"never written");
        assert_eq!(store.size(&absent).await, Ok(None));
        assert_eq!(store.read(&absent, 0, 1).await, Ok(None));

        let directory = Hash::of(b"a directory");
        std::fs::create_dir(scratch.blobs().join(directory.as_str())).expect("planted");
        let link = Hash::of(b"a link");
        nc_platform::testing::link_file(
            &scratch.blobs().join(finished.hash.as_str()),
            &scratch.blobs().join(link.as_str()),
        )
        .expect("planted");
        for planted in [&directory, &link] {
            let refused = store.read(planted, 0, 1).await.expect_err("not bytes");
            assert_eq!(refused.code(), nc_catalog::ErrorCode::BlobIo);
            assert!(
                refused.to_string().contains("not a regular file"),
                "{refused}"
            );
        }
    }

    /// A store removes what it holds and says so, and forgets every writing no
    /// writer holds — and none a writer does.
    ///
    /// On a blocking pool of one thread, so the writing its writer's drop
    /// deletes is gone before the next call of the store runs.
    #[test]
    fn a_writing_no_writer_holds_is_forgotten_and_one_held_is_not() {
        one_blocking_thread().block_on(async {
            let scratch = Scratch::new("unfinished");
            let store = DirectoryBlobs::open(&scratch.blobs()).expect("opens");
            let finished = written(&store, &[b"kept"]).await;
            let held = store.create().await.expect("a writing");
            std::fs::write(
                scratch.blobs().join(format!("{WRITING}0-0-0")),
                b"a stopped process's",
            )
            .expect("planted");
            std::fs::write(scratch.blobs().join("a-note-of-someone-else"), b"").expect("planted");
            assert_eq!(store.forget_unfinished().await, Ok(1));
            assert_eq!(super::names_in(&scratch.blobs()).expect("lists").len(), 3);
            drop(held);
            assert_eq!(store.forget_unfinished().await, Ok(0));
            assert_eq!(store.remove(&finished.hash).await, Ok(true));
            assert_eq!(store.remove(&finished.hash).await, Ok(false));
            assert_eq!(
                super::names_in(&scratch.blobs()).expect("lists"),
                ["a-note-of-someone-else"],
                "what is not a blob or a writing is not the store's to delete"
            );
        });
    }

    /// A directory another user could reach is refused, and left as it was.
    #[test]
    fn a_directory_another_user_can_reach_is_refused_and_left_as_it_was() {
        use nc_platform::testing;

        let scratch = Scratch::new("refused");
        std::fs::create_dir(scratch.blobs()).expect("made");
        testing::make_private(&scratch.blobs()).expect("private");
        let said = testing::open_to_another_user(&scratch.blobs()).expect("opened up");
        let before = testing::access(&scratch.blobs()).expect("readable");
        let refused = DirectoryBlobs::open(&scratch.blobs()).expect_err("refused");
        assert_eq!(refused.code(), nc_catalog::ErrorCode::DbLocalOpenRefused);
        assert!(
            refused.to_string().contains("so the blobs were not opened")
                && refused.to_string().contains(&said),
            "{said}: {refused}"
        );
        assert_eq!(
            testing::access(&scratch.blobs()).expect("readable"),
            before,
            "a refused directory was changed"
        );
    }

    /// The store held in memory answers one script as the directory does.
    #[tokio::test]
    async fn the_two_stores_answer_one_script_alike() {
        let scratch = Scratch::new("alike");
        let directory = DirectoryBlobs::open(&scratch.blobs()).expect("opens");
        assert_eq!(
            script(&directory).await,
            script(&MemoryBlobs::default()).await
        );
    }

    /// Every call a store offers, once each way, written down.
    async fn script(store: &impl BlobStore) -> Vec<String> {
        let mut said = Vec::new();
        let first = written(store, &[b"one", b" two"]).await;
        said.push(format!("{first:?}"));
        said.push(format!("{:?}", written(store, &[b"one two"]).await));
        said.push(format!("{:?}", written(store, &[]).await));
        said.push(format!("{:?}", store.size(&first.hash).await));
        for (offset, max) in [(0, 3), (3, 100), (7, 1), (8, 1), (0, 0)] {
            said.push(format!("{:?}", store.read(&first.hash, offset, max).await));
        }
        let absent = Hash::of(b"absent");
        said.push(format!("{:?}", store.size(&absent).await));
        said.push(format!("{:?}", store.read(&absent, 0, 1).await));
        said.push(format!("{:?}", store.hashes(Duration::ZERO).await));
        said.push(format!("{:?}", store.remove(&first.hash).await));
        said.push(format!("{:?}", store.remove(&first.hash).await));
        said.push(format!("{:?}", store.hashes(Duration::ZERO).await));
        drop(store.create().await.expect("a writing"));
        said.push(format!("{:?}", store.forget_unfinished().await));
        said
    }

    /// A store holds the name of no writing that has ended — named, kept as
    /// bytes held already, or let go — so what it holds is only the writings in
    /// progress.
    ///
    /// **No name past its writing's end**: a name kept past it would leave the
    /// store holding one name for every blob ever written, for the life of the
    /// daemon, which this test refuses.
    #[test]
    fn a_store_holds_the_name_of_no_writing_that_has_ended() {
        one_blocking_thread().block_on(async {
            let scratch = Scratch::new("names");
            let store = DirectoryBlobs::open(&scratch.blobs()).expect("opens");
            written(&store, &[b"once"]).await;
            written(&store, &[b"once"]).await;
            drop(store.create().await.expect("a writing"));
            // The pool runs what it is handed in order, so the deletion the
            // drop handed over has run once this has.
            tokio::task::spawn_blocking(|| ()).await.expect("it ran");
            let names = super::held(&store.writing).clone();
            assert!(names.is_empty(), "ended writings are still held: {names:?}");
        });
    }

    /// A blob's note is read back as the pin its unit's reversal takes off,
    /// and a note of another kind is passed over.
    #[test]
    fn a_blob_no_unit_held_before_is_the_one_a_reversal_unpins() {
        let made = |fill: &[u8], pinned_before| Made {
            hash: Hash::of(fill),
            size: 1,
            kept_before: false,
            pinned_before,
        };
        let fresh = made(b"fresh", false);
        let held = made(b"held", true);
        assert_eq!(
            note(&fresh),
            format!(
                r#"{{"hash":"{}","note":"blob_created","pinned_before":false}}"#,
                fresh.hash
            )
        );
        assert_eq!(
            unpinned_by(&[
                note(&fresh),
                r#"{"note":"authorized"}"#.to_owned(),
                note(&held)
            ]),
            Ok(vec![fresh.hash.to_string()])
        );
        for unreadable in [
            r#"{"note":"blob_created"}"#.to_owned(),
            r#"{"hash":"nothing","note":"blob_created","pinned_before":false}"#.to_owned(),
            format!(
                r#"{{"hash":"{}","note":"blob_created","pinned_before":0}}"#,
                fresh.hash
            ),
            format!(
                r#"{{"also":1,"hash":"{}","note":"blob_created","pinned_before":false}}"#,
                fresh.hash
            ),
        ] {
            assert!(
                matches!(
                    unpinned_by(core::slice::from_ref(&unreadable)),
                    Err(nc_local::LocalError::Unexpected(_))
                ),
                "{unreadable}"
            );
        }
    }
}
