//! The X server's half of the Leader's delivery: a client of the X11 wire
//! protocol written here, in safe Rust, that asks the server for one chord and
//! reads its presses ([`crate::leader`], `NOCTUA.md §12.8`).
//!
//! **Written here because the alternative is a C library or a crate of
//! bindings to one**, and what a Leader needs of the protocol is seven
//! requests: the connection's setup, `GetModifierMapping`, `GrabKey`,
//! `GetInputFocus` — asked only so the server answers every grab before it —
//! `UngrabKeyboard`, `UngrabKey` and `QueryKeymap`. Each is encoded by a
//! function of this module and held to its bytes by a test beside it.
//!
//! # What is asked of the server, in order
//!
//! 1. **The display** is the one `DISPLAY` names, on this machine and no
//!    other: `:N`, `unix:N` or a socket's path, a screen after it or none. A
//!    name with a host in it is refused — this dials no network — and so is
//!    one that does not read.
//! 2. **The socket** is `/tmp/.X11-unix/X<N>`, and where nothing answers
//!    there, on Linux, the abstract socket of the same name, which is how a
//!    server is reached from a mount namespace that has another `/tmp`. A
//!    socket that is a file is spoken to only when the file is this user's or
//!    root's, asked before anything is sent; a display named by its socket's
//!    path is reached at that path or not at all.
//! 3. **Who this is**: the `MIT-MAGIC-COOKIE-1` the file `XAUTHORITY` names
//!    holds for this machine and this display, or `.Xauthority` under `HOME`.
//!    With no cookie the setup is sent with none, which a server that admits
//!    its own user by the socket's credentials accepts.
//! 4. **Which modifier each key is**: the server's own map, asked for and read
//!    by physical key — the modifier Left Alt is in, the one Left Super is in,
//!    the one Num Lock is in — never the convention that Alt is `Mod1`.
//! 5. **The grab**: `GrabKey` on every screen's root window, of the Leader's
//!    one key with its modifiers exactly, and again with Caps Lock, with Num
//!    Lock and with both held, since the server matches the modifiers exactly
//!    and a lock left on is not part of the chord.
//! 6. **Each press**, read on a thread of this module's. A grab that fires
//!    hands this client the whole keyboard until the key goes up; it is handed
//!    back at once (`UngrabKeyboard`), so the keys pressed while the Leader is
//!    held reach the window that has the focus and never this client.
//! 7. **One press for one time the key goes down.** The server fires a grab
//!    again for each repeat of a held key — measured on Xvfb 21.1, every
//!    forty milliseconds past the first two thirds of a second — and each
//!    firing takes the keyboard from the window that has the focus anew. So
//!    with the keyboard the grabs themselves are let go as the press is
//!    reported, and made again once the key is up. That the server is asked,
//!    every [`session::ASKED_EVERY`], with `QueryKeymap`, the one request
//!    that says so to a client the key's release is not sent to.
//! 8. **The chord again, and its loss.** While the key was down the chord
//!    was nobody's, so the server may have granted it to another client, and
//!    then answers `BadAccess` to the grabs made again. Whatever part of the
//!    chord it did grant is let go, and it is asked again every
//!    [`session::TRIED_EVERY`], [`session::TRIES`] times in all; after the
//!    last the connection is closed and the caller is told the chord is
//!    lost, as it is told when the server goes away. Nothing ends in
//!    silence but a registration dropped.
//!
//! # Physical keys
//!
//! A key is its keycode, and a keycode on a Linux X server — Xorg under
//! `evdev` or `libinput`, Xwayland, Xvfb, Xephyr — is the kernel's code for
//! the key plus eight. So the key grabbed is a place on the board, whatever
//! the layout types with it, and no keysym is ever asked for.
//!
//! # What this answers (`§2.4`)
//!
//! - **A grab is a keylogger's capability, so one chord is all that is
//!   asked.** The one key with a modifier — [`nc_schema::keys::Leader`] cannot
//!   be built without one — and nothing else: no event is selected on any
//!   window and no keyboard is grabbed. The keyboard a firing grab hands over
//!   is handed back before the caller is told, and an event that is not the
//!   Leader's press is dropped unread but for the two bytes that say so. The
//!   keys' state is asked for only between the Leader's press and its
//!   release, and of the answer one bit is read, the Leader's own key's.
//! - **Every length the server sends is untrusted.** A reply longer than the
//!   longest this asks for, a setup that ends before its own counts do, a map
//!   whose length disagrees with its header: each ends the connection and the
//!   registration, and nothing is allocated for a length before it is held to
//!   a bound.
//! - **Another program holding the chord** is the server's `BadAccess` to the
//!   grab, and is refused as [`Refused::Taken`] with every grab this made let
//!   go: the chord is never shared and never taken from its holder.
//! - **The cookie is sent to this user's server or the system's**, where the
//!   socket is a file: one another account made in `/tmp/.X11-unix` — a
//!   directory every account may write in — is refused before a byte is
//!   sent. The file it is read from is read up to a bound.
//! - **A registration that stops delivering says so.** The caller is told
//!   when the server went away and when the chord was lost to another
//!   client, once, and no registration is left looking held.
//! - **A server whose map cannot say which modifier Alt or Super is**, or
//!   puts two of the chord's in one, is refused: grabbing by a guess would
//!   grab another chord.
//!
//! # What it does not
//!
//! - **Another account answering at the abstract socket's name** while no
//!   server holds it and no file is at the socket's path in `/tmp`: an
//!   abstract name is anyone's to bind and has no owner to ask after, so that
//!   account is sent this display's cookie, if the file still holds one, and
//!   can send presses. It reaches no key the person types.
//! - **A server that numbers its keys another way** — the retired `kbd`
//!   driver, a server of another system — where the keycode sent is another
//!   key's. Every server a Linux session runs today numbers them as above.
//! - **A process of this user's**, which can ask the server for the same grab
//!   first, send it synthetic presses, or read the keyboard itself: an X
//!   server keeps nothing of one client from another.
//! - **The chord while the Leader's key is down.** From the press until the
//!   grabs are made again — for as long as the key is held, which is seconds
//!   when the Leader is held for what holding it does (`§12.8`), and then one
//!   [`session::ASKED_EVERY`] and an answer — the chord is nobody's. A second
//!   press in it reaches the window with the focus; and another client that
//!   asks for the chord then is granted it, not refused as
//!   [`Refused::Taken`]. That client keeps the chord for as long as it holds
//!   it: this one has it back only if the other lets it go within the tries
//!   above, and is otherwise told it lost it. Keeping the grabs through the
//!   hold would close this, at the price of the focus taken from the window
//!   twenty-five times a second while the key repeats.
//! - **While the Leader is held, its own repeats**, which reach the window
//!   with the focus as any key's do.

use std::io::Read;

use nc_schema::keys::{Modifier, Modifiers};

use crate::leader::Refused;

/// The name of the one authorization this client speaks.
const MIT_MAGIC_COOKIE_1: &[u8] = b"MIT-MAGIC-COOKIE-1";

/// An entry of the authorization file for a display on this machine, named by
/// the machine's host name.
const FAMILY_LOCAL: u16 = 256;

/// An entry of the authorization file for any machine's display.
const FAMILY_WILD: u16 = 65535;

/// The error a server answers a grab with when another client holds it.
const BAD_ACCESS: u8 = 10;

/// The event a key going down is.
pub(crate) const KEY_PRESS: u8 = 2;

/// The event that carries a length of its own, which nothing here asks for.
const GENERIC_EVENT: u8 = 35;

/// The modifier bit Shift is, fixed by the protocol.
const SHIFT: u16 = 1 << 0;

/// The modifier bit Caps Lock is, fixed by the protocol.
const LOCK: u16 = 1 << 1;

/// The modifier bit Control is, fixed by the protocol.
const CONTROL: u16 = 1 << 2;

/// What a Linux X server adds to the kernel's code for a key to make its
/// keycode.
const EVDEV_OFFSET: u8 = 8;

/// The kernel's code for Left Alt, by which the server's map is asked which
/// modifier Alt is.
const EVDEV_LEFT_ALT: u8 = 56;

/// The kernel's code for Left Super, by which the map is asked which
/// modifier Super is.
const EVDEV_LEFT_SUPER: u8 = 125;

/// The kernel's code for Num Lock, by which the map is asked which modifier,
/// if any, Num Lock is.
const EVDEV_NUM_LOCK: u8 = 69;

/// The most bytes a reply or an event may carry past its first thirty-two.
///
/// The longest this asks for is the modifier map's: eight modifiers of at
/// most 255 keycodes each, 2040 bytes. Anything longer is a server saying
/// what this did not ask, and ends the connection.
pub(crate) const EXTRA_BYTES_MAX: usize = 2040;

/// The most bytes of an authorization file that are read.
///
/// A person's file holds an entry per display, of some sixty bytes each; a
/// megabyte is past sixteen thousand of them.
pub(crate) const AUTHORITY_BYTES_MAX: u64 = 1 << 20;

/// The most characters of a server's reason for refusing a connection that
/// are kept.
const REASON_CHARS_MAX: usize = 120;

/// `length` rounded up to the protocol's unit, four bytes.
const fn padded(length: usize) -> usize {
    length.div_ceil(4) * 4
}

/// A display on this machine, as `DISPLAY` names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Display {
    /// The socket's own path, when the name gave one.
    pub(crate) path: Option<String>,
    /// The display's number.
    pub(crate) number: u16,
}

impl Display {
    /// The display `text` names.
    ///
    /// # Errors
    ///
    /// [`Refused::Failed`] when it names a host, which this never dials, or
    /// does not read as a display at all.
    pub(crate) fn parse(text: &str) -> Result<Self, Refused> {
        let unreadable = || Self::unreadable(text);
        let (before, after) = text.rsplit_once(':').ok_or_else(unreadable)?;
        let number = after.split_once('.').map_or(after, |(number, screen)| {
            if screen.bytes().all(|byte| byte.is_ascii_digit()) && !screen.is_empty() {
                number
            } else {
                ""
            }
        });
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(unreadable());
        }
        let number: u16 = number.parse().map_err(|_| unreadable())?;
        match before {
            "" | "unix" => Ok(Self { path: None, number }),
            path if path.starts_with('/') => Ok(Self {
                path: Some(path.to_owned()),
                number,
            }),
            _ => Err(unreadable()),
        }
    }

    /// Why `text` is no display this dials.
    pub(crate) fn unreadable(text: &str) -> Refused {
        Refused::Failed(format!(
            "`DISPLAY` is `{text}`, which names no display on this machine: one is `:N`, \
             `unix:N` or a socket's path"
        ))
    }

    /// The path of the socket a server of this display listens at.
    pub(crate) fn socket(&self) -> String {
        self.path
            .clone()
            .unwrap_or_else(|| format!("/tmp/.X11-unix/X{}", self.number))
    }
}

/// The cookie `file` — an authorization file's bytes — holds for the display
/// numbered `number` of the machine named `host`, if it holds one.
///
/// **The first entry that fits, and none from a file that does not read.** An
/// entry fits when it is this machine's by name or any machine's, names this
/// display or none, and is a `MIT-MAGIC-COOKIE-1`. A file that ends inside an
/// entry holds no cookie at all, since what it held before the break is not
/// known to be what its writer meant.
pub(crate) fn cookie(file: &[u8], host: Option<&[u8]>, number: u16) -> Option<Vec<u8>> {
    let wanted = number.to_string();
    let mut rest = file;
    let mut found = None;
    while !rest.is_empty() {
        let family = u16::from_be_bytes(taken(&mut rest, 2)?.try_into().ok()?);
        let address = counted(&mut rest)?;
        let display = counted(&mut rest)?;
        let name = counted(&mut rest)?;
        let data = counted(&mut rest)?;
        let ours = match family {
            FAMILY_LOCAL => host == Some(address),
            FAMILY_WILD => true,
            _ => false,
        };
        let this_display = display.is_empty() || display == wanted.as_bytes();
        if found.is_none() && ours && this_display && name == MIT_MAGIC_COOKIE_1 {
            found = Some(data.to_vec());
        }
    }
    found
}

/// The first `length` bytes of `rest`, which then begins after them.
fn taken<'a>(rest: &mut &'a [u8], length: usize) -> Option<&'a [u8]> {
    let (head, tail) = rest.split_at_checked(length)?;
    *rest = tail;
    Some(head)
}

/// A field of the authorization file: its length in two bytes, most
/// significant first, then that many bytes.
fn counted<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    let length = u16::from_be_bytes(taken(rest, 2)?.try_into().ok()?);
    taken(rest, usize::from(length))
}

/// The connection's first message, with `cookie` as a `MIT-MAGIC-COOKIE-1`
/// or no authorization at all.
///
/// It says the least significant byte comes first, and the protocol's
/// version, 11.0.
pub(crate) fn setup_request(cookie: Option<&[u8]>) -> Vec<u8> {
    let (name, data) = cookie.map_or((&[][..], &[][..]), |data| (MIT_MAGIC_COOKIE_1, data));
    let mut request = vec![b'l', 0, 11, 0, 0, 0];
    request.extend(u16::try_from(name.len()).unwrap_or(0).to_le_bytes());
    request.extend(u16::try_from(data.len()).unwrap_or(0).to_le_bytes());
    request.extend([0, 0]);
    for field in [name, data] {
        request.extend(field);
        request.resize(padded(request.len()), 0);
    }
    request
}

/// What a server says of itself as a connection opens, as far as a grab needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Setup {
    /// Every screen's root window.
    pub(crate) roots: Vec<u32>,
    /// The lowest keycode the server has.
    pub(crate) min_keycode: u8,
    /// The highest keycode the server has.
    pub(crate) max_keycode: u8,
}

/// Why a server's message was not read.
fn unread(what: &str) -> Refused {
    Refused::Failed(format!(
        "the X server sent {what}, which is not the protocol this reads, so the connection was \
         closed"
    ))
}

/// Why a connection ended before a message did.
fn cut_short(error: &std::io::Error) -> Refused {
    Refused::Failed(format!(
        "the X server's connection ended before its answer did: {error}"
    ))
}

/// Exactly `N` bytes of `from`.
fn exactly<const N: usize>(from: &mut impl Read) -> Result<[u8; N], Refused> {
    let mut bytes = [0_u8; N];
    from.read_exact(&mut bytes)
        .map_err(|error| cut_short(&error))?;
    Ok(bytes)
}

/// `length` bytes of `from`, a length already held to a bound by its caller.
fn bytes_of(from: &mut impl Read, length: usize) -> Result<Vec<u8>, Refused> {
    let mut bytes = vec![0_u8; length];
    from.read_exact(&mut bytes)
        .map_err(|error| cut_short(&error))?;
    Ok(bytes)
}

/// Two bytes of `bytes` at `at`, least significant first.
fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

