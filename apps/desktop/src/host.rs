//! The window and the webview in it, and the event loop they run on
//! (`NOCTUA.md §12.2`, §4): hosted directly on `tao` and `wry`, with the one
//! call each system makes its own way in `nc_platform::webview`.
//!
//! **A window is built hidden and without decorations**, at least
//! `shell_window_min_width` × `shell_window_min_height` CSS pixels (§12.19),
//! its webview handed the document [`crate::page`] builds, so the expensive
//! step — making a webview — is done before anybody asks to see it (§12.10).
//! **It is shown and hidden by the interface's state alone**: the daemon's
//! first window, as [`crate::mirror`] holds it, says whether it is shown, and
//! a close the system asks for is `window.close` dispatched as the person,
//! which the state then says (§12.1, §12.2) — the window never decides it
//! alone.
//!
//! **The event loop is the main thread's**, as macOS requires and GTK
//! expects, and the two connections to the daemon run on two threads of
//! their own ([`crate::client`]), handing what they hear to the loop through
//! its proxy.
//!
//! **`--probe` asks whether this system can host the interface at all**
//! ([`probe`]): a window opened hidden and undecorated, its document loaded,
//! and the window closed — with no daemon, so an installer or a test can ask
//! it of any machine with a display.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! The webview's message channel carries one word, [`crate::page::LOADED`],
//! and any other message is read as nothing; the webview loads the one
//! document this build wrote, which links nothing. **What it does not
//! answer:** a process running as the same user, which can drive the
//! interface through the CLI as the person can, or inject into this one.

use core::time::Duration;
use std::time::Instant;

use nc_channel::found::Found;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::run_return::EventLoopExtRunReturn as _;
use tao::window::{Window, WindowBuilder};

use crate::client::{Asker, Flow, Heard};
use crate::mirror::{Mirror, Shell};

/// The window's title, which a window list shows though the window draws no
/// title bar.
pub const TITLE: &str = "Noctua";

/// What reaches the event loop from outside it.
#[derive(Debug)]
pub enum Arrived {
    /// The document said it has loaded.
    Loaded,
    /// What following the daemon heard.
    Heard(Heard),
}

/// Why no window was opened.
#[derive(Debug)]
pub enum Unopened {
    /// The system refused the window.
    Window(tao::error::OsError),
    /// The window was built, and its webview was not.
    Webview(nc_platform::webview::Unbuilt),
    /// A thread a connection to the daemon runs on was not started.
    Thread(std::io::Error),
}

impl core::fmt::Display for Unopened {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Window(error) => write!(formatter, "the system refused a window: {error}"),
            Self::Webview(unbuilt) => write!(formatter, "a window was built, and {unbuilt}"),
            Self::Thread(error) => write!(
                formatter,
                "a thread a connection to the daemon runs on was not started: {error}"
            ),
        }
    }
}

impl core::error::Error for Unopened {}

/// A window and the webview in it.
pub struct Opened {
    /// The window.
    pub window: Window,
    /// The webview, which lives as long as the window shows it.
    webview: wry::WebView,
}

impl core::fmt::Debug for Opened {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Opened")
            .field("window", &self.window.id())
            .field("webview", &self.webview.id())
            .finish()
    }
}

/// A window opened on `target`, hidden and undecorated, its webview handed
/// the document in `locale`; the document's word reaches the loop through
/// `proxy`.
///
/// # Errors
///
/// [`Unopened::Window`] for a window the system refused, and
/// [`Unopened::Webview`] for a webview it did not build.
pub fn open(
    target: &EventLoopWindowTarget<Arrived>,
    proxy: EventLoopProxy<Arrived>,
    locale: nc_catalog::Locale,
) -> Result<Opened, Unopened> {
    let least = tao::dpi::LogicalSize::new(
        f64::from(nc_catalog::KERNEL.shell_window_min_width),
        f64::from(nc_catalog::KERNEL.shell_window_min_height),
    );
    let window = WindowBuilder::new()
        .with_title(TITLE)
        .with_decorations(false)
        .with_visible(false)
        .with_min_inner_size(least)
        .build(target)
        .map_err(Unopened::Window)?;
    let builder = wry::WebViewBuilder::new()
        .with_html(crate::page::document(locale))
        .with_ipc_handler(move |request: wry::http::Request<String>| {
            if is_loaded(request.body()) {
                // A loop that has ended has no use for the word.
                let _ = proxy.send_event(Arrived::Loaded);
            }
        });
    let webview = nc_platform::webview::into(builder, &window).map_err(Unopened::Webview)?;
    Ok(Opened { window, webview })
}

