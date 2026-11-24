//! The Leader's delivery: how the one chord that reaches the interface from
//! anywhere on the computer reaches this process, on the system and the
//! session it runs in (`NOCTUA.md §12.8`).
//!
//! | Session | [`Delivery`] | What [`register`] asks |
//! |---|---|---|
//! | X11 | [`Delivery::GlobalHotkey`] | the X server, for a grab of the chord on every root window, by this crate's own client of the protocol |
//! | Windows | [`Delivery::GlobalHotkey`] | `RegisterHotKey`, on a thread of this module's that reads its own message queue |
//! | macOS | [`Delivery::GlobalHotkey`] | Carbon's `RegisterEventHotKey`, with a handler on the application's event target |
//! | Wayland | [`Delivery::CompositorBinding`] | nothing: the compositor is told, in its own configuration, to run `noctua leader` on the chord |
//!
//! **A caller never names a system.** It asks [`delivery`] which of the two
//! this session has, and [`register`] where it is a hotkey; both decide here,
//! from the system this was built for and the environment the caller reads.
//!
//! **Wayland has no registration, by decision.** A Wayland client is handed
//! no key it does not have the focus for; the two ways round are the
//! `GlobalShortcuts` desktop portal and a key binding of the compositor's own.
//! The portal is spoken over D-Bus, and a client of D-Bus is a dependency this
//! tree does not take for one call (decided for Stage 16), so the binding is
//! the delivery there and the portal is not built. Xwayland's server answers
//! a grab too, and hands over only what is pressed while an X window has the
//! focus — so a session that names a Wayland display is a Wayland session
//! here, whatever else it names.
//!
//! # Presses only
//!
//! **What is reported is that the chord went down, once for each time it
//! does.** Neither Windows' registration nor the portal reports the key going
//! up, so nothing here says whether the Leader was tapped or held: that is
//! decided in the window, from the key events it is sent once it has the
//! focus (`§12.8`). A key that repeats while held is one press on every
//! system: by `MOD_NOREPEAT` on Windows, by Carbon's own rule on macOS, and on
//! X11 by the grabs being let go until the key is up.
//!
//! # Physical keys
//!
//! The chord's key is a place on the board ([`nc_schema::keys::Key`]), and
//! each system is asked by that place: X11 by the keycode its servers derive
//! from the kernel's, macOS by its virtual keycode, which names a position,
//! and Windows by the virtual key its layout has at the key's scan code when
//! the chord is registered — the one system where the registration is by a
//! layout's name for the key, so a layout switched afterwards leaves the
//! chord where the old one had it until it is registered again. One table,
//! [`physical`], holds all three, and the compiler holds it to every key of
//! the vocabulary.
//!
//! # Where the press arrives
//!
//! **On X11 and Windows, on a thread of this module's; on macOS, on the main
//! thread**, from the run loop the application's event loop turns — so on
//! macOS a registration delivers nothing until the main thread runs one, and
//! [`register`] is called, and its [`Held`] dropped, on that thread. A caller
//! written for every system therefore calls it on the thread that runs its
//! event loop, holds the [`Held`] there — it is not `Send` on any system, so
//! code that moves one does not compile anywhere — and hands `on_press`
//! something that does little and returns: a send on a channel. It is called
//! with nothing held, and a panic in it is caught and dropped, since one
//! could not cross the system's own frames.
//!
//! # What this answers (`§2.4`)
//!
//! - **What is taken is one chord, and never a bare key.** [`register`] takes
//!   a [`Leader`], which cannot be built without a modifier; one registration
//!   asks the system for that chord and reads nothing else of the keyboard,
//!   on any system.
//! - **A session that cannot be identified is refused** ([`Refused::NoSession`]):
//!   with no display named, nothing is guessed and nothing is asked.
//! - **A chord another program holds is refused, typed** ([`Refused::Taken`]),
//!   and never shared or taken over: X11's `BadAccess` to the grab, Windows'
//!   `ERROR_HOTKEY_ALREADY_REGISTERED`, Carbon's `eventHotKeyExistsErr`.
//! - **Dropping the [`Held`] gives the chord back**: `UngrabKey` and the
//!   connection closed, `UnregisterHotKey`, `UnregisterEventHotKey` with the
//!   handler removed.
//!
//! # What it does not
//!
//! - **macOS refusing a chord another application registered.** Carbon
//!   answers `eventHotKeyExistsErr` where it refuses; whether it refuses a
//!   chord another application holds, or delivers to one of the two, is the
//!   system's and was not measured. A chord macOS keeps for itself — a
//!   symbolic hotkey — is registered without complaint and never delivered;
//!   `nc_schema::keys` warns of those it knows.
//! - **A process of this user's**, which on every system can register the
//!   chord first, and on X11 and Windows send a synthetic press.
//! - **Who pressed it.** A press says the chord went down on this session's
//!   keyboard, as the system reports it, and nothing of which person or
//!   device.

use std::ffi::OsString;
use std::marker::PhantomData;

use nc_schema::keys::{Key, Leader, Modifier, Modifiers};

