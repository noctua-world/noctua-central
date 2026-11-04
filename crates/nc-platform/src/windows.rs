//! The Windows half: the user the process token names, and access lists, through
//! the Win32 security calls (`windows-sys`).
//!
//! **What is made is made private in the one call that makes it.** A directory
//! and a file are created with a security descriptor whose owner is the user
//! and whose access list is protected — nothing inherited from the parent —
//! and holds one entry: allow, the user, full control. On a directory that
//! entry is inherited by what is created in it, so the `-wal`, `-shm` and
//! `-journal` SQLite creates beside the database with its own default security
//! are the user's alone too. The owner is set, never left to the system: in an
//! elevated administrator's process — GitHub's Windows runners are one — the
//! owner the token gives a new object is the Administrators group.
//!
//! **What is found is judged through one handle** opened with
//! `FILE_FLAG_OPEN_REPARSE_POINT`, so a link is judged as the link and never
//! as what it points at — and any reparse point is a link — and with
//! `FILE_FLAG_BACKUP_SEMANTICS`, without which a directory cannot be opened at
//! all. Its owner must be the user — or, for a file another program made
//! beside one of this crate's with the system's own security, as SQLite makes
//! its companions, the group the token makes the owner of what this process
//! creates; **no access list at all** lets everybody in and is refused;
//! **every allow entry must name the user**, inherited or not and whether it
//! applies to the path or only to what is made in it; an entry of a kind this
//! does not read is refused rather than guessed at; and what the user's
//! entries grant, less what any deny entry takes, must cover what the store
//! needs — and a file marked read-only, which no access list can open for
//! writing, lacks it. A deny entry is read as this user's whoever it names:
//! which groups a user is in is the token's to say, and reading every one as
//! this user's refuses more, never less.
//!
//! **Every `unsafe` block says, beside it, why each pointer it hands the
//! system is valid for the call, and what is freed where**: a handle the
//! system opens is an `OwnedHandle` or a `File` at once, which closes it on
//! every path; what the system allocates — `GetSecurityInfo`'s descriptor, and
//! one a test writes from its text — is held by [`Allocated`], which frees it
//! with `LocalFree`; and every buffer this module hands the system —
//! a SID, an access list, a path — is a `Vec` that outlives the call.

#![cfg(windows)]

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::Path;
use std::ptr::null_mut;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{
    ERROR_SUCCESS, FALSE, GENERIC_ALL, GENERIC_EXECUTE, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE, LocalFree, TRUE, WIN32_ERROR,
};
use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_REVISION, AddAccessAllowedAceEx, AddAccessDeniedAceEx,
    CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, GetAce, GetLengthSid, GetTokenInformation,
    INHERIT_ONLY_ACE, InitializeAcl, InitializeSecurityDescriptor, IsValidSid, OBJECT_INHERIT_ACE,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
    SECURITY_DESCRIPTOR, SetSecurityDescriptorControl, SetSecurityDescriptorDacl,
    SetSecurityDescriptorOwner, TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TokenOwner, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateDirectoryW, CreateFileW, DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY,
    FILE_ALL_ACCESS, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_TRAVERSE, FILE_WRITE_ATTRIBUTES, READ_CONTROL,
};
#[cfg(any(test, feature = "test-support"))]
use windows_sys::Win32::Storage::FileSystem::{WRITE_DAC, WRITE_OWNER};
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE, SECURITY_DESCRIPTOR_REVISION,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::{Failed, Judged, Kind, Refusal};

/// What a private directory is here, as a sentence says it.
pub(crate) const PRIVATE_DIRECTORY: &str =
    "owned by this user and shut to everybody else by an access list naming this user alone";

/// What a private file is here, as a sentence says it.
pub(crate) const PRIVATE_FILE: &str = "owned and shut the same way";

/// A directory has no handle to sync here.
pub(crate) const SYNCS_A_DIRECTORY: bool = false;

/// A directory is held through a lock file in it, since no directory takes a
/// lock here.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const HOLDS_THROUGH_A_FILE: bool = true;

/// What a link is called when one is refused: any reparse point, of which a
/// symbolic link and a junction are two.
const A_LINK: &str = "is a symbolic link, a junction or another reparse point";

/// Every right a file grants: what the one entry of a private path allows.
const EVERYTHING: u32 = FILE_ALL_ACCESS;

/// What the store needs of a file: read it, write it, delete it.
const A_FILE_NEEDS: u32 = FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE;

/// What the store needs of a directory: list it, enter it, add a file and a
/// directory to it, and delete what is in it.
const A_DIRECTORY_NEEDS: u32 =
    FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY | FILE_DELETE_CHILD;

/// A security identifier, held as its bytes, which compare as the SIDs do.
#[derive(Clone, PartialEq, Eq)]
struct Sid(Vec<u8>);

impl Sid {
    /// A copy of the SID at `sid`.
    ///
    /// # Safety
    ///
    /// `sid` is null or points at memory that holds a SID for as long as the
    /// call runs.
    unsafe fn copied(sid: PSID) -> io::Result<Self> {
        // SAFETY: the caller's pointer, which `IsValidSid` reads and nothing
        // writes; a null one is answered before the call.
        if sid.is_null() || unsafe { IsValidSid(sid) } == FALSE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the system answered something that is not a security identifier",
            ));
        }
        // SAFETY: a valid SID, whose length `GetLengthSid` reads from its own
        // header.
        let length = usize::try_from(unsafe { GetLengthSid(sid) })
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "a SID longer than memory"))?;
        // SAFETY: a valid SID is `length` bytes from where it begins, all of
        // them inside the caller's memory, which is only read here.
        let bytes = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), length) };
        Ok(Self(bytes.to_vec()))
    }

    /// The well-known SID `S-1-<authority>-<subs…>`, built from its parts.
    #[cfg(any(test, feature = "test-support"))]
    fn well_known(authority: u8, subs: &[u32]) -> Self {
        let mut bytes = vec![1, u8::try_from(subs.len()).unwrap_or(u8::MAX)];
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, authority]);
        for sub in subs {
            bytes.extend_from_slice(&sub.to_le_bytes());
        }
        Self(bytes)
    }

    /// The SID as a call that only reads it takes one.
    fn as_psid(&self) -> PSID {
        self.0.as_ptr().cast_mut().cast()
    }
}

