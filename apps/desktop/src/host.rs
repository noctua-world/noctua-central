//! The window and the webview in it, and the event loop they run on
//! (`NOCTUA.md §12.2`, §4): hosted directly on `tao` and `wry`, with the one
//! call each system makes its own way in `nc_platform::webview`.
//!
//! **A window is built hidden and without decorations**, never smaller than
//! `shell_window_min_width` × `shell_window_min_height` CSS pixels (§12.19) —
//! the least the system's own gestures may make it, and the least this
//! process ever asks for ([`at_least`]) — its webview handed the document
//! [`crate::page`] builds, so the expensive step — making a webview — is done
//! before anybody asks to see it (§12.10). **Its theme is decided before the
//! webview exists** ([`crate::theme`]): the system's mode resolved, the
//! window told it, the webview's own background the page's where the design
//! system gives it, and the document stamped with it.
//!
//! **Everything the webview loads is the custom protocol's**
//! ([`crate::assets`]): the document is loaded from its root, a navigation
//! to any address outside it is refused, a new window is never opened, and
//! a download never starts.
//!
//! **It is shown and hidden by the interface's state alone, and shown only
//! once its document has loaded** ([`Showing`]): the daemon's first window,
//! as [`crate::mirror`] holds it, says whether it is shown — `leader.tap`
//! shows it, `window.hide` hides it — and a show it asks for before the
//! document and every sheet it links have loaded waits for them, so the
//! first frame a person sees is the page in its theme (§9.6). A close the
//! system asks for is `window.close` dispatched as the person, which hides
//! the last window and which the state then says (§12.1, §12.2) — the window
//! never decides it alone.
//!
//! **The event loop is the main thread's**, as macOS requires and GTK
//! expects, and the two connections to the daemon run on two threads of
//! their own ([`crate::client`]), handing what they hear to the loop through
//! its proxy.
//!
//! **`--probe` asks whether this system can host the interface at all**
//! ([`probe`]): with no daemon, a window is opened as the interface opens
//! one, in the light and then in the dark, asked to show at once, and
//! watched — whether it stayed hidden until its document loaded, what its
//! first frame and a settled frame painted, and how large its page is when
//! it is asked to be smaller than the least — and closed, so an installer
//! or a test can ask it of any machine with a display.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! The webview's message channel carries a closed set of messages
//! ([`said`]) and anything else is read as nothing; the webview loads the
//! one document this build wrote, from bytes this build embeds, and is let
//! to navigate nowhere else. **What it does not answer:** a process running
//! as the same user, which can drive the interface through the CLI as the
//! person can, or inject into this one.

use core::time::Duration;
use std::time::Instant;

use nc_channel::found::Found;
use noctua_design::model::Mode;
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::run_return::EventLoopExtRunReturn as _;
use tao::window::{Window, WindowBuilder};

use crate::client::{Asker, Flow, Heard};
use crate::mirror::{Mirror, Shell};
use crate::theme::Theme;

/// The window's title, which a window list shows though the window draws no
/// title bar.
pub const TITLE: &str = "Noctua";

/// What the document says through the webview's message channel: a closed
/// set, each written one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// `loaded`: the document and every sheet it links have loaded.
    Loaded,
    /// `scale <thousandths>`: how many device pixels one CSS pixel of the
    /// page is, in thousandths.
    Scale(u32),
    /// `first-frame <r> <g> <b> <a>`: what the page's background was at its
    /// first frame, read back as a pixel — said by a probe's observer alone.
    FirstFrame([u8; 4]),
    /// `settled <r> <g> <b> <a>`: the same, once everything has loaded and
    /// two more frames have been drawn — said by a probe's observer alone.
    Settled([u8; 4]),
    /// `viewport <width> <height>`: the page's size in CSS pixels once it
    /// changed — said by a probe's observer alone.
    Viewport(u32, u32),
}

/// A number written in ASCII digits alone, of at most ten.
fn number<T: core::str::FromStr>(word: &str) -> Option<T> {
    (!word.is_empty() && word.len() <= 10 && word.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| word.parse().ok())
        .flatten()
}

/// Four channels written as numbers each of one byte.
fn channels(words: &[&str]) -> Option<[u8; 4]> {
    match words {
        [red, green, blue, alpha] => {
            Some([number(red)?, number(green)?, number(blue)?, number(alpha)?])
        }
        _ => None,
    }
}