/// Four bytes of `bytes` at `at`, least significant first.
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// The server's answer to [`setup_request`], read from `from`.
///
/// **Bounded by the protocol itself**: the answer's length is two bytes of
/// four-byte units, a quarter of a megabyte at most, read whole before any of
/// it is believed, and every count inside it is held to what was read.
///
/// # Errors
///
/// [`Refused::Failed`] when the server refuses the connection, with the
/// reason it gives; when it asks for an authorization this does not speak; and
/// when what it sends ends early or is not a setup.
pub(crate) fn read_setup(from: &mut impl Read) -> Result<Setup, Refused> {
    let head: [u8; 8] = exactly(from)?;
    let length = usize::from(u16::from_le_bytes([head[6], head[7]])) * 4;
    let body = bytes_of(from, length)?;
    match head[0] {
        1 => setup_of(&body).ok_or_else(|| unread("a setup that ends before its own counts do")),
        0 => {
            let reason = body.get(..usize::from(head[1])).unwrap_or(&body);
            let reason: String = String::from_utf8_lossy(reason)
                .chars()
                .filter(|character| !character.is_control())
                .take(REASON_CHARS_MAX)
                .collect();
            Err(Refused::Failed(format!(
                "the X server refused the connection: {reason}"
            )))
        }
        _ => Err(unread("a request for an authorization this does not speak")),
    }
}

/// What an accepted setup's `body` says, or `None` where it ends early.
fn setup_of(body: &[u8]) -> Option<Setup> {
    let vendor = usize::from(u16_at(body, 16)?);
    let screens = *body.get(20)?;
    let formats = usize::from(*body.get(21)?);
    let min_keycode = *body.get(26)?;
    let max_keycode = *body.get(27)?;
    let mut rest = body.get(32 + padded(vendor) + 8 * formats..)?;
    let mut roots = Vec::new();
    for _ in 0..screens {
        let screen = taken(&mut rest, 40)?;
        roots.push(u32_at(screen, 0)?);
        for _ in 0..screen[39] {
            let depth = taken(&mut rest, 8)?;
            taken(&mut rest, usize::from(u16_at(depth, 2)?) * 24)?;
        }
    }
    (!roots.is_empty()).then_some(Setup {
        roots,
        min_keycode,
        max_keycode,
    })
}

/// `GetModifierMapping`: which keycodes each of the eight modifiers is.
pub(crate) const fn get_modifier_mapping() -> [u8; 4] {
    [119, 0, 1, 0]
}

/// `GetInputFocus`: asked for its reply alone, which the server sends after
/// it has answered every request before it.
pub(crate) const fn get_input_focus() -> [u8; 4] {
    [43, 0, 1, 0]
}

/// `GrabKey` of `keycode` held with exactly `modifiers`, on `window`.
///
/// Both devices keep running (`Asynchronous`), and events are reported to the
/// grab window and not to the window they happen in (`owner-events` false).
pub(crate) fn grab_key(window: u32, modifiers: u16, keycode: u8) -> [u8; 16] {
    let window = window.to_le_bytes();
    let modifiers = modifiers.to_le_bytes();
    [
        33,
        0,
        4,
        0,
        window[0],
        window[1],
        window[2],
        window[3],
        modifiers[0],
        modifiers[1],
        keycode,
        1,
        1,
        0,
        0,
        0,
    ]
}

/// `UngrabKey`: the grab [`grab_key`] made, let go.
pub(crate) fn ungrab_key(window: u32, modifiers: u16, keycode: u8) -> [u8; 12] {
    let window = window.to_le_bytes();
    let modifiers = modifiers.to_le_bytes();
    [
        34,
        keycode,
        3,
        0,
        window[0],
        window[1],
        window[2],
        window[3],
        modifiers[0],
        modifiers[1],
        0,
        0,
    ]
}

/// `UngrabKeyboard` at the server's own time: the keyboard a firing grab
/// handed this client, handed back.
pub(crate) const fn ungrab_keyboard() -> [u8; 8] {
    [32, 0, 2, 0, 0, 0, 0, 0]
}

/// `QueryKeymap`: which keys are down, asked while the Leader's key is and
/// read for that key alone.
pub(crate) const fn query_keymap() -> [u8; 4] {
    [44, 0, 1, 0]
}

/// Whether the key numbered `keycode` is down in a `QueryKeymap` reply: a bit
/// a key, the first twenty-four bytes in the reply's `head` and the last
/// eight in its `rest`.
///
/// **A reply that is not a keymap's length says the key is up**, so a server
/// answering what was not asked ends the wait rather than holding it.
pub(crate) fn is_down(head: &[u8; 24], rest: &[u8], keycode: u8) -> bool {
    let at = usize::from(keycode / 8);
    let byte = if rest.len() == 8 {
        head.get(at).or_else(|| rest.get(at - 24))
    } else {
        None
    };
    byte.is_some_and(|byte| byte & (1 << (keycode % 8)) != 0)
}

/// One message of the server's, after the setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Packet {
    /// The answer to the request numbered `sequence`.
    Reply {
        /// Which request it answers, in the sixteen bits the server sends.
        sequence: u16,
        /// The byte a reply carries beside its code.
        detail: u8,
        /// What its first thirty-two bytes carry past their header of eight.
        head: [u8; 24],
        /// What it carries past its first thirty-two bytes.
        rest: Vec<u8>,
    },
    /// A request refused.
    Error {
        /// Why, as the protocol numbers it.
        code: u8,
        /// Which request, in the sixteen bits the server sends.
        sequence: u16,
    },
    /// Something that happened.
    Event {
        /// Which, as the protocol numbers it, whoever sent it.
        code: u8,
        /// The byte an event carries beside its code: a key event's keycode.
        detail: u8,
    },
}

/// The next message of `from`.
///
/// # Errors
///
/// [`Refused::Failed`] when the connection ends, and when a message says it
/// is longer than [`EXTRA_BYTES_MAX`] — before a byte of that length is read
/// or allocated.
pub(crate) fn packet(from: &mut impl Read) -> Result<Packet, Refused> {
    let head: [u8; 32] = exactly(from)?;
    let sequence = u16::from_le_bytes([head[2], head[3]]);
    let code = head[0] & 0x7f;
    if head[0] == 0 {
        return Ok(Packet::Error {
            code: head[1],
            sequence,
        });
    }
    if head[0] != 1 && code != GENERIC_EVENT {
        return Ok(Packet::Event {
            code,
            detail: head[1],
        });
    }
    let units = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    let length = usize::try_from(units)
        .ok()
        .and_then(|units| units.checked_mul(4))
        .filter(|length| *length <= EXTRA_BYTES_MAX)
        .ok_or_else(|| unread("a message longer than any this asks for"))?;
    let rest = bytes_of(from, length)?;
    Ok(if head[0] == 1 {
        let mut carried = [0_u8; 24];
        carried.copy_from_slice(&head[8..]);
        Packet::Reply {
            sequence,
            detail: head[1],
            head: carried,
            rest,
        }
    } else {
        Packet::Event {
            code,
            detail: head[1],
        }
    })
}

/// The keycode a Linux X server gives the key the kernel numbers `evdev`.
pub(crate) const fn keycode(evdev: u8) -> u8 {
    evdev + EVDEV_OFFSET
}

/// The bit of the modifier that holds `keycode` in `map`, the eight rows
/// of `per` keycodes a `GetModifierMapping` reply carries.
fn modifier_holding(map: &[u8], per: usize, keycode: u8) -> Option<u16> {
    if per == 0 {
        return None;
    }
    map.chunks_exact(per)
        .position(|row| row.contains(&keycode))
        .and_then(|row| u16::try_from(row).ok())
        .map(|row| 1 << row)
}

/// The modifier masks a chord is grabbed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Masks {
    /// The chord's own modifiers, and no other.
    pub(crate) chord: u16,
    /// The modifier Num Lock is, where the server has one apart from the
    /// chord's.
    pub(crate) num_lock: Option<u16>,
}

impl Masks {
    /// The masks `modifiers` are on a server whose `GetModifierMapping` reply
    /// carried `per` keycodes a modifier and `map`.
    ///
    /// **Read from the server's map by physical key**, and refused where the
    /// map cannot tell the chord's modifiers apart.
    ///
    /// # Errors
    ///
    /// [`Refused::Failed`] when the map's length is not eight rows of `per`,
    /// when Alt or Super is held and no modifier past the protocol's fixed
    /// three holds its key, and when Alt and Super are one modifier.
    pub(crate) fn read(modifiers: Modifiers, per: u8, map: &[u8]) -> Result<Self, Refused> {
        let per = usize::from(per);
        if map.len() != 8 * per {
            return Err(unread("a modifier map whose length is not its header's"));
        }
        let found = |evdev: u8| {
            modifier_holding(map, per, keycode(evdev))
                .filter(|bit| *bit & (SHIFT | LOCK | CONTROL) == 0)
        };
        let unmapped = |name: &str| {
            Refused::Failed(format!(
                "the X server's modifier map puts the left {name} key in no modifier of its own, \
                 so a chord holding {name} cannot be told from another and was not grabbed"
            ))
        };
        let mut chord = 0;
        for modifier in modifiers.iter() {
            chord |= match modifier {
                Modifier::Shift => SHIFT,
                Modifier::Control => CONTROL,
                Modifier::Alt => found(EVDEV_LEFT_ALT).ok_or_else(|| unmapped("Alt"))?,
                Modifier::Super => found(EVDEV_LEFT_SUPER).ok_or_else(|| unmapped("Super"))?,
            };
        }
        if found(EVDEV_LEFT_ALT).is_some() && found(EVDEV_LEFT_ALT) == found(EVDEV_LEFT_SUPER) {
            return Err(unmapped("Super"));
        }
        let num_lock = found(EVDEV_NUM_LOCK).filter(|bit| chord & bit == 0);
        Ok(Self { chord, num_lock })
    }

    /// Every mask the chord is grabbed with: itself, and with each lock that
    /// may be left on.
    pub(crate) fn variants(self) -> Vec<u16> {
        let mut variants = vec![self.chord, self.chord | LOCK];
        if let Some(num_lock) = self.num_lock {
            variants.push(self.chord | num_lock);
            variants.push(self.chord | LOCK | num_lock);
        }
        variants
    }
}

/// What the server made of the grabs, read from `from` up to the reply
/// numbered `until`.
///
/// # Errors
///
/// [`Refused::Taken`] when any grab was answered `BadAccess`: another client
/// holds the chord. [`Refused::Failed`] for any other error, and for a
/// connection that ends first.
pub(crate) fn grabs_answered(from: &mut impl Read, until: u16) -> Result<(), Refused> {
    let mut verdict = Ok(());
    loop {
        match packet(from)? {
            Packet::Reply { sequence, .. } if sequence == until => return verdict,
            Packet::Error {
                code: BAD_ACCESS, ..
            } => verdict = Err(Refused::Taken),
            Packet::Error { code, .. } => {
                if verdict.is_ok() {
                    verdict = Err(Refused::Failed(format!(
                        "the X server refused the grab with error {code}"
                    )));
                }
            }
            Packet::Reply { .. } | Packet::Event { .. } => {}
        }
    }
}

/// A connection to an X server, as the session needs one: read and written,
/// held by two threads, bounded and closed.
///
/// **A trait, so the session is code every system compiles and tests.** The
/// one connection a session is ever handed outside a test is a Unix socket;
/// the tests hand it a pipe of their own, which every system has.
pub(crate) trait Wire: Read + std::io::Write + Send + Sized + 'static {
    /// A second handle of the same connection, or why the system made none.
    fn second(&self) -> std::io::Result<Self>;

    /// Bounds each read and each write to `bound`, or lifts the bound.
    fn within(&self, bound: Option<std::time::Duration>) -> std::io::Result<()>;

    /// Closes the connection both ways, which ends a read waiting on any
    /// handle of it.
    fn close(&self);
}

/// The socket an X server on this machine is reached through.
#[cfg(unix)]
pub(crate) type Socket = std::os::unix::net::UnixStream;

#[cfg(unix)]
impl Wire for Socket {
    fn second(&self) -> std::io::Result<Self> {
        self.try_clone()
    }

    fn within(&self, bound: Option<std::time::Duration>) -> std::io::Result<()> {
        bounded(self, bound)
    }

    fn close(&self) {
        let _ = self.shutdown(std::net::Shutdown::Both);
    }
}

/// Bounds each read and each write of `socket` to `bound`, or lifts the
/// bound: what keeps a server that takes a connection and never answers from
/// holding a registration.
#[cfg(unix)]
fn bounded(socket: &Socket, bound: Option<std::time::Duration>) -> std::io::Result<()> {
    socket.set_read_timeout(bound)?;
    socket.set_write_timeout(bound)
}

/// Whether a socket file that the user numbered `owner` owns is one the user
/// numbered `me` speaks to: its own, or the system's.
///
/// **Asked before a byte is sent**, since what is sent first carries this
/// display's cookie. `/tmp/.X11-unix` is a directory every account may make a
/// file in, so where no server holds a display's socket another account can
/// put one there; a server this user reaches is one this user started, or one
/// the system did, as root.
#[allow(
    dead_code,
    reason = "asked where a socket is a file with an owner, which is every system but one, and \
              tested on all of them"
)]
pub(crate) const fn answers_for(owner: u32, me: u32) -> bool {
    owner == me || owner == 0
}

/// A connection to the server of `display`: its socket in `/tmp`, and
/// on Linux, where nothing answers there, the abstract socket of that name.
///
/// # Errors
///
/// [`Refused::Failed`] when nothing answers at either, and when the socket's
/// file is another account's.
#[cfg(unix)]
pub(crate) fn dial(display: &Display) -> Result<Socket, Refused> {
    dial_as(display, rustix::process::geteuid().as_raw())
}

/// As [`dial`], for the user numbered `me`.
///
/// **A socket file is judged by its owner before it is connected to**
/// ([`answers_for`]), and one refused is not gone round: the abstract socket
/// is tried only where no file answers at all. The file is read and then
/// connected to, two steps, and what stands between them is the directory's
/// sticky bit, under which only a file's owner replaces it.
///
/// # Errors
///
/// As [`dial`].
#[cfg(unix)]
pub(crate) fn dial_as(display: &Display, me: u32) -> Result<Socket, Refused> {
    use std::os::unix::fs::MetadataExt as _;

    let socket = display.socket();
    if let Ok(found) = std::fs::metadata(&socket)
        && !answers_for(found.uid(), me)
    {
        return Err(Refused::Failed(format!(
            "the socket at `{socket}`, the display `DISPLAY` names, belongs to another account, \
             user {}, so nothing was sent to it",
            found.uid()
        )));
    }
    let by_path = Socket::connect(&socket);
    #[cfg(target_os = "linux")]
    let by_path = by_path.or_else(|error| {
        use std::os::linux::net::SocketAddrExt as _;
        if display.path.is_some() {
            return Err(error);
        }
        std::os::unix::net::SocketAddr::from_abstract_name(socket.as_bytes())
            .and_then(|address| Socket::connect_addr(&address))
    });
    by_path.map_err(|error| {
        Refused::Failed(format!(
            "no X server answers at `{socket}`, the display `DISPLAY` names: {error}"
        ))
    })
}

/// The socket an X server on this machine is reached through: on Windows
/// there is none, and so no value of this type.
#[cfg(windows)]
#[derive(Debug)]
pub(crate) enum Socket {}

#[cfg(windows)]
impl Read for Socket {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        match *self {}
    }
}

#[cfg(windows)]
impl std::io::Write for Socket {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        match *self {}
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match *self {}
    }
}

#[cfg(windows)]
impl Wire for Socket {
    fn second(&self) -> std::io::Result<Self> {
        match *self {}
    }

    fn within(&self, _: Option<std::time::Duration>) -> std::io::Result<()> {
        match *self {}
    }

    fn close(&self) {
        match *self {}
    }
}