impl core::fmt::Display for Sid {
    /// `S-1-5-21-…`, as Windows writes a SID.
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let revision = self.0.first().copied().unwrap_or(0);
        let authority = self.0.get(2..8).map_or(0, |bytes| {
            bytes
                .iter()
                .fold(0_u64, |at, byte| (at << 8) | u64::from(*byte))
        });
        write!(formatter, "S-{revision}-{authority}")?;
        for sub in self.0.get(8..).unwrap_or_default().as_chunks::<4>().0 {
            write!(formatter, "-{}", u32::from_le_bytes(*sub))?;
        }
        Ok(())
    }
}

impl core::fmt::Debug for Sid {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{self}")
    }
}

/// Who this process runs as, as its token names it.
///
/// The user, and the owner the token gives what this process creates: the
/// same user, or in an elevated administrator's process the Administrators
/// group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct User {
    sid: Sid,
    creates_as: Sid,
}

impl core::fmt::Display for User {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}", self.sid)
    }
}

/// The token of this process, open to be asked about.
fn token() -> io::Result<OwnedHandle> {
    let mut token: HANDLE = null_mut();
    // SAFETY: `GetCurrentProcess` answers a pseudo-handle that needs no
    // closing, and `token` is a local the call writes a handle into.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == FALSE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call succeeded, so `token` is a handle this process now owns
    // and nothing else holds; the `OwnedHandle` closes it once, on every path.
    Ok(unsafe { OwnedHandle::from_raw_handle(token) })
}

/// The SID the token's `class` answers with: `TokenUser` or `TokenOwner`,
/// both of which begin with the pointer to the SID they name.
fn token_sid(token: &OwnedHandle, class: TOKEN_INFORMATION_CLASS) -> io::Result<Sid> {
    let mut needed = 0_u32;
    // SAFETY: a null buffer of length zero asks for the length alone, which
    // the call writes into the local `needed`; the handle is `token`'s.
    let sized = unsafe {
        GetTokenInformation(token.as_raw_handle(), class, null_mut(), 0, &raw mut needed)
    };
    if sized != FALSE || needed == 0 {
        return Err(io::Error::last_os_error());
    }
    let length = usize::try_from(needed)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "a token answer past memory"))?;
    let mut buffer = vec![0_u64; length.div_ceil(8)];
    // SAFETY: `buffer` holds at least `needed` bytes, aligned for the pointer
    // the answer begins with, and lives across the call, which writes into it
    // alone.
    let answered = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            class,
            buffer.as_mut_ptr().cast(),
            needed,
            &raw mut needed,
        )
    };
    if answered == FALSE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `TOKEN_USER` and `TOKEN_OWNER` both begin with the `PSID` of what
    // they name, and `buffer` holds the one just written.
    let sid = unsafe { buffer.as_ptr().cast::<PSID>().read() };
    // SAFETY: the SID lies inside `buffer`, which outlives the copy.
    unsafe { Sid::copied(sid) }
}

/// Who this process runs as, asked of its token once and kept: a process's
/// token does not change its user.
fn this_user() -> io::Result<User> {
    static USER: OnceLock<User> = OnceLock::new();
    if let Some(user) = USER.get() {
        return Ok(user.clone());
    }
    let token = token()?;
    let user = User {
        sid: token_sid(&token, TokenUser)?,
        creates_as: token_sid(&token, TokenOwner)?,
    };
    Ok(USER.get_or_init(|| user).clone())
}

/// One entry of an access list as this module reads and writes them.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ace {
    /// An entry that allows or denies `mask` to `sid`.
    Known {
        /// Whether it allows; it denies otherwise.
        allows: bool,
        /// Its inheritance flags.
        flags: u32,
        /// The rights it allows or denies.
        mask: u32,
        /// Whom it names.
        sid: Sid,
    },
    /// An entry of another type, which no rule here reads.
    Unread(u8),
}

impl Ace {
    /// An entry allowing `mask` to `sid`, with `flags`.
    const fn allow(sid: Sid, mask: u32, flags: u32) -> Self {
        Self::Known {
            allows: true,
            flags,
            mask,
            sid,
        }
    }

    /// An entry denying `mask` to `sid`.
    #[cfg(any(test, feature = "test-support"))]
    const fn deny(sid: Sid, mask: u32) -> Self {
        Self::Known {
            allows: false,
            flags: 0,
            mask,
            sid,
        }
    }
}

/// The inheritance a private path's one entry carries: on a directory, to
/// every file and directory made in it.
const fn inherited_by(kind: Kind) -> u32 {
    match kind {
        Kind::Directory => OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
        Kind::File => 0,
    }
}

/// The access list of a private path of `kind`: one entry, `user`, every
/// right.
fn private_entries(user: &Sid, kind: Kind) -> Vec<Ace> {
    vec![Ace::allow(user.clone(), EVERYTHING, inherited_by(kind))]
}