/// What `message`, from the webview's channel, says; nothing for a message
/// of any other shape — the words are split at single spaces, and nothing
/// else separates or surrounds them.
#[must_use]
pub fn said(message: &str) -> Option<Said> {
    let words: Vec<&str> = message.split(' ').collect();
    match words.as_slice() {
        [loaded] if *loaded == crate::page::LOADED => Some(Said::Loaded),
        [scale, thousandths] if *scale == crate::page::SCALE => {
            number(thousandths).map(Said::Scale)
        }
        ["first-frame", rest @ ..] => channels(rest).map(Said::FirstFrame),
        ["settled", rest @ ..] => channels(rest).map(Said::Settled),
        ["viewport", width, height] => Some(Said::Viewport(number(width)?, number(height)?)),
        _ => None,
    }
}

/// What reaches the event loop from outside it.
#[derive(Debug)]
pub enum Arrived {
    /// The document of the window opened `window`-th said something.
    Said {
        /// Which window, counting from 0 in the order they were opened.
        window: usize,
        /// What it said.
        said: Said,
    },
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

/// The least a window is, in CSS pixels: the catalog's
/// `shell_window_min_width` × `shell_window_min_height` (§12.19).
#[must_use]
pub fn least() -> LogicalSize<f64> {
    LogicalSize::new(
        f64::from(nc_catalog::KERNEL.shell_window_min_width),
        f64::from(nc_catalog::KERNEL.shell_window_min_height),
    )
}

/// `asked`, made at least `least` in each dimension: the size this process
/// asks a window for when it is asked for `asked`.
#[must_use]
pub fn at_least(asked: LogicalSize<f64>, least: LogicalSize<f64>) -> LogicalSize<f64> {
    LogicalSize::new(asked.width.max(least.width), asked.height.max(least.height))
}

/// The least size of a window, in the system's logical pixels, whose page is
/// at least `least` CSS pixels, when one CSS pixel is `thousandths` device
/// pixels and one logical pixel `scale_factor` of them — rounded up to whole
/// pixels; `least` itself for a scale that is no number above zero.
///
/// **A system may scale a page by more than its own scale factor**:
/// `WebKitGTK` scales it by the screen's resolution over 96 as well, so a
/// window 320 logical pixels wide on a screen of 100 dots an inch holds a
/// page 307 CSS pixels wide.
#[must_use]
pub fn fitted(least: LogicalSize<f64>, thousandths: u32, scale_factor: f64) -> LogicalSize<f64> {
    let ratio = f64::from(thousandths) / 1000.0 / scale_factor;
    if !(ratio.is_finite() && ratio > 0.0) {
        return least;
    }
    LogicalSize::new((least.width * ratio).ceil(), (least.height * ratio).ceil())
}

/// Whether `address` is one the webview may navigate to: under `origin`, the
/// custom protocol's root, which ends in `/`.
#[must_use]
pub fn within(origin: &str, address: &str) -> bool {
    origin.ends_with('/') && address.starts_with(origin)
}

/// A window and the webview in it.
pub struct Opened {
    /// The window.
    pub window: Window,
    /// The webview, which lives as long as the window shows it.
    webview: wry::WebView,
    /// The theme it was opened in.
    pub theme: Theme,
    /// The least the window is, in logical pixels: the catalog's least,
    /// fitted to the page's scale once the page has said it ([`fitted`]).
    least: LogicalSize<f64>,
}

impl Opened {
    /// The window's least size fitted to a page whose CSS pixel is
    /// `thousandths` device pixels, so its page is never smaller than the
    /// catalog's least; a window already smaller is made that size.
    pub fn fit(&mut self, thousandths: u32) {
        let scale_factor = self.window.scale_factor();
        let fitted = fitted(least(), thousandths, scale_factor);
        if fitted == self.least {
            return;
        }
        self.least = fitted;
        self.window.set_min_inner_size(Some(fitted));
        let inner = self.window.inner_size().to_logical::<f64>(scale_factor);
        if inner.width < fitted.width || inner.height < fitted.height {
            self.window.set_inner_size(at_least(inner, fitted));
        }
    }

    /// The window asked for `asked`, made its least where it falls short:
    /// the one way this process sizes a window.
    pub fn resize(&self, asked: LogicalSize<f64>) {
        self.window.set_inner_size(at_least(asked, self.least));
    }
}

impl core::fmt::Debug for Opened {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Opened")
            .field("window", &self.window.id())
            .field("webview", &self.webview.id())
            .field("theme", &self.theme)
            .field("least", &self.least)
            .finish()
    }
}