/// Whether `message`, from the webview's channel, is the document's one
/// word: anything else is read as nothing.
fn is_loaded(message: &str) -> bool {
    message == crate::page::LOADED
}

/// What [`probe`] saw of a window opened and closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probed {
    /// The window was hidden once it was opened.
    pub hidden_when_opened: bool,
    /// The window draws no decoration of the system's.
    pub undecorated: bool,
    /// Whether the window was still hidden once the document said it had
    /// loaded; nothing when it did not say so within the wait.
    pub hidden_when_loaded: Option<bool>,
}

/// Opens a window as the interface does, waits `within` at most for its
/// document to say it loaded, and closes it: whether this system can host
/// the interface, asked with no daemon.
///
/// # Errors
///
/// [`Unopened`], as [`open`] refuses.
pub fn probe(within: Duration) -> Result<Probed, Unopened> {
    let mut event_loop = EventLoopBuilder::<Arrived>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let opened = open(&event_loop, proxy, nc_catalog::Locale::En)?;
    let mut probed = Probed {
        hidden_when_opened: !opened.window.is_visible(),
        undecorated: !opened.window.is_decorated(),
        hidden_when_loaded: None,
    };
    let deadline = Instant::now() + within;
    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::WaitUntil(deadline);
        match event {
            Event::UserEvent(Arrived::Loaded) => {
                probed.hidden_when_loaded = Some(!opened.window.is_visible());
                *flow = ControlFlow::Exit;
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => *flow = ControlFlow::Exit,
            _ => {}
        }
    });
    // **Closed**: the webview first, then the window it is drawn in.
    drop(opened);
    Ok(probed)
}

/// The window this process shows of the state `shell` holds: the first —
/// the one kept hidden and ready from the first run (§12.1).
#[must_use]
pub fn this_window(shell: &Shell) -> Option<&crate::mirror::Window> {
    shell.windows.first()
}

/// Whether the state `mirror` holds shows this process's window; nothing
/// before the daemon has answered, and the window stays as it is.
#[must_use]
pub fn shown(mirror: &Mirror) -> Option<bool> {
    mirror
        .shell()
        .and_then(this_window)
        .map(|window| window.shown)
}

/// What the event loop asks of the daemon, on the asking thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// `window.close` for this window, which the system asked to close.
    Close(u64),
}

impl Ask {
    /// The command it is.
    #[must_use]
    pub const fn command(&self) -> &'static str {
        match self {
            Self::Close(_) => "window.close",
        }
    }

    /// The arguments it is asked with.
    #[must_use]
    pub fn arguments(&self) -> nc_canon::Value {
        match self {
            Self::Close(window) => nc_canon::Value::object([(
                "window",
                nc_canon::Value::Integer(i64::try_from(*window).unwrap_or(i64::MAX)),
            )]),
        }
    }
}

/// A runtime of one thread, for one connection.
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
}

/// Starts the thread that follows the daemon of `found`, handing what it
/// hears to the loop through `proxy` until the loop has ended.
fn following(found: Found, proxy: EventLoopProxy<Arrived>) -> std::io::Result<()> {
    let runtime = runtime()?;
    std::thread::Builder::new()
        .name("noctua-following".to_owned())
        .spawn(move || {
            runtime.block_on(crate::client::follow(&found, &mut |heard| match proxy
                .send_event(Arrived::Heard(heard))
            {
                Ok(()) => Flow::Go,
                Err(_) => Flow::Stop,
            }));
        })
        .map(drop)
}