/// `entries` as an access list, in a buffer the system reads.
///
/// **A `Vec<u32>`**, because an access list is aligned to four bytes; every
/// entry is eight bytes before its SID, and a SID is a whole number of four.
fn acl_of(entries: &[Ace]) -> io::Result<Vec<u32>> {
    let mut size = size_of::<ACL>();
    for entry in entries {
        let Ace::Known { sid, .. } = entry else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "an entry of a kind this does not write",
            ));
        };
        size += size_of::<ACCESS_ALLOWED_ACE>() - size_of::<u32>() + sid.0.len();
    }
    let length = u32::try_from(size)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "an access list too long"))?;
    let mut acl = vec![0_u32; size.div_ceil(4)];
    // SAFETY: `acl` holds `size` bytes, aligned to four, and the call writes
    // the list's header into it alone.
    if unsafe { InitializeAcl(acl.as_mut_ptr().cast(), length, ACL_REVISION) } == FALSE {
        return Err(io::Error::last_os_error());
    }
    for entry in entries {
        let Ace::Known {
            allows,
            flags,
            mask,
            sid,
        } = entry
        else {
            continue;
        };
        // SAFETY: the list was initialised above with room for every entry,
        // and the call appends this one inside it; `sid` is a valid SID it
        // only reads, alive across the call.
        let added = unsafe {
            if *allows {
                AddAccessAllowedAceEx(
                    acl.as_mut_ptr().cast(),
                    ACL_REVISION,
                    *flags,
                    *mask,
                    sid.as_psid(),
                )
            } else {
                AddAccessDeniedAceEx(
                    acl.as_mut_ptr().cast(),
                    ACL_REVISION,
                    *flags,
                    *mask,
                    sid.as_psid(),
                )
            }
        };
        if added == FALSE {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(acl)
}

/// A security descriptor making a new path private to one user, with the
/// buffers it points into.
struct Private {
    descriptor: SECURITY_DESCRIPTOR,
    /// The owner the descriptor points at, kept for as long as it is used.
    _owner: Vec<u8>,
    /// The access list the descriptor points at, kept for as long as it is
    /// used.
    _acl: Vec<u32>,
}

impl Private {
    /// A descriptor making a new path private to `user`.
    ///
    /// Its owner is `user`, and its protected access list holds one entry: the
    /// user, every right, inherited by what is made inside a directory.
    fn new(user: &Sid, kind: Kind) -> io::Result<Self> {
        let mut owner = user.0.clone();
        let acl = acl_of(&private_entries(user, kind))?;
        let mut descriptor = SECURITY_DESCRIPTOR::default();
        let at: PSECURITY_DESCRIPTOR = (&raw mut descriptor).cast();
        // SAFETY: `at` is the local `descriptor`, a whole `SECURITY_DESCRIPTOR`,
        // which the call initialises in place.
        if unsafe { InitializeSecurityDescriptor(at, SECURITY_DESCRIPTOR_REVISION) } == FALSE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `owner` is a valid SID; the descriptor keeps the pointer,
        // and the buffer behind it moves into the answer with it and is never
        // reallocated, so it stays valid for as long as the descriptor is used.
        if unsafe { SetSecurityDescriptorOwner(at, owner.as_mut_ptr().cast(), FALSE) } == FALSE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `acl` is the access list built above, kept with the
        // descriptor exactly as `owner` is.
        if unsafe { SetSecurityDescriptorDacl(at, TRUE, acl.as_ptr().cast(), FALSE) } == FALSE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the descriptor initialised above, whose control bits the
        // call sets in place: the list inherits nothing from the parent.
        let protected =
            unsafe { SetSecurityDescriptorControl(at, SE_DACL_PROTECTED, SE_DACL_PROTECTED) };
        if protected == FALSE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            descriptor,
            _owner: owner,
            _acl: acl,
        })
    }

    /// The attributes a create call takes, pointing at this descriptor.
    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(u32::MAX),
            lpSecurityDescriptor: (&raw const self.descriptor).cast_mut().cast(),
            bInheritHandle: FALSE,
        }
    }
}

/// `path` as the wide, NUL-ended string a Win32 call takes.
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a path holding a NUL names no file",
        ));
    }
    wide.push(0);
    Ok(wide)
}

/// Creates `directory`, one level, private to this user from its first
/// moment.
pub(crate) fn create_private_directory(directory: &Path) -> io::Result<()> {
    let user = this_user()?;
    let private = Private::new(&user.sid, Kind::Directory)?;
    let attributes = private.attributes();
    let path = wide(directory)?;
    // SAFETY: `path` is NUL-ended and alive across the call, and `attributes`
    // points at a descriptor whose owner and access list `private` keeps alive
    // across it; the call only reads both.
    if unsafe { CreateDirectoryW(path.as_ptr(), &raw const attributes) } == FALSE {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A new file at `path`, private to this user from its first moment, open to
/// read and write.
///
/// **`CREATE_NEW` and `FILE_FLAG_OPEN_REPARSE_POINT`**: a name that is there,
/// a link to anything included, is refused, and never followed.
pub(crate) fn create_private(path: &Path) -> io::Result<File> {
    let user = this_user()?;
    let private = Private::new(&user.sid, Kind::File)?;
    let attributes = private.attributes();
    let name = wide(path)?;
    // SAFETY: `name` is NUL-ended and alive across the call; `attributes`
    // points at a descriptor whose owner and access list `private` keeps alive
    // across it; there is no template. The call only reads all three.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &raw const attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `CreateFileW` handed back a handle this function owns and
    // nothing else holds; the `File` closes it once, on every path.
    Ok(File::from(unsafe { OwnedHandle::from_raw_handle(handle) }))
}

/// How often a path refused with `Access is denied` is asked again, and the
/// first wait in milliseconds, which grows by itself each time: SQLite's own.
const ASKED_AGAIN: (u32, u64) = (10, 25);

/// `path` itself opened with `access`, never what a link at it points at, a
/// directory as well as a file.
///
/// **A file being deleted answers `Access is denied`** until its last handle
/// closes — SQLite's `-shm` as another open of the profile lets it go, one
/// another program such as a scanner holds a moment longer — so that answer is
/// asked again, as SQLite asks again for its own files (`winIoerrRetry` times,
/// `winIoerrRetryDelay` apart and growing), before it is taken:
/// measured on the Windows runner, two of a hundred and twenty opens of a
/// profile from four threads at once were refused as `NC-DB-0006` for a
/// companion that was going. A path still refused after the last wait is
/// answered as refused, so a list that denies this user stays a refusal.
fn opened(path: &Path, access: u32) -> io::Result<File> {
    let (again, wait) = ASKED_AGAIN;
    let mut asked = 0_u32;
    loop {
        match opened_once(path, access) {
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied && asked < again => {
                asked += 1;
                std::thread::sleep(std::time::Duration::from_millis(wait * u64::from(asked)));
            }
            answered => return answered,
        }
    }
}

/// `path` itself opened with `access`, once.
#[expect(
    clippy::disallowed_methods,
    reason = "a path is opened here to be judged or to have its access list read or set, \
              never written to; `ui` is the output path and none is a terminal — each is a \
              path inside a directory the caller judged, or a test's own"
)]
fn opened_once(path: &Path, access: u32) -> io::Result<File> {
    std::fs::OpenOptions::new()
        .access_mode(access)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

/// A descriptor the system allocated — `GetSecurityInfo`'s, and a test's
/// written from its text — freed when this is dropped.
struct Allocated(PSECURITY_DESCRIPTOR);

impl Drop for Allocated {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the system allocated this with `LocalAlloc`, as
            // `GetSecurityInfo` and `ConvertStringSecurityDescriptorToSecurityDescriptorW`
            // both document, and it is freed once, here; nothing read from it
            // outlives this value.
            unsafe { LocalFree(self.0) };
        }
    }
}

