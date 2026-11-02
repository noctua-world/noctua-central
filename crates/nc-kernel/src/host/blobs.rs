//! `blobs`: content-addressed files, read and written as streams.
//!
//! Two capabilities divide the interface (`catalog/capabilities.toml`):
//! `blobs.read` grants `locate`, `open` and every method of a `reader`, and
//! `blobs.write` grants `create`, `announce` and every method of a `writer`.
//! Every function asks its capability first and answers `denied` to a plugin
//! not granted it.
//!
//! **Where a blob is, honestly.** `locate` answers `local` for a blob on this
//! machine — its row pinned by a committed unit, or made by the unit of the
//! call tree asking, and its bytes in the store (`crate::blobs`) — and
//! `unavailable` for every other, and **never `remote`**: a blob served by
//! another machine is reached over the network between a person's devices,
//! which `NOCTUA-PLAN.md` builds in Stage 25, and a kernel with no peer to ask
//! cannot say one has it. `open` of a blob that is not local is refused
//! `unavailable` (`[NC-BLOB-0002]`) at once — the world's comment says the
//! kernel fetches it, and this one has nowhere to fetch from, so it waits on
//! nothing — and `announce` is refused `unavailable` (`[NC-BLOB-0006]`): there
//! is no peer to tell, and a call that answered success would be a lie the
//! plugin builds on. A hash is sixty-four lowercase hexadecimal characters,
//! and any other text is `invalid` (`[NC-BLOB-0001]`) before anything is read.
//!
//! **A reader streams, in bounded chunks.** Each `read` answers at most
//! `blob_read_chunk_bytes`, whatever it asked for, so a large file never
//! enters the plugin's memory whole; `seek` moves where the next read begins,
//! to at most the end; `size` is the blob's.
//!
//! **A writer writes only where a command changes state.** `create`, `write`
//! and `finish` run only in the frame of a command that changes state, whose
//! unit the blob becomes part of — anywhere else they are refused with
//! `[NC-PLUG-0024]` — and a blob is at most `blob_bytes` (`[NC-BLOB-0003]`,
//! the write refused and nothing of it written). `finish` answers the hash,
//! and the writer takes nothing more after it (`[NC-BLOB-0005]`); one a disk
//! refused is broken and keeps nothing (`[NC-BLOB-0004]`). `finish` reads the
//! blob's row by the hash the writer holds so far before the bytes are named,
//! so a row refused or never answered lets the writing go with nothing named.
//! The one exception is the store's own `finish` failing after it named the
//! bytes — its directory refused the sync — or abandoned at the deadline while
//! the thread it runs on goes on to name them: they stay under their hash, the
//! unit records nothing of them, and the sweep deletes them unless a row names
//! that hash. A plugin holds at most `blob_handles` readers and writers open
//! at once (`[NC-PLUG-0017]`). Every function that reads the disk or the store
//! waits under `host_call_deadline_ms`, past which the plugin waiting is
//! killed; a writer let go waits on nothing, its writing deleted on a thread
//! of the blocking pool (`crate::blobs`).
//!
//! **A method is reachable only through its constructor.** A reader or a
//! writer is a handle `open` or `create` handed out, under the capability its
//! methods answer to; `reader.size`, which has no `error` to refuse with,
//! refuses by trapping, and no plugin meets that, since it holds a reader only
//! where `blobs.read` was granted. A handle this kernel did not issue traps,
//! which only a fault of the host could bring about.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin told a blob is somewhere
//! when no machine the kernel can reach has it; a plugin waiting on a fetch
//! that will never come; a blob changing state outside a unit, or outliving a
//! unit let go; and a plugin holding the kernel's files or memory without
//! bound. What it does not answer: who may read a blob, which is whoever holds
//! its hash and `blobs.read` — content-addressed bytes carry no owner, and a
//! hash is a capability to them (`NOCTUA.md §13.3`); and how much disk the
//! blobs a plugin may write take, within `blob_bytes` each.

use std::collections::BTreeMap;
use std::sync::Arc;

use nc_catalog::{Capability, ErrorCode, KERNEL};
use wasmtime::component::Resource;

use super::{HostState, refused};
use crate::Environment;
use crate::backend::StoreBackend as _;
use crate::bindings::KernelError;
use crate::bindings::noctua::kernel::blobs as world;
use crate::blobs::{BlobStore, Hash, Made};
use crate::dispatch::{Refusal, RefusalKind};
use crate::supervisor::in_time;
use crate::units;

/// One reader a plugin holds: the blob it reads, its size, and where the next
/// read begins.
#[derive(Debug, Clone)]
struct Reading {
    hash: Hash,
    size: u64,
    at: u64,
}

/// One writer a plugin holds, and how far it has got.
#[derive(Debug)]
enum Writing<W> {
    /// Taking bytes: the store's writer — away while a call to the store holds
    /// it — and how many bytes it holds.
    Open { writer: Option<W>, size: u64 },
    /// Finished, as the blob it named.
    Finished(Hash),
    /// Broken by what the disk refused: nothing it held can be kept.
    Broken(String),
}

