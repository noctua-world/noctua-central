//! The macOS half of the Leader's delivery: Carbon's hotkey registration, the
//! one global hotkey macOS gives a program that is not an accessibility
//! client ([`crate::leader`], `NOCTUA.md §12.8`).
//!
//! **`RegisterEventHotKey` with a handler on the application's event
//! target.** The system sends a `kEventHotKeyPressed` event to the
//! application when the chord goes down, whichever application has the
//! focus, and nothing of any other key: no event tap is made, and no
//! permission of the person's is asked for or needed. The event is dispatched
//! from the main thread's run loop, so a registration delivers nothing until
//! the main thread runs the application's event loop, and it is made and
//! dropped on that thread.
//!
//! **Why the declarations are written here**, where the Windows half takes
//! `windows-sys`: Carbon's hotkey calls are six functions and two structures
//! of two integers each, fixed since Mac OS X 10.0, and the crates that
//! declare them bring the rest of Carbon and Core Foundation with them.
//!
//! **Every `unsafe` block says, beside it, why each pointer it hands the
//! system is valid for the call, and what is freed where.** What a handler is
//! handed back is one allocation, a [`Pressed`], which the [`HotKey`] holds
//! and lets go only when it is dropped, after the handler that reads it is
//! removed.

#![cfg(target_os = "macos")]

use std::ffi::{c_ulong, c_void};
use std::ptr::null_mut;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::leader::{Call, OnPress};

/// Carbon's status: zero, or why not.
type OsStatus = i32;

/// The class keyboard events are of (`kEventClassKeyboard`, `'keyb'`).
const EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");

/// A registered hotkey going down (`kEventHotKeyPressed`).
const EVENT_HOT_KEY_PRESSED: u32 = 5;

/// The parameter a hotkey event carries its identifier in: its direct
/// object, `kEventParamDirectObject`, `'----'`.
const PARAM_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

/// The type of that parameter (`typeEventHotKeyID`, `'hkid'`).
const TYPE_EVENT_HOT_KEY_ID: u32 = u32::from_be_bytes(*b"hkid");

/// What a handler answers for an event that is not its own, so the next
/// handler is asked (`eventNotHandledErr`).
const EVENT_NOT_HANDLED: OsStatus = -9874;

/// The signature every hotkey of this module carries: its identifiers are
/// told from another library's by it.
const SIGNATURE: u32 = u32::from_be_bytes(*b"ncLK");

/// The identifier the next hotkey takes, so two registered in one process are
/// two.
static NEXT: AtomicU32 = AtomicU32::new(1);

/// `EventTypeSpec`: an event's class and kind.
#[repr(C)]
struct EventTypeSpec {
    /// The event's class.
    class: u32,
    /// The event's kind within it.
    kind: u32,
}

/// `EventHotKeyID`: whose a hotkey is, and which of theirs.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct EventHotKeyId {
    /// Whose.
    signature: u32,
    /// Which.
    id: u32,
}

/// `EventHandlerUPP`: what Carbon calls with an event.
type Handler = extern "C" fn(call: *mut c_void, event: *mut c_void, data: *mut c_void) -> OsStatus;

// SAFETY: these are Carbon's own declarations (`CarbonEvents.h`,
// `CarbonEventsCore.h`), layout for layout: every `…Ref` is a pointer to a
// structure only Carbon reads, `ItemCount` and `ByteCount` are `unsigned
// long`, `OSStatus`, `OSType`, `OptionBits` and the event's names are 32-bit
// integers, and `EventHotKeyID` is passed by value as two of them.
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: Handler,
        count: c_ulong,
        types: *const EventTypeSpec,
        data: *mut c_void,
        installed: *mut *mut c_void,
    ) -> OsStatus;
    fn RemoveEventHandler(installed: *mut c_void) -> OsStatus;
    fn RegisterEventHotKey(
        code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: *mut c_void,
        options: u32,
        registered: *mut *mut c_void,
    ) -> OsStatus;
    fn UnregisterEventHotKey(registered: *mut c_void) -> OsStatus;
    fn GetEventParameter(
        event: *mut c_void,
        name: u32,
        kind: u32,
        actual_kind: *mut u32,
        size: c_ulong,
        actual_size: *mut c_ulong,
        data: *mut c_void,
    ) -> OsStatus;
}

/// What a handler is handed back with each event: which hotkey is its own,
/// and what a press of it is handed to.
struct Pressed {
    /// The hotkey this handler answers for.
    id: EventHotKeyId,
    /// What its press is handed to.
    on_press: OnPress,
}