/// A connection to the server of `display`, which Windows has no socket for.
///
/// # Errors
///
/// [`Refused::Failed`], always: no session of Windows' is an X server's.
#[cfg(windows)]
pub(crate) fn dial(display: &Display) -> Result<Socket, Refused> {
    Err(Refused::Failed(format!(
        "no X server answers at `{}`, the display `DISPLAY` names: this system has no such \
         socket",
        display.socket()
    )))
}

/// The connection itself: the requests in order, and the thread that reads
/// the presses.
pub(crate) mod session {
    use std::ffi::OsString;
    use std::io::Read as _;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use nc_schema::keys::Modifiers;

    use super::{
        AUTHORITY_BYTES_MAX, Display, KEY_PRESS, Masks, Packet, Socket, Wire, cookie, dial,
        get_input_focus, get_modifier_mapping, grab_key, grabs_answered, is_down, packet,
        query_keymap, read_setup, setup_request, ungrab_key, ungrab_keyboard,
    };
    use crate::leader::{OnLost, OnPress, Refused};

    /// What a registration tells its caller, each from the thread that reads
    /// the presses and with nothing held.
    pub(crate) struct Told {
        /// Called once for each time the chord goes down.
        pub(crate) on_press: OnPress,
        /// Called once, if ever, when the chord is lost.
        ///
        /// That is when it stops being this client's with the registration
        /// still held: the server went away, or the chord could not be
        /// grabbed again after a press.
        pub(crate) on_lost: OnLost,
    }

    /// How long a server may take over each read and write of a registration.
    ///
    /// Far past what a server on this machine takes, and a bound on a
    /// registration that one which never answers would otherwise hold.
    pub(crate) const WITHIN: Duration = Duration::from_secs(5);

    /// How often the server is asked whether the Leader's key is still down,
    /// from its press to its release.
    ///
    /// Short of the gap a person leaves between letting a key go and pressing
    /// it again, which is some tens of milliseconds at the fastest, so the
    /// grabs are made again before the next press.
    pub(crate) const ASKED_EVERY: Duration = Duration::from_millis(10);

    /// How long a chord another client took while the Leader was held is
    /// left before it is asked for again.
    pub(crate) const TRIED_EVERY: Duration = Duration::from_millis(250);

    /// How many times the chord is asked for again once its key is up, before
    /// it is given up as lost: five seconds of [`TRIED_EVERY`].
    ///
    /// Long enough for a program that asked only whether the chord was free
    /// to have let it go, and short enough that the caller is told while the
    /// person still remembers which program they started.
    pub(crate) const TRIES: u32 = 20;

    /// How long each wait of a session is.
    #[derive(Debug, Clone, Copy)]
    pub(crate) struct Pace {
        /// The bound on each read and write of a registration.
        pub(crate) within: Duration,
        /// How often the server is asked whether the key is still down.
        pub(crate) asked_every: Duration,
        /// How long a chord found taken is left before it is asked for again.
        pub(crate) tried_every: Duration,
        /// How many times the chord is asked for before it is lost.
        pub(crate) tries: u32,
    }

    impl Pace {
        /// The pace of every session outside a test.
        pub(crate) const USUAL: Self = Self {
            within: WITHIN,
            asked_every: ASKED_EVERY,
            tried_every: TRIED_EVERY,
            tries: TRIES,
        };
    }

    /// The connection's writing half, and how many requests have crossed it.
    struct Writer<W> {
        /// The connection.
        stream: W,
        /// The number of the last request sent, in the sixteen bits a reply
        /// carries.
        sent: u16,
        /// Whether the registration was dropped, which closes the connection
        /// and is no loss to tell its caller of.
        dropped: bool,
    }

    impl<W: Wire> Writer<W> {
        /// Sends one `request`, and answers the number the server gives it.
        fn send(&mut self, request: &[u8]) -> std::io::Result<u16> {
            self.stream.write_all(request)?;
            self.sent = self.sent.wrapping_add(1);
            Ok(self.sent)
        }
    }

    /// A chord grabbed on a server: let go, and its connection closed, when
    /// this drops.
    pub(crate) struct Grabbed<W: Wire = Socket> {
        /// The connection's writing half, shared with the thread that reads
        /// the presses.
        writer: Arc<Mutex<Writer<W>>>,
        /// Each grab made: the window, the mask and the keycode.
        grabs: Vec<(u32, u16, u8)>,
        /// The thread reading the presses.
        reader: Option<std::thread::JoinHandle<()>>,
    }

    impl<W: Wire> core::fmt::Debug for Grabbed<W> {
        fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter
                .debug_struct("Grabbed")
                .field("grabs", &self.grabs)
                .finish_non_exhaustive()
        }
    }

    impl<W: Wire> Drop for Grabbed<W> {
        /// Lets every grab go, closes the connection, and waits for the
        /// reading thread — which a closed connection ends — unless this is
        /// that thread.
        fn drop(&mut self) {
            if let Ok(mut writer) = self.writer.lock() {
                writer.dropped = true;
                for (window, mask, keycode) in &self.grabs {
                    let _ = writer.send(&ungrab_key(*window, *mask, *keycode));
                }
                writer.stream.close();
            }
            if let Some(reader) = self.reader.take()
                && reader.thread().id() != std::thread::current().id()
            {
                let _ = reader.join();
            }
        }
    }

    /// Why the server could not be asked.
    fn unsent(error: &std::io::Error) -> Refused {
        Refused::Failed(format!("the X server could not be asked: {error}"))
    }

    /// This machine's host name, as an authorization file names it.
    ///
    /// The kernel's, which is where `xauth` reads it from; `None` where the
    /// kernel does not say, and then no entry is this machine's by name.
    pub(crate) fn host() -> Option<Vec<u8>> {
        let name = std::fs::read("/proc/sys/kernel/hostname").ok()?;
        Some(name.trim_ascii().to_vec())
    }

    /// The cookie this user's authorization file holds for `display` of the
    /// machine named `host`, if the environment `variable` reads names a file
    /// that holds one.
    ///
    /// **The file is the one `XAUTHORITY` names, and `.Xauthority` under
    /// `HOME` only where it names none**, and of either no more than
    /// [`AUTHORITY_BYTES_MAX`] is read: a longer file is one cut short, which
    /// holds no cookie ([`cookie`]).
    pub(crate) fn cookie_in(
        display: &Display,
        variable: &dyn Fn(&str) -> Option<OsString>,
        host: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        let file = variable("XAUTHORITY")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                variable("HOME")
                    .filter(|home| !home.is_empty())
                    .map(|home| PathBuf::from(home).join(".Xauthority"))
            })?;
        let mut bytes = Vec::new();
        std::fs::File::open(file)
            .ok()?
            .take(AUTHORITY_BYTES_MAX)
            .read_to_end(&mut bytes)
            .ok()?;
        cookie(&bytes, host, display.number)
    }

    /// Grabs the key the kernel numbers `evdev`, held with `modifiers`, on
    /// the server the environment `variable` reads names, and tells `told`
    /// of each press, and of the chord lost.
    ///
    /// # Errors
    ///
    /// As [`register_paced`].
    pub(crate) fn register(
        evdev: u8,
        modifiers: Modifiers,
        variable: &dyn Fn(&str) -> Option<OsString>,
        told: Told,
    ) -> Result<Grabbed, Refused> {
        register_paced(evdev, modifiers, variable, Pace::USUAL, told)
    }

    /// As [`register`], with each wait as long as `pace` says.
    ///
    /// # Errors
    ///
    /// What [`over`] refuses; [`Refused::NoSession`] when `DISPLAY` is not
    /// set; and [`Refused::Failed`] when it is set to what is not a text or
    /// names no display on this machine, when no server answers there, and
    /// when the socket there is another account's.
    pub(crate) fn register_paced(
        evdev: u8,
        modifiers: Modifiers,
        variable: &dyn Fn(&str) -> Option<OsString>,
        pace: Pace,
        told: Told,
    ) -> Result<Grabbed, Refused> {
        let named = variable("DISPLAY").ok_or(Refused::NoSession)?;
        // Set, and not a text: a display this cannot read, which is another
        // refusal than a session that names none.
        let Some(named) = named.to_str() else {
            return Err(Display::unreadable(&named.to_string_lossy()));
        };
        let display = Display::parse(named)?;
        let stream = dial(&display)?;
        let cookie = cookie_in(&display, variable, host().as_deref());
        over(stream, cookie.as_deref(), evdev, modifiers, pace, told)
    }

    /// Grabs the chord over `stream`, a connection no message has crossed.
    ///
    /// # Errors
    ///
    /// [`Refused::Taken`] when another client holds the chord, with every
    /// grab this made let go as the connection closes; [`Refused::Failed`]
    /// when the server refuses the connection, answers what this does not
    /// read, has no such key, or cannot say which modifier the chord's are.
    pub(crate) fn over<W: Wire>(
        stream: W,
        cookie: Option<&[u8]>,
        evdev: u8,
        modifiers: Modifiers,
        pace: Pace,
        told: Told,
    ) -> Result<Grabbed<W>, Refused> {
        stream
            .within(Some(pace.within))
            .map_err(|error| unsent(&error))?;
        let mut reading = stream.second().map_err(|error| unsent(&error))?;
        let mut writer = Writer {
            stream,
            sent: 0,
            dropped: false,
        };
        writer
            .stream
            .write_all(&setup_request(cookie))
            .map_err(|error| unsent(&error))?;
        let setup = read_setup(&mut reading)?;
        let keycode = super::keycode(evdev);
        if !(setup.min_keycode..=setup.max_keycode).contains(&keycode) {
            return Err(Refused::Failed(format!(
                "the X server has no key numbered {keycode}, so the chord was not grabbed"
            )));
        }
        writer
            .send(&get_modifier_mapping())
            .map_err(|error| unsent(&error))?;
        let Packet::Reply { detail, rest, .. } = packet(&mut reading)? else {
            return Err(Refused::Failed(
                "the X server did not answer with its modifier map, so the chord was not grabbed"
                    .to_owned(),
            ));
        };
        let masks = Masks::read(modifiers, detail, &rest)?;
        let grabs: Vec<(u32, u16, u8)> = setup
            .roots
            .iter()
            .flat_map(|root| {
                masks
                    .variants()
                    .into_iter()
                    .map(move |mask| (*root, mask, keycode))
            })
            .collect();
        let until = grab(&mut writer, &grabs)?;
        grabs_answered(&mut reading, until)?;
        writer.stream.within(None).map_err(|error| unsent(&error))?;
        let writer = Arc::new(Mutex::new(writer));
        let (shared, grabbed) = (Arc::clone(&writer), grabs.clone());
        let reader = std::thread::Builder::new()
            .name("nc-leader-x11".to_owned())
            .spawn(move || {
                let Told { on_press, on_lost } = told;
                let Err(why) =
                    read_presses(&shared, &mut reading, &grabbed, keycode, pace, &on_press);
                // The reading ended. Where the drop of the `Grabbed` ended
                // it, by closing the connection, nobody is told; anywhere
                // else the chord is this client's no longer, so the
                // connection is closed — which lets go whatever the server
                // still holds for it — and the caller is told why, with the
                // writer let go first.
                let dropped = shared.lock().map_or(true, |writer| {
                    if !writer.dropped {
                        writer.stream.close();
                    }
                    writer.dropped
                });
                if !dropped {
                    on_lost(why);
                }
            })
            .map_err(|error| {
                Refused::Failed(format!(
                    "the thread reading the Leader did not start: {error}"
                ))
            })?;
        Ok(Grabbed {
            writer,
            grabs,
            reader: Some(reader),
        })
    }

    /// Asks for every grab of `grabs`, and answers the number of the request
    /// whose reply follows whatever the server makes of them
    /// ([`grabs_answered`]).
    ///
    /// # Errors
    ///
    /// [`Refused::Failed`] when the server is gone.
    fn grab<W: Wire>(writer: &mut Writer<W>, grabs: &[(u32, u16, u8)]) -> Result<u16, Refused> {
        for (window, mask, keycode) in grabs {
            writer
                .send(&grab_key(*window, *mask, *keycode))
                .map_err(|error| unsent(&error))?;
        }
        writer
            .send(&get_input_focus())
            .map_err(|error| unsent(&error))
    }

    /// Reads the server's messages for as long as the connection lasts, and
    /// calls `on_press` once for each time the key numbered `keycode` goes
    /// down.
    ///
    /// **At a press everything is let go** — the keyboard the firing grab
    /// handed over, and the grabs themselves, which the server would fire
    /// again for each repeat — and the caller is told; then the server is
    /// asked, every [`Pace::asked_every`], whether the key is still down, and
    /// once it is not the grabs are made again ([`regrab`]).
    ///
    /// # Errors
    ///
    /// Why the reading ended, which it only ever does so: the connection
    /// closed, or the chord was not this client's to grab again.
    fn read_presses<W: Wire>(
        writer: &Mutex<Writer<W>>,
        reading: &mut W,
        grabs: &[(u32, u16, u8)],
        keycode: u8,
        pace: Pace,
        on_press: &OnPress,
    ) -> Result<core::convert::Infallible, Refused> {
        loop {
            if !pressed(&packet(reading)?, keycode) {
                continue;
            }
            let_go(writer, grabs)?;
            on_press();
            loop {
                std::thread::sleep(pace.asked_every);
                let asked = writer
                    .lock()
                    .map_err(|_| gone())?
                    .send(&query_keymap())
                    .map_err(|error| unsent(&error))?;
                let down = loop {
                    if let Packet::Reply {
                        sequence,
                        head,
                        rest,
                        ..
                    } = packet(reading)?
                        && sequence == asked
                    {
                        break is_down(&head, &rest, keycode);
                    }
                };
                if !down {
                    break;
                }
            }
            regrab(writer, reading, grabs, pace)?;
        }
    }

    /// Why a writer another thread panicked holding cannot be written through.
    fn gone() -> Refused {
        Refused::Failed("the connection's writer is gone".to_owned())
    }

    /// Hands back the keyboard a firing grab handed over, and lets every
    /// grab of `grabs` go.
    ///
    /// Each is nothing to a server that holds none of it for this client.
    ///
    /// # Errors
    ///
    /// [`Refused::Failed`] when the server is gone.
    fn let_go<W: Wire>(writer: &Mutex<Writer<W>>, grabs: &[(u32, u16, u8)]) -> Result<(), Refused> {
        let mut writer = writer.lock().map_err(|_| gone())?;
        writer
            .send(&ungrab_keyboard())
            .map_err(|error| unsent(&error))?;
        for (window, mask, keycode) in grabs {
            writer
                .send(&ungrab_key(*window, *mask, *keycode))
                .map_err(|error| unsent(&error))?;
        }
        Ok(())
    }

    /// Makes the grabs again, once the key is up.
    ///
    /// **The chord was nobody's while the key was down**, so another client
    /// may have been granted it. Where the server says so, whatever part of
    /// the chord it did grant is let go — the chord is held whole or not at
    /// all — and it is asked again after [`Pace::tried_every`], up to
    /// [`Pace::tries`] times in all: a program that asked only whether the
    /// chord was free has let it go by then, and one that keeps it has taken
    /// it.
    ///
    /// # Errors
    ///
    /// [`Refused::Taken`] when the last try is refused as the first was, and
    /// [`Refused::Failed`] when the server refuses for another reason or is
    /// gone.
    fn regrab<W: Wire>(
        writer: &Mutex<Writer<W>>,
        reading: &mut W,
        grabs: &[(u32, u16, u8)],
        pace: Pace,
    ) -> Result<(), Refused> {
        let mut tried = 1;
        loop {
            let until = grab(&mut *writer.lock().map_err(|_| gone())?, grabs)?;
            match grabs_answered(reading, until) {
                Err(Refused::Taken) if tried < pace.tries => {
                    tried += 1;
                    let_go(writer, grabs)?;
                    std::thread::sleep(pace.tried_every);
                }
                answered => return answered,
            }
        }
    }

    /// Whether `read` is the press of the key numbered `keycode`: the one
    /// event this client acts on.
    pub(crate) fn pressed(read: &Packet, keycode: u8) -> bool {
        matches!(read, Packet::Event { code, detail } if *code == KEY_PRESS && *detail == keycode)
    }
}