/// Every reader and writer one plugin holds open, by the handle each was
/// given.
///
/// **Bounded by the catalog**: a writer holds a file open in the kernel's
/// process and every handle a record in host memory, outside the linear
/// memory the plugin's own bound counts, so at most `blob_handles` are held at
/// once.
#[derive(Debug)]
pub(crate) struct Handles<W> {
    readers: BTreeMap<u32, Reading>,
    writers: BTreeMap<u32, Writing<W>>,
    next: u32,
}

impl<W> Default for Handles<W> {
    fn default() -> Self {
        Self {
            readers: BTreeMap::new(),
            writers: BTreeMap::new(),
            next: 0,
        }
    }
}

impl<W> Handles<W> {
    /// Refuses one more handle when the plugin holds as many as it may.
    fn room(&self) -> Result<(), Refusal> {
        let held = self.readers.len().saturating_add(self.writers.len());
        if u64::try_from(held).unwrap_or(u64::MAX) < u64::from(KERNEL.blob_handles) {
            return Ok(());
        }
        Err(Refusal::new(
            RefusalKind::Unavailable,
            ErrorCode::PluginBoundReached,
            format!(
                "a plugin holds at most {} blob readers and writers open at once, and this \
                 one would be one more",
                KERNEL.blob_handles
            ),
        ))
    }

    /// A handle neither a reader nor a writer of this plugin holds now.
    fn fresh(&mut self) -> u32 {
        loop {
            let handle = self.next;
            self.next = self.next.wrapping_add(1);
            if !self.readers.contains_key(&handle) && !self.writers.contains_key(&handle) {
                return handle;
            }
        }
    }
}

impl<E: Environment> world::Host for HostState<E> {
    async fn locate(&mut self, h: String) -> Result<world::Availability, KernelError> {
        self.require(Capability::BlobsRead, "locate")?;
        let hash = hash_of(&h).map_err(|refusal| refused(&refusal))?;
        let here = in_time("blobs#locate", self.left(), self.located(&hash)).await?;
        // **Never `remote`**: no peer is asked in this build, so none can be
        // said to have it (the header says why).
        Ok(if here.is_some() {
            world::Availability::Local
        } else {
            world::Availability::Unavailable
        })
    }

    async fn open(&mut self, h: String) -> Result<Resource<world::Reader>, KernelError> {
        self.require(Capability::BlobsRead, "open")?;
        let hash = hash_of(&h).map_err(|refusal| refused(&refusal))?;
        self.handles.room().map_err(|refusal| refused(&refusal))?;
        let Some(size) = in_time("blobs#open", self.left(), self.located(&hash)).await? else {
            return Err(refused(&not_here(&hash)));
        };
        let handle = self.handles.fresh();
        self.handles
            .readers
            .insert(handle, Reading { hash, size, at: 0 });
        Ok(Resource::new_own(handle))
    }

    async fn create(&mut self) -> Result<Resource<world::Writer>, KernelError> {
        self.require(Capability::BlobsWrite, "create")?;
        self.writes_here()
            .map_err(|refusal| self.refused_here("blobs#create", &refusal))?;
        self.handles.room().map_err(|refusal| refused(&refusal))?;
        let blobs = Arc::clone(&self.blobs);
        let writer = in_time("blobs#create", self.left(), async move {
            blobs
                .create()
                .await
                .map_err(|failed| refused(&failed.refusal()))
        })
        .await?;
        let handle = self.handles.fresh();
        self.handles.writers.insert(
            handle,
            Writing::Open {
                writer: Some(writer),
                size: 0,
            },
        );
        Ok(Resource::new_own(handle))
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the world declares it async, and it answers without asking anything: there \
                  is no peer to tell in this build"
    )]
    async fn announce(&mut self, h: String) -> Result<(), KernelError> {
        self.require(Capability::BlobsWrite, "announce")?;
        let hash = hash_of(&h).map_err(|refusal| refused(&refusal))?;
        Err(refused(&Refusal::new(
            RefusalKind::Unavailable,
            ErrorCode::BlobNoPeers,
            format!(
                "`{hash}` was announced to no peer: this build of the kernel reaches none, so no \
                 other machine was told this one holds it"
            ),
        )))
    }
}