/// What a probe's observer says, from the first moment of the document: the
/// page's background at its first frame and once settled, each painted into
/// a canvas of one pixel and read back (`NOCTUA.md §9.6`, lesson 5), and the
/// page's size whenever it changes.
const OBSERVES: &str = "(() => {\
    const background = () => getComputedStyle(document.documentElement).backgroundColor;\
    const say = (word, colour) => {\
        const canvas = document.createElement('canvas');\
        canvas.width = 1;\
        canvas.height = 1;\
        const context = canvas.getContext('2d');\
        context.fillStyle = colour;\
        context.fillRect(0, 0, 1, 1);\
        window.ipc.postMessage(word + ' ' + Array.from(context.getImageData(0, 0, 1, 1).data).join(' '));\
    };\
    requestAnimationFrame(() => say('first-frame', background()));\
    window.addEventListener('load', () => requestAnimationFrame(() =>\
        requestAnimationFrame(() => say('settled', background()))));\
    window.addEventListener('resize', () =>\
        window.ipc.postMessage('viewport ' + window.innerWidth + ' ' + window.innerHeight));\
})();";

/// A window opened on `target`, hidden and undecorated, in the theme `mode`
/// asks for — the system's resolved as the window reads it — its webview
/// handed the document in `locale`; what the document says reaches the loop
/// through `proxy`, tagged `index`, and a probe's observer watches it when
/// `observed`.
///
/// # Errors
///
/// [`Unopened::Window`] for a window the system refused, and
/// [`Unopened::Webview`] for a webview it did not build.
pub fn open(
    target: &EventLoopWindowTarget<Arrived>,
    proxy: EventLoopProxy<Arrived>,
    locale: nc_catalog::Locale,
    mode: Mode,
    (index, observed): (usize, bool),
) -> Result<Opened, Unopened> {
    let window = WindowBuilder::new()
        .with_title(TITLE)
        .with_decorations(false)
        .with_visible(false)
        .with_min_inner_size(least())
        .build(target)
        .map_err(Unopened::Window)?;
    let theme = Theme::chosen(
        mode,
        noctua_design::model::Palette::DEFAULT,
        window.theme() == tao::window::Theme::Dark,
    );
    window.set_theme(Some(if theme.is_dark() {
        tao::window::Theme::Dark
    } else {
        tao::window::Theme::Light
    }));
    let document = crate::page::document(locale, theme);
    let origin = crate::assets::origin();
    let allowed = origin.clone();
    let mut builder = wry::WebViewBuilder::new()
        .with_custom_protocol(crate::assets::SCHEME.to_owned(), move |_, request| {
            crate::assets::response(crate::assets::answer(
                request.method().as_str(),
                request.uri().path(),
                &document,
            ))
        })
        .with_url(origin)
        .with_navigation_handler(move |address| within(&allowed, &address))
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_download_started_handler(|_, _| false)
        .with_ipc_handler(move |request: wry::http::Request<String>| {
            if let Some(said) = said(request.body()) {
                // A loop that has ended has no use for what was said.
                let _ = proxy.send_event(Arrived::Said {
                    window: index,
                    said,
                });
            }
        });
    if let Some([red, green, blue]) = theme.backdrop() {
        builder = builder.with_background_color((red, green, blue, u8::MAX));
    }
    if observed {
        builder = builder.with_initialization_script(OBSERVES);
    }
    let webview = nc_platform::webview::into(builder, &window).map_err(Unopened::Webview)?;
    Ok(Opened {
        window,
        webview,
        theme,
        least: least(),
    })
}

/// Whether a window is shown: when the state wants it shown and its document
/// has loaded, and at no other time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Showing {
    /// Whether the state wants it shown.
    wanted: bool,
    /// Whether its document said it loaded.
    loaded: bool,
}

impl Showing {
    /// The state wants the window shown, or hidden.
    pub const fn want(&mut self, shown: bool) {
        self.wanted = shown;
    }

    /// The window's document said it loaded.
    pub const fn loaded(&mut self) {
        self.loaded = true;
    }

    /// Whether the window is to be shown now.
    #[must_use]
    pub const fn shows(self) -> bool {
        self.wanted && self.loaded
    }
}