use crate::clipboard::System;

/// How the Leader reaches this process in a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// The system registers the chord for this process: [`register`].
    GlobalHotkey,
    /// The compositor runs `noctua leader` on the chord, bound in its own
    /// configuration; there is nothing to register.
    CompositorBinding,
}

/// Why the Leader was not registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The environment names no graphical session, so there is nothing to
    /// ask and nothing is guessed.
    NoSession,
    /// This session delivers the Leader by a compositor binding, which is
    /// [`Delivery::CompositorBinding`], and has no registration.
    ByBinding,
    /// Another program holds the chord, and keeps it.
    Taken,
    /// The system could not be asked, or answered what this does not read.
    Failed(String),
}

impl core::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoSession => formatter.write_str(
                "this process is in no graphical session it can name: neither `WAYLAND_DISPLAY` \
                 nor `DISPLAY` is set",
            ),
            Self::ByBinding => formatter.write_str(
                "a Wayland session delivers the Leader by a key binding of the compositor's that \
                 runs `noctua leader`, and has nothing to register",
            ),
            Self::Taken => formatter.write_str("another program holds the chord"),
            Self::Failed(why) => formatter.write_str(why),
        }
    }
}

impl std::error::Error for Refused {}

/// The variable that names a Wayland display.
pub const WAYLAND_DISPLAY: &str = "WAYLAND_DISPLAY";

/// The variable that names an X display.
pub const DISPLAY: &str = "DISPLAY";

/// The variable a login session's kind is written in.
pub const XDG_SESSION_TYPE: &str = "XDG_SESSION_TYPE";

/// How `system` delivers the Leader in the session named by the environment
/// that `variable` reads.
///
/// **macOS and Windows have one session and one answer.** On Linux a session
/// is Wayland's when it names a Wayland display or says it is one, and
/// otherwise X's when it names an X display — in that order, because a
/// Wayland session names an X display too, Xwayland's, whose grabs are not
/// global.
///
/// # Errors
///
/// [`Refused::NoSession`] on Linux when the environment names neither.
pub fn delivery_on(
    system: System,
    variable: &dyn Fn(&str) -> Option<OsString>,
) -> Result<Delivery, Refused> {
    match system {
        System::MacOs | System::Windows => Ok(Delivery::GlobalHotkey),
        System::Linux => {
            let named = |name: &str| variable(name).is_some_and(|value| !value.is_empty());
            let said = variable(XDG_SESSION_TYPE).is_some_and(|kind| kind == "wayland");
            if named(WAYLAND_DISPLAY) || said {
                Ok(Delivery::CompositorBinding)
            } else if named(DISPLAY) {
                Ok(Delivery::GlobalHotkey)
            } else {
                Err(Refused::NoSession)
            }
        }
    }
}

/// How the Leader is delivered in this session, read from the environment
/// that `variable` reads.
///
/// # Errors
///
/// [`Refused::NoSession`] when the session cannot be identified.
pub fn delivery(variable: &dyn Fn(&str) -> Option<OsString>) -> Result<Delivery, Refused> {
    delivery_on(crate::clipboard::SYSTEM, variable)
}

/// What each system calls a physical key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Physical {
    /// The Linux kernel's code for the key, which an X server's keycode is
    /// eight above, and which is the key's scan code on a PC keyboard.
    pub evdev: u8,
    /// The virtual keycode macOS gives the key, which names a position on the
    /// board.
    pub mac: u8,
    /// Windows' virtual key, for a key every layout has in one place; `None`
    /// for one a layout moves, which is asked for by its scan code.
    pub windows: Option<u8>,
}

/// A key a layout types a character with: Windows is asked which virtual key
/// its layout has there.
const fn typed(evdev: u8, mac: u8) -> Physical {
    Physical {
        evdev,
        mac,
        windows: None,
    }
}

/// A key every layout has in one place: Windows' virtual key is fixed.
const fn fixed(evdev: u8, mac: u8, windows: u8) -> Physical {
    Physical {
        evdev,
        mac,
        windows: Some(windows),
    }
}