impl<E: Environment> world::HostReader for HostState<E> {
    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the world declares it async, and a reader's size is the one it was opened \
                  with, held here"
    )]
    async fn size(&mut self, reader: Resource<world::Reader>) -> wasmtime::Result<u64> {
        self.require_or_trap(Capability::BlobsRead, "[method]reader.size")?;
        Ok(self.reading(&reader, "[method]reader.size")?.size)
    }

    async fn read(
        &mut self,
        reader: Resource<world::Reader>,
        max: u32,
    ) -> Result<Vec<u8>, KernelError> {
        self.require(Capability::BlobsRead, "[method]reader.read")?;
        let reading = self.reading(&reader, "[method]reader.read")?.clone();
        // **At most a chunk, whatever was asked**, and never past the end.
        let chunk = u64::from(max)
            .min(KERNEL.blob_read_chunk_bytes)
            .min(reading.size.saturating_sub(reading.at));
        if chunk == 0 {
            return Ok(Vec::new());
        }
        let blobs = Arc::clone(&self.blobs);
        let (hash, at) = (reading.hash.clone(), reading.at);
        let bytes = in_time("blobs#[method]reader.read", self.left(), async move {
            blobs
                .read(&hash, at, usize::try_from(chunk).unwrap_or(usize::MAX))
                .await
                .map_err(|failed| refused(&failed.refusal()))
        })
        .await?;
        let Some(bytes) = bytes else {
            return Err(refused(&Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::BlobNotLocal,
                format!("`{}` is no longer on this machine", reading.hash),
            )));
        };
        if let Some(open) = self.handles.readers.get_mut(&reader.rep()) {
            open.at = at.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        }
        Ok(bytes)
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the world declares it async, and a seek moves where the next read begins, \
                  which reads nothing"
    )]
    async fn seek(
        &mut self,
        reader: Resource<world::Reader>,
        offset: u64,
    ) -> Result<(), KernelError> {
        self.require(Capability::BlobsRead, "[method]reader.seek")?;
        let size = self.reading(&reader, "[method]reader.seek")?.size;
        if offset > size {
            return Err(refused(&Refusal::new(
                RefusalKind::Invalid,
                ErrorCode::ValidationOutOfRange,
                format!(
                    "a reader of {size} bytes seeks to at most {size}, and this one to {offset}"
                ),
            )));
        }
        if let Some(open) = self.handles.readers.get_mut(&reader.rep()) {
            open.at = offset;
        }
        Ok(())
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the world declares it async, and a reader holds nothing a drop waits on"
    )]
    async fn drop(&mut self, reader: Resource<world::Reader>) -> wasmtime::Result<()> {
        self.release(Capability::BlobsRead)?;
        self.handles.readers.remove(&reader.rep());
        Ok(())
    }
}

impl<E: Environment> world::HostWriter for HostState<E> {
    async fn write(
        &mut self,
        writer: Resource<world::Writer>,
        bytes: Vec<u8>,
    ) -> Result<(), KernelError> {
        self.require(Capability::BlobsWrite, "[method]writer.write")?;
        self.writes_here()
            .map_err(|refusal| self.refused_here("blobs#[method]writer.write", &refusal))?;
        let (taken, size) = self.taken(&writer, "[method]writer.write")?;
        let after = size.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if after > KERNEL.blob_bytes {
            self.put_back(&writer, taken, size);
            return Err(refused(&Refusal::new(
                RefusalKind::Invalid,
                ErrorCode::BlobTooLarge,
                format!(
                    "a blob is at most {} bytes, and this write would make it {after}, so \
                     nothing of it was written",
                    KERNEL.blob_bytes
                ),
            )));
        }
        let blobs = Arc::clone(&self.blobs);
        let (back, wrote) = in_time("blobs#[method]writer.write", self.left(), async move {
            Ok(blobs.write(taken, bytes).await)
        })
        .await?;
        match wrote {
            Ok(()) => {
                self.put_back(&writer, back, after);
                Ok(())
            }
            Err(failed) => {
                drop(back);
                self.handles
                    .writers
                    .insert(writer.rep(), Writing::Broken(failed.to_string()));
                Err(refused(&failed.refusal()))
            }
        }
    }

    async fn finish(&mut self, writer: Resource<world::Writer>) -> Result<String, KernelError> {
        self.require(Capability::BlobsWrite, "[method]writer.finish")?;
        self.writes_here()
            .map_err(|refusal| self.refused_here("blobs#[method]writer.finish", &refusal))?;
        self.room_for_a_blob()
            .map_err(|refusal| refused(&refusal))?;
        let (taken, _) = self.taken(&writer, "[method]writer.finish")?;
        // The writer is spent whatever the store answers: named, or broken.
        self.handles.writers.insert(
            writer.rep(),
            Writing::Broken("its writing ended in the call that finished it".to_owned()),
        );
        let (blobs, store) = (Arc::clone(&self.blobs), Arc::clone(&self.store));
        // **The row is read before the bytes are named**, by the hash the
        // writer holds so far. The unit records the blob only once both have
        // answered, so a row refused or never answered after the bytes were
        // named left them under a hash no row names, which the unit let go did
        // not know of. Read first, a refusal or the deadline drops the writer
        // instead, and that deletes the writing.
        let named = blobs.hash_so_far(&taken);
        let made = in_time("blobs#[method]writer.finish", self.left(), async move {
            let row = store
                .blob(named.as_str())
                .await
                .map_err(|failed| refused(&units::store_failed(&failed)))?;
            let finished = blobs
                .finish(taken)
                .await
                .map_err(|failed| refused(&failed.refusal()))?;
            Ok(Made {
                kept_before: finished.held_before || row.is_some(),
                pinned_before: row.is_some_and(|row| row.pinned),
                hash: finished.hash,
                size: finished.size,
            })
        })
        .await?;
        let hash = made.hash.clone();
        if let Some(frame) = self.frame()
            && let Some(unit) = frame.tree().unit_mut()
        {
            unit.make(made);
        }
        self.handles
            .writers
            .insert(writer.rep(), Writing::Finished(hash.clone()));
        Ok(hash.to_string())
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the world declares it async, and the writing a writer holds is deleted \
                  when the writer is, on a thread of the blocking pool nothing here waits for"
    )]
    async fn drop(&mut self, writer: Resource<world::Writer>) -> wasmtime::Result<()> {
        self.release(Capability::BlobsWrite)?;
        self.handles.writers.remove(&writer.rep());
        Ok(())
    }
}