/// `window` shown or hidden as `showing` says, and focused when it is shown
/// by this.
fn apply(window: &Window, showing: Showing) {
    let shows = showing.shows();
    if window.is_visible() != shows {
        window.set_visible(shows);
        if shows {
            window.set_focus();
        }
    }
}

/// What [`probe`] saw of one window, opened and closed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Seen {
    /// The window was hidden once it was opened.
    pub hidden_when_opened: bool,
    /// The window draws no decoration of the system's.
    pub undecorated: bool,
    /// Whether the window, asked to show from the moment it was opened, was
    /// still hidden once its document said it loaded; nothing when it did
    /// not say so within the wait.
    pub hidden_when_loaded: Option<bool>,
    /// What the page's background was at its first frame, as red, green,
    /// blue and alpha; nothing when it was not said within the wait.
    pub first_frame: Option<[u8; 4]>,
    /// What it was once settled.
    pub settled: Option<[u8; 4]>,
    /// The page's size in CSS pixels once the window, settled, was asked to
    /// be half the least in each dimension.
    pub least: Option<(u32, u32)>,
}

/// What [`probe`] saw, in each mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probed {
    /// The window opened in the light.
    pub light: Seen,
    /// The window opened in the dark.
    pub dark: Seen,
    /// What the dark page is painted with, as the design system gives it.
    pub dark_backdrop: Option<[u8; 3]>,
}

/// The modes a probe opens a window in, one after the other.
const PROBED: [Mode; 2] = [Mode::Light, Mode::Dark];

/// One window a probe watches, and where its watching is.
struct Watched {
    /// The window and its webview.
    opened: Opened,
    /// Whether it is shown.
    showing: Showing,
    /// What was seen of it.
    seen: Seen,
    /// The page's size it last said, in CSS pixels.
    viewport: Option<(u32, u32)>,
    /// Whether the window was asked to be smaller than the least.
    shrunk: bool,
    /// The page's size it had last said when the window was asked.
    before: Option<(u32, u32)>,
    /// When the probe stops waiting for it.
    deadline: Instant,
}

impl Watched {
    /// A window opened in `index`-th of [`PROBED`]'s modes, asked to show at
    /// once, watched until `within` has passed.
    fn open(
        target: &EventLoopWindowTarget<Arrived>,
        proxy: EventLoopProxy<Arrived>,
        index: usize,
        within: Duration,
    ) -> Result<Self, Unopened> {
        let mode = PROBED.get(index).copied().unwrap_or(Mode::Light);
        let opened = open(target, proxy, nc_catalog::Locale::En, mode, (index, true))?;
        let mut showing = Showing::default();
        showing.want(true);
        apply(&opened.window, showing);
        let seen = Seen {
            hidden_when_opened: !opened.window.is_visible(),
            undecorated: !opened.window.is_decorated(),
            ..Seen::default()
        };
        Ok(Self {
            opened,
            showing,
            seen,
            viewport: None,
            shrunk: false,
            before: None,
            deadline: Instant::now() + within,
        })
    }

    /// Takes in what its document said; whether the window has been watched
    /// to the end.
    fn heard(&mut self, said: Said) -> bool {
        match said {
            Said::Loaded => {
                self.seen.hidden_when_loaded = Some(!self.opened.window.is_visible());
                self.showing.loaded();
                apply(&self.opened.window, self.showing);
            }
            Said::Scale(thousandths) => self.opened.fit(thousandths),
            Said::FirstFrame(colour) => {
                self.seen.first_frame.get_or_insert(colour);
            }
            Said::Settled(colour) => {
                self.seen.settled = Some(colour);
                let least = least();
                self.opened
                    .resize(LogicalSize::new(least.width / 2.0, least.height / 2.0));
                self.shrunk = true;
                self.before = self.viewport;
            }
            // The first size the page says after the asking that differs
            // from the one it had then is the answer to it.
            Said::Viewport(width, height) => {
                self.viewport = Some((width, height));
                if self.shrunk && self.before != Some((width, height)) {
                    self.seen.least = Some((width, height));
                    return true;
                }
            }
        }
        false
    }
}