/// A path's owner, its access list — none when it has no list at all — and,
/// for a test reading what a path lets in, whether that list is protected from
/// its parent's.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Security {
    owner: Sid,
    entries: Option<Vec<Ace>>,
    #[cfg(any(test, feature = "test-support"))]
    protected: bool,
}

/// An I/O error carrying what a Win32 call answered.
fn answered(status: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(i32::try_from(status).unwrap_or(i32::MAX))
}

/// The owner and the access list of what `file` is open on.
fn security_of(file: &File) -> io::Result<Security> {
    let mut owner: PSID = null_mut();
    let mut dacl: *mut ACL = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: the handle is `file`'s, open across the call; every out-pointer
    // is a local the call writes into; the group and the system list are not
    // asked for, so theirs are null.
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &raw mut owner,
            null_mut(),
            &raw mut dacl,
            null_mut(),
            &raw mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(answered(status));
    }
    let descriptor = Allocated(descriptor);
    // SAFETY: `owner` points into the descriptor, which `descriptor` keeps
    // until the end of this function.
    let owner = unsafe { Sid::copied(owner) }?;
    let entries = if dacl.is_null() {
        None
    } else {
        // SAFETY: `dacl` points into the descriptor, kept as `owner` is.
        Some(unsafe { entries_of(dacl) }?)
    };
    let security = Security {
        owner,
        entries,
        #[cfg(any(test, feature = "test-support"))]
        protected: protected(&descriptor)?,
    };
    // Freed once everything read from it is a copy of its own.
    drop(descriptor);
    Ok(security)
}

/// Whether the access list of `descriptor` is protected from its parent's.
#[cfg(any(test, feature = "test-support"))]
fn protected(descriptor: &Allocated) -> io::Result<bool> {
    use windows_sys::Win32::Security::GetSecurityDescriptorControl;

    let mut control = 0_u16;
    let mut revision = 0_u32;
    // SAFETY: the descriptor the system answered, which `descriptor` keeps
    // alive across the call; both out-pointers are locals the call writes into.
    let read =
        unsafe { GetSecurityDescriptorControl(descriptor.0, &raw mut control, &raw mut revision) };
    if read == FALSE {
        return Err(io::Error::last_os_error());
    }
    Ok(control & SE_DACL_PROTECTED != 0)
}

/// Every entry of the access list at `acl`, in its order.
///
/// # Safety
///
/// `acl` points at an access list the system answered, valid while the call
/// runs.
unsafe fn entries_of(acl: *const ACL) -> io::Result<Vec<Ace>> {
    // SAFETY: the caller's list, whose header is read and not written.
    let count = unsafe { acl.read_unaligned() }.AceCount;
    let mut entries = Vec::with_capacity(usize::from(count));
    for index in 0..u32::from(count) {
        let mut ace: *mut c_void = null_mut();
        // SAFETY: `index` is below the list's own count, and `ace` is a local
        // the call writes a pointer into the list into.
        if unsafe { GetAce(acl, index, &raw mut ace) } == FALSE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: every entry begins with its header, which `ace` points at,
        // inside the caller's list.
        let header = unsafe { ace.cast::<ACE_HEADER>().read_unaligned() };
        let kind = u32::from(header.AceType);
        if kind != ACCESS_ALLOWED_ACE_TYPE && kind != ACCESS_DENIED_ACE_TYPE {
            entries.push(Ace::Unread(header.AceType));
            continue;
        }
        // SAFETY: an allowed and a denied entry share one layout — the header,
        // the mask, then the SID from `SidStart` — and the whole of it lies
        // inside the caller's list; the SID is copied before the list can go.
        let (mask, sid) = unsafe {
            let body = ace.cast::<ACCESS_ALLOWED_ACE>().read_unaligned();
            let sid = ace
                .cast::<u8>()
                .add(core::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart));
            (body.Mask, Sid::copied(sid.cast())?)
        };
        entries.push(Ace::Known {
            allows: kind == ACCESS_ALLOWED_ACE_TYPE,
            flags: u32::from(header.AceFlags),
            mask,
            sid,
        });
    }
    Ok(entries)
}

/// `mask` with each generic right turned into the file rights it stands for.
const fn mapped(mask: u32) -> u32 {
    let mut rights = mask & !(GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE | GENERIC_ALL);
    if mask & GENERIC_READ != 0 {
        rights |= FILE_GENERIC_READ;
    }
    if mask & GENERIC_WRITE != 0 {
        rights |= FILE_GENERIC_WRITE;
    }
    if mask & GENERIC_EXECUTE != 0 {
        rights |= FILE_GENERIC_EXECUTE;
    }
    if mask & GENERIC_ALL != 0 {
        rights |= FILE_ALL_ACCESS;
    }
    rights
}

/// Who this process is, once `directory` is judged private to that user.
pub(crate) fn user_in(directory: &Path) -> Result<User, Judged> {
    let user = this_user().map_err(|error| {
        Failed::new(
            "the user this process runs as could not be asked of its token".to_owned(),
            error,
        )
    })?;
    judge(directory, Kind::Directory, &user)?;
    Ok(user)
}

/// Who a path that is judged may be owned by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owned {
    /// The user alone: what this crate makes it makes owned by the user,
    /// set outright.
    ByTheUser,
    /// The user, or the owner the token gives what this process creates: what
    /// another program made with the system's own security, as SQLite does.
    ByDefault,
}