impl<E: Environment> HostState<E> {
    /// Where the blob `hash` is for this plugin now: its size when it is on
    /// this machine, and nothing when it is not.
    ///
    /// **On this machine** is a row a committed unit pins, or a blob the unit
    /// of the call tree asking made, with its bytes in the store either way: a
    /// row whose bytes are gone is not a blob this machine can read, and bytes
    /// no row and no unit hold are what the sweep deletes.
    async fn located(&self, hash: &Hash) -> Result<Option<u64>, KernelError> {
        if !self.made_here(hash) {
            let row = self
                .store
                .blob(hash.as_str())
                .await
                .map_err(|failed| refused(&units::store_failed(&failed)))?;
            if !row.is_some_and(|row| row.pinned) {
                return Ok(None);
            }
        }
        self.blobs
            .size(hash)
            .await
            .map_err(|failed| refused(&failed.refusal()))
    }

    /// Whether the unit of the call tree running now made `hash`.
    fn made_here(&self, hash: &Hash) -> bool {
        self.frame()
            .is_some_and(|frame| frame.tree().unit().is_some_and(|unit| unit.made_blob(hash)))
    }

    /// Refuses a blob's write anywhere but in a command that changes state.
    fn writes_here(&self) -> Result<(), Refusal> {
        let held = self
            .frame()
            .is_some_and(|frame| frame.mutating() && frame.tree().unit().is_some());
        if held {
            return Ok(());
        }
        Err(Refusal::new(
            RefusalKind::Denied,
            ErrorCode::PluginReadOnlyFrame,
            "a blob is written only in a command that changes state: `activate`, a callback \
             and a command that only reads write nothing themselves — a timer's callback \
             changes state only through the commands it dispatches"
                .to_owned(),
        ))
    }

    /// Refuses a blob more when the tree's unit holds as many as it may.
    fn room_for_a_blob(&self) -> Result<(), Refusal> {
        self.frame().map_or(Ok(()), |frame| {
            frame
                .tree()
                .unit()
                .map_or(Ok(()), units::Ambient::room_for_a_blob)
        })
    }

    /// The reader `reader` names.
    ///
    /// # Errors
    ///
    /// [`wasmtime::Error`] when this kernel did not issue it, a trap the
    /// plugin cannot have brought about.
    fn reading(
        &self,
        reader: &Resource<world::Reader>,
        function: &'static str,
    ) -> wasmtime::Result<&Reading> {
        self.handles
            .readers
            .get(&reader.rep())
            .ok_or_else(|| not_issued(function))
    }

    /// The store's writer that `writer` names, taken out of it, and how many
    /// bytes it holds.
    ///
    /// # Errors
    ///
    /// `invalid` with `[NC-BLOB-0005]` for a writer that has finished,
    /// `unavailable` with `[NC-BLOB-0004]` for one a disk broke, and a trap
    /// for a handle this kernel did not issue.
    fn taken(
        &mut self,
        writer: &Resource<world::Writer>,
        function: &'static str,
    ) -> Result<(<E::Blobs as BlobStore>::Writer, u64), KernelError> {
        match self.handles.writers.get_mut(&writer.rep()) {
            Some(Writing::Open { writer, size }) => match writer.take() {
                Some(taken) => Ok((taken, *size)),
                None => Err(refused(&broken("its last write did not end"))),
            },
            Some(Writing::Finished(hash)) => Err(refused(&Refusal::new(
                RefusalKind::Invalid,
                ErrorCode::BlobWriterFinished,
                format!(
                    "the writer of `{hash}` has finished, and a finished writer takes nothing more"
                ),
            ))),
            Some(Writing::Broken(why)) => Err(refused(&broken(why))),
            None => Err(KernelError::Trap(not_issued(function))),
        }
    }

    /// Puts the store's writer back under `writer`, holding `size` bytes.
    fn put_back(
        &mut self,
        writer: &Resource<world::Writer>,
        taken: <E::Blobs as BlobStore>::Writer,
        size: u64,
    ) {
        self.handles.writers.insert(
            writer.rep(),
            Writing::Open {
                writer: Some(taken),
                size,
            },
        );
    }
}

/// The hash `text` is.
///
/// # Errors
///
/// `invalid` with `[NC-BLOB-0001]` for text that is not one, repeated cut to a
/// bound and escaped.
fn hash_of(text: &str) -> Result<Hash, Refusal> {
    Hash::parse(text).ok_or_else(|| {
        Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::BlobMalformedHash,
            format!(
                "{} is not a blob's hash: its BLAKE3, sixty-four lowercase hexadecimal \
                 characters",
                crate::registry::quoted_up_to(text, HASH_QUOTED)
            ),
        )
    })
}

/// How many characters of text a plugin named as a hash a refusal repeats: a
/// hash is sixty-four, and anything longer is not one.
const HASH_QUOTED: usize = 80;

/// The refusal of a blob that is not on this machine.
fn not_here(hash: &Hash) -> Refusal {
    Refusal::new(
        RefusalKind::Unavailable,
        ErrorCode::BlobNotLocal,
        format!(
            "`{hash}` is not on this machine, and this build of the kernel fetches no blob from \
             another"
        ),
    )
}