/// Starts the thread that asks the daemon of `found` what the loop sends it,
/// one request at a time, until the loop has ended.
fn asking(found: Found) -> std::io::Result<std::sync::mpsc::Sender<Ask>> {
    let runtime = runtime()?;
    let (sender, asks) = std::sync::mpsc::channel::<Ask>();
    std::thread::Builder::new()
        .name("noctua-asking".to_owned())
        .spawn(move || {
            let mut asker = Asker::new(found);
            while let Ok(ask) = asks.recv() {
                let (command, args) = (ask.command(), ask.arguments());
                // What the daemon answers reaches the window through the
                // state it then holds; a refusal is the info line's to say,
                // once the interface draws one.
                let _ = runtime.block_on(asker.dispatch(command, args));
            }
        })?;
    Ok(sender)
}

/// Runs the interface for the daemon of `found`'s install: a window opened
/// hidden, shown and hidden as the daemon's state says, until the event loop
/// ends — which nothing in this build asks it to.
///
/// # Errors
///
/// [`Unopened`], as [`open`] refuses, and for a thread not started.
pub fn run(found: Found) -> Result<(), Unopened> {
    let mut event_loop = EventLoopBuilder::<Arrived>::with_user_event().build();
    let opened = open(
        &event_loop,
        event_loop.create_proxy(),
        nc_catalog::Locale::En,
    )?;
    following(found.clone(), event_loop.create_proxy()).map_err(Unopened::Thread)?;
    let asks = asking(found).map_err(Unopened::Thread)?;
    let mut mirror = Mirror::new();
    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Arrived::Heard(Heard::Reached)) => mirror.reconnected(),
            Event::UserEvent(Arrived::Heard(Heard::Changed(changed))) => {
                // A state this build does not read leaves the one held drawn.
                if mirror.watched(changed).is_ok()
                    && let Some(shown) = shown(&mirror)
                {
                    opened.window.set_visible(shown);
                    if shown {
                        opened.window.set_focus();
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if let Some(window) = mirror.shell().and_then(this_window) {
                    // The asking thread outlives the loop.
                    let _ = asks.send(Ask::Close(window.id));
                }
            }
            _ => {}
        }
    });
    drop(opened);
    Ok(())
}

#[cfg(test)]
mod tests {
    use nc_canon::Value;
    use nc_ipc::client::Changed;

    use super::{Ask, is_loaded, shown, this_window};
    use crate::mirror::Mirror;

    /// A state as the kernel writes one, its windows shown as `shown` says.
    fn state(shown: &[bool]) -> Value {
        let windows = shown
            .iter()
            .zip(1_i64..)
            .map(|(shown, id)| {
                Value::object([
                    ("id", Value::Integer(id)),
                    ("shown", Value::Bool(*shown)),
                    ("pages", Value::Array(Vec::new())),
                ])
            })
            .collect();
        crate::mirror::tests::a_state_with_windows(windows)
    }

    /// The window this process shows is the state's first, shown as the
    /// state says, and nothing is said before the daemon has answered.
    #[test]
    fn this_window_is_the_states_first_shown_as_it_says() {
        let mut mirror = Mirror::new();
        assert_eq!(shown(&mirror), None);
        for (windows, first) in [(&[true, false][..], true), (&[false, true], false)] {
            mirror
                .watched(Changed {
                    revision: 1,
                    state: Some(state(windows)),
                })
                .expect("the state is read");
            assert_eq!(shown(&mirror), Some(first), "{windows:?}");
            let shell = mirror.shell().expect("a state is held");
            assert_eq!(this_window(shell).map(|window| window.id), Some(1));
        }
    }

    /// Closing the window is `window.close`, naming it.
    #[test]
    fn a_close_is_window_close_naming_the_window() {
        let close = Ask::Close(3);
        assert_eq!(close.command(), "window.close");
        assert_eq!(
            close.arguments(),
            Value::object([("window", Value::Integer(3))])
        );
    }

    /// The webview's channel is read for the one word, and nothing else is
    /// it.
    #[test]
    fn only_the_documents_word_is_read_from_the_webview() {
        assert!(is_loaded(crate::page::LOADED));
        for other in ["", "Loaded", "loaded ", " loaded", "ready", "loaded\n"] {
            assert!(!is_loaded(other), "{other:?}");
        }
    }
}