/// What each system calls `key`.
///
/// **Copied from each system's own list**: the kernel's
/// `input-event-codes.h`, Carbon's `Events.h` (`kVK_…`) and Windows'
/// `winuser.h` (`VK_…`). A key joins the vocabulary as a variant of [`Key`],
/// and this does not compile until it has a row.
#[must_use]
pub const fn physical(key: Key) -> Physical {
    match key {
        Key::KeyA => typed(30, 0),
        Key::KeyB => typed(48, 11),
        Key::KeyC => typed(46, 8),
        Key::KeyD => typed(32, 2),
        Key::KeyE => typed(18, 14),
        Key::KeyF => typed(33, 3),
        Key::KeyG => typed(34, 5),
        Key::KeyH => typed(35, 4),
        Key::KeyI => typed(23, 34),
        Key::KeyJ => typed(36, 38),
        Key::KeyK => typed(37, 40),
        Key::KeyL => typed(38, 37),
        Key::KeyM => typed(50, 46),
        Key::KeyN => typed(49, 45),
        Key::KeyO => typed(24, 31),
        Key::KeyP => typed(25, 35),
        Key::KeyQ => typed(16, 12),
        Key::KeyR => typed(19, 15),
        Key::KeyS => typed(31, 1),
        Key::KeyT => typed(20, 17),
        Key::KeyU => typed(22, 32),
        Key::KeyV => typed(47, 9),
        Key::KeyW => typed(17, 13),
        Key::KeyX => typed(45, 7),
        Key::KeyY => typed(21, 16),
        Key::KeyZ => typed(44, 6),
        Key::Digit0 => typed(11, 29),
        Key::Digit1 => typed(2, 18),
        Key::Digit2 => typed(3, 19),
        Key::Digit3 => typed(4, 20),
        Key::Digit4 => typed(5, 21),
        Key::Digit5 => typed(6, 23),
        Key::Digit6 => typed(7, 22),
        Key::Digit7 => typed(8, 26),
        Key::Digit8 => typed(9, 28),
        Key::Digit9 => typed(10, 25),
        Key::Minus => typed(12, 27),
        Key::Equal => typed(13, 24),
        Key::BracketLeft => typed(26, 33),
        Key::BracketRight => typed(27, 30),
        Key::Backslash => typed(43, 42),
        Key::Semicolon => typed(39, 41),
        Key::Quote => typed(40, 39),
        Key::Backquote => typed(41, 50),
        Key::Comma => typed(51, 43),
        Key::Period => typed(52, 47),
        Key::Slash => typed(53, 44),
        Key::Space => fixed(57, 49, 0x20),
        Key::Tab => fixed(15, 48, 0x09),
        Key::Enter => fixed(28, 36, 0x0D),
        Key::Backspace => fixed(14, 51, 0x08),
        Key::ArrowUp => fixed(103, 126, 0x26),
        Key::ArrowDown => fixed(108, 125, 0x28),
        Key::ArrowLeft => fixed(105, 123, 0x25),
        Key::ArrowRight => fixed(106, 124, 0x27),
        Key::Home => fixed(102, 115, 0x24),
        Key::End => fixed(107, 119, 0x23),
        Key::PageUp => fixed(104, 116, 0x21),
        Key::PageDown => fixed(109, 121, 0x22),
        Key::Insert => fixed(110, 114, 0x2D),
        Key::Delete => fixed(111, 117, 0x2E),
        Key::F1 => fixed(59, 122, 0x70),
        Key::F2 => fixed(60, 120, 0x71),
        Key::F3 => fixed(61, 99, 0x72),
        Key::F4 => fixed(62, 118, 0x73),
        Key::F5 => fixed(63, 96, 0x74),
        Key::F6 => fixed(64, 97, 0x75),
        Key::F7 => fixed(65, 98, 0x76),
        Key::F8 => fixed(66, 100, 0x77),
        Key::F9 => fixed(67, 101, 0x78),
        Key::F10 => fixed(68, 109, 0x79),
        Key::F11 => fixed(87, 103, 0x7A),
        Key::F12 => fixed(88, 111, 0x7B),
    }
}

/// The flag that keeps Windows from sending a hotkey again while its key
/// repeats (`MOD_NOREPEAT`).
pub const WINDOWS_NO_REPEAT: u32 = 0x4000;

/// `modifiers` as `RegisterHotKey` takes them — `MOD_ALT`, `MOD_CONTROL`,
/// with `MOD_SHIFT` and `MOD_WIN` — and [`WINDOWS_NO_REPEAT`] beside them.
#[must_use]
pub fn windows_modifiers(modifiers: Modifiers) -> u32 {
    modifiers.iter().fold(WINDOWS_NO_REPEAT, |flags, modifier| {
        flags
            | match modifier {
                Modifier::Alt => 0x1,
                Modifier::Control => 0x2,
                Modifier::Shift => 0x4,
                Modifier::Super => 0x8,
            }
    })
}

/// `modifiers` as `RegisterEventHotKey` takes them: `cmdKey` for Super and
/// for Alt `optionKey`, with `shiftKey` and `controlKey`.
#[must_use]
pub fn carbon_modifiers(modifiers: Modifiers) -> u32 {
    modifiers.iter().fold(0, |flags, modifier| {
        flags
            | match modifier {
                Modifier::Super => 0x0100,
                Modifier::Shift => 0x0200,
                Modifier::Alt => 0x0800,
                Modifier::Control => 0x1000,
            }
    })
}

/// What Windows says when another program has registered the hotkey:
/// its `ERROR_HOTKEY_ALREADY_REGISTERED`.
pub const WINDOWS_ALREADY_REGISTERED: u32 = 1409;

/// What Carbon says when the hotkey is already registered, which is
/// its `eventHotKeyExistsErr`.
pub const CARBON_HOT_KEY_EXISTS: i32 = -9878;