/// The refusal of a writer a disk broke.
fn broken(why: &str) -> Refusal {
    Refusal::new(
        RefusalKind::Unavailable,
        ErrorCode::BlobIo,
        format!("the writer was broken and keeps nothing: {why}"),
    )
}

/// The trap of a method called on a handle this kernel never handed out.
fn not_issued(function: &'static str) -> wasmtime::Error {
    wasmtime::Error::msg(format!(
        "`blobs#{function}` was called on a handle this kernel did not issue"
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wasmtime::component::Resource;

    use super::Writing;
    use crate::bindings::KernelError;
    use crate::bindings::noctua::kernel::blobs::{
        self as world, Host as _, HostReader as _, HostWriter as _,
    };
    use crate::bindings::noctua::kernel::types;
    use crate::blobs::{BlobError, BlobStore, Finished, Hash, Made, MemoryBlobs};
    use crate::capability::GrantSet;
    use crate::dispatch::{Call, Frame};
    use crate::host::HostState;
    use crate::limits::{Limits, MemoryGuard};
    use crate::{Environment, InMemory};
    use nc_catalog::{Capability, ErrorCode, KERNEL};

    /// A unit, minted for a test.
    fn a_unit() -> nc_ids::Snowflake {
        nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        )
        .mint()
        .expect("a unit")
    }

    /// The plugin `tests:door`, granted both halves of the blobs, in `frame`
    /// when there is one.
    fn granted(frame: Option<Frame<InMemory>>) -> HostState<InMemory> {
        let mut state = HostState::in_memory(GrantSet::of([
            Capability::BlobsRead,
            Capability::BlobsWrite,
        ]));
        if let Some(frame) = frame {
            state.enter(frame);
        }
        state
    }

    /// The same, running a command that changes state in a unit of its own.
    fn writing() -> HostState<InMemory> {
        granted(Some(Frame::for_tests(true, Some(a_unit()))))
    }

    /// The case and the code of a refusal the world carries.
    fn refusal(answer: &Result<impl core::fmt::Debug, KernelError>) -> (String, String) {
        match answer {
            Err(KernelError::Refused(error)) => match error {
                types::Error::Denied(why) => ("denied".to_owned(), why.clone()),
                types::Error::NotFound(why) => ("not-found".to_owned(), why.clone()),
                types::Error::Invalid(why) => ("invalid".to_owned(), why.clone()),
                types::Error::Conflict(why) => ("conflict".to_owned(), why.clone()),
                types::Error::Unavailable(why) => ("unavailable".to_owned(), why.clone()),
                types::Error::Internal(why) => ("internal".to_owned(), why.clone()),
            },
            other => panic!("not a refusal: {other:?}"),
        }
    }

    /// Whether `answer` is a refusal of `case` opening on `code`.
    fn refused_as(
        answer: &Result<impl core::fmt::Debug, KernelError>,
        case: &str,
        code: ErrorCode,
    ) -> bool {
        let (said, why) = refusal(answer);
        said == case && why.starts_with(&format!("[{code}] "))
    }

    /// Makes a blob of `bytes` in `state`'s frame, and answers its hash and
    /// the writer that made it, finished and still held.
    async fn finished(state: &mut HostState<InMemory>, bytes: &[u8]) -> (String, u32) {
        let writer = state.create().await.expect("a writer");
        state
            .write(Resource::new_own(writer.rep()), bytes.to_vec())
            .await
            .expect("written");
        let hash = state
            .finish(Resource::new_own(writer.rep()))
            .await
            .expect("finished");
        (hash, writer.rep())
    }

    /// Makes a blob of `bytes` in `state`'s frame, lets its writer go, and
    /// answers its hash.
    async fn made(state: &mut HostState<InMemory>, bytes: &[u8]) -> String {
        let (hash, writer) = finished(state, bytes).await;
        world::HostWriter::drop(state, Resource::new_own(writer))
            .await
            .expect("let go");
        hash
    }

    #[tokio::test]
    async fn a_blob_is_written_only_where_a_command_changes_state() {
        for frame in [None, Some(Frame::for_tests(false, Some(a_unit())))] {
            let mut state = granted(frame);
            assert!(
                refused_as(
                    &state.create().await.map(drop),
                    "denied",
                    ErrorCode::PluginReadOnlyFrame
                ),
                "a writer made outside a command that changes state"
            );
            for answer in [
                state.write(Resource::new_own(0), vec![1]).await,
                state.finish(Resource::new_own(0)).await.map(drop),
            ] {
                assert!(
                    refused_as(&answer, "denied", ErrorCode::PluginReadOnlyFrame),
                    "{answer:?}"
                );
            }
        }
        // A writer made in one, then written where only reading is allowed,
        // is refused there too.
        let mut state = writing();
        let writer = state.create().await.expect("a writer");
        state.leave();
        state.enter(Frame::for_tests(false, Some(a_unit())));
        assert!(refused_as(
            &state.write(Resource::new_own(writer.rep()), vec![1]).await,
            "denied",
            ErrorCode::PluginReadOnlyFrame
        ));
    }

    #[tokio::test]
    async fn a_timers_callback_that_began_a_unit_writes_no_blob_itself_and_is_told_why() {
        // **A review found the old sentence false**: it said a callback runs
        // in no unit of work, and a timer's callback may begin one.
        let mut state = granted(Some(
            Frame::for_tests(false, Some(a_unit()))
                .calling_for_tests(Call::Timer { may_change: true }),
        ));
        let created = state.create().await.map(drop);
        assert!(
            refused_as(&created, "denied", ErrorCode::PluginReadOnlyFrame),
            "{created:?}"
        );
        let (_, why) = refusal(&created);
        assert!(
            why.ends_with(
                "write nothing themselves — a timer's callback changes state only through the \
                 commands it dispatches"
            ),
            "{why}"
        );
        assert!(
            state
                .frame()
                .expect("the frame is held")
                .tree()
                .unit()
                .is_some_and(|unit| unit.made().is_empty()),
            "a refused writer made a blob"
        );
    }

    /// A finished writer names its blob, which the frame's unit has made and
    /// which is on this machine for that unit's tree, and takes nothing more.
    #[tokio::test]
    async fn a_finished_writer_names_its_blob_in_the_unit_and_takes_nothing_more() {
        let mut state = writing();
        let (hash, writer) = finished(&mut state, b"a blob").await;
        assert_eq!(hash, Hash::of(b"a blob").as_str());
        let made_in_the_unit: Vec<Made> = state
            .frame()
            .and_then(|frame| frame.tree().unit().map(|unit| unit.made().to_vec()))
            .expect("the frame holds a unit");
        assert_eq!(
            made_in_the_unit,
            [Made {
                hash: Hash::of(b"a blob"),
                size: 6,
                kept_before: false,
                pinned_before: false,
            }]
        );
        assert_eq!(
            state.locate(hash.clone()).await.expect("located"),
            world::Availability::Local
        );
        for answer in [
            state.write(Resource::new_own(writer), vec![1]).await,
            state.finish(Resource::new_own(writer)).await.map(drop),
        ] {
            assert!(
                refused_as(&answer, "invalid", ErrorCode::BlobWriterFinished),
                "{answer:?}"
            );
        }
        // The same bytes again are the same blob, made once.
        assert_eq!(made(&mut state, b"a blob").await, hash);
        assert_eq!(
            state
                .frame()
                .and_then(|frame| frame.tree().unit().map(|unit| unit.made().len())),
            Some(1)
        );
        // Outside the tree that made it, a blob no committed unit keeps is not
        // on this machine.
        state.leave();
        assert_eq!(
            state.locate(hash).await.expect("located"),
            world::Availability::Unavailable
        );
    }

    /// A blob grows to `blob_bytes` and not a byte past, and the write that
    /// would take it past is refused whole: the writer goes on as it was.
    #[tokio::test]
    async fn a_blob_past_its_bound_is_refused_and_nothing_of_the_write_is_kept() {
        let mut state = writing();
        let writer = state.create().await.expect("a writer");
        let rep = writer.rep();
        // As though `blob_bytes - 1` had been written: the bound is the size
        // the kernel counts, not what the store held in memory.
        if let Some(Writing::Open { size, .. }) = state.handles.writers.get_mut(&rep) {
            *size = KERNEL.blob_bytes - 1;
        }
        assert!(refused_as(
            &state.write(Resource::new_own(rep), vec![1, 2]).await,
            "invalid",
            ErrorCode::BlobTooLarge
        ));
        state
            .write(Resource::new_own(rep), vec![7])
            .await
            .expect("exactly the bound is a blob's size");
        state
            .write(Resource::new_own(rep), Vec::new())
            .await
            .expect("a write of nothing at the bound takes it nowhere past it");
        assert!(refused_as(
            &state.write(Resource::new_own(rep), vec![8]).await,
            "invalid",
            ErrorCode::BlobTooLarge
        ));
        assert_eq!(
            state
                .finish(Resource::new_own(rep))
                .await
                .expect("finished"),
            Hash::of(&[7]).as_str(),
            "a refused write wrote nothing"
        );
    }

    /// A plugin holds at most `blob_handles` readers and writers at once, and
    /// one let go is room for another.
    #[tokio::test]
    async fn a_plugin_holds_no_more_blob_handles_than_the_catalog_allows() {
        let mut state = writing();
        let hash = made(&mut state, b"opened").await;
        let reader = state.open(hash.clone()).await.expect("a reader");
        let mut writers = Vec::new();
        for _ in 1..KERNEL.blob_handles {
            writers.push(state.create().await.expect("within the bound"));
        }
        assert!(refused_as(
            &state.create().await.map(drop),
            "unavailable",
            ErrorCode::PluginBoundReached
        ));
        assert!(refused_as(
            &state.open(hash.clone()).await.map(drop),
            "unavailable",
            ErrorCode::PluginBoundReached
        ));
        world::HostReader::drop(&mut state, reader)
            .await
            .expect("let go");
        let again = state.open(hash).await.expect("room for one more");
        world::HostReader::drop(&mut state, again)
            .await
            .expect("let go");
        let writer = writers.pop().expect("held");
        world::HostWriter::drop(&mut state, writer)
            .await
            .expect("let go");
        state.create().await.expect("room for one more");
    }

    /// A reader answers at most a chunk whatever it asks for, seeks within the
    /// blob and to its end and no further, and reads nothing at the end.
    #[tokio::test]
    async fn a_reader_reads_at_most_a_chunk_and_seeks_within_its_blob() {
        let mut state = writing();
        let chunk = usize::try_from(KERNEL.blob_read_chunk_bytes).expect("fits");
        let bytes: Vec<u8> = (0..chunk + 10)
            .map(|at| u8::try_from(at % 251).expect("fits"))
            .collect();
        let hash = made(&mut state, &bytes).await;
        let reader = state.open(hash).await.expect("a reader");
        let rep = reader.rep();
        assert_eq!(
            state.size(Resource::new_own(rep)).await.expect("a size"),
            u64::try_from(bytes.len()).expect("fits")
        );
        let first = state
            .read(Resource::new_own(rep), u32::MAX)
            .await
            .expect("read");
        assert_eq!(first, bytes[..chunk], "a read answered past its chunk");
        assert_eq!(
            state.read(Resource::new_own(rep), 4).await.expect("read"),
            bytes[chunk..chunk + 4]
        );
        let end = u64::try_from(bytes.len()).expect("fits");
        assert!(refused_as(
            &state.seek(Resource::new_own(rep), end + 1).await,
            "invalid",
            ErrorCode::ValidationOutOfRange
        ));
        state
            .seek(Resource::new_own(rep), 3)
            .await
            .expect("within the blob");
        assert_eq!(
            state.read(Resource::new_own(rep), 2).await.expect("read"),
            bytes[3..5]
        );
        state
            .seek(Resource::new_own(rep), end)
            .await
            .expect("its end");
        assert_eq!(
            state.read(Resource::new_own(rep), 10).await.expect("read"),
            [] as [u8; 0]
        );
        assert_eq!(
            state.read(Resource::new_own(rep), 0).await.expect("read"),
            [] as [u8; 0]
        );
    }

    /// A reader whose blob's bytes went after it was opened is told they are
    /// not on this machine.
    ///
    /// **Not handed an empty chunk**, which says the blob has ended, while its
    /// size still says how many bytes there were: a plugin would take a blob
    /// cut short for a whole one.
    #[tokio::test]
    async fn a_reader_whose_bytes_are_gone_is_told_so_and_not_given_an_end() {
        let mut state = writing();
        let hash = made(&mut state, b"here, then gone").await;
        let reader = state.open(hash.clone()).await.expect("a reader");
        assert!(
            state
                .blobs
                .remove(&Hash::parse(&hash).expect("a hash"))
                .await
                .expect("removed")
        );
        assert_eq!(
            state
                .size(Resource::new_own(reader.rep()))
                .await
                .expect("a size"),
            15
        );
        let read = state.read(Resource::new_own(reader.rep()), 4).await;
        assert!(
            refused_as(&read, "unavailable", ErrorCode::BlobNotLocal),
            "{read:?}"
        );
        assert!(
            refusal(&read).1.contains("is no longer on this machine"),
            "{read:?}"
        );
    }

    /// Text that is not a hash is refused `invalid` with `[NC-BLOB-0001]` by
    /// every function that takes one; a hash no machine holds is not here.
    #[tokio::test]
    async fn every_function_naming_a_blob_refuses_text_that_is_not_a_hash() {
        let mut state = writing();
        for text in ["", "z", &"A".repeat(64), &"0".repeat(65)] {
            let answers = [
                state.locate(text.to_owned()).await.map(drop),
                state.open(text.to_owned()).await.map(drop),
                state.announce(text.to_owned()).await,
            ];
            for answer in &answers {
                assert!(
                    refused_as(answer, "invalid", ErrorCode::BlobMalformedHash),
                    "{text:?}: {answer:?}"
                );
            }
        }
        let elsewhere = "0".repeat(64);
        assert_eq!(
            state.locate(elsewhere.clone()).await.expect("located"),
            world::Availability::Unavailable
        );
        assert!(refused_as(
            &state.open(elsewhere.clone()).await.map(drop),
            "unavailable",
            ErrorCode::BlobNotLocal
        ));
        assert!(refused_as(
            &state.announce(elsewhere).await,
            "unavailable",
            ErrorCode::BlobNoPeers
        ));
    }

    /// A unit makes as many blobs as it may note and not one more, refused
    /// before the writer is finished — which then still takes bytes.
    #[tokio::test]
    async fn a_unit_makes_no_more_blobs_than_it_may_note() {
        let mut state = writing();
        if let Some(frame) = state.frame()
            && let Some(unit) = frame.tree().unit_mut()
        {
            for n in 0..crate::units::notes_per_step() {
                unit.make(Made {
                    hash: Hash::of(&n.to_le_bytes()),
                    size: 8,
                    kept_before: false,
                    pinned_before: false,
                });
            }
        }
        let writer = state.create().await.expect("a writer");
        let rep = writer.rep();
        assert!(refused_as(
            &state.finish(Resource::new_own(rep)).await.map(drop),
            "unavailable",
            ErrorCode::PluginBoundReached
        ));
        state
            .write(Resource::new_own(rep), vec![1])
            .await
            .expect("the writer is untouched");
    }

    /// A blob store whose writes fail, for the writer a disk breaks.
    #[derive(Debug, Default)]
    struct Breaking(MemoryBlobs);

    impl BlobStore for Breaking {
        type Writer = <MemoryBlobs as BlobStore>::Writer;

        fn size(&self, hash: &Hash) -> impl Future<Output = Result<Option<u64>, BlobError>> + Send {
            self.0.size(hash)
        }

        fn read(
            &self,
            hash: &Hash,
            offset: u64,
            max: usize,
        ) -> impl Future<Output = Result<Option<Vec<u8>>, BlobError>> + Send {
            self.0.read(hash, offset, max)
        }

        fn create(&self) -> impl Future<Output = Result<Self::Writer, BlobError>> + Send {
            self.0.create()
        }

        fn write(
            &self,
            writer: Self::Writer,
            _: Vec<u8>,
        ) -> impl Future<Output = (Self::Writer, Result<(), BlobError>)> + Send {
            core::future::ready((writer, Err(BlobError::Io("the disk is full".to_owned()))))
        }

        fn hash_so_far(&self, writer: &Self::Writer) -> Hash {
            self.0.hash_so_far(writer)
        }

        fn finish(
            &self,
            writer: Self::Writer,
        ) -> impl Future<Output = Result<Finished, BlobError>> + Send {
            self.0.finish(writer)
        }

        fn remove(&self, hash: &Hash) -> impl Future<Output = Result<bool, BlobError>> + Send {
            self.0.remove(hash)
        }

        fn hashes(
            &self,
            named: std::time::Duration,
        ) -> impl Future<Output = Result<Vec<Hash>, BlobError>> + Send {
            self.0.hashes(named)
        }

        fn forget_unfinished(&self) -> impl Future<Output = Result<u64, BlobError>> + Send {
            self.0.forget_unfinished()
        }
    }

    /// The kernel over a [`Breaking`] blob store.
    #[derive(Debug)]
    struct OverBreaking;

    impl Environment for OverBreaking {
        type Store = crate::backend::MemoryBackend;
        type Blobs = Breaking;
        type Journal = nc_journal::InMemory;
        type Clock = nc_ids::SystemClock;
        type Directory = nc_authz::MemoryDirectory;
        type Log = crate::logging::Captured;
        type Transport = crate::net::Scripted;
    }

    /// A writer a disk broke keeps nothing and says so, every time it is asked.
    #[tokio::test]
    async fn a_writer_a_disk_broke_keeps_nothing_and_says_so() {
        let mut state: HostState<OverBreaking> = HostState::new(
            GrantSet::of([Capability::BlobsWrite]),
            (
                HostState::<crate::InMemory>::home(),
                "tests:door".to_owned(),
            ),
            Vec::new(),
            Arc::new(crate::backend::MemoryBackend::default()),
            Arc::new(Breaking::default()),
            Arc::new(crate::net::Scripted::default()),
            MemoryGuard::new(&Limits::catalog()),
            crate::host::log::SharedRing::default(),
        );
        state.enter(Frame::for_tests(true, Some(a_unit())));
        let writer = state.create().await.expect("a writer");
        let rep = writer.rep();
        let first = state.write(Resource::new_own(rep), vec![1]).await;
        assert!(
            refused_as(&first, "unavailable", ErrorCode::BlobIo),
            "{first:?}"
        );
        assert!(refusal(&first).1.contains("the disk is full"), "{first:?}");
        for answer in [
            state.write(Resource::new_own(rep), vec![1]).await,
            state.finish(Resource::new_own(rep)).await.map(drop),
        ] {
            assert!(
                refused_as(&answer, "unavailable", ErrorCode::BlobIo),
                "{answer:?}"
            );
        }
    }

    #[tokio::test]
    async fn the_methods_with_no_error_trap_and_a_drop_needs_its_capability() {
        let mut none = HostState::in_memory(GrantSet::none());
        // **Refused for the capability, not for the handle.** Both are traps
        // here, so only the sentence tells a `size` that asked from one that
        // did not.
        let size = none.size(Resource::new_own(0)).await;
        assert!(
            size.as_ref()
                .is_err_and(|trap| trap.to_string().contains("was not granted")),
            "{size:?}"
        );
        assert_eq!(none.reached().len(), 1, "the refused `size` was recorded");
        assert!(world_drop_reader(&mut none).await.is_err());
        assert!(world_drop_writer(&mut none).await.is_err());

        let mut both = granted(None);
        let size = both.size(Resource::new_own(0)).await;
        assert!(
            size.as_ref()
                .is_err_and(|trap| trap.to_string().contains("did not issue")),
            "a size on a handle this kernel never handed out traps saying so: {size:?}"
        );
        assert!(world_drop_reader(&mut both).await.is_ok());
        assert!(world_drop_writer(&mut both).await.is_ok());
    }

    /// `HostReader::drop`, which the method syntax would read as `Drop::drop`.
    async fn world_drop_reader(state: &mut HostState<InMemory>) -> wasmtime::Result<()> {
        world::HostReader::drop(state, Resource::new_own(0)).await
    }

    /// `HostWriter::drop`, for the same reason.
    async fn world_drop_writer(state: &mut HostState<InMemory>) -> wasmtime::Result<()> {
        world::HostWriter::drop(state, Resource::new_own(0)).await
    }
}