#[cfg(test)]
mod tests {
    use nc_schema::keys::{Modifier, Modifiers};

    use super::session::{cookie_in, host};
    use super::{
        AUTHORITY_BYTES_MAX, BAD_ACCESS, Display, EXTRA_BYTES_MAX, GENERIC_EVENT, KEY_PRESS, Masks,
        Packet, answers_for, cookie, get_input_focus, get_modifier_mapping, grab_key,
        grabs_answered, is_down, keycode, packet, query_keymap, read_setup, setup_request,
        ungrab_key, ungrab_keyboard,
    };
    use crate::leader::Refused;

    /// A directory of one test's own, removed with it.
    pub(super) struct Scratch(pub(super) std::path::PathBuf);

    impl Scratch {
        /// An empty directory named for `name` and this process.
        ///
        /// The name is kept short, since a socket's path made under it has to
        /// fit the hundred or so bytes a system gives one.
        pub(super) fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("nc-x11-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("a scratch directory");
            Self(root)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The modifier Alt is in [`a_modifier_map`]: `Mod1`.
    pub(super) const ALT: u16 = 1 << 3;
    /// The modifier Num Lock is in [`a_modifier_map`]: `Mod2`.
    pub(super) const NUM_LOCK: u16 = 1 << 4;
    /// The modifier Super is in [`a_modifier_map`]: `Mod4`.
    pub(super) const SUPER: u16 = 1 << 6;
    /// Space's keycode on a Linux X server.
    pub(super) const SPACE: u8 = 65;

    /// The map an Xorg server starts with, two keycodes a modifier.
    ///
    /// The protocol's Shift, Lock and Control, then Alt in `Mod1`, with Num
    /// Lock in `Mod2`, nothing in `Mod3`, the Super keys in `Mod4` and the
    /// third level in `Mod5`.
    pub(super) fn a_modifier_map() -> Vec<u8> {
        vec![
            50, 62, 66, 0, 37, 105, 64, 108, 77, 0, 0, 0, 133, 134, 92, 203,
        ]
    }

    /// A reply numbered `sequence`, carrying `detail`, `head` after its
    /// header and `rest` after its first thirty-two bytes.
    pub(super) fn a_reply(sequence: u16, detail: u8, head: &[u8], rest: &[u8]) -> Vec<u8> {
        let mut reply = vec![1, detail];
        reply.extend(sequence.to_le_bytes());
        reply.extend(u32::try_from(rest.len() / 4).expect("short").to_le_bytes());
        reply.extend(head);
        reply.resize(32, 0);
        reply.extend(rest);
        reply
    }

    /// The error `code` for the request numbered `sequence`.
    pub(super) fn an_error(code: u8, sequence: u16) -> Vec<u8> {
        let mut error = vec![0, code];
        error.extend(sequence.to_le_bytes());
        error.resize(32, 0);
        error
    }

    /// The event `code` of the key numbered `key`.
    pub(super) fn a_key_event(code: u8, key: u8) -> Vec<u8> {
        let mut event = vec![code, key];
        event.resize(32, 0);
        event
    }

    /// An accepted setup of a server with `roots`, one to a screen, each
    /// screen of one depth with two visuals, and keys numbered 8 to `highest`.
    pub(super) fn a_setup(roots: &[u32], highest: u8) -> Vec<u8> {
        let vendor = b"a test";
        let mut body = vec![0_u8; 16];
        body.extend(u16::try_from(vendor.len()).expect("short").to_le_bytes());
        body.extend([0xff, 0xff]);
        body.push(u8::try_from(roots.len()).expect("few"));
        body.push(1);
        body.extend([0, 0, 32, 32, 8, highest, 0, 0, 0, 0]);
        body.extend(vendor);
        body.extend([0, 0]);
        body.extend([24, 32, 32, 0, 0, 0, 0, 0]);
        for root in roots {
            let mut screen = vec![0_u8; 40];
            screen[..4].copy_from_slice(&root.to_le_bytes());
            screen[39] = 1;
            body.extend(screen);
            body.extend([24, 0, 2, 0, 0, 0, 0, 0]);
            body.extend([0_u8; 48]);
        }
        let mut setup = vec![1, 0, 11, 0, 0, 0];
        setup.extend(u16::try_from(body.len() / 4).expect("short").to_le_bytes());
        setup.extend(body);
        setup
    }

    /// A refused setup, with the server's `reason`.
    pub(super) fn a_refusal(reason: &[u8]) -> Vec<u8> {
        let mut body = reason.to_vec();
        body.resize(reason.len().div_ceil(4) * 4, 0);
        let mut setup = vec![0, u8::try_from(reason.len()).expect("short"), 11, 0, 0, 0];
        setup.extend(u16::try_from(body.len() / 4).expect("short").to_le_bytes());
        setup.extend(body);
        setup
    }

    /// One entry of an authorization file.
    pub(super) fn an_entry(
        family: u16,
        address: &[u8],
        display: &[u8],
        name: &[u8],
        data: &[u8],
    ) -> Vec<u8> {
        let mut entry = family.to_be_bytes().to_vec();
        for field in [address, display, name, data] {
            entry.extend(u16::try_from(field.len()).expect("short").to_be_bytes());
            entry.extend(field);
        }
        entry
    }

    /// Why a registration was refused, as a sentence, or what it was instead.
    fn why(refused: Result<impl core::fmt::Debug, Refused>) -> String {
        match refused {
            Err(Refused::Failed(why)) => why,
            other => format!("not a failure: {other:?}"),
        }
    }

    /// A display is `:N`, `unix:N` or a socket's path, with a screen or none,
    /// and its socket is the path, or the one in `/tmp` its number names.
    #[test]
    fn a_display_on_this_machine_is_read() {
        for (text, path, number, socket) in [
            (":0", None, 0, "/tmp/.X11-unix/X0"),
            (":1.0", None, 1, "/tmp/.X11-unix/X1"),
            ("unix:12", None, 12, "/tmp/.X11-unix/X12"),
            ("unix:12.3", None, 12, "/tmp/.X11-unix/X12"),
            (":65535", None, 65535, "/tmp/.X11-unix/X65535"),
            ("/run/x/socket:0", Some("/run/x/socket"), 0, "/run/x/socket"),
            ("/run/x:y/s:7.1", Some("/run/x:y/s"), 7, "/run/x:y/s"),
        ] {
            let display = Display::parse(text).expect(text);
            assert_eq!(display.path.as_deref(), path, "{text}");
            assert_eq!(display.number, number, "{text}");
            assert_eq!(display.socket(), socket, "{text}");
        }
    }

    /// A display with a host in its name is refused, since this dials no
    /// network, and so is every name that does not read.
    #[test]
    fn a_display_on_another_machine_or_unreadable_is_refused() {
        for text in [
            "",
            "0",
            ":",
            ":x",
            ":0.",
            ":0.x",
            ":.0",
            ":65536",
            ":-1",
            ":+1",
            ": 1",
            "host:0",
            "localhost:0",
            "tcp/host:0",
            "127.0.0.1:0",
            "::1:0",
            "relative/path:0",
            "/run/x/socket",
            "unix:",
        ] {
            let refused = why(Display::parse(text));
            assert!(
                refused.contains("names no display on this machine"),
                "`{text}`: {refused}"
            );
            assert!(refused.contains(&format!("`{text}`")), "{refused}");
        }
    }

    /// The cookie is the first entry's that is this machine's or any
    /// machine's, names this display or none, and is a magic cookie.
    #[test]
    fn the_cookie_is_the_entry_for_this_machine_and_this_display() {
        let magic = b"MIT-MAGIC-COOKIE-1";
        let file = [
            an_entry(256, b"elsewhere", b"0", magic, b"another machine's"),
            an_entry(0, b"\x7f\x00\x00\x01", b"0", magic, b"an address's"),
            an_entry(256, b"here", b"1", magic, b"another display's"),
            an_entry(
                256,
                b"here",
                b"0",
                b"XDM-AUTHORIZATION-1",
                b"another scheme's",
            ),
            an_entry(256, b"here", b"0", magic, b"this one"),
            an_entry(256, b"here", b"0", magic, b"a later one"),
            an_entry(65535, b"", b"", magic, b"any machine's"),
        ]
        .concat();
        assert_eq!(
            cookie(&file, Some(b"here"), 0).as_deref(),
            Some(&b"this one"[..])
        );
        assert_eq!(
            cookie(&file, Some(b"here"), 1).as_deref(),
            Some(&b"another display's"[..])
        );
        assert_eq!(
            cookie(&file, Some(b"here"), 2).as_deref(),
            Some(&b"any machine's"[..]),
            "an entry naming no display is every display's"
        );
        assert_eq!(
            cookie(&file, Some(b"nowhere"), 0).as_deref(),
            Some(&b"any machine's"[..])
        );
        assert_eq!(
            cookie(&file, None, 0).as_deref(),
            Some(&b"any machine's"[..]),
            "a machine with no name read is no entry's by name"
        );
        let named = an_entry(256, b"here", b"0", magic, b"this one");
        assert_eq!(cookie(&named, Some(b"nowhere"), 0), None);
        assert_eq!(cookie(&named, None, 0), None);
        assert_eq!(cookie(&[], Some(b"here"), 0), None);
        let wild_for_one = an_entry(65535, b"", b"10", magic, b"display ten's");
        assert_eq!(cookie(&wild_for_one, None, 1), None);
        assert_eq!(
            cookie(&wild_for_one, None, 10).as_deref(),
            Some(&b"display ten's"[..])
        );
    }

    /// A file that ends inside an entry holds no cookie, wherever it ends —
    /// an entry before the break included.
    #[test]
    fn an_authorization_file_cut_short_holds_no_cookie() {
        let magic = b"MIT-MAGIC-COOKIE-1";
        let whole = [
            an_entry(256, b"here", b"0", magic, b"this one"),
            an_entry(256, b"here", b"1", magic, b"the next"),
        ]
        .concat();
        let first = an_entry(256, b"here", b"0", magic, b"this one").len();
        for length in 1..whole.len() {
            let found = cookie(&whole[..length], Some(b"here"), 0);
            if length == first {
                assert_eq!(found.as_deref(), Some(&b"this one"[..]));
            } else {
                assert_eq!(found, None, "cut at {length}");
            }
        }
        let mut long = an_entry(256, b"here", b"0", magic, b"");
        let at = long.len() - 2;
        long[at..].copy_from_slice(&u16::MAX.to_be_bytes());
        long.extend([7_u8; 100]);
        assert_eq!(
            cookie(&long, Some(b"here"), 0),
            None,
            "a length past the file's end is not read"
        );
    }

    /// The cookie is read from the file `XAUTHORITY` names, from `.Xauthority`
    /// under `HOME` only where it names none, for the host it is asked for,
    /// and from no more of either than the bound.
    ///
    /// A file longer than the bound is one cut short, and holds no cookie.
    #[test]
    fn the_cookie_is_read_from_the_file_the_environment_names_up_to_the_bound() {
        use std::path::Path;

        let scratch = Scratch::new("cookie");
        let magic = b"MIT-MAGIC-COOKIE-1";
        let named = scratch.0.join("named");
        std::fs::write(&named, an_entry(256, b"here", b"0", magic, b"the named")).expect("written");
        let home = scratch.0.join("home");
        std::fs::create_dir(&home).expect("made");
        std::fs::write(
            home.join(".Xauthority"),
            an_entry(256, b"here", b"0", magic, b"the home's"),
        )
        .expect("written");
        let display = Display::parse(":0").expect("a display");
        let read_as = |variables: &[(&str, &Path)], host: Option<&[u8]>| {
            cookie_in(
                &display,
                &|name| {
                    variables
                        .iter()
                        .find(|(held, _)| *held == name)
                        .map(|(_, value)| value.as_os_str().to_owned())
                },
                host,
            )
        };
        let read = |variables: &[(&str, &Path)]| read_as(variables, Some(b"here"));
        assert_eq!(
            read(&[("XAUTHORITY", &named), ("HOME", &home)]).as_deref(),
            Some(&b"the named"[..]),
            "the file `XAUTHORITY` names is the one read"
        );
        assert_eq!(
            read(&[("HOME", &home)]).as_deref(),
            Some(&b"the home's"[..])
        );
        assert_eq!(
            read(&[("XAUTHORITY", Path::new("")), ("HOME", &home)]).as_deref(),
            Some(&b"the home's"[..]),
            "an empty `XAUTHORITY` names no file"
        );
        assert_eq!(
            read(&[("XAUTHORITY", &scratch.0.join("absent")), ("HOME", &home)]),
            None,
            "a file named and absent is not gone round"
        );
        assert_eq!(read(&[]), None);
        assert_eq!(read(&[("HOME", Path::new(""))]), None);
        assert_eq!(read(&[("HOME", &scratch.0)]), None);
        assert_eq!(
            read_as(&[("XAUTHORITY", &named)], Some(b"elsewhere")),
            None,
            "an entry is this machine's by the name asked for"
        );
        assert_eq!(read_as(&[("XAUTHORITY", &named)], None), None);

        let entry = an_entry(256, b"here", b"0", magic, b"0123456789abcdef");
        let bound = usize::try_from(AUTHORITY_BYTES_MAX).expect("a megabyte");
        assert_ne!(bound % entry.len(), 0, "the bound falls inside an entry");
        let fits = bound / entry.len();
        let under = scratch.0.join("under");
        std::fs::write(&under, entry.repeat(fits)).expect("written");
        assert_eq!(
            read(&[("XAUTHORITY", &under)]).as_deref(),
            Some(&b"0123456789abcdef"[..]),
            "a file of whole entries under the bound is read"
        );
        let over = scratch.0.join("over");
        std::fs::write(&over, entry.repeat(fits + 1)).expect("written");
        assert_eq!(
            read(&[("XAUTHORITY", &over)]),
            None,
            "a file past the bound is read up to it, which is a file cut short"
        );
    }

    /// This machine's name is the kernel's, where the kernel says it in the
    /// one place this reads, with nothing around it.
    #[test]
    fn this_machines_name_is_the_kernels_where_it_says_one() {
        let name = host();
        assert_eq!(
            name.is_some(),
            crate::clipboard::SYSTEM == crate::clipboard::System::Linux,
            "{name:?}"
        );
        if let Some(name) = name {
            assert_ne!(name, b"");
            assert!(!name.iter().any(u8::is_ascii_whitespace), "{name:?}");
        }
    }

    /// A socket's file is spoken to when it is this user's or the system's,
    /// and no other account's.
    #[test]
    fn a_socket_is_spoken_to_when_it_is_this_users_or_the_systems() {
        assert!(answers_for(1000, 1000));
        assert!(answers_for(0, 1000));
        assert!(answers_for(0, 0));
        assert!(!answers_for(1001, 1000));
        assert!(!answers_for(1000, 0), "root speaks to no account's socket");
        assert!(!answers_for(u32::MAX, 1000));
    }

    /// The connection's first message, byte for byte, with a cookie and with
    /// none.
    #[test]
    fn the_setup_request_is_the_protocols_bytes() {
        assert_eq!(
            setup_request(None),
            [b'l', 0, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        let mut with = vec![b'l', 0, 11, 0, 0, 0, 18, 0, 5, 0, 0, 0];
        with.extend(b"MIT-MAGIC-COOKIE-1\0\0");
        with.extend(b"abcde\0\0\0");
        assert_eq!(setup_request(Some(b"abcde")), with);
        assert_eq!(
            setup_request(Some(&[9; 16])).len(),
            12 + 20 + 16,
            "a cookie of the usual sixteen bytes needs no padding"
        );
    }

    /// Each request, byte for byte: the opcode, the length in four-byte
    /// units, and every field where the protocol puts it.
    #[test]
    fn each_request_is_the_protocols_bytes() {
        assert_eq!(get_modifier_mapping(), [119, 0, 1, 0]);
        assert_eq!(get_input_focus(), [43, 0, 1, 0]);
        assert_eq!(query_keymap(), [44, 0, 1, 0]);
        assert_eq!(ungrab_keyboard(), [32, 0, 2, 0, 0, 0, 0, 0]);
        assert_eq!(
            grab_key(0x0102_0304, 0x0548, 65),
            [33, 0, 4, 0, 4, 3, 2, 1, 0x48, 0x05, 65, 1, 1, 0, 0, 0]
        );
        assert_eq!(
            ungrab_key(0x0102_0304, 0x0548, 65),
            [34, 65, 3, 0, 4, 3, 2, 1, 0x48, 0x05, 0, 0]
        );
    }

    /// An accepted setup gives every screen's root and the range of keys,
    /// past a vendor, the formats and each screen's depths and visuals.
    #[test]
    fn an_accepted_setup_gives_every_root_and_the_keys() {
        let setup = read_setup(&mut &a_setup(&[0x1d5, 0x2a0_0001], 255)[..]).expect("accepted");
        assert_eq!(setup.roots, [0x1d5, 0x2a0_0001]);
        assert_eq!((setup.min_keycode, setup.max_keycode), (8, 255));
    }

    /// A setup that ends before its own counts do is refused, wherever it
    /// ends, and so is one with no screen.
    #[test]
    fn a_setup_cut_short_is_refused() {
        let whole = a_setup(&[1, 2], 255);
        for length in 0..whole.len() {
            let mut cut = whole[..length].to_vec();
            if length >= 8 {
                let units = u16::try_from((length - 8) / 4).expect("short");
                cut[6..8].copy_from_slice(&units.to_le_bytes());
                cut.truncate(8 + usize::from(units) * 4);
            }
            let refused = why(read_setup(&mut &cut[..]));
            assert!(
                refused.contains("ends before its own counts do")
                    || refused.contains("ended before its answer did"),
                "cut at {length}: {refused}"
            );
        }
        let refused = why(read_setup(&mut &a_setup(&[], 255)[..]));
        assert!(
            refused.contains("ends before its own counts do"),
            "{refused}"
        );
        let mut many = a_setup(&[1], 255);
        many[8 + 20] = 2;
        let refused = why(read_setup(&mut &many[..]));
        assert!(
            refused.contains("ends before its own counts do"),
            "{refused}"
        );
        let mut visuals = a_setup(&[1], 255);
        let at = visuals.len() - 48 - 6;
        visuals[at..at + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        let refused = why(read_setup(&mut &visuals[..]));
        assert!(
            refused.contains("ends before its own counts do"),
            "{refused}"
        );
    }

    /// A server that refuses says why, in at most a line with nothing a
    /// terminal acts on; one asking for more authorization is not spoken to.
    #[test]
    fn a_refused_setup_says_the_servers_reason() {
        let refused = why(read_setup(&mut &a_refusal(b"Authorization required")[..]));
        assert_eq!(
            refused,
            "the X server refused the connection: Authorization required"
        );
        let mut long = vec![b'a'; 200];
        long.extend(b"\x1b[2J");
        let refused = why(read_setup(&mut &a_refusal(&long)[..]));
        assert_eq!(
            refused,
            format!("the X server refused the connection: {}", "a".repeat(120))
        );
        let refused = why(read_setup(&mut &a_refusal(b"\x1b[2Jb\n")[..]));
        assert_eq!(refused, "the X server refused the connection: [2Jb");
        let mut more = a_refusal(b"more");
        more[0] = 2;
        let refused = why(read_setup(&mut &more[..]));
        assert!(
            refused.contains("an authorization this does not speak"),
            "{refused}"
        );
        let mut unknown = a_setup(&[1], 255);
        unknown[0] = 3;
        let refused = why(read_setup(&mut &unknown[..]));
        assert!(
            refused.contains("an authorization this does not speak"),
            "{refused}"
        );
    }

    /// A message is an error, an event or a reply, read by its first byte, a
    /// sent event as the event it is.
    #[test]
    fn a_message_is_read_as_what_its_first_byte_says() {
        assert_eq!(
            packet(&mut &an_error(BAD_ACCESS, 0x0102)[..]),
            Ok(Packet::Error {
                code: BAD_ACCESS,
                sequence: 0x0102
            })
        );
        assert_eq!(
            packet(&mut &a_key_event(KEY_PRESS, 65)[..]),
            Ok(Packet::Event {
                code: KEY_PRESS,
                detail: 65
            })
        );
        assert_eq!(
            packet(&mut &a_key_event(KEY_PRESS | 0x80, 65)[..]),
            Ok(Packet::Event {
                code: KEY_PRESS,
                detail: 65
            })
        );
        let head: Vec<u8> = (1..=24).collect();
        assert_eq!(
            packet(&mut &a_reply(0xfffe, 2, &head, &[9, 8, 7, 6])[..]),
            Ok(Packet::Reply {
                sequence: 0xfffe,
                detail: 2,
                head: head.clone().try_into().expect("twenty-four"),
                rest: vec![9, 8, 7, 6]
            })
        );
        let mut generic = a_reply(0, 0, &[], &[0; 8]);
        generic[0] = GENERIC_EVENT;
        generic.extend(a_key_event(KEY_PRESS, 65));
        let mut both = &generic[..];
        assert_eq!(
            packet(&mut both),
            Ok(Packet::Event {
                code: GENERIC_EVENT,
                detail: 0
            })
        );
        assert_eq!(
            packet(&mut both),
            Ok(Packet::Event {
                code: KEY_PRESS,
                detail: 65
            }),
            "what an event carries past its thirty-two bytes is read past, not into the next"
        );
        let refused = why(packet(&mut &[0_u8; 31][..]));
        assert!(refused.contains("ended before its answer did"), "{refused}");
    }

    /// A reply or an event that says it is longer than any this asks for is
    /// refused before a byte of that length is read.
    ///
    /// The reader handed it holds nothing past the header, and the refusal is
    /// of the length.
    #[test]
    fn a_message_longer_than_any_asked_for_is_refused_unread() {
        let longest = vec![0_u8; EXTRA_BYTES_MAX];
        assert!(matches!(
            packet(&mut &a_reply(1, 0, &[], &longest)[..]),
            Ok(Packet::Reply { rest, .. }) if rest.len() == EXTRA_BYTES_MAX
        ));
        for first in [1, GENERIC_EVENT, GENERIC_EVENT | 0x80] {
            for units in [
                u32::try_from(EXTRA_BYTES_MAX / 4 + 1).expect("short"),
                0x4000_0000,
                u32::MAX,
            ] {
                let mut head = a_reply(1, 0, &[], &[]);
                head[0] = first;
                head[4..8].copy_from_slice(&units.to_le_bytes());
                let refused = why(packet(&mut &head[..]));
                assert!(
                    refused.contains("a message longer than any this asks for"),
                    "{first} of {units} units: {refused}"
                );
            }
        }
    }

    /// The chord's masks are read from the server's map by physical key, and
    /// grabbed bare and with each lock.
    #[test]
    fn the_masks_are_read_from_the_servers_map() {
        let map = a_modifier_map();
        let masks = |held: &[Modifier]| Masks::read(Modifiers::of(held), 2, &map).expect("read");
        assert_eq!(
            masks(&[Modifier::Alt, Modifier::Super]),
            Masks {
                chord: ALT | SUPER,
                num_lock: Some(NUM_LOCK)
            }
        );
        assert_eq!(masks(&[Modifier::Alt]).chord, ALT);
        assert_eq!(masks(&[Modifier::Control, Modifier::Alt]).chord, 4 | ALT);
        assert_eq!(masks(&[Modifier::Shift, Modifier::Super]).chord, 1 | SUPER);
        assert_eq!(
            masks(&[Modifier::Alt, Modifier::Super]).variants(),
            [
                ALT | SUPER,
                ALT | SUPER | 2,
                ALT | SUPER | NUM_LOCK,
                ALT | SUPER | 2 | NUM_LOCK
            ]
        );
        let mut moved = vec![0_u8; 24];
        moved[3 * 3 + 1] = 133;
        moved[7 * 3 + 2] = 64;
        let moved =
            Masks::read(Modifiers::of(&[Modifier::Alt, Modifier::Super]), 3, &moved).expect("read");
        assert_eq!(
            moved,
            Masks {
                chord: (1 << 7) | (1 << 3),
                num_lock: None
            },
            "a map that keeps Alt in `Mod5` and Super in `Mod1` is believed over the convention"
        );
        assert_eq!(
            moved.variants(),
            [(1 << 7) | (1 << 3), (1 << 7) | (1 << 3) | 2]
        );
    }

    /// A map that cannot tell the chord's modifiers apart is refused, and a
    /// map whose length is not its header's is not read.
    ///
    /// The first is one with Alt or Super in no modifier, in one of the
    /// protocol's fixed three, or both in one.
    #[test]
    fn a_map_that_cannot_tell_the_chord_apart_is_refused() {
        let both = Modifiers::of(&[Modifier::Alt, Modifier::Super]);
        let without = |key: u8| -> Vec<u8> {
            a_modifier_map()
                .into_iter()
                .map(|held| if held == key { 0 } else { held })
                .collect()
        };
        let refused = why(Masks::read(both, 2, &without(64)));
        assert!(
            refused.contains("the left Alt key in no modifier of its own"),
            "{refused}"
        );
        let refused = why(Masks::read(both, 2, &without(133)));
        assert!(
            refused.contains("the left Super key in no modifier of its own"),
            "{refused}"
        );
        assert!(
            Masks::read(Modifiers::of(&[Modifier::Control]), 2, &without(64)).is_ok(),
            "a chord that holds neither asks the map for neither"
        );
        for fixed in 0..3 {
            let mut map = without(64);
            map[fixed * 2 + 1] = 64;
            let refused = why(Masks::read(both, 2, &map));
            assert!(
                refused.contains("the left Alt key in no modifier of its own"),
                "Alt in fixed modifier {fixed}: {refused}"
            );
        }
        let mut one = without(133);
        one[3 * 2 + 1] = 133;
        for held in [both, Modifiers::of(&[Modifier::Alt])] {
            let refused = why(Masks::read(held, 2, &one));
            assert!(
                refused.contains("the left Super key in no modifier of its own"),
                "{refused}"
            );
        }
        let mut locked = without(77);
        locked[3 * 2 + 1] = 77;
        assert_eq!(
            Masks::read(both, 2, &locked).expect("read").num_lock,
            None,
            "a Num Lock that is one of the chord's own modifiers is no variant of it"
        );
        assert_eq!(
            Masks::read(Modifiers::of(&[Modifier::Super]), 2, &locked)
                .expect("read")
                .num_lock,
            Some(ALT)
        );
        for (per, map) in [
            (2, vec![0_u8; 15]),
            (2, vec![0; 17]),
            (3, vec![0; 16]),
            (0, vec![0; 8]),
        ] {
            let refused = why(Masks::read(both, per, &map));
            assert!(
                refused.contains("a modifier map whose length is not its header's"),
                "{refused}"
            );
        }
        let refused = why(Masks::read(both, 0, &[]));
        assert!(refused.contains("in no modifier of its own"), "{refused}");
    }

    /// The server's answers to the grabs are read up to the reply asked for
    /// and no other.
    ///
    /// A `BadAccess` to any is the chord taken, whatever else was refused,
    /// and another error is a failure.
    #[test]
    fn the_grabs_answers_are_read_up_to_the_reply_asked_for() {
        let read = |messages: &[Vec<u8>]| grabs_answered(&mut &messages.concat()[..], 9);
        assert_eq!(read(&[a_reply(9, 0, &[], &[])]), Ok(()));
        assert_eq!(
            read(&[a_key_event(KEY_PRESS, 65), a_reply(9, 0, &[], &[])]),
            Ok(())
        );
        assert_eq!(
            read(&[an_error(BAD_ACCESS, 3), a_reply(9, 0, &[], &[])]),
            Err(Refused::Taken)
        );
        assert_eq!(
            read(&[
                an_error(2, 3),
                an_error(BAD_ACCESS, 4),
                an_error(8, 5),
                a_reply(9, 0, &[], &[])
            ]),
            Err(Refused::Taken)
        );
        assert_eq!(
            read(&[an_error(2, 3), an_error(8, 4), a_reply(9, 0, &[], &[])]),
            Err(Refused::Failed(
                "the X server refused the grab with error 2".to_owned()
            ))
        );
        let refused = why(read(&[a_reply(8, 0, &[], &[]), a_reply(10, 0, &[], &[])]));
        assert!(
            refused.contains("ended before its answer did"),
            "a reply to another request is not the one waited for: {refused}"
        );
        let refused = why(read(&[an_error(BAD_ACCESS, 3)]));
        assert!(refused.contains("ended before its answer did"), "{refused}");
    }

    /// A key is down by its own bit of the keymap, in the reply's head or in
    /// its rest, and up in a reply that is not a keymap's length.
    #[test]
    fn a_key_is_down_by_its_own_bit_of_the_keymap() {
        for key in [8_u8, 65, 191, 192, 255] {
            let mut keys = [0_u8; 32];
            keys[usize::from(key / 8)] = 1 << (key % 8);
            let head: [u8; 24] = keys[..24].try_into().expect("twenty-four");
            for asked in 8..=255_u8 {
                assert_eq!(
                    is_down(&head, &keys[24..], asked),
                    asked == key,
                    "{asked} of {key}"
                );
            }
            assert!(!is_down(&head, &keys[24..31], key));
            assert!(!is_down(&head, &[], key));
            assert!(!is_down(&head, &[0xff; 12], key));
        }
    }

    /// A keycode is the kernel's code plus eight.
    #[test]
    fn a_keycode_is_the_kernels_code_plus_eight() {
        assert_eq!(keycode(57), SPACE);
        assert_eq!(keycode(111), 119);
    }
}

/// The connection driven end to end, against a server written here and,
/// for the system whose sessions have one, a real one.
///
/// The server written here answers as the protocol says and keeps what it
/// was sent, on every system.
#[cfg(test)]
pub(crate) mod served {
    use std::collections::VecDeque;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    use nc_schema::keys::{Modifier, Modifiers};

    use super::Wire;
    use super::session::{Grabbed, Pace, Told, over, register_paced};
    use super::tests::{
        ALT, NUM_LOCK, SPACE, SUPER, Scratch, a_key_event, a_modifier_map, a_refusal, a_reply,
        a_setup, an_entry, an_error,
    };
    use super::{
        BAD_ACCESS, KEY_PRESS, get_input_focus, get_modifier_mapping, grab_key, query_keymap,
        setup_request, ungrab_key, ungrab_keyboard,
    };
    use crate::leader::Refused;

    /// How long a test waits for what a thread of the module's does.
    const SOON: Duration = Duration::from_secs(10);

    /// One end of a connection made of two channels, which every system has.
    struct End {
        /// What the other end sent and has not been read: the channel, and
        /// the bytes of the message being read.
        incoming: Mutex<(mpsc::Receiver<Vec<u8>>, VecDeque<u8>)>,
        /// The way to the other end, until this end closes.
        outgoing: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
        /// Whether this end was closed.
        closed: AtomicBool,
        /// Every bound this end's reads and writes were given, in order.
        bounds: Mutex<Vec<Option<Duration>>>,
    }

    /// A handle of one end of such a connection.
    pub(super) struct Pipe(Arc<End>);

    impl Pipe {
        /// A connection, as its two ends.
        fn pair() -> (Self, Self) {
            let (to_one, at_one) = mpsc::channel();
            let (to_other, at_other) = mpsc::channel();
            let end = |incoming, outgoing| {
                Self(Arc::new(End {
                    incoming: Mutex::new((incoming, VecDeque::new())),
                    outgoing: Mutex::new(Some(outgoing)),
                    closed: AtomicBool::new(false),
                    bounds: Mutex::new(Vec::new()),
                }))
            };
            (end(at_one, to_other), end(at_other, to_one))
        }
    }

    impl Read for Pipe {
        /// What the other end sent, waited for until this end is closed or
        /// the other end is gone, either of which is the connection's end.
        fn read(&mut self, into: &mut [u8]) -> std::io::Result<usize> {
            let mut incoming = self.0.incoming.lock().expect("unpoisoned");
            loop {
                if !incoming.1.is_empty() {
                    let count = into.len().min(incoming.1.len());
                    for (slot, byte) in into.iter_mut().zip(incoming.1.drain(..count)) {
                        *slot = byte;
                    }
                    return Ok(count);
                }
                if self.0.closed.load(Ordering::SeqCst) {
                    return Ok(0);
                }
                match incoming.0.recv_timeout(Duration::from_millis(2)) {
                    Ok(bytes) => incoming.1.extend(bytes),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
                }
            }
        }
    }

    impl Write for Pipe {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let gone = || std::io::Error::from(std::io::ErrorKind::BrokenPipe);
            let outgoing = self.0.outgoing.lock().expect("unpoisoned");
            let sender = outgoing.as_ref().ok_or_else(gone)?;
            sender.send(bytes.to_vec()).map_err(|_| gone())?;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Wire for Pipe {
        fn second(&self) -> std::io::Result<Self> {
            Ok(Self(Arc::clone(&self.0)))
        }

        fn within(&self, bound: Option<Duration>) -> std::io::Result<()> {
            self.0.bounds.lock().expect("unpoisoned").push(bound);
            Ok(())
        }

        fn close(&self) {
            self.0.closed.store(true, Ordering::SeqCst);
            self.0.outgoing.lock().expect("unpoisoned").take();
        }
    }

    /// How a server written here answers.
    #[derive(Clone, Copy)]
    struct Answers {
        /// What it says as the connection opens: a setup, or a refusal.
        accepts: bool,
        /// The highest keycode it has.
        highest: u8,
        /// Whether another client holds every grab asked of it.
        taken: bool,
        /// Whether it sends the key's press again before each answer that
        /// says the key is down, as a server repeating a held key does.
        repeats: bool,
    }

    /// A server that accepts, has every key, and holds nothing.
    const WILLING: Answers = Answers {
        accepts: true,
        highest: 255,
        taken: false,
        repeats: false,
    };

    /// A server written here, at the other end of a [`Pipe`].
    struct Server {
        /// The setup request it read, then every request, in order.
        heard: Arc<Mutex<Vec<Vec<u8>>>>,
        /// Its end of the connection, for the events a test has it send.
        events: Arc<Mutex<Pipe>>,
        /// How many more times it answers that the key is down.
        down_for: Arc<AtomicUsize>,
        /// How many more grabs it answers that another client holds.
        taken_for: Arc<AtomicUsize>,
        /// Its thread, which ends when the client's end closes.
        serving: Option<std::thread::JoinHandle<()>>,
    }

    impl Server {
        /// A server with two screens, answering as `answers` says, and the
        /// client's end of its connection.
        fn start(answers: Answers) -> (Self, Pipe) {
            let (client, served) = Pipe::pair();
            let heard = Arc::new(Mutex::new(Vec::new()));
            let events = Arc::new(Mutex::new(served.second().expect("a second handle")));
            let down_for = Arc::new(AtomicUsize::new(0));
            let taken_for = Arc::new(AtomicUsize::new(0));
            let (log, writer, down, taken) = (
                Arc::clone(&heard),
                Arc::clone(&events),
                Arc::clone(&down_for),
                Arc::clone(&taken_for),
            );
            let serving = std::thread::spawn(move || {
                let _ = serve(served, &log, &writer, (&down, &taken), answers);
            });
            (
                Self {
                    heard,
                    events,
                    down_for,
                    taken_for,
                    serving: Some(serving),
                },
                client,
            )
        }

        /// Ends the connection from the server's side, as a server going
        /// away does.
        fn hang_up(&self) {
            self.events.lock().expect("unpoisoned").close();
        }

        /// Sends the client `message`.
        fn send(&self, message: &[u8]) {
            self.events
                .lock()
                .expect("unpoisoned")
                .write_all(message)
                .expect("sent");
        }

        /// Everything heard so far.
        fn heard(&self) -> Vec<Vec<u8>> {
            self.heard.lock().expect("unpoisoned").clone()
        }

        /// Everything heard once it is `count` messages, or what was heard
        /// when the wait ran out.
        fn heard_once(&self, count: usize) -> Vec<Vec<u8>> {
            let deadline = Instant::now() + SOON;
            loop {
                let heard = self.heard();
                if heard.len() >= count || Instant::now() >= deadline {
                    return heard;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        /// Waits for the server's thread, which the client's close ends.
        fn ended(mut self) -> Vec<Vec<u8>> {
            self.serving
                .take()
                .expect("serving")
                .join()
                .expect("the server did not panic");
            self.heard()
        }
    }

    /// Serves one connection until its client closes it.
    fn serve(
        mut stream: Pipe,
        heard: &Mutex<Vec<Vec<u8>>>,
        writer: &Mutex<Pipe>,
        (down_for, taken_for): (&AtomicUsize, &AtomicUsize),
        answers: Answers,
    ) -> std::io::Result<()> {
        let send = |message: &[u8]| writer.lock().expect("unpoisoned").write_all(message);
        let mut setup = vec![0_u8; 12];
        stream.read_exact(&mut setup)?;
        let name = usize::from(u16::from_le_bytes([setup[6], setup[7]])).div_ceil(4) * 4;
        let data = usize::from(u16::from_le_bytes([setup[8], setup[9]])).div_ceil(4) * 4;
        let mut rest = vec![0_u8; name + data];
        stream.read_exact(&mut rest)?;
        setup.extend(rest);
        heard.lock().expect("unpoisoned").push(setup);
        if !answers.accepts {
            return send(&a_refusal(b"No protocol specified"));
        }
        send(&a_setup(&[0x1d5, 0x2a0], answers.highest))?;
        let mut sequence = 0_u16;
        loop {
            let mut request = vec![0_u8; 4];
            stream.read_exact(&mut request)?;
            let length = usize::from(u16::from_le_bytes([request[2], request[3]])) * 4;
            let mut rest = vec![0_u8; length - 4];
            stream.read_exact(&mut rest)?;
            request.extend(rest);
            sequence += 1;
            let opcode = request[0];
            heard.lock().expect("unpoisoned").push(request);
            match opcode {
                119 => send(&a_reply(sequence, 2, &[], &a_modifier_map()))?,
                33 if answers.taken => send(&an_error(BAD_ACCESS, sequence))?,
                // This thread alone counts down, as for the key below.
                33 if taken_for.load(Ordering::SeqCst) > 0 => {
                    taken_for.fetch_sub(1, Ordering::SeqCst);
                    send(&an_error(BAD_ACCESS, sequence))?;
                }
                43 => send(&a_reply(sequence, 0, &[], &[]))?,
                44 => {
                    // This thread alone counts down, and a test stores a
                    // count only while the client waits for a press.
                    let left = down_for.load(Ordering::SeqCst);
                    let down = left > 0;
                    if down {
                        down_for.store(left - 1, Ordering::SeqCst);
                    }
                    let mut keys = [0_u8; 32];
                    if down {
                        keys[usize::from(SPACE / 8)] = 1 << (SPACE % 8);
                        if answers.repeats {
                            send(&a_key_event(KEY_PRESS, SPACE))?;
                        }
                    }
                    send(&a_reply(sequence, 0, &keys[..24], &keys[24..]))?;
                }
                _ => {}
            }
        }
    }

    /// Alt, Super and Space grabbed over `client`, each press sent on the
    /// channel answered beside it.
    fn grabbed(client: Pipe) -> (Result<Grabbed<Pipe>, Refused>, mpsc::Receiver<()>) {
        let (held, presses, _lost) = grabbed_at(client, Pace::USUAL);
        (held, presses)
    }

    /// What tells a channel of each press, and another of the chord lost.
    fn told() -> (Told, mpsc::Receiver<()>, mpsc::Receiver<Refused>) {
        let (said, presses) = mpsc::channel();
        let (gone, lost) = mpsc::channel();
        let told = Told {
            on_press: Box::new(move || {
                let _ = said.send(());
            }),
            on_lost: Box::new(move |why| {
                let _ = gone.send(why);
            }),
        };
        (told, presses, lost)
    }

    /// As [`grabbed`], each wait as long as `pace` says, and with the channel
    /// the chord's loss is sent on.
    fn grabbed_at(
        client: Pipe,
        pace: Pace,
    ) -> (
        Result<Grabbed<Pipe>, Refused>,
        mpsc::Receiver<()>,
        mpsc::Receiver<Refused>,
    ) {
        let (told, presses, lost) = told();
        let held = over(
            client,
            Some(b"a cookie"),
            57,
            Modifiers::of(&[Modifier::Alt, Modifier::Super]),
            pace,
            told,
        );
        (held, presses, lost)
    }

    /// A pace that asks again at once, `tries` times.
    fn hurried(tries: u32) -> Pace {
        Pace {
            asked_every: Duration::from_millis(1),
            tried_every: Duration::from_millis(1),
            tries,
            ..Pace::USUAL
        }
    }

    /// Everything let go: the keyboard handed back, and every grab.
    fn the_letting_go() -> Vec<Vec<u8>> {
        let mut sent = vec![ungrab_keyboard().to_vec()];
        sent.extend(the_ungrabs());
        sent
    }

    /// The eight grabs of the chord: two roots, bare and with each lock.
    fn the_grabs() -> Vec<(u32, u16)> {
        [0x1d5, 0x2a0]
            .into_iter()
            .flat_map(|root| {
                [0, 2, NUM_LOCK, 2 | NUM_LOCK]
                    .into_iter()
                    .map(move |locks| (root, ALT | SUPER | locks))
            })
            .collect()
    }

    /// What a registration sends: the setup with its cookie, the map asked
    /// for, a grab of the one key on every root with each lock, and the
    /// request whose reply follows them.
    fn the_registration() -> Vec<Vec<u8>> {
        let mut sent = vec![
            setup_request(Some(b"a cookie")),
            get_modifier_mapping().to_vec(),
        ];
        sent.extend(
            the_grabs()
                .into_iter()
                .map(|(root, mask)| grab_key(root, mask, SPACE).to_vec()),
        );
        sent.push(get_input_focus().to_vec());
        sent
    }

    /// Every grab of the chord, let go.
    fn the_ungrabs() -> Vec<Vec<u8>> {
        the_grabs()
            .into_iter()
            .map(|(root, mask)| ungrab_key(root, mask, SPACE).to_vec())
            .collect()
    }

    /// A registration asks for the one chord and nothing else, hears a press
    /// of it once, and lets every grab go when it is dropped.
    ///
    /// At the press the keyboard is handed back and the grabs are let go
    /// until the key is up, and then made again; no other event is a press;
    /// and the drop closes the connection.
    #[test]
    fn a_chord_is_grabbed_pressed_once_and_let_go() {
        let (server, client) = Server::start(WILLING);
        let watched = client.second().expect("a second handle");
        let (held, presses, lost) = grabbed_at(client, Pace::USUAL);
        let held = held.expect("grabbed");
        let mut expected = the_registration();
        assert_eq!(server.heard(), expected);
        assert_eq!(
            *watched.0.bounds.lock().expect("unpoisoned"),
            [Some(super::session::WITHIN), None],
            "the registration's reads and writes are bounded, and the wait for a press is not"
        );

        server.send(&a_key_event(KEY_PRESS, SPACE - 1));
        server.send(&a_key_event(3, SPACE));
        server.send(&a_key_event(KEY_PRESS | 0x80, SPACE + 1));
        server.down_for.store(3, Ordering::SeqCst);
        server.send(&a_key_event(KEY_PRESS, SPACE));
        presses.recv_timeout(SOON).expect("the press is heard");
        expected.push(ungrab_keyboard().to_vec());
        expected.extend(the_ungrabs());
        expected.extend(std::iter::repeat_n(query_keymap().to_vec(), 4));
        expected.extend(the_registration().split_off(2));
        assert_eq!(
            server.heard_once(expected.len()),
            expected,
            "everything is let go at the press, the key is asked after until it is up, and the \
             grabs are made again"
        );
        assert!(
            presses.try_recv().is_err(),
            "one press, and none for a key beside it, its release or a sent event"
        );

        server.send(&a_key_event(KEY_PRESS, SPACE));
        presses.recv_timeout(SOON).expect("the next press is heard");
        expected.push(ungrab_keyboard().to_vec());
        expected.extend(the_ungrabs());
        expected.push(query_keymap().to_vec());
        expected.extend(the_registration().split_off(2));
        assert_eq!(server.heard_once(expected.len()), expected);

        drop(held);
        expected.extend(the_ungrabs());
        assert_eq!(
            server.ended(),
            expected,
            "the drop lets every grab go, and the server's read ended: the connection is closed"
        );
        assert!(presses.try_recv().is_err());
        assert_eq!(
            lost.try_recv(),
            Err(mpsc::TryRecvError::Disconnected),
            "a registration dropped is not one lost: what a loss is told to was dropped untold"
        );
    }

    /// A chord another client was granted while the key was down is asked
    /// for again, whatever part of it the server did grant let go first, and
    /// is this client's once more when the other has let it go.
    #[test]
    fn a_chord_taken_while_the_key_was_down_is_asked_for_again() {
        let (server, client) = Server::start(WILLING);
        let (held, presses, lost) = grabbed_at(client, hurried(3));
        let held = held.expect("grabbed");
        server.taken_for.store(5, Ordering::SeqCst);
        server.send(&a_key_event(KEY_PRESS, SPACE));
        presses.recv_timeout(SOON).expect("the press is heard");
        let mut expected = the_registration();
        expected.extend(the_letting_go());
        expected.push(query_keymap().to_vec());
        expected.extend(the_registration().split_off(2));
        expected.extend(the_letting_go());
        expected.extend(the_registration().split_off(2));
        assert_eq!(
            server.heard_once(expected.len()),
            expected,
            "five of the eight grabs refused is the chord taken: the three granted are let go, \
             and all eight are asked for again"
        );
        server.send(&a_key_event(KEY_PRESS, SPACE));
        presses
            .recv_timeout(SOON)
            .expect("the chord is this client's again, and its press is heard");
        assert_eq!(lost.try_recv(), Err(mpsc::TryRecvError::Empty));
        drop(held);
    }

    /// A chord another client keeps is asked for the times the pace says and
    /// no more, and then its loss is told, once, and the connection closed.
    #[test]
    fn a_chord_another_client_keeps_is_lost_and_its_loss_is_told() {
        let (server, client) = Server::start(WILLING);
        let (held, presses, lost) = grabbed_at(client, hurried(3));
        let held = held.expect("grabbed");
        server.taken_for.store(usize::MAX, Ordering::SeqCst);
        server.send(&a_key_event(KEY_PRESS, SPACE));
        presses.recv_timeout(SOON).expect("the press is heard");
        assert_eq!(
            lost.recv_timeout(SOON),
            Ok(Refused::Taken),
            "the caller is told the chord is another program's"
        );
        let mut expected = the_registration();
        expected.extend(the_letting_go());
        expected.push(query_keymap().to_vec());
        for tried in 1..=3 {
            expected.extend(the_registration().split_off(2));
            if tried < 3 {
                expected.extend(the_letting_go());
            }
        }
        assert_eq!(
            server.ended(),
            expected,
            "asked three times, let go between them, and then the connection is closed"
        );
        assert!(presses.try_recv().is_err());
        assert_eq!(
            lost.recv_timeout(SOON),
            Err(mpsc::RecvTimeoutError::Disconnected),
            "told once"
        );
        drop(held);
    }

    /// A server that goes away with the registration held is a loss told,
    /// with why.
    #[test]
    fn a_server_gone_with_the_chord_held_is_a_loss_told() {
        let (server, client) = Server::start(WILLING);
        let (held, _presses, lost) = grabbed_at(client, Pace::USUAL);
        let held = held.expect("grabbed");
        server.hang_up();
        let why = lost.recv_timeout(SOON).expect("the loss is told");
        assert!(
            matches!(&why, Refused::Failed(why) if why.contains("ended before its answer did")),
            "{why:?}"
        );
        drop(held);
    }

    /// The one message this client acts on is the press of the Leader's own
    /// key.
    ///
    /// Not a key beside it, not that key going up, no reply and no error.
    #[test]
    fn only_the_press_of_the_leaders_own_key_is_a_press() {
        use super::Packet;
        use super::session::pressed;

        let event = |code, detail| Packet::Event { code, detail };
        assert!(pressed(&event(KEY_PRESS, SPACE), SPACE));
        assert!(!pressed(&event(KEY_PRESS, SPACE - 1), SPACE));
        assert!(!pressed(&event(KEY_PRESS, SPACE + 1), SPACE));
        assert!(!pressed(&event(3, SPACE), SPACE), "the key going up");
        assert!(!pressed(&event(SPACE, KEY_PRESS), SPACE));
        assert!(!pressed(
            &Packet::Error {
                code: KEY_PRESS,
                sequence: u16::from(SPACE)
            },
            SPACE
        ));
        assert!(!pressed(
            &Packet::Reply {
                sequence: u16::from(KEY_PRESS),
                detail: SPACE,
                head: [0; 24],
                rest: Vec::new()
            },
            SPACE
        ));
    }

    /// A key held is one press, however often the server sends it again
    /// while it is down.
    #[test]
    fn a_key_the_server_repeats_while_held_is_one_press() {
        let (server, client) = Server::start(Answers {
            repeats: true,
            ..WILLING
        });
        let (held, presses) = grabbed(client);
        let held = held.expect("grabbed");
        server.down_for.store(5, Ordering::SeqCst);
        server.send(&a_key_event(KEY_PRESS, SPACE));
        presses.recv_timeout(SOON).expect("the press is heard");
        let count = the_registration().len() + 1 + 8 + 6 + 9;
        assert_eq!(server.heard_once(count).len(), count, "the key went up");
        assert!(
            presses.try_recv().is_err(),
            "five repeats while the key was down are no press"
        );
        server.send(&a_key_event(KEY_PRESS, SPACE));
        presses
            .recv_timeout(SOON)
            .expect("a press after the key went up is one");
        drop(held);
    }

    /// A chord another client holds is refused as taken, and the connection
    /// is closed, which lets go whatever was grabbed.
    #[test]
    fn a_chord_another_client_holds_is_refused_as_taken() {
        let (server, client) = Server::start(Answers {
            taken: true,
            ..WILLING
        });
        let (held, _presses) = grabbed(client);
        assert_eq!(held.err(), Some(Refused::Taken));
        assert_eq!(
            server.ended(),
            the_registration(),
            "nothing follows the refusal but the close"
        );
    }

    /// A server that refuses the connection, has no such key, or goes away
    /// is a registration refused, with nothing grabbed.
    #[test]
    fn a_server_that_cannot_be_asked_is_a_registration_refused() {
        let (server, client) = Server::start(Answers {
            accepts: false,
            ..WILLING
        });
        let (held, _presses) = grabbed(client);
        assert_eq!(
            held.err(),
            Some(Refused::Failed(
                "the X server refused the connection: No protocol specified".to_owned()
            ))
        );
        assert_eq!(server.ended().len(), 1, "only the setup was sent");

        let (server, client) = Server::start(Answers {
            highest: SPACE - 1,
            ..WILLING
        });
        let (held, _presses) = grabbed(client);
        assert_eq!(
            held.err(),
            Some(Refused::Failed(
                "the X server has no key numbered 65, so the chord was not grabbed".to_owned()
            ))
        );
        assert_eq!(server.ended().len(), 1, "no grab was asked of it");
        let (server, client) = Server::start(Answers {
            highest: SPACE,
            ..WILLING
        });
        assert!(grabbed(client).0.is_ok(), "the highest key is a key");
        drop(server);

        let (client, gone) = Pipe::pair();
        drop(gone);
        let (held, _presses) = grabbed(client);
        assert!(
            matches!(&held, Err(Refused::Failed(why))
                if why.contains("could not be asked") || why.contains("ended before its answer did")),
            "{held:?}"
        );
    }

    /// What a test asks of this system's sockets, where a socket is a file.
    #[cfg(unix)]
    mod sockets {
        use std::any::Any;
        use std::ffi::OsString;
        use std::path::Path;

        use crate::leader::Refused;
        use crate::x11::Display;

        /// A socket listening at `path` for as long as what is answered is
        /// kept, which takes a connection and never answers it.
        #[expect(
            clippy::unnecessary_wraps,
            reason = "one signature for both systems' halves, and the other has none to answer"
        )]
        pub(super) fn listening_at(path: &Path) -> Option<Box<dyn Any>> {
            let listener = std::os::unix::net::UnixListener::bind(path).expect("bound");
            Some(Box::new(listener))
        }

        /// The same at the abstract name `name`, where the system has such
        /// names.
        #[cfg(target_os = "linux")]
        #[expect(
            clippy::unnecessary_wraps,
            reason = "one signature for both systems' halves, and the other has none to answer"
        )]
        pub(super) fn listening_by_name(name: &str) -> Option<Box<dyn Any>> {
            use std::os::linux::net::SocketAddrExt as _;

            let address = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes())
                .expect("an abstract name");
            let listener =
                std::os::unix::net::UnixListener::bind_addr(&address).expect("bound by name");
            Some(Box::new(listener))
        }

        /// The same at the abstract name `name`, which this system has none
        /// of.
        #[cfg(not(target_os = "linux"))]
        pub(super) fn listening_by_name(_name: &str) -> Option<Box<dyn Any>> {
            None
        }

        /// Who owns the socket of `display`, and what dialling it as the
        /// user numbered one past its owner is answered.
        #[expect(
            clippy::unnecessary_wraps,
            reason = "one signature for both systems' halves, and the other has none to answer"
        )]
        pub(super) fn dialled_as_another(display: &Display) -> Option<(u32, Result<(), Refused>)> {
            use std::os::unix::fs::MetadataExt as _;

            let owner = std::fs::metadata(display.socket()).expect("a socket").uid();
            let dialled = crate::x11::dial_as(display, owner.wrapping_add(1)).map(|_| ());
            Some((owner, dialled))
        }

        /// A display's name that is not a text, and would name a socket's
        /// path if what does not read in it were let through.
        pub(super) fn not_a_text() -> OsString {
            use std::os::unix::ffi::OsStringExt as _;

            OsString::from_vec(b"/nonexistent/nc-platform/\xff:0".to_vec())
        }
    }

    /// What a test asks of this system's sockets, where a socket is no file:
    /// nothing listens, and nothing is dialled.
    #[cfg(windows)]
    mod sockets {
        use std::any::Any;
        use std::ffi::OsString;
        use std::path::Path;

        use crate::leader::Refused;
        use crate::x11::Display;

        /// Nothing: no socket here is a file.
        pub(super) fn listening_at(_path: &Path) -> Option<Box<dyn Any>> {
            None
        }

        /// Nothing: no socket here has an abstract name.
        pub(super) fn listening_by_name(_name: &str) -> Option<Box<dyn Any>> {
            None
        }

        /// Nothing: no socket here has an owner to be another's.
        pub(super) fn dialled_as_another(_display: &Display) -> Option<(u32, Result<(), Refused>)> {
            None
        }

        /// A display's name that is not a text, and would name a socket's
        /// path if what does not read in it were let through.
        pub(super) fn not_a_text() -> OsString {
            use std::os::windows::ffi::OsStringExt as _;

            let mut name: Vec<u16> = "/nonexistent/nc-platform/".encode_utf16().collect();
            name.push(0xD800);
            name.extend(":0".encode_utf16());
            OsString::from_wide(&name)
        }
    }

    /// A `DISPLAY` that is not a text, on this system.
    pub(crate) fn a_display_that_is_not_a_text() -> std::ffi::OsString {
        sockets::not_a_text()
    }

    /// A pace whose registration is bounded to a third of a second.
    fn brisk() -> Pace {
        Pace {
            within: Duration::from_millis(300),
            ..Pace::USUAL
        }
    }

    /// What registering Alt, Super and Space on the display `named` is
    /// answered at the pace `brisk`, asked on a thread of its own so that a
    /// registration that never returns fails the test and does not hold it.
    fn registered_briskly(named: std::ffi::OsString) -> Result<(), Refused> {
        let (said, answered) = mpsc::channel();
        std::thread::spawn(move || {
            let (told, _presses, _lost) = told();
            let registered = register_paced(
                57,
                Modifiers::of(&[Modifier::Alt, Modifier::Super]),
                &|name| (name == "DISPLAY").then(|| named.clone()),
                brisk(),
                told,
            );
            let _ = said.send(registered.map(|_| ()));
        });
        answered
            .recv_timeout(SOON)
            .expect("a registration is answered within its bound, whatever the server does")
    }

    /// Whether a socket is a file on this system.
    ///
    /// It decides what a display named by a path under a scratch directory
    /// is refused as: by what happens at the socket where it is one, and
    /// before any socket is thought of where it is not.
    fn a_socket_is_a_file() -> bool {
        crate::clipboard::SYSTEM != crate::clipboard::System::Windows
    }

    /// Why a registration was refused, or that it was not.
    fn failure(registered: Result<(), Refused>) -> String {
        match registered {
            Err(Refused::Failed(why)) => why,
            other => format!("not a failure: {other:?}"),
        }
    }

    /// A server that takes the connection and never answers holds a
    /// registration for its bound and no longer.
    ///
    /// Where a socket is a file one listens here and never answers; where it
    /// is not, nothing is dialled, and the registration is refused all the
    /// same.
    #[test]
    fn a_server_that_never_answers_holds_a_registration_for_its_bound_alone() {
        let scratch = Scratch::new("mute");
        let path = scratch.0.join("s");
        let listener = sockets::listening_at(&path);
        assert_eq!(listener.is_some(), a_socket_is_a_file());
        let mut named = path.into_os_string();
        named.push(":0");
        let began = Instant::now();
        let why = failure(registered_briskly(named));
        assert!(began.elapsed() < SOON, "refused within the bound");
        assert!(!why.starts_with("not a failure"), "{why}");
        if a_socket_is_a_file() {
            assert!(why.contains("ended before its answer did"), "{why}");
        }
    }

    /// A display named by its socket's path is reached at that path or not
    /// at all: never at an abstract socket of the same name.
    #[test]
    fn a_display_named_by_its_path_is_reached_by_no_abstract_name() {
        let scratch = Scratch::new("named");
        let path = scratch.0.join("s");
        let _listener = sockets::listening_by_name(&path.to_string_lossy());
        let mut named = path.into_os_string();
        named.push(":0");
        let why = failure(registered_briskly(named));
        assert!(!why.starts_with("not a failure"), "{why}");
        if a_socket_is_a_file() {
            assert!(why.contains("no X server answers at"), "{why}");
        }
    }

    /// A socket another account made is sent nothing, and is not gone round.
    ///
    /// The socket here is this user's, dialled as the user numbered one past
    /// it, which is refused unless this user is the system's.
    #[test]
    fn a_socket_another_account_made_is_sent_nothing() {
        let scratch = Scratch::new("owned");
        let path = scratch.0.join("s");
        let listener = sockets::listening_at(&path);
        let display = super::Display {
            path: Some(path.to_string_lossy().into_owned()),
            number: 0,
        };
        if let Some((owner, dialled)) = sockets::dialled_as_another(&display) {
            if owner == 0 {
                assert_eq!(
                    dialled,
                    Ok(()),
                    "the system's socket is every user's to dial"
                );
            } else {
                let why = failure(dialled);
                assert!(why.contains("belongs to another account"), "{why}");
                assert!(why.contains(&format!("user {owner}")), "{why}");
            }
        }
        assert!(
            super::dial(&display).is_ok() == listener.is_some(),
            "a socket of this user's own is dialled, where a socket is a file"
        );
    }

    /// A `DISPLAY` that is set and is not a text names no display this
    /// reads, and is never read around what does not read in it.
    #[test]
    fn a_display_that_is_not_a_text_is_refused_as_unreadable() {
        let why = failure(registered_briskly(a_display_that_is_not_a_text()));
        assert!(why.contains("names no display on this machine"), "{why}");
    }

    /// A real X server, started for one test and ended with it.
    struct Xvfb(std::process::Child);

    impl Drop for Xvfb {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// A connection read and written.
    trait Duplex: Read + Write {}

    impl<T: Read + Write> Duplex for T {}

    /// A second client of a real server, which presses keys there through
    /// the server's own test extension, and has a window of its own to see
    /// which keys a window is sent.
    ///
    /// No client of this crate's does either outside a test.
    struct Keyboard {
        /// Its connection, boxed.
        ///
        /// So this type has a value on a system whose socket has none, and
        /// what follows its making there is still read by the compiler.
        stream: Box<dyn Duplex>,
        /// The opcode the server gave `XTEST`.
        xtest: u8,
        /// The first screen's root window.
        root: u32,
        /// The first identifier the server gave this client for what it
        /// makes.
        base: u32,
    }

    impl Keyboard {
        /// A client of the server of `display` that has asked for `XTEST`.
        fn of(display: &super::Display) -> Self {
            let mut stream = super::dial(display)
                .map(|socket| -> Box<dyn Duplex> { Box::new(socket) })
                .expect("the server answers");
            stream.write_all(&setup_request(None)).expect("sent");
            let mut setup = vec![0_u8; 8];
            stream.read_exact(&mut setup).expect("answered");
            let mut body = vec![0_u8; usize::from(u16::from_le_bytes([setup[6], setup[7]])) * 4];
            stream.read_exact(&mut body).expect("answered whole");
            setup.extend(&body);
            let root = super::read_setup(&mut &setup[..]).expect("accepted").roots[0];
            let base = u32::from_le_bytes(body[4..8].try_into().expect("four"));
            let mut query = vec![98, 0, 4, 0, 5, 0, 0, 0];
            query.extend(b"XTEST\0\0\0");
            stream.write_all(&query).expect("sent");
            let mut reply = [0_u8; 32];
            stream.read_exact(&mut reply).expect("answered");
            assert_eq!((reply[0], reply[8]), (1, 1), "the server has `XTEST`");
            Self {
                stream,
                xtest: reply[9],
                root,
                base,
            }
        }

        /// Sends the key numbered `key` down, or up.
        fn key(&mut self, key: u8, down: bool) {
            let mut fake = vec![self.xtest, 2, 9, 0, if down { 2 } else { 3 }, key];
            fake.resize(36, 0);
            self.stream.write_all(&fake).expect("sent");
        }

        /// Presses `keys` in order, holds them `held`, and lets them go in
        /// the reverse order.
        fn chord(&mut self, keys: &[u8], held: Duration) {
            self.down(keys);
            std::thread::sleep(held);
            self.up(keys);
        }

        /// Presses `keys` in order, and keeps them down.
        fn down(&mut self, keys: &[u8]) {
            for key in keys {
                self.key(*key, true);
            }
        }

        /// Lets `keys` go, in the reverse order.
        fn up(&mut self, keys: &[u8]) {
            for key in keys.iter().rev() {
                self.key(*key, false);
            }
        }

        /// Makes a window that asks for the keys pressed in it, shows it and
        /// gives it the focus.
        ///
        /// Three requests: `CreateWindow` with `KeyPress` selected, `MapWindow`
        /// and `SetInputFocus`.
        fn takes_the_focus(&mut self) {
            let (window, root) = (self.base.to_le_bytes(), self.root.to_le_bytes());
            let mut requests = vec![1, 0, 9, 0];
            requests.extend(window);
            requests.extend(root);
            requests.extend([0, 0, 0, 0, 16, 0, 16, 0, 0, 0, 1, 0, 0, 0, 0, 0]);
            requests.extend([0, 8, 0, 0, 1, 0, 0, 0]);
            requests.extend([8, 0, 2, 0]);
            requests.extend(window);
            requests.extend([42, 1, 3, 0]);
            requests.extend(window);
            requests.extend([0, 0, 0, 0]);
            self.stream.write_all(&requests).expect("sent");
            assert_eq!(self.typed(), [0_u8; 0], "no key was pressed in it yet");
        }

        /// The keys whose presses the window was sent since this was last
        /// asked: everything the server sent up to its answer to a request
        /// made now, which follows whatever it made of the requests before.
        fn typed(&mut self) -> Vec<u8> {
            self.stream.write_all(&get_input_focus()).expect("sent");
            let mut keys = Vec::new();
            loop {
                let mut message = [0_u8; 32];
                self.stream.read_exact(&mut message).expect("answered");
                match message[0] & 0x7f {
                    0 => panic!("the server refused a request: error {}", message[1]),
                    1 => return keys,
                    KEY_PRESS => keys.push(message[1]),
                    _ => {}
                }
            }
        }
    }

    /// `Xvfb`, where a directory of `PATH` holds it.
    fn xvfb() -> Option<std::path::PathBuf> {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|directory| directory.join("Xvfb"))
            .find(|program| program.is_file())
    }

    /// A real X server started with `more` arguments and no socket
    /// in `/tmp`, and the name of the display it chose.
    ///
    /// **`Xvfb`, reached by its abstract socket**, which Linux has, so a
    /// server killed leaves no file.
    ///
    /// **It never stands aside**: every X server a Linux session runs is the
    /// one program this drives, so a machine without `Xvfb` fails here and is
    /// told how to run it, where a skip would count as a pass nobody saw not
    /// run.
    #[expect(
        clippy::disallowed_methods,
        reason = "the child is the X server, started with its input and its errors null and \
                  its output a pipe this reads its display from"
    )]
    fn a_real_server(more: &[&std::ffi::OsStr]) -> (Xvfb, String) {
        use std::io::BufRead as _;

        let xvfb = xvfb().expect(
            "no `Xvfb` on `PATH`, so there is no real X server to press the Leader on: install \
             it (Debian's and Ubuntu's `xvfb`), or run under `nix shell nixpkgs#xvfb --command`",
        );
        let mut server = Xvfb(
            std::process::Command::new(xvfb)
                .args(["-displayfd", "1", "-nolisten", "tcp", "-nolisten", "unix"])
                .args(more)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("Xvfb starts"),
        );
        let mut number = String::new();
        std::io::BufReader::new(server.0.stdout.take().expect("piped"))
            .read_line(&mut number)
            .expect("Xvfb names its display");
        (server, format!(":{}", number.trim()))
    }