/// A call a system's half makes for a registration, named when it fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    /// Windows' `RegisterHotKey`.
    RegisterHotKey,
    /// Windows' `MapVirtualKeyW`, asked which virtual key a scan code is.
    MapVirtualKey,
    /// The thread a Windows hotkey is registered on.
    Thread,
    /// Carbon's `InstallEventHandler`.
    InstallEventHandler,
    /// Carbon's `RegisterEventHotKey`.
    RegisterEventHotKey,
}

impl Call {
    /// The call's name, as its system writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RegisterHotKey => "RegisterHotKey",
            Self::MapVirtualKey => "MapVirtualKeyW",
            Self::Thread => "CreateThread",
            Self::InstallEventHandler => "InstallEventHandler",
            Self::RegisterEventHotKey => "RegisterEventHotKey",
        }
    }
}

/// Why Windows refused a registration, from the error `code` it gave
/// for `call`.
#[must_use]
pub fn refused_by_windows(call: Call, code: u32) -> Refused {
    if code == WINDOWS_ALREADY_REGISTERED {
        Refused::Taken
    } else {
        Refused::Failed(format!(
            "`{}` failed with Windows error {code}",
            call.name()
        ))
    }
}

/// Why Carbon refused a registration, from the `status` it gave for `call`.
#[must_use]
pub fn refused_by_carbon(call: Call, status: i32) -> Refused {
    if status == CARBON_HOT_KEY_EXISTS {
        Refused::Taken
    } else {
        Refused::Failed(format!("`{}` failed with status {status}", call.name()))
    }
}

/// What the press of the Leader is handed to.
pub(crate) type OnPress = Box<dyn Fn() + Send + 'static>;

/// `on_press`, with a panic in it caught and dropped.
///
/// **A press is delivered from the system's own frames on two systems** — a
/// handler Carbon calls, a message loop — which a panic must not unwind
/// through; and on the third the thread it would end is the one that reads
/// the next press.
pub(crate) fn guarded(on_press: impl Fn() + Send + 'static) -> OnPress {
    Box::new(move || {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&on_press));
    })
}

/// A Leader registered: the chord is this process's until this drops, which
/// gives it back.
///
/// Not `Send`, on any system: macOS takes the registration back on the thread
/// that made it.
///
/// ```compile_fail,E0277
/// fn sent<T: Send>() {}
/// sent::<nc_platform::leader::Held>();
/// ```
#[derive(Debug)]
pub struct Held {
    /// The system's registration, kept for its drop alone.
    #[cfg_attr(not(test), expect(dead_code, reason = "held for its drop alone"))]
    registration: Registration,
    /// What keeps this on the thread that made it.
    _here: PhantomData<*const ()>,
}

/// A registration, as a system holds one.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "each is held for its drop alone, and which of them a build can read differs by \
              system: one of the two has no value on each"
)]
enum Registration {
    /// An X server's grab of the chord.
    X11(crate::x11::session::Grabbed),
    /// The hotkey of a system that registers one for every program.
    Native(Native),
}

#[cfg(test)]
impl Registration {
    /// Sends the press a system delivers for the chord, where a test may
    /// send one through the registration itself, and says whether it did.
    fn press(&self) -> bool {
        match self {
            Self::X11(_) => false,
            Self::Native(native) => native.press(),
        }
    }
}

/// The hotkey Windows registers.
#[cfg(windows)]
type Native = crate::windows::HotKey;

/// The hotkey Carbon registers.
#[cfg(target_os = "macos")]
type Native = crate::macos::HotKey;

/// The hotkey of a system that registers one for every program.
///
/// On a system whose sessions are X's or Wayland's there is none, and so no
/// value of this type.
#[cfg(not(any(windows, target_os = "macos")))]
#[derive(Debug)]
enum Native {}

#[cfg(all(test, not(any(windows, target_os = "macos"))))]
impl Native {
    /// Nothing: there is no value to send a press through.
    fn press(&self) -> bool {
        match *self {}
    }
}

/// Asks this system for `leader`, in the session the environment `variable`
/// reads names, and calls `on_press` each time the chord goes down.
///
/// **Presses only**, where and as the module header says: on a thread of this
/// module's on X11 and Windows, on the main thread's run loop on macOS —
/// where this is called on the main thread — and never the key going up.
///
/// # Errors
///
/// [`Refused::NoSession`] when the session cannot be identified;
/// [`Refused::ByBinding`] in a Wayland session, which has no registration;
/// [`Refused::Taken`] when another program holds the chord; and
/// [`Refused::Failed`] when the system could not be asked.
pub fn register(
    leader: Leader,
    variable: &dyn Fn(&str) -> Option<OsString>,
    on_press: impl Fn() + Send + 'static,
) -> Result<Held, Refused> {
    registered_on(
        crate::clipboard::SYSTEM,
        leader,
        variable,
        guarded(on_press),
    )
}