/// The handler: calls `on_press` for a press of its own hotkey, and passes
/// every other event on.
extern "C" fn handler(_call: *mut c_void, event: *mut c_void, data: *mut c_void) -> OsStatus {
    // SAFETY: `data` is the `Pressed` this handler was installed with, which
    // its `HotKey` holds until it has removed the handler, and nothing writes
    // it in between.
    let pressed = unsafe { &*data.cast::<Pressed>() };
    let mut id = EventHotKeyId {
        signature: 0,
        id: 0,
    };
    // SAFETY: `event` is the event Carbon is dispatching, valid for this
    // call; `id` is a local of the size handed beside it, which is all Carbon
    // writes; and the two outputs not asked for are null, which it allows.
    let status = unsafe {
        GetEventParameter(
            event,
            PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            null_mut(),
            size_of::<EventHotKeyId>() as c_ulong,
            null_mut(),
            (&raw mut id).cast(),
        )
    };
    if status != 0 || id != pressed.id {
        return EVENT_NOT_HANDLED;
    }
    (pressed.on_press)();
    0
}

/// A hotkey registered with Carbon: unregistered, and its handler removed,
/// when this drops.
///
/// It holds raw pointers, so it is not `Send`: Carbon's event target is the
/// main thread's, where this is made and dropped.
pub(crate) struct HotKey {
    /// Carbon's registration (`EventHotKeyRef`).
    registered: *mut c_void,
    /// Carbon's handler (`EventHandlerRef`).
    installed: *mut c_void,
    /// What the handler reads, let go once this has removed the handler.
    pressed: Rc<Pressed>,
}

impl core::fmt::Debug for HotKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("HotKey")
            .field("id", &self.pressed.id.id)
            .finish_non_exhaustive()
    }
}

impl HotKey {
    /// Registers the key macOS numbers `code`, held with `modifiers` as
    /// they are written for Carbon, and hands each press to `on_press`.
    ///
    /// # Errors
    ///
    /// The call that failed and the status Carbon gave it, with nothing left
    /// installed.
    pub(crate) fn register(
        code: u32,
        modifiers: u32,
        on_press: OnPress,
    ) -> Result<Self, (Call, OsStatus)> {
        let id = EventHotKeyId {
            signature: SIGNATURE,
            id: NEXT.fetch_add(1, Ordering::Relaxed),
        };
        let pressed = Rc::new(Pressed { id, on_press });
        let kinds = [EventTypeSpec {
            class: EVENT_CLASS_KEYBOARD,
            kind: EVENT_HOT_KEY_PRESSED,
        }];
        let mut installed = null_mut();
        // SAFETY: the target is the application's own, which Carbon makes and
        // keeps; `kinds` is one `EventTypeSpec`, as the count beside it says,
        // and Carbon copies it before returning; the data is the address of
        // the `Pressed` the `Rc` holds, which does not move and is let go
        // only after the handler is removed, and which the handler only
        // reads; and `installed` is a local Carbon writes the handler's
        // reference to.
        let status = unsafe {
            InstallEventHandler(
                GetApplicationEventTarget(),
                handler,
                1,
                kinds.as_ptr(),
                Rc::as_ptr(&pressed).cast_mut().cast(),
                &raw mut installed,
            )
        };
        if status != 0 {
            return Err((Call::InstallEventHandler, status));
        }
        let mut registered = null_mut();
        // SAFETY: the target is the application's own again, `id` is passed
        // by value, and `registered` is a local Carbon writes the hotkey's
        // reference to.
        let status = unsafe {
            RegisterEventHotKey(
                code,
                modifiers,
                id,
                GetApplicationEventTarget(),
                0,
                &raw mut registered,
            )
        };
        if status != 0 {
            // SAFETY: `installed` is the handler installed above, removed
            // once, here; `pressed` is let go after it, as this returns.
            unsafe { RemoveEventHandler(installed) };
            return Err((Call::RegisterEventHotKey, status));
        }
        Ok(Self {
            registered,
            installed,
            pressed,
        })
    }

    /// Nothing: a test may not send a press on macOS, which the test that
    /// asks says in its output, by the system's name.
    #[cfg(test)]
    #[expect(
        clippy::unused_self,
        reason = "every system's registration answers this, and this one has nothing to send"
    )]
    pub(crate) const fn press(&self) -> bool {
        false
    }
}

impl Drop for HotKey {
    /// Gives the chord back and removes the handler; what the handler read
    /// is let go after it, with this.
    fn drop(&mut self) {
        // SAFETY: `registered` and `installed` are the references Carbon
        // handed `register`, each given back once, here.
        unsafe {
            UnregisterEventHotKey(self.registered);
            RemoveEventHandler(self.installed);
        }
    }
}