    /// Alt, Super and Space: the development channel's default Leader.
    fn the_leader() -> nc_schema::keys::Leader {
        use nc_schema::keys::{Chord, Key, Leader};

        let Ok(leader) = Leader::of(Chord::new(
            Modifiers::of(&[Modifier::Alt, Modifier::Super]),
            Key::Space,
        )) else {
            panic!("a chord with a modifier is a Leader");
        };
        leader
    }

    /// How long a test leaves a real server and this crate's client of it to
    /// act on what was sent: well past [`super::session::ASKED_EVERY`], after
    /// which the grabs let go at a press are made again.
    const SETTLED: Duration = Duration::from_millis(300);

    /// How long a test waits for a press that must not come.
    const UNHEARD: Duration = Duration::from_millis(300);

    /// Against real X servers, through [`crate::leader::register`]: what
    /// [`a_real_server_delivers_the_chord`],
    /// [`a_real_server_grants_a_chord_held_down_and_tells_its_loss`] and
    /// [`a_real_server_is_sent_the_cookie_of_this_users_file`] hold.
    pub(crate) fn a_real_server_grants_the_chord_hears_it_and_gives_it_back() {
        a_real_server_delivers_the_chord();
        a_real_server_grants_a_chord_held_down_and_tells_its_loss();
        a_real_server_is_sent_the_cookie_of_this_users_file();
    }