/// Whether `path` is what it should be and nobody else's.
pub(crate) fn judge(path: &Path, kind: Kind, user: &User) -> Result<(), Judged> {
    judge_owned(path, kind, user, Owned::ByTheUser)
}

/// Whether the file at `path`, made with the system's own security, is
/// nobody else's: the owner the token gives what this process creates is
/// taken as the user's for it alone.
pub(crate) fn judge_made_by_default(path: &Path, user: &User) -> Result<(), Judged> {
    judge_owned(path, Kind::File, user, Owned::ByDefault)
}

/// Whether `path` is what it should be, owned as `owned` says, and nobody
/// else's.
fn judge_owned(path: &Path, kind: Kind, user: &User, owned: Owned) -> Result<(), Judged> {
    let file = opened(path, READ_CONTROL | FILE_READ_ATTRIBUTES)
        .map_err(|error| Failed::new(format!("`{}` could not be read", path.display()), error))?;
    judged(path, &file, kind, user, owned)
}

/// Whether what `file` is open on, at `path`, is a `kind`, owned as `owned`
/// says, and `user`'s alone with everything the store needs.
fn judged(path: &Path, file: &File, kind: Kind, user: &User, owned: Owned) -> Result<(), Judged> {
    let reading =
        |error: io::Error| Failed::new(format!("`{}` could not be read", path.display()), error);
    let refused = |what: String| Err(Judged::Refused(Refusal::of(path, what)));
    let attributes = file.metadata().map_err(reading)?.file_attributes();
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return refused(A_LINK.to_owned());
    }
    let directory = attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    match kind {
        Kind::Directory if !directory => return refused("is not a directory".to_owned()),
        Kind::File if directory => return refused("is not a regular file".to_owned()),
        _ => {}
    }
    let security = security_of(file).map_err(reading)?;
    // **The token's default owner for what the system made alone.** In an
    // elevated administrator's process that owner is the Administrators group,
    // which every file SQLite creates beside the database carries. What this
    // crate creates it creates owned by the user, so a path of its own owned
    // by anybody else was given away since.
    let taken_as_the_users = owned == Owned::ByDefault && security.owner == user.creates_as;
    if security.owner != user.sid && !taken_as_the_users {
        return refused(format!(
            "belongs to user {} and this process runs as user {}",
            security.owner, user.sid
        ));
    }
    let Some(entries) = security.entries else {
        return refused("has no access list at all, so another user can reach it".to_owned());
    };
    let (mut granted, mut denied) = (0_u32, 0_u32);
    for entry in entries {
        match entry {
            Ace::Unread(unread) => {
                return refused(format!(
                    "has an access entry of type {unread}, which this does not read, so it is \
                     taken as one another user can reach"
                ));
            }
            Ace::Known {
                allows: true, sid, ..
            } if sid != user.sid => {
                return refused(format!(
                    "has an access list that lets {sid} in, so another user can reach it"
                ));
            }
            // An entry only what is made inside inherits does not reach this
            // path itself.
            Ace::Known { flags, .. } if flags & INHERIT_ONLY_ACE != 0 => {}
            Ace::Known {
                allows: true, mask, ..
            } => granted |= mapped(mask),
            Ace::Known { mask, .. } => denied |= mapped(mask),
        }
    }
    let (needed, to) = match kind {
        Kind::Directory => (
            A_DIRECTORY_NEEDS,
            "list, enter, add files and folders to and delete from it",
        ),
        Kind::File => (A_FILE_NEEDS, "read, write and delete it"),
    };
    if granted & !denied & needed != needed {
        return refused(format!(
            "has an access list that does not let its owner {to}"
        ));
    }
    // **And a file marked read-only**, which is this system's `0400`: no
    // access list opens it to be written or deleted, so SQLite would open it
    // read-only, the self-check pass, and the next commit come back
    // `NC-DB-0007` for a file whose only fault is this mark. A directory's mark
    // means nothing to the system, and is not read.
    if kind == Kind::File && attributes & FILE_ATTRIBUTE_READONLY != 0 {
        return refused(format!(
            "is marked read-only, which does not let its owner {to}"
        ));
    }
    Ok(())
}

/// `directory`, held through a private file `name` in it under an exclusive
/// lock.
///
/// **A file, because Windows locks no directory**: a directory cannot be
/// opened as a file without backup semantics, and takes no byte-range lock.
/// Never the database file, whose locks are SQLite's. The file is created
/// private when it is missing, and judged as any file of the user's when it
/// is there — through the handle the lock is then taken on, opened to read
/// alone, which is all a lock needs and all a file marked read-only lets a
/// handle be opened with before it is judged.
pub(crate) fn hold(directory: &Path, name: &str, user: &User) -> Result<File, Judged> {
    let path = directory.join(name);
    let held = match create_private(&path) {
        Ok(held) => held,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let held = opened(&path, GENERIC_READ | READ_CONTROL).map_err(|error| {
                Failed::new(format!("`{}` could not be read", path.display()), error)
            })?;
            judged(&path, &held, Kind::File, user, Owned::ByTheUser)?;
            held
        }
        Err(error) => {
            return Err(Judged::Failed(Failed::new(
                format!("`{}` could not be created", path.display()),
                error,
            )));
        }
    };
    held.lock().map_err(|error| {
        Failed::new(
            format!("`{}` could not be held for this open", directory.display()),
            error,
        )
    })?;
    Ok(held)
}

/// Nothing: Windows has no handle to sync a directory through, and NTFS
/// journals its metadata, so a rename is as durable as the volume makes it.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the interface's answer: the Unix half says what syncing a directory said"
)]
pub(crate) fn sync_directory(_directory: &Path) -> io::Result<()> {
    Ok(())
}

/// The time `path` was last written, set to `at`, through a handle opened to
/// set its attributes: one opened to read may not.
#[expect(
    clippy::disallowed_methods,
    reason = "a blob's file is opened here to set the time it was named, never written to; `ui` \
              is the output path and none is a terminal — each is a file inside the blob \
              directory, which its caller judged, or a test's own"
)]
pub(crate) fn set_modified(path: &Path, at: std::time::SystemTime) -> io::Result<()> {
    std::fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(path)?
        .set_modified(at)
}