/// The registration [`register`] asks for, on `system`: the one this was built
/// for, outside a test.
///
/// **The system is an argument**, so what a Linux session is refused is
/// decided, and tested, in code every system runs: only a registration that
/// reaches the system's own call needs that system.
fn registered_on(
    system: System,
    leader: Leader,
    variable: &dyn Fn(&str) -> Option<OsString>,
    on_press: OnPress,
) -> Result<Held, Refused> {
    let registration = match delivery_on(system, variable)? {
        Delivery::CompositorBinding => return Err(Refused::ByBinding),
        Delivery::GlobalHotkey => match system {
            System::Linux => Registration::X11(crate::x11::session::register(
                physical(leader.key()).evdev,
                leader.chord().modifiers(),
                variable,
                on_press,
            )?),
            System::MacOs | System::Windows => Registration::Native(native(leader, on_press)?),
        },
    };
    Ok(Held {
        registration,
        _here: PhantomData,
    })
}

/// Carbon's registration of `leader`.
#[cfg(target_os = "macos")]
fn native(leader: Leader, on_press: OnPress) -> Result<Native, Refused> {
    crate::macos::HotKey::register(
        u32::from(physical(leader.key()).mac),
        carbon_modifiers(leader.chord().modifiers()),
        on_press,
    )
    .map_err(|(call, status)| refused_by_carbon(call, status))
}

/// Windows' registration of `leader`.
#[cfg(windows)]
fn native(leader: Leader, on_press: OnPress) -> Result<Native, Refused> {
    let key = physical(leader.key());
    crate::windows::HotKey::register(
        key.windows,
        key.evdev,
        windows_modifiers(leader.chord().modifiers()),
        on_press,
    )
    .map_err(|(call, code)| refused_by_windows(call, code))
}