    /// What [`register_on`] answers: the registration, the channel each press
    /// is sent on and the channel a loss is.
    type Registered = (
        Result<crate::leader::Held, Refused>,
        mpsc::Receiver<()>,
        mpsc::Receiver<Refused>,
    );

    /// The Leader registered on the display `named`, through
    /// [`crate::leader::register`].
    fn register_on(named: &str) -> Registered {
        let (said, presses) = mpsc::channel();
        let (gone, lost) = mpsc::channel();
        let held = crate::leader::register(
            the_leader(),
            &|name| (name == "DISPLAY").then(|| named.into()),
            move || {
                let _ = said.send(());
            },
            move |why| {
                let _ = gone.send(why);
            },
        );
        (held, presses, lost)
    }

    /// The keycodes a Linux X server gives Left Alt, Left Super, Left Shift
    /// and Caps Lock.
    const KEYS: (u8, u8, u8, u8) = (64, 133, 50, 66);

    /// How long a key is held for a tap.
    const TAP: Duration = Duration::from_millis(20);

    /// Against a real X server, the chord pressed there reaches the caller.
    ///
    /// Once, held past the server's repeat or not, and with Caps Lock on; a
    /// chord beside it does not, with the grabs in place; a key pressed with
    /// the Leader held reaches the window that has the focus; and a second
    /// registration, the key up, is refused as taken.
    fn a_real_server_delivers_the_chord() {
        let (mut server, named) = a_real_server(&[]);
        let (held, presses, _lost) = register_on(&named);
        let held = held.expect("a real server grants the chord");
        let mut keyboard = Keyboard::of(&super::Display::parse(&named).expect("a display"));
        // Another key is the one the kernel numbers 16, a US board's Q; the
        // key numbered one under Space's is Left Alt, which is no other key.
        let ((alt, meta, shift, caps), other, tap) = (KEYS, super::keycode(16), TAP);

        keyboard.chord(&[alt, meta, SPACE], tap);
        presses.recv_timeout(SOON).expect("the chord is heard");
        // The grabs were let go at the press and are made again once the key
        // is up: only then is a chord beside the Leader's one a grab could
        // answer, and each is left time to be seen up before the next.
        std::thread::sleep(SETTLED);
        for neighbour in [
            &[alt, SPACE][..],
            &[meta, SPACE],
            &[alt, meta, shift, SPACE],
            &[alt, meta, other],
            &[SPACE],
        ] {
            keyboard.chord(neighbour, tap);
            std::thread::sleep(SETTLED);
        }
        assert!(
            presses.try_recv().is_err(),
            "a chord short of a modifier, with one more, on another key, or bare is not \
             the Leader"
        );
        keyboard.chord(&[alt, meta, SPACE], tap);
        presses
            .recv_timeout(SOON)
            .expect("the grabs were in place while the chords beside it were pressed");
        assert!(
            presses.recv_timeout(UNHEARD).is_err(),
            "and it is one press"
        );

        keyboard.takes_the_focus();
        keyboard.down(&[alt, meta, SPACE]);
        presses.recv_timeout(SOON).expect("the chord is heard");
        std::thread::sleep(SETTLED);
        keyboard.chord(&[other], tap);
        let typed = keyboard.typed();
        assert!(
            typed.contains(&other),
            "a key pressed with the Leader held reaches the window that has the focus: {typed:?}"
        );
        keyboard.up(&[alt, meta, SPACE]);
        std::thread::sleep(SETTLED);

        keyboard.chord(&[alt, meta, SPACE], Duration::from_millis(1200));
        presses.recv_timeout(SOON).expect("the held chord is heard");
        assert!(
            presses.recv_timeout(UNHEARD).is_err(),
            "held past the server's repeat, it is one press"
        );

        keyboard.chord(&[caps], tap);
        keyboard.chord(&[alt, meta, SPACE], tap);
        presses
            .recv_timeout(SOON)
            .expect("the chord is heard with Caps Lock on");
        keyboard.chord(&[caps], tap);
        std::thread::sleep(SETTLED);

        assert_eq!(
            register_on(&named).0.err(),
            Some(Refused::Taken),
            "a second client asking for the chord, its key up, is refused it"
        );
        drop(held);
        let _ = server.0.kill();
    }