/// Opens a window as the interface does in each of the light and the dark,
/// one after the other, each asked to show from the moment it is opened,
/// watched for `within` at most and closed: whether this system can host the
/// interface, asked with no daemon.
///
/// # Errors
///
/// [`Unopened`], as [`open`] refuses.
pub fn probe(within: Duration) -> Result<Probed, Unopened> {
    let mut event_loop = EventLoopBuilder::<Arrived>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let mut watched = Some(Watched::open(&event_loop, proxy.clone(), 0, within)?);
    let mut seen = [Seen::default(); PROBED.len()];
    let mut at = 0;
    let mut failed = None;
    event_loop.run_return(|event, target, flow| {
        let Some(current) = watched.as_mut() else {
            *flow = ControlFlow::Exit;
            return;
        };
        *flow = ControlFlow::WaitUntil(current.deadline);
        let done = match event {
            Event::UserEvent(Arrived::Said { window, said }) if window == at => current.heard(said),
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                Instant::now() >= current.deadline
            }
            _ => false,
        };
        if !done {
            return;
        }
        if let Some(slot) = seen.get_mut(at) {
            *slot = current.seen;
        }
        // **Closed**: the webview first, then the window it is drawn in.
        watched = None;
        at += 1;
        if at < PROBED.len() {
            match Watched::open(target, proxy.clone(), at, within) {
                Ok(next) => {
                    *flow = ControlFlow::WaitUntil(next.deadline);
                    watched = Some(next);
                }
                Err(unopened) => {
                    failed = Some(unopened);
                    *flow = ControlFlow::Exit;
                }
            }
        } else {
            *flow = ControlFlow::Exit;
        }
    });
    drop(watched);
    if let Some(unopened) = failed {
        return Err(unopened);
    }
    let [light, dark] = seen;
    Ok(Probed {
        light,
        dark,
        dark_backdrop: Theme::chosen(Mode::Dark, noctua_design::model::Palette::DEFAULT, true)
            .backdrop(),
    })
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
/// hidden in the theme the system prefers, shown and hidden as the daemon's
/// state says once its document has loaded, until the event loop ends —
/// which nothing in this build asks it to.
///
/// # Errors
///
/// [`Unopened`], as [`open`] refuses, and for a thread not started.
pub fn run(found: Found) -> Result<(), Unopened> {
    let mut event_loop = EventLoopBuilder::<Arrived>::with_user_event().build();
    let mut opened = open(
        &event_loop,
        event_loop.create_proxy(),
        nc_catalog::Locale::En,
        Mode::DEFAULT,
        (0, false),
    )?;
    following(found.clone(), event_loop.create_proxy()).map_err(Unopened::Thread)?;
    let asks = asking(found).map_err(Unopened::Thread)?;
    let mut mirror = Mirror::new();
    let mut showing = Showing::default();
    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Arrived::Said {
                said: Said::Loaded, ..
            }) => {
                showing.loaded();
                apply(&opened.window, showing);
            }
            Event::UserEvent(Arrived::Said {
                said: Said::Scale(thousandths),
                ..
            }) => opened.fit(thousandths),
            Event::UserEvent(Arrived::Heard(Heard::Reached)) => mirror.reconnected(),
            Event::UserEvent(Arrived::Heard(Heard::Changed(changed))) => {
                // A state this build does not read leaves the one held drawn.
                if mirror.watched(changed).is_ok()
                    && let Some(shown) = shown(&mirror)
                {
                    showing.want(shown);
                    apply(&opened.window, showing);
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
    use tao::dpi::LogicalSize;

    use super::{Ask, Said, Showing, at_least, fitted, least, said, shown, this_window, within};
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

    /// A window is shown only while the state wants it shown and once its
    /// document has loaded: a show asked for before waits for the document,
    /// and a hide is at once, whether or not it has loaded.
    #[test]
    fn a_window_is_shown_only_once_its_document_has_loaded() {
        let mut showing = Showing::default();
        assert!(!showing.shows(), "nothing asked");
        showing.want(true);
        assert!(!showing.shows(), "asked before the document loaded");
        showing.loaded();
        assert!(showing.shows(), "asked, and loaded");
        showing.want(false);
        assert!(!showing.shows(), "hidden");
        showing.want(true);
        assert!(showing.shows(), "asked again");
        let mut showing = Showing::default();
        showing.loaded();
        assert!(!showing.shows(), "loaded, and not asked");
        showing.want(false);
        assert!(!showing.shows(), "loaded, and asked to hide");
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

    /// The webview's channel is read for the closed set of messages, each
    /// written one way, and nothing else is one.
    #[test]
    fn only_the_closed_set_is_read_from_the_webview() {
        assert_eq!(said(crate::page::LOADED), Some(Said::Loaded));
        assert_eq!(
            said("first-frame 1 2 3 255"),
            Some(Said::FirstFrame([1, 2, 3, 255]))
        );
        assert_eq!(said("settled 0 0 0 0"), Some(Said::Settled([0, 0, 0, 0])));
        assert_eq!(said("viewport 320 240"), Some(Said::Viewport(320, 240)));
        assert_eq!(said("scale 1042"), Some(Said::Scale(1042)));
        assert_eq!(said("scale 0"), Some(Said::Scale(0)));
        for other in [
            "",
            "Loaded",
            "loaded ",
            " loaded",
            "ready",
            "loaded\n",
            "loaded loaded",
            "first-frame 1 2 3",
            "first-frame 1 2 3 4 5",
            "first-frame 1 2 3 256",
            "first-frame 1 2 3 -1",
            "first-frame 1 2 3 +4",
            "first-frame  1 2 3 4",
            "first-frame 1 2 3 4 ",
            "first-frame 1.0 2 3 4",
            "settled",
            "settled 1 2 3 x",
            "viewport 320",
            "viewport 320 240 1",
            "viewport 320 -240",
            "viewport 99999999999 240",
            "viewport 0x10 240",
            "Viewport 320 240",
            "scale",
            "scale 1.042",
            "scale 1042 1",
            "scale -1",
            "Scale 1000",
        ] {
            assert_eq!(said(other), None, "{other:?}");
        }
    }

    /// The least is the catalog's, and a size asked for is made the least
    /// in each dimension it falls short in, and kept in each it does not.
    #[test]
    fn no_size_asked_for_is_less_than_the_least() {
        let least = least();
        assert_eq!(
            (least.width, least.height),
            (
                f64::from(nc_catalog::KERNEL.shell_window_min_width),
                f64::from(nc_catalog::KERNEL.shell_window_min_height)
            )
        );
        for (asked, made) in [
            ((1.0, 1.0), (least.width, least.height)),
            ((least.width / 2.0, 1000.0), (least.width, 1000.0)),
            ((1000.0, least.height - 1.0), (1000.0, least.height)),
            ((least.width, least.height), (least.width, least.height)),
            ((1000.0, 800.0), (1000.0, 800.0)),
        ] {
            let sized = at_least(LogicalSize::new(asked.0, asked.1), least);
            assert_eq!((sized.width, sized.height), made, "{asked:?}");
        }
    }

    /// The least in logical pixels holds a page of the least in CSS pixels:
    /// scaled by how many logical pixels one CSS pixel is, rounded up, and
    /// the catalog's own for a scale that is no number above zero.
    #[test]
    fn the_least_is_fitted_to_the_pages_scale() {
        let least = LogicalSize::new(320.0, 240.0);
        for (thousandths, scale_factor, made) in [
            (1000, 1.0, (320.0, 240.0)),
            (2000, 2.0, (320.0, 240.0)),
            (1500, 1.5, (320.0, 240.0)),
            (1042, 1.0, (334.0, 251.0)),
            (2084, 2.0, (334.0, 251.0)),
            (1250, 1.0, (400.0, 300.0)),
            (500, 1.0, (160.0, 120.0)),
            (0, 1.0, (320.0, 240.0)),
            (1000, 0.0, (320.0, 240.0)),
            (1000, -1.0, (320.0, 240.0)),
            (1000, f64::NAN, (320.0, 240.0)),
        ] {
            let sized = fitted(least, thousandths, scale_factor);
            assert_eq!(
                (sized.width, sized.height),
                made,
                "{thousandths} {scale_factor}"
            );
        }
    }

    /// A navigation is allowed under the protocol's root alone: nowhere
    /// else, and not to a host whose name merely begins the same.
    #[test]
    fn a_navigation_is_allowed_under_the_root_alone() {
        let origin = "noctua://localhost/";
        assert!(within(origin, "noctua://localhost/"));
        assert!(within(origin, "noctua://localhost/dist/scope.css"));
        for address in [
            "noctua://localhost",
            "noctua://localhost.example/",
            "noctua://other/",
            "https://localhost/",
            "file:///etc/passwd",
            "about:blank",
            "",
        ] {
            assert!(!within(origin, address), "{address}");
        }
        assert!(!within("noctua://localhost", "noctua://localhost.example/"));
        assert_eq!(
            crate::assets::origin(),
            nc_platform::webview::origin(crate::assets::SCHEME)
        );
        assert!(crate::assets::origin().ends_with('/'));
    }
}