/// What a path lets in: its owner, every entry of its access list, whether
/// that list is protected, and whether the path is marked read-only.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Access {
    security: Security,
    read_only: bool,
}

/// What `path` lets in now.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn access(path: &Path) -> io::Result<Access> {
    let file = opened(path, READ_CONTROL | FILE_READ_ATTRIBUTES)?;
    Ok(Access {
        security: security_of(&file)?,
        read_only: file.metadata()?.file_attributes() & FILE_ATTRIBUTE_READONLY != 0,
    })
}

/// Whether `path` is a directory or a file, read off the path itself.
#[cfg(any(test, feature = "test-support"))]
fn kind_of(path: &Path) -> io::Result<Kind> {
    Ok(if std::fs::symlink_metadata(path)?.is_dir() {
        Kind::Directory
    } else {
        Kind::File
    })
}

/// Whether `path` carries exactly the descriptor this module creates one
/// with: the user its owner, protected, one entry, and no read-only mark.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_as_created(path: &Path) -> bool {
    let (
        Ok(user),
        Ok(kind),
        Ok(Access {
            security,
            read_only,
        }),
    ) = (this_user(), kind_of(path), access(path))
    else {
        return false;
    };
    security.owner == user.sid
        && security.protected
        && security.entries == Some(private_entries(&user.sid, kind))
        && !read_only
}

/// Whether `path` lets in this user alone, with every right.
///
/// Owned by the user or by the group the token makes the owner of what this
/// process creates — this is what SQLite's companions are read by — and every
/// entry, inherited or not, allowing the user.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_private(path: &Path) -> bool {
    let (Ok(user), Ok(Access { security, .. })) = (this_user(), access(path)) else {
        return false;
    };
    let Some(entries) = security.entries else {
        return false;
    };
    let mut granted = 0_u32;
    for entry in entries {
        match entry {
            Ace::Known {
                allows: true,
                mask,
                sid,
                ..
            } if sid == user.sid => granted |= mapped(mask),
            _ => return false,
        }
    }
    (security.owner == user.sid || security.owner == user.creates_as)
        && granted & EVERYTHING == EVERYTHING
}

/// `descriptor` initialised to hold nothing yet, to write one part of a
/// path's through, as the pointer the calls that fill it take.
#[cfg(any(test, feature = "test-support"))]
fn initialised(descriptor: &mut SECURITY_DESCRIPTOR) -> io::Result<PSECURITY_DESCRIPTOR> {
    let at: PSECURITY_DESCRIPTOR = core::ptr::from_mut(descriptor).cast();
    // SAFETY: `at` is the caller's `descriptor`, a whole `SECURITY_DESCRIPTOR`
    // borrowed for the call, which writes its header into it and nothing else.
    if unsafe { InitializeSecurityDescriptor(at, SECURITY_DESCRIPTOR_REVISION) } == FALSE {
        return Err(io::Error::last_os_error());
    }
    Ok(at)
}

/// Gives what `file` is open on the owner `owner`.
///
/// **Through the handle, and nothing else**: `SetKernelObjectSecurity`
/// writes the one descriptor it is handed onto the one object, and walks
/// nothing below it.
#[cfg(any(test, feature = "test-support"))]
fn write_owner(file: &File, owner: &Sid) -> io::Result<()> {
    use windows_sys::Win32::Security::SetKernelObjectSecurity;

    let mut descriptor = SECURITY_DESCRIPTOR::default();
    // Alive to the end of this function, which is as long as `at` is used.
    let at = initialised(&mut descriptor)?;
    // SAFETY: `at` is the descriptor initialised above; `owner` is a valid SID,
    // borrowed for the whole of this function, which the descriptor only
    // points at.
    if unsafe { SetSecurityDescriptorOwner(at, owner.as_psid(), FALSE) } == FALSE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the handle is `file`'s, open across the call with the right to
    // set the owner; `at` and the SID it points at are alive across it, and
    // the call only reads them.
    if unsafe { SetKernelObjectSecurity(file.as_raw_handle(), OWNER_SECURITY_INFORMATION, at) }
        == FALSE
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Gives what `file` is open on the access list `list`, protected from its
/// parent's — none at all for no list.
///
/// **Through the handle, and nothing else**, as [`write_owner`] is. The calls
/// that write a list by a path's name carry its inheritable entries down to
/// every file and directory under it, and a list denying the user the right
/// to list a directory would refuse that very walk.
#[cfg(any(test, feature = "test-support"))]
fn write_list(file: &File, list: Option<&[u32]>) -> io::Result<()> {
    use windows_sys::Win32::Security::SetKernelObjectSecurity;

    let mut descriptor = SECURITY_DESCRIPTOR::default();
    // Alive to the end of this function, which is as long as `at` is used.
    let at = initialised(&mut descriptor)?;
    let pointer: *const ACL = list.map_or(core::ptr::null(), |list| list.as_ptr().cast());
    // SAFETY: `at` is the descriptor initialised above; `pointer` is the
    // access list `list` holds, borrowed for the whole of this function, or
    // null, which is the list's absence. The descriptor only points at it.
    if unsafe { SetSecurityDescriptorDacl(at, TRUE, pointer, FALSE) } == FALSE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the descriptor initialised above, whose control bits the call
    // sets in place: the list inherits nothing from the parent.
    if unsafe { SetSecurityDescriptorControl(at, SE_DACL_PROTECTED, SE_DACL_PROTECTED) } == FALSE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the handle is `file`'s, open across the call with the right to
    // write the list; `at` and the list it points at are alive across it, and
    // the call only reads them.
    if unsafe { SetKernelObjectSecurity(file.as_raw_handle(), DACL_SECURITY_INFORMATION, at) }
        == FALSE
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Marks the file `path` read-only, or clears the mark, through a handle
/// that may set its attributes.
#[cfg(any(test, feature = "test-support"))]
fn set_read_only(path: &Path, read_only: bool) -> io::Result<()> {
    let file = opened(path, FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES)?;
    let mut permissions = file.metadata()?.permissions();
    permissions.set_readonly(read_only);
    file.set_permissions(permissions)
}

/// Gives `path` the owner and the protected `entries` — no access list at
/// all for none — and takes a read-only mark off a file.
///
/// **Each part through a handle that may write it, in an order that always
/// can.** The mark first, while the list that set it — the user's, with
/// every right — is still the path's. Then, for a path somebody else owns —
/// what a process creates, in an elevated administrator's, is owned by the
/// Administrators group — the user's own list, which the owner of a path may
/// always write and which lets the user set the owner, and then that owner.
/// The list asked for last, which the owner may always write, whatever it
/// takes away.
#[cfg(any(test, feature = "test-support"))]
fn set_security(path: &Path, owner: &Sid, entries: Option<&[Ace]>) -> io::Result<()> {
    let kind = kind_of(path)?;
    let Access {
        security,
        read_only,
    } = access(path)?;
    if read_only {
        set_read_only(path, false)?;
    }
    if security.owner != *owner {
        let private = acl_of(&private_entries(owner, kind))?;
        write_list(
            &opened(path, READ_CONTROL | WRITE_DAC)?,
            Some(private.as_slice()),
        )?;
        write_owner(&opened(path, READ_CONTROL | WRITE_OWNER)?, owner)?;
    }
    let acl = entries.map(acl_of).transpose()?;
    write_list(&opened(path, READ_CONTROL | WRITE_DAC)?, acl.as_deref())
}

/// Gives `path` the descriptor this module creates one of its kind with.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn make_private(path: &Path) -> io::Result<()> {
    let user = this_user()?;
    let entries = private_entries(&user.sid, kind_of(path)?);
    set_security(path, &user.sid, Some(&entries))
}

/// `Everyone`, `S-1-1-0`.
#[cfg(any(test, feature = "test-support"))]
fn everyone() -> Sid {
    Sid::well_known(1, &[0])
}

/// Lets `Everyone` read `path` beside the user, and answers its SID.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn open_to_another_user(path: &Path) -> io::Result<String> {
    let user = this_user()?;
    let mut entries = private_entries(&user.sid, kind_of(path)?);
    entries.push(Ace::allow(everyone(), FILE_GENERIC_READ, 0));
    set_security(path, &user.sid, Some(&entries))?;
    Ok(everyone().to_string())
}