    /// Against a real X server, a chord whose key is down is granted to a
    /// second registration, and its loss is told.
    ///
    /// The chord is nobody's while the key is down. The first asks again
    /// once the key is up, has the chord back when the second let it go, and
    /// is told when the second kept it; and a dropped registration gives the
    /// chord back.
    fn a_real_server_grants_a_chord_held_down_and_tells_its_loss() {
        let (mut server, named) = a_real_server(&[]);
        let environment = |name: &str| (name == "DISPLAY").then(|| named.clone().into());
        let register = || register_on(&named);
        let (held, presses, lost) = register();
        let held = held.expect("a real server grants the chord");
        let mut keyboard = Keyboard::of(&super::Display::parse(&named).expect("a display"));
        let ((alt, meta, _, _), tap) = (KEYS, TAP);

        keyboard.down(&[alt, meta, SPACE]);
        presses.recv_timeout(SOON).expect("the chord is heard");
        std::thread::sleep(SETTLED);
        let (second, _, _) = register();
        let second = second.expect(
            "the chord is nobody's while its key is down, so a second client asking then is \
             granted it",
        );
        keyboard.up(&[alt, meta, SPACE]);
        std::thread::sleep(SETTLED + super::session::TRIED_EVERY);
        assert_eq!(
            lost.try_recv(),
            Err(mpsc::TryRecvError::Empty),
            "the first asks again before it calls the chord lost"
        );
        drop(second);
        std::thread::sleep(SETTLED + super::session::TRIED_EVERY);
        keyboard.chord(&[alt, meta, SPACE], tap);
        presses
            .recv_timeout(SOON)
            .expect("the second let the chord go, and it is the first's again");
        std::thread::sleep(SETTLED);
        drop(held);

        let (told, presses, lost) = told();
        let quick = register_paced(
            57,
            Modifiers::of(&[Modifier::Alt, Modifier::Super]),
            &environment,
            Pace {
                tried_every: Duration::from_millis(20),
                tries: 3,
                ..Pace::USUAL
            },
            told,
        )
        .expect("a dropped registration gave the chord back");
        keyboard.down(&[alt, meta, SPACE]);
        presses.recv_timeout(SOON).expect("the chord is heard");
        std::thread::sleep(SETTLED);
        let (keeper, kept, _) = register();
        let keeper = keeper.expect("granted while the key is down");
        keyboard.up(&[alt, meta, SPACE]);
        assert_eq!(
            lost.recv_timeout(SOON),
            Ok(Refused::Taken),
            "a chord another client was granted and kept is lost, and its loss is told"
        );
        keyboard.chord(&[alt, meta, SPACE], tap);
        kept.recv_timeout(SOON)
            .expect("the chord is heard by who holds it now");
        assert!(presses.try_recv().is_err(), "and by nobody else");
        drop(quick);
        drop(keeper);
        let _ = server.0.kill();
    }