/// The registration of a system that has none of its own: refused, since
/// its sessions are X's or Wayland's and [`delivery`] has answered for both.
#[cfg(not(any(windows, target_os = "macos")))]
fn native(_leader: Leader, _on_press: OnPress) -> Result<Native, Refused> {
    Err(Refused::NoSession)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::ffi::OsString;

    use nc_schema::keys::{Chord, Key, Leader, Modifier, Modifiers};

    use super::{
        Call, Delivery, Refused, carbon_modifiers, delivery_on, guarded, physical,
        refused_by_carbon, refused_by_windows, windows_modifiers,
    };
    use crate::clipboard::System;

    /// The kernel's code for each key of the vocabulary, in the order
    /// of `input-event-codes.h`: `KEY_1` is 2 and `KEY_DELETE` is 111.
    const BY_THE_KERNELS_CODE: [(u8, &str); 73] = [
        (2, "Digit1"),
        (3, "Digit2"),
        (4, "Digit3"),
        (5, "Digit4"),
        (6, "Digit5"),
        (7, "Digit6"),
        (8, "Digit7"),
        (9, "Digit8"),
        (10, "Digit9"),
        (11, "Digit0"),
        (12, "Minus"),
        (13, "Equal"),
        (14, "Backspace"),
        (15, "Tab"),
        (16, "KeyQ"),
        (17, "KeyW"),
        (18, "KeyE"),
        (19, "KeyR"),
        (20, "KeyT"),
        (21, "KeyY"),
        (22, "KeyU"),
        (23, "KeyI"),
        (24, "KeyO"),
        (25, "KeyP"),
        (26, "BracketLeft"),
        (27, "BracketRight"),
        (28, "Enter"),
        (30, "KeyA"),
        (31, "KeyS"),
        (32, "KeyD"),
        (33, "KeyF"),
        (34, "KeyG"),
        (35, "KeyH"),
        (36, "KeyJ"),
        (37, "KeyK"),
        (38, "KeyL"),
        (39, "Semicolon"),
        (40, "Quote"),
        (41, "Backquote"),
        (43, "Backslash"),
        (44, "KeyZ"),
        (45, "KeyX"),
        (46, "KeyC"),
        (47, "KeyV"),
        (48, "KeyB"),
        (49, "KeyN"),
        (50, "KeyM"),
        (51, "Comma"),
        (52, "Period"),
        (53, "Slash"),
        (57, "Space"),
        (59, "F1"),
        (60, "F2"),
        (61, "F3"),
        (62, "F4"),
        (63, "F5"),
        (64, "F6"),
        (65, "F7"),
        (66, "F8"),
        (67, "F9"),
        (68, "F10"),
        (87, "F11"),
        (88, "F12"),
        (102, "Home"),
        (103, "ArrowUp"),
        (104, "PageUp"),
        (105, "ArrowLeft"),
        (106, "ArrowRight"),
        (107, "End"),
        (108, "ArrowDown"),
        (109, "PageDown"),
        (110, "Insert"),
        (111, "Delete"),
    ];

    /// The virtual keycode macOS gives each key of the vocabulary, in the
    /// order of Carbon's `Events.h`.
    ///
    /// From `kVK_ANSI_A`, zero, to the up arrow's `kVK_UpArrow`, with a PC
    /// keyboard's Insert as the key a Mac calls Help.
    const BY_THE_MACS_CODE: [(u8, &str); 73] = [
        (0, "KeyA"),
        (1, "KeyS"),
        (2, "KeyD"),
        (3, "KeyF"),
        (4, "KeyH"),
        (5, "KeyG"),
        (6, "KeyZ"),
        (7, "KeyX"),
        (8, "KeyC"),
        (9, "KeyV"),
        (11, "KeyB"),
        (12, "KeyQ"),
        (13, "KeyW"),
        (14, "KeyE"),
        (15, "KeyR"),
        (16, "KeyY"),
        (17, "KeyT"),
        (18, "Digit1"),
        (19, "Digit2"),
        (20, "Digit3"),
        (21, "Digit4"),
        (22, "Digit6"),
        (23, "Digit5"),
        (24, "Equal"),
        (25, "Digit9"),
        (26, "Digit7"),
        (27, "Minus"),
        (28, "Digit8"),
        (29, "Digit0"),
        (30, "BracketRight"),
        (31, "KeyO"),
        (32, "KeyU"),
        (33, "BracketLeft"),
        (34, "KeyI"),
        (35, "KeyP"),
        (36, "Enter"),
        (37, "KeyL"),
        (38, "KeyJ"),
        (39, "Quote"),
        (40, "KeyK"),
        (41, "Semicolon"),
        (42, "Backslash"),
        (43, "Comma"),
        (44, "Slash"),
        (45, "KeyN"),
        (46, "KeyM"),
        (47, "Period"),
        (48, "Tab"),
        (49, "Space"),
        (50, "Backquote"),
        (51, "Backspace"),
        (96, "F5"),
        (97, "F6"),
        (98, "F7"),
        (99, "F3"),
        (100, "F8"),
        (101, "F9"),
        (103, "F11"),
        (109, "F10"),
        (111, "F12"),
        (114, "Insert"),
        (115, "Home"),
        (116, "PageUp"),
        (117, "Delete"),
        (118, "F4"),
        (119, "End"),
        (120, "F2"),
        (121, "PageDown"),
        (122, "F1"),
        (123, "ArrowLeft"),
        (124, "ArrowRight"),
        (125, "ArrowDown"),
        (126, "ArrowUp"),
    ];

    /// Windows' virtual key for each key no layout moves, in the order
    /// of `winuser.h`: `VK_BACK` is 0x08 and `VK_F12` is 0x7B.
    const BY_WINDOWS_VIRTUAL_KEY: [(u8, &str); 26] = [
        (0x08, "Backspace"),
        (0x09, "Tab"),
        (0x0D, "Enter"),
        (0x20, "Space"),
        (0x21, "PageUp"),
        (0x22, "PageDown"),
        (0x23, "End"),
        (0x24, "Home"),
        (0x25, "ArrowLeft"),
        (0x26, "ArrowUp"),
        (0x27, "ArrowRight"),
        (0x28, "ArrowDown"),
        (0x2D, "Insert"),
        (0x2E, "Delete"),
        (0x70, "F1"),
        (0x71, "F2"),
        (0x72, "F3"),
        (0x73, "F4"),
        (0x74, "F5"),
        (0x75, "F6"),
        (0x76, "F7"),
        (0x77, "F8"),
        (0x78, "F9"),
        (0x79, "F10"),
        (0x7A, "F11"),
        (0x7B, "F12"),
    ];

    /// The key the grammar names `code`.
    fn key(code: &str) -> Key {
        Key::of_code(code).unwrap_or_else(|| panic!("`{code}` is a key of the vocabulary"))
    }

    /// An environment holding `variables` and nothing else.
    fn environment(variables: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let variables: Vec<(String, OsString)> = variables
            .iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
            .collect();
        move |name: &str| {
            variables
                .iter()
                .find(|(held, _)| held == name)
                .map(|(_, value)| value.clone())
        }
    }

    /// Every key of the vocabulary has the code each system's own list gives
    /// it, written out here in each list's order, and no two keys share one.
    #[test]
    fn every_key_has_the_code_each_systems_list_gives_it() {
        for (list, of) in [
            (
                &BY_THE_KERNELS_CODE,
                (|key| physical(key).evdev) as fn(Key) -> u8,
            ),
            (&BY_THE_MACS_CODE, |key| physical(key).mac),
        ] {
            let named: BTreeSet<Key> = list.iter().map(|(_, code)| key(code)).collect();
            assert_eq!(
                named,
                Key::ALL.into_iter().collect(),
                "a list names every key of the vocabulary, once"
            );
            let codes: BTreeSet<u8> = list.iter().map(|(code, _)| *code).collect();
            assert_eq!(codes.len(), Key::ALL.len(), "no two keys share a code");
            assert!(list.is_sorted(), "a list is written in its system's order");
            for (code, name) in list {
                assert_eq!(of(key(name)), *code, "{name}");
            }
        }
        assert!(BY_WINDOWS_VIRTUAL_KEY.is_sorted());
        let fixed: BTreeSet<Key> = BY_WINDOWS_VIRTUAL_KEY
            .iter()
            .map(|(_, code)| key(code))
            .collect();
        assert_eq!(fixed.len(), BY_WINDOWS_VIRTUAL_KEY.len());
        for (code, name) in BY_WINDOWS_VIRTUAL_KEY {
            assert_eq!(physical(key(name)).windows, Some(code), "{name}");
        }
        for key in Key::ALL {
            let found = physical(key);
            assert_eq!(
                found.windows.is_none(),
                !fixed.contains(&key),
                "{}: a key a layout types with is asked for by its scan code, and no other",
                key.code()
            );
            assert!(
                found.windows.is_some() || found.evdev < 0x54,
                "{}: a key asked for by its scan code has one of a single byte's main block",
                key.code()
            );
        }
    }

    /// Each system's flags for the modifiers, one by one and together, with
    /// the flag that keeps a held chord to one message always among Windows'.
    #[test]
    fn the_modifiers_are_each_systems_flags() {
        for (modifier, windows, carbon) in [
            (Modifier::Alt, 0x1, 0x0800),
            (Modifier::Control, 0x2, 0x1000),
            (Modifier::Shift, 0x4, 0x0200),
            (Modifier::Super, 0x8, 0x0100),
        ] {
            let held = Modifiers::of(&[modifier]);
            assert_eq!(windows_modifiers(held), 0x4000 | windows, "{modifier:?}");
            assert_eq!(carbon_modifiers(held), carbon, "{modifier:?}");
        }
        let development = Modifiers::of(&[Modifier::Alt, Modifier::Super]);
        assert_eq!(windows_modifiers(development), 0x4009);
        assert_eq!(carbon_modifiers(development), 0x0900);
        assert_eq!(windows_modifiers(Modifiers::of(&Modifier::ALL)), 0x400F);
        assert_eq!(carbon_modifiers(Modifiers::of(&Modifier::ALL)), 0x1B00);
        assert_eq!(windows_modifiers(Modifiers::NONE), 0x4000);
        assert_eq!(carbon_modifiers(Modifiers::NONE), 0);
    }

    /// A chord another program holds is refused as taken, by the one error
    /// each system says it with.
    ///
    /// Any other error is a failure naming the call.
    #[test]
    fn a_chord_another_program_holds_is_taken_on_each_system() {
        assert_eq!(
            refused_by_windows(Call::RegisterHotKey, 1409),
            Refused::Taken
        );
        assert_eq!(
            refused_by_windows(Call::RegisterHotKey, 1408),
            Refused::Failed("`RegisterHotKey` failed with Windows error 1408".to_owned())
        );
        assert_eq!(
            refused_by_carbon(Call::RegisterEventHotKey, -9878),
            Refused::Taken
        );
        assert_eq!(
            refused_by_carbon(Call::RegisterEventHotKey, -50),
            Refused::Failed("`RegisterEventHotKey` failed with status -50".to_owned())
        );
        assert_eq!(
            refused_by_carbon(Call::InstallEventHandler, 9878),
            Refused::Failed("`InstallEventHandler` failed with status 9878".to_owned())
        );
    }

    /// A Mac and Windows register a hotkey whatever the environment says;
    /// a Linux session is Wayland's when it names a Wayland display or says
    /// it is one, X's when it names only an X display, and none otherwise.
    #[test]
    fn the_delivery_is_read_from_the_system_and_the_session() {
        let wayland = environment(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("DISPLAY", ":0"),
            ("XDG_SESSION_TYPE", "wayland"),
        ]);
        let nothing = environment(&[]);
        for system in [System::MacOs, System::Windows] {
            assert_eq!(delivery_on(system, &wayland), Ok(Delivery::GlobalHotkey));
            assert_eq!(delivery_on(system, &nothing), Ok(Delivery::GlobalHotkey));
        }
        for (variables, delivery) in [
            (
                &[("WAYLAND_DISPLAY", "wayland-1"), ("DISPLAY", ":0")][..],
                Ok(Delivery::CompositorBinding),
            ),
            (
                &[("WAYLAND_DISPLAY", "wayland-1")],
                Ok(Delivery::CompositorBinding),
            ),
            (
                &[("XDG_SESSION_TYPE", "wayland"), ("DISPLAY", ":0")],
                Ok(Delivery::CompositorBinding),
            ),
            (
                &[("XDG_SESSION_TYPE", "wayland")],
                Ok(Delivery::CompositorBinding),
            ),
            (&[("DISPLAY", ":0")], Ok(Delivery::GlobalHotkey)),
            (
                &[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "x11")],
                Ok(Delivery::GlobalHotkey),
            ),
            (
                &[("DISPLAY", ":0"), ("WAYLAND_DISPLAY", "")],
                Ok(Delivery::GlobalHotkey),
            ),
            (
                &[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "tty")],
                Ok(Delivery::GlobalHotkey),
            ),
            (&[], Err(Refused::NoSession)),
            (&[("DISPLAY", "")], Err(Refused::NoSession)),
            (&[("WAYLAND_DISPLAY", "")], Err(Refused::NoSession)),
            (&[("XDG_SESSION_TYPE", "x11")], Err(Refused::NoSession)),
            (&[("XDG_SESSION_TYPE", "Wayland")], Err(Refused::NoSession)),
            (
                &[("HOME", "/home/a"), ("TERM", "xterm")],
                Err(Refused::NoSession),
            ),
        ] {
            assert_eq!(
                delivery_on(System::Linux, &environment(variables)),
                delivery,
                "{variables:?}"
            );
        }
    }

    /// Each refusal says what it is, in a sentence of its own.
    #[test]
    fn each_refusal_says_what_it_is() {
        assert!(
            Refused::NoSession
                .to_string()
                .contains("no graphical session")
        );
        assert!(
            Refused::ByBinding
                .to_string()
                .contains("runs `noctua leader`")
        );
        assert_eq!(
            Refused::Taken.to_string(),
            "another program holds the chord"
        );
        assert_eq!(Refused::Failed("why".to_owned()).to_string(), "why");
    }

    /// What a press is handed to is called, and a panic in it goes no
    /// further.
    #[test]
    fn a_panic_in_what_a_press_is_handed_to_goes_no_further() {
        let (said, heard) = std::sync::mpsc::channel();
        let on_press = guarded(move || {
            said.send(()).expect("heard");
            std::panic::resume_unwind(Box::new("a panic with no message printed"));
        });
        on_press();
        on_press();
        assert_eq!(
            heard.try_iter().count(),
            2,
            "called each time, the panic caught"
        );
    }

    /// A Linux session's registration is refused where the session cannot be
    /// named, where it is Wayland's, and where no X server answers.
    ///
    /// Each as itself, with nothing asked of a network, and on whichever
    /// system this runs, since what a Linux session is refused is decided
    /// before any system is asked.
    #[test]
    fn a_registration_is_refused_where_the_session_delivers_none() {
        let register = |variables: &[(&str, &str)]| {
            super::registered_on(
                System::Linux,
                the_development_leader(),
                &environment(variables),
                Box::new(|| {}),
            )
            .map(|_| ())
        };
        assert_eq!(register(&[]), Err(Refused::NoSession));
        assert_eq!(
            register(&[("WAYLAND_DISPLAY", "wayland-1"), ("DISPLAY", ":0")]),
            Err(Refused::ByBinding)
        );
        assert_eq!(
            register(&[("XDG_SESSION_TYPE", "wayland"), ("DISPLAY", ":0")]),
            Err(Refused::ByBinding)
        );
        for host in ["example.invalid:0", "tcp/localhost:0", "localhost:0"] {
            assert!(
                matches!(register(&[("DISPLAY", host)]), Err(Refused::Failed(why))
                    if why.contains("names no display on this machine")),
                "{host}"
            );
        }
        assert!(
            matches!(
                register(&[("DISPLAY", "/nonexistent/nc-platform/no-server:0")]),
                Err(Refused::Failed(why)) if why.contains("no X server answers at")
            ),
            "a socket nothing listens at is no server"
        );
    }

    /// Alt, Super and Space: the development channel's default Leader.
    fn the_development_leader() -> Leader {
        let Ok(leader) = Leader::of(Chord::new(
            Modifiers::of(&[Modifier::Alt, Modifier::Super]),
            Key::Space,
        )) else {
            panic!("a chord with a modifier is a Leader");
        };
        leader
    }

    /// This system grants a chord, delivers its press where a test may send
    /// one, and takes the chord back when the registration is dropped.
    ///
    /// **Every system runs it, each against what it has.** Linux starts a
    /// real X server and presses the chord on it. Windows registers every
    /// modifier with a function key — the first of four no program here
    /// holds — posts the message Windows sends for a hotkey, and holds a
    /// second registration to the refusal. macOS registers the same chord
    /// through Carbon, twice over, and sends no press, since a test may not
    /// send one there.
    #[test]
    fn this_system_grants_the_chord_hears_it_and_gives_it_back() {
        if crate::clipboard::SYSTEM == System::Linux {
            crate::x11::served::a_real_server_grants_the_chord_hears_it_and_gives_it_back();
            return;
        }
        let register = |key: Key| {
            let Ok(leader) = Leader::of(Chord::new(Modifiers::of(&Modifier::ALL), key)) else {
                panic!("a chord with a modifier is a Leader");
            };
            let (said, heard) = std::sync::mpsc::channel();
            let held = super::register(leader, &environment(&[]), move || {
                let _ = said.send(());
            });
            (held, heard)
        };
        let free = [Key::F11, Key::F10, Key::F9, Key::F8]
            .into_iter()
            .find(|key| match register(*key).0 {
                Ok(_) => true,
                Err(Refused::Taken) => false,
                Err(other) => panic!("the registration failed: {other}"),
            })
            .expect("one of four chords nothing ships bound is free to register");
        for _ in 0..2 {
            let (held, heard) = register(free);
            let held = held.expect("a chord given back is granted again");
            if held.registration.press() {
                heard
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .expect("the hotkey's message reaches the caller");
                assert_eq!(
                    register(free).0.err(),
                    Some(Refused::Taken),
                    "a chord held is refused to the next to ask, this process included"
                );
            }
            drop(held);
        }
    }
}