/// The words a refusal of a path of `kind` its owner lacks a right on says.
#[cfg(any(test, feature = "test-support"))]
const fn lacking(kind: Kind) -> &'static str {
    match kind {
        Kind::Directory => {
            "does not let its owner list, enter, add files and folders to and delete from it"
        }
        Kind::File => "does not let its owner read, write and delete it",
    }
}

/// Denies the user the right to write `path` — to add to it, for a
/// directory — before the entry that allows everything, and answers what its
/// refusal says.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn shut_its_owner_out(path: &Path) -> io::Result<String> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_APPEND_DATA, FILE_WRITE_DATA};

    let user = this_user()?;
    let kind = kind_of(path)?;
    let taken = match kind {
        Kind::Directory => FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY,
        Kind::File => FILE_WRITE_DATA | FILE_APPEND_DATA,
    };
    let mut entries = vec![Ace::deny(user.sid.clone(), taken)];
    entries.extend(private_entries(&user.sid, kind));
    set_security(path, &user.sid, Some(&entries))?;
    Ok(lacking(kind).to_owned())
}

/// Denies the user reading the file `path`, before the entry that allows
/// everything.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn make_unreadable(path: &Path) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::FILE_READ_DATA;

    let user = this_user()?;
    let mut entries = vec![Ace::deny(user.sid.clone(), FILE_READ_DATA)];
    entries.extend(private_entries(&user.sid, kind_of(path)?));
    set_security(path, &user.sid, Some(&entries))
}

/// Denies the user listing `directory`, before the entry that allows
/// everything: written in and entered, and not listed.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn hide_its_entries(directory: &Path) -> io::Result<()> {
    let user = this_user()?;
    let mut entries = vec![Ace::deny(user.sid.clone(), FILE_LIST_DIRECTORY)];
    entries.extend(private_entries(&user.sid, Kind::Directory));
    set_security(directory, &user.sid, Some(&entries))
}

/// Gives `path` the owner the token gives what this process creates, and
/// answers whether that is somebody other than the user: the Administrators
/// group, in an elevated administrator's process.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn owned_as_this_process_creates(path: &Path) -> io::Result<bool> {
    let user = this_user()?;
    if user.creates_as == user.sid {
        return Ok(false);
    }
    write_owner(&opened(path, READ_CONTROL | WRITE_OWNER)?, &user.creates_as)?;
    Ok(true)
}

/// A symbolic link at `link` to the file `target`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn link_file(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

/// A junction at `link` to the directory `target`: `mklink /J`, which needs
/// no privilege, where a symbolic link to a directory does.
#[cfg(any(test, feature = "test-support"))]
#[expect(
    clippy::disallowed_methods,
    reason = "`cmd` makes the junction the standard library cannot yet; its output is \
              captured and read, never inherited, so it prints nothing"
)]
pub(crate) fn link_directory(target: &Path, link: &Path) -> io::Result<()> {
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()?;
    if made.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "`mklink /J` refused the junction: {}",
            String::from_utf8_lossy(&made.stderr).trim()
        )))
    }
}

/// The volume and the file index of `path`, which name one file on one
/// volume.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn identity(path: &Path) -> io::Result<(u64, u64)> {
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    let file = opened(path, FILE_READ_ATTRIBUTES)?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle is `file`'s, open across the call; `information` is a
    // local the call fills.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) } == FALSE {
        return Err(io::Error::last_os_error());
    }
    Ok((
        u64::from(information.dwVolumeSerialNumber),
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    ))
}

/// Whether the lock file `name` in `directory` is locked now: one taken here
/// would have to wait.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_held(directory: &Path, name: &str) -> bool {
    std::fs::File::open(directory.join(name))
        .is_ok_and(|handle| matches!(handle.try_lock(), Err(std::fs::TryLockError::WouldBlock)))
}