    /// Against a real X server that admits by cookie alone, the cookie found
    /// is the one sent.
    ///
    /// A registration is refused where the environment names no file, and
    /// granted where `XAUTHORITY` names the file the server was started with.
    fn a_real_server_is_sent_the_cookie_of_this_users_file() {
        let scratch = Scratch::new("auth");
        let file = scratch.0.join("authority");
        let host = super::session::host().expect("this machine's name");
        std::fs::write(
            &file,
            an_entry(
                256,
                &host,
                b"",
                b"MIT-MAGIC-COOKIE-1",
                b"\x9e\x11\x4c\xd0\x27\x6a\xf3\x05\xb8\x72\xe9\x3d\x50\xc6\x1f\xaa",
            ),
        )
        .expect("written");
        let (mut server, named) = a_real_server(&[std::ffi::OsStr::new("-auth"), file.as_os_str()]);
        let register = |authority: Option<&std::path::Path>| {
            crate::leader::register(
                the_leader(),
                &|name| match name {
                    "DISPLAY" => Some(named.clone().into()),
                    "XAUTHORITY" => authority.map(|file| file.as_os_str().to_owned()),
                    _ => None,
                },
                || {},
                |_| {},
            )
            .map(|_| ())
        };
        let refused = failure(register(None));
        assert!(
            refused.contains("the X server refused the connection"),
            "a server that admits by cookie refuses a client that sends none: {refused}"
        );
        assert_eq!(
            register(Some(&file)),
            Ok(()),
            "the cookie of the file `XAUTHORITY` names is the one the server was started with"
        );
        let _ = server.0.kill();
    }
}