/// `SYSTEM`, or `LOCAL SERVICE` for a process that is `SYSTEM`: a user this
/// process is not, owning nothing it makes.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn someone_else(user: &User) -> User {
    let system = Sid::well_known(5, &[18]);
    let other = if user.sid == system {
        Sid::well_known(5, &[19])
    } else {
        system
    };
    User {
        sid: other.clone(),
        creates_as: other,
    }
}

/// Whether a child process ended as `std::process::abort` ends one here:
/// through `__fastfail`, whose exit code is `STATUS_STACK_BUFFER_OVERRUN`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn stopped_by_abort(status: std::process::ExitStatus) -> bool {
    use windows_sys::Win32::Foundation::STATUS_STACK_BUFFER_OVERRUN;

    status.code() == Some(STATUS_STACK_BUFFER_OVERRUN)
}

/// One access list a path is given, to be judged under it.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone)]
pub(crate) struct Opening {
    kind: Kind,
    how: How,
}

/// How an [`Opening`] differs from a private path's list.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone)]
enum How {
    /// No access list at all.
    NoList,
    /// An entry letting `sid` read, with `flags`.
    Lets(Sid, u32),
    /// A conditional entry letting the user read when its condition holds:
    /// of a type no rule here reads.
    Conditional,
    /// An entry denying the user `mask`, before the one allowing everything.
    Denies(u32),
    /// The user's one entry allowing `mask` and nothing more.
    Grants(u32),
    /// The private list, and the file marked read-only.
    ReadOnly,
}

/// The type a conditional entry is written with, and its refusal names.
#[cfg(any(test, feature = "test-support"))]
const CONDITIONAL: u32 =
    windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_CALLBACK_ACE_TYPE;

/// Gives what `file` is open on the access list `sddl` writes, in the
/// security descriptor language: the one way to write a conditional entry
/// without laying out its expression by hand.
#[cfg(any(test, feature = "test-support"))]
fn write_list_of(file: &File, sddl: &str) -> io::Result<()> {
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SetKernelObjectSecurity;

    let text: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: `text` is NUL-ended and alive across the call, which only reads
    // it and writes the descriptor it allocates into the local `descriptor`;
    // its size is not asked for.
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &raw mut descriptor,
            null_mut(),
        )
    };
    if converted == FALSE {
        return Err(io::Error::last_os_error());
    }
    // Freed with `LocalFree` when this returns, as the call documents.
    let descriptor = Allocated(descriptor);
    // SAFETY: the handle is `file`'s, open across the call with the right to
    // write the list; the descriptor is the one the system answered, which
    // `descriptor` keeps alive across the call, and the call only reads it.
    if unsafe {
        SetKernelObjectSecurity(
            file.as_raw_handle(),
            DACL_SECURITY_INFORMATION,
            descriptor.0,
        )
    } == FALSE
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Gives `path` the opening's list, and answers how a refusal names it.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn apply(opening: &Opening, path: &Path) -> io::Result<String> {
    let user = this_user()?;
    let mut entries = private_entries(&user.sid, opening.kind);
    let said = match &opening.how {
        How::NoList => {
            set_security(path, &user.sid, None)?;
            return Ok("no access list at all".to_owned());
        }
        How::Lets(sid, flags) => {
            entries.push(Ace::allow(sid.clone(), FILE_GENERIC_READ, *flags));
            sid.to_string()
        }
        How::Conditional => {
            set_security(path, &user.sid, Some(&entries))?;
            let inherited = match opening.kind {
                Kind::Directory => "OICI",
                Kind::File => "",
            };
            let sid = &user.sid;
            write_list_of(
                &opened(path, READ_CONTROL | WRITE_DAC)?,
                &format!("D:P(A;{inherited};FA;;;{sid})(XA;;FR;;;{sid};(Member_of {{SID(BA)}}))"),
            )?;
            return Ok(format!("an access entry of type {CONDITIONAL}"));
        }
        How::Denies(mask) => {
            entries.insert(0, Ace::deny(user.sid.clone(), *mask));
            lacking(opening.kind).to_owned()
        }
        How::Grants(mask) => {
            entries = vec![Ace::allow(
                user.sid.clone(),
                *mask,
                inherited_by(opening.kind),
            )];
            lacking(opening.kind).to_owned()
        }
        How::ReadOnly => {
            set_security(path, &user.sid, Some(&entries))?;
            set_read_only(path, true)?;
            return Ok(format!(
                "is marked read-only, which {}",
                lacking(opening.kind)
            ));
        }
    };
    set_security(path, &user.sid, Some(&entries))?;
    Ok(said)
}

/// No list; `Everyone`, `Users` and `Authenticated Users` let in, each alone;
/// a conditional entry; and, on a directory, `Everyone` let into what is made
/// inside it alone.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn every_way_another_user_can_reach(kind: Kind) -> Vec<Opening> {
    let mut ways = vec![
        How::NoList,
        How::Lets(everyone(), 0),
        How::Lets(Sid::well_known(5, &[32, 545]), 0),
        How::Lets(Sid::well_known(5, &[11]), 0),
        How::Conditional,
    ];
    if kind == Kind::Directory {
        ways.push(How::Lets(
            everyone(),
            OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE,
        ));
    }
    ways.into_iter().map(|how| Opening { kind, how }).collect()
}

/// Each right the store needs of a `kind` denied alone, an entry granting
/// reading alone, and, on a file, the read-only mark.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn every_way_its_owner_lacks(kind: Kind) -> Vec<Opening> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_READ_DATA, FILE_WRITE_DATA};

    let denied: &[u32] = match kind {
        Kind::File => &[FILE_READ_DATA, FILE_WRITE_DATA, DELETE],
        Kind::Directory => &[
            FILE_LIST_DIRECTORY,
            FILE_TRAVERSE,
            FILE_ADD_FILE,
            FILE_ADD_SUBDIRECTORY,
            FILE_DELETE_CHILD,
        ],
    };
    let mut ways: Vec<How> = denied.iter().map(|mask| How::Denies(*mask)).collect();
    ways.push(How::Grants(FILE_GENERIC_READ | FILE_TRAVERSE));
    if kind == Kind::File {
        ways.push(How::ReadOnly);
    }
    ways.into_iter().map(|how| Opening { kind, how }).collect()
}
